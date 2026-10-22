package dev.linsheng.muster.app.input

import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldColors
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.InterceptPlatformTextInput
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.awaitCancellation

// Text fields that type with the in-app keyboard (or the system keyboard
// when Settings asks for it). The owner keeps the text: edits come back
// through onChange, Enter through onSubmit. While the Muster keyboard is on,
// the platform input session is intercepted so the IME never shows, and the
// focused field registers itself with KeyboardHost, which the root draws.

/**
 * A text field. A `singleLine` field submits on Enter (when the owner handles
 * it) or closes the keyboard; a multi-line one inserts a line break unless
 * `submitOnEnter`. `outlined` draws Material's outlined field; otherwise a
 * plain filled field for a composer.
 */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun MusterTextField(
    text: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    label: String? = null,
    singleLine: Boolean = false,
    submitOnEnter: Boolean = false,
    enterLabel: String = if (singleLine) "完成" else "换行",
    monospace: Boolean = false,
    enabled: Boolean = true,
    outlined: Boolean = true,
    minHeight: Dp = Dp.Unspecified,
    maxHeight: Dp = Dp.Unspecified,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    /** Take focus (and so show the keyboard) on appearing and whenever this turns true. */
    autoFocus: Boolean = false,
    /** A shorter keyboard that fits below a half-fold crease. */
    compactKeyboard: Boolean = false,
    colors: TextFieldColors? = null,
    onSubmit: (() -> Unit)? = null,
) {
    val focusManager = LocalFocusManager.current
    val clipboard = LocalClipboardManager.current
    val focus = remember { FocusRequester() }
    var value by remember { mutableStateOf(TextFieldValue(text, TextRange(text.length))) }
    // The owner's text wins (a cleared draft, a restored one); the caret goes to its end.
    if (value.text != text) value = TextFieldValue(text, TextRange(text.length))
    val latestChange by rememberUpdatedState(onChange)
    val latestSubmit by rememberUpdatedState(onSubmit)
    val systemKeyboard = KeyboardPrefs.systemKeyboard
    val editor = remember(singleLine, submitOnEnter) {
        FieldEditor(
            read = { value },
            write = { next -> value = next; latestChange(next.text) },
            submit = {
                val submit = latestSubmit
                if (submit != null && (singleLine || submitOnEnter)) {
                    submit()
                } else if (singleLine) {
                    focusManager.clearFocus()
                } else {
                    val current = value
                    val a = current.selection.min.coerceIn(0, current.text.length)
                    val b = current.selection.max.coerceIn(0, current.text.length)
                    val next = current.text.substring(0, a) + "\n" + current.text.substring(b)
                    value = TextFieldValue(next, TextRange(a + 1))
                    latestChange(next)
                }
            },
            close = { focusManager.clearFocus() },
            multiline = !singleLine,
            copyText = { clipboard.setText(AnnotatedString(it)) },
        )
    }
    val target = remember(editor) { editor.target() }
    var focused by remember { mutableStateOf(false) }
    // The keyboard follows the focus while it is the in-app one; the system
    // keyboard is the platform's business.
    LaunchedEffect(focused, systemKeyboard, enterLabel, compactKeyboard) {
        if (focused && !systemKeyboard) KeyboardHost.show(target, enterLabel, compactKeyboard) else KeyboardHost.hide(target)
    }
    DisposableEffect(target) { onDispose { KeyboardHost.hide(target) } }
    LaunchedEffect(autoFocus) { if (autoFocus) runCatching { focus.requestFocus() } }
    var fieldModifier = modifier.focusRequester(focus).onFocusChanged { focused = it.isFocused }
    if (minHeight != Dp.Unspecified || maxHeight != Dp.Unspecified) fieldModifier = fieldModifier.heightIn(min = minHeight, max = maxHeight)
    val style = if (monospace) TextStyle(fontFamily = FontFamily.Monospace) else TextStyle.Default
    val options = KeyboardOptions(imeAction = if (singleLine || submitOnEnter) ImeAction.Done else ImeAction.Default)
    val actions = KeyboardActions(onDone = { latestSubmit?.invoke() ?: focusManager.clearFocus() })
    InterceptPlatformTextInput(
        interceptor = { request, nextHandler ->
            if (KeyboardPrefs.systemKeyboard) nextHandler.startInputMethod(request) else awaitCancellation()
        },
    ) {
        if (outlined) {
            OutlinedTextField(
                value = value,
                onValueChange = { value = it; latestChange(it.text) },
                modifier = fieldModifier,
                enabled = enabled,
                textStyle = style,
                label = label?.let { { Text(it) } },
                placeholder = if (placeholder.isNotEmpty()) { { Text(placeholder) } } else null,
                singleLine = singleLine,
                maxLines = maxLines,
                keyboardOptions = options,
                keyboardActions = actions,
                colors = colors ?: OutlinedTextFieldDefaults.colors(),
            )
        } else {
            TextField(
                value = value,
                onValueChange = { value = it; latestChange(it.text) },
                modifier = fieldModifier,
                enabled = enabled,
                textStyle = style,
                placeholder = if (placeholder.isNotEmpty()) { { Text(placeholder) } } else null,
                singleLine = singleLine,
                maxLines = maxLines,
                keyboardOptions = options,
                keyboardActions = actions,
                colors = colors ?: TextFieldDefaults.colors(),
            )
        }
    }
}
