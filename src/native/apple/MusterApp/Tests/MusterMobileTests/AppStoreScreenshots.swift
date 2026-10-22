import XCTest
import SwiftUI
import SwiftProtobuf
import MusterCore
@testable import MusterUI

/// Offline marketing captures of production views. All content is synthetic;
/// this fixture never connects to a user's devices or sends an agent operation.
final class AppStoreScreenshots: XCTestCase {
    @MainActor
    func testCaptureStoreScreenshots() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let core = try MusterCore(config: .phone(dir: dir, name: "Screenshot fixture"), keys: SoftwareKeys(), onViewModel: { _ in }, onEffect: { _, _ in })
        defer { core.shutdown() }
        var vm = ViewModel()
        vm.agents.deviceID = "*"
        vm.serviceDevices = [member("studio", "Studio Mac"), member("laptop", "MacBook Pro")]
        vm.agents.sessions = [
            session("design", "Polish the project dashboard", "codex", "Studio Mac"),
            session("review", "Review the release checklist", "claude", "MacBook Pro"),
            session("tests", "Explain the test results", "codex", "Studio Mac"),
            session("docs", "Write the setup guide", "claude", "MacBook Pro")
        ]
        vm.chat.session = vm.agents.sessions[0]
        vm.chat.attachmentCountLimit = 4
        vm.chat.timeline = [
            message("user", "user", "Please review the dashboard layout and suggest a clearer way to show project progress."),
            message("assistant", "assistant", "I've reviewed the dashboard. Here are three focused improvements:\n\n**Make progress easy to scan**\nShow the current milestone, completed tasks and next step together.\n\n**Keep the important work visible**\nPlace active projects first and move archived work into a separate view.\n\n**Use consistent status labels**\nUse Ready, In progress and Complete throughout the interface.\n\nI can start with the progress summary when you're ready.")
        ]
        vm.chat.messageCount = 2
        for (device, size, scale) in [("iphone", CGSize(width: 402, height: 874), CGFloat(3)), ("ipad", CGSize(width: 1032, height: 1376), CGFloat(2))] {
            try capture(NavigationStack { AgentsView(core: core, vm: vm) }, name: "\(device)-01-sessions", size: size, scale: scale)
            try capture(NavigationStack { ChatView(core: core, vm: vm) }.environment(\.conversationStyle, .codex), name: "\(device)-02-conversation", size: size, scale: scale)
            try capture(NavigationStack { ChatView(core: core, vm: vm) }.environment(\.conversationStyle, .claude).environment(\.colorScheme, .dark), name: "\(device)-03-dark", size: size, scale: scale, dark: true)
        }
        try capture(
            ZStack {
                Color(red: 0xED / 255.0, green: 0xE6 / 255.0, blue: 0xDA / 255.0).ignoresSafeArea()
                HStack(spacing: 64) {
                    MusterMark(size: 224)
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Muster Agents").font(.system(size: 64, weight: .semibold))
                        Text("Your agents. Across devices.").font(.system(size: 38))
                    }
                    .foregroundStyle(Color(red: 0x2A / 255.0, green: 0x28 / 255.0, blue: 0x25 / 255.0))
                }
            },
            name: "header", size: CGSize(width: 1920, height: 823), scale: 2
        )
    }
    private func member(_ id: String, _ name: String) -> Muster_V1_MemberView {
        var value = Muster_V1_MemberView(); value.deviceID = id; value.name = name; value.kind = "desktop"; value.connected = true; return value
    }
    private func session(_ id: String, _ title: String, _ provider: String, _ device: String) -> Muster_V1_AgentSession {
        var value = Muster_V1_AgentSession(); value.id = id; value.title = title; value.provider = provider; value.deviceName = device; value.deviceID = device == "Studio Mac" ? "studio" : "laptop"; value.state = "idle"; return value
    }
    private func message(_ id: String, _ role: String, _ text: String) -> Muster_App_ChatBlock {
        var block = Muster_App_ChatBlock(); block.id = id; block.kind = "message"; block.message.id = id; block.message.role = role; block.message.kind = "text"; block.message.text = text; return block
    }
    @MainActor
    private func capture<V: View>(_ view: V, name: String, size: CGSize, scale: CGFloat, dark: Bool = false) throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(origin: .zero, size: size)
        window.overrideUserInterfaceStyle = dark ? .dark : .light
        let host = UIHostingController(rootView: view.tint(Color(red: 160/255, green: 100/255, blue: 75/255)))
        window.rootViewController = host; window.makeKeyAndVisible()
        host.view.layoutIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(1))
        let format = UIGraphicsImageRendererFormat(); format.scale = scale; format.opaque = true
        var rendered = false
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in rendered = window.drawHierarchy(in: window.bounds, afterScreenUpdates: true) }
        XCTAssertTrue(rendered)
        let attachment = XCTAttachment(image: image); attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
        try XCTUnwrap(image.pngData()).write(to: FileManager.default.temporaryDirectory.appendingPathComponent("muster-store-\(name).png"))
        window.isHidden = true
    }
}
