#if os(macOS)
import AppKit
import SwiftUI
import XCTest
@testable import MusterUI

@MainActor
final class ComposerReturnKeyTests: XCTestCase {
    func testShiftReturnReplacesSelectionWithNativeNewlineWithoutSending() {
        let editor = NSTextView()
        editor.isFieldEditor = true
        editor.string = "before selected after"
        editor.setSelectedRange(NSRange(location: 7, length: 8))
        var sends = 0
        XCTAssertEqual(ComposerReturnKey.handle(.shift, editor: editor) { sends += 1 }, .handled)
        XCTAssertEqual(editor.string, "before \n after")
        XCTAssertEqual(editor.selectedRange(), NSRange(location: 8, length: 0))
        XCTAssertEqual(sends, 0)
    }

    func testPlainReturnSendsOnceWithoutEditingAndLeavesOtherShortcutsNative() {
        let editor = NSTextView()
        editor.string = "a draft"
        editor.setSelectedRange(NSRange(location: 2, length: 3))
        var sends = 0
        XCTAssertEqual(ComposerReturnKey.handle([], editor: editor) { sends += 1 }, .handled)
        XCTAssertEqual(sends, 1)
        XCTAssertEqual(editor.string, "a draft")
        XCTAssertEqual(editor.selectedRange(), NSRange(location: 2, length: 3))
        for modifiers: EventModifiers in [.command, .option, .control, [.command, .shift]] {
            XCTAssertEqual(ComposerReturnKey.handle(modifiers, editor: editor) { sends += 1 }, .ignored)
        }
        XCTAssertEqual(sends, 1)
    }

    func testMarkedTextRetainsReturnForInputMethodConfirmation() {
        let editor = NSTextView()
        editor.setMarkedText("zhong", selectedRange: NSRange(location: 5, length: 0),
                             replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertTrue(editor.hasMarkedText())
        var sends = 0
        for modifiers: EventModifiers in [[], .shift] {
            XCTAssertEqual(ComposerReturnKey.handle(modifiers, editor: editor) { sends += 1 }, .ignored)
        }
        XCTAssertTrue(editor.hasMarkedText())
        XCTAssertEqual(editor.string, "zhong")
        XCTAssertEqual(sends, 0)
    }
}
#endif
