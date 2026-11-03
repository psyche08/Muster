// The session screen's chrome, styled after DeepSeek Harness's session
// header: a title row with session utilities and actions, then view tabs with
// an underline for the active view. The Mac shows the full header; iOS uses
// the compact form under its native navigation bar, which keeps the title and
// the actions menu. Presentation only: every value comes from the Rust-owned
// ChatView and every control sends an Intent or a shell-only preference. See
// ADR 0086, ADR 0103 and ADR 0106.
#if canImport(SwiftUI)
import SwiftUI

/// What the header shows for one session. Built by `ChatView` from the
/// Rust-owned `Muster_App_ChatView`; the header decides nothing.
struct SessionChromeModel: Equatable {
    var title = ""
    var parentTitle = ""
    var providerLabel = ""
    var deviceName = ""
    var state = "unknown"
    var cwd = ""
    var subagentCount = 0
    var subsessionCount = 0
    var canInterrupt = false
    var canChooseModel = false
    var canBrowseFiles = false
    var terminalAvailable = false
}

/// The shell callbacks the header forwards; none of them decide trust.
struct SessionChromeActions {
    var openParent: () -> Void = {}
    var stop: () -> Void = {}
    var modelOptions: () -> Void = {}
    var files: () -> Void = {}
    var git: () -> Void = {}
}

struct SessionChromeHeader: View {
    let model: SessionChromeModel
    @Binding var selected: String
    let actions: SessionChromeActions
    /// The phone and tablet form: chips only (the navigation bar shows the
    /// title and the actions), and touch-sized tabs.
    var compact = false
    @Environment(\.colorScheme) private var scheme
    @AppStorage(ConversationStyle.preferenceKey) private var style = ConversationStyle.codex

    private var palette: SessionChromePalette { SessionChromePalette(scheme: scheme) }

    private var tabs: [(id: String, title: String)] {
        [
            ("Conversation", "Conversation"),
            ("Tracing", "Trace"),
            ("Subagents", model.subagentCount > 0 ? "Subagents (\(model.subagentCount))" : "Subagents"),
            ("Sessions", model.subsessionCount > 0 ? "Sessions (\(model.subsessionCount))" : "Sessions"),
            ("Terminal", "Terminal"),
        ]
    }

    var body: some View {
        VStack(spacing: 0) {
            if compact { compactTitleRow } else { titleRow }
            tabsRow
        }
        .padding(.top, compact ? SessionChromeMetrics.compactHeaderTopPadding : SessionChromeMetrics.headerTopPadding)
        .padding(.leading, compact ? SessionChromeMetrics.compactHeaderPadding : SessionChromeMetrics.headerLeadingPadding)
        .padding(.trailing, compact ? 0 : SessionChromeMetrics.headerTrailingPadding)
        .frame(minHeight: compact ? 0 : SessionChromeMetrics.headerMinHeight, alignment: .top)
        .background(palette.bgBase)
        .overlay(alignment: .bottom) {
            Rectangle().fill(palette.borderL3)
                .frame(height: SessionChromeMetrics.dividerWidth)
        }
        .accessibilityIdentifier("session-chrome")
    }

    // MARK: Title row

    private var titleRow: some View {
        HStack(spacing: SessionChromeMetrics.titleClusterGap) {
            crumbs
            Spacer(minLength: 12)
            utilities
            actionsRow
        }
        .frame(minHeight: SessionChromeMetrics.titleRowMinHeight)
    }

    /// The compact title row scrolls sideways rather than truncating chips.
    private var compactTitleRow: some View {
        ScrollView(.horizontal) {
            HStack(spacing: SessionChromeMetrics.chipGap) {
                if !model.parentTitle.isEmpty {
                    SessionChromeChip(title: model.parentTitle, systemImage: "arrow.turn.up.left",
                                      action: actions.openParent)
                        .accessibilityIdentifier("session-chrome-parent")
                }
                utilities
                if !model.cwd.isEmpty {
                    SessionChromeChip(title: URL(fileURLWithPath: model.cwd).lastPathComponent,
                                      systemImage: "folder")
                        .help(model.cwd)
                }
            }
            .padding(.trailing, SessionChromeMetrics.compactHeaderPadding)
        }
        .scrollIndicators(.hidden)
        .frame(minHeight: SessionChromeMetrics.titleRowMinHeight)
    }

    private var crumbs: some View {
        HStack(spacing: SessionChromeMetrics.crumbGap) {
            if !model.parentTitle.isEmpty {
                Button(action: actions.openParent) {
                    ChromeCrumbText(model.parentTitle, colour: palette.labelTertiary)
                        .background(palette.hover.opacity(0), in: RoundedRectangle(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                .help("Open the parent session")
                Text("/")
                    .font(.system(size: SessionChromeMetrics.titleFontSize))
                    .foregroundStyle(palette.labelCaption)
            }
            Text(model.title.isEmpty ? "Session" : model.title)
                .font(.system(size: SessionChromeMetrics.titleFontSize, weight: .medium))
                .foregroundStyle(palette.labelPrimary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: 420, alignment: .leading)
                .help(model.title)
        }
        .frame(minWidth: 0, alignment: .leading)
    }

    private var utilities: some View {
        HStack(spacing: SessionChromeMetrics.chipGap) {
            SessionChromeChip(title: stateLabel, dotColor: stateColor)
            if !model.providerLabel.isEmpty {
                SessionChromeChip(title: model.providerLabel)
            }
            if !model.deviceName.isEmpty {
                SessionChromeChip(title: model.deviceName,
                                  systemImage: deviceSymbol,
                                  lineLimit: 1)
                    .help(model.deviceName)
            }
            if model.subagentCount > 0 {
                SessionChromeChip(title: "\(model.subagentCount) subagents",
                                  systemImage: "person.2") { selected = "Subagents" }
                    .accessibilityIdentifier("session-chip-subagents")
            }
            if model.subsessionCount > 0 {
                SessionChromeChip(title: "\(model.subsessionCount) sessions",
                                  systemImage: "rectangle.stack") { selected = "Sessions" }
                    .accessibilityIdentifier("session-chip-sessions")
            }
        }
    }

    private var deviceSymbol: String {
        // Session device kinds are not projected into ChatView; the desktop
        // computer icon is the honest default for this screen's peers.
        "desktopcomputer"
    }

    private var stateLabel: String {
        switch model.state {
        case "running": return "working"
        case "waiting_approval": return "needs you"
        case "idle": return "idle"
        case "ended": return "ended"
        default: return "unknown"
        }
    }

    private var stateColor: Color {
        switch model.state {
        case "running": return palette.successPrimary
        case "waiting_approval": return palette.warnLabel
        case "idle": return palette.labelTertiary
        case "ended": return palette.labelTertiary
        default: return palette.idlePrimary
        }
    }

    private var actionsRow: some View {
        HStack(spacing: 2) {
            styleMenu
            if model.canInterrupt {
                ChromeIconButton(systemImage: "stop.fill", help: "Stop") { actions.stop() }
                    .accessibilityIdentifier("session-chrome-stop")
            }
            if model.canChooseModel {
                ChromeIconButton(systemImage: "cpu", help: "Model and effort") { actions.modelOptions() }
                    .accessibilityIdentifier("session-chrome-model")
            }
            if model.canBrowseFiles {
                ChromeIconButton(systemImage: "folder", help: "Files") { actions.files() }
                ChromeIconButton(systemImage: "arrow.triangle.branch", help: "Git") { actions.git() }
            }
        }
        .padding(.leading, SessionChromeMetrics.headerUtilitiesMargin)
    }

    private var styleMenu: some View {
        ChromeIconMenu(systemImage: "paintpalette", help: "Conversation style") {
            ForEach(ConversationStyle.allCases) { option in
                Button {
                    style = option
                } label: {
                    if style == option {
                        Label(option.title, systemImage: "checkmark")
                    } else {
                        Text(option.title)
                    }
                }
            }
        }
        .accessibilityIdentifier("session-style")
    }

    // MARK: View tabs

    private var tabsRow: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal) {
                HStack(alignment: .bottom, spacing: compact ? SessionChromeMetrics.compactTabGap : SessionChromeMetrics.tabGap) {
                    ForEach(tabs, id: \.id) { tab in
                        tabButton(id: tab.id, title: tab.title).id(tab.id)
                    }
                }
                .padding(.trailing, compact ? SessionChromeMetrics.compactHeaderPadding : 0)
            }
            .scrollIndicators(compact ? .hidden : .visible)
            .onAppear { proxy.scrollTo(selected, anchor: .center) }
            .onChange(of: selected) { _, tab in
                withAnimation { proxy.scrollTo(tab, anchor: .center) }
            }
        }
        .frame(height: compact
            ? SessionChromeMetrics.compactTabTopInset + SessionChromeMetrics.compactTabLineHeight + SessionChromeMetrics.tabBottomInset
            : SessionChromeMetrics.tabLineHeight + SessionChromeMetrics.tabBottomInset)
        .padding(.leading, compact ? 0 : SessionChromeMetrics.tabsLeadingInset)
        .padding(.top, compact ? 0 : SessionChromeMetrics.tabsTopGap)
        .accessibilityIdentifier("session-chrome-tabs")
    }

    private func tabButton(id: String, title: String) -> some View {
        let active = selected == id
        return Button { selected = id } label: {
            Text(title)
                .font(.system(size: compact ? SessionChromeMetrics.compactTabFontSize : SessionChromeMetrics.tabFontSize,
                              weight: .medium))
                .foregroundStyle(active ? palette.businessPrimary : palette.labelTertiary)
                .fixedSize()
                .padding(.top, compact ? SessionChromeMetrics.compactTabTopInset : 0)
                .padding(.bottom, SessionChromeMetrics.tabBottomInset)
                .overlay(alignment: .bottom) {
                    Rectangle()
                        .fill(active ? palette.businessPrimary : Color.clear)
                        .frame(height: SessionChromeMetrics.tabUnderlineHeight)
                        .clipShape(RoundedRectangle(cornerRadius: SessionChromeMetrics.tabUnderlineRadius))
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(id == "Terminal" && !model.terminalAvailable)
        .help(id == "Terminal" && !model.terminalAvailable ? "Terminal unavailable for this session" : title)
        .accessibilityIdentifier("session-tab-\(id)")
        .accessibilityAddTraits(active ? .isSelected : [])
    }

}

// MARK: Small chrome controls

private struct SessionChromeChip: View {
    let title: String
    var systemImage: String? = nil
    var dotColor: Color? = nil
    var lineLimit: Int = 1
    var action: (() -> Void)? = nil
    @Environment(\.colorScheme) private var scheme
    @State private var hovered = false
    private var palette: SessionChromePalette { SessionChromePalette(scheme: scheme) }

    var body: some View {
        if let action {
            Button(action: action) { content }
                .buttonStyle(.plain)
                .onHover { hovered = $0 }
        } else {
            content
        }
    }

    private var content: some View {
        HStack(spacing: 4) {
            if let dotColor {
                Circle().fill(dotColor).frame(width: 6, height: 6)
            }
            if let systemImage {
                Image(systemName: systemImage)
                    .font(.system(size: SessionChromeMetrics.chipIconSize))
            }
            Text(title)
                .font(.system(size: SessionChromeMetrics.chipFontSize))
                .lineLimit(lineLimit)
                .truncationMode(.middle)
        }
        .foregroundStyle(palette.labelSecondary)
        .frame(height: SessionChromeMetrics.chipHeight)
        .padding(.horizontal, SessionChromeMetrics.chipHorizontalPadding)
        .background(hovered ? palette.hover : Color.clear,
                    in: RoundedRectangle(cornerRadius: SessionChromeMetrics.chipCornerRadius))
        .contentShape(Rectangle())
    }
}

private struct ChromeIconButton: View {
    let systemImage: String
    let help: String
    let action: () -> Void
    @Environment(\.colorScheme) private var scheme
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(SessionChromePalette(scheme: scheme).labelSecondary)
                .frame(width: 24, height: 24)
                .background(hovered ? SessionChromePalette(scheme: scheme).hover : Color.clear,
                            in: RoundedRectangle(cornerRadius: 8))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
        .onHover { hovered = $0 }
    }
}

private struct ChromeIconMenu<Content: View>: View {
    let systemImage: String
    let help: String
    @ViewBuilder let content: () -> Content
    @Environment(\.colorScheme) private var scheme
    @State private var hovered = false

    var body: some View {
        Menu {
            content()
        } label: {
            Image(systemName: systemImage)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(SessionChromePalette(scheme: scheme).labelSecondary)
                .frame(width: 24, height: 24)
                .background(hovered ? SessionChromePalette(scheme: scheme).hover : Color.clear,
                            in: RoundedRectangle(cornerRadius: 8))
                .contentShape(Rectangle())
        }
        #if os(macOS)
        .menuStyle(.borderlessButton)
        #endif
        .menuIndicator(.hidden)
        .fixedSize()
        .help(help)
        .onHover { hovered = $0 }
    }
}

private struct ChromeCrumbText: View {
    let text: String
    let colour: Color

    init(_ text: String, colour: Color) {
        self.text = text
        self.colour = colour
    }

    var body: some View {
        Text(text)
            .font(.system(size: SessionChromeMetrics.titleFontSize))
            .foregroundStyle(colour)
            .lineLimit(1)
            .truncationMode(.tail)
            .fixedSize(horizontal: true, vertical: false)
            .padding(.vertical, 4)
            .padding(.horizontal, 8)
            .contentShape(Rectangle())
    }
}
#endif
