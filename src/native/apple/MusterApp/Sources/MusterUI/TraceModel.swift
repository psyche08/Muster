// Presentation-only projection of the Rust-owned execution trace (DSH-style
// session trace page, ADR 0083). Foundation only: no SwiftUI, so the ledger
// grouping, three-lane timeline layout and formatting are unit-testable on
// Linux CI. Nothing here decides trust or reconstructs session state: the
// spans come from `chat.trace`, and missing timing stays missing.
import Foundation
import MusterCore

/// How one trace span is presented: the user message that starts a turn, the
/// turn's completion record, a tool call, reasoning, or an execution status.
enum TraceKind: String, Equatable, CaseIterable {
    case user
    case turn
    case tool
    case reasoning
    case status

    /// The title Rust gives the `turn` span it emits for the user's message.
    static let userMessageTitle = "User message"

    /// Classify a Rust-projected span. `turn` spans split into the user
    /// message that opens a turn and the turn's completion record.
    static func classify(kind: String, title: String) -> TraceKind {
        switch kind {
        case "tool": return .tool
        case "reasoning": return .reasoning
        case "turn": return title == userMessageTitle ? .user : .turn
        case "status": return .status
        default: return .status
        }
    }

    /// The word in the row's kind tag pill.
    var tagLabel: String {
        switch self {
        case .user: return "User"
        case .turn: return "Turn"
        case .tool: return "Tool"
        case .reasoning: return "Think"
        case .status: return "Status"
        }
    }

    /// The palette role the tag and the timeline bar use.
    var colorRole: TraceColorRole {
        switch self {
        case .user, .turn: return .business
        case .tool: return .warn
        case .reasoning: return .reasoning
        case .status: return .secondary
        }
    }

    /// Timeline lane: 0 turns/user/status, 1 reasoning, 2 tools.
    var lane: Int {
        switch self {
        case .user, .turn, .status: return 0
        case .reasoning: return 1
        case .tool: return 2
        }
    }
}

/// Palette roles named by the model. `TraceScreen` maps them onto
/// `SessionChromePalette` (the model itself stays free of SwiftUI).
enum TraceColorRole: Equatable {
    case business
    case warn
    case reasoning
    case secondary
    case error
}

/// Formatting helpers for the trace UI. Foundation only.
enum TraceFormat {
    /// `"850 ms"` below one second, `"12.3 s"` above it, then `"1:05"` and
    /// `"1:02:03"` for long records. `"—"` for values that cannot be shown.
    static func durationText(seconds: Double) -> String {
        guard seconds.isFinite, seconds >= 0 else { return "—" }
        if seconds < 1 {
            return "\(Int((seconds * 1000).rounded())) ms"
        }
        if seconds < 60 {
            return String(format: "%.1f s", seconds)
        }
        let total = Int(seconds.rounded())
        let minutes = total / 60
        let remainder = total % 60
        if minutes < 60 {
            return String(format: "%d:%02d", minutes, remainder)
        }
        return String(format: "%d:%02d:%02d", minutes / 60, minutes % 60, remainder)
    }

    /// `"HH:mm:ss"` in `timeZone`, or `""` when the span has no start time.
    static func startTime(_ unix: UInt64, timeZone: TimeZone = .current) -> String {
        guard unix > 0 else { return "" }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = timeZone
        formatter.dateFormat = "HH:mm:ss"
        return formatter.string(from: Date(timeIntervalSince1970: TimeInterval(unix)))
    }
}

/// One display row in the trace ledger.
struct TraceRecord: Identifiable, Equatable {
    /// The turn index used for spans recorded before the first `User message`.
    static let beforeFirstTurnIndex = -1

    let id: String
    let kind: TraceKind
    /// The protocol kind as received, kept for search and the inspector.
    let rawKind: String
    let title: String
    let detail: String
    let status: String
    let startedUnix: UInt64
    let endedUnix: UInt64
    let durationKnown: Bool
    let durationSeconds: UInt64
    /// Zero-based turn index; `beforeFirstTurnIndex` before the first turn.
    let turnIndex: Int
    /// True only for the `User message` record that opens a turn.
    let isTurnStart: Bool

    init(id: String,
         kind: TraceKind,
         rawKind: String,
         title: String,
         detail: String,
         status: String,
         startedUnix: UInt64,
         endedUnix: UInt64,
         durationKnown: Bool,
         durationSeconds: UInt64,
         turnIndex: Int = beforeFirstTurnIndex,
         isTurnStart: Bool = false) {
        self.id = id
        self.kind = kind
        self.rawKind = rawKind
        self.title = title
        self.detail = detail
        self.status = status
        self.startedUnix = startedUnix
        self.endedUnix = endedUnix
        self.durationKnown = durationKnown
        self.durationSeconds = durationSeconds
        self.turnIndex = turnIndex
        self.isTurnStart = isTurnStart
    }

    init(span: Muster_App_TraceSpan, turnIndex: Int = beforeFirstTurnIndex) {
        let kind = TraceKind.classify(kind: span.kind, title: span.title)
        self.init(id: span.id,
                  kind: kind,
                  rawKind: span.kind,
                  title: span.title,
                  detail: span.detail,
                  status: span.status,
                  startedUnix: span.startedUnix,
                  endedUnix: span.endedUnix,
                  durationKnown: span.durationKnown,
                  durationSeconds: span.durationSeconds,
                  turnIndex: turnIndex,
                  isTurnStart: kind == .user)
    }

    var isError: Bool { status == "failed" }

    var isRunning: Bool { status == "running" }

    /// A start timestamp is the only timing the projection guarantees.
    var hasTiming: Bool { startedUnix > 0 }

    /// The timeline color: failures override the kind's role.
    var colorRole: TraceColorRole { isError ? .error : kind.colorRole }

    /// The recorded duration, or `nil` when Rust marked it unknown. Never
    /// fabricates a duration for a running record.
    var durationText: String? {
        durationKnown ? TraceFormat.durationText(seconds: Double(durationSeconds)) : nil
    }

    /// `"HH:mm:ss"`, or `""` when the start time is unavailable.
    var startTimeText: String { TraceFormat.startTime(startedUnix) }

    /// Statuses worth a trailing word in the row; finished and unknown
    /// records stay quiet.
    var statusText: String? {
        switch status {
        case "running", "failed", "interrupted": return status
        default: return nil
        }
    }

    /// The first non-empty line of the bounded detail, without newlines.
    var detailPreview: String {
        for line in detail.split(whereSeparator: \.isNewline) {
            let text = line.trimmingCharacters(in: .whitespaces)
            if !text.isEmpty { return String(text.prefix(240)) }
        }
        return ""
    }

    /// One single-line string for the ledger's content column.
    var previewText: String {
        let preview = detailPreview
        if title.isEmpty { return preview }
        if preview.isEmpty { return title }
        return "\(title) · \(preview)"
    }

    /// Case-insensitive search over everything the row and inspector show.
    func matches(search rawQuery: String) -> Bool {
        let query = rawQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return true }
        let needle = query.lowercased()
        return title.lowercased().contains(needle)
            || detail.lowercased().contains(needle)
            || kind.tagLabel.lowercased().contains(needle)
            || rawKind.lowercased().contains(needle)
            || status.lowercased().contains(needle)
    }

    /// A copy with the ledger grouping's turn fields filled in.
    func assigning(turnIndex: Int, isTurnStart: Bool) -> TraceRecord {
        TraceRecord(id: id,
                    kind: kind,
                    rawKind: rawKind,
                    title: title,
                    detail: detail,
                    status: status,
                    startedUnix: startedUnix,
                    endedUnix: endedUnix,
                    durationKnown: durationKnown,
                    durationSeconds: durationSeconds,
                    turnIndex: turnIndex,
                    isTurnStart: isTurnStart)
    }
}

/// One turn of the ledger. `index` is `beforeFirstTurnIndex` for the leading
/// section; numbered turns are one-based in `number` and `label`.
struct TraceTurn: Identifiable, Equatable {
    let id: String
    let index: Int
    let number: Int?
    let label: String
    let records: [TraceRecord]

    var isBeforeFirstTurn: Bool { number == nil }

    /// The tiny chip shown on the turn's first row.
    var shortLabel: String {
        guard let number else { return "Before" }
        return "Turn \(number)"
    }
}

/// The ordered, turn-grouped ledger. `build(spans:)` is the only grouping
/// step: `User message` starts a turn and everything until the next one
/// (including `Turn completed`) stays in that turn.
struct TraceLedger: Equatable {
    let turns: [TraceTurn]

    /// All records in trace order; the timeline is laid out from this.
    var records: [TraceRecord] { turns.flatMap(\.records) }

    var isEmpty: Bool { turns.isEmpty }

    /// The number of real turns (the leading section is not counted).
    var turnCount: Int { turns.filter { $0.number != nil }.count }

    init(turns: [TraceTurn]) {
        self.turns = turns
    }

    static func build(spans: [Muster_App_TraceSpan]) -> TraceLedger {
        var records: [TraceRecord] = []
        for span in spans {
            records.append(TraceRecord(span: span))
        }
        return group(records)
    }

    /// Group already-classified records. Records before the first
    /// `User message` form a leading, unnumbered section.
    static func build(records: [TraceRecord]) -> TraceLedger {
        group(records)
    }

    private static func group(_ records: [TraceRecord]) -> TraceLedger {
        var sections: [[TraceRecord]] = []
        var current: [TraceRecord] = []
        var currentTurn = TraceRecord.beforeFirstTurnIndex
        var turnCount = 0
        for record in records {
            if record.kind == .user {
                if !current.isEmpty { sections.append(current) }
                current = []
                currentTurn = turnCount
                turnCount += 1
                current.append(record.assigning(turnIndex: currentTurn, isTurnStart: true))
            } else {
                current.append(record.assigning(turnIndex: currentTurn, isTurnStart: false))
            }
        }
        if !current.isEmpty { sections.append(current) }

        var turns: [TraceTurn] = []
        for section in sections {
            guard let first = section.first else { continue }
            if first.turnIndex == TraceRecord.beforeFirstTurnIndex {
                turns.append(TraceTurn(id: "before-first-turn",
                                       index: TraceRecord.beforeFirstTurnIndex,
                                       number: nil,
                                       label: "Before first turn",
                                       records: section))
            } else {
                let number = first.turnIndex + 1
                turns.append(TraceTurn(id: "turn-\(number)",
                                       index: first.turnIndex,
                                       number: number,
                                       label: "Turn \(number)",
                                       records: section))
            }
        }
        return TraceLedger(turns: turns)
    }

    /// Keep only records matching `query`, keeping their turns (a turn whose
    /// rows all miss disappears). An empty query returns the ledger itself.
    func filtering(_ query: String) -> TraceLedger {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return self }
        let kept = turns.compactMap { turn -> TraceTurn? in
            let records = turn.records.filter { $0.matches(search: trimmed) }
            guard !records.isEmpty else { return nil }
            return TraceTurn(id: turn.id, index: turn.index, number: turn.number, label: turn.label, records: records)
        }
        return TraceLedger(turns: kept)
    }
}

/// How the time overview lays records out.
enum TraceTimelineMode: String, Equatable, CaseIterable, Identifiable {
    /// DSH's default: every record gets an equal-width segment in trace order.
    case sequence
    /// Proportional to `startedUnix` and the recorded duration.
    case timed

    var id: String { rawValue }

    /// The toolbar button's value.
    var label: String {
        switch self {
        case .sequence: return "Equal width"
        case .timed: return "Actual time"
        }
    }

    var toggled: TraceTimelineMode {
        self == .sequence ? .timed : .sequence
    }
}

/// One bar in the timeline overview.
struct TraceTimelineBar: Identifiable, Equatable {
    let record: TraceRecord
    let lane: Int
    /// Leading edge as a fraction of the plot's width, 0...1.
    let x: Double
    /// Width as a fraction of the plot's width, 0...1.
    let width: Double

    var id: String { record.id }
}

/// The pure layout of the three-lane time overview.
struct TraceTimelineModel: Equatable {
    static let laneCount = 3
    /// Lane labels, top to bottom.
    static let laneLabels = ["Turn", "Think", "Tools"]

    let mode: TraceTimelineMode
    let lanes: [[TraceTimelineBar]]
    /// Turn-start positions as fractions of the plot's width.
    let boundaries: [Double]
    /// Whether at least one record carries a start time at all.
    let hasTiming: Bool

    init(records: [TraceRecord], mode: TraceTimelineMode) {
        self.mode = mode
        hasTiming = records.contains { $0.hasTiming }
        var lanes = Array(repeating: [TraceTimelineBar](), count: Self.laneCount)
        var boundaries: [Double] = []
        switch mode {
        case .sequence:
            let count = records.count
            guard count > 0 else { break }
            var entries: [(fraction: Double, turnIndex: Int)] = []
            for (index, record) in records.enumerated() {
                let fraction = Double(index) / Double(count)
                lanes[record.kind.lane].append(TraceTimelineBar(record: record,
                                                               lane: record.kind.lane,
                                                               x: fraction,
                                                               width: 1 / Double(count)))
                entries.append((fraction, record.turnIndex))
            }
            boundaries = Self.boundaryFractions(entries)
        case .timed:
            let timed = records.filter { $0.hasTiming }
            guard let minStart = timed.map(\.startedUnix).min() else { break }
            let maxEnd = timed.reduce(timed.map(\.startedUnix).max() ?? minStart) { result, record in
                let end = record.startedUnix &+ (record.durationKnown ? record.durationSeconds : 0)
                return max(result, end)
            }
            let span = maxEnd > minStart ? Double(maxEnd - minStart) : 0
            guard span > 0 else {
                // Every timed record starts at the same second: keep them
                // visible as an ordered sequence instead of dividing by zero.
                let count = timed.count
                guard count > 0 else { break }
                var entries: [(fraction: Double, turnIndex: Int)] = []
                for (index, record) in timed.enumerated() {
                    let fraction = Double(index) / Double(count)
                    lanes[record.kind.lane].append(TraceTimelineBar(record: record,
                                                                   lane: record.kind.lane,
                                                                   x: fraction,
                                                                   width: 1 / Double(count)))
                    entries.append((fraction, record.turnIndex))
                }
                boundaries = Self.boundaryFractions(entries)
                break
            }
            var entries: [(fraction: Double, turnIndex: Int)] = []
            for record in timed {
                let x = Double(record.startedUnix - minStart) / span
                let duration = record.durationKnown ? Double(record.durationSeconds) : 0
                let width = min(duration / span, 1 - x)
                lanes[record.kind.lane].append(TraceTimelineBar(record: record,
                                                               lane: record.kind.lane,
                                                               x: x,
                                                               width: max(0, width)))
                entries.append((x, record.turnIndex))
            }
            boundaries = Self.boundaryFractions(entries)
        }
        self.lanes = lanes
        self.boundaries = boundaries
    }

    /// All bars in trace order.
    var bars: [TraceTimelineBar] { lanes.flatMap { $0 } }

    var isEmpty: Bool { lanes.allSatisfy(\.isEmpty) }

    private static func boundaryFractions(_ entries: [(fraction: Double, turnIndex: Int)]) -> [Double] {
        var fractions: [Double] = []
        var previousTurn: Int?
        for entry in entries {
            if let previousTurn, previousTurn != entry.turnIndex, entry.fraction > 0 {
                if fractions.last.map({ abs($0 - entry.fraction) > 1e-9 }) ?? true {
                    fractions.append(entry.fraction)
                }
            }
            previousTurn = entry.turnIndex
        }
        return fractions
    }
}
