// Copying from the terminal grid: the selected cells of the rows the core
// sent, in reading order whichever way the selection was dragged.
import XCTest
@testable import MusterCore

final class TerminalSelectionTests: XCTestCase {
    private func line(_ index: UInt32, _ text: String) -> Muster_V1_TerminalLine {
        Muster_V1_TerminalLine.with { $0.index = index; $0.text = text }
    }

    private var lines: [Muster_V1_TerminalLine] {
        [line(0, "$ ls -la   "), line(1, "total 8"), line(3, "README.md  src")]
    }

    func testOneRowSelectsItsColumns() {
        var selection = TerminalSelection(at: TerminalCell(row: 0, col: 2))
        selection.head = TerminalCell(row: 0, col: 3)
        XCTAssertEqual(selection.text(in: lines, columns: 80), "ls")
    }

    func testRowsAreJoinedTrimmedAndMissingRowsAreEmpty() {
        var selection = TerminalSelection(at: TerminalCell(row: 3, col: 5))
        selection.head = TerminalCell(row: 0, col: 2)  // dragged upwards
        XCTAssertEqual(selection.start, TerminalCell(row: 0, col: 2))
        XCTAssertEqual(selection.text(in: lines, columns: 80), "ls -la\ntotal 8\n\nREADME")
    }

    func testColumnsBeyondTheTextAndHighlightRanges() {
        var selection = TerminalSelection(at: TerminalCell(row: 1, col: 40))
        selection.head = TerminalCell(row: 1, col: 60)
        XCTAssertEqual(selection.text(in: lines, columns: 80), "")
        XCTAssertEqual(selection.columns(inRow: 1, columns: 80), 40..<61)
        XCTAssertNil(selection.columns(inRow: 2, columns: 80))
        selection.head = TerminalCell(row: 2, col: 0)
        XCTAssertEqual(selection.columns(inRow: 1, columns: 80), 40..<80)
        XCTAssertEqual(selection.columns(inRow: 2, columns: 80), 0..<1)
        XCTAssertFalse(selection.isEmpty)
        XCTAssertTrue(TerminalSelection(at: TerminalCell(row: 0, col: 0)).isEmpty)
    }

    func testWideTextIsCutByCharacter() {
        var selection = TerminalSelection(at: TerminalCell(row: 0, col: 0))
        selection.head = TerminalCell(row: 0, col: 2)
        XCTAssertEqual(selection.text(in: [line(0, "日本語です")], columns: 80), "日本語")
    }
}
