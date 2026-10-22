package dev.linsheng.muster.app

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.InsertDriveFile
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.core.MusterCore
import muster.app.ViewModel

// The session's working tree: a listing, one file, the git log or a diff.

@Composable
fun FilesScreen(core: MusterCore, vm: ViewModel) {
    val files = vm.files
    LazyColumn(Modifier.fillMaxSize()) {
        if (files.loading) item(key = "loading") { CircularProgressIndicator(Modifier.padding(16.dp)) }
        if (files.path.isNotEmpty()) item(key = "..") {
            ListItem(modifier = Modifier.clickable { core.openFiles(files.path.substringBeforeLast('/', "")) }, headlineContent = { Text("..") })
        }
        items(files.entriesList, key = { it.name }) { entry ->
            val path = if (files.path.isEmpty()) entry.name else "${files.path}/${entry.name}"
            ListItem(
                modifier = Modifier.clickable { if (entry.dir) core.openFiles(path) else core.openFile(path) },
                leadingContent = { Icon(if (entry.dir) Icons.Default.Folder else Icons.AutoMirrored.Filled.InsertDriveFile, null) },
                headlineContent = { Text(entry.name) },
                trailingContent = { if (!entry.dir) Text(bytesText(entry.size), style = MaterialTheme.typography.bodySmall) },
            )
        }
    }
}

@Composable
fun FileScreen(core: MusterCore, vm: ViewModel) {
    val file = vm.file
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant).padding(12.dp, 6.dp, 4.dp, 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(if (file.isDiff) "Recorded turn changes" else "Current file contents", style = MaterialTheme.typography.bodySmall, color = secondary)
                if (file.truncated) Text("Preview truncated", style = MaterialTheme.typography.bodySmall, color = Color(0xFFFB8C00))
            }
            if (file.isDiff && file.currentPath.isNotEmpty()) {
                TextButton(onClick = { core.openFile(file.currentPath) }) { Text("Current file") }
            }
        }
        when {
            file.loading -> CircularProgressIndicator(Modifier.padding(16.dp))
            file.error.isNotEmpty() -> Text("Preview unavailable: ${file.error}", Modifier.padding(16.dp), color = secondary)
            !file.content.isValidUtf8 -> Text("Binary file: ${bytesText(file.content.size().toLong())} of binary data", Modifier.padding(16.dp), color = secondary)
            else -> SourcePreview(file.content.toStringUtf8(), file.isDiff, Modifier.weight(1f))
        }
    }
}

/** Source with line numbers, or a diff with added, removed and hunk lines coloured. */
@Composable
fun SourcePreview(text: String, diff: Boolean, modifier: Modifier = Modifier) {
    val lines = remember(text) { text.split('\n') }
    val scroll = rememberScrollState()
    val primary = MaterialTheme.colorScheme.onSurface
    LazyColumn(modifier.fillMaxWidth().horizontalScroll(scroll), contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 8.dp)) {
        itemsIndexed(lines, key = { index, _ -> index }) { index, line ->
            val added = diff && line.startsWith("+") && !line.startsWith("+++")
            val removed = diff && line.startsWith("-") && !line.startsWith("---")
            val hunk = diff && line.startsWith("@@")
            val color = when { added -> Color(0xFF2E7D32); removed -> Color(0xFFC62828); hunk -> Color(0xFF1E88E5); else -> primary }
            val fill = when { added -> Color(0x142E7D32); removed -> Color(0x14C62828); hunk -> Color(0x141E88E5); else -> Color.Transparent }
            Row(Modifier.background(fill).padding(12.dp, 1.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                if (!diff) {
                    Text("${index + 1}", Modifier.widthIn(min = 32.dp), fontFamily = FontFamily.Monospace,
                        style = MaterialTheme.typography.bodySmall, color = primary.copy(alpha = 0.4f), textAlign = TextAlign.End)
                }
                Text(line.ifEmpty { " " }, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall,
                    color = color, maxLines = 1, softWrap = false)
            }
        }
    }
}

@Composable
fun GitScreen(core: MusterCore, vm: ViewModel) {
    val git = vm.git
    when {
        git.loading -> CircularProgressIndicator(Modifier.padding(16.dp))
        git.diff.isNotEmpty() -> SourcePreview(git.diff, diff = true, Modifier.fillMaxSize())
        else -> LazyColumn(Modifier.fillMaxSize()) {
            items(git.commitsList, key = { it.hash }) { commit ->
                ListItem(
                    modifier = Modifier.clickable { core.openGit(commit.hash) },
                    headlineContent = { Text(commit.subject, maxLines = 2) },
                    supportingContent = { Text("${commit.hash.take(8)} · ${commit.author}") },
                )
            }
        }
    }
}
