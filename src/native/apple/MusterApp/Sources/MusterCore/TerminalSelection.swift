import Foundation

/// A cell of the terminal grid: its row (`TerminalLine.index`) and column
/// (a Character index into the row's text, the way the grid is drawn).
public struct TerminalCell: Equatable, Comparable, Sendable {
    public var row: Int
    public var col: Int

    public init(row: Int, col: Int) {
        self.row = row
        self.col = col
    }

    public static func < (a: TerminalCell, b: TerminalCell) -> Bool {
        (a.row, a.col) < (b.row, b.col)
    }
}

/// A text selection on the grid, from the cell it started at to the cell
/// it was dragged to, in either direction. It is display state: copying
/// reads the rows the core sent, nothing goes back to the core.
public struct TerminalSelection: Equatable, Sendable {
    public var anchor: TerminalCell
    public var head: TerminalCell

    public init(at cell: TerminalCell) {
        anchor = cell
        head = cell
    }

    public var start: TerminalCell { min(anchor, head) }
    public var end: TerminalCell { max(anchor, head) }
    public var isEmpty: Bool { anchor == head }

    /// The selected columns of `row`, nil outside the selection; rows
    /// between the first and the last are selected to `columns`.
    public func columns(inRow row: Int, columns: Int) -> Range<Int>? {
        guard row >= start.row, row <= end.row else { return nil }
        let from = row == start.row ? start.col : 0
        let to = row == end.row ? end.col + 1 : columns
        return from < to ? from..<to : nil
    }

    /// The selected text: a row the core did not send is empty, each row
    /// loses its trailing spaces, rows are joined by newlines.
    public func text(in lines: [Muster_V1_TerminalLine], columns: Int) -> String {
        var rows: [Int: [Character]] = [:]
        for line in lines { rows[Int(line.index)] = Array(line.text) }
        return (start.row...end.row).map { row -> String in
            let characters = rows[row] ?? []
            guard let range = self.columns(inRow: row, columns: max(columns, characters.count)) else {
                return ""
            }
            let lower = min(range.lowerBound, characters.count)
            let upper = min(range.upperBound, characters.count)
            var text = String(characters[lower..<upper])
            while text.last == " " { text.removeLast() }
            return text
        }
        .joined(separator: "\n")
    }
}
