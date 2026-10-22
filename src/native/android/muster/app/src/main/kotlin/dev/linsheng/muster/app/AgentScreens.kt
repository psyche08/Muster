package dev.linsheng.muster.app

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.InsertDriveFile
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.google.protobuf.ByteString
import dev.linsheng.muster.app.input.MusterTextField
import dev.linsheng.muster.core.MusterCore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import muster.app.ChatBlock
import muster.app.ChatView
import muster.app.QueuedMessagesView
import muster.app.Screen
import muster.app.UnconfirmedSend
import muster.app.ViewModel
import muster.app.chatDraft
import muster.v1.AgentAttachment
import muster.v1.AgentMessage
import muster.v1.AgentSession
import muster.v1.InteractionQuestion
import muster.v1.PendingInteraction
import muster.v1.QuestionAnswer
import muster.v1.agentAttachment
import muster.v1.questionAnswer
import java.io.ByteArrayOutputStream
import java.text.NumberFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.UUID
import kotlin.math.max
import kotlin.math.min

// Agent sessions: the list across devices, and the conversation with one
// session or desktop chat. The core builds the timeline (messages, grouped
// tool activity, turn summaries) and owns every send and answer; this file
// draws them and collects the user's input. Options shown for an interaction
// are exactly what the provider offered (SECURITY I7).

/** "Mon DD HH:MM:SS" in local time. */
fun stamp(unix: Long): String = SimpleDateFormat("MMM d HH:mm:ss", Locale.US).format(Date(unix * 1000))

private fun grouped(n: Long): String = NumberFormat.getIntegerInstance(Locale.US).format(n)

/** "now", "5 min. ago", "3 hr. ago", "2 days ago" or a date. */
fun relativeTime(unix: Long): String {
    if (unix <= 0) return ""
    val seconds = max(0L, System.currentTimeMillis() / 1000 - unix)
    return when {
        seconds < 60 -> "now"
        seconds < 3600 -> "${seconds / 60} min. ago"
        seconds < 86400 -> "${seconds / 3600} hr. ago"
        seconds < 7 * 86400 -> (seconds / 86400).let { if (it == 1L) "1 day ago" else "$it days ago" }
        else -> SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date(unix * 1000))
    }
}

/** The subtitle under a conversation's title: provider and device. */
fun chatSubtitle(chat: ChatView): String =
    listOf(chat.providerLabel, chat.session.deviceName).filter { it.isNotEmpty() }.joinToString(" · ")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentsScreen(core: MusterCore, vm: ViewModel) {
    val agents = vm.agents
    /** The session shown beside the list (a split workspace). */
    val shown = if (vm.screen == Screen.CHAT) "${vm.chat.deviceId}:${vm.chat.session.id}" else ""
    Column(Modifier.fillMaxSize()) {
        DeviceTabs(vm, agents.deviceId, select = { core.openAgents(it) }, details = { core.selectDevice(it) })
        PullToRefreshBox(isRefreshing = false, onRefresh = { core.openAgents(agents.deviceId) }, modifier = Modifier.fillMaxWidth().weight(1f)) {
            LazyColumn(Modifier.fillMaxSize()) {
                items(agents.errorsList, key = { "error-$it" }) { ErrorLine(it) }
                if (agents.sessionsList.isEmpty()) {
                    item(key = "empty") {
                        if (agents.loading) {
                            CircularProgressIndicator(Modifier.padding(24.dp))
                        } else {
                            Text("Codex and Claude sessions on the selected devices show up here.", Modifier.padding(24.dp),
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
                items(agents.sessionsList, key = { "${it.deviceId}:${it.id}" }) { session ->
                    val key = "${session.deviceId}:${session.id}"
                    SessionRow(
                        session,
                        Modifier.background(if (shown == key) MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f) else Color.Transparent)
                            .clickable { core.openAgent(session.id, session.deviceId) },
                    )
                    HorizontalDivider(Modifier.padding(horizontal = 16.dp))
                }
            }
        }
    }
}

@Composable
fun SessionRow(session: AgentSession, modifier: Modifier = Modifier) {
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    Column(modifier.fillMaxWidth().padding(16.dp, 10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                capitalized(session.provider),
                Modifier.background(if (session.provider == "codex") Color(0x261E88E5) else Color(0x26FB8C00), RoundedCornerShape(8.dp))
                    .padding(6.dp, 2.dp),
                style = MaterialTheme.typography.labelSmall,
            )
            Text(session.title.ifEmpty { session.id }, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Column(horizontalAlignment = Alignment.End) {
                StateBadge(session.state)
                if (session.processState == "running" && session.state == "unknown") {
                    Text("process alive", style = MaterialTheme.typography.labelSmall, color = secondary)
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            if (session.deviceName.isNotEmpty()) {
                Text(session.deviceName, style = MaterialTheme.typography.bodySmall, fontWeight = FontWeight.Medium)
            }
            Text(session.cwd, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall, color = secondary,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(relativeTime(session.updatedUnix), style = MaterialTheme.typography.bodySmall, color = secondary)
        }
    }
}

@Composable
fun StateBadge(state: String) {
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    when (state) {
        "running" -> Text("● working", style = MaterialTheme.typography.labelSmall, color = Color(0xFF43A047))
        "waiting_approval" -> Text("✋ needs you", style = MaterialTheme.typography.labelSmall, color = Color(0xFFFB8C00))
        "idle", "ended" -> Text(state, style = MaterialTheme.typography.labelSmall, color = secondary)
        else -> Text("unknown", style = MaterialTheme.typography.labelSmall, color = secondary)
    }
}

@Composable
fun ChatScreen(core: MusterCore, vm: ViewModel) {
    val chat = vm.chat
    val layout = LocalWorkspaceLayout.current
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    // A desktop chat keeps its draft when the core moves it to another Mac
    // (every Mac shows the same conversation).
    val sessionKey = if (chat.session.mode == "chat") "chat:${chat.session.id}" else "${chat.deviceId}:${chat.session.id}"
    var atLatest by remember(sessionKey) { mutableStateOf(true) }
    var contentTab by remember(sessionKey) { mutableStateOf("Conversation") }
    var activityTab by remember(sessionKey) { mutableStateOf("Tracing") }
    CodePreviewDialog(chat, core) { contentTab = "Terminal" }
    val leading = (if (chat.loading && chat.messageCount == 0) 1 else 0) +
        (if (chat.loadError.isNotEmpty()) 1 else 0) +
        (if (!chat.loading && chat.messageCount == 0 && chat.loadError.isEmpty()) 1 else 0) +
        (if (chat.hasEarlier) 1 else 0)
    // Top and bottom: the cards sit in the lower pane, with the composer.
    val total = leading + chat.timelineList.size + (if (layout.flex) 0 else chat.interactionsList.size)
    suspend fun toLatest() {
        if (total > 0) { list.scrollToItem(total - 1); list.scrollBy(100000f) }
    }
    // Follow new messages and cards while the reader is at the latest one.
    LaunchedEffect(sessionKey, total) { if (atLatest) toLatest() }
    LaunchedEffect(list) {
        snapshotFlow { list.isScrollInProgress }.collect { scrolling ->
            if (!scrolling) {
                val info = list.layoutInfo
                atLatest = info.totalItemsCount == 0 || info.visibleItemsInfo.lastOrNull()?.index == info.totalItemsCount - 1
            }
        }
    }
    Column(Modifier.fillMaxSize()) {
        if (chat.session.parentSessionId.isNotEmpty()) {
            TextButton(onClick = { core.openAgent(chat.session.parentSessionId, chat.deviceId) }) { Text("Parent session") }
        }
        Text("${chat.providerLabel} · ${chat.session.deviceName}", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
        Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(14.dp)).padding(4.dp)) {
            listOf("Conversation", "Activity", "Terminal").forEach { tab ->
                TextButton(modifier = Modifier.weight(1f).background(if (contentTab == tab) MaterialTheme.colorScheme.primaryContainer else Color.Transparent, RoundedCornerShape(10.dp)), enabled = tab != "Terminal" || chat.terminal.available, onClick = {
                    if (tab != contentTab) {
                        if (tab == "Terminal") core.sessionTerminal(0)
                        else if (contentTab == "Terminal") core.sessionTerminal(3)
                        contentTab = tab
                    }
                }) {
                    Text(if (tab == "Conversation") "Chat" else tab, maxLines = 1, fontWeight = if (contentTab == tab) FontWeight.Bold else FontWeight.Normal)
                }
            }
        }
        if (contentTab == "Terminal") {
            CodeRunHistory(chat, core)
            Text(chat.terminal.cwd, style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(8.dp))
            Row {
                TextButton(enabled = !chat.terminal.busy && !chat.terminal.attached && chat.terminal.available, onClick = { core.sessionTerminal(0) }) { Text("Start") }
                TextButton(enabled = chat.terminal.canEnd, onClick = { core.sessionTerminal(1) }) { Text("End") }
                TextButton(enabled = chat.terminal.canRestart, onClick = { core.sessionTerminal(2) }) { Text("Restart") }
            }
            if (chat.terminal.busy) Text("Starting terminal…")
            if (chat.terminal.error.isNotEmpty()) Text(chat.terminal.error, color = MaterialTheme.colorScheme.error)
            if (chat.terminal.attached || vm.terminal.ended) TerminalScreen(core, vm)
            else if (!chat.terminal.busy) Text("Terminal stopped")
            return@Column
        }
        if (contentTab == "Activity") {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                listOf("Tracing", "Subagents", "Sessions").forEach { tab ->
                    TextButton(onClick = { activityTab = tab }) { Text(tab, fontWeight = if (activityTab == tab) FontWeight.Bold else FontWeight.Normal) }
                }
            }
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (activityTab == "Tracing") {
                    item { Text("Execution trace · durations are approximate", style = MaterialTheme.typography.labelMedium) }
                    if (chat.hasEarlier) item { TextButton(onClick = { core.agentLoadEarlier() }) { Text("Load earlier events") } }
                    if (chat.traceList.isEmpty()) item { Text("No execution events yet") }
                    items(chat.traceList, key = { it.id }) { span ->
                        var expanded by remember(span.id) { mutableStateOf(false) }
                        Column {
                            TextButton(onClick = { expanded = !expanded }) {
                                Text(span.title + " · " + span.status + if (span.durationKnown) " · ≈ ${span.durationSeconds} s" else "")
                            }
                            if (span.startedUnix > 0) Text(relativeTime(span.startedUnix), style = MaterialTheme.typography.labelSmall)
                            if (expanded) Text(span.detail, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                } else {
                    val sessions = if (activityTab == "Subagents") chat.session.subagentsList else chat.session.subsessionsList
                    if (sessions.isEmpty()) item { Text("No ${activityTab.lowercase()}") }
                    items(sessions, key = { it.id }) { child ->
                        TextButton(onClick = { core.openAgent(child.id, chat.deviceId) }) {
                            Column(Modifier.weight(1f)) { Text(child.title.ifEmpty { child.id }); Text(child.relationship, style = MaterialTheme.typography.labelSmall) }
                            StateBadge(child.state)
                        }
                    }
                }
            }
            return@Column
        }
        Box(if (layout.flex) Modifier.height(layout.readingHeight.dp).fillMaxWidth() else Modifier.weight(1f).fillMaxWidth()) {
            LazyColumn(
                Modifier.fillMaxSize(), state = list,
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                if (chat.loading && chat.messageCount == 0) {
                    item(key = "loading") {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                            Text("Loading conversation…", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
                if (chat.loadError.isNotEmpty()) item(key = "load-error") { ErrorLine(chat.loadError) }
                if (!chat.loading && chat.messageCount == 0 && chat.loadError.isEmpty()) {
                    item(key = "empty") { Text("No messages yet", color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                if (chat.hasEarlier) {
                    item(key = "earlier") {
                        TextButton(onClick = { atLatest = false; core.agentLoadEarlier() }, Modifier.fillMaxWidth()) { Text("Load earlier") }
                    }
                }
                items(chat.timelineList, key = { it.id }) { block -> BlockView(block, core) }
                if (!layout.flex) {
                    items(chat.interactionsList, key = { "interaction-${it.id}" }) { interaction ->
                        InteractionCard(
                            interaction,
                            submitting = chat.respondingInteraction.isNotEmpty(),
                            error = chat.responseError,
                            outcomeUnknown = chat.responseUnknown,
                        ) { option, answers -> core.agentRespond(interaction.id, option, "", answers) }
                    }
                }
            }
            if (!atLatest) {
                SmallFloatingActionButton(
                    onClick = { scope.launch { atLatest = true; toLatest() } },
                    Modifier.align(Alignment.BottomCenter).padding(bottom = 8.dp),
                ) { Icon(Icons.Default.KeyboardArrowDown, "Jump to latest") }
            }
        }
        // Top and bottom: the composer, actions and cards scroll on their own below the crease.
        if (layout.flex) Spacer(Modifier.height(layout.hingeHeight.dp))
        val lower = if (layout.flex) Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()) else Modifier.fillMaxWidth()
        Column(lower.padding(12.dp, 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (chat.queue.messagesList.isNotEmpty() || chat.queue.error.isNotEmpty()) {
                QueuedMessagesCard(chat.queue) { id, option -> core.agentRespond(id, option) }
            }
            if (chat.lockedUseStatus.isNotEmpty()) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text(chat.lockedUseStatus, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
                    if (chat.canCancelLockedUse) TextButton(onClick = { core.cancelLockedUse() }) { Text("Cancel") }
                }
            }
            // One composer per conversation: a new key starts an empty draft.
            key(sessionKey) { ChatComposer(core, chat, stacked = layout.flex) }
            if (layout.flex) {
                chat.interactionsList.forEach { interaction ->
                    key(interaction.id) {
                        InteractionCard(
                            interaction,
                            submitting = chat.respondingInteraction.isNotEmpty(),
                            error = chat.responseError,
                            outcomeUnknown = chat.responseUnknown,
                        ) { option, answers -> core.agentRespond(interaction.id, option, "", answers) }
                    }
                }
            }
        }
    }
}

@Composable
private fun BlockView(block: ChatBlock, core: MusterCore) {
    when (block.kind) {
        "working" -> WorkingGroup(block, core)
        "activity" -> ActivityGroup(block)
        "compact_context" -> CompactContext(block.message, core)
        "summary" -> Column(Modifier.fillMaxWidth()) {
            TurnChanges(block.message) { path -> core.openChange(block.message.id, path) }
            TurnFooter(block.message)
        }
        else -> MessageBubble(block.message, core, block.partsList)
    }
}

/** The core supplies the queue, order, actions and outcomes. This card renders it. */
@Composable
private fun QueuedMessagesCard(queue: QueuedMessagesView, sendNow: (String, String) -> Unit) {
    Column(
        Modifier.fillMaxWidth().border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(12.dp)).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("待发消息 · ${queue.messagesCount}", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Column(Modifier.heightIn(max = 180.dp).verticalScroll(rememberScrollState())) {
            queue.messagesList.forEachIndexed { index, message ->
                if (index > 0) HorizontalDivider()
                Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(message.prompt, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, maxLines = 3, overflow = TextOverflow.Ellipsis)
                    OutlinedButton(
                        onClick = { message.optionsList.firstOrNull()?.let { sendNow(message.id, it) } },
                        enabled = !queue.submitting && !queue.outcomeUnknown && message.optionsList.isNotEmpty(),
                    ) { Text("Send now", style = MaterialTheme.typography.labelSmall) }
                }
            }
        }
        if (queue.error.isNotEmpty()) Text(queue.error, style = MaterialTheme.typography.bodySmall, color = Color(0xFFFB8C00))
    }
}

private class PickedAttachment(val value: AgentAttachment, val image: Boolean) {
    val id: String = UUID.randomUUID().toString()
}

private fun displayName(context: Context, uri: Uri): String {
    context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0)?.takeIf { it.isNotEmpty() }?.let { return it }
    }
    return uri.lastPathSegment?.substringAfterLast('/') ?: "file"
}

private fun mimeType(context: Context, uri: Uri, name: String): String =
    context.contentResolver.getType(uri)
        ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase())
        ?: "application/octet-stream"

/** At most `limit + 1` bytes, so an oversized file is refused without being held whole. */
private fun readUpTo(context: Context, uri: Uri, limit: Int): ByteArray =
    context.contentResolver.openInputStream(uri)?.use { stream ->
        val out = ByteArrayOutputStream()
        val buffer = ByteArray(64 * 1024)
        var remaining = limit + 1
        while (remaining > 0) {
            val n = stream.read(buffer, 0, min(buffer.size, remaining))
            if (n < 0) break
            out.write(buffer, 0, n)
            remaining -= n
        }
        out.toByteArray()
    } ?: throw IllegalStateException("cannot read the file")

/**
 * The draft and its attachments. Sending hands both to the core with a
 * draft id; the draft is cleared only when the core reports that id as sent,
 * so a failed send keeps what the user wrote.
 */
@Composable
fun ChatComposer(core: MusterCore, chat: ChatView, modifier: Modifier = Modifier, stacked: Boolean = false) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var text by remember { mutableStateOf(chat.draft.text) }
    /** Changes with every keystroke; the core clears a draft by its exact revision. */
    var draftRevision by remember { mutableStateOf(chat.draft.revision.ifEmpty { UUID.randomUUID().toString() }) }
    var attachments by remember { mutableStateOf(listOf<PickedAttachment>()) }
    var importing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    var preview by remember { mutableStateOf<PickedAttachment?>(null) }
    var submittedDraft by remember { mutableStateOf("") }
    var submittedRevision by remember { mutableStateOf("") }
    val busy = chat.sending || importing
    val empty = text.isBlank() && attachments.isEmpty()
    /** Rust sets the limit to zero where attachments are not offered (desktop chats). */
    val acceptsAttachments = chat.attachmentCountLimit > 0

    // The draft the core issued for this conversation, read with every intent
    // sent while this composer is on screen. The key is the core's, never a
    // path the shell made up.
    val draftKey = chat.draft.key
    DisposableEffect(draftKey) {
        val id = core.composerDraft.register { chatDraft { key = draftKey; this.text = text; revision = draftRevision } }
        onDispose { core.composerDraft.unregister(id) }
    }
    // Older snapshots cannot overwrite newer keystrokes: only this exact
    // revision, which the core clears when a send finishes after re-entry.
    LaunchedEffect(chat.draft) {
        if (chat.draft.revision == draftRevision && chat.draft.text != text) text = chat.draft.text
    }
    LaunchedEffect(chat.sentDraftId) {
        val receipt = chat.sentDraftId
        if (receipt.isEmpty() || receipt != submittedDraft) return@LaunchedEffect
        if (draftRevision == submittedRevision) { text = ""; attachments = emptyList(); error = "" }
        submittedDraft = ""
    }

    /** `releasing`: the unconfirmed send's token, once the user has checked the Mac. */
    fun send(releasing: String = "") {
        if (empty || busy) return
        submittedDraft = UUID.randomUUID().toString()
        submittedRevision = draftRevision
        core.agentSend(text, attachments.map { it.value }, submittedDraft, releasing)
    }

    /** Reads a picked file within the core's advertised limits. */
    fun read(uri: Uri, image: Boolean, current: List<PickedAttachment>): PickedAttachment {
        val countLimit = chat.attachmentCountLimit
        val bytesLimit = chat.attachmentBytesLimit
        val used = current.sumOf { it.value.data.size().toLong() }
        val name = displayName(context, uri)
        val data = readUpTo(context, uri, bytesLimit.coerceAtMost(Int.MAX_VALUE.toLong() - 1).toInt())
        if (current.size >= countLimit || used + data.size > bytesLimit) {
            throw IllegalStateException("Up to $countLimit files, ${bytesText(bytesLimit)} total.")
        }
        val type = mimeType(context, uri, name)
        val value = agentAttachment { this.name = name; mediaType = type; this.data = ByteString.copyFrom(data) }
        return PickedAttachment(value, image || type.startsWith("image/"))
    }

    fun addAll(uris: List<Uri>, images: Boolean) {
        if (uris.isEmpty()) return
        scope.launch {
            importing = true
            try {
                for (uri in uris) {
                    try {
                        val current = attachments
                        val picked = withContext(Dispatchers.IO) { read(uri, images, current) }
                        attachments = current + picked
                        error = ""
                    } catch (failure: Exception) {
                        error = failure.message ?: "cannot read the file"
                    }
                }
            } finally {
                importing = false
            }
        }
    }

    val photoContract = remember(chat.attachmentCountLimit) { ActivityResultContracts.PickMultipleVisualMedia(max(2, chat.attachmentCountLimit)) }
    val pickPhotos = rememberLauncherForActivityResult(photoContract) { uris -> addAll(uris, true) }
    val fileContract = remember { ActivityResultContracts.OpenMultipleDocuments() }
    val pickFiles = rememberLauncherForActivityResult(fileContract) { uris -> addAll(uris, false) }

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (chat.hasUnconfirmed()) UnconfirmedCard(chat.unconfirmed, enabled = !empty && !busy) { send(it) }
        if (text.isEmpty() && chat.composerSuggestion.isNotEmpty()) {
            TextButton(onClick = { text = chat.composerSuggestion; draftRevision = UUID.randomUUID().toString() }, enabled = !busy) {
                Text(chat.composerSuggestion, Modifier.weight(1f), textAlign = androidx.compose.ui.text.style.TextAlign.Start)
                Text("Use", Modifier.padding(start = 12.dp))
            }
        }
        Surface(
            Modifier.fillMaxWidth(), shape = RoundedCornerShape(24.dp),
            color = MaterialTheme.colorScheme.surfaceVariant,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        ) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (attachments.isNotEmpty()) {
                    Row(Modifier.horizontalScroll(rememberScrollState()).padding(2.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        attachments.forEach { item ->
                            key(item.id) {
                                AttachmentChip(item, busy, onPreview = { preview = item }, onRemove = { attachments = attachments - item })
                            }
                        }
                    }
                }
                // Top and bottom: the keyboard is up from the start, below the crease.
                MusterTextField(
                    text = text,
                    onChange = { text = it; draftRevision = UUID.randomUUID().toString() },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = chat.composerPlaceholder.ifEmpty { "Follow up" },
                    enabled = !busy,
                    outlined = false,
                    maxLines = 8,
                    autoFocus = stacked,
                    compactKeyboard = stacked,
                    colors = TextFieldDefaults.colors(
                        focusedContainerColor = Color.Transparent, unfocusedContainerColor = Color.Transparent,
                        disabledContainerColor = Color.Transparent, focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent, disabledIndicatorColor = Color.Transparent,
                    ),
                )

                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (acceptsAttachments) {
                        var menu by remember { mutableStateOf(false) }
                        Box {
                            IconButton(onClick = { menu = true }, enabled = !busy) { Icon(Icons.Default.Add, "Add attachment") }
                            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                DropdownMenuItem(text = { Text("Photo library") }, onClick = {
                                    menu = false
                                    pickPhotos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                                })
                                DropdownMenuItem(text = { Text("Add files") }, onClick = { menu = false; pickFiles.launch(arrayOf("*/*")) })
                            }
                        }
                    }
                    Spacer(Modifier.weight(1f))
                    if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    if (chat.canInterrupt && empty && !chat.sending) {
                        IconButton(onClick = { core.agentInterrupt() }) { Icon(Icons.Default.Stop, "Stop", tint = Color(0xFF3D63ED)) }
                    } else {
                        IconButton(onClick = { send() }, enabled = !empty && !busy) {
                            Icon(Icons.Default.ArrowUpward, "Send message", Modifier.alpha(if (!empty && !busy) 1f else 0.35f))
                        }
                    }
                }
            }
        }
        if (error.isNotEmpty()) {
            Text(error, Modifier.padding(start = 8.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
        }
    }
    preview?.let { item ->
        Dialog(onDismissRequest = { preview = null }) {
            Surface(shape = RoundedCornerShape(16.dp)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(item.value.name, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall, maxLines = 2)
                        TextButton(onClick = { preview = null }) { Text("Done") }
                    }
                    val bitmap = remember(item.id) { decode(item.value.data.toByteArray(), 2) }
                    if (bitmap != null) Image(bitmap, item.value.name, Modifier.fillMaxWidth(), contentScale = ContentScale.Fit)
                }
            }
        }
    }
}

private fun decode(bytes: ByteArray, sample: Int) = runCatching {
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
}.getOrNull()

@Composable
private fun AttachmentChip(item: PickedAttachment, busy: Boolean, onPreview: () -> Unit, onRemove: () -> Unit) {
    Row(
        Modifier.background(MaterialTheme.colorScheme.surface, RoundedCornerShape(12.dp)).padding(6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val thumbnail = if (item.image) remember(item.id) { decode(item.value.data.toByteArray(), 8) } else null
        if (thumbnail != null) {
            Image(thumbnail, null, Modifier.size(56.dp).clip(RoundedCornerShape(8.dp)).clickable { onPreview() }, contentScale = ContentScale.Crop)
        } else {
            Icon(Icons.AutoMirrored.Filled.InsertDriveFile, null, Modifier.size(40.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(item.value.name, Modifier.widthIn(max = 140.dp), style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(bytesText(item.value.data.size().toLong()), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        IconButton(onClick = onRemove, Modifier.size(28.dp), enabled = !busy) {
            Icon(Icons.Default.Close, "Remove ${item.value.name}", Modifier.size(14.dp))
        }
    }
}

/**
 * The Mac refused the last send: an earlier one has no receipt from the
 * desktop app. Once the user has looked at the conversation on the Mac
 * and it is not there, this draft goes again and releases it.
 */
@Composable
private fun UnconfirmedCard(refusal: UnconfirmedSend, enabled: Boolean, send: (String) -> Unit) {
    Column(
        Modifier.fillMaxWidth().background(Color(0x1FFB8C00), RoundedCornerShape(14.dp)).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text("上一条消息还没有在 Mac 上确认送达", style = MaterialTheme.typography.titleSmall)
        Text("请先在 Mac 上查看这个对话。那条消息确实没有出现，再继续发送。", style = MaterialTheme.typography.bodySmall)
        Text(refusal.message, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Button(
            onClick = { send(refusal.token) }, enabled = enabled,
            colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFFB8C00)),
        ) { Text("确认未送达，继续发送") }
    }
}

/** Draft selections are user input only; Shared Rust validates and submits them. */
@Composable
fun InteractionCard(
    interaction: PendingInteraction,
    submitting: Boolean,
    error: String,
    outcomeUnknown: Boolean,
    respond: (String, List<QuestionAnswer>) -> Unit,
) {
    var expanded by remember(interaction.id) { mutableStateOf(false) }
    val selections = remember(interaction.id) { mutableStateMapOf<String, List<String>>() }
    val custom = remember(interaction.id) { mutableStateListOf<String>() }
    val answers = remember(interaction.id) { mutableStateMapOf<String, String>() }
    val enabled = !submitting && !outcomeUnknown

    fun selected(question: String, option: String) = selections[question]?.contains(option) == true

    fun toggle(question: InteractionQuestion, option: String) {
        val current = selections[question.id] ?: emptyList()
        if (question.multiSelect) {
            selections[question.id] = if (option in current) current - option else current + option
        } else {
            selections[question.id] = listOf(option)
            custom.remove(question.id)
        }
    }

    fun toggleOther(question: InteractionQuestion) {
        if (question.id in custom) {
            custom.remove(question.id)
        } else {
            custom.add(question.id)
            if (!question.multiSelect) selections[question.id] = emptyList()
        }
    }

    fun writes(question: InteractionQuestion) = question.id in custom || question.optionsList.isEmpty()

    fun submit() {
        respond("", interaction.questionsList.map { question ->
            questionAnswer {
                questionId = question.id
                optionIds.addAll(question.optionsList.filter { selected(question.id, it.id) }.map { it.id })
                text = if (writes(question)) answers[question.id] ?: "" else ""
            }
        })
    }

    @Composable
    fun choice(multi: Boolean, on: Boolean, label: String, detail: String, onToggle: () -> Unit) {
        Row(
            Modifier.fillMaxWidth()
                .border(1.dp, if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(10.dp))
                .background(if (on) MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.3f) else MaterialTheme.colorScheme.surface, RoundedCornerShape(10.dp))
                .clickable(enabled = enabled) { onToggle() }.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (multi) Checkbox(checked = on, onCheckedChange = null, enabled = enabled) else RadioButton(selected = on, onClick = null, enabled = enabled)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(label, fontWeight = if (on) FontWeight.Medium else FontWeight.Normal)
                if (detail.isNotEmpty()) Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }

    @Composable
    fun questionView(question: InteractionQuestion, index: Int) {
        val secondary = MaterialTheme.colorScheme.onSurfaceVariant
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (index > 0) HorizontalDivider()
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("${index + 1} / ${interaction.questionsCount}", style = MaterialTheme.typography.bodySmall, color = secondary)
                if (question.header.isNotEmpty()) Text(question.header, style = MaterialTheme.typography.bodySmall, color = secondary)
                Spacer(Modifier.weight(1f))
                Text(if (question.multiSelect) "Select all that apply" else "Select one", style = MaterialTheme.typography.bodySmall, color = secondary)
            }
            Text(question.prompt, fontWeight = FontWeight.Medium)
            question.optionsList.forEach { option ->
                key(option.id) {
                    choice(question.multiSelect, selected(question.id, option.id), option.label, option.description) { toggle(question, option.id) }
                }
            }
            if (question.allowText) {
                if (question.optionsList.isNotEmpty()) {
                    choice(question.multiSelect, question.id in custom, "Other", "输入自己的回答") { toggleOther(question) }
                }
                if (writes(question)) {
                    MusterTextField(
                        answers[question.id] ?: "", { answers[question.id] = it }, Modifier.fillMaxWidth(),
                        placeholder = "Your answer", enabled = enabled, minHeight = 60.dp,
                    )
                }
            }
        }
    }

    @Composable
    fun card() {
        Column(
            Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(18.dp)).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text(
                if (interaction.optional) "可选回答" else if (interaction.kind == "question") "Your input is needed" else "Approval needed",
                style = MaterialTheme.typography.titleMedium,
            )
            if (interaction.questionsList.isEmpty()) {
                Text(interaction.prompt)
                // Exactly the provider's options (SECURITY I7).
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    interaction.optionsList.forEach { option ->
                        OutlinedButton(onClick = { respond(option, emptyList()) }, enabled = enabled) { Text(option) }
                    }
                }
            } else {
                interaction.questionsList.forEachIndexed { index, question -> key(question.id) { questionView(question, index) } }
                Button(onClick = { submit() }, Modifier.fillMaxWidth(), enabled = enabled) {
                    if (submitting) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = Color.White)
                    Text(if (submitting) "  Submitting…" else "Submit answers")
                }
            }
            if (error.isNotEmpty()) Text(error, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
            if (outcomeUnknown) {
                Text("Check the answer in the desktop app before trying again.", style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }

    if (interaction.optional) {
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(Modifier.clickable { expanded = !expanded }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("可选回答", color = MaterialTheme.colorScheme.onSurfaceVariant)
                Chevron(expanded)
            }
            if (expanded) card()
        }
    } else {
        card()
    }
}

@Composable
private fun Chevron(expanded: Boolean) {
    Icon(if (expanded) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown, null, Modifier.size(16.dp),
        tint = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** A turn's working phase: commentary and tool groups, folded once the turn completes. */
@Composable
fun WorkingGroup(block: ChatBlock, core: MusterCore?) {
    if (block.childrenCount == 0) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
            Text(block.title, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }
    /** The reader's choice; until then the core's default_expanded applies. */
    var toggled by remember(block.id) { mutableStateOf<Boolean?>(null) }
    val expanded = toggled ?: block.defaultExpanded
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(
            Modifier.fillMaxWidth().clickable { toggled = !expanded }.padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (block.completed) Icon(Icons.Default.Check, null, Modifier.size(14.dp), tint = secondary)
            else CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
            Text(block.title, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Chevron(expanded)
        }
        if (expanded) {
            block.childrenList.forEach { child ->
                key(child.id) {
                    if (child.kind == "activity") ActivityGroup(child) else MessageBubble(child.message, core, child.partsList)
                }
            }
        }
    }
}

@Composable
fun ActivityGroup(block: ChatBlock) {
    var expanded by remember(block.id) { mutableStateOf(false) }
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            Modifier.fillMaxWidth().clickable { expanded = !expanded }.padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("⌘", Modifier.width(20.dp), style = MaterialTheme.typography.bodyMedium, color = secondary)
            Text(block.title.ifEmpty { "Tool activity" }, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = secondary, maxLines = 1)
            Chevron(expanded)
        }
        // Shared Rust gives every activity group its rows (calls paired with output).
        if (expanded) block.activityRowsList.forEach { row -> key(row.id) { ToolActivityRow(row) } }
    }
}

@Composable
fun ToolActivityRow(row: muster.app.ChatActivityRow) {
    var expanded by remember(row.id) { mutableStateOf(false) }
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    val symbol = when (row.category) { "read" -> "📖"; "search" -> "🔍"; else -> "›_" }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            Modifier.fillMaxWidth().clickable { expanded = !expanded }.padding(vertical = 3.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(symbol, Modifier.width(20.dp), style = MaterialTheme.typography.bodySmall, color = secondary)
            Text(row.title, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Chevron(expanded)
        }
        if (expanded) {
            Column(
                Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(14.dp)).padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(if (row.category == "shell") "Shell" else row.toolName, style = MaterialTheme.typography.bodySmall, color = secondary)
                Text(row.detail, Modifier.fillMaxWidth().heightIn(max = 280.dp).verticalScroll(rememberScrollState()),
                    fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                if (row.output.isNotEmpty()) {
                    Text("Output", style = MaterialTheme.typography.bodySmall, color = secondary)
                    Text(row.output, Modifier.fillMaxWidth().heightIn(max = 280.dp).verticalScroll(rememberScrollState()),
                        fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
                Text(if (row.isResult) "Output received" else "Arguments", Modifier.fillMaxWidth(),
                    style = MaterialTheme.typography.labelSmall, color = secondary, textAlign = TextAlign.End)
            }
        }
    }
}

@Composable
fun TurnChanges(message: AgentMessage, open: (String) -> Unit) {
    val changes = message.usage.changesList
    if (changes.isEmpty() && !message.usage.changesIncomplete) return
    var expanded by remember(message.id) { mutableStateOf(true) }
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    Column(
        Modifier.fillMaxWidth().padding(top = 10.dp)
            .border(1.dp, Color(0x26808080), RoundedCornerShape(12.dp))
            .background(Color(0x0F808080), RoundedCornerShape(12.dp)).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(Modifier.fillMaxWidth().clickable { expanded = !expanded }, verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("${changes.size} changed files", Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium)
            Chevron(expanded)
        }
        if (expanded) {
            if (message.usage.changesIncomplete) {
                Text("Some recorded changes could not be displayed", style = MaterialTheme.typography.bodySmall, color = secondary)
            }
            changes.forEach { change ->
                key(change.path) {
                    Row(
                        Modifier.fillMaxWidth().clickable { open(change.path) }.padding(vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(change.openPath.ifEmpty { change.path }, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall,
                                maxLines = 2, overflow = TextOverflow.Ellipsis)
                            if (change.previousPath.isNotEmpty()) {
                                Text("From ${change.previousPath}", style = MaterialTheme.typography.labelSmall, color = secondary)
                            }
                        }
                        if (change.countsKnown) {
                            Text("+${change.additions}", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = Color(0xFF43A047))
                            Text("−${change.deletions}", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = Color(0xFFC62828))
                        } else {
                            Text(capitalized(change.kind), style = MaterialTheme.typography.bodySmall, color = secondary)
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun TurnFooter(message: AgentMessage) {
    var details by remember(message.id) { mutableStateOf(false) }
    val usage = message.usage
    val expiry = if (usage.hasCacheExpiry()) usage.cacheExpiry else null

    /** Input, output and cache hit for the whole turn. */
    val tokens = run {
        val counts = "input ${grouped(usage.inputTokens)} · Output ${grouped(usage.outputTokens)}"
        when {
            usage.tokensKnown -> "Turn $counts · Cache hit " + (if (usage.cacheHitKnown) "%.1f%%".format(usage.cacheHitPercent) else "unavailable")
            usage.requestCount > 0 -> "Partial turn $counts · Cache hit unavailable"
            else -> "Token usage unavailable"
        }
    }

    /** The estimated expiry (or range): the time alone on the turn's day, with the date otherwise. */
    fun ends(seconds: Boolean): String {
        if (expiry == null) return ""
        val turnDay = { text: String -> text.substringBeforeLast(' ') }
        val clock = { unix: Long ->
            val full = stamp(unix)
            val time = full.substringAfterLast(' ')
            val shown = if (seconds) time else time.substringBeforeLast(':')
            if (turnDay(full) == turnDay(stamp(message.tsUnix))) shown else "${turnDay(full)} $shown"
        }
        return if (expiry.untilUnix > expiry.fromUnix) "${clock(expiry.fromUnix)}–${clock(expiry.untilUnix)}" else clock(expiry.fromUnix)
    }

    /** The core's TTL estimate and, to the minute, when it lapses. */
    val cache = when {
        expiry == null -> "Cache TTL unavailable"
        expiry.fromUnix <= 0 -> expiry.label
        else -> "${expiry.label} · until ${ends(false)}"
    }

    val detailLines = buildList {
        add("${usage.requestCount} requests · " + if (usage.tokensKnown) {
            "Read ${grouped(usage.cacheReadTokens)} · Write ${grouped(usage.cacheWriteTokens)} tokens"
        } else {
            "Token breakdown unavailable"
        })
        if (expiry != null && expiry.fromUnix > 0) add("Estimated expiry ${ends(true)}")
        if (usage.firstRequestCacheHitKnown) add("First request hit %.1f%%".format(usage.firstRequestCacheHitPercent))
        val gap = expiry?.observedGapSeconds ?: 0L
        if (gap > 0) add("Observed gap before first request: ${grouped(gap)} seconds. Hits and misses do not prove a TTL.")
        if (expiry == null) add("No expiry data received") else if (expiry.detail.isNotEmpty()) add(expiry.detail)
        add("Turn hit rate = cached reads ÷ all input across this turn's requests. Input includes cache reads and writes.")
    }

    val cost = if (!usage.costKnown) "Cost unavailable"
        else "${if (usage.costSource == "recorded") "Recorded" else "Estimated"} $%.5f USD".format(usage.costUsd)

    val small = MaterialTheme.typography.labelSmall
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp).alpha(0.6f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        HorizontalDivider()
        Row(Modifier.fillMaxWidth()) {
            Text(if (message.tsUnix > 0) stamp(message.tsUnix) else "End time unavailable", Modifier.weight(1f), style = small)
            Text(usage.model, style = small, maxLines = 1)
        }
        Text(tokens, style = small, maxLines = 1, overflow = TextOverflow.Ellipsis)
        // One line for the cache; tapping it shows the details.
        Text("${if (details) "▾" else "▸"} $cache", Modifier.fillMaxWidth().clickable { details = !details },
            style = small, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (details) detailLines.forEach { line -> Text(line, Modifier.padding(start = 12.dp), style = small) }
        Text(cost, style = small)
    }
}

@Composable
fun CompactContext(message: AgentMessage, core: MusterCore?) {
    var expanded by remember(message.id) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.clickable { expanded = !expanded }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Compact context", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Chevron(expanded)
        }
        if (expanded) ChatMarkdown(message.text, core)
    }
}

@Composable
fun MessageBubble(message: AgentMessage, core: MusterCore?, parts: List<muster.app.MessagePart> = emptyList()) {
    val user = message.role == "user"
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), horizontalArrangement = if (user) Arrangement.End else Arrangement.Start) {
        if (parts.any { it.kind == "code" }) {
            ConversationParts(parts, core)
        } else if (user) {
            Text(
                message.text,
                Modifier.widthIn(max = 320.dp).background(MaterialTheme.colorScheme.primaryContainer, RoundedCornerShape(20.dp)).padding(16.dp, 10.dp),
                style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onPrimaryContainer,
            )
        } else {
            ChatMarkdown(message.text, core, Modifier.fillMaxWidth())
        }
    }
}
