import Foundation

// Step 3: fixing a dictation from the phone. The model and the diff are pure so they can be tested.

/// One piece of a word diff.
public struct DiffPart: Equatable, Sendable {
    public enum Kind: Sendable { case same, removed, added }
    public let kind: Kind
    public let text: String
}

public enum WordDiff {
    /// Word-level diff by longest common subsequence. Words are whitespace-separated;
    /// adjacent parts of one kind are merged into a single part.
    public static func parts(from old: String, to new: String) -> [DiffPart] {
        let a = old.split(whereSeparator: \.isWhitespace).map(String.init)
        let b = new.split(whereSeparator: \.isWhitespace).map(String.init)
        var t = [[Int]](repeating: [Int](repeating: 0, count: b.count + 1), count: a.count + 1)
        for i in stride(from: a.count - 1, through: 0, by: -1) {
            for j in stride(from: b.count - 1, through: 0, by: -1) {
                t[i][j] = a[i] == b[j] ? t[i + 1][j + 1] + 1 : max(t[i + 1][j], t[i][j + 1])
            }
        }
        var out: [DiffPart] = []
        func push(_ k: DiffPart.Kind, _ w: String) {
            if let l = out.last, l.kind == k { out[out.count - 1] = DiffPart(kind: k, text: l.text + " " + w) }
            else { out.append(DiffPart(kind: k, text: w)) }
        }
        var i = 0, j = 0
        while i < a.count || j < b.count {
            if i < a.count, j < b.count, a[i] == b[j] { push(.same, a[i]); i += 1; j += 1 }
            else if i < a.count, j == b.count || t[i + 1][j] >= t[i][j + 1] { push(.removed, a[i]); i += 1 }
            else { push(.added, b[j]); j += 1 }
        }
        return out
    }

    public static func changed(_ parts: [DiffPart]) -> Bool { parts.contains { $0.kind != .same } }
}

/// What the user sees about a dictation's correction on the server.
public enum FiledState: Equatable, Sendable {
    case none
    case filed(action: String, source: String?)
    case polishedOnly            // a text fix is filed but the transcript is still unjudged
    case waiting                 // saved on the phone, not yet sent
}

extension DictationDetail {
    public var filedState: FiledState {
        if reviewed, let a = reviewAction { return .filed(action: a, source: correctionSource) }
        if polishedText != nil { return .polishedOnly }
        return .none
    }
}

/// The fix sheet's two fields. "What you said" starts as the raw transcript, "How it should read"
/// as the text that was inserted. Saving is a verdict on what was said (transcript_shown) plus a
/// style target, like the desktop fix window.
public struct FixDraft: Equatable, Sendable {
    public let rawText: String
    public let insertedText: String
    public var transcript: String
    public var final: String

    public init(rawText: String, insertedText: String) {
        self.rawText = rawText; self.insertedText = insertedText
        transcript = rawText; final = insertedText
    }

    static func norm(_ s: String) -> String { s.split(whereSeparator: \.isWhitespace).joined(separator: " ") }
    public var transcriptChanged: Bool { Self.norm(transcript) != Self.norm(rawText) }
    public var finalChanged: Bool { Self.norm(final) != Self.norm(insertedText) }
    public var transcriptDiff: [DiffPart] { WordDiff.parts(from: rawText, to: transcript) }
    public var finalDiff: [DiffPart] { WordDiff.parts(from: insertedText, to: final) }
    /// An edited transcript can't be blank: the server rejects `edited` without text.
    public var canSave: Bool { !(transcriptChanged && Self.norm(transcript).isEmpty) && !Self.norm(final).isEmpty }

    public func request() -> CorrectionRequest {
        CorrectionRequest(action: transcriptChanged ? .edited : .accepted,
                          correctedText: transcriptChanged ? transcript.trimmingCharacters(in: .whitespacesAndNewlines) : nil,
                          polishedText: finalChanged ? final.trimmingCharacters(in: .whitespacesAndNewlines) : nil,
                          source: .popup, transcriptShown: true)
    }
}

/// Corrections that could not be sent yet. One per dictation (a newer fix replaces an older
/// one); the PUT is idempotent, so a retry can never double-file. Kept as JSON in the
/// app's container, next to the audio spool.
public struct CorrectionOutbox: Sendable {
    public struct Entry: Codable, Equatable, Sendable, Identifiable {
        public let dictationId: String
        public let request: CorrectionRequest
        public let createdAt: Date
        public var id: String { dictationId }
    }
    public struct Flushed: Equatable, Sendable { public var sent: [String] = []; public var dropped: [String] = []; public var kept: Int = 0 }

    public let file: URL
    public init(file: URL) { self.file = file }

    public func list() -> [Entry] {
        guard let d = try? Data(contentsOf: file), let e = try? Self.decoder.decode([Entry].self, from: d) else { return [] }
        return e
    }
    public func entry(for id: String) -> Entry? { list().first { $0.dictationId == id } }

    public func add(id: String, _ req: CorrectionRequest, at date: Date = Date()) {
        var all = list().filter { $0.dictationId != id }
        all.append(Entry(dictationId: id, request: req, createdAt: date))
        save(all)
    }
    public func remove(id: String) { save(list().filter { $0.dictationId != id }) }

    /// Sends each queued correction. A retryable failure keeps it (and stops, the server is
    /// likely away); a 4xx drops it, as it would never succeed.
    public func flush(using client: UnaClient) async -> Flushed {
        var r = Flushed()
        let all = list()
        for (n, e) in all.enumerated() {
            do { _ = try await client.correct(id: e.dictationId, e.request); remove(id: e.dictationId); r.sent.append(e.dictationId) }
            catch let err as UnaError where err.isRetryable { r.kept = all.count - n; return r }
            catch { remove(id: e.dictationId); r.dropped.append(e.dictationId) }
        }
        return r
    }

    private func save(_ e: [Entry]) {
        try? FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        if e.isEmpty { try? FileManager.default.removeItem(at: file) }
        else if let d = try? Self.encoder.encode(e) { try? d.write(to: file, options: .atomic) }
    }
    private static let encoder: JSONEncoder = { let e = JSONEncoder(); e.dateEncodingStrategy = .iso8601; e.keyEncodingStrategy = .convertToSnakeCase; return e }()
    private static let decoder: JSONDecoder = { let d = JSONDecoder(); d.dateDecodingStrategy = .iso8601; d.keyDecodingStrategy = .convertFromSnakeCase; return d }()
}
