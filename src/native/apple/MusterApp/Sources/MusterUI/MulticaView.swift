#if canImport(SwiftUI)
import MusterCore
import SwiftUI

struct MulticaSettings: View {
    let core: MusterCore
    let status: Muster_V1_MulticaStatus
    @State private var draft = Muster_V1_MulticaRequest()
    @State private var loaded = false

    var body: some View {
        Section("Multica") {
            Text(status.message).textSelection(.enabled)
            TextField("数据目录", text: $draft.dataDir)
            TextField("容器名称", text: $draft.containerName)
                .disabled(status.installed)
            TextField("安装脚本 HTTPS 地址", text: $draft.installerURL)
            TextField("Multica 服务 HTTPS 地址", text: $draft.serverURL)
            TextField("Multica 页面 HTTPS 地址", text: $draft.appURL)
            Toggle("保持 Multica 容器和 daemon 运行", isOn: $draft.keepAlive)
            Text("新安装默认使用 ~/Documents/Multica。修改已有容器的数据目录前，需关闭保活、停止容器，完成数据与挂载迁移并移除旧路径，再保存设置。")
                .font(.footnote).foregroundStyle(.secondary)
            HStack {
                Button(status.installed ? "保存设置" : "安装 Multica") {
                    var request = draft
                    request.action = status.installed ? .save : .install
                    core.multica(request)
                }.disabled(status.busy)
                Button("刷新状态") { core.multica(.init()) }.disabled(status.busy)
                if status.busy { ProgressView().controlSize(.small) }
            }
            if !status.installed {
                Text("安装会运行你填写的可信 Multica 安装脚本，并安装 Apple 容器内及本机的 Multica CLI。")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            if status.installed && !status.loginCommand.isEmpty {
                Text("首次使用请在终端完成登录：").font(.footnote)
                Text(status.loginCommand).font(.caption.monospaced()).textSelection(.enabled)
            }
        }
        .onAppear { load() }
        .onChange(of: status.dataDir) { _, _ in if !loaded { load() } }
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
