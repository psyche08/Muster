// The keyboard's MacBook layout and its caret arithmetic (the same checks
// as the HarmonyOS shell's tests/keyboard-layout.test.cjs).
import XCTest
@testable import MusterCore

final class KeyboardLayoutTests: XCTestCase {
    private func units(_ row: [MacKey]) -> Double { row.reduce(0) { $0 + $1.weight } }
    private func ids(_ row: [MacKey]) -> String { row.map(\.id).joined(separator: " ") }

    func testEveryRowSpansAMacBookRowSoTheColumnsLineUp() {
        for row in [macFunctionRow] + macRows {
            XCTAssertEqual(units(row), macRowUnits, accuracy: 1e-9, ids(row))
        }
    }

    func testTheRowsHoldTheMacBookKeysInOrder() {
        XCTAssertEqual(ids(macRows[0]), "` 1 2 3 4 5 6 7 8 9 0 - = delete")
        XCTAssertEqual(ids(macRows[1]), "tab q w e r t y u i o p [ ] \\")
        XCTAssertEqual(ids(macRows[2]), "caps a s d f g h j k l ; ' return")
        XCTAssertEqual(ids(macRows[3]), "shift z x c v b n m , . / shift")
        XCTAssertEqual(ids(macRows[4]), "fn control option command space command option arrows")
        XCTAssertEqual(ids(macFunctionRow), "escape f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12")
    }

    func testSymbolKeysCarryTheirShiftedCharacter() {
        XCTAssertEqual(macRows[0].filter { $0.kind == .symbol }.map(\.top).joined(), "~!@#$%^&*()_+")
        XCTAssertEqual(macRows[3].first { $0.id == "/" }?.top, "?")
    }

    func testChinesePunctuationFollowsMacOSPinyinQuotesAlternate() {
        let zh = ChinesePunctuation()
        XCTAssertEqual([",", ".", "\\", "$", "^", "<", "[", "1", "a"].map { zh.map($0) },
                       ["，", "。", "、", "￥", "……", "《", "【", "1", "a"])
        XCTAssertEqual(["\"", "\"", "'", "'"].map { zh.map($0) }, ["“", "”", "‘", "’"])
    }

    func testArrowsMoveByCharacterWithoutSplittingSurrogatePairs() {
        let text = "a😀b"
        XCTAssertEqual(TextEdits.charRight(text, 1), 3)
        XCTAssertEqual(TextEdits.charLeft(text, 3), 1)
        XCTAssertEqual(TextEdits.charLeft(text, 0), 0)
        XCTAssertEqual(TextEdits.charRight(text, 4), 4)
    }

    func testUpAndDownKeepTheColumnClampedToShorterLines() {
        let text = "hello\nhi\nworld"
        XCTAssertEqual(TextEdits.lineDown(text, 4), 8)
        XCTAssertEqual(TextEdits.lineDown(text, 8), 11)
        XCTAssertEqual(TextEdits.lineUp(text, 11), 8)
        XCTAssertEqual(TextEdits.lineUp(text, 13), 8)
        XCTAssertEqual(TextEdits.lineUp(text, 8), 2)
        XCTAssertEqual(TextEdits.lineUp(text, 2), 0)
        XCTAssertEqual(TextEdits.lineDown(text, 12), text.utf16.count)
    }

    func testLineStartAndEndAreTheTextLines() {
        let text = "\nab\ncd"
        XCTAssertEqual(TextEdits.lineStart(text, 0), 0)
        XCTAssertEqual(TextEdits.lineStart(text, 3), 1)
        XCTAssertEqual(TextEdits.lineEnd(text, 1), 3)
        XCTAssertEqual(TextEdits.lineEnd(text, 5), 6)
    }

    func testWordsSkipSeparatorsThenAWordChinesePunctuationIncluded() {
        let text = "git push, origin；你好 world"
        let length = text.utf16.count
        XCTAssertEqual(TextEdits.wordLeft(text, length), length - 5)
        XCTAssertEqual(TextEdits.wordLeft(text, 16), 10)
        XCTAssertEqual(TextEdits.wordRight(text, 8), 16)
        XCTAssertEqual(TextEdits.wordRight(text, 0), 3)
        XCTAssertEqual(TextEdits.wordLeft(text, 0), 0)
    }
}
