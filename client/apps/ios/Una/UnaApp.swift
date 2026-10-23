import SwiftUI
import UnaKit

@main
struct UnaApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView().environment(model).task { await model.start() }
        }
    }
}

struct RootView: View {
    var body: some View {
        TabView {
            RecordView().tabItem { Label("Record", systemImage: "mic.fill") }
            HistoryView().tabItem { Label("History", systemImage: "clock") }
            ServersView().tabItem { Label("Servers", systemImage: "server.rack") }
        }
    }
}
