import SwiftUI
import UnaKit

/// First run: point the app at the user's own una server. Nothing is preconfigured.
struct OnboardingView: View {
    @Environment(AppModel.self) private var m
    @State private var url = ""
    @State private var result: ProbeResult?
    @State private var testing = false

    private var entry: ServerEntry { ServerEntry(name: "My server", url: url) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("Una dictates through a server you run yourself. Your voice goes to that server and nowhere else: no account, no analytics, no third party.")
                }
                Section("Your server") {
                    TextField("http://192.168.1.10:8100", text: $url)
                        .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
                        .font(.body.monospaced())
                    Button {
                        Task { await test() }
                    } label: {
                        HStack { Label("Test connection", systemImage: "bolt.horizontal"); if testing { Spacer(); ProgressView() } }
                    }
                    .disabled(entry.baseURL == nil || url.isEmpty || testing)
                    if let r = result {
                        HStack(spacing: 8) {
                            StatusDot(ok: r.ok)
                            Text(r.ok ? "Connected · \(r.latencyMs) ms" : (r.error ?? "No answer")).font(.callout)
                        }
                    }
                }
                Section("Away from home") {
                    Text("To reach a home server from elsewhere, put the phone and the server on one private network such as Tailscale, then add the server's tailnet address in Servers. You can add several addresses; Una tries them all and uses the first that answers.")
                        .font(.callout)
                }
                Section("Keyboard permission") {
                    Text(FullAccessText.explanation).font(.callout)
                }
                Section {
                    Button("Done") { m.finishOnboarding(adding: entry) }
                        .disabled(entry.baseURL == nil || url.isEmpty)
                    Button("Skip for now") { m.finishOnboarding(adding: nil) }
                }
            }
            .navigationTitle("Welcome to Una")
        }
    }

    private func test() async {
        testing = true
        result = await EndpointRace().testAll([entry]).first
        testing = false
    }
}

enum FullAccessText {
    static let explanation = "Una's keyboard asks for Full Access so it can talk to the Una app, which does the recording and sends the audio to your server. Full Access also lets the keyboard use haptics. Audio goes only to the server you configured. Nothing is sent to us or anyone else, and the keyboard never reads what you type."
}
