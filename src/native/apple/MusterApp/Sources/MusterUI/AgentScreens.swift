// Agent sessions and the chat with one of them. Options shown for an
// interaction are exactly what the provider offered (SECURITY I7).
#if canImport(SwiftUI)
import MusterCore
import SwiftUI
import MarkdownUI

struct AgentsView: View {
    let core: MusterCore
    let agents: Muster_App_AgentsView
    let devices: [Muster_V1_MemberView]
    let shown: String
    @State private var creating = false

    init(core: MusterCore, vm: ViewModel) {
        self.core = core
        agents = vm.agents
        devices = vm.serviceDevices
        shown = vm.screen == .chat ? "\(vm.chat.deviceID):\(vm.chat.session.id)" : ""
    }

    var body: some View {
        // Row actions/highlights must not capture this view's entire catalog.
        let core = core, shown = shown
        List {
            if !agents.createHosts.isEmpty {
                Button { creating = true } label: {
                    Label(agents.creating ? "Creating CLI session…" : "New CLI session", systemImage: "plus.bubble")
                }.disabled(agents.creating)
            }
            if agents.loading && agents.sessions.isEmpty {
                ProgressView()
            } else if agents.sessions.isEmpty {
                ContentUnavailableView("No sessions", systemImage: "bubble.left.and.text.bubble.right", description: Text("Codex and Claude sessions on this device show up here."))
            }
            ForEach(agents.errors, id: \.self) { error in
                Label(error, systemImage: "wifi.exclamationmark").font(.caption).foregroundStyle(.secondary)
            }
            ForEach(agents.sessions, id: \.sourceKey) { session in
                Button { core.openAgent(session.id, device: session.deviceID) } label: { SessionRow(session: session) }.buttonStyle(.plain)
                    .listRowBackground(shown == session.sourceKey ? Color.accentColor.opacity(0.15) : nil)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                DeviceTabs(devices: devices, selected: agents.deviceID, select: { core.openAgents(device: $0) }, details: { core.selectDevice($0) })
            }
            .background(.bar)
        }
        .navigationTitle("")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .refreshable { core.openAgents(device: agents.deviceID) }
        .sheet(isPresented: $creating) {
            OrcaCreateSheet(core: core, hosts: agents.createHosts, agents: agents.createAgents)
        }
    }

}

private struct OrcaCreateSheet: View {
    let core: MusterCore
    let hosts: [Muster_V1_MemberView]
    let agents: [String]
    @Environment(\.dismiss) private var dismiss
    @State private var device = ""
    @State private var agent = ""
    @State private var directory = ""
    var body: some View {
        NavigationStack {
            Form {
                Picker("Device", selection: $device) {
                    ForEach(hosts, id: \.deviceID) { Text($0.name).tag($0.deviceID) }
                }
                Picker("CLI agent", selection: $agent) {
                    ForEach(agents, id: \.self) { Text($0.capitalized).tag($0) }
                }
                TextField("Absolute directory on this device", text: $directory)
                    .autocorrectionDisabled()
                Text("Uses the existing directory. The selected CLI agent must be installed and signed in on that device. Session management is built into Muster.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .navigationTitle("New CLI session")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        core.createOrcaSession(device: device, directory: directory, agent: agent)
                        dismiss()
                    }
                }
            }
            .onAppear { device = hosts.first?.deviceID ?? ""; agent = agents.first ?? "" }
        }.frame(minWidth: 350, minHeight: 270)
    }
}

extension Muster_V1_AgentSession {
    var sourceKey: String { "\(deviceID):\(id)" }
}

struct DeviceTabs: View {
    let devices: [Muster_V1_MemberView]
    let selected: String
    let select: (String) -> Void
    let details: (String) -> Void
    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                tab("All devices", id: "*", symbol: "square.grid.2x2", connected: true)
                ForEach(devices, id: \.deviceID) { member in
                    tab(member.name, id: member.deviceID,
                        symbol: member.kind == "phone" ? "iphone" : "desktopcomputer",
                        connected: member.connected || member.isSelf)
                    .contextMenu {
                        Button("Device details") { details(member.deviceID) }
                    }
                }
            }.padding(.horizontal).padding(.vertical, 10)
        }
        .frame(height: 58)
        .background(.bar)
    }
    private func tab(_ name: String, id: String, symbol: String, connected: Bool) -> some View {
        Button { select(id) } label: {
            HStack(spacing: 6) {
                Image(systemName: symbol)
                Text(name).lineLimit(1)
                if id != "*" { Circle().fill(connected ? Color.green : Color.secondary).frame(width: 5, height: 5) }
            }
            .font(.subheadline.weight(.medium))
            .padding(.horizontal, 12).padding(.vertical, 8)
            .foregroundStyle(selected == id ? Color.white : Color.primary)
            .background(selected == id ? Color.accentColor : Color.secondary.opacity(0.1), in: Capsule())
        }.buttonStyle(.plain)
        .accessibilityAddTraits(selected == id ? .isSelected : [])
    }
}

// Rendering must not introduce native DNS/network traffic outside Shared Rust.
// Remote images remain explicit links; ordinary Markdown links open on user tap.
struct ChatImageProvider: ImageProvider {
    func makeImage(url: URL?) -> some View {
        if let url, ["https", "http"].contains(url.scheme ?? "") {
            Link(destination: url) { Label("Image", systemImage: "photo") }
        } else { Label("Image", systemImage: "photo") }
    }
}
struct ChatInlineImageProvider: InlineImageProvider {
    func image(with url: URL, label: String) async throws -> Image { Image(systemName: "photo") }
}

struct TurnFooter: View {
    let message: Muster_V1_AgentMessage
    private var usage: Muster_V1_TurnUsage { message.usage }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Divider()
            HStack {
                if message.tsUnix > 0 {
                    Text(Date(timeIntervalSince1970: TimeInterval(message.tsUnix)), format: .dateTime.month().day().hour().minute().second())
                } else { Text("End time unavailable") }
                Spacer()
                Text(usage.model).lineLimit(1)
            }
            Text(tokens).lineLimit(1).minimumScaleFactor(0.8)
            if usage.costKnown {
                Text("\(usage.costSource == "recorded" ? "Recorded" : "Estimated") $\(usage.costUsd, specifier: "%.5f") USD")
                    .help("API-equivalent cost; not subscription billing. Pricing snapshot: \(usage.pricingDate)")
            } else { Text("Cost unavailable") }
        }
        .font(.caption).monospacedDigit().foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
        .padding(.vertical, 8)
    }

    /// Input, output and cache hit for the whole turn.
    private var tokens: String {
        let counts = "input \(usage.inputTokens.formatted()) · Output \(usage.outputTokens.formatted())"
        if usage.tokensKnown {
            return "Turn \(counts) · Cache hit " + (usage.cacheHitKnown
                ? String(format: "%.1f%%", usage.cacheHitPercent) : "unavailable")
        }
        if usage.requestCount > 0 { return "Partial turn \(counts) · Cache hit unavailable" }
        return "Token usage unavailable"
    }
}

struct SessionRow: View {
    let session: Muster_V1_AgentSession

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(session.provider.capitalized).font(.caption).padding(.horizontal, 6).padding(.vertical, 2)
                    .background(session.provider == "codex" ? Color.blue.opacity(0.15) : Color.orange.opacity(0.15), in: Capsule())
                Text(session.title.isEmpty ? session.id : session.title).font(.headline).lineLimit(1)
                Spacer()
                VStack(alignment: .trailing, spacing: 2) {
                    StateBadge(state: session.state)
                    if session.processState == "running" && session.state == "unknown" {
                        Text("process alive").font(.caption2).foregroundStyle(.secondary)
                    }
                }
            }
            HStack {
                if !session.deviceName.isEmpty { Text(session.deviceName).font(.footnote.weight(.medium)) }
                Text(session.cwd).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
                Spacer()
                Text(relative(session.updatedUnix)).font(.footnote).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
    }
}

struct StateBadge: View {
    let state: String
    var body: some View {
        switch state {
        case "running": Label("working", systemImage: "circle.fill").font(.caption).foregroundStyle(.green)
        case "waiting_approval": Label("needs you", systemImage: "hand.raised.fill").font(.caption).foregroundStyle(.orange)
        case "idle": Text("idle").font(.caption).foregroundStyle(.secondary)
        case "ended": Text("ended").font(.caption).foregroundStyle(.secondary)
        default: Text("unknown").font(.caption).foregroundStyle(.secondary)
        }
    }
}

func relative(_ unix: UInt64) -> String {
    guard unix > 0 else { return "" }
    let date = Date(timeIntervalSince1970: TimeInterval(unix))
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .abbreviated
    return formatter.localizedString(for: date, relativeTo: Date())
}

struct ChatView: View {
    #if os(iOS)
    @StateObject private var keyboardDock = KeyboardDock()
    @State private var latestRequest = 0
    @State private var earlierRequest = 0
    #endif
    @Environment(\.workspaceLayout) private var layout
    let core: MusterCore
    let terminal: Muster_App_TerminalView
    let chat: Muster_App_ChatView
    @State private var showChatOptions = false
    @State private var contentTab = "Conversation"
    @State private var activityTab = "Tracing"
    @State private var jumpToStart = false
    @State private var atLatest = true
    @State private var draggingTranscript = false
    @State private var viewportHeight: CGFloat = 0
    @State private var bottomPosition: CGFloat = 0
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    @ScaledMetric(relativeTo: .body) private var bodySize = ChatStyle.bodySize
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }

    init(core: MusterCore, vm: ViewModel) {
        self.core = core
        chat = vm.chat
        terminal = vm.terminal
    }

    var body: some View {
        let core = core
        VStack(spacing: 0) {
            if !chat.session.parentSessionID.isEmpty {
                Button { core.openAgent(chat.session.parentSessionID, device: chat.deviceID) } label: {
                    Label("Parent session", systemImage: "arrow.turn.up.left")
                }.padding(8)
            }
            #if os(iOS)
            ConversationNavigation(chat: chat, selected: $contentTab)
            Divider().opacity(0.5)
            #else
            Picker("Session view", selection: $contentTab) {
                Text("Conversation").tag("Conversation")
                Text("Tracing").tag("Tracing")
                Text("Subagents").tag("Subagents")
                Text("Sessions").tag("Sessions")
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, 12).padding(.vertical, 8)
            .accessibilityIdentifier("session-content-tabs")
            #endif
            if contentTab == "Conversation" {
            Group {
            #if os(iOS)
            RecycledConversation(chat: chat,
                header: AnyView(conversationHeader(core: core)),
                interactions: AnyView(VStack(alignment: .leading, spacing: 14) { interactionCards(core: core) }),
                showsInteractions: !layout.flex,
                blockContent: { block in AnyView(ConversationBlock(block: block) { message, path in
                    core.openChange(summary: message, path: path)
                }) }, jumpToLatest: latestRequest, holdPosition: earlierRequest, atLatest: $atLatest)
                .overlay(alignment: .bottom) {
                    if !atLatest {
                        Button { latestRequest += 1 } label: {
                            Image(systemName: "arrow.down").font(.system(size: 19, weight: .medium))
                                .frame(width: 38, height: 38)
                                .background(.regularMaterial, in: Circle())
                                .overlay(Circle().stroke(palette.border, lineWidth: 0.5))
                        }
                        .buttonStyle(.plain).accessibilityLabel("Jump to latest")
                        .padding(.bottom, 8)
                    }
                }
            #else
            ScrollViewReader { proxy in
                ScrollView {
                    // Markdown block margins settle after the first layout pass.
                    // Measure the loaded transcript together so a growing reply
                    // cannot leave a cached lazy-row height under the next bubble.
                    VStack(alignment: .leading, spacing: style == .claude ? 20 : 14) {
                        Color.clear.frame(height: 1).id("conversation-start")
                        if chat.loading && chat.messageCount == 0 { ProgressView("Loading conversation…") }
                        if !chat.loadError.isEmpty {
                            Label(chat.loadError, systemImage: "wifi.exclamationmark").font(.callout).foregroundStyle(.secondary)
                        }
                        if !chat.loading && chat.messageCount == 0 && chat.loadError.isEmpty {
                            Text(chat.session.transcriptNotice.isEmpty ? "No messages yet" : chat.session.transcriptNotice)
                                .foregroundStyle(.secondary)
                        }
                        if chat.hasEarlier_p {
                            Button("Load earlier") { atLatest = false; core.agentLoadEarlier() }.frame(maxWidth: .infinity)
                        }
                        ForEach(chat.timeline, id: \.id) { block in
                            ConversationBlock(block: block) { message, path in
                                core.openChange(summary: message, path: path)
                            }.id(block.id)
                        }
                        // Top and bottom: the cards sit in the lower pane, with the composer.
                        if !layout.flex { interactionCards(core: core) }
                        Color.clear.frame(height: 1)
                            .id("conversation-bottom")
                            .background(GeometryReader { geometry in
                                Color.clear.preference(key: ConversationBottomKey.self,
                                    value: geometry.frame(in: .named("conversation-scroll")).maxY)
                            })
                    }
                    .frame(maxWidth: style.columnWidth)
                    .padding(.horizontal, ChatStyle.inset)
                    .padding(.vertical, 12)
                    .frame(maxWidth: .infinity)
                }
                // Initial async history must anchor after layout, not only in
                // onAppear before the transcript has arrived.
                .defaultScrollAnchor(.bottom)
                .coordinateSpace(name: "conversation-scroll")
                .background(GeometryReader { geometry in
                    Color.clear.onAppear { updateViewport(geometry.size.height) }
                        .onChange(of: geometry.size.height) { _, height in updateViewport(height) }
                })
                .onPreferenceChange(ConversationBottomKey.self) { bottom in
                    bottomPosition = bottom
                    #if os(iOS)
                    if draggingTranscript {
                        atLatest = bottom <= viewportHeight + 48
                    } else if atLatest && !jumpToStart {
                        // Content can grow under the same message ID. Keep
                        // following after layout, not only when the count changes.
                        proxy.scrollTo("conversation-bottom", anchor: .bottom)
                    } else if bottom <= viewportHeight + 48 {
                        atLatest = true
                    }
                    #else
                    atLatest = bottom <= viewportHeight + 48
                    #endif
                }
                #if os(iOS)
                .simultaneousGesture(DragGesture().onChanged { _ in
                    draggingTranscript = true
                    atLatest = false
                }.onEnded { _ in
                    draggingTranscript = false
                    atLatest = bottomPosition <= viewportHeight + 48
                })
                #endif
                .scrollDismissesKeyboard(.interactively)
                .onAppear { proxy.scrollTo("conversation-bottom", anchor: .bottom) }
                .onChange(of: chat.loading) { _, loading in
                    if !loading && jumpToStart {
                        jumpToStart = false
                        withAnimation { proxy.scrollTo("conversation-start", anchor: .top) }
                    }
                }
                #if os(macOS)
                .overlay(alignment: .trailing) {
                    VStack(spacing: 12) {
                        Button {
                            atLatest = false
                            if chat.hasEarlier_p { jumpToStart = true; core.agentLoadEarlier(all: true) }
                            else { withAnimation { proxy.scrollTo("conversation-start", anchor: .top) } }
                        } label: { Image(systemName: "arrow.up.to.line").frame(width: 38, height: 38) }
                        .accessibilityLabel("Jump to first message")
                        .disabled(chat.loading)
                        Button {
                            jumpToStart = false; atLatest = true
                            withAnimation { proxy.scrollTo("conversation-bottom", anchor: .bottom) }
                        } label: { Image(systemName: "arrow.down.to.line").frame(width: 38, height: 38) }
                        .accessibilityLabel("Jump to latest")
                    }
                    .buttonStyle(.plain)
                    .background(.regularMaterial, in: Capsule())
                    .overlay(Capsule().strokeBorder(.secondary.opacity(0.2)))
                    .padding(.trailing, 8)
                }
                #endif
                .onChange(of: chat.contentRevision) { _, _ in
                    // Cached earlier pages arrive without a loading transition.
                    if jumpToStart && !chat.loading {
                        jumpToStart = false
                        withAnimation { proxy.scrollTo("conversation-start", anchor: .top) }
                    } else if atLatest && !jumpToStart {
                        proxy.scrollTo("conversation-bottom", anchor: .bottom)
                    }
                }
                #if os(iOS)
                .overlay(alignment: .bottom) {
                    if !atLatest {
                        Button {
                            jumpToStart = false; atLatest = true
                            withAnimation { proxy.scrollTo("conversation-bottom", anchor: .bottom) }
                        } label: {
                            Image(systemName: "arrow.down").font(.system(size: 19, weight: .medium))
                                .frame(width: 38, height: 38)
                                .background(.regularMaterial, in: Circle())
                                .overlay(Circle().stroke(palette.border, lineWidth: 0.5))
                        }
                        .buttonStyle(.plain).accessibilityLabel("Jump to latest")
                        .padding(.bottom, 8)
                    }
                }
                #endif
                .onChange(of: chat.interactions.count) { _, _ in
                    if atLatest && !jumpToStart && !layout.flex, let last = chat.interactions.last(where: { !$0.optional }) { withAnimation { proxy.scrollTo("interaction-\(last.id)", anchor: .bottom) } }
                }
            }
            #endif
            }
            .id("\(chat.deviceID):\(chat.session.id)")
            .frame(maxHeight: layout.flex ? layout.readingHeight : .infinity)
            if layout.flex { Color.clear.frame(height: layout.hingeHeight) }
            // Keep the composer at the same structural identity when the
            // keyboard changes available height. Reparenting it loses focus.
            lowerPane(core: core).fixedSize(horizontal: false, vertical: true)
            if layout.flex {
                ScrollView {
                    interactionCards(core: core)
                        .frame(maxWidth: style.columnWidth)
                        .padding(.horizontal, ChatStyle.inset)
                }
                .frame(maxHeight: chat.interactions.isEmpty ? 0 : 100)
                #if os(iOS)
                MusterKeyboard(target: keyboardDock.target.target(), compact: true, allowsHiding: false)
                    .frame(height: KeyboardMetrics.height(wide: true, compact: true, raw: false))
                    .disabled(chat.sending)
                    .accessibilityIdentifier("persistent-mac-keyboard")
                #endif
            }
            } else if contentTab == "Terminal" {
                CodeRunHistory(runs: chat.codeRuns, error: chat.codeError) { core.codeAction(.return, id: $0) }
                SessionTerminalPane(core: core, workspace: chat.terminal, terminal: terminal)
            } else if contentTab == "Activity" {
                Picker("Activity", selection: $activityTab) {
                    Text("Trace").tag("Tracing")
                    Text("Agents (\(chat.session.subagents.count))").tag("Subagents")
                    Text("Sessions").tag("Sessions")
                }.pickerStyle(.segmented).padding(16)
                if activityTab == "Tracing" { SessionTraceView(core: core, chat: chat) }
                else { RelatedSessionView(core: core, device: chat.deviceID,
                    sessions: activityTab == "Subagents" ? chat.session.subagents : chat.session.subsessions, title: activityTab) }
            } else if contentTab == "Tracing" {
                SessionTraceView(core: core, chat: chat)
            } else {
                RelatedSessionView(core: core, device: chat.deviceID,
                    sessions: contentTab == "Subagents" ? chat.session.subagents : chat.session.subsessions,
                    title: contentTab)
            }
        }
        #if os(iOS)
        .environment(\.keyboardDock, layout.flex ? keyboardDock : nil)
        #endif
        .environment(\.executeCode, { core.codeAction(.preview, id: $0) })
        .sheet(isPresented: Binding(get: { chat.hasCodePreview }, set: { if !$0 { core.codeAction(.dismiss) } })) {
            CodeExecutionPreview(preview: chat.codePreview, cancel: { core.codeAction(.dismiss) }, run: {
                core.codeAction(.run, id: chat.codePreview.id); contentTab = "Terminal"
            })
        }
        .font(.system(size: bodySize * style.bodyScale))
        .foregroundStyle(palette.text)
        .background(palette.canvas)
        .environment(\.openURL, OpenURLAction { url in
            if ["http", "https", "mailto"].contains(url.scheme ?? "") { return .systemAction }
            core.openFileLink(url.absoluteString)
            return .handled
        })
        .navigationTitle(chat.session.title.isEmpty ? "Chat" : chat.session.title)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .onChange(of: "\(chat.deviceID):\(chat.session.id)") { _, _ in jumpToStart = false; atLatest = true; contentTab = "Conversation" }
        .onChange(of: contentTab) { old, new in
            if new == "Terminal" { core.sessionTerminal(.start) }
            else if old == "Terminal" { core.sessionTerminal(.detach) }
        }
        .onDisappear { if contentTab == "Terminal" { core.sessionTerminal(.detach) } }
        .sheet(isPresented: $showChatOptions) {
            ChatOptionsSheet(core: core, chat: chat) { showChatOptions = false }
        }
        .toolbar {
            #if os(iOS)
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Text([chat.providerLabel, chat.session.deviceName].filter { !$0.isEmpty }.joined(separator: " · "))
                    if chat.canChooseModel {
                        Button { showChatOptions = true } label: { Label("Model and effort", systemImage: "cpu") }
                    }
                    subagentMenu
                    if chat.canBrowseFiles {
                        Button { core.openFiles() } label: { Label("Files", systemImage: "folder") }
                        Button { core.openGit() } label: { Label("Git", systemImage: "arrow.triangle.branch") }
                    }
                    if chat.canInterrupt {
                        Button { core.agentInterrupt() } label: { Label("Stop", systemImage: "stop.circle") }
                    }
                } label: { Image(systemName: "ellipsis").accessibilityLabel("Conversation actions") }
            }
            #else
            ToolbarItemGroup(placement: .primaryAction) {
                if chat.canInterrupt {
                    Button { core.agentInterrupt() } label: { Label("Stop", systemImage: "stop.circle") }
                }
                if chat.canChooseModel {
                    Button { showChatOptions = true } label: {
                        Label(chat.chatOptions.models.first(where: \.selected)?.name ?? "Model", systemImage: "cpu")
                    }
                    .help("Model and effort in the desktop app")
                }
                subagentMenu
                if chat.canBrowseFiles {
                    Button { core.openFiles() } label: { Label("Files", systemImage: "folder") }
                    Button { core.openGit() } label: { Label("Git", systemImage: "arrow.triangle.branch") }
                }
            }
            #endif
        }
    }

    @ViewBuilder
    private func conversationHeader(core: MusterCore) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            if chat.loading && chat.messageCount == 0 { ProgressView("Loading conversation…") }
            if !chat.loadError.isEmpty {
                Label(chat.loadError, systemImage: "wifi.exclamationmark").font(.callout).foregroundStyle(.secondary)
            }
            if !chat.loading && chat.messageCount == 0 && chat.loadError.isEmpty {
                Text(chat.session.transcriptNotice.isEmpty ? "No messages yet" : chat.session.transcriptNotice)
                    .foregroundStyle(.secondary)
            }
            if chat.hasEarlier_p {
                Button("Load earlier") {
                    #if os(iOS)
                    earlierRequest += 1
                    #endif
                    core.agentLoadEarlier()
                }.frame(maxWidth: .infinity)
                    .disabled(chat.loading)
            }
        }
    }

    @ViewBuilder
    private var subagentMenu: some View {
        Menu {
            if chat.session.subagents.isEmpty {
                Text("No subagents")
            }
            ForEach(chat.session.subagents + chat.session.subsessions, id: \.id) { child in
                Button { core.openAgent(child.id, device: chat.deviceID) } label: {
                    Text("\(child.title.isEmpty ? child.id : child.title) · \(child.state)")
                }
            }
        } label: {
            Label("Subagents (\(chat.session.subagents.count))", systemImage: "person.2")
        }
        .accessibilityIdentifier("subagent-menu")
    }

    /// The provider's questions and approvals: under the transcript, or in the lower pane of a stacked workspace.
    @ViewBuilder
    private func interactionCards(core: MusterCore) -> some View {
        ForEach(chat.interactions, id: \.id) { interaction in
            InteractionCard(interaction: interaction,
                            submitting: !chat.respondingInteraction.isEmpty,
                            error: chat.responseError, outcomeUnknown: chat.responseUnknown) { option, answers in
                core.agentRespond(interaction: interaction.id, option: option, answers: answers)
            }
            .id("interaction-\(interaction.id)")
        }
    }

    /// The Locked Use status, the queue and the composer.
    @ViewBuilder
    private func lowerPane(core: MusterCore) -> some View {
        if !chat.lockedUseStatus.isEmpty {
            HStack {
                ProgressView().controlSize(.small)
                Text(chat.lockedUseStatus).font(.caption)
                Spacer()
                if chat.canCancelLockedUse { Button("Cancel") { core.cancelLockedUse() } }
            }.padding(8)
        }
        if !chat.queue.messages.isEmpty || !chat.queue.error.isEmpty {
            QueuedMessages(messages: chat.queue.messages,
                           submitting: chat.queue.submitting,
                           error: chat.queue.error, outcomeUnknown: chat.queue.outcomeUnknown) { id, option in
                core.agentRespond(interaction: id, option: option)
            }
            .frame(maxWidth: style.columnWidth)
            .padding(.horizontal, ChatStyle.inset)
        }
        // A desktop chat keeps its draft when the core moves it to
        // another Mac (every Mac shows the same conversation).
        if chat.session.readOnly {
            Label("Subagent · Read only", systemImage: "eye").font(.caption).foregroundStyle(.secondary).padding(12)
        } else {
            ChatComposer(core: core, chat: chat)
                .id(chat.session.mode == "chat" ? chat.session.id : "\(chat.deviceID):\(chat.session.id)")
                .frame(maxWidth: style.columnWidth)
                .padding(.horizontal, composerInset)
                .padding(.bottom, 8)
                .padding(.top, 8)
        }
    }

    private func updateViewport(_ height: CGFloat) {
        viewportHeight = height
        #if os(iOS)
        if draggingTranscript { atLatest = bottomPosition <= height + 48 }
        #else
        atLatest = bottomPosition <= height + 48
        #endif
    }

    private var composerInset: CGFloat {
        #if os(iOS)
        12
        #else
        ChatStyle.inset
        #endif
    }
}

private struct ConversationBottomKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

// A timeline item owns one complete measured row, including summary footers.
// This prevents ForEach from flattening a summary into independently sized rows.
struct ConversationBlock: View {
    let block: Muster_App_ChatBlock
    var openChange: (String, String) -> Void = { _, _ in }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch block.kind {
            case "working": WorkingGroup(block: block)
            case "activity": ActivityGroup(block: block)
            case "reasoning": ReasoningRow(block: block)
            case "diff":
                ForEach(block.diffs, id: \.id) { SessionDiffCard(file: $0) }
            case "notice": SessionNotice(block: block)
            case "compact_context": CompactContext(message: block.message)
            case "provider_card":
                VStack(alignment: .leading, spacing: 8) {
                    Text(block.title).font(.caption).foregroundStyle(.secondary)
                    MessageBubble(message: block.message, parts: block.parts)
                }
                .padding(12)
                .background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 12))
            case "summary":
                TurnChanges(message: block.message) { path in openChange(block.message.id, path) }
                TurnFooter(message: block.message)
            default: MessageBubble(message: block.message, parts: block.parts)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
        .environment(\.transcriptDisclosurePath, [block.id])
    }
}

struct WorkingGroup: View {
    let block: Muster_App_ChatBlock
    private var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("working", id: block.id, defaultValue: block.defaultExpanded) }
    private var expanded: Bool { expansion.wrappedValue }

    var body: some View {
        Group {
            if block.children.isEmpty {
                Label(block.title, systemImage: "circle.dotted")
                    .foregroundStyle(.secondary)
            } else {
                workGroup
            }
        }
        .font(.system(size: ChatStyle.bodySize))
        .fixedSize(horizontal: false, vertical: true)
    }

    private var workGroup: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button { expansion.wrappedValue.toggle() } label: {
                HStack(spacing: 8) {
                    Image(systemName: block.completed ? "checkmark.circle" : "circle.dotted")
                    Text(block.title)
                    Spacer(minLength: 0)
                    Image(systemName: expanded ? "chevron.down" : "chevron.right").font(.caption)
                }.contentShape(Rectangle())
            }
            .buttonStyle(.plain).foregroundStyle(.secondary)
            .accessibilityLabel(block.title)
            .accessibilityValue(expanded ? "Expanded" : "Collapsed")
            if expanded {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(block.children, id: \.id) { child in
                        if child.kind == "activity" { ActivityGroup(block: child) }
                        else { MessageBubble(message: child.message, parts: child.parts) }
                    }
                }
            }
        }
        .font(.system(size: ChatStyle.bodySize))
        .fixedSize(horizontal: false, vertical: true)
    }
}

struct ActivityGroup: View, Equatable {
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.block == rhs.block }
    let block: Muster_App_ChatBlock
    var expanded = false
    @Environment(\.transcriptDisclosurePath) private var disclosurePath

    private var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("activity", id: block.id, defaultValue: expanded) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button { withAnimation(.easeInOut(duration: 0.18)) { expansion.wrappedValue.toggle() } } label: {
                HStack(spacing: 8) {
                    Image(systemName: "terminal").frame(width: 20)
                    Text(block.title.isEmpty ? "Tool activity" : block.title).lineLimit(2)
                    Spacer(minLength: 0)
                    Image(systemName: expansion.wrappedValue ? "chevron.down" : "chevron.right").font(.caption)
                }.contentShape(Rectangle()).padding(.vertical, 4)
            }
            .buttonStyle(.plain).foregroundStyle(.secondary)
            .accessibilityValue(expansion.wrappedValue ? "Expanded" : "Collapsed")
            if expansion.wrappedValue {
                ForEach(block.activityRows, id: \.id) { ToolActivityRow(row: $0).environment(\.transcriptDisclosurePath, disclosurePath + [block.id]) }
            }
        }
        .font(.system(size: ChatStyle.bodySize))
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct ToolActivityRow: View {
    let row: Muster_App_ChatActivityRow
    var expanded = false
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }
    private var symbol: String {
        switch row.category {
        case "read": "book"
        case "search": "magnifyingglass"
        default: "terminal"
        }
    }

    private var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("tool", id: row.id, defaultValue: expanded) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button { withAnimation(.easeInOut(duration: 0.18)) { expansion.wrappedValue.toggle() } } label: {
                HStack(spacing: 8) {
                    Image(systemName: symbol).frame(width: 20)
                    Text(row.title).lineLimit(2).truncationMode(.tail)
                    ToolStatus(status: row.status)
                    Spacer(minLength: 0)
                    Image(systemName: expansion.wrappedValue ? "chevron.down" : "chevron.right").font(.caption)
                }.contentShape(Rectangle()).padding(.vertical, 3)
            }
            .buttonStyle(.plain).foregroundStyle(.secondary)
            .accessibilityValue(expansion.wrappedValue ? "Expanded" : "Collapsed")
            if expansion.wrappedValue {
                VStack(alignment: .leading, spacing: 12) {
                    Text(row.category == "shell" ? "Shell" : row.toolName)
                        .foregroundStyle(.secondary)
                    ScrollView(.vertical) {
                        Text(row.detail).font(.system(size: ChatStyle.bodySize - 2, design: .monospaced))
                            .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                    }.frame(maxHeight: 280)
                    if !row.output.isEmpty {
                        Text("Output").foregroundStyle(.secondary)
                        ScrollView(.vertical) {
                            Text(row.output).font(.system(size: ChatStyle.bodySize - 2, design: .monospaced))
                                .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        }.frame(maxHeight: 280)
                    }
                    Text(row.isResult ? "Output received" : "Arguments")
                        .font(.caption).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
                .padding(16).background(palette.code, in: RoundedRectangle(cornerRadius: 16))
                .overlay(RoundedRectangle(cornerRadius: 16).stroke(palette.border.opacity(0.5)))
            }
        }
        .font(.system(size: ChatStyle.bodySize))
    }
}

struct TurnChanges: View {
    let message: Muster_V1_AgentMessage
    var open: (String) -> Void = { _ in }
    private var expanded = false

    private var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("changes", id: message.id, defaultValue: expanded) }

    var body: some View {
        if !message.usage.changes.isEmpty || message.usage.changesIncomplete {
            DisclosureGroup(isExpanded: expansion) {
                VStack(spacing: 0) {
                    if message.usage.changesIncomplete {
                        Text("Some recorded changes could not be displayed").font(.caption).foregroundStyle(.secondary)
                    }
                    ForEach(message.usage.changes, id: \.path) { change in
                        Button { open(change.path) } label: {
                            HStack(spacing: 8) {
                                Image(systemName: change.kind == "delete" ? "doc.badge.minus" : "doc.text")
                                    .foregroundStyle(.secondary)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(change.openPath.isEmpty ? change.path : change.openPath).lineLimit(2).truncationMode(.middle)
                                    if !change.previousPath.isEmpty {
                                        Text("From \(change.previousPath)").font(.caption2).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer(minLength: 4)
                                if change.countsKnown {
                                    Text("+\(change.additions)").foregroundStyle(.green)
                                    Text("−\(change.deletions)").foregroundStyle(.red)
                                } else {
                                    Text(change.kind.capitalized).foregroundStyle(.secondary)
                                }
                                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
                            }
                            .font(.system(.caption, design: .monospaced))
                            .help("Recorded additions and removals across successful edits")
                            .padding(.vertical, 10).contentShape(Rectangle())
                        }.buttonStyle(.plain)
                        Divider()
                    }
                }.padding(.top, 4)
            } label: {
                Label("\(message.usage.changes.count) changed files", systemImage: "doc.on.doc")
                    .font(.subheadline.weight(.medium))
            }
            .padding(12)
            .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.secondary.opacity(0.15)))
            .padding(.top, 10)
        }
    }
}

struct CompactContext: View, Equatable {
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.message == rhs.message }
    let message: Muster_V1_AgentMessage
    private var expanded = false

    private var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("context", id: message.id, defaultValue: expanded) }

    var body: some View {
        DisclosureGroup(isExpanded: expansion) {
            ChatMarkdown(text: message.text)
                .padding(.top, 8)
        } label: {
            Label("Compact context", systemImage: "text.alignleft")
                .font(.subheadline).foregroundStyle(.secondary)
        }
        .tint(.secondary)
        .padding(.vertical, 6)
    }
}

struct MessageBubble: View, Equatable {
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.message == rhs.message && lhs.parts == rhs.parts }
    let message: Muster_V1_AgentMessage
    var parts: [Muster_App_MessagePart] = []
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    @ScaledMetric(relativeTo: .body) private var bodySize = ChatStyle.bodySize
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }
    private var isUser: Bool { message.role == "user" }

    var body: some View {
        HStack(alignment: .top) {
            if isUser { Spacer(minLength: 76) }
            VStack(alignment: .leading, spacing: 4) {
                if parts.contains(where: { $0.kind == "code" }) {
                    ConversationText(text: message.text, parts: parts)
                } else if isUser {
                    Text(message.text).font(.system(size: bodySize * style.bodyScale))
                        .foregroundStyle(palette.userText)
                        .lineSpacing(bodySize * style.lineSpacing).textSelection(.enabled)
                } else {
                    ChatMarkdown(text: message.text)
                }
            }
            .padding(.horizontal, isUser ? 16 : 0)
            .padding(.vertical, isUser ? 10 : 0)
            .background(isUser ? palette.userSurface : Color.clear, in: RoundedRectangle(cornerRadius: style.messageRadius))
            if !isUser { Spacer(minLength: 0) }
        }
        .fixedSize(horizontal: false, vertical: true)
        .padding(.vertical, 8)
    }
}

private struct SessionTraceView: View {
    let core: MusterCore
    let chat: Muster_App_ChatView
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text("Execution trace").font(.headline)
                    Spacer()
                    StateBadge(state: chat.session.state)
                }
                Text("Tool calls and turn events from the loaded conversation. Durations are approximate.")
                    .font(.caption).foregroundStyle(.secondary)
                if chat.hasEarlier_p {
                    Button("Load earlier events") { core.agentLoadEarlier() }
                }
                if chat.trace.isEmpty { Text(chat.loading ? "Loading trace…" : "No execution events yet").foregroundStyle(.secondary) }
                ForEach(chat.trace, id: \.id) { span in
                    DisclosureGroup {
                        Text(span.detail).font(.system(.caption, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(span.title).font(.callout.weight(.medium))
                                Spacer()
                                if span.durationKnown { Text("≈ \(span.durationSeconds) s").font(.caption).monospacedDigit() }
                            }
                            HStack {
                                if span.startedUnix > 0 { Text(Date(timeIntervalSince1970: TimeInterval(span.startedUnix)), style: .time) }
                                Text(span.status)
                            }.font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Divider()
                }
            }.padding(16).frame(maxWidth: 1000)
                .frame(maxWidth: .infinity)
        }.accessibilityIdentifier("session-trace")
    }
}

private struct RelatedSessionView: View {
    let core: MusterCore
    let device: String
    let sessions: [Muster_V1_AgentSession]
    let title: String
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                if sessions.isEmpty { Text("No \(title.lowercased())").foregroundStyle(.secondary).padding() }
                ForEach(sessions, id: \.id) { session in
                    Button { core.openAgent(session.id, device: device) } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(session.title.isEmpty ? session.id : session.title).multilineTextAlignment(.leading)
                                Spacer()
                                StateBadge(state: session.state)
                                Image(systemName: "chevron.right")
                            }
                            Text(session.relationship == "fork" ? "Fork" : (session.readOnly ? "Subagent · read only" : "Sub-session"))
                                .font(.caption).foregroundStyle(.secondary)
                        }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    }.buttonStyle(.plain)
                    Divider()
                }
            }.padding(12).frame(maxWidth: 1000).frame(maxWidth: .infinity)
        }.accessibilityIdentifier("related-sessions")
    }
}


private struct SessionTerminalPane: View {
    let core: MusterCore
    let workspace: Muster_App_SessionTerminalView
    @State private var desktopAttach = false
    let terminal: Muster_App_TerminalView
    #if os(iOS)
    @ObservedObject private var keyboardPrefs = KeyboardPrefs.shared
    #endif
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Label(workspace.attached ? "Connected" : "Workspace terminal", systemImage: "terminal")
                        .font(.subheadline.weight(.semibold))
                    Text(workspace.cwd).font(.caption2.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
                Spacer(minLength: 0)
                if workspace.busy { ProgressView().controlSize(.small) }
                else if !workspace.attached { Button("Connect") { core.sessionTerminal(.start) }.buttonStyle(.bordered).disabled(!workspace.available) }
                Menu {
                    if !terminal.desktopAttachCommand.isEmpty && !terminal.ended {
                        ForEach(terminal.nativeSources, id: \.source) { source in
                            Button("Open in \(source.label)") {
                                core.terminalCreate(source: source.source, attachManaged: true)
                            }
                        }
                        Button("Copy desktop connection command…") { desktopAttach = true }
                        Divider()
                    }
                    Button("Disconnect") { core.sessionTerminal(.detach) }.disabled(!workspace.attached)
                    Button("Restart terminal") { core.sessionTerminal(.restart) }.disabled(!workspace.canRestart)
                    Button("End terminal", role: .destructive) { core.sessionTerminal(.end) }.disabled(!workspace.canEnd)
                } label: { Image(systemName: "ellipsis.circle").frame(width: 36, height: 36) }
            }.padding(.horizontal, 16).padding(.vertical, 10)
            if !workspace.error.isEmpty { Text(workspace.error).font(.caption).foregroundStyle(.red).padding(8) }
            if workspace.attached || terminal.ended {
                TerminalGridView(terminal: terminal, core: core).id(terminal.session)
                #if os(iOS)
                KeyBar(core: core, canPaste: terminal.canPaste, showsKeys: keyboardPrefs.terminalSystemKeyboard)
                #endif
                Text(terminal.status).font(.caption).padding(4)
            } else { Spacer(); Text(workspace.busy ? "Starting terminal…" : "Not connected").foregroundStyle(.secondary); Spacer() }
        }.accessibilityIdentifier("session-terminal")
        .sheet(isPresented: $desktopAttach) { DesktopTerminalAttachSheet(terminal: terminal) }
    }
}

#endif
