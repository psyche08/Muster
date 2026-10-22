package dev.linsheng.muster.app.input

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Backspace
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.KeyboardHide
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.outlined.ArrowUpward
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// The in-app keyboard: QWERTY with pinyin Chinese and English, shown instead
// of the system keyboard below the screen. Wide windows get a MacBook's
// layout (MacLayout.kt); narrow ones a phone layout with number and symbol
// pages. Fields hand it a KeyboardTarget; it never reads their contents.
// The same behaviour as the HarmonyOS shell's Keyboard.ets.

/**
 * Keyboard choices shared by every field: the system keyboard instead of
 * the in-app one, the same choice for terminals, and the last language. In
 * text fields the in-app keyboard is off until it has been verified on a
 * device, so typing never depends on it; a terminal uses it by default,
 * since the system keyboard has no Esc, F-keys, Ctrl or Alt.
 */
object KeyboardPrefs {
    var systemKeyboard by mutableStateOf(true)
        private set
    /** The system keyboard in a terminal instead of the in-app one. */
    var terminalSystemKeyboard by mutableStateOf(false)
        private set
    var chinese by mutableStateOf(true)
        private set
    /** Height (dp) of the in-app keyboard while it is up: the half-fold layout keeps the hinge clear of it. */
    var keyboardHeight by mutableStateOf(0.0)
    private var store: android.content.SharedPreferences? = null

    fun open(context: Context) {
        val prefs = context.getSharedPreferences("muster-display", Context.MODE_PRIVATE)
        store = prefs
        systemKeyboard = prefs.getBoolean("systemKeyboard", true)
        terminalSystemKeyboard = prefs.getBoolean("terminalSystemKeyboard", false)
        chinese = prefs.getBoolean("keyboardChinese", true)
    }

    fun useSystem(on: Boolean) { systemKeyboard = on; store?.edit()?.putBoolean("systemKeyboard", on)?.apply() }
    fun useSystemInTerminal(on: Boolean) { terminalSystemKeyboard = on; store?.edit()?.putBoolean("terminalSystemKeyboard", on)?.apply() }
    fun useChinese(on: Boolean) { chinese = on; store?.edit()?.putBoolean("keyboardChinese", on)?.apply() }
}

/** The dictionary loads once, off the first keyboard, and one composer keeps what the user picks. */
object Pinyin {
    var composer: PinyinComposer? = null
        private set
    private var loading = false

    suspend fun load(context: Context): PinyinComposer? {
        composer?.let { return it }
        if (loading) return null
        loading = true
        return try {
            val dict = withContext(Dispatchers.IO) { context.assets.open("pinyin.txt").use { PinyinDict.load(it) } }
            PinyinComposer(dict).also { composer = it }
        } catch (_: Exception) {
            null
        } finally {
            loading = false
        }
    }
}

/** Caret moves, deletions and clipboard edits a text field takes from the MacBook layout. */
enum class KeyboardEdit {
    LEFT, RIGHT, UP, DOWN, WORD_LEFT, WORD_RIGHT, LINE_START, LINE_END, START, END,
    DELETE_FORWARD, DELETE_WORD_BACK, DELETE_TO_LINE_START, DELETE_TO_LINE_END, SELECT_ALL, COPY, CUT, TAB,
}

/** What a key does to the field it is bound to. */
class KeyboardTarget(
    val insert: (String) -> Unit,
    val backspace: () -> Unit,
    val enter: () -> Unit,
    val hide: () -> Unit,
    /** Named keys for a terminal (escape, f5, ctrl-c, alt-b…); fields without it get no Esc or F-keys. */
    val key: ((String) -> Unit)? = null,
    /** Text fields: arrows, word and line moves, deletions and the clipboard; `extend` (Shift) moves only the selection's head. */
    val edit: ((KeyboardEdit, Boolean) -> Unit)? = null,
)

/** The field (or terminal) the keyboard currently types into; the root shows the keyboard while one is set. */
object KeyboardHost {
    class Session(val target: KeyboardTarget, val enterLabel: String, val compact: Boolean)

    var session by mutableStateOf<Session?>(null)
        private set

    fun show(target: KeyboardTarget, enterLabel: String, compact: Boolean) {
        session = Session(target, enterLabel, compact)
    }

    /** Hides the keyboard if `target` still owns it (a later field may have taken over). */
    fun hide(target: KeyboardTarget) {
        if (session?.target === target) session = null
    }
}

/**
 * Edits a text field's value for the keyboard: inserts at the caret or over
 * the selection and moves the caret, keeping the anchor while Shift-arrows
 * move the head. `copyText` is the shell's clipboard.
 */
class FieldEditor(
    private val read: () -> TextFieldValue,
    private val write: (TextFieldValue) -> Unit,
    private val submit: () -> Unit,
    private val close: () -> Unit,
    private val multiline: Boolean,
    private val copyText: (String) -> Unit,
) {
    private fun replace(value: TextFieldValue, a: Int, b: Int, text: String) {
        val next = value.text.substring(0, a) + text + value.text.substring(b)
        write(TextFieldValue(next, TextRange(a + text.length)))
    }

    private fun moved(value: String, edit: KeyboardEdit, pos: Int): Int = when (edit) {
        KeyboardEdit.LEFT -> TextEdits.charLeft(value, pos)
        KeyboardEdit.RIGHT -> TextEdits.charRight(value, pos)
        KeyboardEdit.UP -> TextEdits.lineUp(value, pos)
        KeyboardEdit.DOWN -> TextEdits.lineDown(value, pos)
        KeyboardEdit.WORD_LEFT -> TextEdits.wordLeft(value, pos)
        KeyboardEdit.WORD_RIGHT -> TextEdits.wordRight(value, pos)
        KeyboardEdit.LINE_START -> TextEdits.lineStart(value, pos)
        KeyboardEdit.LINE_END -> TextEdits.lineEnd(value, pos)
        KeyboardEdit.START -> 0
        else -> value.length
    }

    private fun apply(edit: KeyboardEdit, extend: Boolean) {
        val value = read()
        val text = value.text
        val anchor = value.selection.start.coerceIn(0, text.length)
        val head = value.selection.end.coerceIn(0, text.length)
        val a = minOf(anchor, head)
        val b = maxOf(anchor, head)
        when (edit) {
            KeyboardEdit.SELECT_ALL -> { write(value.copy(selection = TextRange(0, text.length))); return }
            KeyboardEdit.COPY -> { if (a < b) copyText(text.substring(a, b)); return }
            KeyboardEdit.CUT -> { if (a < b) { copyText(text.substring(a, b)); replace(value, a, b, "") }; return }
            KeyboardEdit.TAB -> { if (multiline) replace(value, a, b, "\t"); return }
            KeyboardEdit.DELETE_FORWARD -> { replace(value, a, if (a < b) b else TextEdits.charRight(text, a), ""); return }
            KeyboardEdit.DELETE_WORD_BACK -> { replace(value, if (a < b) a else TextEdits.wordLeft(text, a), b, ""); return }
            KeyboardEdit.DELETE_TO_LINE_START -> {
                val start = TextEdits.lineStart(text, a)
                replace(value, if (a < b) a else if (start == a) TextEdits.charLeft(text, a) else start, b, "")
                return
            }
            KeyboardEdit.DELETE_TO_LINE_END -> {
                val end = TextEdits.lineEnd(text, b)
                replace(value, a, if (a < b) b else if (end == b) TextEdits.charRight(text, b) else end, "")
                return
            }
            else -> {}
        }
        if (extend) {
            write(value.copy(selection = TextRange(anchor, moved(text, edit, head))))
            return
        }
        // A selection collapses to its own edge first, as on a Mac.
        if (a < b && edit == KeyboardEdit.LEFT) { write(value.copy(selection = TextRange(a))); return }
        if (a < b && edit == KeyboardEdit.RIGHT) { write(value.copy(selection = TextRange(b))); return }
        val backward = edit == KeyboardEdit.LEFT || edit == KeyboardEdit.UP || edit == KeyboardEdit.WORD_LEFT || edit == KeyboardEdit.LINE_START
        write(value.copy(selection = TextRange(moved(text, edit, if (backward) a else b))))
    }

    fun target(): KeyboardTarget = KeyboardTarget(
        insert = { text ->
            val value = read()
            replace(value, value.selection.min.coerceIn(0, value.text.length), value.selection.max.coerceIn(0, value.text.length), text)
        },
        backspace = {
            val value = read()
            val a = value.selection.min.coerceIn(0, value.text.length)
            val b = value.selection.max.coerceIn(0, value.text.length)
            if (!(a == b && a == 0)) replace(value, if (a == b) TextEdits.charLeft(value.text, a) else a, b, "")
        },
        enter = { submit() },
        hide = { close() },
        edit = { edit, extend -> apply(edit, extend) },
    )
}

private enum class Page { LETTERS, NUMBERS, SYMBOLS }

private val LETTER_ROWS = listOf(
    listOf("q", "w", "e", "r", "t", "y", "u", "i", "o", "p"),
    listOf("a", "s", "d", "f", "g", "h", "j", "k", "l"),
    listOf("z", "x", "c", "v", "b", "n", "m"),
)
private val NUMBERS = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
/** The terminal strip: what the terminal's own key bar had, then the function keys. */
private val TERMINAL_KEYS: List<Pair<String, String>> = listOf(
    "esc" to "escape", "tab" to "tab", "←" to "left", "↓" to "down", "↑" to "up", "→" to "right",
    "home" to "home", "end" to "end", "pgup" to "pageup", "pgdn" to "pagedown",
) + (1..12).map { "F$it" to "f$it" }
private val ZH_NUMBER_SYMBOLS = listOf(listOf("-", "/", "：", "；", "（", "）", "¥", "@", "“", "”"), listOf("。", "，", "、", "？", "！", ".", "‘", "’"))
private val EN_NUMBER_SYMBOLS = listOf(listOf("-", "/", ":", ";", "(", ")", "$", "&", "@", "\""), listOf(".", ",", "?", "!", "'", "%", "*", "#"))
private val ZH_MORE_SYMBOLS = listOf(listOf("【", "】", "{", "}", "#", "%", "^", "*", "+", "="), listOf("—", "…", "《", "》", "~", "·", "|", "\\"))
private val EN_MORE_SYMBOLS = listOf(listOf("[", "]", "{", "}", "#", "%", "^", "*", "+", "="), listOf("_", "\\", "|", "~", "<", ">", "`", "$"))
private val KEY_HEIGHT = 42.dp
private val COMPACT_KEY_HEIGHT = 34.dp
private const val DOUBLE_TAP_MS = 350L
/** From this width (dp) the keyboard takes a MacBook's layout. */
val MAC_MIN_WIDTH = 560.dp
private val FN_ARROWS = mapOf("left" to "home", "right" to "end", "up" to "pageup", "down" to "pagedown")
/** Control with a letter in a text field: the macOS (Emacs) text bindings. */
private val CONTROL_EDITS = mapOf(
    "a" to KeyboardEdit.LINE_START, "e" to KeyboardEdit.LINE_END, "b" to KeyboardEdit.LEFT, "f" to KeyboardEdit.RIGHT,
    "p" to KeyboardEdit.UP, "n" to KeyboardEdit.DOWN, "d" to KeyboardEdit.DELETE_FORWARD, "k" to KeyboardEdit.DELETE_TO_LINE_END,
)

/** The modifiers that apply to one key press. */
private class Held(val shift: Boolean, val ctrl: Boolean, val alt: Boolean, val cmd: Boolean, val fn: Boolean)

/** The keyboard's state and key logic; the composables below only draw it. */
private class KeyboardState(val target: KeyboardTarget, val enterLabel: String) {
    var page by mutableStateOf(Page.LETTERS)
    /** Shift, Ctrl, Alt (option), Command and fn: 0 off, 1 for the next key, 2 locked. */
    var shift by mutableIntStateOf(0)
    var ctrl by mutableIntStateOf(0)
    var alt by mutableIntStateOf(0)
    var cmd by mutableIntStateOf(0)
    var fn by mutableIntStateOf(0)
    var capsLock by mutableStateOf(false)
    var composing by mutableStateOf("")
    var candidates by mutableStateOf(listOf<PinyinCandidate>())
    var ready by mutableStateOf(false)
    private val lastTap = HashMap<String, Long>()
    private val punctuation = ChinesePunctuation()

    val chinese: Boolean get() = KeyboardPrefs.chinese && ready
    /** A terminal: the keyboard adds Esc, F1–F12, Ctrl and Alt. */
    val raw: Boolean get() = target.key != null

    fun refresh() {
        val composer = Pinyin.composer
        composing = if (composer != null && composer.active) composer.display() else ""
        candidates = if (composer != null && composer.composing.isNotEmpty()) composer.candidates() else emptyList()
    }

    /** Commits pending pinyin with its best reading before a space, digit or symbol. */
    fun flush() {
        val composer = Pinyin.composer
        if (composer != null && composer.active) {
            target.insert(composer.commitFirst())
            refresh()
        }
    }

    /** With Ctrl or Alt held, a key goes to the terminal as a named combination. */
    private fun modified(base: String): Boolean {
        val send = target.key ?: return false
        if (ctrl == 0 && alt == 0) return false
        flush()
        send("${if (ctrl > 0) "ctrl-" else ""}${if (alt > 0) "alt-" else ""}$base")
        if (ctrl == 1) ctrl = 0
        if (alt == 1) alt = 0
        return true
    }

    fun letter(key: String) {
        if (modified(key)) return
        val upper = shift > 0
        if (shift == 1) shift = 0
        val composer = Pinyin.composer
        if (chinese && !upper && composer != null) {
            composer.type(key)
            refresh()
            return
        }
        flush()
        target.insert(if (upper) key.uppercase() else key)
    }

    fun symbol(text: String) {
        if (modified(text)) return
        flush()
        target.insert(text)
    }

    /** Esc and F1–F12, for terminals only. */
    fun named(name: String) {
        if (modified(name)) return
        flush()
        target.key?.invoke(name)
    }

    fun pick(candidate: PinyinCandidate) {
        val composer = Pinyin.composer ?: return
        composer.pick(candidate)?.let { target.insert(it) }
        refresh()
    }

    fun backspace() {
        if (modified("backspace")) return
        val composer = Pinyin.composer
        if (composer != null && composer.active) {
            composer.backspace()
            refresh()
            return
        }
        target.backspace()
    }

    fun space() {
        if (modified("space")) return
        val composer = Pinyin.composer
        if (composer != null && composer.active) { flush(); return }
        target.insert(" ")
    }

    fun enter() {
        if (modified("enter")) return
        val composer = Pinyin.composer
        // Enter while composing types the letters themselves, as other pinyin keyboards do.
        if (composer != null && composer.active) {
            target.insert(composer.commitRaw())
            refresh()
            return
        }
        target.enter()
    }

    /** Tap: the next key only; double tap: locked; tap again: off. */
    private fun cycle(state: Int, id: String): Int {
        val now = System.currentTimeMillis()
        val last = lastTap[id] ?: 0L
        lastTap[id] = now
        if (state == 1 && now - last < DOUBLE_TAP_MS) return 2
        return if (state == 0) 1 else 0
    }

    fun toggleShift() { shift = cycle(shift, "shift") }
    fun toggleCtrl() { ctrl = cycle(ctrl, "ctrl") }
    fun toggleAlt() { alt = cycle(alt, "alt") }
    fun toggleCmd() { cmd = cycle(cmd, "cmd") }
    fun toggleFn() { fn = cycle(fn, "fn") }

    /** The modifiers for this key press; one-shot ones are released. */
    private fun take(): Held {
        val held = Held(shift > 0, ctrl > 0, alt > 0, cmd > 0, fn > 0)
        if (shift == 1) shift = 0
        if (ctrl == 1) ctrl = 0
        if (alt == 1) alt = 0
        if (cmd == 1) cmd = 0
        if (fn == 1) fn = 0
        return held
    }

    fun macPress(key: MacKey) {
        when (key.kind) {
            MacKeyKind.LETTER -> macCharacter(key.id, key.id.uppercase(), true)
            MacKeyKind.SYMBOL -> macCharacter(key.bottom, key.top, false)
            else -> macAction(key.id)
        }
    }

    private fun macCharacter(plain: String, shifted: String, letter: Boolean) {
        val held = take()
        val upper = held.shift || (letter && capsLock)
        val char = if (upper) shifted else plain
        val send = target.key
        if (held.cmd) {
            if (letter) command(plain)
            return
        }
        if (send != null && (held.ctrl || held.alt)) {
            flush()
            send("${if (held.ctrl) "ctrl-" else ""}${if (held.alt) "alt-" else ""}$char")
            return
        }
        if (send == null && held.ctrl) {
            if (letter) controlEdit(plain)
            return
        }
        val composer = Pinyin.composer
        if (composer != null && chinese && !upper) {
            // Letters compose pinyin; an apostrophe between them separates syllables.
            if (letter || (plain == "'" && composer.composing.isNotEmpty())) {
                composer.type(plain)
                refresh()
                return
            }
        }
        flush()
        target.insert(if (!letter && chinese) punctuation.map(char) else char)
    }

    /** Command with a letter in a text field: select all, copy, cut. Terminals have no such shortcuts. */
    private fun command(letter: String) {
        val edit = target.edit ?: return
        flush()
        when (letter) {
            "a" -> edit(KeyboardEdit.SELECT_ALL, false)
            "c" -> edit(KeyboardEdit.COPY, false)
            "x" -> edit(KeyboardEdit.CUT, false)
        }
    }

    private fun controlEdit(letter: String) {
        val edit = target.edit
        val action = CONTROL_EDITS[letter]
        flush()
        if (letter == "h") target.backspace() else if (edit != null && action != null) edit(action, false)
    }

    fun macAction(id: String) {
        when (id) {
            "shift" -> { toggleShift(); return }
            "control" -> { toggleCtrl(); return }
            "option" -> { toggleAlt(); return }
            "command" -> { toggleCmd(); return }
            "fn" -> { toggleFn(); return }
            // A tap switches language, as Caps Lock does with macOS Pinyin; holding it locks capitals.
            "caps" -> { toggleLanguage(); return }
        }
        val held = take()
        val composer = Pinyin.composer
        if (id == "escape" && composer != null && composer.active) {
            composer.clear()
            refresh()
            return
        }
        val send = target.key
        if (send != null) terminalAction(id, held, send) else fieldAction(id, held)
    }

    private fun terminalAction(id: String, held: Held, send: (String) -> Unit) {
        val mods = "${if (held.ctrl) "ctrl-" else ""}${if (held.alt) "alt-" else ""}"
        val shift = if (held.shift) "shift-" else ""
        if (id == "delete" && !held.fn && !held.cmd && mods.isEmpty()) { backspace(); return }
        if (id == "return" && mods.isEmpty()) { enter(); return }
        if (id == "space" && mods.isEmpty()) { space(); return }
        flush()
        when {
            id == "delete" -> send(if (held.fn) "${mods}delete" else if (held.cmd) "ctrl-u" else "${mods}backspace")
            id == "return" -> send("${mods}enter")
            id == "space" -> send("${mods}space")
            id in FN_ARROWS -> send(terminalArrow(id, held, mods))
            else -> send("$mods$shift$id")
        }
    }

    /** Arrows as macOS terminals send them: fn for Home/End/Page Up/Down, ⌘ for the line, ⌥ for words. */
    private fun terminalArrow(id: String, held: Held, mods: String): String {
        val shift = if (held.shift) "shift-" else ""
        val horizontal = id == "left" || id == "right"
        if (held.fn) return "$mods$shift${FN_ARROWS[id]}"
        if (held.cmd && horizontal) return if (id == "left") "ctrl-a" else "ctrl-e"
        if (held.alt && !held.ctrl && !held.shift && horizontal) return if (id == "left") "alt-b" else "alt-f"
        return "$mods$shift$id"
    }

    private fun fieldAction(id: String, held: Held) {
        val edit = target.edit
        val composing = Pinyin.composer?.active == true
        if (id == "delete") {
            when {
                composing || edit == null -> backspace()
                held.fn -> edit(KeyboardEdit.DELETE_FORWARD, false)
                held.cmd -> edit(KeyboardEdit.DELETE_TO_LINE_START, false)
                held.alt -> edit(KeyboardEdit.DELETE_WORD_BACK, false)
                else -> backspace()
            }
            return
        }
        if (id == "return") { enter(); return }
        if (id == "space") { space(); return }
        if (id == "escape") { target.hide(); return }
        if (edit == null) return
        flush()
        if (id == "tab") { edit(KeyboardEdit.TAB, false); return }
        fieldArrow(id, held)?.let { edit(it, held.shift) }
    }

    private fun fieldArrow(id: String, held: Held): KeyboardEdit? {
        fun pick(left: KeyboardEdit, right: KeyboardEdit, up: KeyboardEdit, down: KeyboardEdit): KeyboardEdit? = when (id) {
            "left" -> left; "right" -> right; "up" -> up; "down" -> down; else -> null
        }
        if (held.fn || held.cmd) return pick(KeyboardEdit.LINE_START, KeyboardEdit.LINE_END, KeyboardEdit.START, KeyboardEdit.END)
        if (held.alt) return pick(KeyboardEdit.WORD_LEFT, KeyboardEdit.WORD_RIGHT, KeyboardEdit.LINE_START, KeyboardEdit.LINE_END)
        return pick(KeyboardEdit.LEFT, KeyboardEdit.RIGHT, KeyboardEdit.UP, KeyboardEdit.DOWN)
    }

    /** A modifier's state: 0 off, 1 for the next key, 2 locked. */
    fun level(id: String): Int = when (id) {
        "shift" -> shift; "control" -> ctrl; "option" -> alt; "command" -> cmd; "fn" -> fn; else -> 0
    }

    fun lit(key: MacKey): Boolean = key.id == "return" || level(key.id) > 0

    /** Return shows what Enter does when it is more than a new line (send, done). */
    val returnHint: String get() = if (enterLabel == "换行" || enterLabel == "回车") "" else enterLabel

    fun toggleLanguage() {
        val composer = Pinyin.composer
        if (composer != null && composer.active) {
            target.insert(composer.commitRaw())
            refresh()
        }
        KeyboardPrefs.useChinese(!KeyboardPrefs.chinese)
    }

    /** The symbols of a row on the current page, in the current language. */
    fun symbols(row: Int): List<String> {
        val set = if (page == Page.NUMBERS) (if (chinese) ZH_NUMBER_SYMBOLS else EN_NUMBER_SYMBOLS)
            else (if (chinese) ZH_MORE_SYMBOLS else EN_MORE_SYMBOLS)
        return set[row]
    }
}

/**
 * The keyboard. `enterLabel` is what Enter does in this field, shown on the
 * key; `compact` draws shorter keys so the keyboard and its field fit below a
 * half-fold crease. The root shows it below the content while a field holds it.
 */
@Composable
fun MusterKeyboard(target: KeyboardTarget, enterLabel: String = "换行", compact: Boolean = false, modifier: Modifier = Modifier) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val density = LocalDensity.current
    val state = remember(target) { KeyboardState(target, enterLabel) }
    var width by remember { mutableStateOf(0.dp) }
    val wide = width >= MAC_MIN_WIDTH
    LaunchedEffect(target) {
        Pinyin.composer?.let { it.clear(); state.ready = true; return@LaunchedEffect }
        val composer = Pinyin.load(context)
        if (composer != null) { composer.clear(); state.ready = true }
    }
    DisposableEffect(target) {
        onDispose {
            KeyboardPrefs.keyboardHeight = 0.0
            // Leaving a field keeps what was typed: pending pinyin goes in as letters.
            val composer = Pinyin.composer
            if (composer != null && composer.active) target.insert(composer.commitRaw())
        }
    }
    val gap = if (compact) 4.dp else 5.dp
    Column(
        modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainer)
            .padding(start = 5.dp, end = 5.dp, top = 4.dp, bottom = if (compact) 4.dp else 8.dp)
            .onSizeChanged { size ->
                with(density) {
                    KeyboardPrefs.keyboardHeight = (size.height.toDp() + 4.dp + (if (compact) 4.dp else 8.dp)).value.toDouble()
                    width = size.width.toDp()
                }
            },
        verticalArrangement = Arrangement.spacedBy(if (compact) 4.dp else 6.dp),
    ) {
        TopBar(state, compact, wide)
        if (wide) MacLayout(state, compact, gap) else PhoneLayout(state, compact, gap)
    }
}

// ---------------------------------------------------------------- caps

@Composable
private fun Cap(
    modifier: Modifier,
    fill: Color,
    radius: Dp,
    shadow: Boolean,
    onClick: () -> Unit,
    repeat: Boolean = false,
    content: @Composable () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scope = rememberCoroutineScope()
    val background = if (pressed) MaterialTheme.colorScheme.surfaceVariant else fill
    val base = modifier.shadow(if (shadow) 1.dp else 0.dp, RoundedCornerShape(radius)).clip(RoundedCornerShape(radius)).background(background)
    val pressable = if (repeat) {
        // Fires once on a tap and repeats while held (delete, arrows).
        base.pointerInput(onClick) {
            detectTapGestures(onPress = {
                var repeated = false
                val job = scope.launch {
                    delay(400)
                    repeated = true
                    while (true) { onClick(); delay(100) }
                }
                tryAwaitRelease()
                job.cancel()
                if (!repeated) onClick()
            })
        }
    } else {
        base.clickable(interactionSource = interaction, indication = null, onClick = onClick)
    }
    Box(pressable, contentAlignment = Alignment.Center) { content() }
}

@Composable
private fun RowScope.CharKey(state: KeyboardState, letter: String, height: Dp) {
    Cap(Modifier.weight(1f).height(height), MaterialTheme.colorScheme.surface, 8.dp, true, { state.letter(letter) }) {
        Text(if (state.shift > 0) letter.uppercase() else letter, fontSize = 21.sp, color = MaterialTheme.colorScheme.onSurface)
    }
}

@Composable
private fun RowScope.TextKey(state: KeyboardState, text: String, height: Dp) {
    Cap(Modifier.weight(1f).height(height), MaterialTheme.colorScheme.surface, 8.dp, true, { state.symbol(text) }) {
        Text(text, fontSize = 19.sp, color = MaterialTheme.colorScheme.onSurface)
    }
}

@Composable
private fun RowScope.ActionKey(state: KeyboardState, label: String, weight: Float, height: Dp, lit: Boolean = false, locked: Boolean = false, onClick: () -> Unit) {
    val fill = if (lit) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant
    Cap(
        Modifier.weight(weight).height(height).border(if (locked) 2.dp else 0.dp, if (locked) MaterialTheme.colorScheme.onSurface else Color.Transparent, RoundedCornerShape(8.dp)),
        fill, 8.dp, false, onClick,
    ) {
        Text(label, fontSize = 15.sp, color = if (lit) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface, textAlign = TextAlign.Center)
    }
}

// ---------------------------------------------------------------- phone layout

@Composable
private fun PhoneLayout(state: KeyboardState, compact: Boolean, gap: Dp) {
    val height = if (compact) COMPACT_KEY_HEIGHT else KEY_HEIGHT
    val surface = MaterialTheme.colorScheme.surface
    val variant = MaterialTheme.colorScheme.surfaceVariant
    val onSurface = MaterialTheme.colorScheme.onSurface
    if (state.raw) {
        LazyRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            itemsIndexed(TERMINAL_KEYS) { _, key ->
                Cap(Modifier.widthIn(min = 44.dp).height(if (compact) 28.dp else 32.dp), variant, 6.dp, false, { state.named(key.second) }) {
                    Text(key.first, fontSize = 12.sp, color = onSurface, modifier = Modifier.padding(horizontal = 6.dp))
                }
            }
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(gap)) { NUMBERS.forEach { TextKey(state, it, height) } }
    if (state.page == Page.LETTERS) {
        Row(horizontalArrangement = Arrangement.spacedBy(gap)) { LETTER_ROWS[0].forEach { CharKey(state, it, height) } }
        Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
            Spacer(Modifier.weight(0.5f))
            LETTER_ROWS[1].forEach { CharKey(state, it, height) }
            Spacer(Modifier.weight(0.5f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
            Cap(Modifier.weight(1.45f).height(height), variant, 8.dp, false, { state.toggleShift() }) {
                Icon(Icons.Outlined.ArrowUpward, if (state.shift == 2) "Caps lock" else "Shift",
                    tint = if (state.shift == 2) MaterialTheme.colorScheme.primary else onSurface)
            }
            Spacer(Modifier.width(3.dp))
            LETTER_ROWS[2].forEach { CharKey(state, it, height) }
            Spacer(Modifier.width(3.dp))
            DeleteKey(state, Modifier.weight(1.45f).height(height))
        }
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(gap)) { state.symbols(0).forEach { TextKey(state, it, height) } }
        Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
            ActionKey(state, if (state.page == Page.NUMBERS) "#+=" else "符", 1.45f, height) {
                state.page = if (state.page == Page.NUMBERS) Page.SYMBOLS else Page.NUMBERS
            }
            Spacer(Modifier.width(3.dp))
            state.symbols(1).forEach { TextKey(state, it, height) }
            Spacer(Modifier.width(3.dp))
            DeleteKey(state, Modifier.weight(1.45f).height(height))
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
        if (state.raw) {
            // Ctrl or Alt: highlighted while it applies, outlined when locked.
            ActionKey(state, "ctrl", 1.1f, height, lit = state.ctrl > 0, locked = state.ctrl == 2) { state.toggleCtrl() }
            ActionKey(state, "alt", 1.1f, height, lit = state.alt > 0, locked = state.alt == 2) { state.toggleAlt() }
        }
        ActionKey(state, if (state.page == Page.LETTERS) "符" else "ABC", 1.2f, height) {
            state.page = if (state.page == Page.LETTERS) Page.NUMBERS else Page.LETTERS
        }
        // 中/英 with the current language emphasized.
        Cap(Modifier.weight(1.2f).height(height), variant, 8.dp, false, { state.toggleLanguage() }) {
            Row {
                Text("中", fontSize = 16.sp, fontWeight = if (KeyboardPrefs.chinese) FontWeight.Bold else FontWeight.Normal,
                    color = if (KeyboardPrefs.chinese) onSurface else MaterialTheme.colorScheme.outline)
                Text("/", fontSize = 16.sp, color = MaterialTheme.colorScheme.outline)
                Text("英", fontSize = 16.sp, fontWeight = if (KeyboardPrefs.chinese) FontWeight.Normal else FontWeight.Bold,
                    color = if (KeyboardPrefs.chinese) MaterialTheme.colorScheme.outline else onSurface)
            }
        }
        if (!state.raw) PunctuationKey(state, 0, height)
        Cap(Modifier.weight(if (state.raw) 3f else 4f).height(height), surface, 8.dp, true, { state.space() }) {
            Text(if (state.chinese) "拼音" else "space", fontSize = 15.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (!state.raw) PunctuationKey(state, 1, height)
        Cap(Modifier.weight(1.6f).height(height), MaterialTheme.colorScheme.primary, 8.dp, false, { state.enter() }) {
            Text(state.enterLabel, fontSize = 16.sp, color = MaterialTheme.colorScheme.onPrimary)
        }
    }
}

@Composable
private fun RowScope.PunctuationKey(state: KeyboardState, which: Int, height: Dp) {
    val text = if (state.chinese) (if (which == 0) "，" else "。") else (if (which == 0) "," else ".")
    Cap(Modifier.weight(0.9f).height(height), MaterialTheme.colorScheme.surface, 8.dp, true, { state.symbol(text) }) {
        Text(text, fontSize = 20.sp, color = MaterialTheme.colorScheme.onSurface)
    }
}

@Composable
private fun DeleteKey(state: KeyboardState, modifier: Modifier) {
    Cap(modifier, MaterialTheme.colorScheme.surfaceVariant, 8.dp, false, { state.backspace() }, repeat = true) {
        Icon(Icons.AutoMirrored.Filled.Backspace, "Delete", tint = MaterialTheme.colorScheme.onSurface)
    }
}

// ---------------------------------------------------------------- MacBook layout

// Rows of MacKeys whose widths are shares of the row, so the columns line up
// as on the real keyboard. Each key's slot carries the gap as padding.
@Composable
private fun MacLayout(state: KeyboardState, compact: Boolean, gap: Dp) {
    val height = if (compact) COMPACT_KEY_HEIGHT else KEY_HEIGHT
    if (state.raw) MacRow(state, MAC_FUNCTION_ROW, height * 0.7f, compact, gap)
    MAC_ROWS.forEach { MacRow(state, it, height, compact, gap) }
}

@Composable
private fun MacRow(state: KeyboardState, keys: List<MacKey>, height: Dp, compact: Boolean, gap: Dp) {
    Row(Modifier.fillMaxWidth().height(height)) {
        keys.forEach { key ->
            if (key.kind == MacKeyKind.ARROWS) {
                ArrowCluster(state, Modifier.weight(key.weight.toFloat()).fillMaxHeight(), gap)
            } else {
                Box(Modifier.weight(key.weight.toFloat()).fillMaxHeight().padding(horizontal = gap / 2)) {
                    when (key.kind) {
                        MacKeyKind.LETTER -> Cap(Modifier.fillMaxWidth().fillMaxHeight(), MaterialTheme.colorScheme.surface, 6.dp, true, { state.macPress(key) }) {
                            Text(key.id.uppercase(), fontSize = if (compact) 16.sp else 18.sp, color = MaterialTheme.colorScheme.onSurface)
                        }
                        MacKeyKind.SYMBOL -> Cap(Modifier.fillMaxWidth().fillMaxHeight(), MaterialTheme.colorScheme.surface, 6.dp, true, { state.macPress(key) }) {
                            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                Text(key.top, fontSize = if (compact) 11.sp else 12.sp,
                                    color = if (state.shift > 0) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.outline)
                                Text(key.bottom, fontSize = if (compact) 14.sp else 16.sp,
                                    color = if (state.shift > 0) MaterialTheme.colorScheme.outline else MaterialTheme.colorScheme.onSurface)
                            }
                        }
                        MacKeyKind.NAMED -> Cap(Modifier.fillMaxWidth().fillMaxHeight(), MaterialTheme.colorScheme.surfaceVariant, 5.dp, false, { state.macAction(key.id) }) {
                            Text(key.bottom, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurface)
                        }
                        else -> ActionCap(state, key, compact)
                    }
                }
            }
        }
    }
}

/** Modifiers and editing keys: small legends in the corners, toward the space bar for ⌃ ⌥ ⌘. */
@Composable
private fun ActionCap(state: KeyboardState, key: MacKey, compact: Boolean) {
    val lit = state.lit(key)
    val legend = if (lit) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
    val fill = if (lit) MaterialTheme.colorScheme.primary else if (key.id == "space") MaterialTheme.colorScheme.surface else MaterialTheme.colorScheme.surfaceVariant
    val locked = state.level(key.id) == 2
    val modifier = Modifier.fillMaxWidth().fillMaxHeight()
        .border(if (locked) 2.dp else 0.dp, if (locked) MaterialTheme.colorScheme.onSurface else Color.Transparent, RoundedCornerShape(6.dp))
    val content: @Composable () -> Unit = {
        Column(
            Modifier.fillMaxWidth().fillMaxHeight().padding(horizontal = 6.dp, vertical = if (compact) 2.dp else 4.dp),
            horizontalAlignment = if (key.end) Alignment.End else Alignment.Start,
        ) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = if (key.end || key.id == "fn") Arrangement.End else Arrangement.Start,
                verticalAlignment = Alignment.CenterVertically) {
                when {
                    key.id == "caps" -> {
                        Box(Modifier.size(5.dp).background(if (state.capsLock) Color(0xFF34C759) else Color(0x40808080), CircleShape))
                        Spacer(Modifier.weight(1f))
                        Text(if (state.chinese) "中" else "英", fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    key.id == "return" -> Text(state.returnHint, fontSize = 10.sp, color = MaterialTheme.colorScheme.onPrimary)
                    key.top.isNotEmpty() -> Text(key.top, fontSize = if (key.id == "fn" || compact) 10.sp else 12.sp, color = legend)
                }
            }
            Spacer(Modifier.weight(1f))
            if (key.id == "fn") {
                Icon(Icons.Default.Language, null, Modifier.size(13.dp), tint = legend)
            } else if (key.bottom.isNotEmpty()) {
                Text(key.bottom, fontSize = if (compact) 10.sp else 11.sp, color = legend, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
    when (key.id) {
        "delete" -> Cap(modifier, fill, 6.dp, false, { state.macAction(key.id) }, repeat = true, content = content)
        "caps" -> Box(
            modifier.clip(RoundedCornerShape(6.dp)).background(fill).pointerInput(Unit) {
                detectTapGestures(onTap = { state.macAction("caps") }, onLongPress = { state.capsLock = !state.capsLock })
            },
        ) { content() }
        else -> Cap(modifier, fill, 6.dp, false, { state.macPress(key) }, content = content)
    }
}

/** Inverted T: full-height ← and →, half-height ↑ over ↓, as on current MacBooks. */
@Composable
private fun ArrowCluster(state: KeyboardState, modifier: Modifier, gap: Dp) {
    val fill = MaterialTheme.colorScheme.surfaceVariant
    val tint = MaterialTheme.colorScheme.onSurface
    Row(modifier) {
        Cap(Modifier.weight(1f).fillMaxHeight().padding(horizontal = gap / 2), fill, 5.dp, false, { state.macAction("left") }, repeat = true) {
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, "left", Modifier.size(14.dp), tint = tint)
        }
        Column(Modifier.weight(1f).fillMaxHeight().padding(horizontal = gap / 2), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Cap(Modifier.weight(1f).fillMaxWidth(), fill, 5.dp, false, { state.macAction("up") }, repeat = true) {
                Icon(Icons.Default.KeyboardArrowUp, "up", Modifier.size(12.dp), tint = tint)
            }
            Cap(Modifier.weight(1f).fillMaxWidth(), fill, 5.dp, false, { state.macAction("down") }, repeat = true) {
                Icon(Icons.Default.KeyboardArrowDown, "down", Modifier.size(12.dp), tint = tint)
            }
        }
        Cap(Modifier.weight(1f).fillMaxHeight().padding(horizontal = gap / 2), fill, 5.dp, false, { state.macAction("right") }, repeat = true) {
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, "right", Modifier.size(14.dp), tint = tint)
        }
    }
}

// ---------------------------------------------------------------- top bar

@Composable
private fun TopBar(state: KeyboardState, compact: Boolean, wide: Boolean) {
    Row(Modifier.fillMaxWidth().height(if (compact) 36.dp else 46.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (state.composing.isNotEmpty()) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(state.composing, fontSize = 13.sp, color = MaterialTheme.colorScheme.primary, maxLines = 1, modifier = Modifier.padding(start = 8.dp))
                LazyRow(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                    itemsIndexed(state.candidates) { index, candidate ->
                        Text(
                            candidate.word, fontSize = 20.sp,
                            fontWeight = if (index == 0) FontWeight.Medium else FontWeight.Normal,
                            color = if (index == 0) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable { state.pick(candidate) }.padding(horizontal = 10.dp, vertical = 2.dp),
                        )
                    }
                }
            }
        } else {
            Text(if (state.chinese) "拼音" else if (KeyboardPrefs.chinese) "词库加载中…" else "English",
                fontSize = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 8.dp))
            if (wide) {
                Text("caps lock：轻点切换中/英，按住锁定大写", fontSize = 11.sp, color = MaterialTheme.colorScheme.outline,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 8.dp))
            }
            Spacer(Modifier.weight(1f))
            Box(Modifier.size(40.dp, 34.dp).clip(RoundedCornerShape(8.dp)).clickable { state.target.hide() }, contentAlignment = Alignment.Center) {
                Icon(Icons.Default.KeyboardHide, "Hide keyboard", tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}
