import Foundation
import Darwin
@_silgen_name("fixture_rust_hot") func rustHot()
@_silgen_name("fixture_stop") func stop()
@_silgen_name("fixture_stopped") func stopped() -> Bool
@inline(never) func swiftHotInner() {
    while !stopped() { _ = ProcessInfo.processInfo.systemUptime }
}
final class Worker: @unchecked Sendable { var id: UInt64 = 0 }
let ready = DispatchSemaphore(value: 0)
let finished = DispatchSemaphore(value: 0)
let rust = Worker(), swift = Worker()
for (worker, body) in [(rust, rustHot), (swift, swiftHotInner)] {
    Thread {
        pthread_threadid_np(nil, &worker.id)
        ready.signal()
        body()
        finished.signal()
    }.start()
}
for _ in 0..<2 { precondition(ready.wait(timeout: .now() + 5) == .success) }
Thread.sleep(forTimeInterval: 0.1)
let report = HighCPUStacks.capture([HotThread(id: rust.id, cpuPercent: 100, main: false),
                                   HotThread(id: swift.id, cpuPercent: 100, main: false)])
stop()
for _ in 0..<2 { precondition(finished.wait(timeout: .now() + 5) == .success, "sampler failed to resume a thread") }
let row: [String: Any] = ["high_cpu_stacks": report, "timestamp": Date().timeIntervalSince1970,
                          "cpu_percent": 200, "screen": 0, "thermal_state": 0]
let data = try JSONSerialization.data(withJSONObject: row, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
print("")
