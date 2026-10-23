import Foundation

/// The app's side of keyboard dictation. Audio capture, upload and time are injected so the
/// whole thing runs under test with fakes; in the app they are the microphone, the endpoint
/// race and the real clock.
@MainActor public protocol AudioCapture: AnyObject {
    /// Open the audio session and start the (discarding) input so the app stays alive in the background.
    func arm() async throws
    func disarm()
    /// Start keeping samples. Instant while armed.
    func begin() async throws
    /// Stop keeping samples and return them as 16 kHz mono.
    func finish() -> [Int16]
    func discard()
}

@MainActor public protocol Transcribing: AnyObject {
    /// Uploads the audio and returns the transcript. Throw `DictationFailure` with a user-facing message.
    func transcribe(samples: [Int16], utteranceId: String) async throws -> String
}

public struct DictationFailure: Error, Equatable, LocalizedError {
    public var message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

public protocol DictationClock: AnyObject, Sendable {
    /// Wall clock seconds (shared with the keyboard through the status file).
    var now: TimeInterval { get }
    func sleep(_ seconds: TimeInterval) async throws
}

public final class SystemClock: DictationClock {
    public init() {}
    public var now: TimeInterval { Date().timeIntervalSince1970 }
    public func sleep(_ seconds: TimeInterval) async throws { try await Task.sleep(for: .seconds(seconds)) }
}

@MainActor
public final class DictationEngine {
    public private(set) var status: Status
    public private(set) var armed = false
    /// How long the armed session may sit unused before it closes. nil = never.
    public var idleTimeout: TimeInterval? = 300

    /// Called on every phase change so the UI can follow along.
    public var onChange: (() -> Void)?
    /// Called after a transcript was produced (for history and the result card).
    public var onTranscript: ((String) -> Void)?

    private let capture: AudioCapture
    private let uploader: Transcribing
    private let clock: DictationClock
    private let store: SharedStore
    private let notifier: Notifying
    private let group: String
    private var observation: AnyObject?
    private var lastHandled: Command?
    private var heartbeat: Task<Void, Never>?
    private var idleTimer: Task<Void, Never>?
    private var recordCap: Task<Void, Never>?
    private var upload: Task<Void, Never>?
    private var current: String?

    public init(capture: AudioCapture, uploader: Transcribing, clock: DictationClock, store: SharedStore, notifier: Notifying, group: String) {
        self.capture = capture; self.uploader = uploader; self.clock = clock
        self.store = store; self.notifier = notifier; self.group = group
        status = Status(phase: .disarmed, at: clock.now)
    }

    /// Start listening for commands. Safe to call once.
    public func startListening() {
        guard observation == nil else { return }
        observation = notifier.observe(Handoff.notificationName(group: group, .command)) { [weak self] in
            Task { @MainActor in self?.commandArrived() }
        }
    }

    // MARK: arming

    public func arm() async throws {
        if armed { publish(.idle); armIdleTimer(); return }
        try await capture.arm()
        armed = true
        publish(.idle)
        heartbeat = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                try? await self.clock.sleep(Handoff.heartbeatInterval)
                if Task.isCancelled { return }
                self.publish(self.status.phase, requestId: self.status.requestId, text: self.status.text, message: self.status.message)
            }
        }
        armIdleTimer()
    }

    public func disarm() {
        guard armed else { return }
        armed = false
        heartbeat?.cancel(); idleTimer?.cancel(); recordCap?.cancel(); upload?.cancel()
        capture.discard(); capture.disarm()
        current = nil
        publish(.disarmed)
    }

    // MARK: commands

    private func commandArrived() {
        guard let c = store.readCommand(), c != lastHandled else { return }
        lastHandled = c
        handle(c)
    }

    public func handle(_ c: Command) {
        guard armed else {
            publish(.error, requestId: c.requestId, message: "Una isn't ready. Open Una to arm dictation.")
            publishDisarmedSoon(); return
        }
        switch c.action {
        case .start:
            guard current == nil else { return }   // one dictation at a time
            current = c.requestId
            idleTimer?.cancel()
            Task { await beginRecording(c.requestId) }
        case .stop:
            guard current == c.requestId, status.phase == .recording else { return }
            finishRecording(c.requestId)
        case .cancel:
            guard current == c.requestId else { return }
            recordCap?.cancel(); upload?.cancel(); capture.discard()
            current = nil
            publish(.idle, requestId: c.requestId)
            armIdleTimer()
        case .ack:
            if status.phase == .result, status.requestId == c.requestId { publish(.idle) }
        }
    }

    private func publishDisarmedSoon() { /* stale status already says disarmed to the keyboard */ }

    private func beginRecording(_ id: String) async {
        do {
            try await capture.begin()
            guard current == id else { capture.discard(); return }
            publish(.recording, requestId: id)
            recordCap = Task { [weak self] in
                try? await self?.clock.sleep(Handoff.maxRecording)
                if Task.isCancelled { return }
                self?.finishRecording(id)
            }
        } catch {
            current = nil
            publish(.error, requestId: id, message: error.localizedDescription)
            armIdleTimer()
        }
    }

    private func finishRecording(_ id: String) {
        guard current == id, status.phase == .recording else { return }
        recordCap?.cancel()
        let samples = capture.finish()
        publish(.transcribing, requestId: id)
        upload = Task { [weak self] in
            guard let self else { return }
            do {
                let text = try await self.uploader.transcribe(samples: samples, utteranceId: id)
                if Task.isCancelled || self.current != id { return }
                self.current = nil
                if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    self.publish(.error, requestId: id, message: "No speech detected.")
                } else {
                    self.publish(.result, requestId: id, text: text)
                    self.onTranscript?(text)
                }
            } catch {
                if Task.isCancelled || self.current != id { return }
                self.current = nil
                self.publish(.error, requestId: id, message: (error as? DictationFailure)?.message ?? error.localizedDescription)
            }
            self.armIdleTimer()
        }
    }

    // MARK: timers and publishing

    private func armIdleTimer() {
        idleTimer?.cancel()
        guard armed, let t = idleTimeout else { return }
        idleTimer = Task { [weak self] in
            try? await self?.clock.sleep(t)
            if Task.isCancelled { return }
            self?.disarm()
        }
    }

    private func publish(_ phase: Status.Phase, requestId: String? = nil, text: String? = nil, message: String? = nil) {
        status = Status(phase: phase, requestId: requestId, text: text, message: message, at: clock.now)
        try? store.write(status)
        notifier.post(Handoff.notificationName(group: group, .status))
        onChange?()
    }
}
