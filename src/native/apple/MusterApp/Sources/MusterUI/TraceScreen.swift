// The DSH-aligned session trace page (ADR 0083): a toolbar, a 50pt three-lane
// time overview and a turn-grouped ledger over `chat.trace`. Pure layout lives
// in TraceModel.swift; this file only draws it. Display only: every span and
// its status is Rust-projected, and missing timing stays missing.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI

struct SessionTraceScreen: View {
    let chat: Muster_App_ChatView
    let loadEarlier: () -> Void

    @Environment(\.colorScheme) private var scheme
    @State private var mode: TraceTimelineMode = .sequence
    @State private var search = ""
    @State private var selectedID: String?
    @State private var collapsedTurns: Set<String> = []

    private var palette: SessionChromePalette { SessionChromePalette(scheme: scheme) }
    private var ledger: TraceLedger { TraceLedger.build(spans: chat.trace) }
    private var shownLedger: TraceLedger {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return query.isEmpty ? ledger : ledger.filtering(query)
    }
    private var timeline: TraceTimelineModel {
        TraceTimelineModel(records: ledger.records, mode: mode)
    }
    private var selectedRecord: TraceRecord? {
        guard let selectedID else { return nil }
        return ledger.records.first { $0.id == selectedID }
    }

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            TraceTimelineOverview(model: timeline,
                                  palette: palette,
                                  hasEarlier: chat.hasEarlier_p,
                                  loadEarlier: loadEarlier,
                                  selectedID: $selectedID)
            #if os(macOS)
            HStack(spacing: 0) {
                ledgerList
                if let selectedRecord {
                    Rectangle()
                        .fill(palette.borderL2)
                        .frame(width: 0.5)
                    TraceInspectorPanel(record: selectedRecord, palette: palette) {
                        selectedID = nil
                    }
                }
            }
            #else
            ledgerList
            #endif
        }
        .background(palette.bgBase)
        .accessibilityIdentifier("session-trace")
        #if os(iOS)
        .sheet(item: inspectorItem) { record in
            TraceInspectorSheet(record: record, palette: palette) {
                selectedID = nil
            }
        }
        #endif
    }

    private var ledgerList: some View {
        TraceLedgerList(ledger: shownLedger,
                        palette: palette,
                        query: search,
                        loading: chat.loading,
                        loadError: chat.loadError,
                        hasEarlier: chat.hasEarlier_p,
                        collapsedTurns: $collapsedTurns,
                        selectedID: $selectedID,
                        loadEarlier: loadEarlier)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    #if os(iOS)
    private var inspectorItem: Binding<TraceRecord?> {
        Binding(get: { selectedRecord }, set: { selectedID = $0?.id })
    }
    #endif

    // MARK: Toolbar

    private var toolbar: some View {
        HStack(spacing: 4) {
            TraceToolbarButton(title: "Duration: \(mode.label)",
                               systemImage: mode == .sequence ? "arrow.left.and.right" : "clock",
                               palette: palette) {
                mode = mode.toggled
            }
            .help("Switch the time overview between equal-width events and their recorded times")
            .accessibilityIdentifier("trace-duration-mode")

            TraceToolbarButton(title: "Expand all", palette: palette) {
                collapsedTurns.removeAll()
            }
            TraceToolbarButton(title: "Collapse all", palette: palette) {
                collapsedTurns = Set(ledger.turns.map(\.id))
            }
            Spacer(minLength: 8)
            searchField
        }
        .padding(.horizontal, 10)
        .frame(height: SessionChromeMetrics.traceToolbarHeight)
        .background(palette.bgLayer1)
        .overlay(alignment: .bottom) {
            Rectangle().fill(palette.borderL2).frame(height: 0.5)
        }
        .accessibilityIdentifier("trace-toolbar")
    }

    private var searchField: some View {
        HStack(spacing: 5) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(palette.labelTertiary)
            TextField("Search", text: $search,
                      prompt: Text("Search").foregroundColor(palette.labelTertiary))
                .textFieldStyle(.plain)
                .font(.system(size: 11))
                .foregroundStyle(palette.labelPrimary)
                .accessibilityIdentifier("trace-search")
            if !search.isEmpty {
                Button {
                    search = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 10))
                        .foregroundStyle(palette.labelTertiary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Clear search")
            }
        }
        .padding(.horizontal, 6)
        .frame(width: SessionChromeMetrics.traceSearchWidth,
               height: SessionChromeMetrics.traceSearchHeight)
        .background(palette.bgLayer2, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(palette.borderL1, lineWidth: 0.5))
    }
}

// MARK: - Toolbar button

private struct TraceToolbarButton: View {
    let title: String
    var systemImage: String?
    let palette: SessionChromePalette
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                if let systemImage {
                    Image(systemName: systemImage).font(.system(size: 10, weight: .medium))
                }
                Text(title).font(.system(size: SessionChromeMetrics.traceToolbarFontSize))
            }
            .foregroundStyle(palette.labelSecondary)
            .padding(.horizontal, 6)
            .frame(height: SessionChromeMetrics.traceToolbarControlHeight)
            .background(hovered ? palette.hover : Color.clear,
                        in: RoundedRectangle(cornerRadius: 4))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
    }
}

// MARK: - Time overview

private struct TraceTimelineOverview: View {
    let model: TraceTimelineModel
    let palette: SessionChromePalette
    let hasEarlier: Bool
    let loadEarlier: () -> Void
    @Binding var selectedID: String?

    private var laneHeight: CGFloat { SessionChromeMetrics.traceTimelineLaneHeight }
    private var barHeight: CGFloat { SessionChromeMetrics.traceTimelineBarHeight }

    var body: some View {
        HStack(spacing: 8) {
            VStack(spacing: 0) {
                ForEach(Array(TraceTimelineModel.laneLabels.enumerated()), id: \.offset) { _, label in
                    Text(label)
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(palette.labelTertiary)
                        .frame(height: laneHeight, alignment: .center)
                }
            }
            .frame(width: SessionChromeMetrics.traceTimelineLabelWidth, alignment: .trailing)

            GeometryReader { geometry in
                plot(size: geometry.size)
            }
            .frame(height: CGFloat(TraceTimelineModel.laneCount) * laneHeight)
        }
        .padding(.horizontal, 10)
        .frame(height: SessionChromeMetrics.traceTimelineHeight)
        .accessibilityIdentifier("trace-timeline")
    }

    private func plot(size: CGSize) -> some View {
        ZStack(alignment: .topLeading) {
            ForEach(model.boundaries, id: \.self) { fraction in
                Rectangle()
                    .fill(palette.borderL2)
                    .frame(width: 0.5, height: CGFloat(TraceTimelineModel.laneCount) * laneHeight)
                    .offset(x: CGFloat(fraction) * size.width)
            }
            ForEach(0..<TraceTimelineModel.laneCount, id: \.self) { lane in
                ForEach(model.lanes[lane]) { bar in
                    barButton(bar, plotWidth: size.width)
                        .offset(x: barX(bar, plotWidth: size.width),
                                y: CGFloat(lane) * laneHeight)
                }
            }
            if model.mode == .timed && !model.hasTiming {
                Text("No timing data")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.labelTertiary)
                    .frame(width: size.width,
                           height: CGFloat(TraceTimelineModel.laneCount) * laneHeight)
            }
        }
        .frame(width: size.width,
               height: CGFloat(TraceTimelineModel.laneCount) * laneHeight,
               alignment: .topLeading)
        .clipped()
        .overlay(alignment: .leading) {
            if hasEarlier {
                Button(action: loadEarlier) {
                    Text("…")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(palette.labelSecondary)
                        .frame(width: 16, height: 18)
                        .background(palette.bgLayer2,
                                    in: RoundedRectangle(cornerRadius: 4))
                }
                .buttonStyle(.plain)
                .help("Load earlier events")
                .accessibilityLabel("Load earlier events")
            }
        }
    }

    private func barButton(_ bar: TraceTimelineBar, plotWidth: CGFloat) -> some View {
        let width = barWidth(bar, plotWidth: plotWidth)
        return Button {
            selectedID = bar.id
        } label: {
            ZStack {
                Color.clear
                RoundedRectangle(cornerRadius: 2)
                    .fill(traceForeground(bar.record.colorRole, palette))
                    .frame(height: barHeight)
            }
            .frame(width: width, height: laneHeight)
            .opacity(barOpacity(bar))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(bar.record.title.isEmpty ? bar.record.rawKind : bar.record.title)
        .accessibilityLabel(bar.record.title.isEmpty ? bar.record.rawKind : bar.record.title)
    }

    private func barWidth(_ bar: TraceTimelineBar, plotWidth: CGFloat) -> CGFloat {
        max(2, CGFloat(bar.width) * plotWidth)
    }

    private func barX(_ bar: TraceTimelineBar, plotWidth: CGFloat) -> CGFloat {
        min(CGFloat(bar.x) * plotWidth, max(0, plotWidth - barWidth(bar, plotWidth: plotWidth)))
    }

    private func barOpacity(_ bar: TraceTimelineBar) -> Double {
        guard let selectedID else { return 1 }
        return bar.id == selectedID ? 1 : 0.2
    }
}

// MARK: - Ledger

private struct TraceLedgerList: View {
    let ledger: TraceLedger
    let palette: SessionChromePalette
    let query: String
    let loading: Bool
    let loadError: String
    let hasEarlier: Bool
    @Binding var collapsedTurns: Set<String>
    @Binding var selectedID: String?
    let loadEarlier: () -> Void

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if !loadError.isEmpty {
                    Label(loadError, systemImage: "exclamationmark.triangle")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.warnLabel)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(palette.warnTertiary)
                }
                if hasEarlier {
                    Button(action: loadEarlier) {
                        Text(loading ? "Loading earlier events…" : "Load earlier events")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(palette.labelSecondary)
                            .frame(maxWidth: .infinity)
                            .frame(height: 26)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(loading)
                    .background(palette.bgLayer1)
                }
                if ledger.isEmpty {
                    emptyState
                } else {
                    ForEach(ledger.turns) { turn in
                        turnSection(turn)
                    }
                }
            }
        }
        .background(palette.bgBase)
    }

    private var emptyState: some View {
        Text(emptyMessage)
            .font(.system(size: 12))
            .foregroundStyle(palette.labelTertiary)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 28)
    }

    private var emptyMessage: String {
        if !trimmedQuery.isEmpty { return "No matching events" }
        return loading ? "Loading trace…" : "No execution events yet"
    }

    private func turnSection(_ turn: TraceTurn) -> some View {
        let collapsed = trimmedQuery.isEmpty && collapsedTurns.contains(turn.id)
        let records = collapsed ? Array(turn.records.prefix(1)) : turn.records
        let firstID = turn.records.first?.id
        return HStack(spacing: 0) {
            Rectangle()
                .fill(palette.turnRail)
                .frame(width: 2)
            VStack(spacing: 0) {
                ForEach(records) { record in
                    TraceRowView(record: record,
                                 turnLabel: turn.shortLabel,
                                 showsTurnLabel: record.id == firstID,
                                 palette: palette,
                                 isSelected: selectedID == record.id) {
                        selectedID = record.id
                    }
                }
            }
        }
    }
}

private struct TraceRowView: View {
    let record: TraceRecord
    let turnLabel: String
    let showsTurnLabel: Bool
    let palette: SessionChromePalette
    let isSelected: Bool
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 0) {
                turnGutter
                TraceKindTag(record: record, palette: palette)
                    .frame(width: SessionChromeMetrics.traceEventColumnWidth,
                           alignment: .trailing)
                    .padding(.trailing, 4)
                Text(record.previewText)
                    .font(.system(size: SessionChromeMetrics.traceContentFontSize))
                    .foregroundStyle(palette.labelPrimary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                trailing
                    .padding(.leading, 8)
                    .layoutPriority(1)
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .frame(height: SessionChromeMetrics.traceRowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(isSelected ? palette.active : (hovered ? palette.hover : Color.clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .overlay(alignment: .bottom) {
            Rectangle().fill(palette.borderL1).frame(height: 1)
        }
        .accessibilityIdentifier("trace-row-\(record.id)")
    }

    private var turnGutter: some View {
        Group {
            if showsTurnLabel {
                Text(turnLabel)
                    .font(.system(size: 8.5, weight: .medium, design: .monospaced))
                    .foregroundStyle(palette.labelTertiary)
                    .lineLimit(1)
                    .padding(.horizontal, 3)
                    .frame(height: 14)
                    .background(palette.modulePlatform,
                                in: RoundedRectangle(cornerRadius: 4))
            } else {
                Color.clear
            }
        }
        .frame(width: 42, alignment: .leading)
    }

    private var trailing: some View {
        HStack(spacing: 8) {
            if !record.startTimeText.isEmpty {
                Text(record.startTimeText)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.labelTertiary)
            }
            if let duration = record.durationText {
                Text(duration)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.labelTertiary)
            }
            if let status = record.statusText {
                Text(status)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(palette.labelTertiary)
            }
        }
        .fixedSize(horizontal: true, vertical: false)
    }
}

private struct TraceKindTag: View {
    let record: TraceRecord
    let palette: SessionChromePalette

    private var role: TraceColorRole { record.isError ? .error : record.kind.colorRole }

    var body: some View {
        Text(record.kind.tagLabel)
            .font(.system(size: SessionChromeMetrics.traceKindTagFontSize, weight: .semibold))
            .foregroundStyle(traceForeground(role, palette))
            .lineLimit(1)
            .padding(.horizontal, 6)
            .frame(height: SessionChromeMetrics.traceKindTagHeight)
            .background(traceBackground(role, palette),
                        in: RoundedRectangle(cornerRadius: SessionChromeMetrics.traceKindTagCornerRadius))
    }
}

// MARK: - Inspector

private struct TraceInspectorContent: View {
    let record: TraceRecord
    let palette: SessionChromePalette
    @State private var copiedAt: UUID?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            infoRow("Status", value: record.status.isEmpty ? "unknown" : record.status,
                    valueColor: statusColor)
            infoRow("Started", value: record.startTimeText.isEmpty ? "Unavailable" : record.startTimeText,
                    valueColor: palette.labelPrimary)
            infoRow("Duration", value: record.durationText ?? "Unavailable",
                    valueColor: palette.labelPrimary)
            if record.detail.isEmpty {
                Text("No detail recorded")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.labelTertiary)
                    .padding(.top, 2)
            } else {
                HStack {
                    Text("Detail")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(palette.labelTertiary)
                    Spacer(minLength: 8)
                    Button {
                        copyToPasteboard(record.detail)
                        copiedAt = UUID()
                    } label: {
                        Label(copiedAt == nil ? "Copy" : "Copied",
                              systemImage: copiedAt == nil ? "doc.on.doc" : "checkmark")
                            .labelStyle(.iconOnly)
                            .font(.system(size: 11))
                            .frame(width: 22, height: 22)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(palette.labelSecondary)
                    .help(copiedAt == nil ? "Copy detail" : "Copied")
                    .accessibilityLabel("Copy detail")
                }
                Text(record.detail)
                    .font(.system(size: SessionChromeMetrics.traceDetailFontSize, design: .monospaced))
                    .foregroundStyle(palette.labelPrimary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
                    .background(palette.codeBlock, in: RoundedRectangle(cornerRadius: 6))
            }
        }
        .task(id: copiedAt) {
            guard copiedAt != nil else { return }
            do { try await Task.sleep(for: .seconds(2)) } catch { return }
            copiedAt = nil
        }
    }

    private var statusColor: Color {
        if record.isError { return palette.errorPrimary }
        if record.isRunning { return palette.successPrimary }
        return palette.labelSecondary
    }

    private func infoRow(_ label: String, value: String, valueColor: Color) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(palette.labelTertiary)
                .frame(width: 56, alignment: .leading)
            Text(value)
                .font(.system(size: 11))
                .foregroundStyle(valueColor)
                .textSelection(.enabled)
            Spacer(minLength: 0)
        }
    }
}

#if os(macOS)
private struct TraceInspectorPanel: View {
    let record: TraceRecord
    let palette: SessionChromePalette
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                TraceKindTag(record: record, palette: palette)
                Text(record.title.isEmpty ? "Event" : record.title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(palette.labelPrimary)
                    .lineLimit(1)
                Spacer(minLength: 6)
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(palette.labelSecondary)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Close inspector")
                .accessibilityLabel("Close inspector")
            }
            .padding(.horizontal, 10)
            .frame(height: SessionChromeMetrics.traceHeaderHeight)
            .overlay(alignment: .bottom) {
                Rectangle().fill(palette.borderL2).frame(height: 0.5)
            }
            ScrollView {
                TraceInspectorContent(record: record, palette: palette)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(width: SessionChromeMetrics.traceInspectorWidth)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(palette.bgLayer1)
    }
}
#endif

#if os(iOS)
private struct TraceInspectorSheet: View {
    let record: TraceRecord
    let palette: SessionChromePalette
    let onClose: () -> Void

    var body: some View {
        NavigationStack {
            ScrollView {
                TraceInspectorContent(record: record, palette: palette)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(palette.bgBase)
            .navigationTitle(record.title.isEmpty ? "Event" : record.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close", action: onClose)
                }
            }
        }
    }
}
#endif

// MARK: - Role colors

private func traceForeground(_ role: TraceColorRole, _ palette: SessionChromePalette) -> Color {
    switch role {
    case .business: return palette.businessPrimary
    case .warn: return palette.warnLabel
    case .reasoning: return palette.reasoning
    case .secondary: return palette.labelSecondary
    case .error: return palette.errorPrimary
    }
}

private func traceBackground(_ role: TraceColorRole, _ palette: SessionChromePalette) -> Color {
    switch role {
    case .business: return palette.businessTertiary
    case .warn: return palette.warnTertiary
    case .reasoning: return palette.reasoningTertiary
    case .secondary: return palette.modulePlatform
    case .error: return palette.errorTertiary
    }
}

#endif
