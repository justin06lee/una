import Foundation

/// Pure keyboard logic (layout, shift, pages, auto-capitalise, double-space period). No UIKit,
/// so it is unit-tested with `swift test` and costs the extension nothing but a few structs.
public enum KeyboardPage: Equatable, Sendable { case letters, numbers, symbols }
public enum ShiftState: Equatable, Sendable { case off, on, caps }

public enum KeyKind: Equatable, Sendable {
    case char(String)
    case shift, delete, space, `return`, globe, mic
    case page(KeyboardPage)     // the 123 / ABC / #+= switch keys
}

public struct KeyDef: Equatable, Sendable {
    public var kind: KeyKind
    /// Width relative to a letter key (1.0). Special keys are wider; spacers are `isSpacer`.
    public var weight: Double
    public var isSpacer = false
    public init(_ kind: KeyKind, weight: Double = 1, isSpacer: Bool = false) {
        self.kind = kind; self.weight = weight; self.isSpacer = isSpacer
    }
}

public enum KeyboardAction: Equatable, Sendable {
    case insert(String)
    case deleteBackward
    case nextKeyboard
    case mic
    case returnKey
    /// Double-space shortcut: delete the typed space, insert ". ".
    case periodShortcut
    case none
}

public enum KeyboardLayout {
    public static let letterRows = ["qwertyuiop", "asdfghjkl", "zxcvbnm"]
    public static let numberRows = ["1234567890", "-/:;()$&@\"", ".,?!'"]
    public static let symbolRows = ["[]{}#%^*+=", "_\\|~<>€£¥•", ".,?!'"]

    public static func rows(page: KeyboardPage, shift: ShiftState, needsGlobe: Bool = true) -> [[KeyDef]] {
        func chars(_ s: String) -> [KeyDef] {
            s.map { KeyDef(.char(page == .letters && shift != .off ? String($0).uppercased() : String($0))) }
        }
        let base = page == .letters ? letterRows : page == .numbers ? numberRows : symbolRows
        var r: [[KeyDef]] = [chars(base[0])]
        if page == .letters {
            r.append([KeyDef(.char(""), weight: 0.5, isSpacer: true)] + chars(base[1]) + [KeyDef(.char(""), weight: 0.5, isSpacer: true)])
            r.append([KeyDef(.shift, weight: 1.5)] + chars(base[2]) + [KeyDef(.delete, weight: 1.5)])
        } else {
            r.append(chars(base[1]))
            let switchTo: KeyboardPage = page == .numbers ? .symbols : .numbers
            r.append([KeyDef(.page(switchTo), weight: 1.5)] + chars(base[2]).map { var k = $0; k.weight = 1.4; return k } + [KeyDef(.delete, weight: 1.5)])
        }
        var bottom = [KeyDef(.page(page == .letters ? .numbers : .letters), weight: 1.25)]
        if needsGlobe { bottom.append(KeyDef(.globe, weight: 1.25)) }
        bottom.append(KeyDef(.mic, weight: 1.25))
        bottom.append(KeyDef(.space, weight: 4))
        bottom.append(KeyDef(.return, weight: 2))
        r.append(bottom)
        return r
    }

    /// Letter-key width, row height and gaps for a keyboard `width` points wide. Portrait follows
    /// Apple's iPhone keyboard (6 pt key gap, 3 pt side margin, ~42 pt keys, 12 pt row gap);
    /// landscape is shorter with wider keys.
    public struct Metrics: Equatable, Sendable {
        public var keyWidth: Double, keyHeight: Double, hGap: Double, vGap: Double, margin: Double
        public var totalHeight: Double { 4 * keyHeight + 3 * vGap + 2 * topBottom }
        public var topBottom: Double { vGap - 1 }
    }
    public static func metrics(width: Double, landscape: Bool) -> Metrics {
        let margin = landscape ? 4.0 : 3.0, gap = landscape ? 5.0 : 6.0
        return Metrics(keyWidth: (width - 2 * margin - 9 * gap) / 10,
                       keyHeight: landscape ? 33 : 42, hGap: gap, vGap: landscape ? 8 : 12, margin: margin)
    }

    /// X ranges (origin, width) for one row inside `width` points, honouring weights.
    public static func frames(row: [KeyDef], width: Double, metrics m: Metrics) -> [(x: Double, w: Double)] {
        let totalWeight = row.reduce(0) { $0 + $1.weight }
        let gaps = Double(row.count - 1) * m.hGap
        let isLetterish = row.contains { if case .char = $0.kind { return true } else { return false } }
        // Letter rows keep the letter key width; other rows share the remaining width by weight.
        let unit = isLetterish && !row.contains { $0.kind == .space }
            ? m.keyWidth : (width - 2 * m.margin - gaps) / totalWeight
        let used = totalWeight * unit + gaps
        var x = (width - used) / 2
        return row.map { k in
            let w = k.weight * unit
            defer { x += w + m.hGap }
            return (x, w)
        }
    }
}

public struct KeyboardState: Equatable, Sendable {
    public var page: KeyboardPage = .letters
    public var shift: ShiftState = .off
    public var autoCapitalize = true
    var lastShiftTap: TimeInterval = -10
    var lastSpace: TimeInterval = -10

    public init() {}

    public static let doubleTapWindow: TimeInterval = 0.4

    /// Handle a tap; `before` is `documentContextBeforeInput`, `now` a monotonic time.
    public mutating func tap(_ kind: KeyKind, before: String?, now: TimeInterval) -> KeyboardAction {
        switch kind {
        case .char(let c):
            let wasOn = shift == .on
            if wasOn { shift = .off }
            lastSpace = -10
            let act = KeyboardAction.insert(c)
            if page == .letters, !wasOn { updateAutoCap(before: (before ?? "") + c) }
            return act
        case .shift:
            if shift == .caps { shift = .off }
            else if shift == .on, now - lastShiftTap < Self.doubleTapWindow { shift = .caps }
            else if shift == .on { shift = .off }
            else if now - lastShiftTap < Self.doubleTapWindow { shift = .caps }
            else { shift = .on }
            lastShiftTap = now
            return .none
        case .delete:
            lastSpace = -10
            return .deleteBackward
        case .space:
            defer { lastSpace = now }
            if now - lastSpace < Self.doubleTapWindow, Self.canPeriodShortcut(before: before) {
                lastSpace = -10
                if page == .letters { updateAutoCap(before: (before ?? "") + ". ") }
                return .periodShortcut
            }
            if page == .letters { updateAutoCap(before: (before ?? "") + " ") }
            return .insert(" ")
        case .return:
            lastSpace = -10
            if page == .letters, autoCapitalize { shift = .on }
            return .returnKey
        case .page(let p): page = p; return .none
        case .globe: return .nextKeyboard
        case .mic: return .mic
        }
    }

    /// True when the text before the cursor is "<non-space char> " (a single trailing space).
    static func canPeriodShortcut(before: String?) -> Bool {
        guard let b = before, b.count >= 2, b.hasSuffix(" ") else { return false }
        let prev = b[b.index(b.endIndex, offsetBy: -2)]
        return !prev.isWhitespace && !".!?,;:".contains(prev)
    }

    /// Sentence-start check: nothing before the cursor, or the last sentence ended with . ! ? plus
    /// whitespace, or a newline. Never turns off caps lock.
    public mutating func updateAutoCap(before: String?) {
        guard autoCapitalize, shift != .caps, page == .letters else { return }
        shift = Self.atSentenceStart(before) ? .on : .off
    }

    public static func atSentenceStart(_ before: String?) -> Bool {
        guard let b = before, !b.isEmpty else { return true }
        if b.hasSuffix("\n") { return true }
        var s = Substring(b)
        var sawSpace = false
        while let l = s.last, l == " " { s.removeLast(); sawSpace = true }
        guard sawSpace, let l = s.last else { return false }
        return ".!?".contains(l)
    }
}
