import XCTest
import SwiftUI
import MusterCore
@testable import MusterUI

@MainActor
final class RecycledConversationTests: XCTestCase {
    private func chat(_ range: Range<Int>) -> Muster_App_ChatView {
        var chat = Muster_App_ChatView(); chat.session.id = "collection-fixture"
        chat.timeline = range.map { index in
            var block = Muster_App_ChatBlock()
            block.id = "row-\(index)"; block.kind = "message"
            block.message.id = block.id; block.message.kind = "text"; block.message.role = "assistant"
            block.message.text = "Message \(index)\n\nA synthetic conversation row with **Markdown** and `code`."
            return block
        }
        chat.messageCount = UInt32(chat.timeline.count); return chat
    }
    private func update(_ controller: ConversationCollectionController, _ chat: Muster_App_ChatView, jump: Int = 0, footer: AnyView = AnyView(EmptyView())) {
        controller.update(chat: chat, showsInteractions: true, spacing: 14, columnWidth: 760,
                          appearance: "fixture", jump: jump, header: AnyView(Text("Earlier history")),
                          interactions: footer, content: { AnyView(ConversationBlock(block: $0)) })
        settle()
    }
    private func show(_ controller: UIViewController, width: CGFloat = 390) throws -> UIWindow {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: width, height: 740)
        window.rootViewController = controller; window.makeKeyAndVisible(); settle(); return window
    }
    private func settle() { RunLoop.main.run(until: Date().addingTimeInterval(0.4)) }
    private func remaining(_ view: UICollectionView) -> CGFloat { view.contentSize.height - view.bounds.height - view.contentOffset.y }
    private func readAt(_ controller: ConversationCollectionController, index: Int) {
        let view = controller.collection!
        controller.scrollViewWillBeginDragging(view)
        view.scrollToItem(at: IndexPath(item: index, section: 0), at: .top, animated: false)
        settle(); controller.scrollViewDidEndDragging(view, willDecelerate: false); settle()
    }
    private func position(_ view: UICollectionView, _ index: Int) -> CGFloat {
        (view.layoutAttributesForItem(at: IndexPath(item: index, section: 0))?.frame.minY ?? -9999) - view.contentOffset.y
    }
    func testLongHistoryRecyclesAndPrependKeepsVisibleRow() throws {
        let controller = ConversationCollectionController(); var value = chat(80..<580)
        update(controller, value); let window = try show(controller); defer { window.isHidden = true }
        let view = controller.collection!
        XCTAssertEqual(view.numberOfItems(inSection: 0), 501)
        XCTAssertLessThan(view.visibleCells.count, 25)
        XCTAssertLessThan(abs(remaining(view)), 4)
        readAt(controller, index: 240)
        let anchored = try XCTUnwrap(view.indexPathsForVisibleItems.sorted().first {
            $0.item > 0 && (view.layoutAttributesForItem(at: $0)?.frame.maxY ?? 0) > view.contentOffset.y
        }).item
        let before = position(view, anchored)
        value = chat(0..<580); update(controller, value)
        XCTAssertEqual(position(view, anchored + 80), before, accuracy: 3, "Prepend preserves row ID and pixel offset")
        XCTAssertLessThan(view.visibleCells.count, 25)
        value.timeline[value.timeline.count - 1].message.text += String(repeating: "\nStreaming output.", count: 40)
        update(controller, value)
        XCTAssertEqual(position(view, anchored + 80), before, accuracy: 3)
        update(controller, value, jump: 1); XCTAssertLessThan(abs(remaining(view)), 4)
        XCTAssertLessThanOrEqual(view.contentSize.width, view.bounds.width + 1)
    }
    func testUnchangedProjectionDoesNotRebuildRowsAndLocalPaginationStopsFollowing() throws {
        let controller = ConversationCollectionController()
        var value = chat(80..<100)
        var builds = 0
        func apply(hold: Int = 0) {
            controller.update(chat: value, showsInteractions: false, spacing: 14, columnWidth: 760,
                              appearance: "fixture", jump: 0, hold: hold,
                              header: AnyView(Text("Earlier")), interactions: AnyView(EmptyView()),
                              content: { block in builds += 1; return AnyView(ConversationBlock(block: block)) })
            settle()
        }
        apply(); let window = try show(controller); defer { window.isHidden = true }
        let initial = builds
        apply(); XCTAssertEqual(builds, initial, "Polling unrelated state must not rebuild transcript cells")
        readAt(controller, index: 0)
        let before = position(controller.collection, 1)
        value = chat(0..<100); apply(hold: 1)
        XCTAssertEqual(position(controller.collection, 81), before, accuracy: 3,
                       "Load earlier must anchor a message rather than the header")
        XCTAssertLessThan(builds, 40, "Revealing history must not instantiate every SwiftUI row")
    }

    func testGrowingMarkdownDoesNotOverlapAndViewportResizeFollowsBottom() throws {
        let controller = ConversationCollectionController(); var value = chat(0..<3)
        update(controller, value); let window = try show(controller, width: 320); defer { window.isHidden = true }
        value.timeline[1].message.text = String(repeating: "- A long Markdown paragraph with **bold**, `inline code` and wrapping text.\n", count: 30)
        update(controller, value)
        let view = controller.collection!
        let middle = try XCTUnwrap(view.layoutAttributesForItem(at: IndexPath(item: 2, section: 0)))
        let last = try XCTUnwrap(view.layoutAttributesForItem(at: IndexPath(item: 3, section: 0)))
        XCTAssertGreaterThan(middle.frame.height, 500)
        XCTAssertGreaterThanOrEqual(last.frame.minY, middle.frame.maxY - 1)
        XCTAssertLessThan(abs(remaining(view)), 4)
        window.frame.size = CGSize(width: 402, height: 450); controller.view.frame = window.bounds
        controller.view.setNeedsLayout(); settle(); XCTAssertLessThan(abs(remaining(view)), 4)
        readAt(controller, index: 2); let before = position(view, 2)
        window.frame.size.height = 600; controller.view.frame = window.bounds
        controller.view.setNeedsLayout(); settle(); XCTAssertEqual(position(view, 2), before, accuracy: 3)
    }
    func testInteractionHostSurvivesReuseAndControllerIsReleased() throws {
        weak var released: ConversationCollectionController?
        weak var releasedChoices: TranscriptDisclosures?
        var window: UIWindow?
        try autoreleasepool {
            let controller = ConversationCollectionController(); released = controller; releasedChoices = controller.disclosures
            var value = chat(0..<150)
            var question = Muster_V1_PendingInteraction(); question.id = "question-fixture"
            value.interactions = [question]
            update(controller, value, footer: AnyView(DraftFixture())); window = try show(controller)
            let retained = controller.interactionHost.view!
            let field = try XCTUnwrap(descendants(retained).compactMap { $0 as? UITextField }.first)
            field.text = "unsent answer"; field.sendActions(for: .editingChanged); settle()
            readAt(controller, index: 1)
            XCTAssertTrue(controller.interactionHost.view === retained)
            value.responseError = "Synthetic response error"
            update(controller, value, jump: 1, footer: AnyView(DraftFixture()))
            XCTAssertEqual(descendants(retained).compactMap { $0 as? UITextField }.first?.text, "unsent answer")
            window?.isHidden = true; window?.rootViewController = nil; window = nil
        }
        settle(); XCTAssertNil(releasedChoices); XCTAssertNil(released, "Layout callbacks must not retain a closed session")
    }
    func testDisclosureChoicesSurviveReuseStreamingAndPrepend() throws {
        let controller = ConversationCollectionController(); var value = chat(0..<150)
        var work = Muster_App_ChatBlock(); work.id = "work"; work.kind = "working"
        work.title = "Working"; work.defaultExpanded = true
        work.children = [value.timeline[0]]
        value.timeline[0] = work
        update(controller, value); let window = try show(controller); defer { window.isHidden = true }
        readAt(controller, index: 1)
        func height(_ index: Int = 1) -> CGFloat {
            controller.collection.layoutAttributesForItem(at: IndexPath(item: index, section: 0))!.frame.height
        }
        let expanded = height()
        // The same presentation choice written by the disclosure control.
        controller.disclosures.choice(["work", "working", "work"]).override = false
        settle(); let collapsed = height()
        XCTAssertGreaterThan(expanded, collapsed + 40)
        readAt(controller, index: 130)
        XCTAssertNil(controller.collection.cellForItem(at: IndexPath(item: 1, section: 0)))
        value.timeline[0].children[0].message.text += " Streaming update."
        update(controller, value); readAt(controller, index: 1)
        XCTAssertEqual(height(), collapsed, accuracy: 2, "Explicit collapse survives discarded hosting cells and streaming")
        value.timeline.insert(contentsOf: chat(200..<210).timeline, at: 0)
        update(controller, value); readAt(controller, index: 11)
        XCTAssertEqual(height(11), collapsed, accuracy: 2, "Prepend must preserve choices by ID, not row index")
        controller.disclosures.choice(["work", "working", "work"]).override = true
        value.timeline[10].defaultExpanded = false
        update(controller, value); let chosenExpanded = height(11)
        XCTAssertGreaterThan(chosenExpanded, collapsed + 40, "A changed Rust default must not overwrite the reader's choice")
        readAt(controller, index: 140); readAt(controller, index: 11)
        XCTAssertGreaterThan(height(11), collapsed + 40, "Expanded content survives reuse after Markdown margins settle")
        let other = ConversationCollectionController()
        update(other, value); let otherWindow = try show(other); defer { otherWindow.isHidden = true }
        readAt(other, index: 11)
        XCTAssertEqual(other.collection.layoutAttributesForItem(at: IndexPath(item: 11, section: 0))!.frame.height,
                       collapsed, accuracy: 2, "Another opened session must not inherit choices with identical IDs")
    }

    func testNestedToolsAndAllTranscriptDisclosuresKeepIndependentChoices() throws {
        let controller = ConversationCollectionController(); var value = chat(0..<150)
        let kinds = ["reasoning", "notice", "compact_context", "diff", "summary", "activity"]
        for (index, kind) in kinds.enumerated() {
            value.timeline[index].kind = kind
            value.timeline[index].title = "Synthetic disclosure"
            value.timeline[index].message.text = String(repeating: "Synthetic detail.\n", count: 5)
        }
        var file = Muster_App_ChatDiffFile(); file.id = "file"; file.path = "example.swift"
        var line = Muster_App_ChatDiffLine(); line.text = "Synthetic addition"; line.kind = "add"
        file.lines = Array(repeating: line, count: 6); value.timeline[3].diffs = [file]
        value.timeline[4].message.usage.changesIncomplete = true
        var tool = Muster_App_ChatActivityRow(); tool.id = "shared-tool"; tool.title = "Synthetic tool"
        tool.detail = String(repeating: "Synthetic arguments\n", count: 5)
        value.timeline[5].activityRows = [tool]
        var second = value.timeline[5]; second.id = "other-group"; value.timeline[6] = second
        update(controller, value); let window = try show(controller); defer { window.isHidden = true }
        let controls = [("reasoning", "row-0"), ("notice", "row-1"), ("context", "row-2"),
                        ("diff", "file"), ("changes", "row-4"), ("activity", "row-5")]
        for (index, control) in controls.enumerated() {
            readAt(controller, index: index + 1)
            let path = IndexPath(item: index + 1, section: 0)
            let before = controller.collection.layoutAttributesForItem(at: path)!.frame.height
            controller.disclosures.choice(["row-\(index)", control.0, control.1]).override = true
            settle()
            let changed = controller.collection.layoutAttributesForItem(at: path)!.frame.height
            XCTAssertGreaterThan(changed, before + 10, "Control must use the retained choice: \(control.0)")
            readAt(controller, index: 130); readAt(controller, index: index + 1)
            XCTAssertEqual(controller.collection.layoutAttributesForItem(at: path)!.frame.height, changed, accuracy: 2)
        }
        readAt(controller, index: 6)
        let closed = controller.collection.layoutAttributesForItem(at: IndexPath(item: 6, section: 0))!.frame.height
        controller.disclosures.choice(["row-5", "row-5", "tool", "shared-tool"]).override = true
        settle()
        let opened = controller.collection.layoutAttributesForItem(at: IndexPath(item: 6, section: 0))!.frame.height
        XCTAssertGreaterThan(opened, closed + 40)
        controller.disclosures.choice(["other-group", "activity", "other-group"]).override = true
        readAt(controller, index: 7)
        XCTAssertEqual(controller.collection.layoutAttributesForItem(at: IndexPath(item: 7, section: 0))!.frame.height,
                       closed, accuracy: 2, "Identical tool IDs in different groups must remain independent")
        readAt(controller, index: 130); readAt(controller, index: 6)
        XCTAssertEqual(controller.collection.layoutAttributesForItem(at: IndexPath(item: 6, section: 0))!.frame.height, opened, accuracy: 2)
    }

    func testContentTabsKeepViewportChoicesAndUnsentAnswer() throws {
        weak var released: ConversationCollectionController?
        var window: UIWindow?
        try autoreleasepool {
            let controller = ConversationCollectionController(); released = controller
            var value = chat(0..<150)
            var question = Muster_V1_PendingInteraction(); question.id = "question-fixture"
            value.interactions = [question]
            var work = value.timeline[0]; work.kind = "working"; work.title = "Worked"
            work.defaultExpanded = false; work.children = [chat(200..<201).timeline[0]]
            value.timeline[0] = work
            let host = UIHostingController(rootView: TabFixture(controller: controller, chat: value))
            window = try show(host)
            let field = try XCTUnwrap(descendants(controller.interactionHost.view).compactMap { $0 as? UITextField }.first)
            field.text = "unsent answer"; field.sendActions(for: .editingChanged); settle()
            controller.disclosures.choice(["row-0", "working", "row-0"]).override = true
            readAt(controller, index: 1)
            let offset = controller.collection.contentOffset.y
            let height = controller.collection.layoutAttributesForItem(at: IndexPath(item: 1, section: 0))!.frame.height
            XCTAssertGreaterThan(height, 70)
            for _ in 0..<3 {
                host.rootView = TabFixture(controller: controller, chat: value, visible: false); settle()
                XCTAssertNotNil(controller.parent, "Inactive tab retains hosted view state")
                host.rootView = TabFixture(controller: controller, chat: value); settle()
                XCTAssertNotNil(controller.parent)
                XCTAssertEqual(controller.collection.contentOffset.y, offset, accuracy: 3)
                XCTAssertEqual(controller.collection.layoutAttributesForItem(at: IndexPath(item: 1, section: 0))!.frame.height, height, accuracy: 3)
                XCTAssertEqual(descendants(controller.interactionHost.view).compactMap { $0 as? UITextField }.first?.text, "unsent answer")
            }
            window?.isHidden = true; window?.rootViewController = nil; window = nil
        }
        settle(); XCTAssertNil(released)
    }

    private func descendants(_ view: UIView) -> [UIView] { [view] + view.subviews.flatMap(descendants) }
}
private struct DraftFixture: View {
    @State private var text = ""
    var body: some View { TextField("Answer", text: $text).textFieldStyle(.roundedBorder).padding() }
}

private struct TabFixture: View {
    let controller: ConversationCollectionController
    let chat: Muster_App_ChatView
    var visible = true
    @State private var atLatest = true
    var body: some View {
        ZStack {
            ConversationTabContent(active: visible) {
                CollectionFixture(controller: controller, chat: chat)
            }
            if !visible { Text("Activity") }
        }
    }
}
private struct CollectionFixture: UIViewControllerRepresentable {
    let controller: ConversationCollectionController
    let chat: Muster_App_ChatView
    func makeUIViewController(context: Context) -> ConversationCollectionController { controller }
    func updateUIViewController(_ controller: ConversationCollectionController, context: Context) {
        controller.update(chat: chat, showsInteractions: true, spacing: 14, columnWidth: 760,
            appearance: "fixture", jump: 0, header: AnyView(Text("Earlier history")),
            interactions: AnyView(DraftFixture()), content: { AnyView(ConversationBlock(block: $0)) })
    }
}
