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
}
#endif
