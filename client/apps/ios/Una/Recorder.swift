import AVFoundation
import UnaKit

/// Microphone capture with AVAudioEngine. `arm()` opens the audio session and runs the input
/// continuously, throwing the samples away, so the app stays alive in the background (the
/// `audio` background mode) and `begin()` starts instantly even when the app isn't frontmost.
/// `finish()` returns what was kept as 16 kHz Int16 for the WAV encoder.
@MainActor
final class Recorder: AudioCapture {
    /// Samples and the capturing flag live outside the main actor: the tap runs on an audio thread.
    private final class Store: @unchecked Sendable {
        private let lock = NSLock()
        private var samples: [Float] = []
        private var capturing = false
        private var onLevel: (@Sendable (Float) -> Void)?
        func setLevel(_ f: (@Sendable (Float) -> Void)?) { lock.withLock { onLevel = f } }
        func begin() { lock.withLock { samples = []; capturing = true } }
        func end() -> [Float] { lock.withLock { capturing = false; let s = samples; samples = []; return s } }
        func feed(_ a: UnsafeBufferPointer<Float>) {
            var peak: Float = 0
            for v in a { peak = max(peak, abs(v)) }
            let (take, cb) = lock.withLock { () -> (Bool, (@Sendable (Float) -> Void)?) in
                if capturing { samples.append(contentsOf: a) }
                return (capturing, onLevel)
            }
            if take { cb?(min(1, peak * 2)) }
        }
    }

    private let engine = AVAudioEngine()
    private let store = Store()
    private var rate: Double = 48_000
    private(set) var isArmed = false
    private var observers: [NSObjectProtocol] = []

    /// Called when the system takes the audio session away (a phone call, Siri, another app).
    var onInterrupted: (() -> Void)?
    var onLevel: (@Sendable (Float) -> Void)? { didSet { store.setLevel(onLevel) } }

    func arm() async throws {
        guard !isArmed else { return }
        guard await AVAudioApplication.requestRecordPermission() else {
            throw DictationFailure("Microphone access is off. Enable it in Settings → Una.")
        }
        let s = AVAudioSession.sharedInstance()
        try s.setCategory(.playAndRecord, mode: .measurement, options: [.defaultToSpeaker, .allowBluetoothHFP, .mixWithOthers])
        try s.setActive(true)
        try startEngine()
        isArmed = true
        let nc = NotificationCenter.default
        observers = [
            nc.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] n in
                let raw = n.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt
                guard raw == AVAudioSession.InterruptionType.began.rawValue else { return }
                MainActor.assumeIsolated { self?.onInterrupted?() }
            },
            nc.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated {   // a route change (headphones, Bluetooth) stops the engine; restart it
                    guard let self, self.isArmed else { return }
                    self.engine.inputNode.removeTap(onBus: 0)
                    do { try self.startEngine() } catch { self.onInterrupted?() }
                }
            },
        ]
    }

    private func startEngine() throws {
        let input = engine.inputNode
        let fmt = input.outputFormat(forBus: 0)
        rate = fmt.sampleRate
        let store = self.store
        input.installTap(onBus: 0, bufferSize: 4096, format: fmt) { buf, _ in
            guard let ch = buf.floatChannelData?[0] else { return }
            store.feed(UnsafeBufferPointer(start: ch, count: Int(buf.frameLength)))
        }
        engine.prepare()
        try engine.start()
    }

    func disarm() {
        guard isArmed else { return }
        isArmed = false
        observers.forEach(NotificationCenter.default.removeObserver); observers = []
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        _ = store.end()
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func begin() async throws {
        if !isArmed { throw DictationFailure("Microphone isn't ready.") }
        store.begin()
    }

    func finish() -> [Int16] { WAV.resampleTo16k(store.end(), inputRate: rate) }
    func discard() { _ = store.end() }
}
