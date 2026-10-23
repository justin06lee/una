#if DEBUG
import Foundation
import UnaKit

/// DEBUG ONLY. Stands in for the microphone when the keyboard asks for a dictation in the
/// simulator (whose mic is the Mac's and unreliable): `begin()`/`finish()` return the samples of
/// the WAV given as `-UnaDebugWAV <path>`. On a device `arm()` still opens the real audio
/// session so background keepalive is exercised; on the simulator it does not.
@MainActor
final class DebugFeedCapture: AudioCapture {
    private let real: Recorder
    private let path: String
    init(real: Recorder, path: String) { self.real = real; self.path = path }

    func arm() async throws {
        #if !targetEnvironment(simulator)
        try? await real.arm()   // the simulator's Core Audio can deadlock starting the engine, so it is skipped there
        #endif
    }
    func disarm() { real.disarm() }
    func begin() async throws {}
    func finish() -> [Int16] {
        guard let d = FileManager.default.contents(atPath: path), let w = WAV.decode(d) else { return [] }
        let f = w.samples.map { Float($0) / 32768 }
        return w.sampleRate == 16000 ? w.samples : WAV.resampleTo16k(f, inputRate: Double(w.sampleRate))
    }
    func discard() {}
}
#endif
