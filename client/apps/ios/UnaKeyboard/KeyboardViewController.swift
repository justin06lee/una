import UIKit
import UnaKit

/// Una's keyboard: a plain UIKit typing keyboard (QWERTY, numbers, symbols) whose mic key is a
/// remote control for the Una app. The app records and uploads; this process only writes a small
/// command file into the App Group, listens for the app's Darwin notification, and inserts the
/// transcript. It holds no audio and opens no sockets.
final class KeyboardViewController: UIInputViewController {
    private var state = KeyboardState()
    private let board = KeyboardView()
    private var heightConstraint: NSLayoutConstraint?
    private var haptic: UIImpactFeedbackGenerator?

    private var remote = MicRemote()
    private let group = Handoff.appGroup()
    private lazy var store: SharedStore? = group.flatMap(Handoff.directory(group:)).map(SharedStore.init(directory:))
    private let notifier = DarwinNotifier()
    private var statusToken: AnyObject?
    private var deadlineTimer: Timer?
    private var now: TimeInterval { Date().timeIntervalSince1970 }

    override func viewDidLoad() {
        super.viewDidLoad()
        board.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(board)
        NSLayoutConstraint.activate([
            board.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            board.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            board.topAnchor.constraint(equalTo: view.topAnchor),
            board.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
        board.needsGlobe = needsInputModeSwitchKey
        board.onTap = { [weak self] kind in self?.handle(kind) }
        board.onGlobeTouch = { [weak self] v, e in if let e { self?.handleInputModeList(from: v, with: e) } }
        board.onFeedback = { [weak self] in self?.feedback() }
        syncTraits()
        refresh()
        if let group {
            statusToken = notifier.observe(Handoff.notificationName(group: group, .status)) { [weak self] in
                Task { @MainActor in self?.statusChanged() }
            }
        }
    }

    override func viewWillLayoutSubviews() {
        super.viewWillLayoutSubviews()
        let landscape = traitCollection.verticalSizeClass == .compact
        let h = KeyboardLayout.metrics(width: Double(view.bounds.width), landscape: landscape).totalHeight
        if let c = heightConstraint { c.constant = h } else {
            let c = view.heightAnchor.constraint(equalToConstant: h)
            c.priority = .required - 1; c.isActive = true; heightConstraint = c
        }
        board.landscape = landscape
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        board.needsGlobe = needsInputModeSwitchKey
        syncTraits(); refresh()
        // Haptics need Full Access; without it the generator would only log a warning.
        haptic = hasFullAccess ? UIImpactFeedbackGenerator(style: .light) : nil
        haptic?.prepare()
        statusChanged()   // a transcript that arrived while the keyboard was away
    }

    override func textDidChange(_ textInput: UITextInput?) { syncTraits(); refresh() }

    private func syncTraits() {
        let p = textDocumentProxy
        state.autoCapitalize = p.autocapitalizationType != UITextAutocapitalizationType.none
        board.returnTitle = Self.returnTitle(p.returnKeyType ?? .default)
        board.returnHighlighted = ![.default, .next, nil].contains(p.returnKeyType)
        if p.autocapitalizationType == .allCharacters { state.shift = .caps }
    }

    private func refresh() {
        state.updateAutoCap(before: textDocumentProxy.documentContextBeforeInput)
        board.set(page: state.page, shift: state.shift)
    }

    private func handle(_ kind: KeyKind) {
        let p = textDocumentProxy
        switch state.tap(kind, before: p.documentContextBeforeInput, now: ProcessInfo.processInfo.systemUptime) {
        case .insert(let s): p.insertText(s)
        case .deleteBackward:
            let fx = remote.discardIfRecording()      // delete while recording throws the take away
            if fx.isEmpty { p.deleteBackward() } else { run(fx) }
        case .returnKey: p.insertText("\n")
        case .periodShortcut: p.deleteBackward(); p.insertText(". ")
        case .nextKeyboard: advanceToNextInputMode()
        case .mic:
            let id = UUID().uuidString.lowercased()
            if store == nil { remote.fail(Handoff.noGroupNotice) }
            else { run(remote.tapMic(fullAccess: hasFullAccess, status: store?.readStatus(), now: now, newId: id)) }
        case .none: break
        }
        board.set(page: state.page, shift: state.shift)
    }

    // MARK: dictation remote

    private func statusChanged() {
        guard let store else { return }
        run(remote.statusChanged(store.readStatus(), now: now))
    }

    private func tick() { run(remote.tick(now: now)) }

    /// Turns the state machine's effects into container writes, notifications and typed text.
    private func run(_ effects: [MicRemote.Effect]) {
        for e in effects {
            switch e {
            case .send(let action, let id):
                guard let store, let group else { break }
                do {
                    try store.write(Command(requestId: id, action: action, at: now))
                    notifier.post(Handoff.notificationName(group: group, .command))
                } catch { remote.fail("Couldn't reach the Una app. Is Full Access on?") }
            case .insert(let text):
                textDocumentProxy.insertText(text)
                refresh()
            case .openApp:
                openUnaApp()
            }
        }
        renderMic()
    }

    private func renderMic() {
        board.micUI = remote.ui
        switch remote.ui {
        case .needsAccess:
            board.showNotice(title: FullAccessText.keyboardTitle, body: FullAccessText.explanation + "\n\n" + FullAccessText.keyboardSteps,
                             primary: nil, secondary: "OK", onPrimary: {}, onSecondary: { [weak self] in self?.dismissNotice() })
        case .needsArm:
            board.showNotice(title: FullAccessText.armTitle, body: FullAccessText.armBody, primary: "Open Una", secondary: "Not now",
                             onPrimary: { [weak self] in guard let self else { return }; self.run(self.remote.openAppTapped()) },
                             onSecondary: { [weak self] in self?.dismissNotice() })
        case .failed(let m):
            board.showNotice(title: "Dictation didn't work", body: m, primary: nil, secondary: "OK", onPrimary: {}, onSecondary: { [weak self] in self?.dismissNotice() })
        default:
            board.hideNotice()
        }
        deadlineTimer?.invalidate(); deadlineTimer = nil
        if let d = remote.deadline {
            deadlineTimer = Timer.scheduledTimer(withTimeInterval: max(0.1, d - now), repeats: false) { [weak self] _ in
                MainActor.assumeIsolated { self?.tick() }
            }
        }
    }

    private func dismissNotice() { remote.dismiss(); renderMic() }

    /// Keyboards have no `UIApplication.shared`; the responder chain reaches the host's
    /// application object, and `open(_:)` on it is public API. iOS 26 only allows it with Full Access.
    private func openUnaApp() {
        guard let url = URL(string: "una://arm") else { return }
        var r: UIResponder? = self
        while let x = r {
            if let app = x as? UIApplication { app.open(url, options: [:], completionHandler: nil); return }
            r = x.next
        }
    }

    private func feedback() { haptic?.impactOccurred(); UIDevice.current.playInputClick() }

    private static func returnTitle(_ t: UIReturnKeyType) -> String {
        switch t {
        case .go: "go"
        case .google, .search, .yahoo: "search"
        case .send: "send"
        case .done: "done"
        case .next: "next"
        case .join: "join"
        case .route: "route"
        case .continue: "continue"
        case .emergencyCall: "call"
        default: "return"
        }
    }
}
