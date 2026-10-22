import Foundation
#if os(macOS) || os(iOS)
import CMusterDiagnostics
#endif

/// OS telemetry only: CPU deltas refer to native threads regardless of language.
struct ThreadCPUCounter {
    let id: UInt64
    let seconds: Double
    let main: Bool
}
struct HotThread {
    let id: UInt64
    let cpuPercent: Double
    let main: Bool
}
struct HighCPUStackPolicy {
    private var previous: [UInt64: Double] = [:]
    private var lastCapture: Double?
    let processThreshold = 80.0
    let threadThreshold = 20.0
    let cooldown = 60.0

    // Restart the baseline on foreground changes without bypassing the cooldown.
    mutating func reset(_ counters: [ThreadCPUCounter] = []) {
        previous = Dictionary(counters.map { ($0.id, $0.seconds) }, uniquingKeysWith: { _, new in new })
    }
    mutating func select(processCPU: Double, interval: Double, now: Double,
                         counters: [ThreadCPUCounter]) -> [HotThread] {
        let baseline = previous
        reset(counters)
        guard processCPU.isFinite, processCPU >= processThreshold,
              interval.isFinite, interval > 0, now.isFinite,
              lastCapture.map({ now - $0 >= cooldown }) ?? true else { return [] }
        let hot = counters.compactMap { counter -> HotThread? in
            guard let old = baseline[counter.id], counter.seconds.isFinite, counter.seconds >= old else { return nil }
            let cpu = 100 * (counter.seconds - old) / interval
            guard cpu >= threadThreshold else { return nil }
            return HotThread(id: counter.id, cpuPercent: cpu, main: counter.main)
        }.sorted { $0.cpuPercent > $1.cpuPercent }.prefix(3)
        guard !hot.isEmpty else { return [] }
        lastCapture = now
        return Array(hot)
    }
}

#if os(macOS) || os(iOS)
enum HighCPUStacks {
    static func capture(_ threads: [HotThread]) -> [String: Any] {
        muster_stack_prepare()
        var images: [String: [String: Any]] = [:]
        var samples: [[String: Any]] = []
        let start = ProcessInfo.processInfo.systemUptime
        for round in 0..<3 {
            if round > 0 { Thread.sleep(forTimeInterval: 0.02) }
            for thread in threads.prefix(3) {
                var stack = MusterThreadStack()
                let sampledAt = Date().timeIntervalSince1970
                muster_thread_stack(thread.id, &stack)
                // Target has resumed before touching Foundation/Swift allocations.
                let addresses = withUnsafeBytes(of: stack.pcs) {
                    Array($0.bindMemory(to: UInt.self).prefix(Int(stack.count)))
                }
                let frames: [[String: Any]] = addresses.enumerated().map { index, pc in
                    var frame: [String: Any] = ["pc": hex(pc), "kind": index == 0 ? "instruction" : "return"]
                    // Return PCs point just after the call. Map pc-1 so a call at
                    // the end of an image stays in the calling image.
                    let lookupPC = index == 0 ? pc : pc &- 1
                    var image = MusterStackImage()
                    if muster_stack_image(lookupPC, &image) {
                        let uuid = cString(&image.uuid)
                        let key = "\(uuid)@\(hex(image.base))"
                        frame["image"] = key
                        frame["offset"] = hex(pc &- image.base)
                        images[key] = ["uuid": uuid, "name": cString(&image.name),
                                       "arch": cString(&image.arch), "base": hex(image.base),
                                       "size": image.size]
                    }
                    return frame
                }
                samples.append(["thread_id": thread.id, "main": thread.main,
                                "thread_cpu_percent": thread.cpuPercent, "round": round,
                                "timestamp": sampledAt, "capture_status": stack.status,
                                "resume_status": stack.resume_status, "stop": stopLabel(stack.stop),
                                "suspend_duration_ns": stack.duration_ns, "frames": frames])
            }
        }
        return ["format": "native-frame-pointer-v1", "samples": samples, "images": images,
                "duration_seconds": ProcessInfo.processInfo.systemUptime - start]
    }
    private static func hex(_ value: UInt) -> String { "0x" + String(value, radix: 16) }
    private static func cString<T>(_ value: inout T) -> String {
        withUnsafeBytes(of: &value) { bytes in
            String(decoding: bytes.prefix(while: { $0 != 0 }), as: UTF8.self)
        }
    }
    private static func stopLabel(_ value: UInt32) -> String {
        switch Int(value) {
        case MUSTER_STACK_END: return "end"
        case MUSTER_STACK_LIMIT: return "frame_limit"
        case MUSTER_STACK_INVALID_FRAME: return "invalid_frame"
        case MUSTER_STACK_UNREADABLE: return "unreadable"
        case MUSTER_STACK_TIME_LIMIT: return "time_limit"
        default: return "unsupported"
        }
    }
}

#endif
