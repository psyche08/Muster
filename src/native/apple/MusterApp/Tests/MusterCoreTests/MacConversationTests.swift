#if os(macOS)
import AppKit
import SwiftUI
import XCTest
import MusterCore
@testable import MusterUI

@MainActor
final class MacConversationTests: XCTestCase {
    func chat(_ range: Range<Int>) -> Muster_App_ChatView {
        var chat = Muster_App_ChatView()
        chat.timeline = range.map { i in
            var b = Muster_App_ChatBlock(); b.id = "row-\(i)"; b.kind = "message"
            b.message.id = b.id; b.message.kind = "text"; b.message.role = "assistant"
            b.message.text = "Message \(i)\n\nSynthetic **Markdown** paragraph with `code`."
            return b
        }
        chat.messageCount = UInt32(chat.timeline.count); return chat
    }
    func settle() { RunLoop.main.run(until: Date().addingTimeInterval(0.3)) }
    func update(_ c: MacConversationController, _ chat: Muster_App_ChatView, jump: Int = 0, hold: Int = 0) {
        c.update(chat: chat, showsInteractions: false, spacing: 14, columnWidth: 760,
            appearance: "test", jump: jump, hold: hold, header: AnyView(Text("Earlier")),
            interactions: AnyView(EmptyView()), content: { AnyView(ConversationBlock(block: $0)) })
        settle()
    }
    func show(_ c: MacConversationController) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 600), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false; w.contentViewController = c; w.setContentSize(NSSize(width: 600, height: 600)); w.orderFront(nil); settle(); return w
    }
    func testRecyclingPrependGrowthResizeAndRelease() throws {
        var c: MacConversationController? = MacConversationController()
        var value = chat(200..<700)
        update(c!, value); let window = show(c!)
        let view = c!.collection!
        XCTAssertEqual(view.numberOfItems(inSection: 0), 501)
        XCTAssertFalse(view.visibleItems().isEmpty)
        XCTAssertLessThan(view.visibleItems().count, 35)
        c!.collection.beginScrolling()
        view.scrollToItems(at: [IndexPath(item: 250, section: 0)], scrollPosition: .top)
        settle(); c!.collection.endScrolling(); settle()
        let first = try XCTUnwrap(view.indexPathsForVisibleItems().sorted().first { $0.item > 0 && (view.layoutAttributesForItem(at: $0)?.frame.maxY ?? 0) > c!.scroll.contentView.bounds.minY })
        func position(_ index: Int) -> CGFloat { view.layoutAttributesForItem(at: IndexPath(item: index, section: 0))!.frame.minY - c!.scroll.contentView.bounds.minY }
        let before = position(first.item)
        value = chat(0..<700); update(c!, value)
        XCTAssertEqual(position(first.item + 200), before, accuracy: 4)
        value.timeline[699].message.text = String(repeating: "Growing **Markdown** text.\n", count: 35)
        update(c!, value)
        XCTAssertEqual(position(first.item + 200), before, accuracy: 4)
        update(c!, value, jump: 1)
        XCTAssertLessThan(abs(view.collectionViewLayout!.collectionViewContentSize.height - c!.scroll.contentView.bounds.height - c!.scroll.contentView.bounds.minY), 5)
        window.setContentSize(NSSize(width: 420, height: 500)); settle()
        XCTAssertLessThanOrEqual(view.collectionViewLayout!.collectionViewContentSize.width, 421)
        window.contentViewController = nil; window.close(); c = nil; settle()
    }
    func testEarlierResponseDuringLiveScrollKeepsOriginalRowInPlace() throws {
        let c = MacConversationController(); var value = chat(200..<260)
        value.hasEarlier_p = true
        var requests = 0; c.onEarlier = { requests += 1 }
        update(c, value); let window = show(c); defer { window.close() }
        let view = c.collection!
        func position(_ index: Int) throws -> CGFloat {
            try XCTUnwrap(view.layoutAttributesForItem(at: IndexPath(item: index, section: 0))).frame.minY
                - c.scroll.contentView.bounds.minY
        }
        c.collection.beginScrolling()
        view.scrollToItems(at: [IndexPath(item: 1, section: 0)], scrollPosition: .top)
        settle()
        NotificationCenter.default.post(name: NSScrollView.didLiveScrollNotification, object: c.scroll)
        XCTAssertEqual(requests, 1)
        let before = try position(1)
        // A cached page may arrive before the wheel debounce or trackpad ends.
        value = chat(0..<260); value.hasEarlier_p = true
        update(c, value, hold: 1)
        XCTAssertTrue(c.collection.scrolling)
        XCTAssertEqual(try position(201), before, accuracy: 4,
            "Loading history must preserve the old row while the gesture is active")
        NotificationCenter.default.post(name: NSScrollView.didLiveScrollNotification, object: c.scroll)
        c.collection.endScrolling(); settle()
        XCTAssertEqual(try position(201), before, accuracy: 4,
            "Gesture end must not adopt a newly inserted row as the reading anchor")
        XCTAssertEqual(requests, 1)
        // The correction must not freeze the reader: another gesture can move.
        c.collection.beginScrolling()
        view.scrollToItems(at: [IndexPath(item: 170, section: 0)], scrollPosition: .top)
        settle()
        let moved = try position(170)
        NotificationCenter.default.post(name: NSScrollView.didLiveScrollNotification, object: c.scroll)
        c.collection.endScrolling(); settle()
        XCTAssertEqual(try position(170), moved, accuracy: 4)
        XCTAssertLessThan(abs(moved), 100)
        XCTAssertGreaterThan(try position(201), before + 1000)
    }
    func testTrackpadHistoryResponsesPreserveAnchorAcrossPages() throws {
        let c = MacConversationController(); var value = chat(400..<460)
        value.hasEarlier_p = true
        var requests = 0; c.onEarlier = { requests += 1 }
        update(c, value); let window = show(c); defer { window.close() }
        func wheel(_ phase: CGScrollPhase, delta: Int32) throws {
            let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel,
                wheelCount: 1, wheel1: delta, wheel2: 0, wheel3: 0))
            cg.setIntegerValueField(.scrollWheelEventScrollPhase, value: Int64(phase.rawValue))
            c.scroll.scrollWheel(with: try XCTUnwrap(NSEvent(cgEvent: cg)))
        }
        func position(_ index: Int) throws -> CGFloat {
            try XCTUnwrap(c.collection.layoutAttributesForItem(at: IndexPath(item: index, section: 0))).frame.minY
                - c.scroll.contentView.bounds.minY
        }
        for page in 1...2 {
            try wheel(.began, delta: 100_000); settle()
            XCTAssertTrue(c.scroll.wheelActive)
            XCTAssertEqual(requests, page)
            let before = try position(1)
            value = chat((400 - page * 200)..<460); value.hasEarlier_p = true
            // Different heights require stable identity anchoring, not an offset
            // based on an estimated height multiplied by the inserted row count.
            for i in 0..<200 {
                value.timeline[i].message.text += String(repeating: "\n\nWrapped **earlier** paragraph.", count: i % 7)
            }
            update(c, value, hold: page)
            XCTAssertEqual(try position(201), before, accuracy: 4)
            try wheel(.ended, delta: 0); settle()
            XCTAssertEqual(try position(201), before, accuracy: 4)
            XCTAssertEqual(requests, page)
            // A subsequent real event must move rather than freeze at the anchor.
            try wheel(.began, delta: -80); try wheel(.changed, delta: -800); settle()
            try wheel(.ended, delta: 0); settle()
            XCTAssertLessThan(try position(201), before - 20)
        }
    }
    func testDisclosuresResizeAndSurviveReuseAndHistory() throws {
        let c = MacConversationController(); var value = chat(0..<150)
        var work = Muster_App_ChatBlock(); work.id = "work"; work.kind = "working"
        work.title = "Working"; work.defaultExpanded = true; work.children = [value.timeline[0]]
        value.timeline[0] = work
        update(c, value); let window = show(c); defer { window.close() }
        func read(_ index: Int) {
            c.collection.beginScrolling()
            c.collection.scrollToItems(at: [IndexPath(item: index, section: 0)], scrollPosition: .top)
            settle(); c.collection.endScrolling(); settle()
        }
        func height(_ index: Int) -> CGFloat { c.collection.layoutAttributesForItem(at: IndexPath(item: index, section: 0))!.frame.height }
        read(1); let expanded = height(1)
        c.disclosures.choice(["work", "working", "work"]).override = false
        settle(); let collapsed = height(1)
        XCTAssertGreaterThan(expanded, collapsed + 30)
        read(130); XCTAssertFalse(c.collection.indexPathsForVisibleItems().contains(IndexPath(item: 1, section: 0)))
        value.timeline[0].children[0].message.text += " Streaming update."
        update(c, value); read(1)
        XCTAssertEqual(height(1), collapsed, accuracy: 3)
        value.timeline.insert(contentsOf: chat(200..<210).timeline, at: 0)
        update(c, value); read(11)
        XCTAssertEqual(height(11), collapsed, accuracy: 3)
        c.disclosures.choice(["work", "working", "work"]).override = true
        settle(); XCTAssertGreaterThan(height(11), collapsed + 30)
        let row = c.collection.layoutAttributesForItem(at: IndexPath(item: 11, section: 0))!
        let next = c.collection.layoutAttributesForItem(at: IndexPath(item: 12, section: 0))!
        XCTAssertGreaterThanOrEqual(next.frame.minY, row.frame.maxY - 1)
        read(140); read(11); XCTAssertGreaterThan(height(11), collapsed + 30)
    }
    func testTabsAndRecyclingRetainUnsentAnswer() throws {
        let c = MacConversationController(); var value = chat(0..<150)
        var question = Muster_V1_PendingInteraction(); question.id = "synthetic-question"
        value.interactions = [question]
        let host = NSHostingController(rootView: MacTabFixture(controller: c, chat: value))
        let window = NSWindow(contentViewController: host)
        window.isReleasedWhenClosed = false
        window.setContentSize(NSSize(width: 600, height: 600)); window.orderFront(nil); settle()
        defer { window.contentViewController = nil; window.close() }
        func fields(_ view: NSView) -> [NSTextField] {
            (view as? NSTextField).map { [$0] } ?? view.subviews.flatMap(fields)
        }
        let field = try XCTUnwrap(fields(c.interactionHost).first)
        field.stringValue = "Unsent synthetic answer"
        field.delegate?.controlTextDidChange?(Notification(name: NSControl.textDidChangeNotification, object: field))
        settle()
        c.collection.beginScrolling()
        c.collection.scrollToItems(at: [IndexPath(item: 1, section: 0)], scrollPosition: .top)
        settle(); c.collection.endScrolling(); settle()
        let offset = c.scroll.contentView.bounds.minY
        for _ in 0..<3 {
            host.rootView = MacTabFixture(controller: c, chat: value, visible: false); settle()
            XCTAssertTrue(c.view.window === window, "Inactive tabs keep the native view mounted")
            host.rootView = MacTabFixture(controller: c, chat: value); settle()
            XCTAssertEqual(c.scroll.contentView.bounds.minY, offset, accuracy: 3)
        }
        c.collection.beginScrolling()
        c.collection.scrollToItems(at: [IndexPath(item: 151, section: 0)], scrollPosition: .bottom)
        settle(); c.collection.endScrolling(); settle()
        XCTAssertEqual(fields(c.interactionHost).first?.stringValue, "Unsent synthetic answer")
    }
    func testMultilineRowsFitTheirRenderedColumn() throws {
        let c = MacConversationController(); var value = chat(0..<8)
        for i in value.timeline.indices {
            value.timeline[i].message.text = String(repeating:
                "A long **Markdown** paragraph with several words that must wrap to the displayed column. 中文正文也必须按照实际宽度换行。\n\n", count: 4)
        }
        value.timeline[3].kind = "provider_card"
        value.timeline[3].title = "Next step"
        value.timeline[3].message.text += "\n- First **nested** list item with wrapping text.\n- Second item with `code`."
        update(c, value); let window = show(c); defer { window.close() }
        for width in [600.0, 420.0, 1000.0] {
            window.setContentSize(NSSize(width: width, height: 600)); settle()
            c.collection.beginScrolling()
            c.collection.scrollToItems(at: [IndexPath(item: 4, section: 0)], scrollPosition: .top)
            settle(); c.collection.endScrolling(); settle()
            let path = IndexPath(item: 4, section: 0)
            let attributes = try XCTUnwrap(c.collection.layoutAttributesForItem(at: path))
            let reference = NSHostingView(rootView: ConversationBlock(block: value.timeline[3])
                .environment(\.displayScale, window.backingScaleFactor)
                .padding(.bottom, 14).frame(width: attributes.size.width)
                .fixedSize(horizontal: false, vertical: true))
            // Markdown resolves margin preferences after its first mounted layout.
            reference.frame = NSRect(x: -2000, y: 0, width: attributes.size.width, height: 1000)
            window.contentView?.addSubview(reference); settle()
            let expected = reference.fittingSize.height
            reference.removeFromSuperview()
            XCTAssertGreaterThan(expected, 150)
            XCTAssertEqual(attributes.size.height, expected, accuracy: 3,
                "Row height must contain the actual wrapped content at width \(width)")
        }
    }
    func testPhaseLessWheelReleasesLatestAndLoadsEarlier() throws {
        let c = MacConversationController(); var value = chat(0..<40); value.hasEarlier_p = true
        var requests = 0; c.onEarlier = { requests += 1 }
        update(c, value); let window = show(c); defer { window.close() }
        let bottom = c.scroll.contentView.bounds.minY
        XCTAssertGreaterThan(bottom, 300)
        let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel,
            wheelCount: 1, wheel1: 100_000, wheel2: 0, wheel3: 0))
        let event = try XCTUnwrap(NSEvent(cgEvent: cg))
        XCTAssertTrue(event.phase.isEmpty)
        for _ in 0..<3 { c.scroll.scrollWheel(with: event) }
        settle()
        XCTAssertFalse(c.scroll.wheelActive)
        XCTAssertFalse(c.collection.scrolling)
        XCTAssertLessThan(c.scroll.contentView.bounds.minY, bottom - 100,
            "A mouse wheel must release latest following before scrolling")
        XCTAssertEqual(requests, 1, "Reaching the top by wheel must request earlier history")
        let top = c.scroll.contentView.bounds.minY
        update(c, value)
        XCTAssertEqual(c.scroll.contentView.bounds.minY, top, accuracy: 3)
        XCTAssertEqual(requests, 1, "Projection updates must not fetch more history")
        value.loading = true; update(c, value)
        c.scroll.scrollWheel(with: event); settle()
        XCTAssertEqual(requests, 1)
        value.loading = false; value.loadError = "Synthetic offline"; update(c, value)
        c.scroll.scrollWheel(with: event); settle()
        XCTAssertEqual(requests, 1)
        value.loadError = ""; update(c, value)
        c.scroll.scrollWheel(with: event); settle()
        XCTAssertEqual(requests, 2, "A new gesture may request the next page")
    }
    func testTrackpadPauseDoesNotRearmHistoryGesture() throws {
        let c = MacConversationController(); var value = chat(0..<10); value.hasEarlier_p = true
        var requests = 0; c.onEarlier = { requests += 1 }
        update(c, value); let window = show(c); defer { window.close() }
        func wheel(_ phase: CGScrollPhase) throws {
            let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel,
                wheelCount: 1, wheel1: 100_000, wheel2: 0, wheel3: 0))
            cg.setIntegerValueField(.scrollWheelEventScrollPhase, value: Int64(phase.rawValue))
            let event = try XCTUnwrap(NSEvent(cgEvent: cg))
            XCTAssertFalse(event.phase.isEmpty)
            c.scroll.scrollWheel(with: event)
        }
        try wheel(.began); settle()
        XCTAssertTrue(c.scroll.wheelActive, "A paused trackpad remains the same gesture")
        XCTAssertEqual(requests, 1)
        try wheel(.changed); settle()
        XCTAssertEqual(requests, 1)
        try wheel(.ended); settle()
        XCTAssertFalse(c.scroll.wheelActive)
        try wheel(.began); try wheel(.ended); settle()
        XCTAssertEqual(requests, 2)
    }
    func testControllerAndDisclosureStoreRelease() {
        weak var controller: MacConversationController?
        weak var disclosures: TranscriptDisclosures?
        autoreleasepool {
            let c = MacConversationController(); controller = c; disclosures = c.disclosures
            update(c, chat(0..<10)); let window = show(c)
            window.contentViewController = nil; window.contentView = nil; window.close()
        }
        settle(); XCTAssertNil(controller); XCTAssertNil(disclosures)
    }
    func testHistoryTriggerRequiresGestureAndStopsAfterOneRequest() {
        var gate = HistoryScrollTrigger()
        XCTAssertFalse(gate.request(offset: 0, hasEarlier: true, loading: false, failed: false))
        gate.beginGesture()
        XCTAssertFalse(gate.request(offset: 200, hasEarlier: true, loading: false, failed: false))
        XCTAssertTrue(gate.request(offset: 100, hasEarlier: true, loading: false, failed: false))
        XCTAssertFalse(gate.request(offset: 0, hasEarlier: true, loading: false, failed: false))
        gate.beginGesture()
        XCTAssertFalse(gate.request(offset: 0, hasEarlier: true, loading: true, failed: false))
        XCTAssertFalse(gate.request(offset: 0, hasEarlier: true, loading: false, failed: true))
        XCTAssertFalse(gate.request(offset: 0, hasEarlier: false, loading: false, failed: false))
        XCTAssertTrue(gate.request(offset: 0, hasEarlier: true, loading: false, failed: false))
    }
    func testAutomaticEarlierDoesNotRunOnProjectionOrTabRestoration() {
        let c = MacConversationController(); var value = chat(0..<5); value.hasEarlier_p = true
        var requests = 0; c.onEarlier = { requests += 1 }
        update(c, value); let window = show(c); defer { window.close() }
        XCTAssertEqual(requests, 0)
        NotificationCenter.default.post(name: NSScrollView.willStartLiveScrollNotification, object: c.scroll)
        c.scroll.contentView.scroll(to: .zero); NotificationCenter.default.post(name: NSScrollView.didEndLiveScrollNotification, object: c.scroll)
        XCTAssertEqual(requests, 1)
        update(c, value); NotificationCenter.default.post(name: NSScrollView.didEndLiveScrollNotification, object: c.scroll)
        XCTAssertEqual(requests, 1)
        value.loadError = "Synthetic offline error"; update(c, value)
        NotificationCenter.default.post(name: NSScrollView.willStartLiveScrollNotification, object: c.scroll); NotificationCenter.default.post(name: NSScrollView.didEndLiveScrollNotification, object: c.scroll)
        XCTAssertEqual(requests, 1)
    }
}
private struct MacDraftFixture: View {
    @State private var text = ""
    var body: some View { TextField("Answer", text: $text).textFieldStyle(.roundedBorder).padding() }
}
private struct MacTabFixture: View {
    let controller: MacConversationController
    let chat: Muster_App_ChatView
    var visible = true
    var body: some View {
        ZStack {
            ConversationTabContent(active: visible) { MacCollectionFixture(controller: controller, chat: chat) }
            if !visible { Text("Activity") }
        }
    }
}
private struct MacCollectionFixture: NSViewControllerRepresentable {
    let controller: MacConversationController
    let chat: Muster_App_ChatView
    func makeNSViewController(context: Context) -> MacConversationController { controller }
    func updateNSViewController(_ controller: MacConversationController, context: Context) {
        controller.update(chat: chat, showsInteractions: true, spacing: 14, columnWidth: 760,
            appearance: "test", jump: 0, header: AnyView(Text("Earlier")),
            interactions: AnyView(MacDraftFixture()), content: { AnyView(ConversationBlock(block: $0)) })
    }
}
#endif
