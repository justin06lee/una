import SwiftUI
import UnaKit

struct ServersView: View {
    @Environment(AppModel.self) private var m
    @State private var adding = false

    var body: some View {
        @Bindable var m = m
        NavigationStack {
            List {
                Section {
                    ForEach($m.servers) { $s in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                TextField("Name", text: $s.name).font(.headline)
                                if let p = m.probes[s.id] { StatusDot(ok: p.ok) }
                            }
                            TextField("http://host:8100", text: $s.url)
                                .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
                                .font(.subheadline.monospaced())
                            if let p = m.probes[s.id] {
                                Text(p.ok ? detail(p) : (p.error ?? "no answer")).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                    .onDelete { m.servers.remove(atOffsets: $0) }
                    .onMove { m.servers.move(fromOffsets: $0, toOffset: $1) }
                } footer: { Text("All are tried at once; the first to answer is used for the next five minutes.") }
                Section {
                    Button("Add server", systemImage: "plus") { m.servers.append(ServerEntry(name: "New server", url: "http://")) }
                    Button { Task { await m.testAll() } } label: {
                        HStack { Label("Test all", systemImage: "bolt.horizontal"); if m.testing { Spacer(); ProgressView() } }
                    }
                }
                Section("Privacy") {
                    Text("Audio goes only to the servers listed above. No analytics, no accounts, nothing sent anywhere else.").font(.callout)
                    Text(FullAccessText.explanation).font(.callout)
                }
            }
            .navigationTitle("Servers")
            .toolbar { EditButton() }
        }
    }

    private func detail(_ p: ProbeResult) -> String {
        let h = p.health!
        var parts = ["\(p.latencyMs) ms", h.asrModel ?? "no model"]
        if h.trainingActive { parts.append("training running") }
        if let g = h.gpu?.name { parts.append(g) }
        return parts.joined(separator: " · ")
    }
}

struct StatusDot: View {
    let ok: Bool
    var body: some View {
        Image(systemName: ok ? "checkmark.circle.fill" : "xmark.circle.fill").foregroundStyle(ok ? .green : .red)
    }
}
