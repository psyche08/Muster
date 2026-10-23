import Foundation

/// Bounded local diagnostic storage. Confined to the monitor's utility queue.
final class PerformanceLog {
    let directory: URL
    let maximumFileBytes: Int
    private let current: URL
    private let previous: URL
    private var file: FileHandle?
    private var bytes = 0

    init(directory: URL, maximumFileBytes: Int = 2 * 1024 * 1024) throws {
        self.directory = directory
        self.maximumFileBytes = maximumFileBytes
        current = directory.appendingPathComponent("performance.jsonl")
        previous = directory.appendingPathComponent("performance.previous.jsonl")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700])
        try openCurrent()
    }

    deinit { try? file?.close() }

    func append(_ record: [String: Any]) throws {
        let data = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
        try appendJSON(data)
    }

    /// Rust owns the record schema; the platform only checks framing and size.
    func appendJSON(_ record: Data) throws {
        guard !record.isEmpty, !record.contains(10), !record.contains(13) else { throw CocoaError(.fileWriteUnknown) }
        var data = record
        data.append(10)
        guard data.count <= maximumFileBytes else { throw CocoaError(.fileWriteOutOfSpace) }
        if bytes + data.count > maximumFileBytes {
            try file?.close(); file = nil
            if FileManager.default.fileExists(atPath: previous.path) {
                try FileManager.default.removeItem(at: previous)
            }
            try FileManager.default.moveItem(at: current, to: previous)
            try openCurrent()
        }
        try file?.write(contentsOf: data)
        bytes += data.count
    }

    /// Freeze a bounded snapshot so sharing never races subsequent appends.
    func export() throws -> URL {
        let exportDirectory = directory.appendingPathComponent("Export")
        try FileManager.default.createDirectory(at: exportDirectory, withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700])
        let destination = exportDirectory.appendingPathComponent("Muster-performance.jsonl")
        var data = Data()
        for url in [previous, current] where FileManager.default.fileExists(atPath: url.path) {
            let contents = try Data(contentsOf: url)
            // A process may have died in the middle of its last write. Keep only complete lines.
            if let end = contents.lastIndex(of: 10) { data.append(contents.prefix(through: end)) }
        }
        try data.write(to: destination, options: .atomic)
        try protect(destination)
        return destination
    }

    private func openCurrent() throws {
        if !FileManager.default.fileExists(atPath: current.path) {
            guard FileManager.default.createFile(atPath: current.path, contents: nil,
                attributes: [.posixPermissions: 0o600]) else { throw CocoaError(.fileWriteUnknown) }
        }
        try protect(current)
        let opened = try FileHandle(forUpdating: current)
        // Recover a torn final JSONL record without discarding earlier sessions.
        let data = try opened.readToEnd() ?? Data()
        let validBytes = data.lastIndex(of: 10).map { $0 + 1 } ?? 0
        if validBytes != data.count { try opened.truncate(atOffset: UInt64(validBytes)) }
        try opened.seekToEnd()
        bytes = validBytes
        file = opened
    }

    private func protect(_ url: URL) throws {
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        #if os(iOS)
        // Diagnostics contain no conversation content or raw device identity. Permit paired-device
        // extraction after the first unlock, subject to iOS's own access restrictions.
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
            ofItemAtPath: url.path)
        #endif
    }
}
