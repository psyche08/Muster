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
        readAt(controller, index: 240); let before = position(view, 240)
        value = chat(0..<580); update(controller, value)
        XCTAssertEqual(position(view, 320), before, accuracy: 3, "Prepend preserves row ID and pixel offset")
        XCTAssertLessThan(view.visibleCells.count, 25)
        value.timeline[value.timeline.count - 1].message.text += String(repeating: "\nStreaming output.", count: 40)
        update(controller, value)
        XCTAssertEqual(position(view, 320), before, accuracy: 3)
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
        var window: UIWindow?
        try autoreleasepool {
            let controller = ConversationCollectionController(); released = controller
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
        settle(); XCTAssertNil(released, "Layout callbacks must not retain a closed session")
    }
    private func descendants(_ view: UIView) -> [UIView] { [view] + view.subviews.flatMap(descendants) }
}
private struct DraftFixture: View {
    @State private var text = ""
    var body: some View { TextField("Answer", text: $text).textFieldStyle(.roundedBorder).padding() }
}
