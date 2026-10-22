package dev.linsheng.muster.app

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.core.MusterCore
import muster.app.Screen
import muster.app.ViewModel

// The tabs above the lists: which workspace (Chat, Agents, Terminals) and,
// within Agents and Terminals, which device. The core owns both choices;
// these only render them and send the intent.

/** The tab the core says the screen belongs to (Home counts as Agents). */
fun selectedWorkspace(vm: ViewModel): Screen = when (vm.workspace) {
    Screen.DESKTOP_CHATS, Screen.TERMINALS -> vm.workspace
    else -> Screen.AGENTS
}

private val workspaceTabs = listOf("Chat" to Screen.DESKTOP_CHATS, "Agents" to Screen.AGENTS, "Terminals" to Screen.TERMINALS)

@Composable
fun WorkspaceSwitcher(core: MusterCore, vm: ViewModel) {
    val selected = selectedWorkspace(vm)
    Row(Modifier.clip(RoundedCornerShape(16.dp)).background(MaterialTheme.colorScheme.surfaceVariant).padding(2.dp)) {
        workspaceTabs.forEach { (label, screen) ->
            val on = selected == screen
            Text(
                label,
                Modifier.clip(RoundedCornerShape(14.dp))
                    .background(if (on) MaterialTheme.colorScheme.surface else Color.Transparent)
                    .clickable {
                        if (on) return@clickable
                        when (screen) {
                            Screen.DESKTOP_CHATS -> core.openDesktopChats()
                            Screen.TERMINALS -> core.openTerminals(vm.agents.deviceId.ifEmpty { "*" })
                            else -> core.openAgents(vm.terminals.deviceId.ifEmpty { "*" })
                        }
                    }
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                style = MaterialTheme.typography.labelLarge,
                color = if (on) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** All devices, then each device that offers services; a long press opens its details. */
@Composable
fun DeviceTabs(vm: ViewModel, selected: String, select: (String) -> Unit, details: (String) -> Unit, modifier: Modifier = Modifier) {
    LazyRow(
        modifier.fillMaxWidth(),
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item(key = "*") { DeviceTab("All devices", "*", true, selected == "*", select, details) }
        items(vm.serviceDevicesList, key = { it.deviceId }) { member ->
            DeviceTab(member.name, member.deviceId, member.connected || member.isSelf, selected == member.deviceId, select, details)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DeviceTab(name: String, id: String, connected: Boolean, on: Boolean, select: (String) -> Unit, details: (String) -> Unit) {
    Row(
        Modifier.clip(RoundedCornerShape(16.dp))
            .background(if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant)
            .combinedClickable(onClick = { select(id) }, onLongClick = { if (id != "*") details(id) })
            .padding(horizontal = 12.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(name, style = MaterialTheme.typography.labelLarge, maxLines = 1,
            color = if (on) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface)
        if (id != "*") {
            Box(Modifier.size(6.dp).background(
                if (connected) Color(0xFF43A047) else if (on) Color(0xB3FFFFFF) else Color(0x66999999), CircleShape))
        }
    }
}

/** The first letter in upper case: "codex" → "Codex". */
fun capitalized(text: String): String = text.replaceFirstChar { it.uppercaseChar() }

/** A byte count as "512 B", "1.5 KB" or "2.0 MB". */
fun bytesText(v: Long): String = when {
    v < 1024 -> "$v B"
    v < 1024 * 1024 -> "%.1f KB".format(v / 1024.0)
    else -> "%.1f MB".format(v / 1024.0 / 1024.0)
}

/** A caption row inside a list, like a section header. */
@Composable
fun SectionHeader(title: String) {
    Text(title, Modifier.padding(16.dp, 12.dp, 16.dp, 4.dp), style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** A transport or read error shown inline above a list. */
@Composable
fun ErrorLine(text: String) {
    Row(Modifier.fillMaxWidth().padding(16.dp, 6.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(Icons.Default.Warning, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
