package dev.linsheng.muster.app

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.core.MusterCore
import muster.app.ChatView
import muster.app.Screen
import muster.app.ViewModel
import muster.v1.AgentChatOption

// Desktop Chat mode: the Claude app's and the ChatGPT app's ordinary
// conversations on one Mac. Shared Rust reads and drives them through
// Accessibility; this screen shows what the core lists and forwards taps.

/**
 * The Chat tab: one app's conversations. There is no device list: every Mac
 * signed in to the account shows the same chats, so the core picks one that
 * can show them now and names it in the footer.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DesktopChatsScreen(core: MusterCore, vm: ViewModel) {
    val view = vm.desktopChats
    val provider = view.provider
    val appName = view.choicesList.firstOrNull { it.id == provider }?.label ?: ""
    val busy = view.opening.isNotEmpty()
    val picked = view.deviceId.isNotEmpty()
    val catalog = if (view.hasCatalog()) view.catalog else null
    val footer = when {
        picked -> "Through ${view.deviceName}. Every Mac shows the same $appName chats; another one is used when this one is busy."
        view.loading -> "Finding a Mac that can show $appName chats…"
        else -> ""
    }
    PullToRefreshBox(isRefreshing = false, onRefresh = { core.refresh() }, modifier = Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize()) {
            item(key = "header") {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    // The desktop apps with a Chat mode, in the core's order and as it names them.
                    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(MaterialTheme.colorScheme.surfaceVariant).padding(2.dp)) {
                        view.choicesList.forEach { choice ->
                            val on = provider == choice.id
                            Text(
                                choice.label,
                                Modifier.weight(1f).clip(RoundedCornerShape(14.dp))
                                    .background(if (on) MaterialTheme.colorScheme.surface else Color.Transparent)
                                    .clickable { if (!on) core.openDesktopChats(choice.id) }
                                    .padding(vertical = 7.dp),
                                style = MaterialTheme.typography.labelLarge,
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                                color = if (on) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    Button(onClick = { core.desktopChatOpen("") }, Modifier.fillMaxWidth(), enabled = !busy && picked) {
                        Icon(Icons.Default.Edit, null, Modifier.size(16.dp))
                        Text("  New chat")
                    }
                    if (footer.isNotEmpty()) {
                        Text(footer, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
            if (view.error.isNotEmpty()) item(key = "error") { ErrorLine(view.error) }
            if (catalog != null) {
                if (view.loading) item(key = "loading-all") {
                    Row(Modifier.padding(start = 16.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                        Text("Loading all conversations…", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                item(key = "conversations") { SectionHeader("Conversations") }
                if (!catalog.chatMode) {
                    item(key = "chat-mode") {
                        Column(Modifier.padding(16.dp, 4.dp, 16.dp, 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("$appName is not in its Chat mode on ${view.deviceName}.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                            OutlinedButton(onClick = { core.desktopChatOpen("", onDesktop = true) }, enabled = !busy && picked) {
                                Text(if (view.opening == "new") "Switching…" else "Switch to Chat mode")
                            }
                        }
                    }
                } else if (catalog.chatsList.isEmpty()) {
                    item(key = "none") { Text("No conversations", Modifier.padding(16.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                items(catalog.chatsList, key = { it.id }) { chat ->
                    val open = vm.screen == Screen.CHAT && vm.chat.session.id == chat.id
                    Row(
                        Modifier.fillMaxWidth()
                            .background(if (open) MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f) else Color.Transparent)
                            .clickable(enabled = !busy && picked) { core.desktopChatOpen(chat.id) }
                            .padding(16.dp, 12.dp).alpha(if (!busy && picked) 1f else 0.5f),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(chat.title, Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
                        if (view.opening == chat.id) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    }
                    HorizontalDivider(Modifier.padding(horizontal = 16.dp))
                }
            } else if (view.loading) {
                item(key = "loading") { CircularProgressIndicator(Modifier.padding(24.dp)) }
            }
        }
    }
}

/**
 * A desktop chat's model and effort, as the app on the Mac offers them.
 * Opening reads them there (the Mac shows its picker briefly); a tap chooses
 * by name. What cannot be set through Accessibility is shown with the app's
 * reason.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatOptionsSheet(core: MusterCore, chat: ChatView, onDismiss: () -> Unit) {
    LaunchedEffect(Unit) { core.chatOptionsLoad() }
    val options = chat.chatOptions
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Model and effort", Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
            if (chat.chatOptionsBusy) {
                CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
            } else {
                IconButton(onClick = { core.chatOptionsLoad() }) { Icon(Icons.Default.Refresh, "Refresh") }
            }
        }
        LazyColumn(Modifier.fillMaxWidth().fillMaxHeight(0.7f)) {
            if (chat.chatOptionsError.isNotEmpty()) item(key = "error") { ErrorLine(chat.chatOptionsError) }
            item(key = "models") { SectionHeader("Model") }
            if (options.modelsList.isEmpty() && chat.chatOptionsBusy) {
                item(key = "models-loading") { CircularProgressIndicator(Modifier.padding(12.dp).size(24.dp)) }
            }
            items(options.modelsList, key = { "model-${it.name}" }) { option ->
                OptionRow(option, chat.chatOptionsBusy) { core.chatSetOptions(model = option.name) }
            }
            item(key = "efforts") { SectionHeader("Reasoning") }
            if (options.effortsList.isEmpty()) {
                item(key = "effort-only") {
                    Row(Modifier.fillMaxWidth().padding(16.dp, 10.dp)) {
                        Text("Effort", Modifier.weight(1f))
                        Text(options.effort.ifEmpty { "—" }, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
            items(options.effortsList, key = { "effort-${it.name}" }) { option ->
                OptionRow(option, chat.chatOptionsBusy) { core.chatSetOptions(effort = option.name) }
            }
            if (options.note.isNotEmpty()) {
                item(key = "note") {
                    Text(options.note, Modifier.padding(16.dp, 4.dp), style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable
private fun OptionRow(option: AgentChatOption, busy: Boolean, choose: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(enabled = !busy && !option.selected) { choose() }
            .padding(16.dp, 10.dp).alpha(if (busy && !option.selected) 0.5f else 1f),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(option.name)
            if (option.detail.isNotEmpty()) {
                Text(option.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (option.selected) Icon(Icons.Default.Check, null, tint = MaterialTheme.colorScheme.primary)
    }
}
