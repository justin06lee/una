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
    @Environment(AppModel.self) private var m
    #if DEBUG
    /// DEBUG ONLY: `-UnaDebugTab 1` opens History, `2` Servers (for screenshots).
    @State private var tab = UserDefaults.standard.integer(forKey: "UnaDebugTab")
    #else
    @State private var tab = 0
    #endif
    var body: some View {
        TabView(selection: $tab) {
            RecordView().tabItem { Label("Record", systemImage: "mic.fill") }.tag(0)
            HistoryView().tabItem { Label("History", systemImage: "clock") }.tag(1)
            ServersView().tabItem { Label("Servers", systemImage: "server.rack") }.tag(2)
        }
        .fullScreenCover(isPresented: .constant(!m.onboarded)) { OnboardingView() }
    }
}
