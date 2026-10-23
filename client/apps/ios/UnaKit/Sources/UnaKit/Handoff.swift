import Foundation

/// The protocol between the Una app (which records) and the keyboard (a remote control).
///
/// The keyboard cannot record or use the network. It writes a `Command` into the App Group
/// container and posts a Darwin notification; the app, kept alive by an active audio session,
/// acts on it and writes a `Status` (recording, transcribing, the transcript) the same way. The
/// notifications carry no data, the files do. Both directions are "latest value wins".
public enum Handoff {
    public enum Channel: String, Sendable { case command, status }

    /// Darwin notification name, namespaced by the App Group so two builds don't talk to each other.
    public static func notificationName(group: String, _ c: Channel) -> String { "\(group).handoff.\(c.rawValue)" }

    public static let commandFile = "command.json"
    public static let statusFile = "status.json"
    public static let directoryName = "handoff"

    /// App heartbeat period while armed, and how stale a heartbeat may be before the keyboard
    /// treats the app as not running.
    public static let heartbeatInterval: TimeInterval = 5
    public static let armedTTL: TimeInterval = 20
    /// Keyboard gives the app this long to answer a start command.
    public static let startAckTimeout: TimeInterval = 6
    /// Upload plus transcription budget (the client's request timeout is 60 s, plus one retry).
    public static let transcribeTimeout: TimeInterval = 100
    /// A finished transcript the keyboard never picked up is dropped after this.
    public static let resultMaxAge: TimeInterval = 120
    /// Recording stops itself after this (matches the desktop client's five minutes).
    public static let maxRecording: TimeInterval = 300

    /// The App Group id from the Info.plist key `UnaAppGroup`, set from `Config/Una.xcconfig`.
    public static func appGroup(bundle: Bundle = .main) -> String? {
        let v = bundle.object(forInfoDictionaryKey: "UnaAppGroup") as? String
        return (v?.isEmpty == false && v?.hasPrefix("$(") == false) ? v : nil
    }

    /// Where the app keeps its spool and correction outbox: the group container when the
    /// entitlement exists, else the app's own Application Support (free-team builds).
    public static func localBase(group: String?, fallback: URL = .applicationSupportDirectory,
                                 container: (String) -> URL? = { FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0) }) -> URL {
        group.flatMap(container) ?? fallback
    }

    /// Shown on the keyboard's mic key when the build has no App Group.
    public static let noGroupNotice = "Dictation needs the Una app group, which a free Apple account can't sign. Typing works."

    /// `<group container>/handoff`, or nil when the entitlement is missing.
    public static func directory(group: String) -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)?
            .appendingPathComponent(directoryName, isDirectory: true)
    }
}

public struct Command: Codable, Equatable, Sendable {
    public enum Action: String, Codable, Sendable { case start, stop, cancel, ack }
    public var requestId: String
    public var action: Action
    public var at: TimeInterval
    public init(requestId: String, action: Action, at: TimeInterval) {
        self.requestId = requestId; self.action = action; self.at = at
    }
}

public struct Status: Codable, Equatable, Sendable {
    public enum Phase: String, Codable, Sendable {
        /// The app has no live audio session; the keyboard must ask the user to open it.
        case disarmed
        case idle, recording, transcribing, result, error
    }
    public var phase: Phase
    public var requestId: String?
    public var text: String?
    public var message: String?
    /// Wall clock of the last write; refreshed by the heartbeat while armed.
    public var at: TimeInterval
    public init(phase: Phase, requestId: String? = nil, text: String? = nil, message: String? = nil, at: TimeInterval) {
        self.phase = phase; self.requestId = requestId; self.text = text; self.message = message; self.at = at
    }

    /// Whether the app can take a start command right now.
    public func isArmed(now: TimeInterval) -> Bool {
        phase != .disarmed && now - at < Handoff.armedTTL && at <= now + 5
    }
}

/// Reads and writes the two JSON files atomically. Takes a directory so tests use a temp one.
public struct SharedStore: Sendable {
    public let directory: URL
    public init(directory: URL) { self.directory = directory }

    public func write(_ c: Command) throws { try put(c, Handoff.commandFile) }
    public func write(_ s: Status) throws { try put(s, Handoff.statusFile) }
    public func readCommand() -> Command? { get(Handoff.commandFile) }
    public func readStatus() -> Status? { get(Handoff.statusFile) }

    private func put<T: Encodable>(_ v: T, _ name: String) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try JSONEncoder().encode(v).write(to: directory.appendingPathComponent(name), options: .atomic)
    }
    private func get<T: Decodable>(_ name: String) -> T? {
        guard let d = try? Data(contentsOf: directory.appendingPathComponent(name)) else { return nil }
        return try? JSONDecoder().decode(T.self, from: d)
    }
}

/// Cross-process "something changed" pokes. The real one is the Darwin notify centre; tests fake it.
public protocol Notifying: AnyObject, Sendable {
    func post(_ name: String)
    /// Returns a token; the observation lives as long as the token does.
    func observe(_ name: String, _ handler: @escaping @Sendable () -> Void) -> AnyObject
}

public final class DarwinNotifier: Notifying {
    public init() {}

    private final class Token {
        let box: Box
        init(_ box: Box) { self.box = box }
        deinit { CFNotificationCenterRemoveEveryObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(box).toOpaque()) }
    }
    private final class Box { let handler: @Sendable () -> Void; init(_ h: @escaping @Sendable () -> Void) { handler = h } }

    public func post(_ name: String) {
        CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName(name as CFString), nil, nil, true)
    }

    public func observe(_ name: String, _ handler: @escaping @Sendable () -> Void) -> AnyObject {
        let box = Box(handler)
        CFNotificationCenterAddObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(box).toOpaque(), { _, observer, _, _, _ in
            guard let observer else { return }
            Unmanaged<Box>.fromOpaque(observer).takeUnretainedValue().handler()
        }, name as CFString, nil, .deliverImmediately)
        return Token(box)
    }
}

/// The words shown wherever Full Access is explained (onboarding, Servers, and the keyboard itself).
public enum FullAccessText {
    public static let explanation = "Una's keyboard asks for Full Access so it can talk to the Una app, which does the recording and sends the audio to your server. Full Access also lets the keyboard use haptics. Audio goes only to the server you configured. Nothing is sent to us or anyone else, and the keyboard never reads what you type."
    /// Shown in the keyboard when its mic key is tapped with Full Access off.
    public static let keyboardTitle = "Allow Full Access to dictate"
    public static let keyboardSteps = "Settings → General → Keyboard → Keyboards → Una → Allow Full Access. You can still type without it."
    public static let armTitle = "Open Una to start dictation"
    public static let armBody = "Una records in its own app and keeps the microphone ready in the background. Open it once, then use the ◀ link at the top left to come back here."
}
