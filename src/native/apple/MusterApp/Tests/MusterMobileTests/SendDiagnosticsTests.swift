#if os(iOS)
import Foundation
import XCTest
@testable import MusterCore

final class SendDiagnosticsTests: XCTestCase {
    func testEventStorageDoesNotWaitForPerformanceSamplingAndExportPreservesErrors() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let monitor = PerformanceDiagnostics(directory: directory)
        let record = Data(#"{"record_type":"send_diagnostic","schema_version":1,"op_id":"app-fixture-1","phase":"dispatch","error_category":"remote","remote_error_code":6}"#.utf8)
        monitor.recordSendDiagnostic(record)
        let file = try await monitor.export()
        let first = try Data(contentsOf: file)
        XCTAssertEqual(first, record + Data([10]))
        let status = await monitor.currentStatus()
        XCTAssertFalse(status.recording, "Send events persist while periodic OS sampling is paused")
        XCTAssertNil(status.error)
        monitor.recordSendDiagnostic(Data(repeating: 65, count: 4097))
        let next = try await monitor.export()
        XCTAssertEqual(try Data(contentsOf: next), first)
        let reopened = PerformanceDiagnostics(directory: directory)
        reopened.recordSendDiagnostic(Data(#"{"record_type":"send_diagnostic","event":"completed_unknown"}"#.utf8))
        let reopenedFile = try await reopened.export()
        let records = try String(contentsOf: reopenedFile, encoding: .utf8).split(separator: "\n")
        XCTAssertEqual(records.count, 2)
        XCTAssertTrue(records[0].contains("app-fixture-1"))
        XCTAssertTrue(records[1].contains("completed_unknown"))
    }
}
#endif
