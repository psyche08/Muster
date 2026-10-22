import XCTest
@testable import MusterCore

final class ComposerDraftSourceTests: XCTestCase {
    func testEditsStayLocalUntilAnActionCapturesTheLatestBuffer() throws {
        let source = ComposerDraftSource()
        var text = "", reads = 0
        _ = source.register {
            reads += 1
            return .with { $0.key = "rust-key"; $0.text = text; $0.revision = "edit" }
        }
        for index in 0..<100 { text = "Draft \(index)" }
        XCTAssertEqual(reads, 0)
        let action = Intent.with { $0.navigate = .with { $0.back = true } }
        let captured = source.attaching(to: action)
        let decoded = try Intent(serializedBytes: captured.serializedData())
        XCTAssertEqual(reads, 1)
        XCTAssertEqual(decoded.composerAction.draft.text, "Draft 99")
        XCTAssertEqual(decoded.composerAction.draft.key, "rust-key")
        XCTAssertEqual(decoded.composerAction.action, action)
    }

    func testOldViewDisappearingCannotRemoveTheNewEditorsCallback() {
        let source = ComposerDraftSource()
        let old = source.register { .with { $0.key = "first" } }
        let new = source.register { .with { $0.key = "second" } }
        source.unregister(old)
        let action = Intent.with { $0.refresh = .init() }
        XCTAssertEqual(source.attaching(to: action).composerAction.draft.key, "second")
        source.unregister(new)
        XCTAssertEqual(source.attaching(to: action), action)
    }
}
