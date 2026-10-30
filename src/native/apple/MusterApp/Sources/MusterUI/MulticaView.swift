#if canImport(SwiftUI)
import MusterCore
import SwiftUI

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
            if status.installed { configuration }
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
            if status.installed { loginInstructions }
        }
        .onAppear { load() }
        .onChange(of: status.dataDir) { _, _ in if !loaded { load() } }
        .sheet(isPresented: $showInstallation) { installationDialog }
    }

    private var configuration: some View {
        Group {
            TextField("数据目录", text: $draft.dataDir)
            TextField("容器名称", text: $draft.containerName).disabled(status.installed)
            TextField("安装脚本 HTTPS 地址", text: $draft.installerURL)
            TextField("Multica 服务 HTTPS 地址", text: $draft.serverURL)
            TextField("Multica 页面 HTTPS 地址", text: $draft.appURL)
            Toggle("保持 Multica 容器和 daemon 运行", isOn: $draft.keepAlive)
            Text("新安装默认使用 ~/Documents/Multica。修改已有容器的数据目录前，需关闭保活、停止容器，完成数据与挂载迁移并移除旧路径，再保存设置。")
                .font(.footnote).foregroundStyle(.secondary)
        }
    }

    private var loginInstructions: some View {
        Group {
            if !status.loginCommand.isEmpty {
                Text("下一步：在终端完成 Multica 登录，再返回此处刷新状态。安装完成不代表已登录或服务正在运行。")
                    .font(.footnote)
                Text(status.loginCommand).font(.caption.monospaced()).textSelection(.enabled)
            }
        }
    }

    private var installationDialog: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("安装 Multica").font(.title2.bold())
            if showPreparation {
                Form {
                    Section("1. 准备安装") {
                        Text("请先安装 Apple container，并向部署管理员获取可信的 HTTPS 安装脚本、服务和页面地址。安装需要网络连接，下载镜像和依赖可能需要几分钟。")
                        configuration
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
                    if status.installed { loginInstructions }
                }
            }
            Divider()
            HStack {
                Button(status.installation.busy ? "后台继续" : "关闭") { showInstallation = false }
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
    @State private var section = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
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
                Label(status.running ? "运行中" : "未运行", systemImage: status.running ? "checkmark.circle" : "pause.circle")
                    .foregroundStyle(.secondary)
                if page.busy { ProgressView().controlSize(.small) }
                Button("刷新", systemImage: "arrow.clockwise") { refresh() }.disabled(page.busy)
            }
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
                ContentUnavailableView("没有可用工作区", systemImage: "square.stack", description: Text("请确认容器已启动，并在设置中完成 Multica 登录。"))
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
        .padding()
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { core.navigate(to: .settings) } label: { Label("Settings", systemImage: "gearshape") }
            }
        }
    }

    @ViewBuilder private var listing: some View {
        if section == 0 {
            VStack {
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
                } else if page.hasAgent {
                    Text(page.agent.name).font(.title2)
                    LabeledContent("状态", value: page.agent.status)
                    LabeledContent("模型", value: page.agent.model)
                    LabeledContent("运行时", value: page.agent.runtimeID)
                    Divider()
                    Text(page.agent.description_p).frame(maxWidth: .infinity, alignment: .leading)
                }
            }.textSelection(.enabled).padding(.trailing, 8)
        }
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
        core.multica(.init())
        query(issue: page.hasIssue ? page.issue.id : "", agent: page.hasAgent ? page.agent.id : "", offset: page.offset)
    }
}
#endif
