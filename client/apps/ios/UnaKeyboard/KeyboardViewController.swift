import UIKit
import UnaKit

/// Una's keyboard: a plain UIKit typing keyboard (QWERTY, numbers, symbols) with a mic key that
/// is a placeholder until the dictation hand-off lands. It holds no audio and opens no sockets.
final class KeyboardViewController: UIInputViewController {
    private var state = KeyboardState()
    private let board = KeyboardView()
    private var heightConstraint: NSLayoutConstraint?
    private var haptic: UIImpactFeedbackGenerator?

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
        case .deleteBackward: p.deleteBackward()
        case .returnKey: p.insertText("\n")
        case .periodShortcut: p.deleteBackward(); p.insertText(". ")
        case .nextKeyboard: advanceToNextInputMode()
        case .mic, .none: break
        }
        board.set(page: state.page, shift: state.shift)
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
