// The keyboard's wide layout follows a MacBook (US ANSI): the same keys in
// the same rows with the same relative widths, so every row spans 14.5 key
// units and the columns line up. Pure data, so the tests check it.
import Foundation

public enum MacKeyKind: Equatable {
    /// Types its letter; the cap shows it in upper case, as a MacBook's does.
    case letter
    /// Types `bottom`, or `top` with Shift.
    case symbol
    /// Escape and F1–F12: a centred label.
    case named
    /// Modifiers, editing keys and the space bar: small corner legends.
    case action
    /// The inverted-T arrow cluster, three units wide.
    case arrows
}

public struct MacKey: Equatable {
    /// The letter, the unshifted character, or the action's name.
    public let id: String
    public let kind: MacKeyKind
    /// Width in key units.
    public let weight: Double
    public let top: String
    public let bottom: String
    /// Legends sit on the right edge: keys right of the letters, and left-hand modifiers by the space bar.
    public let end: Bool
}

public let macRowUnits = 14.5

private func key(_ id: String, _ kind: MacKeyKind, _ weight: Double, _ top: String, _ bottom: String, _ end: Bool) -> MacKey {
    MacKey(id: id, kind: kind, weight: weight, top: top, bottom: bottom, end: end)
}

private func letters(_ text: String) -> [MacKey] {
    text.map { key(String($0), .letter, 1, "", String($0), false) }
}

/// Pairs of unshifted then shifted characters.
private func symbols(_ pairs: String) -> [MacKey] {
    let chars = Array(pairs)
    return stride(from: 0, to: chars.count - 1, by: 2).map {
        key(String(chars[$0]), .symbol, 1, String(chars[$0 + 1]), String(chars[$0]), false)
    }
}

private func action(_ id: String, _ weight: Double, _ top: String, _ bottom: String, _ end: Bool) -> MacKey {
    key(id, .action, weight, top, bottom, end)
}

/// Esc and F1–F12; a MacBook's Touch ID slot is shared out among the F-keys.
public let macFunctionRow: [MacKey] = [action("escape", 1.5, "", "esc", false)]
    + (1...12).map { key("f\($0)", .named, 13.0 / 12.0, "", "F\($0)", false) }

public let macRows: [[MacKey]] = [
    symbols("`~1!2@3#4$5%6^7&8*9(0)-_=+") + [action("delete", 1.5, "", "delete", true)],
    [action("tab", 1.5, "", "tab", false)] + letters("qwertyuiop") + symbols("[{]}\\|"),
    [action("caps", 1.75, "", "caps lock", false)] + letters("asdfghjkl") + symbols(";:'\"")
        + [action("return", 1.75, "", "return", true)],
    [action("shift", 2.25, "⇧", "shift", false)] + letters("zxcvbnm") + symbols(",<.>/?")
        + [action("shift", 2.25, "⇧", "shift", true)],
    [
        action("fn", 1, "fn", "", false),
        action("control", 1, "⌃", "control", true),
        action("option", 1, "⌥", "option", true),
        action("command", 1.25, "⌘", "command", true),
        action("space", 5, "", "", false),
        action("command", 1.25, "⌘", "command", false),
        action("option", 1, "⌥", "option", false),
        key("arrows", .arrows, 3, "", "", false),
    ],
]

/// What the ASCII punctuation types in Chinese, as macOS Pinyin does.
private let chinesePunctuation: [String: String] = [
    "`": "·", "~": "～", "!": "！", "$": "￥", "^": "……", "(": "（", ")": "）", "_": "——",
    "[": "【", "]": "】", "{": "「", "}": "」", "\\": "、", ";": "；", ":": "：",
    ",": "，", ".": "。", "<": "《", ">": "》", "?": "？",
]

/// Chinese punctuation for one keyboard; quotation marks alternate between opening and closing.
public final class ChinesePunctuation {
    private var single = false
    private var double = false

    public init() {}

    public func map(_ char: String) -> String {
        if char == "'" { single.toggle(); return single ? "‘" : "’" }
        if char == "\"" { double.toggle(); return double ? "“" : "”" }
        return chinesePunctuation[char] ?? char
    }
}
