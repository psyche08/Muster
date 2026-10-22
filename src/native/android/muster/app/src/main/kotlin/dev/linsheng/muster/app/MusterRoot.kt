package dev.linsheng.muster.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.ViewAgenda
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.app.display.FlexPreference
import dev.linsheng.muster.app.display.FoldDisplay
import dev.linsheng.muster.app.display.WorkspaceArea
import dev.linsheng.muster.app.display.WorkspaceLayout
import dev.linsheng.muster.app.display.title
import dev.linsheng.muster.app.display.workspaceLayout
import dev.linsheng.muster.app.input.KeyboardHost
import dev.linsheng.muster.app.input.KeyboardPrefs
import dev.linsheng.muster.app.input.MusterKeyboard
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import dev.linsheng.muster.core.MusterCore
import kotlinx.coroutines.delay
import muster.app.Screen
import muster.app.Toast
import muster.app.ViewModel

@Composable
fun MusterTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme(), content = content)
}

/** The workspace tabs, where the switcher replaces the title (the core never shows HOME: it routes it to AGENTS). */
private val roots = setOf(Screen.DESKTOP_CHATS, Screen.AGENTS, Screen.TERMINALS)

/** What a side-by-side split shows beside the workspace's list. */
private val details = setOf(Screen.CHAT, Screen.FILES, Screen.FILE, Screen.GIT, Screen.TERMINAL)

/** The half-fold split the root resolved for the workspace on screen (top and bottom, or side by side). */
val LocalWorkspaceLayout = compositionLocalOf { WorkspaceLayout() }

/** One composable per screen the core names; the core owns the stack, so back is an intent. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MusterRoot(model: MusterModel) {
    val vm by model.viewModel.collectAsStateWithLifecycle()
    val core = model.core
    val scanRequest by model.scanRequest.collectAsState()
    val authenticating by model.authenticating.collectAsState()
    val context = LocalContext.current
    val density = LocalDensity.current
    var chatOptions by remember { mutableStateOf(false) }
    val preference by model.display.preference.collectAsState()
    val geometry by model.display.geometry.collectAsState()
    val keyboardSession = KeyboardHost.session
    var area by remember { mutableStateOf(WorkspaceArea()) }
    val inAppKeyboardHeight = KeyboardPrefs.keyboardHeight
    val layout = remember(area, geometry, preference, inAppKeyboardHeight) {
        // The in-app keyboard is not the IME, so the OS does not report its height.
        val fold = if (inAppKeyboardHeight > geometry.keyboardHeight) geometry.copy(keyboardHeight = inAppKeyboardHeight) else geometry
        workspaceLayout(area, fold, preference)
    }

    LaunchedEffect(scanRequest) {
        if (scanRequest != null) {
            val options = GmsBarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).build()
            GmsBarcodeScanning.getClient(context, options).startScan()
                .addOnSuccessListener { model.scanned(it.rawValue) }
                .addOnCanceledListener { model.scanned(null) }
                .addOnFailureListener { model.scanned(null) }
        }
    }
    val atRoot = vm.screen in roots
    // The workspace's list on the left, the conversation or terminal on the right.
    val split = layout.side && (atRoot || vm.screen in details)
    // Beside the list a conversation or terminal needs no Back: another row replaces it.
    val showsBack = vm.canGoBack && authenticating == null && !(split && (vm.screen == Screen.CHAT || vm.screen == Screen.TERMINAL))
    // Where the layout button applies: a conversation, a terminal or a split workspace.
    val showsLayout = split || atRoot || vm.screen == Screen.CHAT || vm.screen == Screen.TERMINAL
    // Back is the core's decision; nothing navigates while a local authentication is up.
    BackHandler(enabled = vm.canGoBack && authenticating == null) { core.back() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    if (atRoot || split) {
                        WorkspaceSwitcher(core, vm)
                    } else if (vm.screen == Screen.CHAT) {
                        Column {
                            Text(title(vm), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(chatSubtitle(vm.chat), style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    } else {
                        Text(title(vm), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                },
                navigationIcon = {
                    if (showsBack) {
                        IconButton(onClick = { core.back() }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
                    }
                },
                actions = {
                    if (vm.busy) CircularProgressIndicator(Modifier.padding(end = 8.dp).size(20.dp), strokeWidth = 2.dp)
                    if (showsLayout) LayoutMenu(model.display, preference)
                    when (vm.screen) {
                        Screen.CHAT -> ChatActions(core, vm) { chatOptions = true }
                        Screen.TERMINAL -> IconButton(onClick = { core.closeTerminal() }) { Icon(Icons.Default.Close, "Close") }
                        else -> {}
                    }
                    // Settings holds invites and join approvals; the badge says one waits.
                    if (atRoot) {
                        IconButton(onClick = { core.navigate(Screen.SETTINGS) }) {
                            BadgedBox(badge = { if (vm.pendingList.isNotEmpty()) Badge() }) {
                                Icon(Icons.Default.Settings, if (vm.pendingList.isNotEmpty()) "Settings, a device is waiting to join" else "Settings")
                            }
                        }
                    }
                },
            )
        },
        // The in-app keyboard sits below the content like an IME would, so the
        // workspace shrinks above it and the half-fold layout counts its height.
        bottomBar = {
            keyboardSession?.let { session -> MusterKeyboard(session.target, session.enterLabel, session.compact) }
        },
    ) { padding ->
        Box(
            Modifier.fillMaxSize().padding(padding).onGloballyPositioned { coordinates ->
                val position = coordinates.positionInWindow()
                with(density) {
                    area = WorkspaceArea(
                        left = position.x.toDp().value.toDouble(), top = position.y.toDp().value.toDouble(),
                        width = coordinates.size.width.toDp().value.toDouble(), height = coordinates.size.height.toDp().value.toDouble(),
                    )
                }
            },
        ) {
            CompositionLocalProvider(LocalWorkspaceLayout provides layout) {
                if (split) {
                    Row(Modifier.fillMaxSize()) {
                        Box(Modifier.width(layout.listWidth.dp).fillMaxHeight()) {
                            when (vm.workspace) {
                                Screen.TERMINALS -> TerminalsScreen(core, vm)
                                Screen.DESKTOP_CHATS -> DesktopChatsScreen(core, vm)
                                else -> HomeScreen(core, vm)
                            }
                        }
                        Box(Modifier.width(layout.gapWidth.dp).fillMaxHeight(), contentAlignment = Alignment.Center) { VerticalDivider() }
                        Box(Modifier.weight(1f).fillMaxHeight()) {
                            if (atRoot) WorkspaceWelcome(vm.workspace) else ScreenContent(core, vm, model)
                        }
                    }
                } else {
                    ScreenContent(core, vm, model)
                }
            }
            ToastStack(vm.toastsList, Modifier.align(Alignment.BottomCenter)) { core.dismiss(it) }
            authenticating?.let { reason -> AuthenticatingOverlay(reason, Modifier.align(Alignment.Center)) }
        }
    }
    if (chatOptions) ChatOptionsSheet(core, vm.chat) { chatOptions = false }
}

@Composable
private fun ScreenContent(core: MusterCore, vm: ViewModel, model: MusterModel) {
    when (vm.screen) {
        Screen.ONBOARDING -> OnboardingScreen(core, vm)
        Screen.INVITE -> InviteScreen(core, vm)
        Screen.JOIN -> JoinScreen(core, vm)
        Screen.DEVICE -> DeviceScreen(core, vm)
        Screen.HOME, Screen.AGENTS -> HomeScreen(core, vm)
        Screen.CHAT -> ChatScreen(core, vm)
        Screen.DESKTOP_CHATS -> DesktopChatsScreen(core, vm)
        Screen.TERMINALS -> TerminalsScreen(core, vm)
        Screen.TERMINAL -> TerminalScreen(core, vm)
        Screen.BROWSER -> BrowserScreen(core, vm)
        Screen.SETTINGS -> SettingsScreen(core, vm, model)
        Screen.FILES -> FilesScreen(core, vm)
        Screen.FILE -> FileScreen(core, vm)
        Screen.GIT -> GitScreen(core, vm)
        else -> Text("Unknown screen", Modifier.padding(24.dp))
    }
}

/** The right pane of a split workspace before anything is chosen. */
@Composable
private fun WorkspaceWelcome(workspace: Screen) {
    val text = when (workspace) {
        Screen.TERMINALS -> "Choose a terminal on the left to watch it and type into it."
        Screen.DESKTOP_CHATS -> "Choose a conversation on the left, or start a new one. It is shown in the desktop app on a Mac, and its replies are read from there."
        else -> "Choose a Claude or Codex session on the left to follow it and send it messages."
    }
    Column(Modifier.fillMaxSize().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        Icon(Icons.Default.ViewAgenda, null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(text, Modifier.padding(top = 12.dp).widthIn(max = 420.dp), textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** The layout choices: Automatic follows a foldable's posture, the others force a split or none. */
@Composable
private fun LayoutMenu(display: FoldDisplay, preference: FlexPreference) {
    var open by remember { mutableStateOf(false) }
    IconButton(onClick = { open = true }) { Icon(Icons.Default.ViewAgenda, "Layout") }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        FlexPreference.entries.forEach { option ->
            DropdownMenuItem(
                text = { Text(option.title) },
                leadingIcon = { if (option == preference) Icon(Icons.Default.Check, null) },
                onClick = { open = false; display.select(option) },
            )
        }
    }
}

private fun title(vm: ViewModel): String = when (vm.screen) {
    Screen.ONBOARDING -> "Welcome"
    Screen.INVITE -> "Invite a device"
    Screen.JOIN -> "Joining"
    Screen.DEVICE -> vm.network.membersList.firstOrNull { it.deviceId == vm.selectedDevice }?.name ?: "Device"
    Screen.AGENTS, Screen.HOME -> "Agents"
    Screen.CHAT -> vm.chat.session.title.ifEmpty { "Chat" }
    Screen.DESKTOP_CHATS -> "Chat"
    Screen.TERMINALS -> "Terminals"
    Screen.TERMINAL -> if (vm.terminal.session.isEmpty()) "Terminal" else "${vm.terminal.source} · ${vm.terminal.session.take(8)}"
    Screen.BROWSER -> "Agent Halo Browser"
    Screen.SETTINGS -> "Settings"
    Screen.FILES -> vm.files.path.ifEmpty { "Files" }
    Screen.FILE -> vm.file.path
    Screen.GIT -> if (vm.git.diffCommit.isEmpty()) "Git log" else vm.git.diffCommit.take(8)
    else -> "Muster"
}

/** A conversation's actions, as the Apple shells offer them. */
@Composable
private fun ChatActions(core: MusterCore, vm: ViewModel, options: () -> Unit) {
    val chat = vm.chat
    if (!chat.canChooseModel && !chat.canBrowseFiles && !chat.canInterrupt) return
    var open by remember { mutableStateOf(false) }
    IconButton(onClick = { open = true }) { Icon(Icons.Default.MoreVert, "Conversation actions") }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        if (chat.canChooseModel) {
            DropdownMenuItem(text = { Text("Model and effort") }, onClick = { open = false; options() })
        }
        if (chat.canBrowseFiles) {
            DropdownMenuItem(text = { Text("Files") }, onClick = { open = false; core.openFiles() })
            DropdownMenuItem(text = { Text("Git") }, onClick = { open = false; core.openGit() })
        }
        if (chat.canInterrupt) {
            DropdownMenuItem(text = { Text("Stop") }, onClick = { open = false; core.agentInterrupt() })
        }
    }
}

/** The core's toasts; each dismisses itself like the Apple shells' (4 s, errors 8 s). */
@Composable
fun ToastStack(toasts: List<Toast>, modifier: Modifier = Modifier, dismiss: (String) -> Unit) {
    Column(modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        toasts.forEach { toast ->
            key(toast.id) {
                LaunchedEffect(toast.id) {
                    delay(if (toast.error) 8000 else 4000)
                    dismiss(toast.id)
                }
                Surface(
                    Modifier.fillMaxWidth().clickable { dismiss(toast.id) },
                    shape = RoundedCornerShape(10.dp),
                    color = if (toast.error) Color(0xE6C62828) else Color(0xD9424242),
                ) {
                    Text(toast.text, Modifier.padding(10.dp), color = Color.White, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}

@Composable
fun AuthenticatingOverlay(reason: String, modifier: Modifier = Modifier) {
    Surface(modifier, shape = RoundedCornerShape(14.dp), tonalElevation = 6.dp) {
        Column(Modifier.padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator(Modifier.size(32.dp))
            Text(reason, Modifier.padding(top = 8.dp), style = MaterialTheme.typography.bodyMedium)
        }
    }
}
