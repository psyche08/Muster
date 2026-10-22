// Caret arithmetic for the keyboard's arrow, word and line keys, on plain
// text: lines are the text's own lines (not soft wraps), and offsets are
// UTF-16 indices (what UITextView and Compose selections use) that never
// split a surrogate pair. No platform calls, so the tests run it on Linux.
import Foundation

public enum TextEdits {
    private static func isLowSurrogate(_ units: [UInt16], _ index: Int) -> Bool {
        index >= 0 && index < units.count && units[index] >= 0xDC00 && units[index] <= 0xDFFF
    }

    public static func charLeft(_ value: String, _ pos: Int) -> Int {
        let units = Array(value.utf16)
        if pos <= 0 { return 0 }
        return pos >= 2 && isLowSurrogate(units, pos - 1) ? pos - 2 : pos - 1
    }

    public static func charRight(_ value: String, _ pos: Int) -> Int {
        let units = Array(value.utf16)
        if pos >= units.count { return units.count }
        return pos + 1 < units.count && isLowSurrogate(units, pos + 1) ? pos + 2 : pos + 1
    }

    public static func lineStart(_ value: String, _ pos: Int) -> Int {
        if pos <= 0 { return 0 }
        let units = Array(value.utf16)
        var index = min(pos, units.count) - 1
        while index >= 0 {
            if units[index] == 0x0A { return index + 1 }
            index -= 1
        }
        return 0
    }

    public static func lineEnd(_ value: String, _ pos: Int) -> Int {
        let units = Array(value.utf16)
        var index = max(pos, 0)
        while index < units.count {
            if units[index] == 0x0A { return index }
            index += 1
        }
        return units.count
    }

    /// The same column on the line above, or the start of the text from the first line.
    public static func lineUp(_ value: String, _ pos: Int) -> Int {
        let start = lineStart(value, pos)
        if start == 0 { return 0 }
        return min(lineStart(value, start - 1) + pos - start, start - 1)
    }

    /// The same column on the line below, or the end of the text from the last line.
    public static func lineDown(_ value: String, _ pos: Int) -> Int {
        let end = lineEnd(value, pos)
        let count = value.utf16.count
        if end == count { return count }
        return min(end + 1 + pos - lineStart(value, pos), lineEnd(value, end + 1))
    }

    private static let separators: Set<Character> = Set(" \t\n\r.,;:!?'\"`~@#$%^&*()[]{}<>/\\|+=-，。；：！？、（）【】《》「」“”‘’…—·")

    private static func isWord(_ units: [UInt16], _ index: Int) -> Bool {
        guard index >= 0 && index < units.count, let scalar = UnicodeScalar(units[index]) else { return true }
        return !separators.contains(Character(scalar))
    }

    /// Back over separators, then to the start of the word before them.
    public static func wordLeft(_ value: String, _ pos: Int) -> Int {
        let units = Array(value.utf16)
        var index = min(pos, units.count)
        while index > 0 && !isWord(units, index - 1) { index -= 1 }
        while index > 0 && isWord(units, index - 1) { index -= 1 }
        return index
    }

    /// On over separators, then to the end of the word after them.
    public static func wordRight(_ value: String, _ pos: Int) -> Int {
        let units = Array(value.utf16)
        var index = max(pos, 0)
        while index < units.count && !isWord(units, index) { index += 1 }
        while index < units.count && isWord(units, index) { index += 1 }
        return index
    }
}
