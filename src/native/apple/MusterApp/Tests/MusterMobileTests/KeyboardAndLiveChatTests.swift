import XCTest
import SwiftUI
import MusterCore
@testable import MusterUI

final class KeyboardAndLiveChatTests: XCTestCase {
    @MainActor
    func testBuiltInKeyboardWaitsForWindowAndCanSwitchWhileFocused() throws {
        var draft = ""
        let text = Binding(get: { draft }, set: { draft = $0 })
        func editor(_ builtIn: Bool) -> some View {
            PasteTextEditor(text: text, disabled: false, fontSize: 16, minimumHeight: 36,
                            inAppKeyboard: builtIn, autoFocus: true) { _ in }.padding()
        }
        let host = UIHostingController(rootView: editor(true))
        host.loadViewIfNeeded()
        let window = try show(host)
        defer { window.endEditing(true); window.isHidden = true }
        let field = try XCTUnwrap(descendants(host.view).compactMap { $0 as? PasteTextView }.first)
        XCTAssertTrue(field.isFirstResponder, "Focus requested before window attachment must not be lost")
        let keyboard = try XCTUnwrap(field.inputView as? KeyboardInputView)
        waitForKeyboard(field)
        XCTAssertNotNil(keyboard.window, "The built-in keyboard must actually be on screen")
        XCTAssertGreaterThan(keyboard.bounds.height, 250)
        XCTAssertGreaterThan(keyboard.bounds.width, 250)
        host.rootView = editor(false)
        settle()
        XCTAssertNil(field.inputView)
        XCTAssertTrue(field.isFirstResponder)
        host.rootView = editor(true)
        waitForKeyboard(field)
        XCTAssertNotNil(field.inputView?.window)
        XCTAssertTrue(field.isFirstResponder)
    }

    @MainActor
    func testLayoutTransitionPreservesKeyboardAndGrowingReplyFollowsLatest() throws {
        let old = KeyboardPrefs.shared.systemKeyboard
        KeyboardPrefs.shared.systemKeyboard = false
        defer { KeyboardPrefs.shared.systemKeyboard = old }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let core = try MusterCore(config: .phone(dir: directory, name: "Live fixture"), keys: SoftwareKeys(),
                                  onViewModel: { _ in }, onEffect: { _, _ in })
        defer { core.shutdown(); try? FileManager.default.removeItem(at: directory) }
        var vm = ViewModel()
        vm.chat.session.id = "live-fixture"
        vm.chat.session.title = "实时回复与内置键盘"
        vm.chat.contentRevision = 1
        vm.chat.messageCount = 1
        var block = Muster_App_ChatBlock()
        block.id = "reply"; block.kind = "message"
        block.message.id = "reply"; block.message.role = "assistant"; block.message.kind = "text"
        block.message.text = "正在读取。"
        vm.chat.timeline = [block]
        var stacked = WorkspaceLayout()
        stacked.flex = true; stacked.readingHeight = 240; stacked.hingeHeight = 16
        func content(_ value: ViewModel, _ layout: WorkspaceLayout) -> some View {
            ChatView(core: core, vm: value).environment(\.workspaceLayout, layout)
        }
        let host = UIHostingController(rootView: content(vm, stacked))
        let window = try show(host)
        defer { window.endEditing(true); window.isHidden = true }
        let field = try XCTUnwrap(descendants(host.view).compactMap { $0 as? PasteTextView }.first)
        XCTAssertTrue(field.isFirstResponder)
        host.rootView = content(vm, WorkspaceLayout())
        settle()
        let current = try XCTUnwrap(descendants(host.view).compactMap { $0 as? PasteTextView }.first)
        XCTAssertTrue(current === field, "Changing available height must not recreate the composer")
        XCTAssertTrue(field.isFirstResponder)
        XCTAssertNotNil(field.inputView?.window)
        vm.chat.timeline[0].message.text = String(repeating: "回复逐步增长，每一行都应显示在输入区上方。\n\n", count: 35)
        vm.chat.contentRevision = 2
        host.rootView = content(vm, WorkspaceLayout())
        settle()
        let scroll = try XCTUnwrap(descendants(host.view).compactMap { $0 as? UIScrollView }
            .first { !($0 is UITextView) && $0.bounds.height > 100 })
        XCTAssertLessThanOrEqual(scroll.contentSize.width, scroll.bounds.width + 1)
        let remaining = scroll.contentSize.height + scroll.adjustedContentInset.bottom - scroll.bounds.height - scroll.contentOffset.y
        XCTAssertLessThan(remaining, 50, "A reply growing under one ID must remain visible at the bottom")
        XCTAssertTrue(field.isFirstResponder)
        let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        let attachment = XCTAttachment(image: image)
        attachment.name = "live-reply-built-in-keyboard"; attachment.lifetime = .keepAlways
        add(attachment)
        if let keyboard = field.inputView {
            let keyboardImage = UIGraphicsImageRenderer(bounds: keyboard.bounds).image { _ in
                keyboard.drawHierarchy(in: keyboard.bounds, afterScreenUpdates: true)
            }
            try keyboardImage.pngData()?.write(to: FileManager.default.temporaryDirectory.appendingPathComponent("muster-keyboard-only.png"))
        }
        try image.pngData()?.write(to: FileManager.default.temporaryDirectory.appendingPathComponent("muster-live-keyboard.png"))
    }

    @MainActor
    func testStackedMacKeyboardRemainsVisibleAfterFocusLossAndAttachment() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let core = try MusterCore(config: .phone(dir: directory, name: "Dock fixture"), keys: SoftwareKeys(),
                                  onViewModel: { _ in }, onEffect: { _, _ in })
        defer { core.shutdown(); try? FileManager.default.removeItem(at: directory) }
        var vm = ViewModel()
        vm.chat.session.id = "dock-fixture"
        vm.chat.attachmentCountLimit = 3; vm.chat.attachmentBytesLimit = 3_000_000
        vm.chat.contentRevision = 1; vm.chat.messageCount = 1
        var block = Muster_App_ChatBlock()
        block.id = "reply"; block.kind = "message"
        block.message.id = "reply"; block.message.role = "assistant"; block.message.kind = "text"
        block.message.text = "上方查看会话，下方的 Mac 键盘始终可见。"
        vm.chat.timeline = [block]
        let layout = workspaceLayout(area: WorkspaceArea(width: 393, height: 700),
                                     fold: FoldGeometry(), preference: .always)
        let host = UIHostingController(rootView: ChatView(core: core, vm: vm).environment(\.workspaceLayout, layout))
        let window = try show(host)
        defer { window.endEditing(true); window.isHidden = true }
        let field = try XCTUnwrap(descendants(host.view).compactMap { $0 as? PasteTextView }.first)
        XCTAssertEqual(field.inputView?.bounds.height, 0, "The stacked keyboard belongs to the page, not UIKit's dismissible input window")
        field.resignFirstResponder()
        let photo = UIGraphicsImageRenderer(size: CGSize(width: 80, height: 80)).image { context in
            UIColor.orange.setFill(); context.fill(CGRect(x: 0, y: 0, width: 80, height: 80))
        }
        field.images?([try XCTUnwrap(photo.pngData())])
        settle()
        vm.chat.sending = true
        host.rootView = ChatView(core: core, vm: vm).environment(\.workspaceLayout, layout)
        settle()
        vm.chat.sending = false
        host.rootView = ChatView(core: core, vm: vm).environment(\.workspaceLayout, layout)
        settle()
        XCTAssertEqual(field.inputView?.bounds.height, 0)
        XCTAssertGreaterThan(field.convert(field.bounds, to: window).minY, 100)
        XCTAssertLessThan(field.convert(field.bounds, to: window).maxY, window.bounds.height - 230)
        let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        let attachment = XCTAttachment(image: image); attachment.name = "stacked-mac-keyboard-with-attachment"
        attachment.lifetime = .keepAlways; add(attachment)
        try image.pngData()?.write(to: FileManager.default.temporaryDirectory.appendingPathComponent("muster-stacked-mac.png"))
    }

    @MainActor
    func testDockedKeysRestoreFocusAndRespectDisabledInput() throws {
        var draft = ""
        let dock = KeyboardDock()
        let text = Binding(get: { draft }, set: { draft = $0 })
        let host = UIHostingController(rootView: PasteTextEditor(text: text, disabled: false,
            fontSize: 16, minimumHeight: 36, autoFocus: true, dock: dock) { _ in })
        let window = try show(host)
        defer { window.endEditing(true); window.isHidden = true }
        let field = try XCTUnwrap(descendants(host.view).compactMap { $0 as? PasteTextView }.first)
        field.resignFirstResponder()
        dock.target.target().insert("Mac")
        XCTAssertTrue(field.isFirstResponder)
        XCTAssertEqual(draft, "Mac")
        field.isEditable = false
        dock.target.target().insert("ignored")
        dock.target.target().backspace()
        XCTAssertEqual(draft, "Mac")
        field.isEditable = true
        dock.target.target().backspace()
        XCTAssertEqual(draft, "Ma")
    }

    @MainActor
    private func show<V: View>(_ host: UIHostingController<V>) throws -> UIWindow {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.layoutIfNeeded()
        settle()
        return window
    }

    @MainActor
    private func waitForKeyboard(_ field: UITextView) {
        let deadline = Date().addingTimeInterval(3)
        repeat {
            RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        } while field.inputView?.window == nil && Date() < deadline
    }

    @MainActor
    private func settle() { RunLoop.main.run(until: Date().addingTimeInterval(0.7)) }

    @MainActor
    private func descendants(_ view: UIView) -> [UIView] {
        [view] + view.subviews.flatMap(descendants)
    }
}
