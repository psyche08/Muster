#if os(macOS)
import AppKit
import SwiftUI
import XCTest
import MusterCore
@testable import MusterUI

final class AgentRenderingTests: XCTestCase {
    @MainActor
    func testRenderWorkingAndWrappedToolContentAtPhoneWidth() throws {
        let code = "{\n  \"command\": \"cargo test --offline -p muster-agent -p muster-platform --lib\",\n  \"message\": \"中文和 emoji 🛠️ 同样换行，保留原始内容和行号\",\n  \"enabled\": true,\n  \"timeout_ms\": 10000\n}\n\n# A comment\nexport TARGET=\"mac-c\"\n@@ -1 +1 @@\n-old_value\n+new_value\n" + String(repeating: "long_unbroken_path/", count: 12)
        // The core always prepares activity rows (muster-app-core timeline).
        let row = Muster_App_ChatActivityRow.with {
            $0.id = "tool"; $0.title = "Ran cargo test"; $0.category = "shell"; $0.toolName = "exec_command"
            $0.detail = code; $0.output = code; $0.isResult = true
        }
        let activity = Muster_App_ChatBlock.with {
            $0.id = "working"; $0.kind = "activity"; $0.title = "Ran 1 command"; $0.activityRows = [row]
        }
        for scheme in [ColorScheme.light, .dark] {
            let content = VStack(alignment: .leading, spacing: 16) {
                ActivityGroup(block: activity, expanded: true)
                Divider()
                Text("展开后的工具内容").font(.headline)
                ToolActivityRow(row: row, expanded: true)
            }.padding(16).frame(width: 390)
                .background(scheme == .dark ? Color.black : Color.white)
                .environment(\.colorScheme, scheme)
            let host = NSHostingView(rootView: content)
            host.frame = NSRect(origin: .zero, size: host.fittingSize)
            XCTAssertEqual(host.frame.width, 390, accuracy: 1)
            let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false; window.contentView = host
            host.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.2))
            let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
                .write(to: URL(fileURLWithPath: "/private/tmp/muster-tool-\(scheme == .dark ? "dark" : "light").png"))
            window.close()
        }
    }

    @MainActor
    func testRenderDesktopSetup() throws {
        let setup = Muster_App_DesktopSetupView.with {
            $0.visible = true; $0.canInstall = true
            $0.pluginMessage = "需要授权系统 helper 安装或验证 Locked Use 插件。在系统设置 → 通用 → 登录项与扩展中允许 Muster 后台运行，随后将自动安装，最多等待约一分钟。"
            $0.itermMessage = "需要允许 Muster / musterd 自动化控制 iTerm2，并在 iTerm2 Settings → General → Magic 中启用 Python API。首次连接时还需允许 Muster 访问。"
        }
        var refreshCount = 0
        let host = NSHostingView(rootView: DesktopSetupView(setup: setup, accessibility: {}, install: {}, iterm: {}, later: {}, refresh: { refreshCount += 1 }).environment(\.colorScheme, .light).tint(.blue))
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = host
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        NotificationCenter.default.post(name: NSApplication.didBecomeActiveNotification, object: NSApp)
        XCTAssertEqual(refreshCount, 1)
        let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: bitmap)
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            .write(to: URL(fileURLWithPath: "/private/tmp/muster-desktop-setup-preview.png"))
        window.close()
    }

    @MainActor
    func testRenderConversationStylesAndDiff() throws {
        let user = Muster_V1_AgentMessage.with { $0.role = "user"; $0.kind = "text"; $0.text = "把移动设备从 Agent 标签中隐藏，保留在网络设置。" }
        let result = Muster_V1_AgentMessage.with { $0.role = "assistant"; $0.kind = "text"; $0.text = "已完成。设备筛选由 **Shared Rust** 统一处理，网络设置仍显示所有设备。\n\n- 主界面只显示桌面与服务器\n- 移动端隐藏 Agent / Terminal 入口\n\n验证通过，点击下方文件可查看修改。" }
        let activity = Muster_App_ChatBlock.with {
            $0.id = "activity"; $0.completed = true
            $0.activity = (0..<8).map { index in Muster_V1_AgentMessage.with { $0.id = "tool-\(index)"; $0.kind = "tool_result"; $0.toolName = "exec_command"; $0.text = "DETAILS_MUST_START_COLLAPSED" } }
        }
        let diff = "@@ -2,3 +2,4 @@\n fn service_devices() {\n-    all_members()\n+    all_members().filter(|m| {\n+        m.kind == Desktop\n+    })\n }"
        let summary = Muster_V1_AgentMessage.with {
            $0.id = "summary"; $0.tsUnix = 1790572994
            $0.usage = Muster_V1_TurnUsage.with {
                $0.model = "gpt-6-astra"; $0.tokensKnown = true; $0.inputTokens = 34658; $0.outputTokens = 8
                $0.cacheHitPercent = 99.3; $0.costKnown = true; $0.costUsd = 0.037092; $0.costSource = "estimated"
                $0.changes = [Muster_V1_TurnFileChange.with {
                    $0.path = "src/state.rs"; $0.kind = "update"; $0.additions = 3; $0.deletions = 1; $0.countsKnown = true; $0.diff = diff
                }, Muster_V1_TurnFileChange.with {
                    $0.path = "Sources/MusterUI/AgentScreens.swift"; $0.kind = "update"; $0.additions = 2; $0.deletions = 2; $0.countsKnown = true
                }]
            }
        }
        for style in ConversationStyle.allCases {
            for scheme in [ColorScheme.light, .dark] {
                let palette = ChatPalette(scheme: scheme, style: style)
                let content = HStack(alignment: .top, spacing: 20) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Mac 和 iOS 开发").font(.title2.bold())
                        MessageBubble(message: user)
                        ActivityGroup(block: activity)
                        MessageBubble(message: result)
                        TurnChanges(message: summary)
                        TurnFooter(message: summary)
                    }.padding(16).frame(width: 402)
                    VStack(alignment: .leading, spacing: 12) {
                        Text("src/state.rs").font(.title2.bold())
                        Text("Recorded turn changes").font(.caption).foregroundStyle(.secondary)
                        SourcePreview(text: diff, diff: true).frame(width: 370, height: 340)
                        Text("Current file →").font(.subheadline).foregroundStyle(.blue)
                    }.padding(16).frame(width: 402)
                }.background(palette.canvas).foregroundStyle(palette.text)
                    .environment(\.colorScheme, scheme).environment(\.conversationStyle, style)
                let host = NSHostingView(rootView: content)
                host.frame = NSRect(origin: .zero, size: host.fittingSize)
                let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
                window.isReleasedWhenClosed = false; window.contentView = host
                host.layoutSubtreeIfNeeded()
                RunLoop.main.run(until: Date().addingTimeInterval(0.2))
                let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
                host.cacheDisplay(in: host.bounds, to: bitmap)
                try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
                    .write(to: URL(fileURLWithPath: "/private/tmp/muster-\(style.rawValue)-\(scheme == .dark ? "dark" : "light")-preview.png"))
                window.close()
            }
        }
    }

    @MainActor
    func testSingleQueuedMessageKeepsAVisibleViewportInFixedSizeComposer() throws {
        let message = Muster_V1_PendingInteraction.with {
            $0.id = "queued:one"; $0.kind = "queued_message"
            $0.prompt = "这条排队消息应该始终可见"; $0.optional = true
        }
        let content = QueuedMessages(messages: [message], submitting: false, error: "", outcomeUnknown: false, sendNow: { _, _ in })
            .frame(width: 390).fixedSize(horizontal: false, vertical: true)
        let host = NSHostingView(rootView: content)
        XCTAssertGreaterThan(host.fittingSize.height, 110)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        host.layoutSubtreeIfNeeded()
        let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: bitmap)
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            .write(to: URL(fileURLWithPath: "/private/tmp/muster-queue-single-preview.png"))
    }

    @MainActor
    func testQueuedMessagesAtPhoneWidth() throws {
        let messages = ["请检查 Mac-B 与 Mac-C 的发送日志，修复问题后运行完整验证。", "随后整理修改说明，并提交代码。"].enumerated().map { index, text in
            Muster_V1_PendingInteraction.with {
                $0.id = "queued:\(index)"; $0.kind = "queued_message"; $0.prompt = text
                $0.options = ["Send now"]; $0.optional = true
            }
        }
        let content = QueuedMessages(messages: messages, submitting: false, error: "", outcomeUnknown: false, sendNow: { _, _ in })
            .padding(12).frame(width: 390).environment(\.colorScheme, .light)
        let host = NSHostingView(rootView: content)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = host
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: bitmap)
        XCTAssertGreaterThan(host.bounds.height, 100)
        XCTAssertLessThan(host.bounds.height, 330)
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            .write(to: URL(fileURLWithPath: "/private/tmp/muster-queue-phone-preview.png"))
        window.close()
    }

    @MainActor
    func testRenderQuestionCardsAtPhoneWidth() throws {
        let interaction = Muster_V1_PendingInteraction.with {
            $0.id = "question:render"; $0.kind = "question"
            $0.questions = [Muster_V1_InteractionQuestion.with {
                $0.id = "key"; $0.header = "密钥方向"; $0.prompt = "新密钥机制中，应该向沙箱提供哪种密钥？"; $0.allowText = true
                $0.options = [Muster_V1_QuestionOption.with {
                    $0.id = "0"; $0.label = "公钥给沙箱（推荐）"; $0.description_p = "私钥保留在受保护的服务端，沙箱仅验证签名。"
                }, Muster_V1_QuestionOption.with { $0.id = "1"; $0.label = "其他配置" }]
            }, Muster_V1_InteractionQuestion.with {
                $0.id = "clients"; $0.header = "客户端"; $0.prompt = "需要支持哪些设备？"; $0.multiSelect = true; $0.allowText = true
                $0.options = ["Mac", "iPhone"].map { value in Muster_V1_QuestionOption.with { $0.id = value; $0.label = value } }
            }]
        }
        var optional = interaction
        optional.optional = true
        let folded = NSHostingView(rootView: InteractionCard(interaction: optional, submitting: false, error: "", outcomeUnknown: false, respond: { _, _ in }).frame(width: 390))
        XCTAssertLessThan(folded.fittingSize.height, 100, "Optional questions must start collapsed")
        for style in ConversationStyle.allCases {
            let content = InteractionCard(interaction: interaction, submitting: false, error: "提交失败时保留所选答案，可检查后重试。", outcomeUnknown: false, respond: { _, _ in })
                .padding(12).frame(width: 390)
                .environment(\.conversationStyle, style).environment(\.colorScheme, .light)
            let host = NSHostingView(rootView: content)
            host.frame = NSRect(origin: .zero, size: host.fittingSize)
            let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false; window.contentView = host
            host.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.2))
            let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            XCTAssertGreaterThan(bitmap.pixelsHigh, 400)
            try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
                .write(to: URL(fileURLWithPath: "/private/tmp/muster-question-\(style.rawValue)-preview.png"))
            window.close()
        }
    }

    @MainActor
    func testRenderPhoneWidthCatalogAndMarkdownTurn() throws {
        var vm = ViewModel()
        vm.agents.deviceID = "*"
        vm.network.members = ["Mac-B", "Mac-C", "Mac-A"].map { name in
            Muster_V1_MemberView.with { $0.deviceID = name; $0.name = name; $0.kind = "desktop"; $0.connected = true }
        }
        vm.serviceDevices = vm.network.members
        vm.network.members.append(Muster_V1_MemberView.with { $0.deviceID = "phone"; $0.name = "iPhone"; $0.kind = "phone" })
        let session = Muster_V1_AgentSession.with {
            $0.id = "session"; $0.provider = "codex"; $0.title = "Mac and iOS development"
            $0.deviceName = "Mac-B"; $0.cwd = "/Projects/Muster"; $0.state = "idle"
        }
        let markdown = Muster_V1_AgentMessage.with {
            $0.id = "reply"; $0.role = "assistant"; $0.kind = "text"
            $0.text = """
            ## Live chat is ready
            **Markdown** supports lists, links and `inline code`.
            - Device tabs above the agent list
            - Collapsed tools by default

            ```rust
            let ready = true;
            ```

            | Check | Result |
            | --- | --- |
            | Reconnect | Passed |
            | Messages | In order |
            """
        }
        let footer = Muster_V1_AgentMessage.with {
            $0.id = "summary"; $0.kind = "turn_summary"; $0.tsUnix = 1790572991
            $0.usage = Muster_V1_TurnUsage.with {
                $0.model = "gpt-6-astra"; $0.tokensKnown = true
                $0.inputTokens = 34658; $0.outputTokens = 8; $0.cacheHitPercent = 99.35
                $0.costKnown = true; $0.costSource = "estimated"; $0.costUsd = 0.037092
                $0.pricingDate = "2026-09-28"
            }
        }
        let content = VStack(alignment: .leading, spacing: 16) {
            Text("Agents").font(.largeTitle.bold())
            DeviceTabs(devices: vm.serviceDevices, selected: "*", select: { _ in }, details: { _ in })
            SessionRow(session: session)
            Divider()
            MessageBubble(message: markdown)
            TurnFooter(message: footer)
        }.padding(16).frame(width: 402).background(Color.white).environment(\.colorScheme, .light)
        let host = NSHostingView(rootView: content)
        let size = host.fittingSize
        host.frame = NSRect(origin: .zero, size: size)
        let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: bitmap)
        XCTAssertGreaterThan(bitmap.pixelsHigh, 500)
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            .write(to: URL(fileURLWithPath: "/private/tmp/muster-agent-interface-preview.png"))
        window.close()
    }
}
#endif

#if os(macOS)
extension AgentRenderingTests {
    @MainActor
    func testOrcaPresentationAtPhoneAndDesktopWidths() throws {
        let reasoning = Muster_App_ChatBlock.with {
            $0.id = "reason"; $0.kind = "reasoning"; $0.title = "Reasoning"
            $0.message = .with { $0.text = "检查失败的测试，并核对变更范围。" }
        }
        let reply = Muster_V1_AgentMessage.with {
            $0.id = "answer"; $0.role = "assistant"
            $0.text = "已修复会话展示。**正文保持可见**，工具详情按需展开。\n\n- 保留流式更新\n- 显示准确的文件差异"
        }
        let activity = Muster_App_ChatBlock.with {
            $0.id = "tools"; $0.kind = "activity"; $0.title = "1 command · 2 file reads"
            $0.activityRows = [.with {
                $0.id = "call"; $0.title = "shell · cargo test -p muster-agent"; $0.toolName = "shell"
                $0.category = "shell"; $0.detail = "cargo test -p muster-agent"; $0.status = "failed"
                $0.output = "1 test failed: stale prompt revision"; $0.isResult = true
            }]
        }
        let diff = Muster_App_ChatDiffFile.with {
            $0.id = "file"; $0.path = "src/session/render.rs"; $0.added = 2; $0.removed = 1
            $0.patch = "@@ -41 +41,2 @@\n-old\n+new\n+你好 🛠️"
            $0.lines = [
                .with { $0.kind = "meta"; $0.text = "@@ -41 +41,2 @@" },
                .with { $0.kind = "delete"; $0.oldLine = 41; $0.text = "let collapsed = true;" },
                .with { $0.kind = "add"; $0.newLine = 41; $0.text = "let collapsed = false;" },
                .with { $0.kind = "add"; $0.newLine = 42; $0.text = "// 你好 🛠️" }
            ]
        }
        for width: CGFloat in [390, 760] {
            for scheme in [ColorScheme.light, .dark] {
                let content = VStack(alignment: .leading, spacing: 16) {
                    MessageBubble(message: .with { $0.role = "user"; $0.text = "优化会话展示" })
                    ReasoningRow(block: reasoning)
                    ActivityGroup(block: activity, expanded: true)
                    SessionDiffCard(file: diff, expanded: true)
                    MessageBubble(message: reply)
                }.padding(20).frame(width: width)
                    .background(scheme == .dark ? Color(white: 0.13) : .white)
                    .environment(\.colorScheme, scheme)
                let host = NSHostingView(rootView: content)
                host.frame = NSRect(origin: .zero, size: host.fittingSize)
                XCTAssertEqual(host.frame.width, width, accuracy: 1)
                XCTAssertGreaterThan(host.frame.height, 200)
                let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
                window.isReleasedWhenClosed = false; window.contentView = host
                host.layoutSubtreeIfNeeded()
                RunLoop.main.run(until: Date().addingTimeInterval(0.2))
                let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
                host.cacheDisplay(in: host.bounds, to: bitmap)
                try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
                    .write(to: URL(fileURLWithPath: "/private/tmp/muster-orca-render-\(Int(width))-\(scheme == .dark ? "dark" : "light").png"))
                window.close()
            }
        }
    }
}
#endif
