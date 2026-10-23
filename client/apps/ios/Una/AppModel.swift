import SwiftUI
import UIKit
import UnaKit

@MainActor @Observable
final class AppModel: Transcribing {
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
    var pendingCorrections: [String] = []
    var activeServer: ServerEntry?
    var onboarded: Bool = UserDefaults.standard.bool(forKey: "onboarded")

    let recorder = Recorder()
    let race = EndpointRace()
    let spool: Spool
    let outbox: CorrectionOutbox
    private var ticker: Task<Void, Never>?

    // Keyboard dictation: the engine answers the keyboard's commands (see UnaKit Handoff).
    private(set) var engine: DictationEngine?
    var keyboardPhase: Status.Phase = .disarmed
    var armError: String?
    /// Minutes the armed audio session may sit idle before it closes; 0 = never.
    var idleMinutes: Int = UserDefaults.standard.object(forKey: "idleMinutes") as? Int ?? 5 {
        didSet { UserDefaults.standard.set(idleMinutes, forKey: "idleMinutes"); engine?.idleTimeout = idleMinutes > 0 ? Double(idleMinutes) * 60 : nil }
    }
    var keyboardArmed: Bool { keyboardPhase != .disarmed }

    init() {
        let group = Handoff.appGroup()
        let shared = group.flatMap { g in FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: g) }
        spool = Spool(directory: (shared ?? URL.applicationSupportDirectory).appendingPathComponent("spool"))
        outbox = CorrectionOutbox(file: (shared ?? URL.applicationSupportDirectory).appendingPathComponent("corrections.json"))
        pendingCorrections = outbox.list().map(\.dictationId)
        if let d = UserDefaults.standard.data(forKey: "servers"),
           let s = try? JSONDecoder().decode([ServerEntry].self, from: d), !s.isEmpty { servers = s }
        else { servers = [] }
        if let group, let dir = Handoff.directory(group: group) {
            var capture: AudioCapture = recorder
            #if DEBUG
            if let p = UserDefaults.standard.string(forKey: "UnaDebugWAV") { capture = DebugFeedCapture(real: recorder, path: p) }
            #endif
            let e = DictationEngine(capture: capture, uploader: self, clock: SystemClock(), store: SharedStore(directory: dir),
                                    notifier: DarwinNotifier(), group: group)
            e.idleTimeout = idleMinutes > 0 ? Double(idleMinutes) * 60 : nil
            e.onChange = { [weak self, weak e] in if let e { self?.keyboardPhase = e.armed ? e.status.phase : .disarmed } }
            recorder.onInterrupted = { [weak e] in e?.disarm() }
            e.startListening()
            engine = e
        } else { armError = "The shared container is unavailable, so the keyboard can't talk to the app." }
    }

    // MARK: keyboard dictation

    func armKeyboard() async {
        guard let engine else { return }
        do { try await engine.arm(); armError = nil } catch { armError = error.localizedDescription }
        keyboardPhase = engine.armed ? engine.status.phase : .disarmed
    }

    func disarmKeyboard() { engine?.disarm(); keyboardPhase = .disarmed }

    /// Used by the engine when the keyboard asked for a dictation.
    func transcribe(samples: [Int16], utteranceId: String) async throws -> String {
        guard WAV.durationMs(sampleCount: samples.count) >= 300 else { throw DictationFailure("Too short. Speak a little longer.") }
        let wav = WAV.encode(samples)
        guard let server = await race.resolve(servers), let base = server.baseURL else {
            _ = try? spool.add(wav: wav, utteranceId: utteranceId); refreshSpool()
            throw DictationFailure("No server answered. Saved in Una for Retry.")
        }
        activeServer = server
        do {
            let r = try await UnaClient(baseURL: base).createDictation(wav: wav, utteranceId: utteranceId)
            last = r
            await loadHistory()
            return r.text
        } catch let e as UnaError {
            await race.invalidate()
            if e.isRetryable { _ = try? spool.add(wav: wav, utteranceId: utteranceId); refreshSpool() }
            throw DictationFailure(e.localizedDescription + (e.isRetryable ? " Saved in Una for Retry." : ""))
        }
    }

    private func saveServers() {
        UserDefaults.standard.set(try? JSONEncoder().encode(servers), forKey: "servers")
    }

    func start() async {
        refreshSpool()
        #if DEBUG
        if UserDefaults.standard.bool(forKey: "UnaDebugArm") { await armKeyboard() }
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
            guard engine?.status.phase != .recording, engine?.status.phase != .transcribing else {
                phase = .failed("The keyboard is dictating right now."); return
            }
            do {
                recorder.onLevel = { [weak self] l in Task { @MainActor in self?.level = l } }
                try await recorder.arm()
                try await recorder.begin()
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
            } catch { if !keyboardArmed { recorder.disarm() }; phase = .failed(error.localizedDescription) }
        }
    }

    private func finishRecording() async {
        ticker?.cancel(); level = 0
        let samples = recorder.finish()
        recorder.onLevel = nil
        if !keyboardArmed { recorder.disarm() }
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
        do { history = try await UnaClient(baseURL: base).listDictations(limit: 50).items; historyError = nil; await flushCorrections() }
        catch { historyError = error.localizedDescription }
    }

    // MARK: corrections

    private func client() async -> UnaClient? {
        guard let server = await race.resolve(servers), let base = server.baseURL else { return nil }
        activeServer = server
        return UnaClient(baseURL: base)
    }

    func loadDetail(id: String) async throws -> DictationDetail {
        guard let c = await client() else { throw UnaError.unreachable("No server answered.") }
        return try await c.dictation(id: id)
    }

    func loadAudio(id: String) async throws -> Data {
        guard let c = await client() else { throw UnaError.unreachable("No server answered.") }
        return try await c.audio(id: id)
    }

    enum SubmitResult: Equatable { case sent(CorrectionResponse), queued(String) }

    /// Sends the fix now, or keeps it on the phone and sends it when the server next answers.
    func submitCorrection(id: String, _ req: CorrectionRequest) async throws -> SubmitResult {
        if let c = await client() {
            do {
                let r = try await c.correct(id: id, req)
                outbox.remove(id: id); pendingCorrections = outbox.list().map(\.dictationId)
                await loadHistory()
                return .sent(r)
            } catch let e as UnaError where e.isRetryable { await race.invalidate(); return queue(id, req, e.localizedDescription) }
        }
        return queue(id, req, "No server answered.")
    }

    private func queue(_ id: String, _ req: CorrectionRequest, _ why: String) -> SubmitResult {
        outbox.add(id: id, req); pendingCorrections = outbox.list().map(\.dictationId)
        return .queued(why)
    }

    func flushCorrections() async {
        guard !outbox.list().isEmpty, let c = await client() else { return }
        _ = await outbox.flush(using: c)
        pendingCorrections = outbox.list().map(\.dictationId)
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
