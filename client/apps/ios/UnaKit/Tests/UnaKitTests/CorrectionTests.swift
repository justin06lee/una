import XCTest
@testable import UnaKit

// Fixtures: una.example / the tenet dictation_detail capture. Never a real server.
private func fixture(_ name: String) throws -> Data {
    try Data(contentsOf: Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures")!)
}

final class WordDiffTests: XCTestCase {
    func testIdentical() {
        let p = WordDiff.parts(from: "a b c", to: "a  b c")
        XCTAssertEqual(p, [DiffPart(kind: .same, text: "a b c")]); XCTAssertFalse(WordDiff.changed(p))
    }
    func testRemovalAndAddition() {
        let p = WordDiff.parts(from: "this is a test of Oona", to: "this is Una")
        XCTAssertEqual(p, [DiffPart(kind: .same, text: "this is"), DiffPart(kind: .removed, text: "a test of Oona"),
                           DiffPart(kind: .added, text: "Una")])
    }
    func testEmptySides() {
        XCTAssertEqual(WordDiff.parts(from: "", to: "hi there"), [DiffPart(kind: .added, text: "hi there")])
        XCTAssertEqual(WordDiff.parts(from: "bye", to: ""), [DiffPart(kind: .removed, text: "bye")])
        XCTAssertTrue(WordDiff.parts(from: "", to: "").isEmpty)
    }
    func testReplacementInMiddle() {
        let p = WordDiff.parts(from: "see you at noon", to: "see you at one")
        XCTAssertEqual(p.map(\.kind), [.same, .removed, .added])
    }
}

final class FixDraftTests: XCTestCase {
    func testUntouchedIsAcceptedWithNoTexts() {
        let d = FixDraft(rawText: "hello  world", insertedText: "Hello world.")
        let r = d.request()
        XCTAssertEqual(r.action, .accepted); XCTAssertNil(r.correctedText); XCTAssertNil(r.polishedText)
        XCTAssertTrue(r.transcriptShown); XCTAssertEqual(r.source, .popup)
    }
    func testEditedTranscriptIsEditedWithCorrectedText() {
        var d = FixDraft(rawText: "this is a test of Oona", insertedText: "This is Oona.")
        d.transcript = "  this is a test of Una "
        let r = d.request()
        XCTAssertEqual(r.action, .edited); XCTAssertEqual(r.correctedText, "this is a test of Una"); XCTAssertNil(r.polishedText)
    }
    func testStyleFixOnlyIsAcceptedPlusPolished() {
        var d = FixDraft(rawText: "um hi", insertedText: "Hi.")
        d.final = "Hi there."
        let r = d.request()
        XCTAssertEqual(r.action, .accepted); XCTAssertEqual(r.polishedText, "Hi there.")
    }
    func testCannotSaveBlank() {
        var d = FixDraft(rawText: "hi", insertedText: "Hi.")
        d.transcript = "   "; XCTAssertFalse(d.canSave)
        d.transcript = "hi"; d.final = ""; XCTAssertFalse(d.canSave)
        d.final = "Hi!"; XCTAssertTrue(d.canSave)
    }
    func testRequestEncodesWireFields() throws {
        var d = FixDraft(rawText: "a", insertedText: "A")
        d.transcript = "b"
        let j = try JSONSerialization.jsonObject(with: UnaCoding.encoder.encode(d.request())) as! [String: Any]
        XCTAssertEqual(j["action"] as? String, "edited"); XCTAssertEqual(j["corrected_text"] as? String, "b")
        XCTAssertEqual(j["transcript_shown"] as? Bool, true); XCTAssertEqual(j["source"] as? String, "popup")
    }
}

final class FiledStateTests: XCTestCase {
    func testUnreviewedDetailIsNone() throws {
        let d = try UnaCoding.decoder.decode(DictationDetail.self, from: fixture("dictation_detail"))
        XCTAssertEqual(d.filedState, .none)
    }
    func testReviewedAndPolished() throws {
        var s = String(decoding: try fixture("dictation_detail"), as: UTF8.self)
        s = s.replacingOccurrences(of: "\"reviewed\":false,\"review_action\":null", with: "\"reviewed\":true,\"review_action\":\"edited\"")
            .replacingOccurrences(of: "\"correction_source\":null", with: "\"correction_source\":\"popup\"")
        let d = try UnaCoding.decoder.decode(DictationDetail.self, from: Data(s.utf8))
        XCTAssertEqual(d.filedState, .filed(action: "edited", source: "popup"))
        let p = s.replacingOccurrences(of: "\"reviewed\":true,\"review_action\":\"edited\"", with: "\"reviewed\":false,\"review_action\":null")
            .replacingOccurrences(of: "\"polished_text\":null", with: "\"polished_text\":\"Hi.\"")
        XCTAssertEqual(try UnaCoding.decoder.decode(DictationDetail.self, from: Data(p.utf8)).filedState, .polishedOnly)
    }
}

final class OutboxTests: XCTestCase {
    private func outbox() -> CorrectionOutbox {
        CorrectionOutbox(file: FileManager.default.temporaryDirectory.appendingPathComponent("ob-\(UUID())/c.json"))
    }
    private func client() -> UnaClient {
        let c = URLSessionConfiguration.ephemeral; c.protocolClasses = [StubProtocol.self]; StubProtocol.calls = 0
        return UnaClient(baseURL: URL(string: "http://una.example:8100")!, session: URLSession(configuration: c))
    }
    func testOneEntryPerDictationNewestWins() {
        let o = outbox()
        o.add(id: "a", CorrectionRequest(action: .accepted)); o.add(id: "b", CorrectionRequest(action: .skipped))
        o.add(id: "a", CorrectionRequest(action: .edited, correctedText: "x"))
        XCTAssertEqual(o.list().map(\.dictationId), ["b", "a"]); XCTAssertEqual(o.entry(for: "a")?.request.correctedText, "x")
        o.remove(id: "a"); o.remove(id: "b"); XCTAssertTrue(o.list().isEmpty)
    }
    func testFlushKeepsOnOutageAndStops() async throws {
        let o = outbox(); o.add(id: "a", CorrectionRequest(action: .accepted)); o.add(id: "b", CorrectionRequest(action: .accepted))
        StubProtocol.handler = { _, _ in throw URLError(.notConnectedToInternet) }
        let r = await o.flush(using: client())
        XCTAssertEqual(r.kept, 2); XCTAssertTrue(r.sent.isEmpty); XCTAssertEqual(o.list().count, 2)
    }
    func testFlushSendsAndDropsBadRequests() async throws {
        let o = outbox(); o.add(id: "ok", CorrectionRequest(action: .accepted)); o.add(id: "gone", CorrectionRequest(action: .accepted))
        let body = try fixture("correction_response")
        StubProtocol.handler = { req, _ in
            req.url!.path.contains("/gone/") ? (404, Data(#"{"error":{"code":"NOT_FOUND","message":"no"}}"#.utf8)) : (200, body)
        }
        let r = await o.flush(using: client())
        XCTAssertEqual(r.sent, ["ok"]); XCTAssertEqual(r.dropped, ["gone"]); XCTAssertTrue(o.list().isEmpty)
    }
    func test503KeepsQueued() async {
        let o = outbox(); o.add(id: "a", CorrectionRequest(action: .accepted))
        StubProtocol.handler = { _, _ in (503, Data()) }
        let r = await o.flush(using: client()); XCTAssertEqual(r.kept, 1); XCTAssertEqual(o.list().count, 1)
    }
}
