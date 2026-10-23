import AVFoundation
import SwiftUI
import UnaKit

/// Opens one dictation: both texts, the recording, a word diff of your edit, and whether a
/// correction is already filed. Saving sends `PUT /v1/dictations/{id}/correction`, or keeps the
/// fix on the phone until the server answers.
struct FixView: View {
    let summary: DictationSummary
    @Environment(AppModel.self) private var m
    @State private var detail: DictationDetail?
    @State private var draft = FixDraft(rawText: "", insertedText: "")
    @State private var loadError: String?
    @State private var saving = false
    @State private var note: String?
    @State private var queued = false
    @State private var player = AudioPlayer()

    var body: some View {
        Form {
            if let loadError { Section { Text(loadError).foregroundStyle(.secondary); Button("Try again") { Task { await load() } } } }
            if let detail {
                Section { statusRow(detail) }
                Section("Recording") {
                    Button { Task { await player.toggle { try await m.loadAudio(id: summary.id) } } } label: {
                        Label(player.playing ? "Stop" : "Play", systemImage: player.playing ? "stop.fill" : "play.fill")
                    }
                    if let e = player.error { Text(e).font(.caption).foregroundStyle(.secondary) }
                }
                Section {
                    TextEditor(text: $draft.transcript).frame(minHeight: 70).accessibilityLabel("What you said")
                    DiffText(parts: draft.transcriptDiff, show: draft.transcriptChanged)
                } header: { Text("What you said") } footer: { Text("The transcript as the speech model heard it.") }
                Section {
                    TextEditor(text: $draft.final).frame(minHeight: 70).accessibilityLabel("How it should read")
                    DiffText(parts: draft.finalDiff, show: draft.finalChanged)
                } header: { Text("How it should read") } footer: {
                    Text(detail.cleanupApplied ? "The cleaned text that was inserted." : "Cleanup didn't change this dictation.")
                }
                Section {
                    Button { Task { await save() } } label: {
                        HStack { Text(draft.transcriptChanged || draft.finalChanged ? "Save correction" : "Mark as correct"); if saving { Spacer(); ProgressView() } }
                    }.disabled(saving || !draft.canSave)
                    if let note { Text(note).font(.footnote).foregroundStyle(.secondary) }
                    Button("Copy text") { m.copy(draft.final) }
                }
            } else if loadError == nil { Section { HStack { ProgressView(); Text("Loading…").padding(.leading, 8) } } }
        }
        .navigationTitle("Dictation").navigationBarTitleDisplayMode(.inline)
        .task { await load() }
        .onDisappear { player.stop() }
    }

    @ViewBuilder private func statusRow(_ d: DictationDetail) -> some View {
        if queued || m.pendingCorrections.contains(d.id) {
            Label("Your fix is saved on this phone and will be sent when the server answers.", systemImage: "arrow.up.circle")
        } else {
            switch d.filedState {
            case .none: Label("No correction filed yet.", systemImage: "circle.dashed")
            case .waiting: Label("Waiting to send.", systemImage: "arrow.up.circle")
            case .polishedOnly: Label("Text fix filed; the transcript hasn't been judged.", systemImage: "circle.lefthalf.filled")
            case .filed(let a, let s):
                Label("Filed: \(a)\(s.map { " (\($0))" } ?? "")", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            }
        }
    }

    private func load() async {
        loadError = nil
        do {
            let d = try await m.loadDetail(id: summary.id)
            detail = d
            // A fix filed earlier starts the fields where you left them.
            var f = FixDraft(rawText: d.rawText, insertedText: d.text)
            if let c = d.correctedText, d.reviewAction == "edited" { f.transcript = c }
            if let p = d.polishedText { f.final = p }
            draft = f
        } catch { loadError = error.localizedDescription }
    }

    private func save() async {
        saving = true; defer { saving = false }
        do {
            switch try await m.submitCorrection(id: summary.id, draft.request()) {
            case .sent: note = "Sent."; queued = false
            case .queued(let why): note = "\(why) Saved on this phone; it will send later."; queued = true
            }
            if let d = try? await m.loadDetail(id: summary.id) { detail = d }
        } catch { note = error.localizedDescription }
    }
}

private struct DiffText: View {
    let parts: [DiffPart]
    let show: Bool
    var body: some View {
        if show {
            parts.reduce(Text("")) { acc, p in
                let t = Text(p.text + " ")
                switch p.kind {
                case .same: return acc + t
                case .removed: return acc + t.strikethrough().foregroundColor(.red)
                case .added: return acc + t.bold().foregroundColor(.green)
                }
            }
            .font(.callout).accessibilityLabel("Changes")
        }
    }
}

@MainActor @Observable
final class AudioPlayer: NSObject, AVAudioPlayerDelegate {
    var playing = false
    var error: String?
    private var player: AVAudioPlayer?

    func toggle(_ fetch: () async throws -> Data) async {
        if playing { stop(); return }
        do {
            let data = try await fetch()
            // Leave a live recording session (armed keyboard) alone; otherwise play through the speaker.
            if AVAudioSession.sharedInstance().category != .playAndRecord { try AVAudioSession.sharedInstance().setCategory(.playback) }
            let p = try AVAudioPlayer(data: data); p.delegate = self
            player = p; error = nil; playing = p.play()
        } catch { self.error = "Can't play the recording: \(error.localizedDescription)" }
    }
    func stop() { player?.stop(); player = nil; playing = false }
    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        Task { @MainActor in self.playing = false }
    }
}
