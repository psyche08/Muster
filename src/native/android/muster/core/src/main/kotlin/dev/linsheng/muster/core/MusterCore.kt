package dev.linsheng.muster.core

import com.google.protobuf.ByteString
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import muster.app.ChatDraft
import muster.app.Effect
import muster.app.EffectResult
import muster.app.Intent
import muster.app.Lifecycle
import muster.app.Screen
import muster.app.ViewModel
import muster.app.agentLoadEarlier
import muster.app.agentRespond
import muster.app.agentSend
import muster.app.browserRun
import muster.app.browserSetup
import muster.app.chatSetOptions
import muster.app.composerAction
import muster.app.createNetwork
import muster.app.decideJoin
import muster.app.desktopChatOpen
import muster.app.dismiss
import muster.app.effectResult
import muster.app.intent
import muster.app.joinNetwork
import muster.app.navigate
import muster.app.openAgent
import muster.app.openAgents
import muster.app.openBrowser
import muster.app.openChange
import muster.app.openDesktopChats
import muster.app.openFile
import muster.app.openFileLink
import muster.app.openFiles
import muster.app.openGit
import muster.app.openSsh
import muster.app.openTerminal
import muster.app.openTerminals
import muster.app.removeDevice
import muster.app.selectDevice
import muster.app.setCover
import muster.app.setPolicy
import muster.app.setPushToken
import muster.app.setRelay
import muster.app.setScopes
import muster.app.showInvite
import muster.app.codeAction
import muster.app.sessionTerminalAction
import muster.app.terminalCreate
import muster.app.terminalKey
import muster.app.terminalPaste
import muster.app.terminalResize
import muster.app.terminalText
import muster.app.uploadDiagnostics
import muster.v1.AgentAttachment
import muster.v1.Policy
import muster.v1.QuestionAnswer
import org.json.JSONObject
import java.util.UUID

/** The JSON `muster_core_new` reads. */
data class MusterConfig(
    val dir: String,
    val mode: String = "embedded",
    val relay: String? = null,
    val name: String = "",
    val kind: String = "phone",
    val bind: String = "0.0.0.0:0",
    val allowLoopback: Boolean = false,
    /** The app's release version the core reports (settings, heartbeat). */
    val version: String = "",
) {
    fun toJson(): ByteArray = JSONObject().apply {
        put("dir", dir)
        put("mode", mode)
        relay?.let { put("relay", it) }
        put("name", name)
        put("kind", kind)
        put("bind", bind)
        put("allow_loopback", allowLoopback)
        if (version.isNotEmpty()) put("version", version)
    }.toString().toByteArray()
}

/** Where the two keys come from; the core sees public points and signatures. */
interface MusterKeys {
    val devicePublicKey: ByteArray?
    fun deviceSign(message: ByteArray): ByteArray?
    val presencePublicKey: ByteArray?
    /** Signs after local authentication (the caller shows the prompt). */
    suspend fun presenceSign(message: ByteArray, reason: String): ByteArray
}

/** No hardware keys: a software device key in the core, no presence proofs. */
object SoftwareKeys : MusterKeys {
    override val devicePublicKey: ByteArray? = null
    override fun deviceSign(message: ByteArray): ByteArray? = null
    override val presencePublicKey: ByteArray? = null
    override suspend fun presenceSign(message: ByteArray, reason: String): ByteArray =
        throw IllegalStateException("no presence key on this device")
}

/**
 * A narrow input callback, owned by the visible composer: it reports its
 * buffer with every intent the shell sends, and Rust decides when to keep
 * it (ComposerAction). Nothing runs on typing.
 */
class ComposerDraftSource {
    private val lock = Any()
    private var source: Pair<String, () -> ChatDraft>? = null

    fun register(read: () -> ChatDraft): String {
        val id = UUID.randomUUID().toString()
        synchronized(lock) { source = id to read }
        return id
    }

    fun unregister(id: String) {
        synchronized(lock) { if (source?.first == id) source = null }
    }

    /** The intent as sent: wrapped with the draft while a composer is on screen. */
    fun attaching(sent: Intent): Intent {
        val read = synchronized(lock) { source?.second } ?: return sent
        val snapshot = read()
        return intent { composerAction = composerAction { draft = snapshot; action = sent } }
    }
}

/**
 * The typed face of the core (ARCHITECTURE §3.7): intents in, view models
 * and effects out. `viewModel` and `effects` emit from the core's threads.
 */
class MusterCore(config: MusterConfig, val keys: MusterKeys) : AutoCloseable {
    private val _viewModel = MutableStateFlow(ViewModel.getDefaultInstance())
    val viewModel: StateFlow<ViewModel> = _viewModel

    private val _effects = MutableSharedFlow<Pair<Long, Effect>>(extraBufferCapacity = 64)
    val effects: SharedFlow<Pair<Long, Effect>> = _effects

    val composerDraft = ComposerDraftSource()

    private val callbacks = object : MusterNative.Callbacks {
        override fun onView(bytes: ByteArray) {
            runCatching { ViewModel.parseFrom(bytes) }.getOrNull()?.let { _viewModel.value = it }
        }

        override fun onEffect(id: Long, bytes: ByteArray) {
            val effect = runCatching { Effect.parseFrom(bytes) }.getOrNull()
            if (effect == null) {
                answer(id, ok = false, error = "undecodable effect")
            } else if (!_effects.tryEmit(id to effect)) {
                answer(id, ok = false, error = "shell busy")
            }
        }

        override fun hardwarePublicKey(): ByteArray? = keys.devicePublicKey
        override fun hardwareSign(message: ByteArray): ByteArray? = keys.deviceSign(message)
        override fun presencePublicKey(): ByteArray? = keys.presencePublicKey
    }

    @Volatile
    private var handle: Long = MusterNative.create(config.toJson(), callbacks)

    val version: String get() = MusterNative.version()

    override fun close() {
        val h = handle
        handle = 0
        if (h != 0L) MusterNative.destroy(h)
    }

    /** Sends one intent; while a composer is on screen its draft rides along. */
    fun send(intent: Intent) {
        val h = handle
        if (h != 0L) MusterNative.intent(h, composerDraft.attaching(intent).toByteArray())
    }

    fun lifecycle(event: Lifecycle) {
        val h = handle
        if (h != 0L) MusterNative.lifecycle(h, event.number)
    }

    /** The answer to an effect. `data` carries a signature, `text` a scan. */
    fun answer(id: Long, ok: Boolean, error: String = "", data: ByteArray = ByteArray(0), text: String = "") {
        val h = handle
        if (h == 0L) return
        val result: EffectResult = effectResult {
            this.ok = ok
            this.error = error
            this.data = ByteString.copyFrom(data)
            this.text = text
        }
        MusterNative.effectResult(h, id, result.toByteArray())
    }

    /** This device's key as an OpenSSH public key line, for a server's authorized_keys. */
    fun sshPublicKeyLine(): String? {
        val h = handle
        return if (h != 0L) MusterNative.sshPublicKey(h) else null
    }

    // Convenience intents, one per app.proto message a phone sends.

    fun refresh() = send(intent { refresh = muster.app.Refresh.getDefaultInstance() })
    fun back() = send(intent { navigate = navigate { back = true } })
    fun navigate(to: Screen) = send(intent { navigate = navigate { screen = to } })
    fun dismiss(toastId: String) = send(intent { dismiss = dismiss { this.toastId = toastId } })
    fun createNetwork(name: String, cidr: String = "") =
        send(intent { createNetwork = createNetwork { this.name = name; vnetCidr = cidr } })
    fun showInvite(ttlSeconds: Int = 0) = send(intent { showInvite = showInvite { ttlSecs = ttlSeconds } })
    fun join(invite: String, deviceName: String, kind: String) =
        send(intent { joinNetwork = joinNetwork { inviteText = invite; this.deviceName = deviceName; deviceKind = kind } })
    fun decideJoin(device: String, approve: Boolean, scopes: List<String> = emptyList(), reason: String = "") =
        send(intent { decideJoin = decideJoin { deviceId = device; this.approve = approve; this.scopes.addAll(scopes); this.reason = reason } })
    fun removeDevice(device: String) = send(intent { removeDevice = removeDevice { deviceId = device } })
    fun setScopes(device: String, scopes: List<String>, role: String) =
        send(intent { setScopes = setScopes { deviceId = device; this.scopes.addAll(scopes); this.role = role } })
    fun setRelay(relay: String) = send(intent { setRelay = setRelay { this.relay = relay } })
    /** Covers (or uncovers) a desktop's screen; device "" for this one. The core checks who may. */
    fun setCover(on: Boolean, device: String = "") = send(intent { setCover = setCover { this.on = on; deviceId = device } })
    fun setPolicy(policy: Policy) = send(intent { setPolicy = setPolicy { this.policy = policy } })
    fun selectDevice(device: String) = send(intent { selectDevice = selectDevice { deviceId = device } })

    /** The Chrome extension on a device ("" for the current target); `draft` prefills a request. */
    fun openBrowser(device: String = "", draft: String = "") =
        send(intent { openBrowser = openBrowser { this.device = device; this.draft = draft } })
    /** Checks the extension's connection; `install` sets up the native host on this device. */
    fun browserSetup(install: Boolean = false) = send(intent { browserSetup = browserSetup { this.install = install } })
    fun browserRun(json: String) = send(intent { browserRun = browserRun { this.json = json } })

    /** device "*" for every device, "" for the current target. */
    fun openAgents(device: String = "") = send(intent { openAgents = openAgents { deviceId = device } })
    fun openAgent(session: String, device: String = "") =
        send(intent { openAgent = openAgent { this.session = session; deviceId = device } })
    /** The Chat tab: provider "claude" (the Claude app) or "codex" (the ChatGPT app), "" for the last one. */
    fun openDesktopChats(provider: String = "") =
        send(intent { openDesktopChats = openDesktopChats { this.provider = provider } })
    /** A listed conversation by id, or a new one (""); `onDesktop` shows a new one on the Mac now. */
    fun desktopChatOpen(chat: String, onDesktop: Boolean = false) =
        send(intent { desktopChatOpen = desktopChatOpen { this.chat = chat; desktop = onDesktop } })
    /** The open desktop chat's model / effort choices, read from the Mac. */
    fun chatOptionsLoad() = send(intent { chatOptionsLoad = muster.app.ChatOptionsLoad.getDefaultInstance() })
    /** Chooses a model and/or effort by name exactly as the Mac offered it. */
    fun chatSetOptions(model: String = "", effort: String = "") =
        send(intent { chatSetOptions = chatSetOptions { this.model = model; this.effort = effort } })
    /** `releaseUnconfirmed`: the token of `ChatView.unconfirmed`, once the user
     *  has checked on the Mac that the earlier send never arrived. */
    fun agentSend(text: String, attachments: List<AgentAttachment> = emptyList(), draftId: String = "", releaseUnconfirmed: String = "") =
        send(intent {
            agentSend = agentSend {
                this.text = text; this.attachments.addAll(attachments); this.draftId = draftId
                this.releaseUnconfirmed = releaseUnconfirmed
            }
        })
    fun agentRespond(interaction: String, option: String = "", answer: String = "", answers: List<QuestionAnswer> = emptyList()) =
        send(intent {
            agentRespond = agentRespond {
                this.interaction = interaction; this.option = option; this.answer = answer; this.answers.addAll(answers)
            }
        })
    fun agentInterrupt() = send(intent { agentInterrupt = muster.app.AgentInterrupt.getDefaultInstance() })
    fun setDeviceLockedUse(on: Boolean, device: String) = send(intent { setDeviceLockedUse = muster.app.SetDeviceLockedUse.newBuilder().setOn(on).setDeviceId(device).build() })
    fun cancelLockedUse() = send(intent { cancelLockedUse = muster.app.CancelLockedUse.getDefaultInstance() })
    fun agentLoadEarlier(all: Boolean = false) = send(intent { agentLoadEarlier = agentLoadEarlier { this.all = all } })

    fun openTerminals(device: String = "") = send(intent { openTerminals = openTerminals { deviceId = device } })
    fun openTerminal(session: String, device: String = "") =
        send(intent { openTerminal = openTerminal { this.session = session; deviceId = device } })
    fun terminalKey(key: String) = send(intent { terminalKey = terminalKey { this.key = key } })
    fun terminalText(text: String) = send(intent { terminalText = terminalText { this.text = text } })
    /** Clipboard text for the terminal; the core types it while `TerminalView.canPaste` holds. */
    fun terminalPaste(text: String) = send(intent { terminalPaste = terminalPaste { this.text = text } })
    fun codeAction(action: Int, id: String = "") = send(intent { codeAction = codeAction { actionValue = action; this.id = id; autoReturn = true } })
    fun sessionTerminal(action: Int) = send(intent { sessionTerminalAction = sessionTerminalAction { actionValue = action } })
    fun terminalCreate(profile: String = "") = send(intent { terminalCreate = terminalCreate { this.profile = profile } })
    fun terminalResize(cols: Int, rows: Int) = send(intent { terminalResize = terminalResize { this.cols = cols; this.rows = rows } })
    fun openSsh(port: Int = 0, user: String, cols: Int, rows: Int) =
        send(intent { openSsh = openSsh { this.port = port; this.user = user; this.cols = cols; this.rows = rows } })
    fun closeTerminal() = send(intent { closeTerminal = muster.app.CloseTerminal.getDefaultInstance() })

    fun openFiles(path: String = "") = send(intent { openFiles = openFiles { this.path = path } })
    fun openFile(path: String) = send(intent { openFile = openFile { this.path = path } })
    /** A link in a reply that is not a web URL; the core resolves it in the session. */
    fun openFileLink(url: String) = send(intent { openFileLink = openFileLink { this.url = url } })
    /** A file a turn changed, as the turn's summary recorded it. */
    fun openChange(summary: String, path: String) = send(intent { openChange = openChange { summaryId = summary; this.path = path } })
    fun openGit(diffCommit: String = "") = send(intent { openGit = openGit { this.diffCommit = diffCommit } })

    /** Diagnostics the shell read, saved by a member under /tmp/muster-reports; the result is a toast. */
    fun uploadDiagnostics(device: String, kind: String = "performance", data: ByteArray) =
        send(intent { uploadDiagnostics = uploadDiagnostics { deviceId = device; this.kind = kind; this.data = ByteString.copyFrom(data) } })
    /** kind 1 APNs, 2 FCM, 3 HarmonyOS. */
    fun setPushToken(kind: Int, token: ByteArray) =
        send(intent { setPushToken = setPushToken { this.kind = kind; this.token = ByteString.copyFrom(token) } })

    companion object {
        fun initLogging(level: Int) = MusterNative.logInit(level)
    }
}

/** The key names muster-terminal maps to bytes. `ctrl-`, `alt-` and `shift-`
 *  prefixes combine with any of them or with one character; the core decides
 *  what they send, so the shell passes every such name through. */
object TerminalKeys {
    const val ENTER = "enter"
    const val TAB = "tab"
    const val BACKSPACE = "backspace"
    const val ESCAPE = "escape"
    const val UP = "up"
    const val DOWN = "down"
    const val RIGHT = "right"
    const val LEFT = "left"
    const val HOME = "home"
    const val END = "end"
    const val PAGE_UP = "pageup"
    const val PAGE_DOWN = "pagedown"
    const val DELETE = "delete"

    /** `ctrl-<letter>` for any letter; the core maps it. */
    fun control(letter: Char): String? = if (letter.isLetter()) "ctrl-${letter.lowercaseChar()}" else null
}

/** One run of equally styled cells (muster-terminal's 11-byte encoding). */
data class StyleRun(val length: Int, val fg: Int, val bg: Int, val flags: Int) {
    val bold get() = flags and 1 != 0
    val italic get() = flags and 2 != 0
    val underline get() = flags and 4 != 0
    val inverse get() = flags and 8 != 0
    val dim get() = flags and 16 != 0
    val strike get() = flags and 32 != 0

    companion object {
        fun decode(bytes: ByteArray): List<StyleRun> {
            val runs = ArrayList<StyleRun>(bytes.size / 11)
            var i = 0
            while (i + 11 <= bytes.size) {
                fun u(at: Int) = bytes[i + at].toInt() and 0xff
                val len = (u(0) shl 8) or u(1)
                val fg = (u(2) shl 24) or (u(3) shl 16) or (u(4) shl 8) or u(5)
                val bg = (u(6) shl 24) or (u(7) shl 16) or (u(8) shl 8) or u(9)
                runs.add(StyleRun(len, fg, bg, u(10)))
                i += 11
            }
            return runs
        }

        /** 0 = default (null), 0x01RRGGBB = rgb, 0x02000000 | index = palette. */
        fun argb(color: Int, palette: IntArray): Int? = when (color ushr 24) {
            0x01 -> 0xff000000.toInt() or (color and 0xffffff)
            0x02 -> palette.getOrNull(color and 0xff)
            else -> null
        }
    }
}
