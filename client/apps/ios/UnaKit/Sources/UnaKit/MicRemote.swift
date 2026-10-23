import Foundation

/// The keyboard's side of dictation as a pure state machine: what the mic key shows, and what the
/// keyboard should do next. It never records or touches the network; the controller turns its
/// effects into container writes, notifications and `insertText`.
public struct MicRemote: Equatable, Sendable {
    public enum UI: Equatable, Sendable {
        case idle
        /// Full Access is off: explain why it is needed.
        case needsAccess
        /// The app isn't running or has no live audio session: ask the user to open it.
        case needsArm
        case starting, recording, transcribing
        case failed(String)
    }
    public enum Effect: Equatable, Sendable {
        case send(Command.Action, requestId: String)
        case insert(String)
        case openApp
    }

    public private(set) var ui: UI = .idle
    public private(set) var requestId: String?
    /// When `tick` next needs to run (wall clock), or nil.
    public private(set) var deadline: TimeInterval?

    public init() {}

    public var busy: Bool { [.starting, .recording, .transcribing].contains(ui) }

    /// The mic key was tapped.
    public mutating func tapMic(fullAccess: Bool, status: Status?, now: TimeInterval, newId: String) -> [Effect] {
        guard fullAccess else { ui = .needsAccess; return [] }
        switch ui {
        case .idle, .needsAccess, .needsArm, .failed:
            guard let status, status.isArmed(now: now) else { ui = .needsArm; return [] }
            requestId = newId; ui = .starting; deadline = now + Handoff.startAckTimeout
            return [.send(.start, requestId: newId)]
        case .starting, .transcribing:
            return cancel()                 // tapping again while waiting cancels
        case .recording:
            guard let id = requestId else { return reset() }
            ui = .transcribing; deadline = now + Handoff.transcribeTimeout
            return [.send(.stop, requestId: id)]
        }
    }

    /// Something local went wrong (the command couldn't be written).
    public mutating func fail(_ message: String) { requestId = nil; deadline = nil; ui = .failed(message) }

    /// Delete pressed while recording discards the take.
    public mutating func discardIfRecording() -> [Effect] { ui == .recording ? cancel() : [] }

    /// The user tapped "Open Una" in the arm prompt.
    public mutating func openAppTapped() -> [Effect] { dismiss(); return [.openApp] }

    public mutating func dismiss() {
        if case .idle = ui { return }
        if !busy { ui = .idle }
    }

    /// The status file changed (Darwin notification, or the keyboard appeared).
    public mutating func statusChanged(_ s: Status?, now: TimeInterval) -> [Effect] {
        guard let s else { return [] }
        guard let id = requestId else { return recover(s, now: now) }
        guard s.requestId == id || s.phase == .disarmed else { return [] }
        switch s.phase {
        case .recording:
            if ui == .starting || ui == .recording {
                ui = .recording; deadline = now + Handoff.maxRecording + 10
            }
            return []
        case .transcribing:
            ui = .transcribing; deadline = now + Handoff.transcribeTimeout
            return []
        case .result:
            let text = s.text ?? ""
            _ = reset()
            return text.isEmpty ? [.send(.ack, requestId: id)] : [.insert(text), .send(.ack, requestId: id)]
        case .error:
            _ = reset(); ui = .failed(s.message ?? "Dictation failed."); return []
        case .idle:                         // the app dropped our request (cancelled)
            return reset()
        case .disarmed:
            _ = reset(); ui = .needsArm; return []
        }
    }

    /// Periodic check against `deadline`.
    public mutating func tick(now: TimeInterval) -> [Effect] {
        guard let d = deadline, now >= d, let id = requestId else { return [] }
        switch ui {
        case .starting:
            reset2(); ui = .needsArm
            return [.send(.cancel, requestId: id)]
        case .recording, .transcribing:
            reset2(); ui = .failed("Timed out waiting for the transcript.")
            return [.send(.cancel, requestId: id)]
        default: deadline = nil; return []
        }
    }

    private mutating func cancel() -> [Effect] {
        guard let id = requestId else { return reset() }
        _ = reset()
        return [.send(.cancel, requestId: id)]
    }

    @discardableResult private mutating func reset() -> [Effect] { reset2(); ui = .idle; return [] }
    private mutating func reset2() { requestId = nil; deadline = nil }

    /// A finished transcript this keyboard never saw (it was torn down mid-request) is inserted
    /// once if it is still fresh; the ack clears it so a second keyboard won't repeat it.
    private mutating func recover(_ s: Status, now: TimeInterval) -> [Effect] {
        guard s.phase == .result, let id = s.requestId, let text = s.text, !text.isEmpty,
              now - s.at < Handoff.resultMaxAge else { return [] }
        return [.insert(text), .send(.ack, requestId: id)]
    }
}
