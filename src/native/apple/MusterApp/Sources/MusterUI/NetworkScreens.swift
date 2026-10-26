// Onboarding, home, invite, join, device and settings: the network side.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI

struct OnboardingView: View {
    let core: MusterCore
    let vm: ViewModel
    @State private var creating = false
    @State private var editingRelay = false
    @State private var networkName = "home"

    var body: some View {
        VStack(spacing: 24) {
            Spacer()
            MusterMark()
            Text("Muster").font(.largeTitle.bold())
            VStack(spacing: 4) {
                Text(vm.self_p.name).font(.headline)
                Text(vm.self_p.fingerprint).font(.system(.footnote, design: .monospaced)).foregroundStyle(.secondary)
            }
            VStack(spacing: 12) {
                Button {
                    core.join(invite: "", deviceName: vm.self_p.name, kind: vm.self_p.kind)
                } label: {
                    Label(vm.embedded ? "Scan an invite" : "Join with an invite", systemImage: vm.embedded ? "qrcode.viewfinder" : "doc.on.clipboard")
                        .frame(maxWidth: 280)
                }
                .buttonStyle(.borderedProminent)
                Button { creating = true } label: {
                    Label("Create a network", systemImage: "plus.circle").frame(maxWidth: 280)
                }
                .buttonStyle(.bordered)
                // Creating a network and inviting need a relay; joining takes the invite's.
                Button { editingRelay = true } label: {
                    Label(vm.self_p.relay.isEmpty ? "Relay: not set" : "Relay: \(vm.self_p.relay)", systemImage: "antenna.radiowaves.left.and.right")
                        .font(.footnote)
                }
                .buttonStyle(.borderless)
            }
            Spacer()
            if vm.join.phase != "idle" {
                Text(vm.join.message).font(.footnote).foregroundStyle(.secondary)
            }
        }
        .padding()
        .navigationTitle("Welcome")
        .sheet(isPresented: $creating) {
            NavigationStack {
                Form {
                    #if os(iOS)
                    MusterTextField("Network name", text: $networkName)
                    #else
                    TextField("Network name", text: $networkName)
                    #endif
                }
                .groupedForm()
                .navigationTitle("New network")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { creating = false } }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Create") { core.createNetwork(name: networkName); creating = false }
                            .disabled(networkName.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
            #if os(macOS)
            .frame(minWidth: 360, minHeight: 160)
            #endif
        }
        .sheet(isPresented: $editingRelay) {
            NavigationStack {
                Form { RelaySection(core: core, vm: vm) }
                    .groupedForm()
                    .navigationTitle("Relay")
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) { Button("Done") { editingRelay = false } }
                    }
            }
            #if os(macOS)
            .frame(minWidth: 420, minHeight: 220)
            #endif
        }
    }
}

/// The relay this device registers with, in Settings and on the welcome screen.
struct RelaySection: View {
    let core: MusterCore
    let vm: ViewModel
    @State private var relay: String

    init(core: MusterCore, vm: ViewModel) {
        self.core = core
        self.vm = vm
        _relay = State(initialValue: vm.self_p.relay)
    }

    var body: some View {
        Section {
            #if os(iOS)
            MusterTextField("relay.example.com:9443", text: $relay, monospace: true)
            #else
            TextField("relay.example.com:9443", text: $relay)
                .autocorrectionDisabled()
                .font(.system(.body, design: .monospaced))
            #endif
            HStack {
                Text(vm.self_p.state.relay).foregroundStyle(.secondary)
                Spacer()
                Button("Use this relay") { core.setRelay(relay.trimmingCharacters(in: .whitespaces)) }
                    .disabled(relay.trimmingCharacters(in: .whitespaces).isEmpty || relay == vm.self_p.relay)
            }
        } header: {
            Text("Relay")
        } footer: {
            Text("Where this device registers so members can reach it from anywhere. TCP/TLS and UDP/QUIC share host:port and are selected automatically. Both verify the server certificate. Joining a network switches to the relay in its invite.")
        }
        .onChange(of: vm.self_p.relay) { _, new in relay = new }
    }
}

struct HomeView: View {
    let core: MusterCore
    let vm: ViewModel
    var body: some View {
        AgentsView(core: core, vm: vm)
        #if os(iOS)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                SettingsButton(pending: !vm.pending.isEmpty) { core.navigate(to: .settings) }
            }
        }
        #endif
    }
}

extension Muster_V1_PendingJoin: Identifiable {
    public var id: String { deviceID }
}

/// The phone's way into Settings: a gear, with a dot while a join waits.
struct SettingsButton: View {
    let pending: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "gearshape")
                .overlay(alignment: .topTrailing) {
                    if pending { Circle().fill(Color.red).frame(width: 8, height: 8).offset(x: 3, y: -3) }
                }
        }
        .accessibilityLabel(pending ? "Settings, a device is waiting to join" : "Settings")
    }
}

/// A device's release version and source commit as the core reports them.
func buildText(version: String, commit: String) -> String {
    [version, commit].filter { !$0.isEmpty }.joined(separator: " · ")
}

struct MemberRow: View {
    let member: Muster_V1_MemberView

    var body: some View {
        HStack {
            Image(systemName: member.kind == "phone" ? "iphone" : member.kind == "server" ? "server.rack" : "desktopcomputer")
                .foregroundStyle(member.connected || member.isSelf ? Color.accentColor : Color.secondary)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(member.name).font(.headline)
                    if member.isSelf { Text("this device").font(.caption).foregroundStyle(.secondary) }
                    if member.role == "admin" { Text("admin").font(.caption).padding(.horizontal, 6).background(Color.accentColor.opacity(0.15), in: Capsule()) }
                }
                Text(subtitle).font(.footnote).foregroundStyle(.secondary)
                if !build.isEmpty {
                    Text(build).font(.caption2.monospaced()).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if !member.isSelf {
                Circle().fill(member.connected ? Color.green : Color.gray.opacity(0.4)).frame(width: 8, height: 8)
            }
        }
        .contentShape(Rectangle())
    }

    private var build: String { buildText(version: member.version, commit: member.commit) }

    private var subtitle: String {
        [member.vip, member.link].filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// Edits a member's access as the core groups it: each module's device-level
/// switch, its providers (greyed while the switch is off), then the rest.
/// Sends back every scope left on; the core validates and signs.
struct AccessEditorSheet: View {
    let title: String
    let access: Muster_App_AccessView
    let done: ([String]) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var selected: Set<String> = []

    private var offered: [Muster_App_AccessItem] {
        access.sections.flatMap { ($0.hasToggle ? [$0.toggle] : []) + $0.items }
    }

    var body: some View {
        NavigationStack {
            Form {
                ForEach(access.sections, id: \.id) { section in
                    Section {
                        if section.hasToggle {
                            toggle(section.toggle).font(.headline)
                        }
                        ForEach(section.items, id: \.scope) { item in
                            toggle(item)
                                .disabled(item.requiresToggle && !selected.contains(section.toggle.scope))
                                .padding(.leading, item.requiresToggle ? 16 : 0)
                        }
                    } header: {
                        if !section.hasToggle { Text(section.title) }
                    } footer: {
                        if !section.footer.isEmpty { Text(section.footer) }
                    }
                }
            }
            .groupedForm()
            .navigationTitle(title)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { done(offered.map(\.scope).filter { selected.contains($0) }); dismiss() }
                }
            }
            .onAppear { selected = Set(offered.filter(\.granted).map(\.scope)) }
        }
        #if os(macOS)
        .frame(minWidth: 420, minHeight: 520)
        #endif
    }

    private func toggle(_ item: Muster_App_AccessItem) -> some View {
        Toggle(item.label, isOn: Binding(
            get: { selected.contains(item.scope) },
            set: { on in if on { selected.insert(item.scope) } else { selected.remove(item.scope) } }
        ))
    }
}

struct InviteView: View {
    let core: MusterCore
    let vm: ViewModel
    @State private var copied = false

    var body: some View {
        VStack(spacing: 20) {
            if vm.invite.text.isEmpty {
                ProgressView("Preparing the invite")
            } else {
                QRCodeImage(text: vm.invite.text)
                Text(vm.invite.text)
                    .font(.system(.caption2, design: .monospaced))
                    .textSelection(.enabled)
                    .lineLimit(4)
                    .frame(maxWidth: 360)
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    let left = Int(Double(vm.invite.expiresUnix) - context.date.timeIntervalSince1970)
                    Text(left > 0 ? "Expires in \(left / 60):\(String(format: "%02d", left % 60))" : "Expired")
                        .font(.footnote).foregroundStyle(left > 0 ? Color.secondary : Color.red)
                }
                HStack {
                    Button {
                        copyToPasteboard(vm.invite.text)
                        copied = true
                    } label: { Label(copied ? "Copied" : "Copy", systemImage: "doc.on.doc") }
                    Button("New invite") { core.showInvite() }
                }
            }
            Text("Scan on the phone, or paste the text on a desktop. One invite admits one device; approve it here afterwards.")
                .font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center).frame(maxWidth: 360)
        }
        .padding()
        .navigationTitle("Invite a device")
    }
}

func copyToPasteboard(_ text: String) {
    #if os(iOS)
    UIPasteboard.general.string = text
    #elseif os(macOS)
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
    #endif
}

struct JoinView: View {
    let core: MusterCore
    let vm: ViewModel

    var body: some View {
        VStack(spacing: 16) {
            if vm.join.phase == "failed" {
                Image(systemName: "xmark.octagon").font(.largeTitle).foregroundStyle(.red)
            } else if vm.join.phase == "joined" {
                Image(systemName: "checkmark.seal").font(.largeTitle).foregroundStyle(.green)
            } else {
                ProgressView()
            }
            Text(vm.join.message).multilineTextAlignment(.center)
            if vm.join.phase == "failed" {
                Button("Back") { core.back() }
            }
        }
        .padding()
        .navigationTitle("Joining")
    }
}

struct DeviceView: View {
    let core: MusterCore
    let vm: ViewModel
    @State private var editingScopes = false
    @State private var confirmRemove = false
    @State private var ssh = false

    private var member: Muster_V1_MemberView? {
        vm.network.members.first { $0.deviceID == vm.selectedDevice }
    }

    var body: some View {
        if let member {
            List {
                Section {
                    LabeledContent("Name", value: member.name)
                    LabeledContent("Kind", value: member.kind)
                    LabeledContent("Role", value: member.role)
                    if !member.vip.isEmpty { LabeledContent("Virtual IP", value: member.vip) }
                    LabeledContent("Fingerprint") {
                        Text(member.fingerprint).font(.system(.body, design: .monospaced))
                    }
                    if !member.link.isEmpty {
                        LabeledContent("Link", value: member.link)
                    }
                    // From the link heartbeat; empty until the device reports it.
                    LabeledContent("Version", value: member.version.isEmpty ? "not reported" : member.version)
                    LabeledContent("Commit") {
                        Text(member.commit.isEmpty ? "not reported" : member.commit).font(.system(.body, design: .monospaced))
                    }
                }
                if let control = vm.deviceLockedUse.first(where: { $0.deviceID == member.deviceID }) {
                    Section("Locked Use") { DeviceLockedUseToggle(core: core, control: control) }
                }
                if vm.selectedDeviceHasServices {
                    Section("Use") {
                        Button { core.openBrowser(device: member.deviceID) } label: { Label("Chrome browser", systemImage: "globe") }
                        Button { core.openAgents() } label: { Label("Agent sessions", systemImage: "bubble.left.and.text.bubble.right") }
                        Button { core.openTerminals() } label: { Label("Terminals", systemImage: "terminal") }
                        if vm.deviceActions.canCover {
                            Button { core.setCover(true, device: member.deviceID) } label: { Label("Cover its screen", systemImage: "rectangle.fill") }
                            Button { core.setCover(false, device: member.deviceID) } label: { Label("Uncover its screen", systemImage: "rectangle") }
                        }
                        if vm.deviceActions.canSsh {
                            Button { ssh = true } label: { Label("SSH", systemImage: "lock.shield") }
                        }
                    }
                }
                if vm.hasDeviceAccess && vm.deviceActions.showAccess {
                    Section("Access to other devices") {
                        ForEach(vm.deviceAccess.sections, id: \.id) { section in
                            LabeledContent(section.title, value: section.summary)
                        }
                        if vm.deviceActions.canEditAccess {
                            Button("Change…") { editingScopes = true }
                        }
                    }
                }
                if vm.deviceActions.canRemove {
                    Section {
                        Button("Remove from the network", role: .destructive) { confirmRemove = true }
                    }
                }
            }
            .navigationTitle(member.name)
            .sheet(isPresented: $editingScopes) {
                AccessEditorSheet(title: "Access for \(member.name)", access: vm.deviceAccess) { scopes in
                    core.setScopes(device: member.deviceID, scopes: scopes, role: member.role)
                }
            }
            .sheet(isPresented: $ssh) {
                SshSheet { user, port in core.openSsh(port: port, user: user, cols: 80, rows: 24) }
            }
            .confirmationDialog("Remove \(member.name)?", isPresented: $confirmRemove, titleVisibility: .visible) {
                Button("Remove", role: .destructive) { core.removeDevice(member.deviceID) }
            } message: {
                Text("Its links close within two seconds and it cannot rejoin without a new invite.")
            }
        } else {
            ContentUnavailableView("No such device", systemImage: "questionmark.circle")
        }
    }
}

struct SshSheet: View {
    let open: (String, UInt32) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var user = NSUserName()
    @State private var port = "22"

    var body: some View {
        NavigationStack {
            Form {
                #if os(iOS)
                MusterTextField("User", text: $user, monospace: true)
                MusterTextField("Port", text: $port, monospace: true)
                #else
                TextField("User", text: $user).autocorrectionDisabled()
                TextField("Port", text: $port)
                #endif
                Text("Signs in with this phone's own key: put its public key line (Settings) in the target's authorized_keys.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            .groupedForm()
            .navigationTitle("SSH")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Connect") { open(user, UInt32(port) ?? 22); dismiss() }.disabled(user.isEmpty)
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 360, minHeight: 200)
        #endif
    }
}

struct SettingsView: View {
    let core: MusterCore
    let vm: ViewModel
    @ObservedObject var model: MusterModel
    @State private var policy = Muster_V1_Policy()
    @State private var newPort = ""
    @State private var loaded = false
    @State private var deciding: Muster_V1_PendingJoin?
    #if os(iOS)
    @ObservedObject private var keyboardPrefs = KeyboardPrefs.shared
    #endif

    var body: some View {
        Form {
            #if os(iOS)
            Section {
                LayoutPicker()
            } header: {
                Text("页面布局")
            } footer: {
                Text("上下布局将内容与输入区分开；左右分栏需要足够宽度，窄屏自动使用单页。iPhone 的自动模式使用单页。")
            }
            Section {
                Toggle("Muster 内置键盘", isOn: Binding(get: { !keyboardPrefs.systemKeyboard }, set: { keyboardPrefs.systemKeyboard = !$0 }))
                Toggle("终端使用内置键盘", isOn: Binding(get: { !keyboardPrefs.terminalSystemKeyboard }, set: { keyboardPrefs.terminalSystemKeyboard = !$0 }))
            } header: {
                Text("Keyboard")
            } footer: {
                Text("支持拼音、英文、数字和符号。会话输入框旁的键盘按钮也可以直接切换并打开键盘。普通输入默认使用系统键盘；终端默认使用内置键盘，提供 Esc、功能键、Ctrl 和 Alt。宽屏显示 MacBook 布局。")
            }
            #endif
            #if os(iOS)
            Section {
                NavigationLink("性能监控") { PerformanceMonitorView(core: core, devices: vm.serviceDevices) }
            }
            #endif
            if vm.desktopUpdate.supported {
                Section("Muster 更新") {
                    Text(updateStatus(vm.desktopUpdate.status))
                    if !vm.desktopUpdate.version.isEmpty { Text("新版本：\(vm.desktopUpdate.version)") }
                    if !vm.desktopUpdate.error.isEmpty { Text(vm.desktopUpdate.error).foregroundStyle(.red).textSelection(.enabled) }
                    if vm.desktopUpdate.canCheck { Button("检查更新") { core.desktopUpdate(.check) } }
                    if vm.desktopUpdate.canDownload { Button("下载更新") { core.desktopUpdate(.download) } }
                    if vm.desktopUpdate.canInstall { Button("安装并重启 Muster") { core.desktopUpdate(.install) } }
                }
            }
            if vm.multica.supported && !vm.embedded {
                MulticaSettings(core: core, status: vm.multica)
            }
            Section("Network devices") {
                ForEach(vm.pending, id: \.deviceID) { join in
                    VStack(alignment: .leading, spacing: 6) {
                        Label("\(join.name) wants to join", systemImage: "person.badge.plus").font(.headline)
                        Text("\(join.kind) · \(join.fingerprint)").font(.caption.monospaced()).foregroundStyle(.secondary)
                        HStack {
                            Button("Approve…") { deciding = join }.buttonStyle(.borderedProminent)
                            Button("Reject", role: .destructive) { core.decideJoin(device: join.deviceID, approve: false, reason: "rejected") }
                                .buttonStyle(.bordered)
                        }
                    }
                    .padding(.vertical, 4)
                }
                ForEach(vm.network.members, id: \.deviceID) { member in
                    Button { core.selectDevice(member.deviceID) } label: {
                        MemberRow(member: member)
                    }.buttonStyle(.plain)
                    if let control = vm.deviceLockedUse.first(where: { $0.deviceID == member.deviceID }) {
                        DeviceLockedUseToggle(core: core, control: control)
                    }
                }
                if vm.canInvite {
                    Button { core.showInvite() } label: { Label("Invite device", systemImage: "qrcode") }
                }
            }
            if vm.localDeviceHasServices {
                Section("Chrome extension") {
                    Button { core.openBrowser(device: vm.self_p.deviceID) } label: {
                        Label("Set up Agent Halo Browser", systemImage: "puzzlepiece.extension")
                    }
                    Text("Install the extension, check its connection, and manage browser access.").font(.footnote).foregroundStyle(.secondary)
                }
                Section("What this device offers") {
                    Toggle("Agent sessions", isOn: $policy.offerAgent)
                    Toggle("Terminals", isOn: $policy.offerTerminal)
                    Toggle("Control while the screen is locked", isOn: $policy.allowLockedControl)
                }
            }
            Section("Open ports") {
                ForEach(policy.openPorts, id: \.self) { port in
                    Text(port).font(.system(.body, design: .monospaced))
                }
                .onDelete { policy.openPorts.remove(atOffsets: $0) }
                HStack {
                    #if os(iOS)
                    MusterTextField("tcp/22", text: $newPort, monospace: true)
                    #else
                    TextField("tcp/22", text: $newPort).autocorrectionDisabled()
                    #endif
                    Button("Add") {
                        let trimmed = newPort.trimmingCharacters(in: .whitespaces)
                        if !trimmed.isEmpty && !policy.openPorts.contains(trimmed) { policy.openPorts.append(trimmed) }
                        newPort = ""
                    }
                }
            }
            Section {
                Button("Save policy") { core.setPolicy(policy) }
            }
            RelaySection(core: core, vm: vm)
            Section("This device") {
                LabeledContent("Name", value: vm.self_p.name)
                LabeledContent("Fingerprint") { Text(vm.self_p.fingerprint).font(.system(.body, design: .monospaced)) }
                LabeledContent("Key", value: vm.self_p.keyLabel)
                LabeledContent("Relay", value: vm.self_p.relay.isEmpty ? vm.self_p.state.relay : "\(vm.self_p.relay) · \(vm.self_p.state.relay)")
                if !vm.self_p.state.accessibility.isEmpty {
                    LabeledContent("Accessibility") {
                        HStack {
                            Text(vm.self_p.state.accessibility == "granted" ? "granted" : "not granted")
                                .foregroundStyle(vm.self_p.state.accessibility == "granted" ? Color.secondary : Color.orange)
                            if vm.self_p.state.accessibility != "granted" {
                                Button("Grant…") { core.requestAccessibility() }
                            }
                        }
                    }
                    if vm.self_p.state.accessibility != "granted" {
                        Text("Needed to operate Codex and Claude desktop sessions. Enable musterd under System Settings › Privacy & Security › Accessibility on this Mac. Locked Use requires this permission too.")
                            .font(.footnote).foregroundStyle(.secondary)
                    }
                }
                #if os(macOS)
                if !vm.embedded {
                    LabeledContent("Locked Use", value: vm.desktopSetup.pluginReady ? "已安装并验证" : "需要设置")
                    LabeledContent(vm.desktopSetup.terminalApp.isEmpty ? "iTerm2" : vm.desktopSetup.terminalApp,
                                   value: vm.desktopSetup.itermReady ? "自动化已授权" : "需要设置")
                    if !vm.desktopSetup.ready {
                        Button("完成 Mac 授权与安装…") { core.desktopSetup(.show) }
                        Text(vm.desktopSetup.pluginMessage).font(.footnote).foregroundStyle(.secondary)
                    }
                }
                #endif
                LabeledContent("Screen", value: vm.self_p.state.locked ? "locked" : (vm.self_p.state.covered ? "covered" : "unlocked"))
                #if os(macOS)
                if !vm.embedded {
                    Button(vm.self_p.state.covered ? "Uncover the screen…" : "Cover the screen (⌃⌥⌘L)") {
                        core.setCover(!vm.self_p.state.covered)
                    }
                    Text("The cover blacks out every display and keeps the Mac awake while its apps stay reachable from your other devices; dropping it asks for Touch ID or your password.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                #endif
                if vm.embedded, let line = core.sshPublicKeyLine() {
                    VStack(alignment: .leading) {
                        Text("SSH public key").font(.subheadline)
                        Text(line).font(.system(.caption2, design: .monospaced)).textSelection(.enabled).lineLimit(3)
                        Button("Copy") { copyToPasteboard(line) }
                    }
                }
                LabeledContent("Version", value: vm.self_p.version.isEmpty ? "not reported" : vm.self_p.version)
                LabeledContent("Commit") {
                    Text(vm.self_p.commit.isEmpty ? "not reported" : vm.self_p.commit)
                        .font(.system(.body, design: .monospaced)).textSelection(.enabled)
                }
            }
        }
        .groupedForm()
        .navigationTitle("Settings")
        .sheet(item: $deciding) { join in
            AccessEditorSheet(title: "Approve \(join.name)", access: vm.joinAccess) { scopes in
                core.decideJoin(device: join.deviceID, approve: true, scopes: scopes)
            }
        }
        .onAppear {
            if !loaded { policy = vm.policy; loaded = true }
        }
        .onChange(of: vm.policy) { _, new in policy = new }
    }
}
#endif

private func updateStatus(_ status: String) -> String {
    switch status {
    case "checking": "正在检查更新…"
    case "available": "有新版本可用"
    case "downloading": "正在下载并验证更新…"
    case "ready": "更新已就绪，安装将短暂中断远程连接"
    case "waiting": "正在等待保护事务结束，后台服务保持运行…"
    case "installing": "正在安装并重启…"
    case "current": "已是最新版本"
    case "error": "更新未完成，可重试"
    default: "从 linsheng.dev 获取签名更新"
    }
}

#if canImport(SwiftUI)
struct DeviceLockedUseToggle: View {
    let core: MusterCore
    let control: Muster_App_DeviceLockedUseView
    var body: some View {
        Toggle(isOn: Binding(get: { control.on }, set: { core.setDeviceLockedUse($0, device: control.deviceID) })) {
            VStack(alignment: .leading, spacing: 3) {
                Label("Locked Use", systemImage: "lock.shield")
                Text(control.status).font(.caption).foregroundStyle(.secondary)
            }
        }
        .disabled(!control.enabled)
        .accessibilityIdentifier("device-locked-use-\(control.deviceID)")
    }
}
#endif
