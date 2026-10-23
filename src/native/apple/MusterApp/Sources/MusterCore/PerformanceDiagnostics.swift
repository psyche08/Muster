#if os(iOS)
import Darwin
import Foundation
import UIKit

public struct PerformanceMonitorStatus: Sendable {
    public var recording = false
    public var sampledAt: Date?
    public var cpuPercent = 0.0
    public var memoryBytes: UInt64 = 0
    public var thermalState = 0
    public var error: String?
}

/// OS telemetry and opaque Rust send diagnostics. No conversation content is recorded.
public final class PerformanceDiagnostics: @unchecked Sendable {
    public static let shared = PerformanceDiagnostics()
    private let storageDirectory: URL?
    init(directory: URL? = nil) { storageDirectory = directory }

    private let lock = NSLock()
    private let queue = DispatchQueue(label: "muster.performance-diagnostics", qos: .utility)
    private let runID = UUID().uuidString
    private var timer: DispatchSourceTimer?
    private var log: PerformanceLog?
    private var started = false
    private var active = false
    private var pendingDiagnosticRows = 0
    private var droppedDiagnosticRows = 0
    private var snapshots = 0
    private var bytes = 0
    private var decodeSeconds = 0.0
    private var screen = -1
    private var messages = 0
    private var mainLagSeconds = 0.0
    private var brightness = -1.0
    private var batteryLevel = -1.0
    private var batteryState = -1
    private var previousCPU = 0.0
    private var previousTime = 0.0
    private var generation: UInt64 = 0
    private var monitorWorkSeconds = 0.0
    private var mainThread: UInt64 = 0
    private var stackPolicy = HighCPUStackPolicy()
    private var status = PerformanceMonitorStatus()

    /// Called once from the main thread before starting the core; no launch arguments.
    @MainActor public func start() {
        guard !started else { return }
        started = true
        pthread_threadid_np(nil, &mainThread)
        UIDevice.current.isBatteryMonitoringEnabled = true
        captureUIKitContext()
        setForeground(UIApplication.shared.applicationState == .active)
    }

    /// Native scene lifecycle only. Background time is excluded from CPU intervals.
    @MainActor public func setForeground(_ foreground: Bool) {
        guard started else { return }
        queue.async { [self] in
            guard foreground != status.recording else { return }
            if foreground {
                do {
                    try ensureLog()
                    stackPolicy.reset(threadCounters(threads()))
                    previousCPU = Self.cpuSeconds()
                    previousTime = ProcessInfo.processInfo.systemUptime
                    lock.lock()
                    snapshots = 0; bytes = 0; decodeSeconds = 0; mainLagSeconds = 0; active = true; generation &+= 1
                    lock.unlock()
                    status.recording = true; status.error = nil
                    let source = DispatchSource.makeTimerSource(queue: queue)
                    source.schedule(deadline: .now() + 10, repeating: 10, leeway: .seconds(1))
                    source.setEventHandler { [weak self] in self?.sample() }
                    timer = source
                    source.resume()
                } catch { fail(error) }
            } else {
                timer?.cancel(); timer = nil
                lock.lock(); active = false; lock.unlock()
                status.recording = false
                stackPolicy.reset()
            }
        }
    }

    public func recordView(bytes: Int, decodeSeconds: Double, screen: Int, messages: Int) {
        lock.lock()
        defer { lock.unlock() }
        self.screen = screen; self.messages = messages
        guard active else { return }
        snapshots += 1
        self.bytes += bytes
        self.decodeSeconds += decodeSeconds
    }

    /// Opaque Rust records; event-driven even when periodic OS sampling is paused.
    public func recordSendDiagnostic(_ bytes: Data) {
        guard !bytes.isEmpty, bytes.count <= 4096 else { return }
        lock.lock()
        guard pendingDiagnosticRows < 256 else {
            droppedDiagnosticRows += 1; lock.unlock(); return
        }
        pendingDiagnosticRows += 1; lock.unlock()
        queue.async { [self] in
            defer { lock.lock(); pendingDiagnosticRows -= 1; lock.unlock() }
            do {
                try ensureLog()
                lock.lock(); let dropped = droppedDiagnosticRows; droppedDiagnosticRows = 0; lock.unlock()
                if dropped > 0 { try log?.append(["record_type": "diagnostic_storage", "dropped_records": dropped]) }
                try log?.appendJSON(bytes)
            } catch { status.error = "部分发送诊断未能写入本机日志。" }
        }
    }

    private func ensureLog() throws {
        guard log == nil else { return }
        let directory = try storageDirectory ?? FileManager.default.url(for: .cachesDirectory, in: .userDomainMask,
            appropriateFor: nil, create: true).appendingPathComponent("MusterDiagnostics")
        log = try PerformanceLog(directory: directory)
    }

    public func currentStatus() async -> PerformanceMonitorStatus {
        await withCheckedContinuation { continuation in
            queue.async { continuation.resume(returning: self.status) }
        }
    }

    public func export() async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { [self] in
                do {
                    guard let log else { throw CocoaError(.fileNoSuchFile) }
                    continuation.resume(returning: try log.export())
                } catch { continuation.resume(throwing: error) }
            }
        }
    }

    private static func seconds(_ value: timeval) -> Double {
        Double(value.tv_sec) + Double(value.tv_usec) / 1_000_000
    }

    private static func cpuSeconds() -> Double {
        var usage = rusage()
        getrusage(RUSAGE_SELF, &usage)
        return seconds(usage.ru_utime) + seconds(usage.ru_stime)
    }

    private static func footprint() -> UInt64? {
        var info = task_vm_info_data_t()
        var size = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(size)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &size)
            }
        }
        return result == KERN_SUCCESS ? info.phys_footprint : nil
    }

    @MainActor private func captureUIKitContext() {
        let device = UIDevice.current
        let screenBrightness = UIScreen.main.brightness
        lock.lock()
        brightness = Double(screenBrightness)
        batteryLevel = Double(device.batteryLevel)
        batteryState = device.batteryState.rawValue
        lock.unlock()
    }

    private func sample() {
        let now = ProcessInfo.processInfo.systemUptime
        let cpu = Self.cpuSeconds()
        lock.lock()
        var row: [String: Any] = ["snapshots": snapshots, "snapshot_bytes": bytes,
            "decode_seconds": decodeSeconds, "screen": screen, "messages": messages,
            "main_queue_lag_seconds": mainLagSeconds, "brightness": brightness,
            "battery_level": batteryLevel, "battery_state": batteryState]
        snapshots = 0; bytes = 0; decodeSeconds = 0; mainLagSeconds = 0
        let probeGeneration = generation
        lock.unlock()
        let cpuPercent = 100 * (cpu - previousCPU) / max(0.001, now - previousTime)
        let thermal = ProcessInfo.processInfo.thermalState.rawValue
        let memory = Self.footprint()
        row["schema_version"] = 2
        row["run_id"] = runID
        row["version"] = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
        row["build"] = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? ""
        row["timestamp"] = Date().timeIntervalSince1970
        row["foreground"] = true
        row["interval_seconds"] = now - previousTime
        row["cpu_percent"] = cpuPercent // 100% means one core.
        row["memory_footprint_bytes"] = memory
        row["thermal_state"] = thermal
        row["low_power_mode"] = ProcessInfo.processInfo.isLowPowerModeEnabled
        row["monitor_previous_work_seconds"] = monitorWorkSeconds
        // Baseline every interval so a new high-CPU episode can identify the actual
        // busy threads. Do not use lifetime CPU totals to rank them.
        let nativeThreads = threads()
        row["threads"] = nativeThreads
        let hot = stackPolicy.select(processCPU: cpuPercent, interval: now - previousTime,
                                     now: now, counters: threadCounters(nativeThreads))
        if !hot.isEmpty {
            row["high_cpu_stacks"] = HighCPUStacks.capture(hot)
        }
        previousTime = now; previousCPU = cpu
        do { try log?.append(row) } catch { fail(error); return }
        status.sampledAt = Date(); status.cpuPercent = cpuPercent
        status.memoryBytes = memory ?? 0; status.thermalState = thermal
        monitorWorkSeconds = ProcessInfo.processInfo.systemUptime - now
        let probeStart = ProcessInfo.processInfo.systemUptime
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let lag = ProcessInfo.processInfo.systemUptime - probeStart
            captureUIKitContext()
            lock.lock()
            // Ignore a probe that crosses a background/foreground boundary.
            if active && generation == probeGeneration { mainLagSeconds = max(mainLagSeconds, lag) }
            lock.unlock()
        }
    }

    private func threadCounters(_ rows: [[String: Any]]) -> [ThreadCPUCounter] {
        rows.compactMap { row in
            guard let id = row["id"] as? UInt64, let seconds = row["cpu_seconds"] as? Double,
                  let main = row["main"] as? Bool else { return nil }
            return ThreadCPUCounter(id: id, seconds: seconds, main: main)
        }
    }

    private func threads() -> [[String: Any]] {
        var list: thread_act_array_t?
        var count: mach_msg_type_number_t = 0
        guard task_threads(mach_task_self_, &list, &count) == KERN_SUCCESS, let list else { return [] }
        defer {
            for index in 0..<Int(count) { mach_port_deallocate(mach_task_self_, list[index]) }
            vm_deallocate(mach_task_self_, vm_address_t(UInt(bitPattern: list)),
                vm_size_t(Int(count) * MemoryLayout<thread_t>.stride))
        }
        return (0..<Int(count)).compactMap { index in
            var info = thread_basic_info()
            var size = mach_msg_type_number_t(MemoryLayout<thread_basic_info>.size / MemoryLayout<integer_t>.size)
            let result = withUnsafeMutablePointer(to: &info) {
                $0.withMemoryRebound(to: integer_t.self, capacity: Int(size)) {
                    thread_info(list[index], thread_flavor_t(THREAD_BASIC_INFO), $0, &size)
                }
            }
            guard result == KERN_SUCCESS else { return nil }
            var identity = thread_identifier_info()
            var idSize = mach_msg_type_number_t(MemoryLayout<thread_identifier_info>.size / MemoryLayout<integer_t>.size)
            let idResult = withUnsafeMutablePointer(to: &identity) {
                $0.withMemoryRebound(to: integer_t.self, capacity: Int(idSize)) {
                    thread_info(list[index], thread_flavor_t(THREAD_IDENTIFIER_INFO), $0, &idSize)
                }
            }
            guard idResult == KERN_SUCCESS else { return nil }
            let cpu = Double(info.user_time.seconds + info.system_time.seconds)
                + Double(info.user_time.microseconds + info.system_time.microseconds) / 1_000_000
            return ["id": identity.thread_id, "main": identity.thread_id == mainThread,
                "cpu_seconds": cpu, "state": info.run_state]
        }
    }

    private func fail(_ error: Error) {
        timer?.cancel(); timer = nil
        lock.lock(); active = false; lock.unlock()
        status.recording = false
        status.error = "性能日志暂不可写（\((error as NSError).code)）"
        log = nil
    }
}
#endif
