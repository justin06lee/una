import SwiftUI
import UIKit
import UnaKit

@MainActor @Observable
final class AppModel {
    enum Phase: Equatable { case idle, recording, uploading, done, failed(String) }

    var servers: [ServerEntry] { didSet { saveServers() } }
    var probes: [UUID: ProbeResult] = [:]
    var testing = false
    var phase: Phase = .idle
    var level: Float = 0
    var seconds: Double = 0
    var last: DictationResponse?
    var history: [DictationSummary] = []
    var historyError: String?
    var spooled: [Spool.Item] = []
    var activeServer: ServerEntry?
    var onboarded: Bool = UserDefaults.standard.bool(forKey: "onboarded")

    let recorder = Recorder()
    let race = EndpointRace()
    let spool: Spool
    private var ticker: Task<Void, Never>?

    init() {
        let dir = URL.applicationSupportDirectory.appendingPathComponent("spool")
        spool = Spool(directory: dir)
        if let d = UserDefaults.standard.data(forKey: "servers"),
           let s = try? JSONDecoder().decode([ServerEntry].self, from: d), !s.isEmpty { servers = s }
        else { servers = [] }
    }

    private func saveServers() {
        UserDefaults.standard.set(try? JSONEncoder().encode(servers), forKey: "servers")
    }

    func start() async {
        refreshSpool()
        #if DEBUG
        if let path = UserDefaults.standard.string(forKey: "UnaDebugWAV"), UserDefaults.standard.bool(forKey: "UnaDebugAutorun") {
            await debugFeed(path: path)
        }
        #endif
    }

    func finishOnboarding(adding entry: ServerEntry?) {
        if let entry { servers.append(entry) }
        onboarded = true
        UserDefaults.standard.set(true, forKey: "onboarded")
    }

    func refreshSpool() { spooled = spool.list() }

    // MARK: recording

    func toggleRecording() async {
        switch phase {
        case .recording: await finishRecording()
        case .uploading: break
        default:
            do {
                try await recorder.start { [weak self] l in Task { @MainActor in self?.level = l } }
                phase = .recording; seconds = 0
                let t0 = Date()
                ticker = Task { [weak self] in
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .milliseconds(100))
                        guard let self else { return }
                        self.seconds = Date().timeIntervalSince(t0)
                        if self.seconds >= 300 { await self.finishRecording(); return }
                    }
                }
            } catch { phase = .failed(error.localizedDescription) }
        }
    }

    private func finishRecording() async {
        ticker?.cancel(); level = 0
        let samples = recorder.stop()
        await upload(samples: samples)
    }

    func upload(samples: [Int16]) async {
        guard WAV.durationMs(sampleCount: samples.count) >= 300 else { phase = .failed("Too short — hold a little longer."); return }
        phase = .uploading
        let wav = WAV.encode(samples), id = UUID().uuidString.lowercased()
        await send(wav: wav, id: id)
    }

    private func send(wav: Data, id: String) async {
        guard let server = await race.resolve(servers), let base = server.baseURL else {
            _ = try? spool.add(wav: wav, utteranceId: id); refreshSpool()
            phase = .failed("No server answered. Saved for Retry."); return
        }
        activeServer = server
        do {
            let r = try await UnaClient(baseURL: base).createDictation(wav: wav, utteranceId: id)
            UIPasteboard.general.string = r.text
            last = r; phase = .done
            await loadHistory()
        } catch let e as UnaError {
            await race.invalidate()
            if e.isRetryable { _ = try? spool.add(wav: wav, utteranceId: id); refreshSpool() }
            phase = .failed(e.localizedDescription + (e.isRetryable ? " Saved for Retry." : ""))
        } catch { phase = .failed(error.localizedDescription) }
    }

    func retry(_ item: Spool.Item) async {
        guard let data = try? spool.data(for: item) else { return }
        phase = .uploading
        await send(wav: data, id: item.utteranceId)
        if case .done = phase { spool.remove(item) }
        refreshSpool()
    }

    func discard(_ item: Spool.Item) { spool.remove(item); refreshSpool() }

    // MARK: servers & history

    func testAll() async {
        testing = true
        let results = await race.testAll(servers)
        probes = Dictionary(uniqueKeysWithValues: results.map { ($0.entry.id, $0) })
        testing = false
    }

    func loadHistory() async {
        guard let server = await race.resolve(servers), let base = server.baseURL else {
            historyError = "No server answered."; return
        }
        do { history = try await UnaClient(baseURL: base).listDictations(limit: 50).items; historyError = nil }
        catch { historyError = error.localizedDescription }
    }

    func copy(_ text: String) { UIPasteboard.general.string = text }

    #if DEBUG
    /// DEBUG ONLY: feeds a WAV from disk through the same encode → upload path as a recording,
    /// for the simulator demo (the simulator mic is the Mac's, which is silent in a quiet room).
    /// Launch with `-UnaDebugWAV /path/clip.wav`; add `-UnaDebugAutorun YES` to run on start.
    func debugFeed(path: String) async {
        guard let d = FileManager.default.contents(atPath: path), let w = WAV.decode(d) else {
            phase = .failed("Debug WAV unreadable: \(path)"); return
        }
        let f = w.samples.map { Float($0) / 32768 }
        await upload(samples: w.sampleRate == 16000 ? w.samples : WAV.resampleTo16k(f, inputRate: Double(w.sampleRate)))
    }
    #endif
}
