import XCTest
@testable import UnaKit

final class KeyboardLogicTests: XCTestCase {
    func testLetterRowsAndShiftCase() {
        let r = KeyboardLayout.rows(page: .letters, shift: .off)
        XCTAssertEqual(r.count, 4)
        XCTAssertEqual(r[0].count, 10); XCTAssertEqual(r[1].filter { !$0.isSpacer }.count, 9)
        XCTAssertEqual(r[2].count, 9)   // shift + 7 + delete
        XCTAssertEqual(r[0][0].kind, .char("q"))
        XCTAssertEqual(KeyboardLayout.rows(page: .letters, shift: .on)[0][0].kind, .char("Q"))
        XCTAssertEqual(KeyboardLayout.rows(page: .numbers, shift: .on)[0][0].kind, .char("1"))
    }
    func testBottomRowGlobeAndMic() {
        let b = KeyboardLayout.rows(page: .letters, shift: .off)[3].map(\.kind)
        XCTAssertEqual(b, [.page(.numbers), .globe, .mic, .space, .return])
        let noGlobe = KeyboardLayout.rows(page: .letters, shift: .off, needsGlobe: false)[3].map(\.kind)
        XCTAssertFalse(noGlobe.contains(.globe))
    }
    func testPageSwitching() {
        var s = KeyboardState()
        _ = s.tap(.page(.numbers), before: nil, now: 0); XCTAssertEqual(s.page, .numbers)
        let r = KeyboardLayout.rows(page: s.page, shift: s.shift)
        XCTAssertEqual(r[2].first?.kind, .page(.symbols))
        XCTAssertEqual(r[3].first?.kind, .page(.letters))
        _ = s.tap(.page(.symbols), before: nil, now: 0); XCTAssertEqual(s.page, .symbols)
        XCTAssertEqual(KeyboardLayout.rows(page: .symbols, shift: .off)[2].first?.kind, .page(.numbers))
    }
    func testShiftOneShotAndCapsLock() {
        var s = KeyboardState(); s.autoCapitalize = false
        _ = s.tap(.shift, before: "x", now: 10); XCTAssertEqual(s.shift, .on)
        XCTAssertEqual(s.tap(.char("A"), before: "x", now: 11), .insert("A")); XCTAssertEqual(s.shift, .off)
        // double tap -> caps, stays on across letters, single tap clears
        _ = s.tap(.shift, before: "x", now: 20); _ = s.tap(.shift, before: "x", now: 20.2)
        XCTAssertEqual(s.shift, .caps)
        _ = s.tap(.char("B"), before: "x", now: 21); XCTAssertEqual(s.shift, .caps)
        _ = s.tap(.shift, before: "x", now: 30); XCTAssertEqual(s.shift, .off)
    }
    func testShiftTapWhileOnTurnsOff() {
        var s = KeyboardState(); s.autoCapitalize = false
        _ = s.tap(.shift, before: "x", now: 10); _ = s.tap(.shift, before: "x", now: 12)
        XCTAssertEqual(s.shift, .off)
    }
    func testAutoCapitalise() {
        XCTAssertTrue(KeyboardState.atSentenceStart(nil))
        XCTAssertTrue(KeyboardState.atSentenceStart(""))
        XCTAssertTrue(KeyboardState.atSentenceStart("Hello. "))
        XCTAssertTrue(KeyboardState.atSentenceStart("What?  "))
        XCTAssertTrue(KeyboardState.atSentenceStart("a\n"))
        XCTAssertFalse(KeyboardState.atSentenceStart("Hello"))
        XCTAssertFalse(KeyboardState.atSentenceStart("Hello, "))
        XCTAssertFalse(KeyboardState.atSentenceStart("Hello."))   // no space yet
        var s = KeyboardState()
        s.updateAutoCap(before: "Done. "); XCTAssertEqual(s.shift, .on)
        _ = s.tap(.char("T"), before: "Done. ", now: 1); XCTAssertEqual(s.shift, .off)
        s.shift = .caps; s.updateAutoCap(before: "Done. "); XCTAssertEqual(s.shift, .caps)
        var off = KeyboardState(); off.autoCapitalize = false; off.updateAutoCap(before: nil); XCTAssertEqual(off.shift, .off)
    }
    func testTypingAfterSentenceShiftsAgain() {
        var s = KeyboardState(); s.shift = .off
        XCTAssertEqual(s.tap(.space, before: "Hi.", now: 1), .insert(" "))
        XCTAssertEqual(s.shift, .on)
    }
    func testDoubleSpacePeriod() {
        var s = KeyboardState()
        XCTAssertEqual(s.tap(.space, before: "hello", now: 1), .insert(" "))
        XCTAssertEqual(s.tap(.space, before: "hello ", now: 1.2), .periodShortcut)
        XCTAssertEqual(s.shift, .on)
        // too slow, or after punctuation or another space: plain space
        var t = KeyboardState(); _ = t.tap(.space, before: "hello", now: 1)
        XCTAssertEqual(t.tap(.space, before: "hello ", now: 2), .insert(" "))
        var u = KeyboardState(); _ = u.tap(.space, before: "hi,", now: 1)
        XCTAssertEqual(u.tap(.space, before: "hi, ", now: 1.1), .insert(" "))
        var v = KeyboardState(); _ = v.tap(.space, before: "a", now: 1); _ = v.tap(.space, before: "a ", now: 1.1)
        XCTAssertEqual(v.tap(.space, before: "a. ", now: 1.2), .insert(" "))
    }
    func testActions() {
        var s = KeyboardState()
        XCTAssertEqual(s.tap(.delete, before: "a", now: 0), .deleteBackward)
        XCTAssertEqual(s.tap(.globe, before: nil, now: 0), .nextKeyboard)
        XCTAssertEqual(s.tap(.mic, before: nil, now: 0), .mic)
        XCTAssertEqual(s.tap(.return, before: "a", now: 0), .returnKey); XCTAssertEqual(s.shift, .on)
    }
    func testMetrics() {
        let p = KeyboardLayout.metrics(width: 390, landscape: false)
        XCTAssertEqual(p.keyWidth, (390 - 6 - 54) / 10, accuracy: 0.001)
        XCTAssertLessThan(KeyboardLayout.metrics(width: 844, landscape: true).totalHeight, p.totalHeight)
        let rows = KeyboardLayout.rows(page: .letters, shift: .off)
        for row in rows {
            let f = KeyboardLayout.frames(row: row, width: 390, metrics: p)
            XCTAssertGreaterThanOrEqual(f.first!.x, 0); XCTAssertLessThanOrEqual(f.last!.x + f.last!.w, 390.001)
        }
        let f0 = KeyboardLayout.frames(row: rows[0], width: 390, metrics: p)
        XCTAssertEqual(f0[0].w, p.keyWidth, accuracy: 0.001)
    }
}
