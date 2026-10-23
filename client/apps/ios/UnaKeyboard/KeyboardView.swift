import UIKit
import UnaKit

/// Draws and hit-tests the keys. One plain UIView per key, drawn with layers and SF Symbols;
/// no images, no SwiftUI. Touches are handled here so a thumb sliding between keys works.
final class KeyboardView: UIView, UIInputViewAudioFeedback {
    var enableInputClicksWhenVisible: Bool { true }

    var onTap: ((KeyKind) -> Void)?
    var onFeedback: (() -> Void)?
    var onGlobeTouch: ((UIView, UIEvent?) -> Void)?
    var needsGlobe = true { didSet { if oldValue != needsGlobe { rebuild() } } }
    var landscape = false { didSet { if oldValue != landscape { setNeedsLayout() } } }
    var returnTitle = "return" { didSet { updateLabels() } }
    var returnHighlighted = false { didSet { updateLabels() } }
    /// What the mic key shows: idle, listening (red), or transcribing.
    var micUI = MicRemote.UI.idle { didSet { if oldValue != micUI { updateLabels() } } }
    private let notice = NoticeView()

    private var page = KeyboardPage.letters
    private var shift = ShiftState.off
    private var rows: [[KeyDef]] = []
    private var keyViews: [[KeyView]] = []
    private var active: [UITouch: KeyView] = [:]
    private let popup = PopupView()
    private var repeatTimer: Timer?
    private var repeatCount = 0

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        backgroundColor = Palette.background
        addSubview(popup); popup.isHidden = true
        addSubview(notice); notice.isHidden = true
        rebuild()
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (v: KeyboardView, _) in v.restyleAll() }
    }
    required init?(coder: NSCoder) { fatalError() }

    func set(page: KeyboardPage, shift: ShiftState) {
        let pageChanged = page != self.page
        self.page = page; self.shift = shift
        if pageChanged { rebuild() } else { updateLabels() }
    }

    private func rebuild() {
        keyViews.flatMap { $0 }.forEach { $0.removeFromSuperview() }
        rows = KeyboardLayout.rows(page: page, shift: shift, needsGlobe: needsGlobe)
        keyViews = rows.map { row in row.map { def in
            let v = KeyView(def: def); if !def.isSpacer { insertSubview(v, belowSubview: popup) }; return v
        } }
        updateLabels(); setNeedsLayout()
    }

    /// Letter case changes only the titles, so shift never rebuilds views.
    private func updateLabels() {
        let fresh = KeyboardLayout.rows(page: page, shift: shift, needsGlobe: needsGlobe)
        for (r, row) in fresh.enumerated() where r < keyViews.count {
            for (i, def) in row.enumerated() where i < keyViews[r].count {
                keyViews[r][i].def = def
                keyViews[r][i].apply(shift: shift, returnTitle: returnTitle, returnHighlighted: returnHighlighted, mic: micUI)
            }
        }
        rows = fresh
    }

    /// A message that covers the keys: the Full Access explanation, the "open Una" prompt, errors.
    func showNotice(title: String, body: String, primary: String?, secondary: String, onPrimary: @escaping () -> Void, onSecondary: @escaping () -> Void) {
        notice.set(title: title, body: body, primary: primary, secondary: secondary, onPrimary: onPrimary, onSecondary: onSecondary)
        notice.isHidden = false; bringSubviewToFront(notice); setNeedsLayout()
    }
    func hideNotice() { notice.isHidden = true }

    override func layoutSubviews() {
        super.layoutSubviews()
        notice.frame = bounds
        let m = KeyboardLayout.metrics(width: Double(bounds.width), landscape: landscape)
        var y = m.topBottom
        for (r, row) in rows.enumerated() {
            let fr = KeyboardLayout.frames(row: row, width: Double(bounds.width), metrics: m)
            for (i, f) in fr.enumerated() where !row[i].isSpacer {
                keyViews[r][i].frame = CGRect(x: f.x, y: y, width: f.w, height: m.keyHeight)
            }
            y += m.keyHeight + m.vGap
        }
    }

    private func restyleAll() {
        backgroundColor = Palette.background
        keyViews.flatMap { $0 }.forEach { $0.restyle() }
    }

    // MARK: touches

    /// The key under a point, tolerating the gaps between keys by choosing the nearest.
    private func key(at p: CGPoint) -> KeyView? {
        var best: (KeyView, CGFloat)?
        for k in keyViews.flatMap({ $0 }) where k.superview != nil {
            guard k.frame.insetBy(dx: -3, dy: -5).contains(p) else { continue }
            let d = hypot(p.x - k.frame.midX, p.y - k.frame.midY)
            if best == nil || d < best!.1 { best = (k, d) }
        }
        return best?.0
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        for t in touches {
            guard let k = key(at: t.location(in: self)) else { continue }
            active[t] = k; press(k, event: event)
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        for t in touches {
            guard let cur = active[t], case .char = cur.def.kind else { continue }
            let next = key(at: t.location(in: self))
            if next !== cur {   // slide between letter keys
                release(cur, fire: false)
                if let next, case .char = next.def.kind { active[t] = next; press(next, event: event) } else { active[t] = nil }
            }
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        for t in touches { if let k = active.removeValue(forKey: t) { release(k, fire: true) } }
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        for t in touches { if let k = active.removeValue(forKey: t) { release(k, fire: false) } }
    }

    private func press(_ k: KeyView, event: UIEvent?) {
        k.isPressed = true
        onFeedback?()
        switch k.def.kind {
        case .char: popup.show(over: k, title: k.title, in: self)
        case .delete: onTap?(.delete); startRepeat()
        case .globe: onGlobeTouch?(k, event)
        default: break
        }
    }

    private func release(_ k: KeyView, fire: Bool) {
        k.isPressed = false; popup.isHidden = true
        switch k.def.kind {
        case .delete: stopRepeat()
        case .globe: break
        default: if fire { onTap?(k.def.kind) }
        }
    }

    // Press-and-hold delete: after 0.4 s repeat every 0.1 s, then every 0.05 s after ten deletes.
    private func startRepeat() {
        stopRepeat()
        repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: false) { [weak self] _ in MainActor.assumeIsolated { self?.tickRepeat() } }
    }
    private func tickRepeat() {
        onTap?(.delete); repeatCount += 1
        repeatTimer = Timer.scheduledTimer(withTimeInterval: repeatCount > 10 ? 0.05 : 0.1, repeats: false) { [weak self] _ in MainActor.assumeIsolated { self?.tickRepeat() } }
    }
    private func stopRepeat() { repeatTimer?.invalidate(); repeatTimer = nil; repeatCount = 0 }
}

// MARK: colours (Apple's keyboard greys, light and dark)

enum Palette {
    static func dyn(_ light: UInt32, _ dark: UInt32) -> UIColor {
        UIColor { $0.userInterfaceStyle == .dark ? hex(dark) : hex(light) }
    }
    static func hex(_ v: UInt32) -> UIColor {
        UIColor(red: CGFloat((v >> 16) & 255) / 255, green: CGFloat((v >> 8) & 255) / 255, blue: CGFloat(v & 255) / 255, alpha: 1)
    }
    static let background = dyn(0xD1D3D9, 0x2B2B2D)
    static let letterKey = dyn(0xFFFFFF, 0x6B6B6D)
    static let specialKey = dyn(0xAEB3BE, 0x454547)
    static let shadow = dyn(0x898A8D, 0x000000)
    static let text = dyn(0x000000, 0xFFFFFF)
    static let accent = UIColor.systemBlue
}

/// One key: a rounded rectangle with a 1 pt shadow line, and a label or SF Symbol.
final class KeyView: UIView {
    var def: KeyDef
    var isPressed = false { didSet { restyle() } }
    private let label = UILabel()
    private let icon = UIImageView()
    private var highlighted = false
    private var shifted = false
    private(set) var title = ""

    init(def: KeyDef) {
        self.def = def
        super.init(frame: .zero)
        layer.cornerRadius = 5; layer.shadowOffset = CGSize(width: 0, height: 1)
        layer.shadowOpacity = 1; layer.shadowRadius = 0
        label.textAlignment = .center; label.adjustsFontSizeToFitWidth = true; label.minimumScaleFactor = 0.6
        icon.contentMode = .center
        addSubview(label); addSubview(icon)
        isUserInteractionEnabled = false
        apply(shift: .off, returnTitle: "return", returnHighlighted: false)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (v: KeyView, _) in v.restyle() }
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layoutSubviews() {
        super.layoutSubviews()
        label.frame = bounds.insetBy(dx: 2, dy: 0); icon.frame = bounds
        layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: 5).cgPath
    }

    private var micUI = MicRemote.UI.idle

    func apply(shift: ShiftState, returnTitle: String, returnHighlighted: Bool, mic: MicRemote.UI = .idle) {
        highlighted = false; shifted = shift != .off; micUI = mic
        var symbol: String?
        switch def.kind {
        case .char(let c): title = c; label.text = c; label.font = .systemFont(ofSize: 25)
        case .shift: symbol = shift == .caps ? "capslock.fill" : shift == .on ? "shift.fill" : "shift"
        case .delete: symbol = "delete.left"
        case .globe: symbol = "globe"
        case .mic:
            switch mic {
            case .recording: symbol = "mic.fill"
            case .transcribing: symbol = "ellipsis"
            case .starting: symbol = "mic.fill"
            case .needsAccess, .needsArm, .failed, .idle: symbol = "mic"
            }
        case .space: label.text = "space"; label.font = .systemFont(ofSize: 16)
        case .return: label.text = returnTitle; label.font = .systemFont(ofSize: 16); highlighted = returnHighlighted
        case .page(let p): label.text = p == .letters ? "ABC" : p == .numbers ? "123" : "#+="; label.font = .systemFont(ofSize: 16)
        }
        if let symbol {
            icon.image = UIImage(systemName: symbol, withConfiguration: UIImage.SymbolConfiguration(pointSize: 18, weight: .regular))
            icon.isHidden = false; label.isHidden = true
        } else { icon.isHidden = true; label.isHidden = false }
        restyle()
    }

    func restyle() {
        var isLetter = def.kind == .space
        if case .char = def.kind { isLetter = true }
        var fill = isLetter ? Palette.letterKey : Palette.specialKey
        if def.kind == .shift, shifted { fill = Palette.letterKey }
        var text = Palette.text
        if def.kind == .return, highlighted { fill = Palette.accent; text = .white }
        if def.kind == .mic {
            switch micUI {
            case .recording: fill = .systemRed; text = .white
            case .transcribing: fill = Palette.accent; text = .white
            case .starting: fill = .systemRed.withAlphaComponent(0.55); text = .white
            default: break
            }
            updatePulse(micUI == .recording)
        }
        if isPressed { fill = isLetter ? Palette.specialKey : Palette.letterKey }
        layer.backgroundColor = fill.resolvedColor(with: traitCollection).cgColor
        layer.shadowColor = Palette.shadow.resolvedColor(with: traitCollection).cgColor
        label.textColor = text; icon.tintColor = text
    }
}

extension KeyView {
    /// A slow fade on the red key so "listening" reads at a glance without a timer.
    fileprivate func updatePulse(_ on: Bool) {
        if on, layer.animation(forKey: "pulse") == nil {
            let a = CABasicAnimation(keyPath: "opacity"); a.fromValue = 1; a.toValue = 0.65
            a.duration = 0.8; a.autoreverses = true; a.repeatCount = .infinity
            layer.add(a, forKey: "pulse")
        } else if !on { layer.removeAnimation(forKey: "pulse") }
    }
}

/// A card laid over the keys with a title, a few lines of text and one or two buttons.
final class NoticeView: UIView {
    private let title = UILabel(), body = UILabel()
    private let primary = UIButton(type: .system), secondary = UIButton(type: .system)
    private var onPrimary: (() -> Void)?, onSecondary: (() -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = Palette.background
        title.font = .boldSystemFont(ofSize: 16); title.textAlignment = .center; title.textColor = Palette.text
        body.font = .systemFont(ofSize: 13); body.textAlignment = .center; body.numberOfLines = 0
        body.textColor = .secondaryLabel; body.adjustsFontSizeToFitWidth = true; body.minimumScaleFactor = 0.7
        var p = UIButton.Configuration.filled(); p.cornerStyle = .medium; p.title = "Open"
        primary.configuration = p
        var s = UIButton.Configuration.plain(); s.title = "Not now"
        secondary.configuration = s
        primary.addAction(UIAction { [weak self] _ in self?.onPrimary?() }, for: .touchUpInside)
        secondary.addAction(UIAction { [weak self] _ in self?.onSecondary?() }, for: .touchUpInside)
        [title, body, primary, secondary].forEach(addSubview)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (v: NoticeView, _) in v.backgroundColor = Palette.background }
    }
    required init?(coder: NSCoder) { fatalError() }

    func set(title t: String, body b: String, primary p: String?, secondary s: String, onPrimary op: @escaping () -> Void, onSecondary os: @escaping () -> Void) {
        title.text = t; body.text = b
        primary.configuration?.title = p; primary.isHidden = p == nil
        secondary.configuration?.title = s
        onPrimary = op; onSecondary = os
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let w = bounds.width - 32, compact = bounds.height < 190
        var y: CGFloat = compact ? 8 : 14
        title.frame = CGRect(x: 16, y: y, width: w, height: 22); y += 24
        let buttonsH: CGFloat = 38
        let bodyH = max(30, bounds.height - y - buttonsH - 12)
        body.frame = CGRect(x: 16, y: y, width: w, height: bodyH); y = bounds.height - buttonsH - 8
        if primary.isHidden {
            secondary.frame = CGRect(x: bounds.midX - 70, y: y, width: 140, height: buttonsH)
        } else {
            secondary.frame = CGRect(x: 16, y: y, width: w / 2 - 6, height: buttonsH)
            primary.frame = CGRect(x: 16 + w / 2 + 6, y: y, width: w / 2 - 6, height: buttonsH)
        }
    }
}

/// The balloon that shows the letter under the thumb.
final class PopupView: UIView {
    private let label = UILabel()
    override init(frame: CGRect) {
        super.init(frame: frame)
        layer.cornerRadius = 8; layer.shadowOpacity = 0.35; layer.shadowRadius = 4; layer.shadowOffset = CGSize(width: 0, height: 1)
        label.textAlignment = .center; label.font = .systemFont(ofSize: 34)
        addSubview(label); isUserInteractionEnabled = false
    }
    required init?(coder: NSCoder) { fatalError() }
    func show(over key: UIView, title: String, in host: UIView) {
        label.text = title
        let w = key.frame.width + 22, h = key.frame.height + 28
        let x = max(2, min(host.bounds.width - w - 2, key.frame.midX - w / 2))
        frame = CGRect(x: x, y: max(0, key.frame.maxY - h - key.frame.height * 0.2), width: w, height: h)
        label.frame = CGRect(x: 0, y: 4, width: w, height: key.frame.height)
        backgroundColor = Palette.letterKey.resolvedColor(with: traitCollection)
        label.textColor = Palette.text
        isHidden = false; host.bringSubviewToFront(self)
    }
}
