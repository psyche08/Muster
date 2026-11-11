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
    @State private var creatingDesktop = false

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
            if !agents.nativeCreateHosts.isEmpty {
                Button { creatingDesktop = true } label: {
                    Label(agents.nativeCreating ? "Creating desktop session…" : "New desktop session", systemImage: "macwindow.badge.plus")
                }.disabled(agents.nativeCreating)
            }
            if agents.loading || agents.searching {
                HStack { ProgressView(); Text(agents.searching ? "Searching conversations…" : "Loading sessions…").foregroundStyle(.secondary) }
            }
            if !agents.loading && !agents.searching && agents.sections.allSatisfy({ $0.count == 0 }) {
                ContentUnavailableView(agents.query.isEmpty ? "No sessions" : "No matches", systemImage: agents.query.isEmpty ? "bubble.left.and.text.bubble.right" : "magnifyingglass", description: Text(agents.query.isEmpty ? "Codex and Claude sessions on this device show up here." : "Try another word from the title or conversation."))
            }
            ForEach(agents.errors + agents.searchNotices, id: \.self) { error in
                Label(error, systemImage: "info.circle").font(.caption).foregroundStyle(.secondary)
            }
            if agents.searchHasMore {
                Button { core.agentCatalogAction(.searchMore, value: "") } label: {
                    Label("Search more conversations", systemImage: "magnifyingglass")
                }.disabled(agents.searching)
            }
            ForEach(agents.sections, id: \.key) { section in
                CatalogSessionGroup(core: core, section: section, shown: shown, searching: !agents.query.isEmpty)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                DeviceTabs(devices: devices, selected: agents.deviceID, select: { core.openAgents(device: $0) }, details: { core.selectDevice($0) })
            }
            .background(.bar)
        }
        .searchable(text: Binding(get: { agents.query }, set: { core.agentCatalogAction(.search, value: $0) }), prompt: "Search titles and conversations")
        .navigationTitle("")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .refreshable { core.openAgents(device: agents.deviceID) }
        .sheet(isPresented: $creating) {
            OrcaCreateSheet(core: core, hosts: agents.createHosts, agents: agents.createAgents)
        }
        .sheet(isPresented: $creatingDesktop) {
            NativeCreateSheet(core: core, hosts: agents.nativeCreateHosts, apps: agents.nativeCreateApps,
                              draft: agents.hasNativeCreateDraft ? agents.nativeCreateDraft : nil)
        }
    }

}

private struct CatalogSessionGroup: View {
    let core: MusterCore
    let section: Muster_App_AgentCatalogSection
    let shown: String
    let searching: Bool
    var body: some View {
        Section {
            ForEach(section.rows, id: \.key) { row in
                CatalogSessionRow(core: core, row: row, displayMinute: Int(Date().timeIntervalSince1970 / 60))
                    .equatable()
                    .listRowBackground(shown == row.session.sourceKey ? Color.accentColor.opacity(0.15) : nil)
            }
            if section.expanded && section.hasMore_p {
                Button { core.agentCatalogAction(.loadMore, value: section.key) } label: {
                    HStack {
                        if section.loadingMore { ProgressView().controlSize(.small) }
                        Text(section.loadingMore ? "Loading…" : "Load more")
                    }
                }.disabled(section.loadingMore)
            }
            if section.expanded && section.count == 0 {
                Text("No sessions").font(.caption).foregroundStyle(.secondary)
            }
        } header: {
            Button { core.agentCatalogAction(.toggleSection, value: section.key) } label: {
                HStack(spacing: 7) {
                    Image(systemName: section.expanded ? "chevron.down" : "chevron.right").font(.caption2)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(section.title).font(.subheadline.weight(.semibold))
                        if !section.subtitle.isEmpty {
                            Text(section.subtitle).font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                    Spacer()
                    Text("\(section.count)").font(.caption).foregroundStyle(.secondary)
                }.contentShape(Rectangle())
            }.buttonStyle(.plain).textCase(nil).disabled(searching)
            .accessibilityLabel("\(section.title), \(section.count) sessions, \(section.expanded ? "expanded" : "collapsed")")

        }
    }
}

struct CatalogSessionRow: View, Equatable {
    let core: MusterCore
    let row: Muster_App_AgentCatalogRow
    let displayMinute: Int

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.core === rhs.core && lhs.row == rhs.row && lhs.displayMinute == rhs.displayMinute
    }

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            if row.depth > 0 {
                Rectangle().fill(Color.secondary.opacity(0.25)).frame(width: 1)
                    .padding(.leading, CGFloat(min(row.depth, 8) - 1) * 14 + 6)
            }
            Button { core.openAgent(row.session.id, device: row.session.deviceID) } label: {
                SessionRow(session: row.session, source: row.sourceLabel)
            }.buttonStyle(.plain)
            if row.expandable {
                Button { core.agentCatalogAction(.toggleFamily, value: row.key) } label: {
                    Image(systemName: row.expanded ? "chevron.down" : "chevron.right")
                        .font(.caption).frame(width: 24, height: 28).contentShape(Rectangle())
                }.buttonStyle(.plain)
                    .accessibilityLabel(row.expanded ? "Collapse child conversations" : "Expand child conversations")
            }
        }
        .accessibilityIdentifier("agent-row-\(row.session.id)")
        .contextMenu {
            if row.canOrganize { organizationActions }
            if !row.session.parentSessionID.isEmpty {
                Button { core.openAgent(row.session.parentSessionID, device: row.session.deviceID) } label: {
                    Label(row.session.parentTitle.isEmpty ? "Parent conversation" : row.session.parentTitle, systemImage: "arrow.turn.up.left")
                }
            }
        }
        #if os(iOS)
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if row.canOrganize {
                Button("Archive", role: .destructive) { perform(.archive) }.disabled(row.organizing)
                Button(row.session.pinned ? "Unpin" : "Pin") { perform(row.session.pinned ? .unpin : .pin) }.tint(.orange).disabled(row.organizing)
            }
        }
        #endif
    }
    private func perform(_ action: Muster_V1_AgentOrganizeRequest.Action) {
        core.agentOrganize(row.key, action: action)
    }
    @ViewBuilder private var organizationActions: some View {
        Group {
            Button(row.session.pinned ? "Unpin" : "Pin", systemImage: row.session.pinned ? "pin.slash" : "pin") { perform(row.session.pinned ? .unpin : .pin) }
            Button("Archive", systemImage: "archivebox", role: .destructive) { perform(.archive) }
        }.disabled(row.organizing)
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

/// A Code mode session in Claude or Codex on a Mac, started by its first
/// message. A locked Mac asks for authentication first (ADR 0150).
private struct NativeCreateSheet: View {
    let core: MusterCore
    let hosts: [Muster_V1_MemberView]
    let apps: [String]
    let draft: Muster_App_NativeCreate?
    @Environment(\.dismiss) private var dismiss
    @State private var device = ""
    @State private var provider = ""
    @State private var directory = ""
    @State private var prompt = ""
    private var ready: Bool {
        hosts.contains { $0.deviceID == device } && apps.contains(provider)
            && directory.trimmingCharacters(in: .whitespaces).hasPrefix("/")
            && !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
    var body: some View {
        NavigationStack {
            Form {
                Picker("Mac", selection: $device) {
                    ForEach(hosts, id: \.deviceID) { Text($0.name).tag($0.deviceID) }
                }
                Picker("App", selection: $provider) {
                    ForEach(apps, id: \.self) { Text($0.capitalized).tag($0) }
                }
                TextField("Absolute directory on that Mac", text: $directory)
                    .autocorrectionDisabled()
                Section("First message") {
                    TextEditor(text: $prompt).frame(minHeight: 100)
                }
                Text("Opens a new Code session in the app on that Mac and sends the first message. The app must be installed and signed in there, and the directory must exist. A locked Mac asks you to authenticate first; the session is never submitted twice.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .navigationTitle("New desktop session")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        core.createDesktopSession(device: device, provider: provider, directory: directory, prompt: prompt)
                        dismiss()
                    }.disabled(!ready)
                }
            }
            .onAppear {
                device = hosts.first(where: { $0.deviceID == draft?.deviceID })?.deviceID ?? hosts.first?.deviceID ?? ""
                provider = apps.first(where: { $0 == draft?.provider }) ?? apps.first ?? ""
                directory = draft?.directory ?? ""
                prompt = draft?.prompt ?? ""
            }
        }.frame(minWidth: 380, minHeight: 360)
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
    let source: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(session.title.isEmpty ? session.id : session.title)
                    .font(.body.weight(.medium)).lineLimit(1).truncationMode(.tail)
                    .help(session.title.isEmpty ? session.id : session.title)
                Spacer(minLength: 0)
                VStack(alignment: .trailing, spacing: 2) {
                    StateBadge(state: session.state)
                        .labelStyle(.titleAndIcon)
                    if session.processState == "running" && session.state == "unknown" {
                        Text("process alive").font(.caption2).foregroundStyle(.secondary)
                    }
                }.fixedSize(horizontal: true, vertical: false)
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(source).lineLimit(1).layoutPriority(1)
                if !session.cwd.isEmpty {
                    Text(session.cwd).lineLimit(1).truncationMode(.middle)
                        .foregroundStyle(.tertiary).help(session.cwd)
                }
                Spacer(minLength: 0)
                if session.updatedUnix > 0 {
                    Text(relative(session.updatedUnix)).fixedSize().layoutPriority(1)
                        .accessibilityLabel("Last message \(relative(session.updatedUnix))")
                }
            }.font(.caption).foregroundStyle(.secondary)
            if !session.searchSnippet.isEmpty {
                Text(session.searchSnippet).font(.callout).foregroundStyle(.secondary).lineLimit(3)
            }
        }
        .padding(.vertical, 3)
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
    #endif
    @State private var latestRequest = 0
    @State private var earlierRequest = 0
    @Environment(\.workspaceLayout) private var layout
    let core: MusterCore
    let terminal: Muster_App_TerminalView
    let chat: Muster_App_ChatView
    @State private var showChatOptions = false
    @State private var contentTab = "Conversation"
    @State private var atLatest = true
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
            #if os(iOS)
            // The navigation bar keeps the title and the actions menu; the
            // compact header carries the chips and every view tab.
            SessionChromeHeader(model: chromeModel, selected: $contentTab, actions: chromeActions, compact: true)
            #else
            SessionChromeHeader(model: chromeModel, selected: $contentTab, actions: chromeActions)
            #endif
            ZStack {
                ConversationTabContent(active: contentTab == "Conversation") {
                    conversationPane(core: core)
                }

                if contentTab != "Conversation" { alternatePane(core: core) }
            }
        }
        #if os(iOS)
        .environment(\.keyboardDock, layout.flex ? keyboardDock : nil)
        #endif
        .environment(\.executeCode, { core.codeAction(.preview, id: $0) })
        .environment(\.loadWork, { core.agentLoadWork($0) })
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
        #if os(iOS)
        .navigationTitle(chat.session.title.isEmpty ? "Chat" : chat.session.title)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .onChange(of: "\(chat.deviceID):\(chat.session.id)") { _, _ in atLatest = true; contentTab = "Conversation" }
        // A message of the reader's own belongs on screen the moment Rust
        // admits it, even if they had scrolled up while the agent worked.
        .onChange(of: chat.timeline.last { $0.kind == "admitted_send" }?.id ?? "") { _, admitted in
            if !admitted.isEmpty { latestRequest += 1 }
        }
        .onChange(of: contentTab) { old, new in
            if new == "Terminal" { core.sessionTerminal(.start) }
            else if old == "Terminal" { core.sessionTerminal(.detach) }
        }
        .onDisappear { if contentTab == "Terminal" { core.sessionTerminal(.detach) } }
        .sheet(isPresented: $showChatOptions) {
            ChatOptionsSheet(core: core, chat: chat) { showChatOptions = false }
        }
        #if os(iOS)
        .toolbar {
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
        }
        #endif
    }

    private var chromeModel: SessionChromeModel {
        var value = SessionChromeModel()
        value.title = chat.session.title.isEmpty ? chat.session.id : chat.session.title
        value.parentTitle = chat.session.parentSessionID.isEmpty ? "" : (chat.session.parentTitle.isEmpty ? "Parent session" : chat.session.parentTitle)
        value.providerLabel = chat.providerLabel
        value.deviceName = chat.session.deviceName
        value.state = chat.session.state
        value.cwd = chat.session.cwd
        value.subagentCount = chat.session.subagents.count
        value.subsessionCount = chat.session.subsessions.count
        value.canInterrupt = chat.canInterrupt
        value.canChooseModel = chat.canChooseModel
        value.canBrowseFiles = chat.canBrowseFiles
        value.terminalAvailable = chat.terminal.available
        return value
    }

    private var chromeActions: SessionChromeActions {
        SessionChromeActions(
            openParent: { core.openAgent(chat.session.parentSessionID, device: chat.deviceID) },
            stop: { core.agentInterrupt() },
            modelOptions: { showChatOptions = true },
            files: { core.openFiles() },
            git: { core.openGit() }
        )
    }

    @ViewBuilder
    private func conversationPane(core: MusterCore) -> some View {
        VStack(spacing: 0) {
            Group {
            RecycledConversation(chat: chat,
                header: AnyView(conversationHeader(core: core)),
                interactions: AnyView(VStack(alignment: .leading, spacing: 14) { interactionCards(core: core) }),
                showsInteractions: !layout.flex,
                blockContent: { block in AnyView(ConversationBlock(block: block) { message, path in
                    core.openChange(summary: message, path: path)
                }) }, jumpToLatest: latestRequest, holdPosition: earlierRequest, atLatest: $atLatest,
                loadEarlier: { earlierRequest += 1; core.agentLoadEarlier() })
                .overlay(alignment: .bottom) {
                    if !atLatest || chat.hasNewer_p {
                        Button { core.agentLoadEarlier(latest: true); latestRequest += 1 } label: {
                            Image(systemName: "arrow.down").font(.system(size: 19, weight: .medium))
                                .frame(width: 38, height: 38)
                                .background(.regularMaterial, in: Circle())
                                .overlay(Circle().stroke(palette.border, lineWidth: 0.5))
                        }
                        .buttonStyle(.plain).accessibilityLabel("Jump to latest")
                        .padding(.bottom, 8)
                    }
                }
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
        }
    }

    private func alternatePane(core: MusterCore) -> some View {
        VStack(spacing: 0) {
            if contentTab == "Terminal" {
                CodeRunHistory(runs: chat.codeRuns, error: chat.codeError) { core.codeAction(.return, id: $0) }
                SessionTerminalPane(core: core, workspace: chat.terminal, terminal: terminal)
            } else if contentTab == "Tracing" {
                SessionTraceScreen(chat: chat, loadEarlier: { core.agentLoadEarlier() })
            } else {
                RelatedSessionView(core: core, device: chat.deviceID,
                    sessions: contentTab == "Subagents" ? chat.session.subagents : chat.session.subsessions,
                    title: contentTab)
            }
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
            if chat.hasNewer_p {
                Button("Load newer") { core.agentLoadEarlier(newer: true) }.disabled(chat.loading)
                    .frame(maxWidth: .infinity)
            }
            if chat.hasEarlier_p {
                Button("Load earlier") {
                    earlierRequest += 1
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
                            compactWrapUp: chat.compactWrapUpIds.contains(interaction.id),
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

    private var composerInset: CGFloat {
        #if os(iOS)
        12
        #else
        ChatStyle.inset
        #endif
    }
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
            case "admitted_send": AdmittedSendRow(block: block)
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

/// A message this app sent that the conversation does not account for yet.
/// Rust decides the state and its wording; this row only shows both, so a
/// message inserted while the agent works is never invisible.
struct AdmittedSendRow: View, Equatable {
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.block == rhs.block }
    let block: Muster_App_ChatBlock
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }
    private var failed: Bool { block.title.hasPrefix("未发送") }
    private var attention: Bool { failed || block.title.contains("未确认") || block.title.contains("尚未") }

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            MessageBubble(message: block.message).opacity(failed ? 0.65 : 0.85)
            Label(block.title, systemImage: failed ? "exclamationmark.circle"
                : attention ? "clock.badge.questionmark" : "arrow.up.circle")
                .font(.caption)
                .foregroundStyle(attention ? AnyShapeStyle(.orange) : AnyShapeStyle(palette.secondary))
                .padding(.trailing, 4)
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .accessibilityElement(children: .combine)
    }
}

private struct LoadWorkKey: EnvironmentKey { static let defaultValue: (String) -> Void = { _ in } }
extension EnvironmentValues {
    /// Asks Rust for a collapsed turn's steps by Worked block ID (ADR 0160).
    var loadWork: (String) -> Void {
        get { self[LoadWorkKey.self] }
        set { self[LoadWorkKey.self] = newValue }
    }
}

struct WorkingGroup: View {
    let block: Muster_App_ChatBlock
    @Environment(\.conversationStyle) private var style
    // Not private: a plain private stored property makes the synthesized
    // memberwise initializer private and unusable from other types.
    var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("working", id: block.id, defaultValue: block.defaultExpanded) }
    private var expanded: Bool { expansion.wrappedValue }

    var body: some View {
        Group {
            if block.children.isEmpty && block.details.isEmpty {
                if style == .deepseek {
                    DeepSeekRowTitle(title: block.title, expanded: false, running: !block.completed)
                } else {
                    Label(block.title, systemImage: "circle.dotted")
                        .foregroundStyle(.secondary)
                }
            } else if style == .deepseek {
                deepseekGroup
            } else {
                workGroup
            }
        }
        .font(.system(size: ChatStyle.bodySize))
        .fixedSize(horizontal: false, vertical: true)
    }

    /// DSH's turn process: a quiet title with its chevron, the rows 6pt apart.
    private var deepseekGroup: some View {
        VStack(alignment: .leading, spacing: DeepSeekChat.titleGap) {
            Button { expansion.wrappedValue.toggle() } label: {
                DeepSeekRowTitle(title: block.title, expanded: expanded, running: !block.completed)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(block.title)
            .accessibilityValue(expanded ? "Expanded" : "Collapsed")
            if expanded {
                VStack(alignment: .leading, spacing: DeepSeekChat.rowGap) { rows }
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
                VStack(alignment: .leading, spacing: 12) { rows }
            }
        }
        .font(.system(size: ChatStyle.bodySize))
        .fixedSize(horizontal: false, vertical: true)
    }

    @ViewBuilder private var rows: some View {
        ForEach(block.children, id: \.id) { child in
            if child.kind == "activity" { ActivityGroup(block: child) }
            else if child.kind == "reasoning" { ReasoningRow(block: child) }
            else if child.kind == "notice" { Text(child.title).font(.caption).foregroundStyle(.secondary) }
            else { MessageBubble(message: child.message, parts: child.parts) }
        }
        WorkDetails(block: block)
    }
}

/// An expanded Worked group whose steps are not loaded yet. Rust decides the
/// state; this asks for the steps when shown and offers a retry.
struct WorkDetails: View {
    let block: Muster_App_ChatBlock
    @Environment(\.loadWork) private var load

    var body: some View {
        switch block.details {
        case "unloaded", "loading":
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("Loading steps…")
            }
            .font(.caption).foregroundStyle(.secondary)
            .onAppear { if block.details == "unloaded" { load(block.id) } }
        case "failed":
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(block.detailsError).lineLimit(3)
                Button("Retry") { load(block.id) }
            }
            .font(.caption).foregroundStyle(.secondary)
        default:
            EmptyView()
        }
    }
}

struct ActivityGroup: View, Equatable {
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.block == rhs.block }
    let block: Muster_App_ChatBlock
    var expanded = false
    @Environment(\.transcriptDisclosurePath) private var disclosurePath
    @Environment(\.conversationStyle) private var style

    var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("activity", id: block.id, defaultValue: expanded) }

    var body: some View {
        if style == .deepseek { deepseekBody } else { standardBody }
    }

    private var deepseekBody: some View {
        VStack(alignment: .leading, spacing: DeepSeekChat.rowGap) {
            Button { withAnimation(.easeInOut(duration: 0.18)) { expansion.wrappedValue.toggle() } } label: {
                DeepSeekRowTitle(systemImage: "terminal", title: block.title.isEmpty ? "Tool activity" : block.title,
                                 expanded: expansion.wrappedValue)
            }
            .buttonStyle(.plain)
            .accessibilityValue(expansion.wrappedValue ? "Expanded" : "Collapsed")
            if expansion.wrappedValue {
                VStack(alignment: .leading, spacing: DeepSeekChat.rowGap) {
                    ForEach(block.activityRows, id: \.id) { ToolActivityRow(row: $0).environment(\.transcriptDisclosurePath, disclosurePath + [block.id]) }
                }
                .padding(.leading, DeepSeekChat.bodyIndent)
            }
        }
        .font(.system(size: ChatStyle.bodySize))
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var standardBody: some View {
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

    var disclosure = TranscriptDisclosure()
    private var expansion: Binding<Bool> { disclosure.binding("tool", id: row.id, defaultValue: expanded) }

    var body: some View {
        if style == .deepseek { deepseekBody } else { standardBody }
    }

    private var deepseekBody: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { withAnimation(.easeInOut(duration: 0.18)) { expansion.wrappedValue.toggle() } } label: {
                DeepSeekRowTitle(systemImage: symbol, title: row.title, status: row.status,
                                 expanded: expansion.wrappedValue)
            }
            .buttonStyle(.plain)
            .accessibilityValue(expansion.wrappedValue ? "Expanded" : "Collapsed")
            if expansion.wrappedValue {
                DeepSeekRowBody {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(row.category == "shell" ? "Shell" : row.toolName)
                            .font(.system(size: DeepSeekChat.secondarySize - 1))
                        DeepSeekBoundedText(text: row.detail, maxHeight: DeepSeekChat.bodyMaxHeight / 2)
                        if !row.output.isEmpty {
                            Divider().opacity(0.6)
                            Text("Output").font(.system(size: DeepSeekChat.secondarySize - 1))
                            DeepSeekBoundedText(text: row.output, maxHeight: DeepSeekChat.bodyMaxHeight)
                        }
                    }
                }
            }
        }
        .font(.system(size: ChatStyle.bodySize))
    }

    private var standardBody: some View {
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
    var expanded = false

    var disclosure = TranscriptDisclosure()
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
    var expanded = false

    var disclosure = TranscriptDisclosure()
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
            if isUser { Spacer(minLength: style == .deepseek ? 56 : 76) }
            VStack(alignment: .leading, spacing: 4) {
                ForEach(message.previewJpegs.indices, id: \.self) { index in
                    if let image = rasterThumbnail(message.previewJpegs[index], size: 480) {
                        Image(decorative: image, scale: 1).resizable().scaledToFit()
                            .frame(maxWidth: 320, maxHeight: 260, alignment: .leading)
                            .accessibilityLabel("Image preview")
                    }
                }
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
            // DSH caps the user's bubble at about 70% of its chat column.
            .frame(maxWidth: isUser && style == .deepseek ? DeepSeekChat.userMaxWidth : nil, alignment: .trailing)
            if !isUser { Spacer(minLength: 0) }
        }
        .fixedSize(horizontal: false, vertical: true)
        .padding(.vertical, 8)
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
