import SwiftUI
import UnaKit

struct HistoryView: View {
    @Environment(AppModel.self) private var m

    var body: some View {
        NavigationStack {
            List {
                if !m.spooled.isEmpty {
                    Section("Waiting to send") {
                        ForEach(m.spooled) { item in
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(String(format: "%.1f s recording", Double(item.durationMs) / 1000))
                                    Text(item.createdAt, style: .relative).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button("Retry") { Task { await m.retry(item) } }.buttonStyle(.bordered)
                            }
                            .swipeActions { Button("Discard", role: .destructive) { m.discard(item) } }
                        }
                    }
                }
                Section("Recent") {
                    if let e = m.historyError { Text(e).foregroundStyle(.secondary) }
                    ForEach(m.history) { d in
                        Button { m.copy(d.text) } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(d.text.isEmpty ? "(nothing heard)" : d.text).foregroundStyle(.primary)
                                Text("\(d.createdAt.prefix(16).replacingOccurrences(of: "T", with: " ")) · \(d.durationMs / 100 % 10 == 0 ? "\(d.durationMs / 1000)" : String(format: "%.1f", Double(d.durationMs) / 1000)) s")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .navigationTitle("History")
            .refreshable { await m.loadHistory(); m.refreshSpool() }
            .task { await m.loadHistory(); m.refreshSpool() }
        }
    }
}
