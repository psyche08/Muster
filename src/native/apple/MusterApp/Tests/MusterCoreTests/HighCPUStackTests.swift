import XCTest
#if os(macOS) || os(iOS)
import Darwin
import CMusterDiagnostics
#endif
@testable import MusterCore

final class HighCPUStackTests: XCTestCase {
    private func counters(_ values: [(UInt64, Double)]) -> [ThreadCPUCounter] {
        values.map { ThreadCPUCounter(id: $0.0, seconds: $0.1, main: $0.0 == 1) }
    }
    func testRanksDeltasNotLifetimeTotalsAndLimitsCaptures() {
        var policy = HighCPUStackPolicy()
        policy.reset(counters([(1, 500), (2, 0), (3, 0), (4, 0), (5, 0)]))
        let hot = policy.select(processCPU: 200, interval: 10, now: 10,
                                counters: counters([(1, 500.1), (2, 8), (3, 5), (4, 4), (5, 3)]))
        XCTAssertEqual(hot.map(\.id), [2, 3, 4])
        XCTAssertEqual(hot[0].cpuPercent, 80, accuracy: 0.001)
        XCTAssertTrue(policy.select(processCPU: 200, interval: 10, now: 20,
                                    counters: counters([(2, 16)])).isEmpty)
        XCTAssertEqual(policy.select(processCPU: 100, interval: 50, now: 70,
                                     counters: counters([(2, 56)])).first?.id, 2)
    }
    func testForegroundBaselineAndCooldownPreventBackgroundOrRestartSpikes() {
        var policy = HighCPUStackPolicy()
        policy.reset(counters([(1, 10)]))
        XCTAssertEqual(policy.select(processCPU: 100, interval: 10, now: 10,
                                     counters: counters([(1, 20)])).count, 1)
        policy.reset(counters([(1, 100)]))
        XCTAssertTrue(policy.select(processCPU: 100, interval: 10, now: 30,
                                    counters: counters([(1, 110)])).isEmpty)
        policy.reset()
        XCTAssertTrue(policy.select(processCPU: 100, interval: 10, now: 80,
                                    counters: counters([(1, 1000)])).isEmpty)
        XCTAssertEqual(policy.select(processCPU: 100, interval: 10, now: 90,
                                     counters: counters([(1, 1010)])).count, 1)
    }
    func testLowCPUCounterResetAndNewThreadsDoNotTrigger() {
        var policy = HighCPUStackPolicy()
        policy.reset(counters([(1, 100)]))
        XCTAssertTrue(policy.select(processCPU: 20, interval: 10, now: 10,
                                    counters: counters([(1, 105)])).isEmpty)
        XCTAssertTrue(policy.select(processCPU: 100, interval: 10, now: 20,
                                    counters: counters([(1, 0), (2, 500)])).isEmpty)
        XCTAssertTrue(policy.select(processCPU: .nan, interval: 10, now: 30,
                                    counters: counters([(1, 10)])).isEmpty)
    }
    #if os(macOS) || os(iOS)
    func testSamplerRejectsSelfAndExitedThreadWithoutSuspendingCaller() {
        var caller: UInt64 = 0
        pthread_threadid_np(nil, &caller)
        var result = MusterThreadStack()
        muster_thread_stack(caller, &result)
        XCTAssertEqual(result.status, KERN_INVALID_ARGUMENT)
        XCTAssertEqual(result.count, 0)
        muster_thread_stack(UInt64.max, &result)
        XCTAssertEqual(result.status, KERN_INVALID_NAME)
        XCTAssertEqual(result.count, 0)
    }
    #if os(iOS)
    func testHighCPUAutomaticallyReachesExportedLog() async throws {
        let worker = BusyWorker()
        let ready = DispatchSemaphore(value: 0)
        let finished = DispatchSemaphore(value: 0)
        Thread {
            pthread_threadid_np(nil, &worker.id)
            ready.signal()
            worker.burn()
            finished.signal()
        }.start()
        XCTAssertEqual(ready.wait(timeout: .now() + 5), .success)
        defer { worker.stop() }
        let monitor = PerformanceDiagnostics.shared
        await MainActor.run {
            monitor.start(); monitor.setForeground(false); monitor.setForeground(true)
        }
        try await Task.sleep(nanoseconds: 12_000_000_000)
        await MainActor.run { monitor.setForeground(false) }
        worker.stop()
        XCTAssertEqual(finished.wait(timeout: .now() + 5), .success)
        let url = try await monitor.export()
        let rows = try String(contentsOf: url, encoding: .utf8).split(separator: "\n").compactMap {
            try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any]
        }
        let samples = rows.compactMap { $0["high_cpu_stacks"] as? [String: Any] }
            .flatMap { $0["samples"] as? [[String: Any]] ?? [] }
        XCTAssertTrue(samples.contains { ($0["thread_id"] as? UInt64) == worker.id &&
            ($0["capture_status"] as? Int) == 0 && ($0["resume_status"] as? Int) == 0 },
            "An actual high-CPU interval must automatically produce stacks in the existing exported log")
    }
    #endif

    func testCapturesRunningSwiftThreadAndResumesIt() throws {
        let ready = DispatchSemaphore(value: 0)
        let finished = DispatchSemaphore(value: 0)
        let worker = BusyWorker()
        let thread = Thread {
            pthread_threadid_np(nil, &worker.id)
            ready.signal()
            worker.burn()
            finished.signal()
        }
        thread.qualityOfService = .userInitiated
        thread.start()
        XCTAssertEqual(ready.wait(timeout: .now() + 5), .success)
        defer { worker.stop() }
        let report = HighCPUStacks.capture([HotThread(id: worker.id, cpuPercent: 100, main: false)])
        let samples = try XCTUnwrap(report["samples"] as? [[String: Any]])
        XCTAssertEqual(samples.count, 3)
        for sample in samples {
            XCTAssertEqual(sample["capture_status"] as? Int32, KERN_SUCCESS)
            XCTAssertEqual(sample["resume_status"] as? Int32, KERN_SUCCESS)
            let count = (sample["frames"] as? [[String: Any]])?.count ?? 0
            XCTAssertGreaterThan(count, 0)
            if count == 1 {
                XCTAssertTrue(["time_limit", "invalid_frame", "unreadable"].contains(sample["stop"] as? String ?? ""),
                              "A budget-limited/partial walk must be explicit, never presented as complete")
            }
        }
        XCTAssertFalse(try XCTUnwrap(report["images"] as? [String: Any]).isEmpty)
        XCTAssertNoThrow(try JSONSerialization.data(withJSONObject: report))
        worker.stop()
        XCTAssertEqual(finished.wait(timeout: .now() + 5), .success)
    }
    #endif
}
#if os(macOS) || os(iOS)
private final class BusyWorker: @unchecked Sendable {
    var id: UInt64 = 0 // published before ready.signal()
    private let lock = NSLock()
    private var stopped = false
    func stop() { lock.lock(); stopped = true; lock.unlock() }
    @inline(never) func burn() {
        while true {
            lock.lock(); let done = stopped; lock.unlock()
            if done { return }
        }
    }
}

#endif
