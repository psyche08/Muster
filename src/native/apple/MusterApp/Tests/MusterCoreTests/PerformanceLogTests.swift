import Foundation
import XCTest
@testable import MusterCore

final class PerformanceLogTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("muster-performance-test-\(UUID())")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try FileManager.default.removeItem(at: url) }
        return url
    }

    func testRotationBoundsDiskUsageAndExportsInOrder() throws {
        let directory = try directory()
        let log = try PerformanceLog(directory: directory, maximumFileBytes: 128)
        for index in 0..<100 { try log.append(["index": index, "cpu_percent": 5.5]) }
        for name in ["performance.jsonl", "performance.previous.jsonl"] {
            let data = try Data(contentsOf: directory.appendingPathComponent(name))
            XCTAssertLessThanOrEqual(data.count, 128)
        }
        let export = try log.export()
        let snapshot = try Data(contentsOf: export)
        XCTAssertLessThanOrEqual(snapshot.count, 256)
        let rows = try String(decoding: snapshot, as: UTF8.self).split(separator: "\n").map {
            try XCTUnwrap(JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any])["index"] as! Int
        }
        XCTAssertEqual(rows, rows.sorted())
        XCTAssertEqual(rows.last, 99)
        try log.append(["index": 100])
        XCTAssertEqual(try Data(contentsOf: export), snapshot, "Sharing must use a frozen snapshot")
    }

    func testRelaunchPreservesHistoryAndRecoversPartialLastRecord() throws {
        let directory = try directory()
        do {
            let log = try PerformanceLog(directory: directory)
            try log.append(["run": "first"])
        }
        let path = directory.appendingPathComponent("performance.jsonl")
        let file = try FileHandle(forWritingTo: path)
        try file.seekToEnd()
        try file.write(contentsOf: Data("{\"incomplete\":".utf8))
        try file.close()
        let log = try PerformanceLog(directory: directory)
        try log.append(["run": "second"])
        let lines = try String(contentsOf: log.export(), encoding: .utf8).split(separator: "\n")
        XCTAssertEqual(lines.count, 2)
        for line in lines { XCTAssertNoThrow(try JSONSerialization.jsonObject(with: Data(line.utf8))) }
        XCTAssertTrue(lines[0].contains("first"))
        XCTAssertTrue(lines[1].contains("second"))
    }

    func testOversizedRecordDoesNotDiscardExistingHistory() throws {
        let log = try PerformanceLog(directory: directory(), maximumFileBytes: 128)
        try log.append(["valid": true])
        let before = try Data(contentsOf: log.export())
        XCTAssertThrowsError(try log.append(["oversized": String(repeating: "x", count: 256)]))
        XCTAssertEqual(try Data(contentsOf: log.export()), before)
    }
}
