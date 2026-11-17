#if os(macOS)
import XCTest
import MusterCore
@testable import MusterUI

final class DesktopStartupTests: XCTestCase {
    private actor Attempts {
        var count = 0
        func prepare() async throws {
            count += 1
            try await Task.sleep(for: .milliseconds(50))
            throw MusterRuntimeError.create("fixture service unavailable")
        }
    }

    @MainActor
    func testRetryRepeatsPreparationWithoutConcurrentStarts() async throws {
        let attempts = Attempts()
        let model = MusterModel(keys: SoftwareKeys())
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = MusterConfig.desktop(dir: dir, socket: dir.appendingPathComponent("missing.sock"))
        model.start(config: config, prepare: { try await attempts.prepare() })
        model.retry()
        model.start(config: config)
        for _ in 0..<200 where model.startError == nil { try await Task.sleep(for: .milliseconds(5)) }
        XCTAssertEqual(model.startError, "fixture service unavailable")
        let firstCount = await attempts.count
        XCTAssertEqual(firstCount, 1)
        XCTAssertNil(model.core)
        model.retry()
        XCTAssertNil(model.startError)
        for _ in 0..<200 where model.startError == nil { try await Task.sleep(for: .milliseconds(5)) }
        let secondCount = await attempts.count
        XCTAssertEqual(secondCount, 2)
        XCTAssertEqual(model.startError, "fixture service unavailable")
        XCTAssertNil(model.core)
    }

    private actor Launch {
        var checks = 0
        var prepares = 0
        let startup: DesktopStartup
        init(_ startup: DesktopStartup) { self.startup = startup }
        func check() -> DesktopStartup { checks += 1; return startup }
        func prepare() throws {
            prepares += 1
            throw MusterRuntimeError.create("fixture password cancelled")
        }
    }

    @MainActor
    private func launch(_ startup: DesktopStartup) -> (MusterModel, Launch, MusterConfig, DesktopLaunch) {
        let fixture = Launch(startup)
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = MusterConfig.desktop(dir: dir, socket: dir.appendingPathComponent("missing.sock"))
        let desktop = DesktopLaunch(managed: true, check: { await fixture.check() }, prepare: { try await fixture.prepare() })
        return (MusterModel(keys: SoftwareKeys()), fixture, config, desktop)
    }

    @MainActor
    private func settle(_ model: MusterModel, until done: () -> Bool) async throws {
        for _ in 0..<400 where !done() { try await Task.sleep(for: .milliseconds(5)) }
    }

    /// A stopped musterd is asked about; nothing starts until the owner
    /// says so, and a failed start stays on the question with its reason.
    @MainActor
    func testStoppedDaemonWaitsForTheOwnersStart() async throws {
        let (model, fixture, config, desktop) = launch(.start(needsAdmin: true, automatic: false))
        model.start(config: config, desktop: desktop)
        try await settle(model) { model.gate != nil }
        XCTAssertEqual(model.gate, .start(needsAdmin: true, managed: true))
        let before = await fixture.prepares
        XCTAssertEqual(before, 0, "the first look starts nothing")
        model.continueWithoutMigration()
        XCTAssertNotNil(model.gate, "only a move can be put off")
        model.startService()
        XCTAssertTrue(model.gateBusy)
        model.startService()
        try await settle(model) { model.startError != nil }
        XCTAssertEqual(model.startError, "fixture password cancelled")
        XCTAssertEqual(model.gate, .start(needsAdmin: true, managed: true))
        XCTAssertFalse(model.gateBusy)
        XCTAssertNil(model.core)
        let after = await fixture.prepares
        XCTAssertEqual(after, 1, "a second click while busy does nothing")
        model.retry()
        try await settle(model) { model.gate != nil }
        let checks = await fixture.checks
        XCTAssertEqual(checks, 2, "retry looks again")
    }

    /// Muster's own restart starts the daemon it stopped without asking; a
    /// failure falls back to the question.
    @MainActor
    func testResumedStartRunsWithoutAsking() async throws {
        let (model, fixture, config, desktop) = launch(.start(needsAdmin: false, automatic: true))
        model.start(config: config, desktop: desktop)
        try await settle(model) { model.startError != nil }
        let prepares = await fixture.prepares
        XCTAssertEqual(prepares, 1)
        XCTAssertEqual(model.gate, .start(needsAdmin: false, managed: true))
        XCTAssertEqual(model.startError, "fixture password cancelled")
    }

    /// A daemon running from an older registration may stay for now.
    @MainActor
    func testMoveCanBePutOff() async throws {
        let (model, fixture, config, desktop) = launch(.migrate(needsAdmin: true))
        model.start(config: config, desktop: desktop)
        try await settle(model) { model.gate != nil }
        XCTAssertEqual(model.gate, .migrate(needsAdmin: true))
        model.continueWithoutMigration()
        XCTAssertNil(model.gate)
        XCTAssertTrue(model.core != nil || model.startError != nil, "the main page is started")
        let prepares = await fixture.prepares
        XCTAssertEqual(prepares, 0)
    }
}
#endif
