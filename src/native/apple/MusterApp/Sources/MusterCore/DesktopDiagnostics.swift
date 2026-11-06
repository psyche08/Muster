#if os(macOS)
import Foundation

/// Opaque Rust records, serialized into the same bounded export format as iOS.
/// No parsing or product decisions occur here.
final class DesktopDiagnostics: @unchecked Sendable {
    static let shared = DesktopDiagnostics()
    private let queue = DispatchQueue(label: "muster.desktop-diagnostics", qos: .utility)
    private let lock = NSLock()
    private var pending = 0
    private var log: PerformanceLog?

    func append(_ bytes: Data) {
        guard !bytes.isEmpty, bytes.count <= 4096 else { return }
        lock.lock()
        guard pending < 256 else { lock.unlock(); return }
        pending += 1
        lock.unlock()
        queue.async { [self] in
            defer { lock.lock(); pending -= 1; lock.unlock() }
            do {
                if log == nil {
                    let directory = try FileManager.default.url(for: .cachesDirectory,
                        in: .userDomainMask, appropriateFor: nil, create: true)
                        .appendingPathComponent("MusterDiagnostics")
                    log = try PerformanceLog(directory: directory)
                }
                try log?.appendJSON(bytes)
            } catch { /* Diagnostics never gate product work. */ }
        }
    }
}
#endif
