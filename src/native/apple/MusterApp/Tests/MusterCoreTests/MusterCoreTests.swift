// The Swift wrapper against the real Rust core (libmuster_ffi): a phone core
// with a software key creates a network, the view models arrive decoded,
// effects can be answered, and the helpers agree with muster-terminal.
import Foundation
import XCTest
@testable import MusterCore

final class Capture {
    private let lock = NSLock()
    private var views: [ViewModel] = []
    private var effects: [(UInt64, Effect)] = []

    func add(_ vm: ViewModel) { lock.lock(); views.append(vm); lock.unlock() }
    func add(_ id: UInt64, _ effect: Effect) { lock.lock(); effects.append((id, effect)); lock.unlock() }
    var latest: ViewModel? { lock.lock(); defer { lock.unlock() }; return views.last }
    var count: Int { lock.lock(); defer { lock.unlock() }; return views.count }
    var lastEffect: (UInt64, Effect)? { lock.lock(); defer { lock.unlock() }; return effects.last }

    func wait(_ what: String, seconds: Double = 30, until check: (ViewModel) -> Bool) throws -> ViewModel {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            if let vm = latest, check(vm) { return vm }
            Thread.sleep(forTimeInterval: 0.05)
        }
        XCTFail("timed out waiting for \(what); last: \(String(describing: latest))")
        throw MusterRuntimeError.create("timeout")
    }
}

final class MusterCoreTests: XCTestCase {
    func testCreatesANetworkThroughTheABI() throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("muster-swift-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }

        let capture = Capture()
        var config = MusterConfig.phone(dir: dir, name: "swift phone")
        config.bind = "127.0.0.1:0"
        config.allowLoopback = true
        config.version = "9.9 (42)"
        let core = try MusterCore(
            config: config,
            keys: SoftwareKeys(),
            onViewModel: { capture.add($0) },
            onEffect: { capture.add($0, $1) }
        )
        XCTAssertFalse(core.version.isEmpty)

        let onboarding = try capture.wait("onboarding") { $0.hasSelf_p && $0.screen == .onboarding }
        XCTAssertTrue(onboarding.embedded)
        XCTAssertEqual(onboarding.self_p.name, "swift phone")
        XCTAssertEqual(onboarding.self_p.kind, "phone")
        XCTAssertEqual(onboarding.self_p.deviceID.count, 64)
        XCTAssertEqual(onboarding.self_p.version, "9.9 (42)")
        XCTAssertFalse(onboarding.self_p.commit.isEmpty)

        core.createNetwork(name: "home")
        let home = try capture.wait("agents") { $0.screen == .agents && $0.workspace == .agents && $0.network.hasNetwork_p }
        XCTAssertEqual(home.network.name, "home")
        XCTAssertEqual(home.network.members.count, 1)
        XCTAssertTrue(home.network.members[0].isSelf)
        XCTAssertEqual(home.network.members[0].version, "9.9 (42)")
        XCTAssertEqual(home.network.members[0].commit, onboarding.self_p.commit)

        // An intent that needs an open chat is refused with a toast.
        core.openFiles()
        _ = try capture.wait("toast") { vm in vm.toasts.contains { $0.text.contains("open a session first") } }

        // Lifecycle and push token are accepted without a relay.
        core.setPushToken(kind: 1, token: Data([1, 2, 3]))
        core.lifecycle(.networkChanged)
        core.lifecycle(.foreground)

        let line = try XCTUnwrap(core.sshPublicKeyLine())
        XCTAssertTrue(line.hasPrefix("ecdsa-sha2-nistp256 "), line)
        XCTAssertTrue(line.hasSuffix(" muster-soft"), line)

        // Without a relay an invite cannot be issued: the refusal is a toast,
        // and the shell is back where it was.
        core.showInvite(ttlSeconds: 60)
        _ = try capture.wait("invite refused") { vm in vm.toasts.contains { $0.error && $0.text.contains("relay") } }
        core.navigate(to: .settings)
        _ = try capture.wait("settings") { $0.screen == .settings }
        core.back()
        _ = try capture.wait("agents again") { $0.screen == .agents && $0.workspace == .agents }

        core.shutdown()
        XCTAssertGreaterThanOrEqual(capture.count, 4)
    }

    func testBadConfigIsAnError() {
        XCTAssertThrowsError(
            try MusterRuntime(configJSON: Data("{\"dir\": \"/tmp/x\", \"mode\": \"teleport\"}".utf8), shell: SoftwareShell())
        ) { error in
            XCTAssertTrue("\(error)".contains("teleport"), "\(error)")
        }
    }

    func testStyleRunsDecodeLikeMusterTerminal() {
        // Two runs: 3 bold red-on-default cells, then 2 palette-4 background cells.
        let bytes = Data([
            0, 3, 0x01, 0xff, 0x00, 0x00, 0, 0, 0, 0, 1,
            0, 2, 0, 0, 0, 0, 0x02, 0, 0, 4, 0,
        ])
        let runs = TerminalStyleRun.decode(bytes)
        XCTAssertEqual(runs.count, 2)
        XCTAssertEqual(runs[0].length, 3)
        XCTAssertEqual(runs[0].foreground, .rgb(255, 0, 0))
        XCTAssertEqual(runs[0].background, .default)
        XCTAssertTrue(runs[0].flags.contains(.bold))
        XCTAssertEqual(runs[1].background, .palette(4))
        XCTAssertEqual(TerminalStyleRun.perCell(bytes, columns: 4).count, 4)
        XCTAssertTrue(TerminalStyleRun.decode(Data()).isEmpty)
        XCTAssertTrue(TerminalStyleRun.decode(Data([1, 2, 3])).isEmpty)
    }

    func testKeyNamesMatchTheCore() {
        XCTAssertEqual(TerminalKey.control("c"), .ctrlC)
        XCTAssertEqual(TerminalKey.control("C"), .ctrlC)
        XCTAssertEqual(TerminalKey.control("q")?.rawValue, "ctrl-q")
        XCTAssertNil(TerminalKey.control("1"))
        XCTAssertEqual(TerminalKey.pageup.rawValue, "pageup")
    }

    func testConfigJSONMatchesTheHeader() throws {
        let config = MusterConfig.desktop(dir: URL(fileURLWithPath: "/tmp/h"), socket: URL(fileURLWithPath: "/tmp/h/musterd.sock"))
        let json = try JSONSerialization.jsonObject(with: config.json()) as? [String: Any]
        XCTAssertEqual(json?["mode"] as? String, "socket")
        XCTAssertEqual(json?["socket"] as? String, "/tmp/h/musterd.sock")
        XCTAssertEqual(json?["allow_loopback"] as? Bool, false)
        var phone = MusterConfig.phone(dir: URL(fileURLWithPath: "/tmp/p"), name: "phone")
        phone.version = "1.0 (16)"
        let phoneJSON = try JSONSerialization.jsonObject(with: phone.json()) as? [String: Any]
        XCTAssertEqual(phoneJSON?["version"] as? String, "1.0 (16)")
    }
}

/// A shell that renders nothing, for the error-path test.
final class SoftwareShell: MusterShell {
    func onView(_ bytes: Data) {}
    func onEffect(id: UInt64, bytes: Data) {}
    func hardwarePublicKey() -> Data? { nil }
    func hardwareSign(_ message: Data) -> Data? { nil }
    func presencePublicKey() -> Data? { nil }
}
