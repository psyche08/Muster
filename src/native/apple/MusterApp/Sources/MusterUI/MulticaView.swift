#if canImport(SwiftUI)
import MusterCore
import SwiftUI

/// The editable Multica container configuration, shared by the Settings
/// section (an installed container) and the installation sheet (before one
/// exists). The shell only collects text; Rust validates and owns it.
private struct MulticaConfigurationFields: View {
    @Binding var draft: Muster_V1_MulticaRequest
    let containerNameDisabled: Bool

    var body: some View {
        Group {
            TextField("数据目录", text: $draft.dataDir)
            TextField("容器名称", text: $draft.containerName).disabled(containerNameDisabled)
            TextField("安装脚本 HTTPS 地址", text: $draft.installerURL)
            TextField("Multica 服务 HTTPS 地址", text: $draft.serverURL)
            TextField("Multica 页面 HTTPS 地址", text: $draft.appURL)
            Toggle("保持 Multica 容器和 daemon 运行", isOn: $draft.keepAlive)
            Text("新安装默认使用 ~/Documents/Multica。修改已有容器的数据目录前，需关闭保活、停止容器，完成数据与挂载迁移并移除旧路径，再保存设置。")
                .font(.footnote).foregroundStyle(.secondary)
        }
    }
}

/// Login stays a separate terminal operation; the shell shows the command
/// Rust built and never runs it.
private struct MulticaLoginInstructions: View {
    let status: Muster_V1_MulticaStatus

    var body: some View {
        Group {
            if !status.loginCommand.isEmpty {
                Text("下一步：在终端完成 Multica 登录，再返回此处刷新状态。安装完成不代表已登录或服务正在运行。")
                    .font(.footnote)
                Text(status.loginCommand).font(.caption.monospaced()).textSelection(.enabled)
            }
        }
    }
}

/// The guided install sheet: preparation before the start action and the
/// Rust-owned stage/log projection afterwards. Closing it does not cancel
/// the daemon-owned installation.
private struct MulticaInstallationDialog: View {
    let core: MusterCore
    let status: Muster_V1_MulticaStatus
    @Binding var draft: Muster_V1_MulticaRequest
    @Binding var showPreparation: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("安装 Multica").font(.title2.bold())
            if showPreparation {
                Form {
                    Section("1. 准备安装") {
                        Text("请先安装 Apple container，并向部署管理员获取可信的 HTTPS 安装脚本、服务和页面地址。安装需要网络连接，下载镜像和依赖可能需要几分钟。")
                        MulticaConfigurationFields(draft: $draft, containerNameDisabled: status.installed)
                    }
                    Section("2. 安装并查看日志") {
                        Text("点击开始安装后，将准备数据目录、创建容器、安装容器和本机 Multica CLI，并检查服务状态。安装脚本将在容器及本机执行，请确认其来源可信。")
                    }
                    Section("3. 登录并验证") {
                        Text("安装后按提示在终端登录，返回设置刷新状态。")
                    }
                }.formStyle(.grouped)
            } else {
                HStack {
                    if status.installation.busy { ProgressView().controlSize(.small) }
                    Text(status.installation.stage).font(.headline)
                }
                Text("安装日志（最近 64 KiB，常见凭据内容会隐藏）")
                    .font(.caption).foregroundStyle(.secondary)
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            Text(status.installation.log.isEmpty ? "等待安装服务输出…" : status.installation.log)
                                .font(.system(.caption, design: .monospaced))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Color.clear.frame(height: 1).id("installation-log-end")
                        }.padding(12)
                    }
                    .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))
                    .onChange(of: status.installation.log) { _, _ in
                        proxy.scrollTo("installation-log-end", anchor: .bottom)
                    }
                }
                if status.installation.finished {
                    Text(status.message).font(.footnote).textSelection(.enabled)
                    if status.installed { MulticaLoginInstructions(status: status) }
                }
            }
            Divider()
            HStack {
                Button(status.installation.busy ? "后台继续" : "关闭") { showPreparation = false; dismiss() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                if showPreparation {
                    Button("开始安装") {
                        showPreparation = false
                        var request = draft
                        request.action = .install
                        core.multica(request)
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(status.busy)
                } else if !status.busy {
                    Button("刷新状态") { core.multica(.init()) }
                    if !status.installed {
                        Button("返回配置") { showPreparation = true }
                    }
                }
            }
        }
        .padding(24)
        .frame(minWidth: 580, idealWidth: 660, minHeight: 560, idealHeight: 640)
        .interactiveDismissDisabled(status.installation.busy)
    }
}

struct MulticaSettings: View {
    let core: MusterCore
    let status: Muster_V1_MulticaStatus
    @State private var draft = Muster_V1_MulticaRequest()
    @State private var loaded = false
    @State private var showInstallation = false
    @State private var showPreparation = true

    var body: some View {
        Section("Multica") {
            Text(status.message).textSelection(.enabled)
            if status.migrationRequired {
                Label("数据目录与容器挂载不一致：请关闭保活、停止容器并完成离线迁移后重新保存设置。", systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
                    .accessibilityIdentifier("multica-migration-warning")
            }
            if !status.version.isEmpty {
                LabeledContent("Multica 版本", value: status.version)
                    .accessibilityIdentifier("multica-version-settings")
            }
            if status.installed {
                MulticaConfigurationFields(draft: $draft, containerNameDisabled: status.installed)
            }
            HStack {
                Button(status.installed ? "保存设置" : "安装 Multica") {
                    if status.installed {
                        var request = draft
                        request.action = .save
                        core.multica(request)
                    } else {
                        showPreparation = !status.installation.busy
                        showInstallation = true
                    }
                }.disabled(status.busy)
                Button("刷新状态") { core.multica(.init()) }.disabled(status.busy)
                if status.hasInstallation {
                    Button("查看安装过程") {
                        showPreparation = false
                        showInstallation = true
                    }
                }
                if status.busy { ProgressView().controlSize(.small) }
            }
            if !status.installed {
                Text("通过安装向导配置容器和数据目录，查看安装步骤与实时日志。")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            if status.installed { MulticaLoginInstructions(status: status) }
        }
        .onAppear { load() }
        .onChange(of: status.dataDir) { _, _ in if !loaded { load() } }
        .sheet(isPresented: $showInstallation) {
            MulticaInstallationDialog(core: core, status: status, draft: $draft, showPreparation: $showPreparation)
        }
    }

    private func load() {
        guard !loaded && !status.dataDir.isEmpty else { return }
        loaded = true
        draft.dataDir = status.dataDir
        draft.containerName = status.containerName
        draft.installerURL = status.installerURL
        draft.serverURL = status.serverURL
        draft.appURL = status.appURL
        draft.keepAlive = status.installed ? status.keepAlive : true
    }
}

struct MulticaView: View {
    let core: MusterCore
    let status: Muster_V1_MulticaStatus
    let page: Muster_V1_MulticaPage
    let workspace: Muster_App_MulticaWorkspaceView
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var sharing = false
    @State private var sharedWorkspaces: Set<String> = []
    @State private var section = 0
    @State private var editor: MulticaEditor?
    @State private var showInstallation = false
    @State private var installationPreparation = false
    @State private var installationDraft = Muster_V1_MulticaRequest()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Picker("设备", selection: Binding(get: { workspace.selectedDevice }, set: { core.multicaSelect($0) })) {
                    ForEach(workspace.devices, id: \.deviceID) { target in
                        Text(target.name + (target.connected ? "" : " · 离线")).tag(target.deviceID)
                    }
                }
                if let target = workspace.devices.first(where: { $0.deviceID == workspace.selectedDevice }) {
                    Text(target.service.canControl ? "已授予控制权限" : "只读")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if workspace.selectedIsLocal && workspace.localService.canManage { sharingSettings }
            if workspace.selectedIsLocal && !workspace.mutationMessage.isEmpty {
                HStack {
                    Text(workspace.mutationMessage).font(.callout).textSelection(.enabled)
                    if workspace.mutationBusy { ProgressView().controlSize(.small) }
                    if workspace.mutationUncertain {
                        Button("已核对当前结果") { core.multicaAcknowledgeMutation() }
                    }
                }
            }
            HStack {
                Picker("工作区", selection: Binding(get: { page.workspaceID }, set: { workspace in
                    var request = Muster_V1_MulticaBrowseRequest()
                    request.workspaceID = workspace
                    core.multicaBrowse(request)
                })) {
                    ForEach(page.workspaces, id: \.id) { workspace in
                        Text(workspace.name).tag(workspace.id)
                    }
                }.frame(maxWidth: 320)
                Spacer()
                if workspace.selectedIsLocal {
                    Label(status.running ? "运行中" : "未运行", systemImage: status.running ? "checkmark.circle" : "pause.circle")
                        .foregroundStyle(.secondary)
                }
                if page.busy { ProgressView().controlSize(.small) }
                Button("刷新", systemImage: "arrow.clockwise") { refresh() }.disabled(page.busy)
            }
            if workspace.selectedIsLocal && !status.message.isEmpty {
                Text(status.message)
                    .font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                    .accessibilityIdentifier("multica-status-message")
            }
            if workspace.selectedIsLocal && status.migrationRequired {
                Label("数据目录与容器挂载不一致：请关闭保活、停止容器并完成离线迁移后重新保存设置。", systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
                    .accessibilityIdentifier("multica-migration-warning-tab")
            }
            if workspace.selectedIsLocal && !status.version.isEmpty {
                Text("Multica 版本：" + status.version)
                    .font(.caption).foregroundStyle(.secondary)
                    .accessibilityIdentifier("multica-version")
            }
            if workspace.selectedIsLocal { installationBanner }
            if !page.error.isEmpty {
                Label(page.error, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.orange).textSelection(.enabled)
            }
            Picker("内容", selection: $section) {
                Text("任务").tag(0)
                Text("代理").tag(1)
                Text("运行时").tag(2)
            }.pickerStyle(.segmented)
            if page.workspaces.isEmpty && !page.busy {
                ContentUnavailableView("没有可用工作区", systemImage: "square.stack", description: Text("请在提供服务的设备上确认运行与登录状态，并检查共享范围和查看权限。"))
            } else {
                if sizeClass == .compact {
                    if page.hasIssue || page.hasAgent {
                        Button("返回列表", systemImage: "chevron.left") { query() }
                        detail
                    } else { listing }
                } else {
                HStack(alignment: .top, spacing: 16) {
                    listing.frame(minWidth: 260, maxWidth: .infinity, maxHeight: .infinity)
                    if page.hasIssue || page.hasAgent {
                        Divider()
                        detail.frame(minWidth: 280, maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
                }
            }
        }
        .padding()
        .sheet(item: $editor) { input in
            MulticaOperationEditor(core: core, input: input, page: page, workspace: workspace)
        }
        .onAppear { loadSharing() }
        .onChange(of: workspace.localService) { _, _ in loadSharing() }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { core.navigate(to: .settings) } label: { Label("Settings", systemImage: "gearshape") }
            }
        }
        .sheet(isPresented: $showInstallation) {
            MulticaInstallationDialog(core: core, status: status, draft: $installationDraft, showPreparation: $installationPreparation)
        }
    }

    private var sharingSettings: some View {
        DisclosureGroup("向已授权设备开放 Multica") {
            VStack(alignment: .leading, spacing: 8) {
                Toggle("开放共享", isOn: $sharing)
                Text("共享勾选工作区的任务、代理和运行时信息。其他设备仍需查看权限；会话操作尚不可用。安装、登录和配置只能在本机完成。")
                    .font(.footnote).foregroundStyle(.secondary)
                ForEach(page.workspaces, id: \.id) { item in
                    Toggle(item.name, isOn: Binding(get: { sharedWorkspaces.contains(item.id) }, set: { selected in
                        if selected { sharedWorkspaces.insert(item.id) } else { sharedWorkspaces.remove(item.id) }
                    }))
                }
                Button("保存共享设置") {
                    core.multicaService(.with { $0.action = .save; $0.offered = sharing; $0.workspaces = sharedWorkspaces.sorted() })
                }
            }.padding(.vertical, 8)
        }
    }
    private func loadSharing() {
        sharing = workspace.localService.offered
        sharedWorkspaces = Set(workspace.localService.workspaces)
    }

    /// Rust owns the installation projection; the tab only surfaces an
    /// in-flight stage or a finished-but-failed outcome and can reopen the
    /// same guided sheet.
    @ViewBuilder private var installationBanner: some View {
        if status.installation.busy {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text(status.installation.stage.isEmpty ? "正在安装 Multica…" : status.installation.stage)
                    .font(.callout)
                Spacer()
                Button("查看安装过程") { openInstallation() }
                    .accessibilityIdentifier("multica-install-banner-open")
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("multica-install-banner")
        } else if status.installation.finished && !status.installation.succeeded {
            HStack(spacing: 8) {
                Label("上次 Multica 安装未完成，请查看日志后重试。", systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
                Spacer()
                Button("查看安装过程") { openInstallation() }
                    .accessibilityIdentifier("multica-install-banner-open")
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("multica-install-banner")
        }
    }

    @ViewBuilder private var listing: some View {
        if section == 0 {
            VStack {
                if workspace.selectedIsLocal {
                    Button("新建任务", systemImage: "plus") {
                        editIssue(.init())
                    }.disabled(operationsDisabled)
                }
                List(page.issues, id: \.id) { issue in
                    Button { query(issue: issue.id) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(issue.title).font(.headline)
                            Text([issue.identifier, issue.status, issue.priority].filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.caption).foregroundStyle(.secondary)
                        }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                    }.buttonStyle(.plain)
                }
                HStack {
                    Button("上一页") { query(offset: page.previousOffset) }.disabled(page.offset == 0 || page.busy)
                    Spacer()
                    if page.issues.isEmpty && !page.busy { Text("暂无任务").foregroundStyle(.secondary) }
                    Spacer()
                    Button("下一页") { query(offset: page.nextOffset) }.disabled(!page.hasMore_p || page.busy)
                }
            }
        } else if section == 1 {
            List(page.agents, id: \.id) { agent in
                Button { query(agent: agent.id) } label: {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(agent.name).font(.headline)
                        Text([agent.status, agent.model].filter { !$0.isEmpty }.joined(separator: " · "))
                            .font(.caption).foregroundStyle(.secondary)
                    }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
        } else {
            List(page.runtimes, id: \.id) { runtime in
                VStack(alignment: .leading, spacing: 6) {
                    Text(runtime.name).font(.headline)
                    Text([runtime.provider, runtime.status].filter { !$0.isEmpty }.joined(separator: " · "))
                    if !runtime.deviceInfo.isEmpty { Text(runtime.deviceInfo).font(.caption).foregroundStyle(.secondary) }
                    if !runtime.lastSeenAt.isEmpty { Text("最近在线：" + runtime.lastSeenAt).font(.caption).foregroundStyle(.secondary) }
                }.textSelection(.enabled)
            }
        }
    }

    private var detail: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("详情").font(.headline)
                    Spacer()
                    Button("关闭", systemImage: "xmark") { query(offset: page.offset) }
                }
                if page.hasIssue {
                    Text(page.issue.title).font(.title2)
                    LabeledContent("编号", value: page.issue.identifier)
                    LabeledContent("状态", value: page.issue.status)
                    LabeledContent("优先级", value: page.issue.priority)
                    LabeledContent("更新时间", value: page.issue.updatedAt)
                    Divider()
                    Text(page.issue.description_p).frame(maxWidth: .infinity, alignment: .leading)
                    if workspace.selectedIsLocal {
                        HStack {
                            Button("编辑") { editIssue(page.issue) }
                            Menu("任务操作") {
                                Button("分派智能体") { issueAction(.assign, title: "分派智能体") }
                                Button("修改状态") { issueAction(.status, title: "修改任务状态") }
                                Button("添加评论") { issueAction(.comment, title: "添加评论") }
                                Button("启动 / 续跑：App 任务桥接待接通") {}.disabled(true)
                            }
                        }.disabled(operationsDisabled)
                    }
                    Divider()
                    Text("执行历史（最近 200 条）").font(.headline)
                    if page.runs.isEmpty { Text("暂无执行记录").foregroundStyle(.secondary) }
                    ForEach(page.runs, id: \.id) { run in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(run.status).font(.subheadline.bold())
                            Text(run.createdAt).font(.caption).foregroundStyle(.secondary)
                            if !run.failureReason.isEmpty { Text(run.failureReason).font(.caption) }
                            if workspace.selectedIsLocal && run.canCancel {
                                Button("取消这次执行", role: .destructive) {
                                    issueAction(.cancel, title: "取消执行", value: run.id)
                                }.disabled(operationsDisabled)
                            }
                        }.padding(.vertical, 4)
                    }
                    Divider()
                    Text("最近评论").font(.headline)
                    ForEach(page.comments, id: \.id) { comment in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(comment.authorType + " · " + comment.createdAt).font(.caption).foregroundStyle(.secondary)
                            Text(comment.content)
                        }.padding(.vertical, 4)
                    }
                } else if page.hasAgent {
                    Text(page.agent.name).font(.title2)
                    LabeledContent("状态", value: page.agent.status)
                    LabeledContent("模型", value: page.agent.model)
                    LabeledContent("运行时", value: page.agent.runtimeID)
                    Divider()
                    Text(page.agent.description_p).frame(maxWidth: .infinity, alignment: .leading)
                    if workspace.selectedIsLocal && page.agent.editable {
                        Button("编辑智能体配置") { editAgent(page.agent) }.disabled(operationsDisabled)
                    }
                }
            }.textSelection(.enabled).padding(.trailing, 8)
        }
    }

    private var operationsDisabled: Bool { page.busy || workspace.mutationBusy || workspace.mutationUncertain }
    private func editAgent(_ agent: Muster_V1_MulticaAgent) {
        let request = Muster_V1_MulticaMutation.with {
            $0.workspaceID = page.workspaceID
            $0.agent = .with {
                $0.id = agent.id; $0.revision = agent.revision; $0.name = agent.name
                $0.description_p = agent.description_p; $0.instructions = agent.instructions
                $0.model = agent.model; $0.maxConcurrentTasks = agent.maxConcurrentTasks
            }
        }
        editor = MulticaEditor(title: "编辑智能体", request: request)
    }
    private func editIssue(_ issue: Muster_V1_MulticaIssue) {
        let request = Muster_V1_MulticaMutation.with {
            $0.workspaceID = page.workspaceID
            $0.issue = .with {
                $0.id = issue.id; $0.revision = issue.revision; $0.title = issue.title; $0.description_p = issue.description_p
            }
        }
        editor = MulticaEditor(title: issue.id.isEmpty ? "新建任务" : "编辑任务", request: request)
    }
    private func issueAction(_ kind: Muster_V1_MulticaIssueAction.Kind, title: String, value: String = "") {
        let request = Muster_V1_MulticaMutation.with {
            $0.workspaceID = page.workspaceID
            $0.issueAction = .with { $0.kind = kind; $0.id = page.issue.id; $0.revision = page.issue.revision; $0.value = value }
        }
        editor = MulticaEditor(title: title, request: request)
    }

    private func query(issue: String = "", agent: String = "", offset: UInt32 = 0) {
        var request = Muster_V1_MulticaBrowseRequest()
        request.workspaceID = page.workspaceID
        request.issueID = issue
        request.agentID = agent
        request.offset = offset
        core.multicaBrowse(request)
    }
    private func refresh() {
        if workspace.selectedIsLocal { core.multica(.init()) }
        query(issue: page.hasIssue ? page.issue.id : "", agent: page.hasAgent ? page.agent.id : "", offset: page.offset)
    }

    private func openInstallation() {
        installationDraft.dataDir = status.dataDir
        installationDraft.containerName = status.containerName
        installationDraft.installerURL = status.installerURL
        installationDraft.serverURL = status.serverURL
        installationDraft.appURL = status.appURL
        installationDraft.keepAlive = status.installed ? status.keepAlive : true
        installationPreparation = false
        showInstallation = true
    }
}
#endif
