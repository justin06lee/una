import XCTest
@testable import UnaKit

private func fixture(_ name: String) throws -> Data {
    try Data(contentsOf: Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures")!)
}

final class ModelTests: XCTestCase {
    func testDictationResponse() throws {
        let r = try UnaCoding.decoder.decode(DictationResponse.self, from: fixture("dictation_response"))
        XCTAssertEqual(r.text, "Hello world, this is Oona on the iPhone.")
        XCTAssertTrue(r.cleanupApplied)
        XCTAssertNil(r.cleanupError)
        XCTAssertEqual(r.timings.cleanupMs, 2712)
        XCTAssertEqual(r.asrModel, "base:large-v3-turbo")
    }
    func testListDetailQueueHealthCorrection() throws {
        let l = try UnaCoding.decoder.decode(DictationList.self, from: fixture("dictation_list"))
        XCTAssertEqual(l.items.count, 2); XCTAssertNotNil(l.nextCursor); XCTAssertEqual(l.items[1].reviewAction, "accepted")
        let d = try UnaCoding.decoder.decode(DictationDetail.self, from: fixture("dictation_detail"))
        XCTAssertEqual(d.rawText, "Hello world, this is a test of Oona on the iPhone."); XCTAssertEqual(d.language, "en")
        let q = try UnaCoding.decoder.decode(ReviewQueue.self, from: fixture("review_queue"))
        XCTAssertGreaterThanOrEqual(q.pending, 1)
        let h = try UnaCoding.decoder.decode(Health.self, from: fixture("health"))
        XCTAssertEqual(h.status, "ok"); XCTAssertFalse(h.trainingActive); XCTAssertNotNil(h.gpu?.name)
        let c = try UnaCoding.decoder.decode(CorrectionResponse.self, from: fixture("correction_response"))
        XCTAssertEqual(c.action, "skipped"); XCTAssertFalse(c.trainingEligible)
        let e = try UnaCoding.decoder.decode(ErrorEnvelope.self, from: fixture("error_audio_invalid"))
        XCTAssertEqual(e.error.code, "AUDIO_INVALID")
    }
    func testCorrectionRequestEncodesSnakeCase() throws {
        let req = CorrectionRequest(action: .edited, correctedText: "a", polishedText: "b", source: .review, transcriptShown: true)
        let j = try JSONSerialization.jsonObject(with: UnaCoding.encoder.encode(req)) as! [String: Any]
        XCTAssertEqual(j["corrected_text"] as? String, "a"); XCTAssertEqual(j["polished_text"] as? String, "b")
        XCTAssertEqual(j["transcript_shown"] as? Bool, true); XCTAssertEqual(j["action"] as? String, "edited")
    }
}

final class WAVTests: XCTestCase {
    func testHeader() {
        let d = WAV.encode([1, -2, 3])
        XCTAssertEqual(d.count, 44 + 6)
        XCTAssertEqual(String(decoding: d[0..<4], as: UTF8.self), "RIFF")
        XCTAssertEqual(String(decoding: d[8..<16], as: UTF8.self), "WAVEfmt ")
        XCTAssertEqual(Array(d[24..<28]), [0x80, 0x3E, 0, 0])   // 16000 Hz
        XCTAssertEqual(Array(d[34..<36]), [16, 0])              // 16-bit
        XCTAssertEqual(Array(d[40..<44]), [6, 0, 0, 0])         // data size
        XCTAssertEqual(Array(d[4..<8]), [42, 0, 0, 0])          // 36 + data
    }
    func testRoundTrip() {
        let s: [Int16] = [0, 100, -100, .max, .min]
        let back = WAV.decode(WAV.encode(s))!
        XCTAssertEqual(back.samples, s); XCTAssertEqual(back.sampleRate, 16000)
    }
    func testResample48kTo16k() {
        let n = 48_000
        let sine = (0..<n).map { Float(sin(2 * Double.pi * 440 * Double($0) / 48_000)) * 0.5 }
        let out = WAV.resampleTo16k(sine, inputRate: 48_000)
        XCTAssertEqual(out.count, 16_000)
        XCTAssertEqual(WAV.durationMs(sampleCount: out.count), 1000)
        XCTAssertGreaterThan(Int(out.map { abs(Int($0)) }.max()!), 12_000)
    }
    func testResampleEmpty() { XCTAssertTrue(WAV.resampleTo16k([], inputRate: 48_000).isEmpty) }
}

private func ok(_ e: ServerEntry) -> ProbeResult {
    ProbeResult(entry: e, health: Health(status: "ok", asrModel: nil, asrModelLoaded: true, ollama: "ok", trainingActive: false, gpu: nil), error: nil, latencyMs: 1)
}

final class RaceTests: XCTestCase {
    private func entry(_ n: String) -> ServerEntry { ServerEntry(name: n, url: "http://\(n)") }

    func testFirstSuccessWinsNotFirstInList() async {
        let slow = entry("slow"), fast = entry("fast"), dead = entry("dead")
        let race = EndpointRace(probe: { e in
            if e.name == "dead" { return ProbeResult(entry: e, health: nil, error: "x", latencyMs: 0) }
            try? await Task.sleep(for: .milliseconds(e.name == "slow" ? 400 : 20))
            return ok(e)
        })
        let w = await race.resolve([dead, slow, fast])
        XCTAssertEqual(w, fast)
    }
    func testNoneAnswer() async {
        let a = entry("a")
        let race = EndpointRace(probe: { e in ProbeResult(entry: e, health: nil, error: "x", latencyMs: 0) })
        let w = await race.resolve([a]); XCTAssertNil(w)
    }
    func testCacheTTLAndInvalidate() async {
        let a = entry("a")
        let counter = Counter()
        let clock = Clock()
        let race = EndpointRace(probe: { e in await counter.bump(); return ok(e) }, now: { clock.now })
        _ = await race.resolve([a]); _ = await race.resolve([a])
        var n = await counter.n; XCTAssertEqual(n, 1)               // second came from the cache
        clock.now.addTimeInterval(EndpointRace.cacheTTL + 1)
        _ = await race.resolve([a]); n = await counter.n; XCTAssertEqual(n, 2)   // expired
        await race.invalidate()
        _ = await race.resolve([a]); n = await counter.n; XCTAssertEqual(n, 3)   // dropped
    }
    func testTestAllKeepsOrder() async {
        let a = entry("a"), b = entry("b")
        let race = EndpointRace(probe: { e in
            try? await Task.sleep(for: .milliseconds(e.name == "a" ? 80 : 5)); return ok(e)
        })
        let r = await race.testAll([a, b]); XCTAssertEqual(r.map(\.entry), [a, b])
    }
    func testServerEntryURL() {
        XCTAssertEqual(ServerEntry(name: "x", url: "192.168.1.10:8100/").baseURL?.absoluteString, "http://192.168.1.10:8100")
    }
}
private actor Counter { var n = 0; func bump() { n += 1 } }
private final class Clock: @unchecked Sendable { var now = Date() }

final class SpoolTests: XCTestCase {
    private func tmp() -> URL { FileManager.default.temporaryDirectory.appendingPathComponent("spool-\(UUID())") }

    func testKeepsNewestFiveAndNoDuplicates() throws {
        let s = Spool(directory: tmp())
        var ids = [String]()
        for i in 0..<7 {
            let id = "id\(i)"; ids.append(id)
            try s.add(wav: WAV.encode([Int16(i)]), utteranceId: id)
            Thread.sleep(forTimeInterval: 0.02)
        }
        XCTAssertEqual(s.list().count, 5)
        XCTAssertEqual(s.list().first?.utteranceId, "id6")
        XCTAssertFalse(s.list().contains { $0.utteranceId == "id0" })
        try s.add(wav: WAV.encode([9]), utteranceId: "id6")
        XCTAssertEqual(s.list().count, 5)
    }
}

final class StubProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var handler: ((URLRequest, Int) throws -> (Int, Data))?
    nonisolated(unsafe) static var calls = 0
    nonisolated(unsafe) static var lastBody = Data()
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.calls += 1
        if let s = request.httpBodyStream {
            s.open(); var d = Data(); var buf = [UInt8](repeating: 0, count: 65536)
            while s.hasBytesAvailable { let n = s.read(&buf, maxLength: buf.count); if n <= 0 { break }; d.append(buf, count: n) }
            Self.lastBody = d
        } else { Self.lastBody = request.httpBody ?? Data() }
        do {
            let (code, data) = try Self.handler!(request, Self.calls)
            let resp = HTTPURLResponse(url: request.url!, statusCode: code, httpVersion: nil, headerFields: nil)!
            client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch { client?.urlProtocol(self, didFailWithError: error) }
    }
    override func stopLoading() {}
}

final class ClientTests: XCTestCase {
    private func client() -> UnaClient {
        let c = URLSessionConfiguration.ephemeral; c.protocolClasses = [StubProtocol.self]
        StubProtocol.calls = 0
        return UnaClient(baseURL: URL(string: "http://una.example:8100")!, session: URLSession(configuration: c))
    }

    func testUploadBuildsMultipartAndDecodes() async throws {
        let body = try fixture("dictation_response")
        StubProtocol.handler = { _, _ in (200, body) }
        let r = try await client().createDictation(wav: WAV.encode([1, 2, 3]), utteranceId: "u-1")
        XCTAssertEqual(r.id, "01M5K3DMA54KANRKMQBZPAFD8C")
        let sent = String(decoding: StubProtocol.lastBody, as: UTF8.self)
        for needle in ["name=\"audio\"; filename=\"audio.wav\"", "name=\"utterance_id\"\r\n\r\nu-1", "name=\"clean\"\r\n\r\ntrue", "una-ios/", "RIFF"] {
            XCTAssertTrue(sent.contains(needle), needle)
        }
        XCTAssertTrue(sent.hasSuffix("--\r\n"))
    }
    func testRetriesOnceOnConnectErrorWithSameId() async throws {
        let body = try fixture("dictation_response")
        StubProtocol.handler = { _, n in if n == 1 { throw URLError(.networkConnectionLost) }; return (200, body) }
        _ = try await client().createDictation(wav: WAV.encode([1]), utteranceId: "u-2")
        XCTAssertEqual(StubProtocol.calls, 2)
        XCTAssertTrue(String(decoding: StubProtocol.lastBody, as: UTF8.self).contains("u-2"))
    }
    func testNoRetryOnHTTPStatus() async throws {
        let body = try fixture("error_audio_invalid")
        StubProtocol.handler = { _, _ in (422, body) }
        do { _ = try await client().createDictation(wav: Data(), utteranceId: "u-3"); XCTFail() }
        catch let UnaError.server(status, code, _) { XCTAssertEqual(status, 422); XCTAssertEqual(code, "AUDIO_INVALID") }
        XCTAssertEqual(StubProtocol.calls, 1)
    }
    func test503IsTrainingInProgress() async {
        StubProtocol.handler = { _, _ in (503, Data(#"{"error":{"code":"TRAINING_IN_PROGRESS","message":"busy"}}"#.utf8)) }
        do { _ = try await client().createDictation(wav: Data(), utteranceId: "u-4"); XCTFail() }
        catch { XCTAssertEqual(error as? UnaError, .trainingInProgress("busy")); XCTAssertTrue((error as! UnaError).isRetryable) }
    }
    func testCorrectionPUT() async throws {
        let body = try fixture("correction_response")
        StubProtocol.handler = { req, _ in
            XCTAssertEqual(req.httpMethod, "PUT"); XCTAssertEqual(req.url?.path, "/v1/dictations/abc/correction")
            return (200, body)
        }
        let r = try await client().correct(id: "abc", CorrectionRequest(action: .skipped))
        XCTAssertEqual(r.action, "skipped")
    }
    func testSpoolRetryRemovesOnSuccessKeepsOnFailure() async throws {
        let spool = Spool(directory: FileManager.default.temporaryDirectory.appendingPathComponent("sp-\(UUID())"))
        let item = try spool.add(wav: WAV.encode([1]), utteranceId: "u-5")
        let body = try fixture("dictation_response")
        StubProtocol.handler = { _, _ in (503, Data()) }
        do { _ = try await spool.retry(item, using: client()); XCTFail() } catch {}
        XCTAssertEqual(spool.list().count, 1)
        StubProtocol.handler = { _, _ in (200, body) }
        _ = try await spool.retry(item, using: client())
        XCTAssertEqual(spool.list().count, 0)
    }
}
