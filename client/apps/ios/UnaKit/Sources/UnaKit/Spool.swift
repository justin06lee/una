import Foundation

/// Failed utterances kept for Retry: newest `keep` WAVs with their utterance id, like spool.rs.
/// File name is `<utterance-id>.wav`; creation time orders them.
public struct Spool: Sendable {
    public struct Item: Sendable, Equatable, Identifiable {
        public let utteranceId: String
        public let url: URL
        public let createdAt: Date
        public let bytes: Int
        public var id: String { utteranceId }
        public var durationMs: Int { max(0, bytes - WAV.headerSize) / 2 * 1000 / WAV.sampleRate }
    }

    public static let keep = 5
    public let directory: URL
    public init(directory: URL) { self.directory = directory }

    private var fm: FileManager { .default }

    /// Stores (or re-stores, without stacking a duplicate) and prunes to the newest `keep`.
    @discardableResult
    public func add(wav: Data, utteranceId: String) throws -> Item {
        try fm.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("\(utteranceId).wav")
        if !fm.fileExists(atPath: url.path) { try wav.write(to: url, options: .atomic) }
        prune()
        return item(at: url)!
    }

    public func list() -> [Item] {
        let urls = (try? fm.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        return urls.filter { $0.pathExtension == "wav" }.compactMap(item(at:)).sorted { $0.createdAt > $1.createdAt }
    }

    public func data(for item: Item) throws -> Data { try Data(contentsOf: item.url) }
    public func remove(_ item: Item) { try? fm.removeItem(at: item.url) }

    private func item(at url: URL) -> Item? {
        guard let a = try? fm.attributesOfItem(atPath: url.path) else { return nil }
        return Item(utteranceId: url.deletingPathExtension().lastPathComponent, url: url,
                    createdAt: (a[.creationDate] as? Date) ?? .distantPast, bytes: (a[.size] as? Int) ?? 0)
    }

    private func prune() { for old in list().dropFirst(Self.keep) { remove(old) } }

    /// Upload one spooled item; removes it on success. Keeps it (and its id) on failure.
    public func retry(_ item: Item, using client: UnaClient) async throws -> DictationResponse {
        let r = try await client.createDictation(wav: try data(for: item), utteranceId: item.utteranceId)
        remove(item)
        return r
    }
}
