import XCTest
import SwiftUI
import MusterCore
import SwiftProtobuf
@testable import MusterUI

final class ConversationLayoutTests: XCTestCase {
    @MainActor
    func testPhoneConversationAndCompactComposer() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let core = try MusterCore(config: .phone(dir: directory, name: "Layout fixture"), keys: SoftwareKeys(),
                                  onViewModel: { _ in }, onEffect: { _, _ in })
        defer { core.shutdown() }
        var vm = ViewModel()
        vm.chat.session = .fixture {
            $0.id = "fixture"; $0.title = "监控 Muster 日志并分析错误"
            $0.provider = "codex"; $0.deviceName = "mac-c"; $0.state = "running"
        }
        vm.chat.attachmentCountLimit = 4
        let user = Muster_V1_AgentMessage.fixture {
            $0.id = "user"; $0.role = "user"; $0.kind = "text"
            $0.text = "监控 mac-a、mac-b 和 mac-c 的 Muster 日志，如果出现 error，就进行分析，重复问题不要重复排查。监控 1 小时。"
        }
        let commentary = Muster_V1_AgentMessage.fixture {
            $0.id = "commentary"; $0.role = "assistant"; $0.kind = "text"; $0.phase = "commentary"
            $0.text = "我会先确认三台机器的连接方式和 Muster 日志位置，持续监控 1 小时；对新出现的 error 做分析，并按错误特征去重，避免重复排查。"
        }
        let tool = Muster_V1_AgentMessage.fixture {
            $0.id = "tool"; $0.kind = "tool_call"; $0.toolName = "exec_command"; $0.text = "pwd && rg --files"
        }
        let followup = Muster_V1_AgentMessage.fixture {
            $0.id = "followup"; $0.role = "assistant"; $0.kind = "text"
            $0.text = "已连通 mac-a 和 mac-b；本机是 mac-c，三台的 Muster 守护进程都在运行。接下来会建立日志增量读取基线。"
        }
        vm.chat.messageCount = 4
        let work = Muster_App_ChatBlock.fixture {
            $0.id = "working"; $0.kind = "working"; $0.title = "Working"; $0.defaultExpanded = true
            $0.children = [
                .fixture { $0.id = commentary.id; $0.kind = "message"; $0.message = commentary },
                .fixture {
                    $0.id = "activity"; $0.kind = "activity"; $0.title = "2 commands · 1 file read"
                    $0.activity = [tool]
                    $0.activityRows = [.fixture {
                        $0.id = tool.id; $0.title = "exec_command · pwd && rg --files"
                        $0.toolName = "exec_command"; $0.category = "shell"
                        $0.detail = "pwd && rg --files"; $0.output = "/repo"; $0.isResult = true
                    }]
                },
                .fixture { $0.id = followup.id; $0.kind = "message"; $0.message = followup }
            ]
        }
        vm.chat.timeline = [.fixture { $0.id = user.id; $0.kind = "message"; $0.message = user }, work]
        let open = UIHostingController(rootView: WorkingGroup(block: work))
        var collapsed = work; collapsed.defaultExpanded = false
        let closed = UIHostingController(rootView: WorkingGroup(block: collapsed))
        XCTAssertGreaterThan(open.sizeThatFits(in: CGSize(width: 370, height: 900)).height,
                             closed.sizeThatFits(in: CGSize(width: 370, height: 900)).height + 100,
                             "Working must render progress text by default while its tool batch stays collapsed")
        for width in [CGFloat(320), 402] {
            for attachmentLimit in [UInt32(0), 4] {
                var chat = vm.chat
                chat.attachmentCountLimit = attachmentLimit
                for style in [ConversationStyle.codex, .claude] {
                    let composer = UIHostingController(rootView: ChatComposer(core: core, chat: chat)
                        .environment(\.conversationStyle, style))
                    let size = composer.sizeThatFits(in: CGSize(width: width - 24, height: 500))
                    XCTAssertLessThan(size.height, 125, "The full-width editor and touch-sized toolbar must leave room for the conversation")
                }
            }
        }
        vm.chat.composerSuggestion = "继续检查发送失败，并保留现有草稿。"
        vm.chat.composerPlaceholder = "Work with Codex"
        vm.chat.providerLabel = "Codex"
        for scheme in [ColorScheme.light, .dark] {
            let content = NavigationStack {
                ChatView(core: core, vm: vm)
                    .toolbar { ToolbarItem(placement: .topBarLeading) { Image(systemName: "chevron.left") } }
            }.environment(\.conversationStyle, .codex).environment(\.colorScheme, scheme)
            try snapshot(content, name: "phone-\(scheme == .dark ? "dark" : "light")")
        }
    }

    @MainActor
    func testConsecutiveToolBatchHasOneSummaryAndThreeCallRows() throws {
        let block = Muster_App_ChatBlock.fixture {
            $0.id = "batch"; $0.kind = "activity"; $0.title = "2 commands · 1 file read"
            $0.activityRows = (0..<3).map { index in
                .fixture {
                    $0.id = "call-\(index)"; $0.title = ["exec_command · pwd", "exec_command · rg --files", "Read · AGENTS.md"][index]
                    $0.category = index == 2 ? "read" : "shell"
                    $0.detail = "Arguments"; $0.output = "Recorded output"; $0.isResult = true
                }
            }
        }
        let collapsed = UIHostingController(rootView: ActivityGroup(block: block))
        let expanded = UIHostingController(rootView: ActivityGroup(block: block, expanded: true))
        XCTAssertLessThan(collapsed.sizeThatFits(in: CGSize(width: 370, height: 900)).height, 60)
        XCTAssertGreaterThan(expanded.sizeThatFits(in: CGSize(width: 370, height: 900)).height, 100)
        try snapshot(VStack(alignment: .leading, spacing: 16) {
            Text("Working").foregroundStyle(.secondary)
            Text("先检查目录与项目说明。")
            ActivityGroup(block: block, expanded: true)
            Text("三次工具调用已显示在同一个分组下，输出保留在各自详情中。")
            Spacer()
        }.padding(16), name: "working-tool-batch")
    }

    @MainActor
    func testExpandedToolStaysWithinPhoneWidth() throws {
        let row = Muster_App_ChatActivityRow.fixture {
            $0.id = "shell"; $0.title = "exec_command · pwd && rg --files -g AGENTS.md"
            $0.category = "shell"; $0.toolName = "exec_command"; $0.isResult = true
            $0.detail = "$ pwd && rg --files -g AGENTS.md\n/Volumes/Workspace/Projects/Muster\n" + String(repeating: "long-output-without-spaces/", count: 30)
        }
        let content = VStack(alignment: .leading, spacing: 16) {
            Text("已浏览文件并执行搜索").foregroundStyle(.secondary)
            ToolActivityRow(row: row, expanded: true)
            Text("工具输出保持在卡片内，可以独立滚动查看。")
            Spacer()
        }.padding(16).environment(\.conversationStyle, .codex)
        let host = UIHostingController(rootView: content)
        let size = host.sizeThatFits(in: CGSize(width: 320, height: 874))
        XCTAssertLessThanOrEqual(size.width, 320)
        try snapshot(content, name: "phone-tool-expanded")
    }

    @MainActor
    func testLongMarkdownKeepsHeightBeforeFollowingMessage() throws {
        let markdown = """
        **还没做**
        - `GATEWAY_LEG_STAT` / `GATEWAY_PATH_STAT` 的聚合和告警。服务端目前没有任何客户端打点接入，等待上报后才有数据。
        - 移动端是否走同一个 `controller` 拉取，还没确认。

        文档已同步：新增相关章节。注意：分支基于 `origin/master`，后续需要更新引用。

        """ + String(repeating: "- 长列表项包含 `inline_code` 和需要自动换行的中文说明，完整保留每一行。\n", count: 16)
            + "\n要提交吗？包括服务端分支、客户端新改动和其他分支。"
        let message = Muster_V1_AgentMessage.fixture {
            $0.id = "long"; $0.role = "assistant"; $0.kind = "text"; $0.text = markdown
        }
        let user = Muster_V1_AgentMessage.fixture {
            $0.id = "next"; $0.role = "user"; $0.kind = "text"
            $0.text = "客户端目前切 test 环境非常麻烦，提供一个方便的方案。"
        }
        for width in [CGFloat(320), 402] {
            let reference = UIHostingController(rootView: ChatMarkdown(text: markdown))
            let required = reference.sizeThatFits(in: CGSize(width: width - 32, height: .greatestFiniteMagnitude)).height
            var initial = message
            initial.text = "正在整理。"
            let bubble = UIHostingController(rootView: MessageBubble(message: initial))
            let initialHeight = bubble.sizeThatFits(in: CGSize(width: width - 32, height: 200)).height
            bubble.rootView = MessageBubble(message: message)
            RunLoop.main.run(until: Date().addingTimeInterval(0.1))
            let constrained = bubble.sizeThatFits(in: CGSize(width: width - 32, height: 200)).height
            XCTAssertGreaterThan(constrained, initialHeight)
            XCTAssertGreaterThanOrEqual(constrained, required,
                "A long message must reserve its complete height even in a short scroll viewport")
        }
        try snapshot(ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    MessageBubble(message: message)
                    MessageBubble(message: user).id(user.id)
                }.padding(16)
            }.onAppear { proxy.scrollTo(user.id, anchor: .bottom) }
        }, name: "long-markdown-followup")
    }

    @MainActor
    func testCacheTTLIsVisibleWithoutOpeningDetails() throws {
        let known = Muster_V1_AgentMessage.fixture {
            $0.id = "ttl"; $0.kind = "turn_summary"; $0.tsUnix = 1_790_848_800
            $0.usage = .fixture {
                $0.model = "fixture"; $0.tokensKnown = true; $0.inputTokens = 100
                $0.cacheExpiry = .fixture {
                    $0.label = "Estimated cache TTL: at least 30 min"
                    $0.fromUnix = 1_790_850_600
                    $0.detail = "Policy estimate; not live server cache state."
                }
            }
        }
        var unknown = known
        unknown.usage.cacheExpiry = .fixture {
            $0.label = "Cache TTL unknown"
            $0.detail = "No cache read/write evidence in the last request."
        }
        var legacy = known
        legacy.usage.clearCacheExpiry()
        for width in [CGFloat(320), 402] {
            let content = VStack(alignment: .leading, spacing: 12) {
                TurnFooter(message: known)
                TurnFooter(message: unknown)
                TurnFooter(message: legacy)
                TurnFooter(message: known, cacheDetails: true)
                Spacer()
            }.padding(16)
            let host = UIHostingController(rootView: content)
            XCTAssertLessThanOrEqual(host.sizeThatFits(in: CGSize(width: width, height: 874)).width, width)
            try snapshot(content, name: "cache-ttl-\(Int(width))", width: width)
        }
    }

    @MainActor
    func testTableAndStreamingReplyDoNotOverlapTurnFooter() throws {
        for width in [CGFloat(320), 402] {
            var bounds: [String: CGRect] = [:]
            var initialReplyHeight: CGFloat?
            let content = FooterLayoutFixture {
                bounds = $0
                if initialReplyHeight == nil, let height = $0["reply"]?.height, height > 0 {
                    initialReplyHeight = height
                }
            }
            try snapshot(content, name: "table-footer-\(Int(width))", width: width)
            let reply = try XCTUnwrap(bounds["reply"])
            let footer = try XCTUnwrap(bounds["footer"])
            XCTAssertGreaterThan(reply.height, try XCTUnwrap(initialReplyHeight),
                "The same row must grow when the table and prose arrive")
            XCTAssertGreaterThanOrEqual(footer.minY, reply.maxY + 10,
                "Usage and cost must start after the whole reply, including its last wrapped paragraph")
            XCTAssertLessThanOrEqual(reply.width, width)
        }
    }

    @MainActor
    func testWorkbenchCodeAndConfirmationFitPhoneWidths() throws {
        let preview = Muster_App_CodePreview.fixture {
            $0.id = "fixture-command"; $0.deviceName = "Mac-C"
            $0.cwd = "/Volumes/Workspace/Projects/Muster"
            $0.language = "sh"; $0.code = "printf 'MUSTER_WORKBENCH\\n'; pwd; exit 7"
        }
        let part = Muster_App_MessagePart.fixture {
            $0.kind = "code"; $0.language = preview.language; $0.text = preview.code; $0.commandID = preview.id
        }
        var chat = Muster_App_ChatView()
        chat.providerLabel = "Codex"; chat.session.deviceName = "Mac-C"
        chat.session.cwd = preview.cwd; chat.terminal.available = true
        for width in [CGFloat(320), 402] {
            for scheme in [ColorScheme.light, .dark] {
                let content = NavigationStack {
                    VStack(alignment: .leading, spacing: 16) {
                        ConversationNavigation(chat: chat, selected: .constant("Conversation"))
                        ScrollView {
                            ConversationText(text: "", parts: [
                                .fixture { $0.kind = "markdown"; $0.text = "在当前工作目录执行命令，完成后把退出码和输出返回这条会话。" }, part
                            ]).padding(.horizontal, 16)
                        }
                    }.navigationTitle("检查工作目录").navigationBarTitleDisplayMode(.inline)
                }.environment(\.colorScheme, scheme)
                let host = UIHostingController(rootView: content)
                XCTAssertLessThanOrEqual(host.sizeThatFits(in: CGSize(width: width, height: 874)).width, width)
                try snapshot(content, name: "workbench-\(Int(width))-\(scheme == .dark ? "dark" : "light")", width: width)
            }
            let confirmation = CodeExecutionPreview(preview: preview, cancel: {}, run: {})
            let host = UIHostingController(rootView: confirmation)
            XCTAssertLessThanOrEqual(host.sizeThatFits(in: CGSize(width: width, height: 874)).width, width)
            try snapshot(confirmation, name: "command-confirmation-\(Int(width))", width: width)
        }
    }

    @MainActor
    private func snapshot<V: View>(_ view: V, name: String, width: CGFloat = 402) throws {
        let controller = UIHostingController(rootView: view)
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: width, height: 874)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        controller.view.layoutIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        var rendered = false
        let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            rendered = window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        XCTAssertTrue(rendered, "Snapshot must contain the rendered native window")
        let attachment = XCTAttachment(image: image)
        attachment.name = name; attachment.lifetime = .keepAlways
        add(attachment)
        try XCTUnwrap(image.pngData()).write(to: FileManager.default.temporaryDirectory.appendingPathComponent("muster-\(name).png"))
        window.isHidden = true
    }
}

private struct FooterBoundsKey: PreferenceKey {
    static var defaultValue: [String: CGRect] = [:]
    static func reduce(value: inout [String: CGRect], nextValue: () -> [String: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, latest in latest })
    }
}

private struct FooterLayoutFixture: View {
    var onBounds: ([String: CGRect]) -> Void
    @State private var streamed = false
    private var reply: Muster_App_ChatBlock {
        .fixture {
            $0.id = "reply"; $0.kind = "message"
            $0.message = .fixture {
                $0.id = "reply"; $0.role = "assistant"; $0.kind = "text"
                $0.text = streamed ? """
                | Seed | AUROC | Status |
                | --- | --- | --- |
                | 42 | 0.9829 | Complete |
                | 43 | 0.9803 | Complete |
                | 44 | 0.9818 | Training |

                对比 v19 第九轮风险头（dev AUROC 0.9829 / 0.9803 / 0.9818），本轮前两头略高。dev 切片含新增 Aegis 行，不能直接等同，但至少说明新路径没有让表示学混乱。

                预计 seed 44 约 18:02 训完，校准、评测、复现后继续整理结果。最后一行正文必须完整显示在统计栏上方。
                """ : "正在整理评测结果。"
            }
        }
    }
    private var footer: Muster_App_ChatBlock {
        .fixture {
            $0.id = "footer"; $0.kind = "summary"
            $0.message = .fixture {
                $0.id = "footer"; $0.kind = "turn_summary"; $0.tsUnix = 1790845200
                $0.usage = .fixture {
                    $0.model = "claude-test"; $0.tokensKnown = true; $0.inputTokens = 120000
                    $0.outputTokens = 1444; $0.cacheHitKnown = true; $0.cacheHitPercent = 99.6
                    $0.costKnown = true; $0.costUsd = 0.35210
                }
            }
        }
    }
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    ConversationBlock(block: reply).background(measure("reply"))
                    ConversationBlock(block: footer).background(measure("footer")).id("footer")
                }.padding(16)
            }
            .coordinateSpace(name: "footer-test")
            .onPreferenceChange(FooterBoundsKey.self, perform: onBounds)
            .onAppear { DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { streamed = true } }
            .onChange(of: streamed) { _, _ in proxy.scrollTo("footer", anchor: .bottom) }
        }
    }
    private func measure(_ name: String) -> some View {
        GeometryReader { geometry in
            Color.clear.preference(key: FooterBoundsKey.self,
                value: [name: geometry.frame(in: .named("footer-test"))])
        }
    }
}

private extension SwiftProtobuf.Message {
    static func fixture(_ build: (inout Self) -> Void) -> Self {
        var message = Self()
        build(&message)
        return message
    }
}
