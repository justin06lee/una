import SwiftUI
import UnaKit

@main
struct UnaApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView().environment(model).task { await model.start() }
                .onOpenURL { url in
                    // una://arm, from the keyboard's "Open Una" prompt: start the audio session while we're frontmost.
                    if url.host == "arm" { Task { await model.armKeyboard() } }
                }
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
            #if DEBUG
            DebugFieldView().tabItem { Label("Field", systemImage: "keyboard") }.tag(3)
            #endif
            ServersView().tabItem { Label("Servers", systemImage: "server.rack") }.tag(2)
        }
        .fullScreenCover(isPresented: .constant(!m.onboarded)) { OnboardingView() }
    }
}

#if DEBUG
/// DEBUG ONLY: an autofocused field (`-UnaDebugTab 3`) to try the keyboard in the simulator.
struct DebugFieldView: View {
    @State private var text = ""
    @FocusState private var focus: Bool
    var body: some View {
        VStack { TextField("Type here", text: $text, axis: .vertical).focused($focus).padding().textFieldStyle(.roundedBorder); Spacer() }
            .onAppear { focus = true }
    }
}
#endif
