import SwiftUI
import UnaKit

struct RecordView: View {
    @Environment(AppModel.self) private var m

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                statusLine
                Spacer(minLength: 0)
                if let r = m.last, m.phase == .done || m.phase == .idle { ResultCard(r: r) }
                Spacer(minLength: 0)
                LevelBar(level: m.level).frame(height: 8).padding(.horizontal, 40).opacity(m.phase == .recording ? 1 : 0.15)
                RecordButton()
                #if DEBUG
                DebugFeedButton()
                #endif
            }
            .padding()
            .navigationTitle("Una")
        }
    }

    @ViewBuilder private var statusLine: some View {
        switch m.phase {
        case .idle: Text("Tap to dictate").foregroundStyle(.secondary)
        case .recording: Text(String(format: "Recording  %.1fs", m.seconds)).monospacedDigit()
        case .uploading: HStack { ProgressView(); Text("Transcribing…") }
        case .done: Label("Copied to clipboard", systemImage: "checkmark.circle")
        case .failed(let e): Text(e).foregroundStyle(.red).multilineTextAlignment(.center)
        }
    }
}

struct RecordButton: View {
    @Environment(AppModel.self) private var m
    var body: some View {
        Button { Task { await m.toggleRecording() } } label: {
            ZStack {
                Circle().fill(m.phase == .recording ? Color.red : Color.accentColor).frame(width: 92, height: 92)
                Image(systemName: m.phase == .recording ? "stop.fill" : "mic.fill").font(.system(size: 34)).foregroundStyle(.white)
            }
        }
        .disabled(m.phase == .uploading)
        .accessibilityLabel(m.phase == .recording ? "Stop" : "Record")
    }
}

struct LevelBar: View {
    let level: Float
    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(.quaternary)
                Capsule().fill(Color.accentColor).frame(width: g.size.width * CGFloat(level))
                    .animation(.linear(duration: 0.08), value: level)
            }
        }
    }
}

struct ResultCard: View {
    @Environment(AppModel.self) private var m
    let r: DictationResponse
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(r.text).font(.title3).textSelection(.enabled)
            if r.rawText != r.text { Text(r.rawText).font(.footnote).foregroundStyle(.secondary) }
            HStack {
                Text("\(r.timings.totalMs) ms · \(r.asrModel)").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Copy", systemImage: "doc.on.doc") { m.copy(r.text) }.buttonStyle(.bordered)
            }
        }
        .padding().frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 14))
    }
}

#if DEBUG
/// DEBUG ONLY. Sends a WAV file from the Mac's disk (path in the `UnaDebugWAV` launch
/// argument) through the same encode → upload path, for the simulator demo.
struct DebugFeedButton: View {
    @Environment(AppModel.self) private var m
    var body: some View {
        if let p = UserDefaults.standard.string(forKey: "UnaDebugWAV") {
            Button("DEBUG: feed test WAV", systemImage: "ladybug") { Task { await m.debugFeed(path: p) } }
                .font(.footnote).buttonStyle(.bordered).tint(.orange)
        }
    }
}
#endif
