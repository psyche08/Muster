// The typed face of the core: Intents in, ViewModels and Effects out, as
// the protobuf messages of app.proto. The shell never interprets what it
// renders; it forwards what the user did and what the platform answered.
import Foundation
import SwiftProtobuf

public typealias ViewModel = Muster_App_ViewModel
public typealias Intent = Muster_App_Intent
public typealias Effect = Muster_App_Effect
public typealias Screen = Muster_App_Screen
public typealias Lifecycle = Muster_App_Lifecycle

public final class MusterCore: MusterShell {
    public let composerDraft = ComposerDraftSource()
    public let keys: MusterKeys
    private(set) var runtime: MusterRuntime!
    private let onViewModel: (ViewModel) -> Void
    private let onEffect: (UInt64, Effect) -> Void

    /// Starts a core. `onViewModel` and `onEffect` run on the core's own
    /// threads; hop to the main actor before touching views.
    public init(
        config: MusterConfig,
        keys: MusterKeys,
        onViewModel: @escaping (ViewModel) -> Void,
        onEffect: @escaping (UInt64, Effect) -> Void
    ) throws {
        self.keys = keys
        self.onViewModel = onViewModel
        self.onEffect = onEffect
        runtime = try MusterRuntime(configJSON: try config.json(), shell: self)
    }

    public var version: String { MusterRuntime.version }

    public func shutdown() {
        runtime.shutdown()
    }

    // MARK: Intents

    public func send(_ it: Intent.OneOf_It) {
        var intent = Intent()
        intent.it = it
        send(intent)
    }

    public func send(_ intent: Intent) {
        let intent = composerDraft.attaching(to: intent)
        guard let bytes = try? intent.serializedData() else { return }
        runtime.send(intentBytes: bytes)
    }

    public func lifecycle(_ event: Lifecycle) {
        runtime.lifecycle(UInt32(event.rawValue))
    }

    /// The answer to an effect. `data` carries a signature, `text` a scan.
    public func answer(effect id: UInt64, ok: Bool, error: String = "", data: Data = Data(), text: String = "") {
        var result = Muster_App_EffectResult()
        result.ok = ok
        result.error = error
        result.data = data
        result.text = text
        guard let bytes = try? result.serializedData() else { return }
        runtime.send(effectID: id, resultBytes: bytes)
    }

    /// This device's key as an OpenSSH public key line, for the user to put
    /// in a server's authorized_keys (phones).
    public func sshPublicKeyLine() -> String? {
        runtime.sshPublicKeyLine()
    }

    // MARK: Convenience intents (each is one `OneOf_It`)

    public func openBrowser(device: String = "", draft: String = "") {
        send(.openBrowser(Muster_App_OpenBrowser.with { $0.device = device; $0.draft = draft }))
    }
    public func browserSetup(install: Bool = false) { send(.browserSetup(Muster_App_BrowserSetup.with { $0.install = install })) }
    public func browserRun(_ json: String) { send(.browserRun(Muster_App_BrowserRun.with { $0.json = json })) }

    public func refresh() { send(.refresh(Muster_App_Refresh())) }

    public func back() {
        send(.navigate(Muster_App_Navigate.with { $0.back = true }))
    }

    public func back(count: Int, expectedPath: [Screen]) {
        guard count > 0 else { return }
        send(.navigate(Muster_App_Navigate.with {
            $0.back = true; $0.backCount = UInt32(count); $0.expectedPath = expectedPath
        }))
    }

    public func navigate(to screen: Screen) {
        send(.navigate(Muster_App_Navigate.with { $0.screen = screen }))
    }

    public func dismiss(toast id: String) {
        send(.dismiss(Muster_App_Dismiss.with { $0.toastID = id }))
    }

    public func createNetwork(name: String, cidr: String = "") {
        send(.createNetwork(Muster_App_CreateNetwork.with { $0.name = name; $0.vnetCidr = cidr }))
    }

    public func showInvite(ttlSeconds: UInt32 = 0) {
        send(.showInvite(Muster_App_ShowInvite.with { $0.ttlSecs = ttlSeconds }))
    }

    public func join(invite: String, deviceName: String, kind: String) {
        send(.joinNetwork(Muster_App_JoinNetwork.with {
            $0.inviteText = invite; $0.deviceName = deviceName; $0.deviceKind = kind
        }))
    }

    public func decideJoin(device: String, approve: Bool, scopes: [String] = [], reason: String = "") {
        send(.decideJoin(Muster_App_DecideJoin.with {
            $0.deviceID = device; $0.approve = approve; $0.scopes = scopes; $0.reason = reason
        }))
    }

    public func removeDevice(_ device: String) {
        send(.removeDevice(Muster_App_RemoveDevice.with { $0.deviceID = device }))
    }

    public func setScopes(device: String, scopes: [String], role: String) {
        send(.setScopes(Muster_App_SetScopes.with { $0.deviceID = device; $0.scopes = scopes; $0.role = role }))
    }

    /// macOS: ask for the Accessibility permission that lets musterd drive
    /// the Claude desktop app (the OS prompts; the pane opens).
    public func requestAccessibility() {
        send(.requestAccessibility(Muster_App_RequestAccessibility()))
    }

    /// The privacy cover: this desktop's (device empty) or a member's.
    public func setCover(_ on: Bool, device: String = "") {
        send(.setCover(Muster_App_SetCover.with { $0.on = on; $0.deviceID = device }))
    }

    public func setRelay(_ relay: String) {
        send(.setRelay(Muster_App_SetRelay.with { $0.relay = relay }))
    }

    public func setPolicy(_ policy: Muster_V1_Policy) {
        send(.setPolicy(Muster_App_SetPolicy.with { $0.policy = policy }))
    }

    public func selectDevice(_ device: String) {
        send(.selectDevice(Muster_App_SelectDevice.with { $0.deviceID = device }))
    }

    public func openAgents(device: String = "") { send(.openAgents(Muster_App_OpenAgents.with { $0.deviceID = device })) }

    public func createOrcaSession(device: String, directory: String, agent: String) {
        send(.orcaCreate(Muster_App_OrcaCreate.with { $0.deviceID = device; $0.directory = directory; $0.agent = agent }))
    }
    public func openAgent(_ session: String, device: String = "") {
        send(.openAgent(Muster_App_OpenAgent.with { $0.session = session; $0.deviceID = device }))
    }

    /// The Chat tab: `provider` "claude" (the Claude app) or "codex" (the
    /// ChatGPT app), "" for the last one. The core picks the Mac.
    public func openDesktopChats(provider: String = "") {
        send(.openDesktopChats(Muster_App_OpenDesktopChats.with { $0.provider = provider }))
    }

    /// A listed conversation by session id, or a new one (""); `onDesktop`
    /// shows a new one on the Mac now instead of with its first message.
    public func desktopChatOpen(_ chat: String, onDesktop: Bool = false) {
        send(.desktopChatOpen(Muster_App_DesktopChatOpen.with { $0.chat = chat; $0.desktop = onDesktop }))
    }

    /// The open desktop chat's model / effort choices, read from the Mac.
    public func chatOptionsLoad() { send(.chatOptionsLoad(Muster_App_ChatOptionsLoad())) }
    public func composerComplete() { send(.composerComplete(Muster_App_ComposerComplete())) }

    /// Chooses a model and/or effort by name exactly as the Mac offered it.
    public func chatSetOptions(model: String = "", effort: String = "") {
        send(.chatSetOptions(Muster_App_ChatSetOptions.with { $0.model = model; $0.effort = effort }))
    }

    /// `releaseUnconfirmed`: the visible notice token for an explicit retry.
    /// Shared Rust validates and consumes it through the ordinary send path.
    public func agentSend(_ text: String, attachments: [Muster_V1_AgentAttachment] = [], draftID: String = "",
                          releaseUnconfirmed: String = "") {
        send(.agentSend(Muster_App_AgentSend.with {
            $0.text = text; $0.attachments = attachments; $0.draftID = draftID
            $0.releaseUnconfirmed = releaseUnconfirmed
        }))
    }

    public func agentRespond(interaction: String, option: String = "", answer: String = "", answers: [Muster_V1_QuestionAnswer] = []) {
        send(.agentRespond(Muster_App_AgentRespond.with {
            $0.interaction = interaction; $0.option = option; $0.answer = answer; $0.answers = answers
        }))
    }

    public func multicaBrowse(_ request: Muster_V1_MulticaBrowseRequest) {
        var intent = Muster_App_Intent()
        intent.multicaBrowse = request
        send(intent)
    }

    public func multica(_ request: Muster_V1_MulticaRequest) { send(.multica(request)) }

    public func desktopUpdate(_ action: Muster_App_DesktopUpdateAction.Action) { send(.desktopUpdateAction(.with { $0.action = action })) }
    public func desktopSetup(_ action: Muster_App_DesktopSetupAction.Action) { send(.desktopSetupAction(.with { $0.action = action })) }
    public func setDeviceLockedUse(_ on: Bool, device: String) {
        send(.setDeviceLockedUse(Muster_App_SetDeviceLockedUse.with { $0.on = on; $0.deviceID = device }))
    }

    public func cancelLockedUse() { send(.cancelLockedUse(Muster_App_CancelLockedUse())) }
    public func ignoreAgentSend(token: String) {
        send(.resolveAgentSend(Muster_App_ResolveAgentSend.with { $0.token = token; $0.dismiss = true }))
    }

    public func resolveAgentSend(token: String, delivered: Bool) {
        send(.resolveAgentSend(Muster_App_ResolveAgentSend.with { $0.token = token; $0.delivered = delivered }))
    }

    public func agentInterrupt() { send(.agentInterrupt(Muster_App_AgentInterrupt())) }

    public func agentLoadEarlier(all: Bool = false) { var r = Muster_App_AgentLoadEarlier(); r.all = all; send(.agentLoadEarlier(r)) }

    public func openTerminals(device: String = "") { send(.openTerminals(Muster_App_OpenTerminals.with { $0.deviceID = device })) }

    public func openTerminal(_ session: String, device: String = "") {
        send(.openTerminal(Muster_App_OpenTerminal.with { $0.session = session; $0.deviceID = device }))
    }

    public func terminalKey(_ key: TerminalKey) {
        send(.terminalKey(Muster_App_TerminalKey.with { $0.key = key.rawValue }))
    }

    public func terminalText(_ text: String) {
        send(.terminalText(Muster_App_TerminalText.with { $0.text = text }))
    }

    /// Sends diagnostics (the performance log the shell read) to a member,
    /// which saves them under /tmp/muster-reports; the result is a toast.
    public func uploadDiagnostics(device: String, kind: String = "performance", data: Data) {
        send(.uploadDiagnostics(Muster_App_UploadDiagnostics.with {
            $0.deviceID = device; $0.kind = kind; $0.data = data
        }))
    }

    /// Clipboard text for the terminal; the core decides how it is typed.
    public func terminalPaste(_ text: String) {
        send(.terminalPaste(Muster_App_TerminalPaste.with { $0.text = text }))
    }

    public func codeAction(_ action: Muster_App_CodeAction.Action, id: String = "", autoReturn: Bool = true) {
        send(.codeAction(Muster_App_CodeAction.with { $0.action = action; $0.id = id; $0.autoReturn = autoReturn }))
    }
    public func sessionTerminal(_ action: Muster_App_SessionTerminalAction.Action) {
        send(.sessionTerminalAction(Muster_App_SessionTerminalAction.with { $0.action = action }))
    }

    public func terminalCreate(profile: String = "", source: String = "", attachManaged: Bool = false) {
        send(.terminalCreate(Muster_App_TerminalCreate.with {
            $0.profile = profile; $0.source = source; $0.attachManaged = attachManaged
        }))
    }

    public func terminalResize(cols: UInt32, rows: UInt32) {
        send(.terminalResize(Muster_App_TerminalResize.with { $0.cols = cols; $0.rows = rows }))
    }

    public func openSsh(port: UInt32 = 0, user: String, cols: UInt32, rows: UInt32) {
        send(.openSsh(Muster_App_OpenSsh.with { $0.port = port; $0.user = user; $0.cols = cols; $0.rows = rows }))
    }

    public func closeTerminal() { send(.closeTerminal(Muster_App_CloseTerminal())) }

    public func openFiles(path: String = "") {
        send(.openFiles(Muster_App_OpenFiles.with { $0.path = path }))
    }

    public func openFileLink(_ url: String) {
        send(.openFileLink(Muster_App_OpenFileLink.with { $0.url = url }))
    }
    public func openChange(summary: String, path: String) {
        send(.openChange(Muster_App_OpenChange.with { $0.summaryID = summary; $0.path = path }))
    }
    public func openFile(path: String) {
        send(.openFile(Muster_App_OpenFile.with { $0.path = path }))
    }

    public func openGit(diffCommit: String = "") {
        send(.openGit(Muster_App_OpenGit.with { $0.diffCommit = diffCommit }))
    }

    /// The platform's push token: kind 1 APNs, 2 FCM, 3 HarmonyOS.
    public func setPushToken(kind: UInt32, token: Data) {
        send(.setPushToken(Muster_App_SetPushToken.with { $0.kind = kind; $0.token = token }))
    }

    // MARK: MusterShell

    public func onView(_ bytes: Data) {
        #if os(iOS)
        let diagnostic = PerformanceDiagnostics.shared
        let decodeStart = ProcessInfo.processInfo.systemUptime
        #endif
        guard let vm = try? ViewModel(serializedBytes: bytes) else { return }
        #if os(iOS)
        diagnostic.recordView(bytes: bytes.count, decodeSeconds: ProcessInfo.processInfo.systemUptime - decodeStart,
            screen: vm.screen.rawValue, messages: Int(vm.chat.messageCount))
        #endif
        onViewModel(vm)
    }

    public func onEffect(id: UInt64, bytes: Data) {
        guard let effect = try? Effect(serializedBytes: bytes) else {
            answer(effect: id, ok: false, error: "undecodable effect")
            return
        }
        if case .sendDiagnostic(let record) = effect.ef {
            #if os(iOS)
            PerformanceDiagnostics.shared.recordSendDiagnostic(record.json)
            #endif
            return // Fire-and-forget: never acknowledges an application operation.
        }
        onEffect(id, effect)
    }

    public func hardwarePublicKey() -> Data? { keys.devicePublicKey }
    public func hardwareSign(_ message: Data) -> Data? { keys.deviceSign(message) }
    public func presencePublicKey() -> Data? { keys.presencePublicKey }
}
