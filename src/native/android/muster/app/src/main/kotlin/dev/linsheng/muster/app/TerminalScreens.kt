package dev.linsheng.muster.app

import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.key.utf16CodePoint
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.linsheng.muster.app.input.KeyboardHost
import dev.linsheng.muster.app.input.KeyboardPrefs
import dev.linsheng.muster.app.input.KeyboardTarget
import dev.linsheng.muster.core.MusterCore
import dev.linsheng.muster.core.StyleRun
import dev.linsheng.muster.core.TerminalKeys
import muster.app.Screen
import muster.app.TerminalView
import muster.app.ViewModel
import kotlin.math.max
import kotlin.math.min

// Terminals: the list, and one session drawn on a canvas as a grid of
// styled cells. The shell draws rows and the cursor and forwards keys.

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TerminalsScreen(core: MusterCore, vm: ViewModel) {
    val terminals = vm.terminals
    var ssh by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            DeviceTabs(vm, terminals.deviceId, select = { core.openTerminals(it) }, details = { core.selectDevice(it) }, modifier = Modifier.weight(1f))
            if (terminals.canCreate) {
                IconButton(onClick = { core.terminalCreate() }) { Icon(Icons.Default.Add, "New window") }
            }
            if (terminals.sshAvailable) {
                IconButton(onClick = { ssh = true }) { Icon(Icons.Default.Lock, "SSH") }
            }
        }
        PullToRefreshBox(isRefreshing = false, onRefresh = { core.openTerminals(terminals.deviceId) }, modifier = Modifier.fillMaxWidth().weight(1f)) {
            LazyColumn(Modifier.fillMaxSize()) {
                items(terminals.errorsList, key = { "error-$it" }) { ErrorLine(it) }
                if (terminals.sessionsList.isEmpty()) {
                    item(key = "empty") {
                        if (terminals.loading) {
                            CircularProgressIndicator(Modifier.padding(24.dp))
                        } else {
                            Text("Terminal sessions on the selected devices show up here.", Modifier.padding(24.dp),
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
                items(terminals.sessionsList, key = { "${it.deviceId}:${it.id}" }) { session ->
                    val open = vm.screen == Screen.TERMINAL && vm.terminal.session == session.id && vm.terminal.deviceId == session.deviceId
                    Column(
                        Modifier.fillMaxWidth()
                            .background(if (open) MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f) else Color.Transparent)
                            .clickable { core.openTerminal(session.id, session.deviceId) }
                            .padding(16.dp, 10.dp),
                    ) {
                        Text(session.title.ifEmpty { session.id }, style = MaterialTheme.typography.bodyLarge, maxLines = 1)
                        Text("${session.deviceName} · ${session.source} · ${session.cols}×${session.rows}",
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    HorizontalDivider(Modifier.padding(horizontal = 16.dp))
                }
            }
        }
    }
    if (ssh) SshDialog({ ssh = false }) { user, port -> core.openSsh(port, user, 80, 24); ssh = false }
}

/**
 * The grid, the key bar, and a text field that carries the soft keyboard's
 * input to the core. With the Muster keyboard on, the keyboard itself types
 * into the session (text as typed, everything else as named keys) and the
 * special keys and field stay hidden; Paste remains available in either mode.
 * A tap on the grid brings the keyboard back after Hide.
 */
@Composable
fun TerminalScreen(core: MusterCore, vm: ViewModel) {
    val terminal = vm.terminal
    val layout = LocalWorkspaceLayout.current
    val systemKeyboard = KeyboardPrefs.terminalSystemKeyboard
    var keyboardHidden by remember { mutableStateOf(false) }
    val target = remember(core) {
        KeyboardTarget(
            insert = { core.terminalText(it) },
            backspace = { core.terminalKey(TerminalKeys.BACKSPACE) },
            enter = { core.terminalKey(TerminalKeys.ENTER) },
            hide = { keyboardHidden = true },
            key = { core.terminalKey(it) },
        )
    }
    LaunchedEffect(systemKeyboard, keyboardHidden, layout.flex, target) {
        if (!systemKeyboard && !keyboardHidden) KeyboardHost.show(target, "回车", layout.flex) else KeyboardHost.hide(target)
    }
    DisposableEffect(target) { onDispose { KeyboardHost.hide(target) } }
    Column(Modifier.fillMaxSize().background(Color.Black)) {
        // Top and bottom: the grid stays above the crease; keys and status go below it.
        val grid = if (layout.flex) Modifier.height(layout.readingHeight.dp).fillMaxWidth() else Modifier.weight(1f).fillMaxWidth()
        TerminalGrid(terminal, core, grid.pointerInput(Unit) { detectTapGestures { keyboardHidden = false } })
        if (layout.flex) Spacer(Modifier.height(layout.hingeHeight.dp))
        KeyBar(core, terminal.canPaste, showsKeys = systemKeyboard)
        if (systemKeyboard) {
            TerminalInput(core)
        } else if (keyboardHidden) {
            Text("Tap the terminal to type", Modifier.padding(8.dp, 4.dp), color = Color.LightGray, style = MaterialTheme.typography.labelSmall)
        }
        if (terminal.ended || terminal.status.isNotEmpty()) {
            Text(
                if (terminal.ended) "Ended · ${terminal.status}" else terminal.status,
                Modifier.padding(8.dp, 4.dp), color = if (terminal.ended) Color(0xffffb74d) else Color.LightGray,
                style = MaterialTheme.typography.labelSmall,
            )
        }
        if (layout.flex) Spacer(Modifier.weight(1f))
    }
}

@Composable
private fun KeyBar(core: MusterCore, canPaste: Boolean, showsKeys: Boolean) {
    val clipboard = LocalClipboardManager.current
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(4.dp)) {
        // The clipboard is typed by the core while the session accepts it
        // (line breaks become Enter, control characters are dropped).
        OutlinedButton(
            onClick = { clipboard.getText()?.text?.takeIf { it.isNotEmpty() }?.let { core.terminalPaste(it) } },
            Modifier.padding(horizontal = 2.dp), enabled = canPaste,
        ) {
            Icon(Icons.Default.ContentPaste, null, Modifier.size(14.dp))
            Spacer(Modifier.width(4.dp))
            Text("Paste", fontSize = 12.sp)
        }
        if (showsKeys) {
            listOf(
                "esc" to TerminalKeys.ESCAPE, "tab" to TerminalKeys.TAB, "^C" to "ctrl-c", "^D" to "ctrl-d", "^Z" to "ctrl-z", "^L" to "ctrl-l",
                "←" to TerminalKeys.LEFT, "↓" to TerminalKeys.DOWN, "↑" to TerminalKeys.UP, "→" to TerminalKeys.RIGHT,
                "home" to TerminalKeys.HOME, "end" to TerminalKeys.END, "pgup" to TerminalKeys.PAGE_UP, "pgdn" to TerminalKeys.PAGE_DOWN,
            ).forEach { (label, key) ->
                OutlinedButton(onClick = { core.terminalKey(key) }, Modifier.padding(horizontal = 2.dp)) { Text(label, fontSize = 12.sp) }
            }
        }
    }
}

/**
 * A one-line field: every character typed goes to the core and the field is
 * cleared; enter and backspace go as keys; a hardware keyboard's arrows and
 * control chords are caught before the field sees them.
 */
@Composable
private fun TerminalInput(core: MusterCore) {
    var text by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    TextField(
        value = text,
        onValueChange = { new ->
            if (new.length < text.length) core.terminalKey(TerminalKeys.BACKSPACE)
            else if (new.length > text.length) core.terminalText(new.substring(text.length))
            text = ""
        },
        modifier = Modifier.fillMaxWidth().focusRequester(focus).onPreviewKeyEvent { event ->
            if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
            val special = when (event.key) {
                Key.DirectionUp -> TerminalKeys.UP
                Key.DirectionDown -> TerminalKeys.DOWN
                Key.DirectionLeft -> TerminalKeys.LEFT
                Key.DirectionRight -> TerminalKeys.RIGHT
                Key.Escape -> TerminalKeys.ESCAPE
                Key.Tab -> TerminalKeys.TAB
                Key.MoveHome -> TerminalKeys.HOME
                Key.MoveEnd -> TerminalKeys.END
                Key.PageUp -> TerminalKeys.PAGE_UP
                Key.PageDown -> TerminalKeys.PAGE_DOWN
                Key.Delete -> TerminalKeys.DELETE
                Key.Enter -> TerminalKeys.ENTER
                else -> if (event.isCtrlPressed) TerminalKeys.control(event.utf16CodePoint.toChar()) else null
            }
            if (special != null) { core.terminalKey(special); true } else false
        },
        singleLine = true,
        placeholder = { Text("type here") },
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Ascii, imeAction = ImeAction.Send),
        keyboardActions = KeyboardActions(onSend = { core.terminalKey(TerminalKeys.ENTER) }),
        colors = TextFieldDefaults.colors(),
    )
}

private val palette16 = intArrayOf(
    0xff000000.toInt(), 0xffcc3333.toInt(), 0xff33b333.toInt(), 0xffccb333.toInt(), 0xff4d66e6.toInt(), 0xffb34db3.toInt(), 0xff33b3b3.toInt(), 0xffd9d9d9.toInt(),
    0xff666666.toInt(), 0xffff6666.toInt(), 0xff66e666.toInt(), 0xffffe666.toInt(), 0xff8099ff.toInt(), 0xffe680e6.toInt(), 0xff66e6e6.toInt(), 0xffffffff.toInt(),
)

/** The 256-colour palette: 16 basic, a 6×6×6 cube, 24 grays. */
val palette256: IntArray = IntArray(256) { i ->
    when {
        i < 16 -> palette16[i]
        i < 232 -> {
            val n = i - 16
            val steps = intArrayOf(0, 95, 135, 175, 215, 255)
            0xff000000.toInt() or (steps[n / 36] shl 16) or (steps[(n / 6) % 6] shl 8) or steps[n % 6]
        }
        else -> { val g = 8 + (i - 232) * 10; 0xff000000.toInt() or (g shl 16) or (g shl 8) or g }
    }
}

/**
 * The grid. Only our own grids (PTY, SSH) take the viewport's size and report
 * it; a live window (someone's iTerm2) keeps its own columns, and the font
 * shrinks to fit them.
 */
@Composable
fun TerminalGrid(terminal: TerminalView, core: MusterCore, modifier: Modifier) {
    val density = LocalDensity.current
    val base = with(density) { 13.sp.toPx() }
    val smallest = with(density) { 5.sp.toPx() }
    var width by remember { mutableIntStateOf(0) }
    var height by remember { mutableIntStateOf(0) }
    val paint = remember { Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = Typeface.MONOSPACE } }
    val ratio = remember(paint, base) { paint.textSize = base; paint.measureText("M") / base }
    val drawSize = if (!terminal.resizable && terminal.cols > 0 && width > 0) {
        max(smallest, min(base, width / (terminal.cols * ratio)))
    } else {
        base
    }
    paint.textSize = drawSize
    val metrics = paint.fontMetrics
    val cellWidth = ratio * drawSize
    val cellHeight = metrics.descent - metrics.ascent
    var reported by remember { mutableStateOf(0 to 0) }
    LaunchedEffect(width, height, terminal.resizable, cellWidth, cellHeight) {
        if (!terminal.resizable || width == 0 || height == 0) return@LaunchedEffect
        val cols = max(20, (width / cellWidth).toInt())
        val rows = max(5, (height / cellHeight).toInt())
        if (cols to rows != reported) { reported = cols to rows; core.terminalResize(cols, rows) }
    }
    Canvas(modifier.padding(4.dp).onSizeChanged { size -> width = size.width; height = size.height }) {
        val canvas = drawContext.canvas.nativeCanvas
        val cols = max(terminal.cols, 1)
        for (line in terminal.linesList) {
            val top = line.index * cellHeight
            val baseline = top - metrics.ascent
            val runs = StyleRun.decode(line.styles.toByteArray())
            val text = line.text
            var col = 0
            if (runs.isEmpty()) {
                paint.color = 0xffffffff.toInt()
                paint.isFakeBoldText = false
                paint.isUnderlineText = false
                paint.isStrikeThruText = false
                canvas.drawText(text, 0f, baseline, paint)
                continue
            }
            for (run in runs) {
                if (col >= text.length) break
                val end = minOf(text.length, col + run.length, cols)
                var fg = StyleRun.argb(run.fg, palette256) ?: 0xffffffff.toInt()
                var bg = StyleRun.argb(run.bg, palette256)
                if (run.inverse) { val swap = fg; fg = bg ?: 0xff000000.toInt(); bg = swap }
                val x = col * cellWidth
                if (bg != null) {
                    paint.color = bg
                    canvas.drawRect(x, top, x + (end - col) * cellWidth, top + cellHeight, paint)
                }
                paint.color = if (run.dim) (fg and 0x00ffffff) or 0x99000000.toInt() else fg
                paint.isFakeBoldText = run.bold
                paint.isUnderlineText = run.underline
                paint.isStrikeThruText = run.strike
                canvas.drawText(text, col, end, x, baseline, paint)
                col = end
            }
        }
        if (!terminal.ended) {
            paint.color = 0x99ffffff.toInt()
            val x = terminal.cursorX * cellWidth
            val y = terminal.cursorY * cellHeight
            canvas.drawRect(x, y, x + cellWidth, y + cellHeight, paint)
        }
    }
}
