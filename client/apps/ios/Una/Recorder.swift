import AVFoundation
import UnaKit

/// Captures the mic with AVAudioEngine at the hardware rate and keeps Float mono samples;
/// `stop()` returns them as 16 kHz Int16 for the WAV encoder.
final class Recorder: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var buffer: [Float] = []
    private var rate: Double = 48_000

    private func reset() { lock.withLock { buffer = [] } }
    private func append(_ a: [Float]) { lock.withLock { buffer.append(contentsOf: a) } }
    private func take() -> [Float] { lock.withLock { let b = buffer; buffer = []; return b } }

    func start(onLevel: @escaping @Sendable (Float) -> Void) async throws {
        guard await AVAudioApplication.requestRecordPermission() else {
            throw NSError(domain: "Una", code: 1, userInfo: [NSLocalizedDescriptionKey: "Microphone access is off. Enable it in Settings → Una."])
        }
        let s = AVAudioSession.sharedInstance()
        try s.setCategory(.playAndRecord, mode: .measurement, options: [.defaultToSpeaker, .allowBluetoothHFP])
        try s.setActive(true)
        reset()
        let input = engine.inputNode
        let fmt = input.outputFormat(forBus: 0)
        rate = fmt.sampleRate
        input.installTap(onBus: 0, bufferSize: 4096, format: fmt) { [weak self] buf, _ in
            guard let self, let ch = buf.floatChannelData?[0] else { return }
            let n = Int(buf.frameLength)
            let arr = Array(UnsafeBufferPointer(start: ch, count: n))
            var peak: Float = 0
            for v in arr { peak = max(peak, abs(v)) }
            self.append(arr)
            onLevel(min(1, peak * 2))
        }
        engine.prepare()
        try engine.start()
    }

    func stop() -> [Int16] {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        let b = take()
        return WAV.resampleTo16k(b, inputRate: rate)
    }
}
