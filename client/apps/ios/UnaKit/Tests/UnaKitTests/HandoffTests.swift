import XCTest
@testable import UnaKit

// Fixtures: the group id below is a test placeholder, not a real identifier.
private let group = "group.una.example"

final class FakeNotifier: Notifying, @unchecked Sendable {
    private let lock = NSLock()
    private(set) var posted: [String] = []
    private var handlers: [(String, @Sendable () -> Void)] = []
    func post(_ name: String) {
        let hs = lock.withLock { posted.append(name); return handlers.filter { $0.0 == name }.map(\.1) }
        hs.forEach { $0() }
    }
    func observe(_ name: String, _ handler: @escaping @Sendable () -> Void) -> AnyObject {
        lock.withLock { handlers.append((name, handler)) }; return NSObject()
    }
}

final class FakeClock: DictationClock, @unchecked Sendable {
    private let lock = NSLock()
    private var t: TimeInterval = 1_000
    private var sleepers: [(UUID, TimeInterval, CheckedContinuation<Void, Error>)] = []
    var now: TimeInterval { lock.withLock { t } }
    func sleep(_ s: TimeInterval) async throws {
        let id = UUID()
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
                lock.withLock { sleepers.append((id, t + s, c)) }
            }
        } onCancel: {
            let c = lock.withLock { () -> CheckedContinuation<Void, Error>? in
                guard let i = sleepers.firstIndex(where: { $0.0 == id }) else { return nil }
                return sleepers.remove(at: i).2
            }
            c?.resume(throwing: CancellationError())
        }
    }
    /// Moves time forward in small steps so sleepers wake in order and re-sleep.
    @MainActor func advance(_ s: TimeInterval) async {
        var left = s
        while left > 0 {
            let step = min(left, 1); left -= step
            let due = lock.withLock { () -> [CheckedContinuation<Void, Error>] in
                t += step
                let d = sleepers.filter { $0.1 <= t }; sleepers.removeAll { $0.1 <= t }; return d.map(\.2)
            }
            due.forEach { $0.resume() }
            for _ in 0..<20 { await Task.yield() }
        }
    }
}

@MainActor final class FakeCapture: AudioCapture {
    var armed = false, recording = false, failBegin = false
    var samples = [Int16](repeating: 1000, count: 16_000)
    var discards = 0
    func arm() async throws { armed = true }
    func disarm() { armed = false }
    func begin() async throws {
        if failBegin { throw DictationFailure("Microphone access is off.") }
        recording = true
    }
    func finish() -> [Int16] { recording = false; return samples }
    func discard() { if recording { discards += 1 }; recording = false }
}

@MainActor final class FakeUploader: Transcribing {
    var result: Result<String, DictationFailure> = .success("hello world")
    var gate: CheckedContinuation<Void, Never>?
    var hold = false
    var calls: [(Int, String)] = []
    func transcribe(samples: [Int16], utteranceId: String) async throws -> String {
        calls.append((samples.count, utteranceId))
        if hold { await withCheckedContinuation { gate = $0 } }
        try Task.checkCancellation()
        return try result.get()
    }
    func release() { gate?.resume(); gate = nil }
}

@MainActor final class HandoffTests: XCTestCase {
    var dir: URL!, store: SharedStore!, note: FakeNotifier!, clock: FakeClock!
    var cap: FakeCapture!, up: FakeUploader!, engine: DictationEngine!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("handoff-\(UUID())")
        store = SharedStore(directory: dir); note = FakeNotifier(); clock = FakeClock()
        cap = FakeCapture(); up = FakeUploader()
        engine = DictationEngine(capture: cap, uploader: up, clock: clock, store: store, notifier: note, group: group)
        engine.startListening()
    }
    override func tearDown() async throws { engine.disarm(); try? FileManager.default.removeItem(at: dir) }

    private func send(_ a: Command.Action, _ id: String = "r1") async {
        try! store.write(Command(requestId: id, action: a, at: clock.now + Double.random(in: 0..<1)))
        note.post(Handoff.notificationName(group: group, .command))
        for _ in 0..<20 { await Task.yield() }
    }
    private var phase: Status.Phase? { store.readStatus()?.phase }

    // MARK: protocol

    func testNamesAreNamespacedByGroup() {
        XCTAssertEqual(Handoff.notificationName(group: group, .command), "group.una.example.handoff.command")
        XCTAssertNotEqual(Handoff.notificationName(group: group, .command), Handoff.notificationName(group: group, .status))
    }
    func testStoreRoundTrip() throws {
        let c = Command(requestId: "a", action: .start, at: 5)
        try store.write(c); XCTAssertEqual(store.readCommand(), c)
        let s = Status(phase: .result, requestId: "a", text: "hi", at: 6)
        try store.write(s); XCTAssertEqual(store.readStatus(), s)
        XCTAssertNil(SharedStore(directory: dir.appendingPathComponent("none")).readStatus())
    }
    func testArmedFreshness() {
        XCTAssertTrue(Status(phase: .idle, at: 100).isArmed(now: 110))
        XCTAssertFalse(Status(phase: .idle, at: 100).isArmed(now: 130))      // stale heartbeat: app suspended or killed
        XCTAssertFalse(Status(phase: .disarmed, at: 100).isArmed(now: 101))
    }

    // MARK: engine

    func testNotArmedAnswersWithError() async {
        await send(.start)
        XCTAssertEqual(store.readStatus()?.phase, .error)
        XCTAssertFalse(cap.recording)
    }
    func testFullRoundTrip() async throws {
        try await engine.arm()
        XCTAssertEqual(phase, .idle); XCTAssertTrue(cap.armed)
        await send(.start)
        XCTAssertEqual(phase, .recording); XCTAssertEqual(store.readStatus()?.requestId, "r1"); XCTAssertTrue(cap.recording)
        var heard: [String] = []; engine.onTranscript = { heard.append($0) }
        await send(.stop)
        XCTAssertEqual(phase, .result)
        XCTAssertEqual(store.readStatus()?.text, "hello world")
        XCTAssertEqual(up.calls.first?.0, 16_000); XCTAssertEqual(up.calls.first?.1, "r1")
        XCTAssertEqual(heard, ["hello world"])
        await send(.ack)
        XCTAssertEqual(phase, .idle)
        XCTAssertTrue(note.posted.contains(Handoff.notificationName(group: group, .status)))
    }
    func testCancelWhileRecordingDiscards() async throws {
        try await engine.arm(); await send(.start); await send(.cancel)
        XCTAssertEqual(phase, .idle); XCTAssertEqual(cap.discards, 1); XCTAssertTrue(up.calls.isEmpty)
    }
    func testCancelWhileTranscribingDropsResult() async throws {
        try await engine.arm(); up.hold = true
        await send(.start); await send(.stop)
        XCTAssertEqual(phase, .transcribing)
        await send(.cancel)
        up.release(); for _ in 0..<20 { await Task.yield() }
        XCTAssertEqual(phase, .idle, "a cancelled upload must not publish a result")
    }
    func testUploadFailurePublishesError() async throws {
        try await engine.arm(); up.result = .failure(DictationFailure("Can't reach the server. Saved for Retry."))
        await send(.start); await send(.stop)
        XCTAssertEqual(phase, .error); XCTAssertEqual(store.readStatus()?.message, "Can't reach the server. Saved for Retry.")
    }
    func testEmptyTranscriptIsAnError() async throws {
        try await engine.arm(); up.result = .success("  ")
        await send(.start); await send(.stop)
        XCTAssertEqual(phase, .error); XCTAssertEqual(store.readStatus()?.message, "No speech detected.")
    }
    func testMicFailureOnBegin() async throws {
        try await engine.arm(); cap.failBegin = true
        await send(.start)
        XCTAssertEqual(phase, .error); XCTAssertEqual(store.readStatus()?.message, "Microphone access is off.")
        cap.failBegin = false; await send(.start, "r2")
        XCTAssertEqual(phase, .recording, "a failed start must not wedge the engine")
    }
    func testSecondStartWhileBusyIsIgnored() async throws {
        try await engine.arm(); await send(.start, "r1"); await send(.start, "r2")
        XCTAssertEqual(store.readStatus()?.requestId, "r1")
    }
    func testHeartbeatRefreshesStatus() async throws {
        try await engine.arm()
        let t0 = store.readStatus()!.at
        await clock.advance(Handoff.heartbeatInterval + 1)
        XCTAssertGreaterThan(store.readStatus()!.at, t0)
        XCTAssertTrue(store.readStatus()!.isArmed(now: clock.now))
    }
    func testIdleTimeoutDisarms() async throws {
        engine.idleTimeout = 60
        try await engine.arm()
        await clock.advance(61)
        XCTAssertFalse(engine.armed); XCTAssertFalse(cap.armed); XCTAssertEqual(phase, .disarmed)
    }
    func testIdleTimerPausedWhileRecordingAndRestartedAfter() async throws {
        engine.idleTimeout = 60
        try await engine.arm(); await send(.start)
        await clock.advance(50)          // recording; idle timer must not kill the session
        XCTAssertTrue(engine.armed)
        await send(.stop); await send(.ack, "r1")
        await clock.advance(30)
        XCTAssertTrue(engine.armed)
        await clock.advance(40)
        XCTAssertFalse(engine.armed)
    }
    func testRecordingCapStopsAndTranscribes() async throws {
        engine.idleTimeout = nil
        try await engine.arm(); await send(.start)
        await clock.advance(Handoff.maxRecording + 2)
        XCTAssertEqual(phase, .result)
    }
    func testNoIdleTimeoutStaysArmed() async throws {
        engine.idleTimeout = nil; try await engine.arm()
        await clock.advance(3_600); XCTAssertTrue(engine.armed)
    }
    func testDuplicateCommandNotificationHandledOnce() async throws {
        try await engine.arm()
        let c = Command(requestId: "r1", action: .start, at: 1)
        try store.write(c)
        note.post(Handoff.notificationName(group: group, .command)); note.post(Handoff.notificationName(group: group, .command))
        for _ in 0..<20 { await Task.yield() }
        XCTAssertEqual(phase, .recording)
        _ = c
    }
}

final class MicRemoteTests: XCTestCase {
    private func armed(_ at: TimeInterval = 100) -> Status { Status(phase: .idle, at: at) }

    func testFullAccessOffShowsExplanation() {
        var m = MicRemote()
        XCTAssertEqual(m.tapMic(fullAccess: false, status: armed(), now: 100, newId: "a"), [])
        XCTAssertEqual(m.ui, .needsAccess)
    }
    func testNotArmedPromptsAndOpensApp() {
        var m = MicRemote()
        XCTAssertEqual(m.tapMic(fullAccess: true, status: nil, now: 100, newId: "a"), []); XCTAssertEqual(m.ui, .needsArm)
        XCTAssertEqual(m.openAppTapped(), [.openApp]); XCTAssertEqual(m.ui, .idle)
        var s = MicRemote()
        _ = s.tapMic(fullAccess: true, status: armed(100), now: 200, newId: "a")   // stale heartbeat
        XCTAssertEqual(s.ui, .needsArm)
    }
    func testHappyPath() {
        var m = MicRemote()
        XCTAssertEqual(m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a"), [.send(.start, requestId: "a")])
        XCTAssertEqual(m.ui, .starting)
        XCTAssertEqual(m.statusChanged(Status(phase: .recording, requestId: "a", at: 102), now: 102), [])
        XCTAssertEqual(m.ui, .recording)
        XCTAssertEqual(m.tapMic(fullAccess: true, status: armed(), now: 105, newId: "b"), [.send(.stop, requestId: "a")])
        XCTAssertEqual(m.ui, .transcribing)
        XCTAssertEqual(m.statusChanged(Status(phase: .result, requestId: "a", text: "hi there", at: 107), now: 107),
                       [.insert("hi there"), .send(.ack, requestId: "a")])
        XCTAssertEqual(m.ui, .idle); XCTAssertNil(m.requestId)
    }
    func testOtherRequestsStatusIgnored() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        XCTAssertEqual(m.statusChanged(Status(phase: .result, requestId: "zzz", text: "nope", at: 102), now: 102), [])
        XCTAssertEqual(m.ui, .starting)
    }
    func testTapAgainWhileStartingOrTranscribingCancels() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        XCTAssertEqual(m.tapMic(fullAccess: true, status: armed(), now: 102, newId: "b"), [.send(.cancel, requestId: "a")])
        XCTAssertEqual(m.ui, .idle)
        _ = m.tapMic(fullAccess: true, status: armed(), now: 103, newId: "c")
        _ = m.statusChanged(Status(phase: .recording, requestId: "c", at: 103), now: 103)
        _ = m.tapMic(fullAccess: true, status: armed(), now: 104, newId: "d")           // stop
        XCTAssertEqual(m.tapMic(fullAccess: true, status: armed(), now: 105, newId: "e"), [.send(.cancel, requestId: "c")])
        XCTAssertEqual(m.ui, .idle)
    }
    func testDeleteWhileRecordingDiscards() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        XCTAssertEqual(m.discardIfRecording(), [])                      // still starting: delete is a normal delete
        _ = m.statusChanged(Status(phase: .recording, requestId: "a", at: 102), now: 102)
        XCTAssertEqual(m.discardIfRecording(), [.send(.cancel, requestId: "a")]); XCTAssertEqual(m.ui, .idle)
    }
    func testServerErrorShowsMessage() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        _ = m.statusChanged(Status(phase: .error, requestId: "a", message: "Can't reach the server.", at: 102), now: 102)
        XCTAssertEqual(m.ui, .failed("Can't reach the server."))
        // tapping the mic from the failed state tries again
        XCTAssertEqual(m.tapMic(fullAccess: true, status: armed(102), now: 103, newId: "b"), [.send(.start, requestId: "b")])
    }
    func testStartTimeoutMeansAppNotAnswering() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        XCTAssertEqual(m.deadline, 101 + Handoff.startAckTimeout)
        XCTAssertEqual(m.tick(now: 104), [])
        XCTAssertEqual(m.tick(now: 108), [.send(.cancel, requestId: "a")]); XCTAssertEqual(m.ui, .needsArm)
    }
    func testTranscribeTimeout() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        _ = m.statusChanged(Status(phase: .recording, requestId: "a", at: 102), now: 102)
        _ = m.tapMic(fullAccess: true, status: armed(), now: 110, newId: "b")
        XCTAssertEqual(m.tick(now: 110 + Handoff.transcribeTimeout + 1), [.send(.cancel, requestId: "a")])
        XCTAssertEqual(m.ui, .failed("Timed out waiting for the transcript."))
    }
    func testAppDisarmedMidRequest() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        _ = m.statusChanged(Status(phase: .disarmed, at: 102), now: 102)
        XCTAssertEqual(m.ui, .needsArm)
    }
    func testAppDroppedRequestReturnsToIdle() {
        var m = MicRemote()
        _ = m.tapMic(fullAccess: true, status: armed(), now: 101, newId: "a")
        _ = m.statusChanged(Status(phase: .idle, requestId: "a", at: 102), now: 102)
        XCTAssertEqual(m.ui, .idle)
    }
    func testFreshOrphanResultIsInsertedOnceAndStaleOneIsNot() {
        var m = MicRemote()
        let s = Status(phase: .result, requestId: "x", text: "late text", at: 100)
        XCTAssertEqual(m.statusChanged(s, now: 110), [.insert("late text"), .send(.ack, requestId: "x")])
        XCTAssertEqual(m.statusChanged(s, now: 100 + Handoff.resultMaxAge + 1), [])
        XCTAssertEqual(m.statusChanged(Status(phase: .idle, at: 100), now: 101), [])
    }
}

final class LocalBaseTests: XCTestCase {
    func testNilContainerFallsBack() {
        let fb = URL(fileURLWithPath: "/tmp/una-fallback")
        XCTAssertEqual(Handoff.localBase(group: "group.una.example", fallback: fb, container: { _ in nil }), fb)
        XCTAssertEqual(Handoff.localBase(group: nil, fallback: fb, container: { _ in URL(fileURLWithPath: "/x") }), fb)
    }
    func testContainerWinsWhenPresent() {
        let c = URL(fileURLWithPath: "/tmp/una-container")
        XCTAssertEqual(Handoff.localBase(group: "group.una.example", fallback: URL(fileURLWithPath: "/tmp/fb"), container: { _ in c }), c)
    }
    func testSpoolAndOutboxWorkInFallbackDir() throws {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent("una-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: d) }
        let base = Handoff.localBase(group: "group.una.example", fallback: d, container: { _ in nil })
        let spool = Spool(directory: base.appendingPathComponent("spool"))
        _ = try spool.add(wav: Data([1, 2, 3]), utteranceId: "u1")
        XCTAssertEqual(spool.list().count, 1)
        XCTAssertTrue(CorrectionOutbox(file: base.appendingPathComponent("corrections.json")).list().isEmpty)
    }
}
