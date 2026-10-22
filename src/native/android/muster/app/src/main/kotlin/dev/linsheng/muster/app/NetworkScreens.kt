package dev.linsheng.muster.app

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.PersonAdd
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import dev.linsheng.muster.app.display.FlexPreference
import dev.linsheng.muster.app.display.title
import dev.linsheng.muster.app.input.KeyboardPrefs
import dev.linsheng.muster.app.input.MusterTextField
import dev.linsheng.muster.core.MusterCore
import kotlinx.coroutines.delay
import muster.app.AccessItem
import muster.app.AccessView
import muster.app.ViewModel
import muster.v1.MemberView
import muster.v1.PendingJoin
import muster.v1.copy

// Onboarding, home, invite, join, device and settings: the network side.

@Composable
fun OnboardingScreen(core: MusterCore, vm: ViewModel) {
    var creating by remember { mutableStateOf(false) }
    var pasting by remember { mutableStateOf(false) }
    var editingRelay by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("home") }
    var invite by remember { mutableStateOf("") }
    val relayLabel = if (vm.self.relay.isEmpty()) "Relay: not set" else "Relay: ${vm.self.relay}"
    Column(
        Modifier.fillMaxSize().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Muster", style = MaterialTheme.typography.headlineLarge)
        Spacer(Modifier.height(8.dp))
        Text(vm.self.name, style = MaterialTheme.typography.titleMedium)
        Text(vm.self.fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        Spacer(Modifier.height(24.dp))
        Button(onClick = { core.join("", vm.self.name, vm.self.kind) }, Modifier.width(260.dp)) {
            Icon(Icons.Default.QrCode, null); Spacer(Modifier.width(8.dp)); Text("Scan an invite")
        }
        OutlinedButton(onClick = { creating = true }, Modifier.width(260.dp)) { Text("Create a network") }
        TextButton(onClick = { pasting = true }) { Text("Paste an invite") }
        // Creating a network and inviting need a relay; joining takes the invite's.
        TextButton(onClick = { editingRelay = true }) { Text(relayLabel, style = MaterialTheme.typography.bodySmall) }
        if (vm.join.phase != "idle") {
            Spacer(Modifier.height(16.dp))
            Text(vm.join.message, style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
        }
    }
    if (creating) {
        AlertDialog(
            onDismissRequest = { creating = false },
            title = { Text("New network") },
            text = { MusterTextField(name, { name = it }, label = "Network name", singleLine = true) },
            confirmButton = { TextButton(onClick = { core.createNetwork(name.trim()); creating = false }, enabled = name.isNotBlank()) { Text("Create") } },
            dismissButton = { TextButton(onClick = { creating = false }) { Text("Cancel") } },
        )
    }
    if (pasting) {
        AlertDialog(
            onDismissRequest = { pasting = false },
            title = { Text("Join a network") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    MusterTextField(invite, { invite = it }, placeholder = "muster1:…", monospace = true, minHeight = 120.dp, maxHeight = 200.dp)
                    Text("Paste the invite text shown on the admin's device.", style = MaterialTheme.typography.bodySmall)
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    core.join(invite.trim(), vm.self.name, vm.self.kind)
                    invite = ""
                    pasting = false
                }, enabled = invite.trim().startsWith("muster1:")) { Text("Join") }
            },
            dismissButton = { TextButton(onClick = { pasting = false }) { Text("Cancel") } },
        )
    }
    if (editingRelay) {
        AlertDialog(
            onDismissRequest = { editingRelay = false },
            title = { Text("Relay") },
            text = { RelayEditor(core, vm) },
            confirmButton = { TextButton(onClick = { editingRelay = false }) { Text("Done") } },
        )
    }
}

/** The relay this device registers with, in Settings and on the welcome screen. */
@Composable
fun RelayEditor(core: MusterCore, vm: ViewModel) {
    // Every frame is a new ViewModel; only a changed relay replaces the draft.
    var relay by remember(vm.self.relay) { mutableStateOf(vm.self.relay) }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        MusterTextField(relay, { relay = it }, Modifier.fillMaxWidth(), placeholder = "relay.example.com:9443", singleLine = true, monospace = true)
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(vm.self.state.relay, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            TextButton(onClick = { core.setRelay(relay.trim()) }, enabled = relay.isNotBlank() && relay.trim() != vm.self.relay) { Text("Use this relay") }
        }
        Text(
            "Where this device registers so members can reach it from anywhere. TCP/TLS and UDP/QUIC share host:port and are selected automatically. Both verify the server certificate. Joining a network switches to the relay in its invite.",
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** The Agents tab: join requests above the sessions across devices. */
@Composable
fun HomeScreen(core: MusterCore, vm: ViewModel) {
    var deciding by remember { mutableStateOf<PendingJoin?>(null) }
    Column(Modifier.fillMaxSize()) {
        vm.pendingList.forEach { join ->
            PendingJoinRow(join, approve = { deciding = join }, reject = { core.decideJoin(join.deviceId, false, reason = "rejected") })
        }
        AgentsScreen(core, vm)
    }
    deciding?.let { join ->
        AccessEditor("Approve ${join.name}", vm.joinAccess, onDismiss = { deciding = null }) {
            core.decideJoin(join.deviceId, true, it); deciding = null
        }
    }
}

/** A device asking to join, on Home and in Settings. */
@Composable
fun PendingJoinRow(join: PendingJoin, approve: () -> Unit, reject: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant).padding(16.dp, 8.dp, 8.dp, 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(Icons.Default.PersonAdd, null, tint = MaterialTheme.colorScheme.primary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("${join.name} wants to join", style = MaterialTheme.typography.bodyLarge, maxLines = 1)
            Text("${join.kind} · ${join.fingerprint}", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.labelSmall, maxLines = 1)
        }
        Button(onClick = approve) { Text("Approve…") }
        TextButton(onClick = reject) { Text("Reject", color = MaterialTheme.colorScheme.error) }
    }
}

/** A device's release version and source commit as the core reports them. */
private fun buildText(member: MemberView): String = listOf(member.version, member.commit).filter { it.isNotEmpty() }.joinToString(" · ")

@Composable
fun MemberRow(member: MemberView, onClick: () -> Unit) {
    val subtitle = listOf(member.vip, member.link).filter { it.isNotEmpty() }.joinToString(" · ")
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        leadingContent = {
            Icon(when (member.kind) { "phone" -> Icons.Default.Smartphone; "server" -> Icons.Default.Dns; else -> Icons.Default.Computer }, null)
        },
        headlineContent = {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(member.name)
                if (member.isSelf) Text("this device", style = MaterialTheme.typography.labelSmall)
                if (member.role == "admin") Text("admin", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
            }
        },
        supportingContent = {
            Column {
                Text(subtitle)
                if (buildText(member).isNotEmpty()) Text(buildText(member), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.labelSmall)
            }
        },
        trailingContent = {
            if (!member.isSelf) Box(Modifier.size(8.dp).background(if (member.connected) Color(0xFF43A047) else Color(0x66999999), CircleShape))
        },
    )
}

/**
 * Edits a member's access as the core groups it: each module's device-level
 * switch, its providers (disabled while the switch is off), then the rest.
 * Sends back every scope left on; the core validates and signs.
 */
@Composable
fun AccessEditor(title: String, access: AccessView, onDismiss: () -> Unit, onSave: (List<String>) -> Unit) {
    val offered: List<AccessItem> = access.sectionsList.flatMap { (if (it.hasToggle()) listOf(it.toggle) else emptyList()) + it.itemsList }
    val selected = remember { mutableStateOf(offered.filter { it.granted }.map { it.scope }.toSet()) }
    @Composable
    fun row(item: AccessItem, enabled: Boolean, indent: Boolean) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier
            .padding(start = if (indent) 24.dp else 0.dp)
            .clickable(enabled = enabled) {
                selected.value = if (item.scope in selected.value) selected.value - item.scope else selected.value + item.scope
            }) {
            Checkbox(checked = item.scope in selected.value, onCheckedChange = null, enabled = enabled)
            Text(item.label)
        }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            LazyColumn {
                access.sectionsList.forEach { section ->
                    item {
                        Text(section.title, Modifier.padding(top = 12.dp), style = MaterialTheme.typography.labelLarge)
                        if (section.hasToggle()) row(section.toggle, enabled = true, indent = false)
                        section.itemsList.forEach { item ->
                            row(item, enabled = !item.requiresToggle || section.toggle.scope in selected.value, indent = item.requiresToggle)
                        }
                        if (section.footer.isNotEmpty()) Text(section.footer, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = { onSave(offered.map { it.scope }.filter { it in selected.value }) }) { Text("Save") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
fun InviteScreen(core: MusterCore, vm: ViewModel) {
    val clipboard = LocalClipboardManager.current
    var secondsLeft by remember { mutableLongStateOf(0L) }
    LaunchedEffect(vm.invite.expiresUnix) {
        while (true) {
            secondsLeft = vm.invite.expiresUnix - System.currentTimeMillis() / 1000
            delay(1000)
        }
    }
    Column(Modifier.fillMaxSize().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        if (vm.invite.text.isEmpty()) {
            CircularProgressIndicator()
        } else {
            val bitmap = remember(vm.invite.text) { qrBitmap(vm.invite.text) }
            Image(bitmap.asImageBitmap(), "Invite QR code", Modifier.size(280.dp))
            Text(vm.invite.text, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, maxLines = 4)
            Text(if (secondsLeft > 0) "Expires in ${secondsLeft / 60}:${"%02d".format(secondsLeft % 60)}" else "Expired",
                style = MaterialTheme.typography.bodySmall)
            Row {
                TextButton(onClick = { clipboard.setText(AnnotatedString(vm.invite.text)) }) { Text("Copy") }
                TextButton(onClick = { core.showInvite() }) { Text("New invite") }
            }
        }
        Text("Scan on the phone, or paste the text on a desktop. One invite admits one device; approve it here afterwards.",
            style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
    }
}

fun qrBitmap(text: String, size: Int = 640): Bitmap {
    val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size)
    val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.RGB_565)
    for (x in 0 until size) for (y in 0 until size) bitmap.setPixel(x, y, if (matrix[x, y]) android.graphics.Color.BLACK else android.graphics.Color.WHITE)
    return bitmap
}

@Composable
fun JoinScreen(core: MusterCore, vm: ViewModel) {
    Column(Modifier.fillMaxSize().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        if (vm.join.phase != "failed" && vm.join.phase != "joined") CircularProgressIndicator()
        Spacer(Modifier.height(16.dp))
        Text(vm.join.message, textAlign = TextAlign.Center)
        if (vm.join.phase == "failed") TextButton(onClick = { core.back() }) { Text("Back") }
    }
}

@Composable
fun DeviceScreen(core: MusterCore, vm: ViewModel) {
    val member = vm.network.membersList.firstOrNull { it.deviceId == vm.selectedDevice }
    if (member == null) { Text("No such device", Modifier.padding(24.dp)); return }
    /** What this page offers for the member, as the core decided it. */
    val actions = vm.deviceActions
    var editing by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf(false) }
    var ssh by remember { mutableStateOf(false) }
    LazyColumn(Modifier.fillMaxSize()) {
        item { ListItem(headlineContent = { Text("Kind") }, supportingContent = { Text(member.kind) }) }
        item { ListItem(headlineContent = { Text("Role") }, supportingContent = { Text(member.role) }) }
        if (member.vip.isNotEmpty()) item { ListItem(headlineContent = { Text("Virtual IP") }, supportingContent = { Text(member.vip) }) }
        item { ListItem(headlineContent = { Text("Fingerprint") }, supportingContent = { Text(member.fingerprint, fontFamily = FontFamily.Monospace) }) }
        if (member.link.isNotEmpty()) item { ListItem(headlineContent = { Text("Link") }, supportingContent = { Text(member.link) }) }
        // From the link heartbeat; empty until the device reports it.
        item { ListItem(headlineContent = { Text("Version") }, supportingContent = { Text(member.version.ifEmpty { "not reported" }) }) }
        item { ListItem(headlineContent = { Text("Commit") }, supportingContent = { Text(member.commit.ifEmpty { "not reported" }, fontFamily = FontFamily.Monospace) }) }
        vm.deviceLockedUseList.firstOrNull { it.deviceId == member.deviceId }?.let { item { DeviceLockedUseToggle(core, it) } }
        if (vm.selectedDeviceHasServices) {
            item { SectionHeader("Use") }
            item { ListItem(modifier = Modifier.clickable { core.openBrowser(member.deviceId) }, headlineContent = { Text("Chrome browser", color = MaterialTheme.colorScheme.primary) }) }
            item { ListItem(modifier = Modifier.clickable { core.openAgents() }, headlineContent = { Text("Agent sessions", color = MaterialTheme.colorScheme.primary) }) }
            item { ListItem(modifier = Modifier.clickable { core.openTerminals() }, headlineContent = { Text("Terminals", color = MaterialTheme.colorScheme.primary) }) }
            if (actions.canCover) {
                item { ListItem(modifier = Modifier.clickable { core.setCover(true, member.deviceId) }, headlineContent = { Text("Cover its screen", color = MaterialTheme.colorScheme.primary) }) }
                item { ListItem(modifier = Modifier.clickable { core.setCover(false, member.deviceId) }, headlineContent = { Text("Uncover its screen", color = MaterialTheme.colorScheme.primary) }) }
            }
            if (actions.canSsh) {
                item { ListItem(modifier = Modifier.clickable { ssh = true }, headlineContent = { Text("SSH", color = MaterialTheme.colorScheme.primary) }) }
            }
        }
        if (actions.showAccess) {
            item { SectionHeader("Access to other devices") }
            items(vm.deviceAccess.sectionsList, key = { it.id }) { ListItem(headlineContent = { Text(it.title) }, supportingContent = { Text(it.summary) }) }
        }
        if (actions.canEditAccess) {
            item { ListItem(modifier = Modifier.clickable { editing = true }, headlineContent = { Text("Change access…", color = MaterialTheme.colorScheme.primary) }) }
        }
        if (actions.canRemove) {
            item { ListItem(modifier = Modifier.clickable { removing = true }, headlineContent = { Text("Remove from the network", color = MaterialTheme.colorScheme.error) }) }
        }
    }
    if (editing) AccessEditor("Access for ${member.name}", vm.deviceAccess, { editing = false }) {
        core.setScopes(member.deviceId, it, member.role); editing = false
    }
    if (removing) AlertDialog(
        onDismissRequest = { removing = false },
        title = { Text("Remove ${member.name}?") },
        text = { Text("Its links close within two seconds and it cannot rejoin without a new invite.") },
        confirmButton = { TextButton(onClick = { core.removeDevice(member.deviceId); removing = false }) { Text("Remove", color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = { removing = false }) { Text("Cancel") } },
    )
    if (ssh) SshDialog({ ssh = false }) { user, port -> core.openSsh(port, user, 80, 24); ssh = false }
}

/** In-app SSH from this phone to the selected device, opened as a terminal. */
@Composable
fun SshDialog(onDismiss: () -> Unit, onConnect: (String, Int) -> Unit) {
    var user by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("22") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("SSH") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                MusterTextField(user, { user = it }, label = "User", singleLine = true, monospace = true)
                MusterTextField(port, { port = it }, label = "Port", singleLine = true, monospace = true)
                Text("Signs in with this phone's own key: put its public key line (Settings) in the target's authorized_keys.",
                    style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = { TextButton(onClick = { onConnect(user.trim(), port.toIntOrNull() ?: 22) }, enabled = user.isNotBlank()) { Text("Connect") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
fun SettingsScreen(core: MusterCore, vm: ViewModel, model: MusterModel) {
    val clipboard = LocalClipboardManager.current
    var offerAgent by remember(vm.policy) { mutableStateOf(vm.policy.offerAgent) }
    var offerTerminal by remember(vm.policy) { mutableStateOf(vm.policy.offerTerminal) }
    var lockedControl by remember(vm.policy) { mutableStateOf(vm.policy.allowLockedControl) }
    var ports by remember(vm.policy) { mutableStateOf(vm.policy.openPortsList.toList()) }
    var newPort by remember { mutableStateOf("") }
    var deciding by remember { mutableStateOf<PendingJoin?>(null) }
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    LazyColumn(Modifier.fillMaxSize()) {
        item { SectionHeader("Network devices") }
        items(vm.pendingList, key = { "pending-${it.deviceId}" }) { join ->
            PendingJoinRow(join, approve = { deciding = join }, reject = { core.decideJoin(join.deviceId, false, reason = "rejected") })
        }
        items(vm.network.membersList, key = { it.deviceId }) { member ->
            MemberRow(member) { core.selectDevice(member.deviceId) }
            vm.deviceLockedUseList.firstOrNull { it.deviceId == member.deviceId }?.let { DeviceLockedUseToggle(core, it) }
        }
        if (vm.canInvite) {
            item { TextButton(onClick = { core.showInvite() }, Modifier.padding(start = 8.dp)) { Icon(Icons.Default.QrCode, null); Text("  Invite device") } }
        }
        item { SectionHeader("Half-fold workspace") }
        item {
            val preference by model.display.preference.collectAsState()
            Column(Modifier.padding(horizontal = 16.dp)) {
                Text("Top and bottom: read above a horizontal crease, type and respond below it. Side by side: the list beside the conversation or terminal across a vertical crease. Automatic follows how the phone is folded.",
                    style = MaterialTheme.typography.bodySmall, color = secondary)
                FlexPreference.entries.forEach { option ->
                    Row(Modifier.fillMaxWidth().clickable { model.display.select(option) }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(selected = option == preference, onClick = null)
                        Text(option.title, Modifier.padding(start = 8.dp))
                    }
                }
            }
        }
        item {
            ListItem(
                headlineContent = { Text("Muster keyboard") },
                supportingContent = { Text("Pinyin Chinese and English, numbers and symbols in every text field; a MacBook's layout from 560 dp wide. Off, the default, uses the system keyboard.") },
                trailingContent = { Switch(!KeyboardPrefs.systemKeyboard, { KeyboardPrefs.useSystem(!it) }) },
            )
        }
        item {
            ListItem(
                headlineContent = { Text("Muster keyboard in terminals") },
                supportingContent = { Text("On, the default, types straight into the session and adds Esc, F-keys, Ctrl and Alt, which the system keyboard lacks.") },
                trailingContent = { Switch(!KeyboardPrefs.terminalSystemKeyboard, { KeyboardPrefs.useSystemInTerminal(!it) }) },
            )
        }
        if (vm.localDeviceHasServices) {
            item { SectionHeader("Chrome extension") }
            item {
                Column(Modifier.padding(start = 8.dp, end = 16.dp)) {
                    TextButton(onClick = { core.openBrowser(vm.self.deviceId) }) { Text("Set up Agent Halo Browser") }
                    Text("Install the extension, check its connection, and manage browser access.", style = MaterialTheme.typography.bodySmall, color = secondary)
                }
            }
            item { SectionHeader("What this device offers") }
            item { ListItem(headlineContent = { Text("Agent sessions") }, trailingContent = { Switch(offerAgent, { offerAgent = it }) }) }
            item { ListItem(headlineContent = { Text("Terminals") }, trailingContent = { Switch(offerTerminal, { offerTerminal = it }) }) }
            item { ListItem(headlineContent = { Text("Control while the screen is locked") }, trailingContent = { Switch(lockedControl, { lockedControl = it }) }) }
        }
        item { SectionHeader("Open ports") }
        items(ports, key = { "port-$it" }) { port ->
            ListItem(headlineContent = { Text(port, fontFamily = FontFamily.Monospace) },
                trailingContent = { TextButton(onClick = { ports = ports - port }) { Text("Remove") } })
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                MusterTextField(newPort, { newPort = it }, Modifier.weight(1f), label = "tcp/22", singleLine = true, monospace = true)
                TextButton(onClick = { val p = newPort.trim(); if (p.isNotEmpty() && p !in ports) ports = ports + p; newPort = "" }) { Text("Add") }
            }
        }
        item {
            Button(onClick = {
                // Built from the policy this device holds, so saving these
                // three switches leaves what this screen does not edit — the
                // task runtime, the web window — as it was.
                core.setPolicy(vm.policy.copy {
                    this.offerAgent = offerAgent; this.offerTerminal = offerTerminal; allowLockedControl = lockedControl
                    openPorts.clear(); openPorts.addAll(ports)
                })
            }, Modifier.padding(16.dp)) { Text("Save policy") }
        }
        item { SectionHeader("Relay") }
        item { Box(Modifier.padding(horizontal = 16.dp)) { RelayEditor(core, vm) } }
        item { SectionHeader("This device") }
        item {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(vm.self.name)
                Text(vm.self.fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = secondary)
                Text("Key: ${vm.self.keyLabel}", style = MaterialTheme.typography.bodySmall, color = secondary)
                Text("Presence key: ${if (model.keys.presencePublicKey != null) "Android Keystore, biometric" else "none (set a screen lock)"}",
                    style = MaterialTheme.typography.bodySmall, color = secondary)
                Text("Version: ${vm.self.version.ifEmpty { "not reported" }}", style = MaterialTheme.typography.bodySmall, color = secondary)
                Text("Commit: ${vm.self.commit.ifEmpty { "not reported" }}", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = secondary)
            }
        }
        core.sshPublicKeyLine()?.let { line ->
            item {
                ListItem(headlineContent = { Text("SSH public key") },
                    supportingContent = { Text(line, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, maxLines = 3) },
                    trailingContent = { TextButton(onClick = { clipboard.setText(AnnotatedString(line)) }) { Text("Copy") } })
            }
        }
    }
    deciding?.let { join ->
        AccessEditor("Approve ${join.name}", vm.joinAccess, onDismiss = { deciding = null }) {
            core.decideJoin(join.deviceId, true, it); deciding = null
        }
    }
}

@Composable
private fun DeviceLockedUseToggle(core: MusterCore, control: muster.app.DeviceLockedUseView) {
    ListItem(headlineContent = { Text("Locked Use") }, supportingContent = { Text(control.status) },
        trailingContent = { Switch(checked = control.on, enabled = control.enabled,
            onCheckedChange = { core.setDeviceLockedUse(it, control.deviceId) }) })
}
