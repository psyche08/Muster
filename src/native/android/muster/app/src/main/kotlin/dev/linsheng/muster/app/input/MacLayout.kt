package dev.linsheng.muster.app.input

// The keyboard's wide layout follows a MacBook (US ANSI): the same keys in
// the same rows with the same relative widths, so every row spans 14.5 key
// units and the columns line up. Pure data, so the tests check it.

enum class MacKeyKind {
    /** Types its letter; the cap shows it in upper case, as a MacBook's does. */
    LETTER,
    /** Types `bottom`, or `top` with Shift. */
    SYMBOL,
    /** Escape and F1–F12: a centred label. */
    NAMED,
    /** Modifiers, editing keys and the space bar: small corner legends. */
    ACTION,
    /** The inverted-T arrow cluster, three units wide. */
    ARROWS,
}

/**
 * One key: the letter, the unshifted character, or the action's name; its
 * width in key units; its legends; `end` puts the legends on the right edge
 * (keys right of the letters, and left-hand modifiers by the space bar).
 */
data class MacKey(val id: String, val kind: MacKeyKind, val weight: Double, val top: String, val bottom: String, val end: Boolean)

const val MAC_ROW_UNITS = 14.5

private fun letters(text: String): List<MacKey> = text.map { MacKey(it.toString(), MacKeyKind.LETTER, 1.0, "", it.toString(), false) }

/** Pairs of unshifted then shifted characters. */
private fun symbols(pairs: String): List<MacKey> =
    (0 until pairs.length - 1 step 2).map { MacKey(pairs[it].toString(), MacKeyKind.SYMBOL, 1.0, pairs[it + 1].toString(), pairs[it].toString(), false) }

private fun action(id: String, weight: Double, top: String, bottom: String, end: Boolean) = MacKey(id, MacKeyKind.ACTION, weight, top, bottom, end)

/** Esc and F1–F12; a MacBook's Touch ID slot is shared out among the F-keys. */
val MAC_FUNCTION_ROW: List<MacKey> = listOf(action("escape", 1.5, "", "esc", false)) +
    (1..12).map { MacKey("f$it", MacKeyKind.NAMED, 13.0 / 12.0, "", "F$it", false) }

val MAC_ROWS: List<List<MacKey>> = listOf(
    symbols("`~1!2@3#4$5%6^7&8*9(0)-_=+") + action("delete", 1.5, "", "delete", true),
    listOf(action("tab", 1.5, "", "tab", false)) + letters("qwertyuiop") + symbols("[{]}\\|"),
    listOf(action("caps", 1.75, "", "caps lock", false)) + letters("asdfghjkl") + symbols(";:'\"") +
        action("return", 1.75, "", "return", true),
    listOf(action("shift", 2.25, "⇧", "shift", false)) + letters("zxcvbnm") + symbols(",<.>/?") +
        action("shift", 2.25, "⇧", "shift", true),
    listOf(
        action("fn", 1.0, "fn", "", false),
        action("control", 1.0, "⌃", "control", true),
        action("option", 1.0, "⌥", "option", true),
        action("command", 1.25, "⌘", "command", true),
        action("space", 5.0, "", "", false),
        action("command", 1.25, "⌘", "command", false),
        action("option", 1.0, "⌥", "option", false),
        MacKey("arrows", MacKeyKind.ARROWS, 3.0, "", "", false),
    ),
)

/** What the ASCII punctuation types in Chinese, as macOS Pinyin does. */
private val CHINESE = mapOf(
    "`" to "·", "~" to "～", "!" to "！", "$" to "￥", "^" to "……", "(" to "（", ")" to "）", "_" to "——",
    "[" to "【", "]" to "】", "{" to "「", "}" to "」", "\\" to "、", ";" to "；", ":" to "：",
    "," to "，", "." to "。", "<" to "《", ">" to "》", "?" to "？",
)

/** Chinese punctuation for one keyboard; quotation marks alternate between opening and closing. */
class ChinesePunctuation {
    private var single = false
    private var double = false

    fun map(char: String): String {
        if (char == "'") { single = !single; return if (single) "‘" else "’" }
        if (char == "\"") { double = !double; return if (double) "“" else "”" }
        return CHINESE[char] ?: char
    }
}
