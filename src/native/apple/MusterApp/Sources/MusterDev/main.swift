// Local development only. The installed app uses Apps/macOS instead.
#if os(macOS) && canImport(SwiftUI)
import AppKit
import Foundation
import MusterCore
import MusterUI
import SwiftUI

/// OS lifecycle only: a fresh private instance, never a launch agent or the
/// installed daemon. The shared core still owns all product state.
final class DevRuntime {
    static let shared = DevRuntime()
    let directory: URL
    let daemon = Process()
    var error: String?
    private var log: FileHandle?

    private init() {
        directory = URL(fileURLWithPath: "/tmp/muster-dev-\(UUID().uuidString)", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                                    attributes: [.posixPermissions: 0o700])
            setenv("MUSTER_DIR", directory.path, 1)
            let executable = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/musterd")
            guard FileManager.default.isExecutableFile(atPath: executable.path) else {
                throw NSError(domain: "MusterDev", code: 1, userInfo: [NSLocalizedDescriptionKey:
                    "Build the isolated dev bundle with scripts/dev-terminal.sh first."])
            }
            let path = directory.appendingPathComponent("daemon.log")
            FileManager.default.createFile(atPath: path.path, contents: nil, attributes: [.posixPermissions: 0o600])
            log = try FileHandle(forWritingTo: path)
            daemon.executableURL = executable
            daemon.arguments = ["--dir", directory.path, "--bind", "127.0.0.1:0", "--name", "Muster Dev",
                                "--no-vnet", "--no-agent", "--no-cloud", "--terminal", "auto"]
            daemon.standardOutput = log
            daemon.standardError = log
            try daemon.run()
            print("Muster Dev data: \(directory.path)")
        } catch { self.error = String(describing: error) }
    }

    func stop() {
        if daemon.isRunning { daemon.terminate() }
    }
}

final class DevDelegate: NSObject, NSApplicationDelegate {
    func applicationWillTerminate(_ notification: Notification) { DevRuntime.shared.stop() }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

struct MusterDevApp: App {
    @NSApplicationDelegateAdaptor(DevDelegate.self) private var delegate
    @StateObject private var model = MusterModel(keys: SoftwareKeys())
    private let runtime = DevRuntime.shared

    var body: some Scene {
        WindowGroup("Muster Dev — isolated local instance") {
            VStack(spacing: 0) {
                Text("DEV · \(runtime.directory.path)").font(.caption.monospaced()).textSelection(.enabled).padding(6)
                if let error = runtime.error { Text(error).foregroundStyle(.red).padding() }
                else { MusterRootView(model: model) }
            }
            .frame(minWidth: 720, minHeight: 480)
            .onAppear {
                guard runtime.error == nil else { return }
                let directory = runtime.directory
                MusterRuntime.initLogging(level: 3)
                model.start(config: .desktop(dir: directory, socket: directory.appendingPathComponent("musterd.sock")), prepare: {
                    try await Task.detached(priority: .userInitiated) {
                        try MusterRuntime.prepareDesktop(socket: directory.appendingPathComponent("musterd.sock").path,
                                                         managed: false, action: { _ in 2 })
                    }.value
                })
            }
        }
        .commands { CommandGroup(replacing: .newItem) {} }
    }
}

MusterDevApp.main()
#else
print("MusterDev requires macOS.")
#endif
