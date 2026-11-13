// Pure-model tests for the DSH-aligned trace page (ADR 0083). Foundation +
// MusterCore/MusterUI only: no SwiftUI, so these run on Linux CI.
import XCTest
import MusterCore
@testable import MusterUI

final class TraceModelTests: XCTestCase {
    // MARK: - Fixtures

    private func span(_ id: String,
                      kind: String,
                      title: String,
                      detail: String = "",
                      status: String = "completed",
                      started: UInt64 = 0,
                      ended: UInt64 = 0,
                      durationKnown: Bool = false,
                      durationSeconds: UInt64 = 0) -> Muster_App_TraceSpan {
        var span = Muster_App_TraceSpan()
        span.id = id
        span.kind = kind
        span.title = title
        span.detail = detail
        span.status = status
        span.startedUnix = started
        span.endedUnix = ended
        span.durationKnown = durationKnown
        span.durationSeconds = durationSeconds
        return span
    }

    private func record(_ id: String,
                        kind: TraceKind,
                        rawKind: String? = nil,
                        title: String = "",
                        detail: String = "",
                        status: String = "completed",
                        started: UInt64 = 0,
                        ended: UInt64 = 0,
                        durationKnown: Bool = false,
                        durationSeconds: UInt64 = 0,
                        turnIndex: Int = TraceRecord.beforeFirstTurnIndex,
                        isTurnStart: Bool = false) -> TraceRecord {
        TraceRecord(id: id,
                    kind: kind,
                    rawKind: rawKind ?? kind.rawValue,
                    title: title,
                    detail: detail,
                    status: status,
                    startedUnix: started,
                    endedUnix: ended,
                    durationKnown: durationKnown,
                    durationSeconds: durationSeconds,
                    turnIndex: turnIndex,
                    isTurnStart: isTurnStart)
    }

    // MARK: - Kind classification

    func testKindClassifiesTurnStartCompletionAndUnknownKinds() {
        XCTAssertEqual(TraceKind.classify(kind: "turn", title: "User message"), .user)
        XCTAssertEqual(TraceKind.classify(kind: "turn", title: "Turn completed"), .turn)
        XCTAssertEqual(TraceKind.classify(kind: "tool", title: "Shell"), .tool)
        XCTAssertEqual(TraceKind.classify(kind: "reasoning", title: "Reasoning"), .reasoning)
        XCTAssertEqual(TraceKind.classify(kind: "status", title: "Turn status"), .status)
        XCTAssertEqual(TraceKind.classify(kind: "something-new", title: "Whatever"), .status)

        XCTAssertEqual(TraceKind.user.tagLabel, "User")
        XCTAssertEqual(TraceKind.turn.tagLabel, "Turn")
        XCTAssertEqual(TraceKind.tool.tagLabel, "Tool")
        XCTAssertEqual(TraceKind.reasoning.tagLabel, "Think")
        XCTAssertEqual(TraceKind.status.tagLabel, "Status")

        XCTAssertEqual(TraceKind.user.lane, 0)
        XCTAssertEqual(TraceKind.turn.lane, 0)
        XCTAssertEqual(TraceKind.status.lane, 0)
        XCTAssertEqual(TraceKind.reasoning.lane, 1)
        XCTAssertEqual(TraceKind.tool.lane, 2)

        XCTAssertEqual(TraceKind.user.colorRole, .business)
        XCTAssertEqual(TraceKind.tool.colorRole, .warn)
        XCTAssertEqual(TraceKind.reasoning.colorRole, .reasoning)
        XCTAssertEqual(TraceKind.status.colorRole, .secondary)
        XCTAssertEqual(TraceTimelineModel.laneLabels, ["Turn", "Think", "Tools"])
    }

    // MARK: - Turn grouping

    func testLedgerGroupsUserMessagesIntoNumberedTurns() {
        let ledger = TraceLedger.build(spans: [
            span("u1", kind: "turn", title: "User message", started: 100),
            span("r1", kind: "reasoning", title: "Reasoning"),
            span("t1", kind: "tool", title: "Shell"),
            span("c1", kind: "turn", title: "Turn completed"),
            span("u2", kind: "turn", title: "User message"),
            span("t2", kind: "tool", title: "Read"),
        ])

        XCTAssertEqual(ledger.turns.count, 2)
        XCTAssertEqual(ledger.turnCount, 2)
        XCTAssertEqual(ledger.turns[0].id, "turn-1")
        XCTAssertEqual(ledger.turns[0].number, 1)
        XCTAssertEqual(ledger.turns[0].label, "Turn 1")
        XCTAssertEqual(ledger.turns[0].records.map(\.id), ["u1", "r1", "t1", "c1"])
        XCTAssertEqual(ledger.turns[0].records.map(\.turnIndex), [0, 0, 0, 0])
        XCTAssertEqual(ledger.turns[0].records.map(\.isTurnStart), [true, false, false, false])
        // "Turn completed" stays inside the turn it closes.
        XCTAssertEqual(ledger.turns[0].records.last?.kind, .turn)

        XCTAssertEqual(ledger.turns[1].number, 2)
        XCTAssertEqual(ledger.turns[1].records.map(\.id), ["u2", "t2"])
        XCTAssertEqual(ledger.records.map(\.id), ["u1", "r1", "t1", "c1", "u2", "t2"])
        XCTAssertFalse(ledger.isEmpty)
    }

    func testLedgerKeepsEventsBeforeFirstUserMessageInLeadingSection() {
        let ledger = TraceLedger.build(spans: [
            span("t0", kind: "tool", title: "Shell"),
            span("u1", kind: "turn", title: "User message"),
            span("t1", kind: "tool", title: "Read"),
        ])

        XCTAssertEqual(ledger.turns.count, 2)
        XCTAssertEqual(ledger.turns[0].id, "before-first-turn")
        XCTAssertNil(ledger.turns[0].number)
        XCTAssertEqual(ledger.turns[0].index, TraceRecord.beforeFirstTurnIndex)
        XCTAssertEqual(ledger.turns[0].label, "Before first turn")
        XCTAssertEqual(ledger.turns[0].records.map(\.id), ["t0"])
        XCTAssertEqual(ledger.turns[0].records[0].turnIndex, TraceRecord.beforeFirstTurnIndex)

        XCTAssertEqual(ledger.turns[1].number, 1)
        XCTAssertEqual(ledger.turns[1].records.map(\.id), ["u1", "t1"])
        XCTAssertEqual(ledger.turns[1].records[0].turnIndex, 0)
        XCTAssertEqual(ledger.turnCount, 1)
    }

    func testLedgerWithoutUserMessagesKeepsOneLeadingSection() {
        let ledger = TraceLedger.build(spans: [span("t1", kind: "tool", title: "Shell")])
        XCTAssertEqual(ledger.turns.count, 1)
        XCTAssertNil(ledger.turns[0].number)
        XCTAssertEqual(ledger.turnCount, 0)
        XCTAssertEqual(ledger.turns[0].shortLabel, "Before")
        XCTAssertEqual(ledger.turns[0].label, "Before first turn")
    }

    func testEmptyTraceHasAnEmptyLedger() {
        let ledger = TraceLedger.build(spans: [])
        XCTAssertTrue(ledger.isEmpty)
        XCTAssertEqual(ledger.records.count, 0)
        XCTAssertEqual(ledger.turnCount, 0)
    }

    // MARK: - Search

    func testSearchMatchesTitleDetailKindAndStatus() {
        let tool = TraceRecord(span: span("t1", kind: "tool", title: "Shell",
                                          detail: "git status", status: "failed"))
        XCTAssertTrue(tool.matches(search: "shell"))
        XCTAssertTrue(tool.matches(search: "GIT"))
        XCTAssertTrue(tool.matches(search: "tool"))
        XCTAssertTrue(tool.matches(search: "failed"))
        XCTAssertTrue(tool.matches(search: "   "))
        XCTAssertFalse(tool.matches(search: "reasoning"))
        XCTAssertFalse(tool.matches(search: "zzz"))
    }

    func testFilteringLedgerKeepsMatchingRowsAndDropsEmptyTurns() {
        let ledger = TraceLedger.build(spans: [
            span("u1", kind: "turn", title: "User message"),
            span("t1", kind: "tool", title: "Shell"),
            span("r1", kind: "reasoning", title: "Reasoning"),
            span("u2", kind: "turn", title: "User message"),
            span("t2", kind: "tool", title: "Read"),
        ])

        let shell = ledger.filtering("shell")
        XCTAssertEqual(shell.turns.count, 1)
        XCTAssertEqual(shell.turns[0].number, 1)
        XCTAssertEqual(shell.turns[0].records.map(\.id), ["t1"])

        let read = ledger.filtering("read")
        XCTAssertEqual(read.turns.count, 1)
        XCTAssertEqual(read.turns[0].number, 2)
        XCTAssertEqual(read.turns[0].records.map(\.id), ["t2"])

        let reasoning = ledger.filtering("reasoning")
        XCTAssertEqual(reasoning.turns.count, 1)
        XCTAssertEqual(reasoning.turns[0].records.map(\.id), ["r1"])

        XCTAssertEqual(ledger.filtering(""), ledger)
        XCTAssertTrue(ledger.filtering("zzz").isEmpty)
    }

    // MARK: - Sequence timeline

    func testSequenceTimelineUsesEqualWidthsAndLanes() {
        let model = TraceTimelineModel(records: [
            record("u1", kind: .user, started: 10),
            record("r1", kind: .reasoning),
            record("t1", kind: .tool),
            record("s1", kind: .status),
        ], mode: .sequence)

        XCTAssertEqual(model.mode, .sequence)
        XCTAssertEqual(model.lanes.count, TraceTimelineModel.laneCount)
        XCTAssertEqual(model.lanes[0].map(\.id), ["u1", "s1"])
        XCTAssertEqual(model.lanes[1].map(\.id), ["r1"])
        XCTAssertEqual(model.lanes[2].map(\.id), ["t1"])
        XCTAssertEqual(model.bars.count, 4)

        for bar in model.bars {
            XCTAssertEqual(bar.width, 0.25, accuracy: 1e-9)
        }
        XCTAssertEqual(model.lanes[0][0].x, 0, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[1][0].x, 0.25, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[2][0].x, 0.5, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[0][1].x, 0.75, accuracy: 1e-9)
        XCTAssertTrue(model.hasTiming)
        XCTAssertTrue(model.boundaries.isEmpty)
    }

    func testSequenceTimelineKeepsUntimedRecordsAndDrawsTurnBoundaries() {
        let model = TraceTimelineModel(records: [
            record("t0", kind: .tool, turnIndex: TraceRecord.beforeFirstTurnIndex),
            record("u1", kind: .user, turnIndex: 0, isTurnStart: true),
            record("t1", kind: .tool, turnIndex: 0),
            record("u2", kind: .user, turnIndex: 1, isTurnStart: true),
            record("t2", kind: .tool, turnIndex: 1),
        ], mode: .sequence)

        XCTAssertEqual(model.bars.count, 5)
        XCTAssertFalse(model.hasTiming)
        XCTAssertEqual(model.boundaries.count, 2)
        XCTAssertEqual(model.boundaries[0], 0.2, accuracy: 1e-9)
        XCTAssertEqual(model.boundaries[1], 0.6, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[2].map(\.x), [0, 0.4, 0.8])
        XCTAssertEqual(model.lanes[2].map(\.width), [0.2, 0.2, 0.2])
    }

    func testEmptySequenceTimelineHasNoBars() {
        let model = TraceTimelineModel(records: [], mode: .sequence)
        XCTAssertTrue(model.isEmpty)
        XCTAssertFalse(model.hasTiming)
        XCTAssertTrue(model.bars.isEmpty)
        XCTAssertTrue(model.boundaries.isEmpty)
    }

    // MARK: - Timed timeline

    func testTimedTimelineIsProportionalToStartAndDuration() {
        let model = TraceTimelineModel(records: [
            record("u1", kind: .user, started: 1000),
            record("t1", kind: .tool, started: 1010, ended: 1020, durationKnown: true, durationSeconds: 10),
            record("r1", kind: .reasoning, started: 1020, durationKnown: true, durationSeconds: 10),
            record("t2", kind: .tool, started: 1030, durationKnown: true, durationSeconds: 10),
        ], mode: .timed)

        XCTAssertEqual(model.mode, .timed)
        XCTAssertTrue(model.hasTiming)
        XCTAssertEqual(model.bars.count, 4)
        XCTAssertEqual(model.lanes[0].map(\.id), ["u1"])
        XCTAssertEqual(model.lanes[1].map(\.id), ["r1"])
        XCTAssertEqual(model.lanes[2].map(\.id), ["t1", "t2"])

        let user = model.lanes[0][0]
        XCTAssertEqual(user.x, 0, accuracy: 1e-9)
        XCTAssertEqual(user.width, 0, accuracy: 1e-9)  // start only, no recorded duration

        let tool = model.lanes[2][0]
        XCTAssertEqual(tool.x, 0.25, accuracy: 1e-9)
        XCTAssertEqual(tool.width, 0.25, accuracy: 1e-9)

        let reasoning = model.lanes[1][0]
        XCTAssertEqual(reasoning.x, 0.5, accuracy: 1e-9)
        XCTAssertEqual(reasoning.width, 0.25, accuracy: 1e-9)

        let lastTool = model.lanes[2][1]
        XCTAssertEqual(lastTool.x, 0.75, accuracy: 1e-9)
        XCTAssertEqual(lastTool.width, 0.25, accuracy: 1e-9)
    }

    func testTimedTimelineSkipsRecordsWithoutAStartTime() {
        let records = [
            record("u1", kind: .user, started: 1000),
            record("t1", kind: .tool, started: 1010, durationKnown: true, durationSeconds: 10),
            record("s1", kind: .status, started: 0),
        ]
        let timed = TraceTimelineModel(records: records, mode: .timed)
        XCTAssertNil(timed.bars.first { $0.id == "s1" })
        XCTAssertEqual(timed.bars.count, 2)

        let sequence = TraceTimelineModel(records: records, mode: .sequence)
        XCTAssertNotNil(sequence.bars.first { $0.id == "s1" })
        XCTAssertEqual(sequence.bars.count, 3)
    }

    func testTimedTimelineWithoutTimingReportsNoTiming() {
        let model = TraceTimelineModel(records: [
            record("t1", kind: .tool),
            record("r1", kind: .reasoning),
        ], mode: .timed)

        XCTAssertFalse(model.hasTiming)
        XCTAssertTrue(model.isEmpty)
        XCTAssertTrue(model.bars.isEmpty)
        XCTAssertTrue(model.boundaries.isEmpty)
    }

    func testTimedTimelineDistributesRecordsSharingOneStartSecond() {
        let model = TraceTimelineModel(records: [
            record("u1", kind: .user, started: 500),
            record("t1", kind: .tool, started: 500),
        ], mode: .timed)

        XCTAssertTrue(model.hasTiming)
        XCTAssertEqual(model.bars.count, 2)
        XCTAssertEqual(model.lanes[0][0].x, 0, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[0][0].width, 0.5, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[2][0].x, 0.5, accuracy: 1e-9)
        XCTAssertEqual(model.lanes[2][0].width, 0.5, accuracy: 1e-9)
    }

    func testTimedTimelineDrawsTurnBoundaryAtTheNextTurn() {
        let model = TraceTimelineModel(records: [
            record("u1", kind: .user, started: 10, turnIndex: 0, isTurnStart: true),
            record("t1", kind: .tool, started: 20, durationKnown: true, durationSeconds: 10, turnIndex: 0),
            record("u2", kind: .user, started: 40, turnIndex: 1, isTurnStart: true),
            record("t2", kind: .tool, started: 50, durationKnown: true, durationSeconds: 10, turnIndex: 1),
        ], mode: .timed)

        XCTAssertEqual(model.boundaries.count, 1)
        XCTAssertEqual(model.boundaries[0], 0.6, accuracy: 1e-9)
    }

    // MARK: - Record flags and display text

    func testRecordFlagsFailuresRunsAndUnknownDurations() {
        let failed = TraceRecord(span: span("t1", kind: "tool", title: "Shell", status: "failed"))
        XCTAssertTrue(failed.isError)
        XCTAssertFalse(failed.isRunning)
        XCTAssertEqual(failed.colorRole, .error)
        XCTAssertEqual(failed.statusText, "failed")
        XCTAssertNil(failed.durationText)  // never fabricates a duration

        let running = TraceRecord(span: span("t2", kind: "tool", title: "Read", status: "running",
                                             started: 120))
        XCTAssertFalse(running.isError)
        XCTAssertTrue(running.isRunning)
        XCTAssertTrue(running.hasTiming)
        XCTAssertEqual(running.statusText, "running")
        XCTAssertNil(running.durationText)
        XCTAssertEqual(running.startTimeText, TraceFormat.startTime(120))

        let interrupted = TraceRecord(span: span("t3", kind: "tool", title: "Read", status: "interrupted"))
        XCTAssertFalse(interrupted.isError)
        XCTAssertEqual(interrupted.statusText, "interrupted")

        let finished = TraceRecord(span: span("t4", kind: "tool", title: "Read", status: "completed",
                                              durationKnown: true, durationSeconds: 12))
        XCTAssertNil(finished.statusText)
        XCTAssertEqual(finished.durationText, "12.0 s")
        XCTAssertFalse(finished.hasTiming)
    }

    func testPreviewTextIsOneLineBuiltFromTitleAndFirstDetailLine() {
        let withDetail = TraceRecord(span: span("t1", kind: "tool", title: "Shell",
                                                detail: "git status\ngit diff"))
        XCTAssertEqual(withDetail.detailPreview, "git status")
        XCTAssertEqual(withDetail.previewText, "Shell · git status")
        let reasoning = TraceRecord(span: span("r", kind: "reasoning", title: "Reasoning",
                                               detail: "Checking the fixtures."))
        XCTAssertEqual(reasoning.previewText, "Checking the fixtures.")

        let described = TraceRecord(span: span("call", kind: "tool", title: "shell · git status", detail: "git status\nclean"))
        XCTAssertEqual(described.previewText, "shell · git status")

        let titleOnly = TraceRecord(span: span("t2", kind: "tool", title: "Shell", detail: "\n  \n"))
        XCTAssertEqual(titleOnly.previewText, "Shell")

        let detailOnly = TraceRecord(span: span("t3", kind: "tool", title: "",
                                                detail: "\n  \nhello\nworld"))
        XCTAssertEqual(detailOnly.detailPreview, "hello")
        XCTAssertEqual(detailOnly.previewText, "hello")
    }

    // MARK: - Formatting

    func testDurationFormattingUsesMillisecondsBelowOneSecond() {
        XCTAssertEqual(TraceFormat.durationText(seconds: 0.85), "850 ms")
        XCTAssertEqual(TraceFormat.durationText(seconds: 0), "0 ms")
        XCTAssertEqual(TraceFormat.durationText(seconds: 12.34), "12.3 s")
        XCTAssertEqual(TraceFormat.durationText(seconds: 12), "12.0 s")
        XCTAssertEqual(TraceFormat.durationText(seconds: 65), "1:05")
        XCTAssertEqual(TraceFormat.durationText(seconds: 3723), "1:02:03")
        XCTAssertEqual(TraceFormat.durationText(seconds: -1), "—")
    }

    func testStartTimeFormattingUsesTwentyFourHourClock() {
        let utc = TimeZone(identifier: "UTC")!
        XCTAssertEqual(TraceFormat.startTime(0, timeZone: utc), "")
        XCTAssertEqual(TraceFormat.startTime(1_700_000_000, timeZone: utc), "22:13:20")
        XCTAssertEqual(TraceFormat.startTime(1_700_000_005, timeZone: utc), "22:13:25")
    }

    func testTimelineModeLabelsAndToggle() {
        XCTAssertEqual(TraceTimelineMode.sequence.label, "Equal width")
        XCTAssertEqual(TraceTimelineMode.timed.label, "Actual time")
        XCTAssertEqual(TraceTimelineMode.sequence.toggled, .timed)
        XCTAssertEqual(TraceTimelineMode.timed.toggled, .sequence)
        XCTAssertEqual(TraceTimelineMode.allCases.count, 2)
    }
}
