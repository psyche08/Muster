package dev.linsheng.muster.app

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.core.MusterCore
import muster.app.ChatView
import muster.app.MessagePart

@Composable
fun ConversationParts(parts: List<MessagePart>, core: MusterCore?) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        parts.forEach { part ->
            if (part.kind == "code") {
                Column(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(14.dp))) {
                    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                        Text(part.language.uppercase().ifEmpty { "CODE" }, modifier = Modifier.padding(vertical = 14.dp), style = MaterialTheme.typography.labelSmall)
                        if (part.commandId.isNotEmpty()) TextButton(onClick = { core?.codeAction(0, part.commandId) }) { Text("Run") }
                    }
                    HorizontalDivider()
                    SelectionContainer { Text(part.text.trimEnd(), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, modifier = Modifier.horizontalScroll(rememberScrollState()).padding(14.dp)) }
                }
            } else ChatMarkdown(part.text, core)
        }
    }
}

@Composable
fun CodePreviewDialog(chat: ChatView, core: MusterCore, onRun: () -> Unit) {
    if (chat.hasCodePreview()) {
        val preview = chat.codePreview
        AlertDialog(
            onDismissRequest = { core.codeAction(2) },
            title = { Text("Review command") },
            text = {
                Column(Modifier.heightIn(max = 480.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(preview.deviceName, style = MaterialTheme.typography.titleSmall)
                    Text(preview.cwd, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.labelSmall)
                    SelectionContainer { Text(preview.code, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                    Text("The exit code and output will be sent back to this conversation when the command finishes.")
                }
            },
            confirmButton = { TextButton(onClick = { core.codeAction(1, preview.id); onRun() }) { Text("Run & return result") } },
            dismissButton = { TextButton(onClick = { core.codeAction(2) }) { Text("Cancel") } },
        )
    }
}

@Composable
fun CodeRunHistory(chat: ChatView, core: MusterCore) {
    var selected by remember(chat.deviceId, chat.session.id) { mutableStateOf<String?>(null) }
    if (chat.codeError.isNotEmpty()) Text(chat.codeError, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp))
    if (chat.codeRunsList.isNotEmpty()) {
        Text("RECENT RUNS", style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            chat.codeRunsList.forEach { item ->
                OutlinedButton(onClick = { selected = item.run.id }, shape = RoundedCornerShape(12.dp)) {
                    Column(Modifier.width(220.dp)) {
                        Text("${item.run.state} · ${item.delivery}", style = MaterialTheme.typography.labelMedium)
                        Text(item.run.code, maxLines = 2, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
    chat.codeRunsList.find { it.run.id == selected }?.let { item ->
        AlertDialog(onDismissRequest = { selected = null }, title = { Text("Execution result") }, text = {
            Column(Modifier.heightIn(max = 500.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(item.run.cwd, style = MaterialTheme.typography.labelSmall)
                Text(item.run.code, fontFamily = FontFamily.Monospace)
                if (item.run.exitKnown) Text("Exit code ${item.run.exitCode}")
                if (item.run.truncated) Text("Output truncated")
                SelectionContainer { Text(item.run.output.ifEmpty { if (item.run.state == "completed") "No output" else "Waiting for output…" }, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                if (item.error.isNotEmpty()) Text(item.error, color = MaterialTheme.colorScheme.error)
                if (item.canReturn) TextButton(onClick = { core.codeAction(3, item.run.id) }) { Text("Return result to conversation") }
            }
        }, confirmButton = { TextButton(onClick = { selected = null }) { Text("Done") } })
    }
}
