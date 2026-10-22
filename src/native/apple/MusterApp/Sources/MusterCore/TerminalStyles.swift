import Foundation

/// One run of equally styled cells, as `TerminalLine.styles` carries them:
/// 11 bytes per run, big-endian (len u16, fg u32, bg u32, flags u8). The
/// encoding is owned by muster-terminal; a line with no runs is all default.
public struct TerminalStyleRun: Equatable {
    public struct Flags: OptionSet {
        public let rawValue: UInt8
        public init(rawValue: UInt8) { self.rawValue = rawValue }
        public static let bold = Flags(rawValue: 1)
        public static let italic = Flags(rawValue: 2)
        public static let underline = Flags(rawValue: 4)
        public static let inverse = Flags(rawValue: 8)
        public static let dim = Flags(rawValue: 16)
        public static let strike = Flags(rawValue: 32)
    }

    /// 0 = default, 0x01RRGGBB = rgb, 0x02000000 | index = palette.
    public enum Color: Equatable {
        case `default`
        case rgb(UInt8, UInt8, UInt8)
        case palette(UInt8)

        public init(raw: UInt32) {
            switch raw >> 24 {
            case 0x01:
                self = .rgb(UInt8((raw >> 16) & 0xff), UInt8((raw >> 8) & 0xff), UInt8(raw & 0xff))
            case 0x02:
                self = .palette(UInt8(raw & 0xff))
            default:
                self = .default
            }
        }
    }

    public var length: Int
    public var foreground: Color
    public var background: Color
    public var flags: Flags

    public static func decode(_ bytes: Data) -> [TerminalStyleRun] {
        var runs: [TerminalStyleRun] = []
        var index = bytes.startIndex
        while bytes.distance(from: index, to: bytes.endIndex) >= 11 {
            let b = bytes[index..<index + 11]
            let base = b.startIndex
            let len = Int(b[base]) << 8 | Int(b[base + 1])
            let fg = UInt32(b[base + 2]) << 24 | UInt32(b[base + 3]) << 16 | UInt32(b[base + 4]) << 8 | UInt32(b[base + 5])
            let bg = UInt32(b[base + 6]) << 24 | UInt32(b[base + 7]) << 16 | UInt32(b[base + 8]) << 8 | UInt32(b[base + 9])
            runs.append(TerminalStyleRun(
                length: len,
                foreground: Color(raw: fg),
                background: Color(raw: bg),
                flags: Flags(rawValue: b[base + 10])
            ))
            index += 11
        }
        return runs
    }

    /// The style of every cell of a line, `columns` wide; runs beyond the
    /// text are default.
    public static func perCell(_ bytes: Data, columns: Int) -> [TerminalStyleRun] {
        let runs = decode(bytes)
        guard !runs.isEmpty else { return [] }
        var cells: [TerminalStyleRun] = []
        cells.reserveCapacity(columns)
        for run in runs {
            for _ in 0..<run.length where cells.count < columns {
                cells.append(TerminalStyleRun(length: 1, foreground: run.foreground, background: run.background, flags: run.flags))
            }
        }
        return cells
    }
}

/// A key name the core maps to bytes (muster-terminal `key_bytes`): a named
/// key, or one held with ctrl / alt / shift such as "ctrl-q".
public struct TerminalKey: RawRepresentable, Hashable, Sendable {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let enter = Self(rawValue: "enter"), tab = Self(rawValue: "tab")
    public static let backspace = Self(rawValue: "backspace"), escape = Self(rawValue: "escape")
    public static let up = Self(rawValue: "up"), down = Self(rawValue: "down")
    public static let right = Self(rawValue: "right"), left = Self(rawValue: "left")
    public static let home = Self(rawValue: "home"), end = Self(rawValue: "end")
    public static let pageup = Self(rawValue: "pageup"), pagedown = Self(rawValue: "pagedown")
    public static let delete = Self(rawValue: "delete")
    public static let ctrlC = Self(rawValue: "ctrl-c"), ctrlD = Self(rawValue: "ctrl-d")
    public static let ctrlZ = Self(rawValue: "ctrl-z"), ctrlL = Self(rawValue: "ctrl-l")
    public static let ctrlA = Self(rawValue: "ctrl-a"), ctrlE = Self(rawValue: "ctrl-e")
    public static let ctrlU = Self(rawValue: "ctrl-u"), ctrlW = Self(rawValue: "ctrl-w")
    public static let ctrlR = Self(rawValue: "ctrl-r")

    /// A letter typed with Ctrl held: the core maps every letter.
    public static func control(_ letter: Character) -> TerminalKey? {
        guard letter.isASCII, letter.isLetter else { return nil }
        return TerminalKey(rawValue: "ctrl-\(letter.lowercased())")
    }
}
