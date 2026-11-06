#if canImport(SwiftUI)
import MusterCore
import SwiftUI

/// Presentation identity and user input only. Rust validates and submits it.
struct MulticaEditor: Identifiable {
    let id = UUID()
    let title: String
    let request: Muster_V1_MulticaMutation
}

struct MulticaOperationEditor: View {
    let core: MusterCore
    let input: MulticaEditor
    let page: Muster_V1_MulticaPage
    let workspace: Muster_App_MulticaWorkspaceView
    @Environment(\.dismiss) private var dismiss
    @State private var submitted = false
    @State private var request: Muster_V1_MulticaMutation

    init(core: MusterCore, input: MulticaEditor, page: Muster_V1_MulticaPage, workspace: Muster_App_MulticaWorkspaceView) {
        self.core = core; self.input = input; self.page = page; self.workspace = workspace
        _request = State(initialValue: input.request)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(input.title).font(.title2.bold())
            Form {
                switch request.action {
                case .agent:
                    TextField("名称", text: $request.agent.name)
                    TextField("模型（留空使用运行时默认值）", text: $request.agent.model)
                    Stepper("并发任务上限：\(request.agent.maxConcurrentTasks)", value: $request.agent.maxConcurrentTasks, in: 1...100)
                    Section("描述") { TextEditor(text: $request.agent.description_p).frame(minHeight: 70) }
                    Section("系统指令") { TextEditor(text: $request.agent.instructions).frame(minHeight: 160) }
                    Text("模型修改只影响新任务；不会取消已有执行。环境变量、凭据和身份配置请在 Multica 平台管理。")
                        .font(.footnote).foregroundStyle(.secondary)
                case .issue:
                    TextField("标题", text: $request.issue.title)
                    Section("描述") { TextEditor(text: $request.issue.description_p).frame(minHeight: 180) }
                    Text(request.issue.id.isEmpty ? "新任务保存到待办积压，不分派智能体、不立即执行。" : "保存标题和描述，不启动新的执行。")
                        .font(.footnote).foregroundStyle(.secondary)
                case .issueAction:
                    actionFields
                case nil:
                    Text("请选择操作")
                }
            }.formStyle(.grouped)
            if submitted && !workspace.mutationMessage.isEmpty {
                Text(workspace.mutationMessage).font(.callout).textSelection(.enabled)
            }
            HStack {
                Button("关闭") { dismiss() }.disabled(workspace.mutationBusy)
                Spacer()
                if workspace.mutationBusy { ProgressView().controlSize(.small) }
                Button("确认提交") { submitted = true; core.multicaMutate(request) }
                    .buttonStyle(.borderedProminent)
                    .disabled(workspace.mutationBusy || workspace.mutationUncertain)
            }
        }
        .padding(20)
        .frame(idealWidth: 580, idealHeight: 560)
        .interactiveDismissDisabled(workspace.mutationBusy)
        .onChange(of: workspace.mutationResult) { _, _ in
            if workspace.mutationApplied || workspace.mutationUncertain { dismiss() }
        }
        .onChange(of: workspace.selectedDevice) { _, _ in dismiss() }
        .onChange(of: page.workspaceID) { _, _ in dismiss() }
    }

    @ViewBuilder private var actionFields: some View {
        switch request.issueAction.kind {
        case .assign:
            Picker("分派给", selection: $request.issueAction.value) {
                Text("请选择智能体").tag("")
                ForEach(page.agents, id: \.id) { agent in Text(agent.name).tag(agent.id) }
            }
            Text("仅修改分派，不主动启动新执行。App 任务调度接通后再开放启动和续跑。")
        case .status:
            Picker("状态", selection: $request.issueAction.value) {
                Text("请选择状态").tag("")
                ForEach(page.statusChoices, id: \.key) { status in Text(status.title).tag(status.key) }
            }
            Text("保存状态时不主动启动新执行；选择已取消会停止未结束的任务。")
        case .comment:
            Section("评论") { TextEditor(text: $request.issueAction.value).frame(minHeight: 180) }
            Text("目前支持普通评论。@提及调度将在 App 任务桥接接通后开放。")
        case .rerun:
            Text("重新排队当前分配智能体的任务，使用全新会话，不继承上次上下文。该智能体在此任务中已有的排队或运行会被取消。")
        case .cancel:
            Text("取消选中的这次执行并请求中断。已经产生的代码、文件和评论不会撤销。")
        default:
            Text("不支持的操作")
        }
    }
}
#endif
