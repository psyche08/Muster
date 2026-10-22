// Muster.app on macOS (XcodeGen target MusterMac): a client of musterd
// over its socket. The daemon ships inside the bundle
// (Contents/MacOS/musterd) with a LaunchAgent plist
// (Contents/Library/LaunchAgents/dev.linsheng.musterd.plist); the app
// registers it with launchd on first run, and the daemon keeps its files
// in ~/Library/Application Support/Muster and ~/Library/Logs/Muster.
import MusterCore
import MusterUI
import ServiceManagement
import SwiftUI

@main
struct MusterMacApp: App {
    init() { MusterRuntime.desktopUpdateStartup() }

    @StateObject private var model = MusterModel()

    var body: some Scene {
        WindowGroup("Muster") {
            MusterRootView(model: model)
                .tint(Color("AccentColor"))
                .frame(minWidth: 720, minHeight: 480)
                .task {
                    MusterRuntime.initLogging(level: 3)
                    DesktopHelper.registerIfInstalled()
                    let socket = MusterPaths.daemonSocket
                    let managed = DaemonAgent.isBundled && ProcessInfo.processInfo.environment["MUSTER_DIR"] == nil
                    model.start(config: .desktop(dir: MusterPaths.stateDirectory, socket: socket), prepare: {
                        try await Task.detached {
                            try MusterRuntime.prepareDesktop(socket: socket.path, managed: managed, action: { DaemonAgent.perform($0) })
                        }.value
                    })
                }
        }
        .commands { MusterSettingsCommands(model: model) }
    }

}

/// The LaunchAgent that runs the bundled musterd (SMAppService needs the
/// plist inside the app bundle and the program named relative to it).
enum DaemonAgent {
    static let plist = "dev.linsheng.musterd.plist"

    static var isBundled: Bool {
        Bundle.main.url(forResource: "dev.linsheng.musterd", withExtension: "plist", subdirectory: "Library/LaunchAgents") != nil
            || FileManager.default.fileExists(atPath: Bundle.main.bundleURL.appendingPathComponent("Contents/Library/LaunchAgents/\(plist)").path)
    }

    /// Only OS service operations live here; Shared Rust decides when to call.
    static func perform(_ action: UInt32) -> Int32 {
        let service = SMAppService.agent(plistName: plist)
        guard service.status != .requiresApproval else { return 1 }
        do {
            switch action {
            case 0:
                if service.status != .enabled { try service.register() }
            case 1:
                let job = try inspectJob()
                if job.running {
                    NSLog("muster: desktop startup: native=running_job_preserved")
                    return 0
                }
                if job.exists {
                    // No -k: never kill an existing daemon or its sessions.
                    let result = try launchctl(["kickstart", jobTarget])
                    guard result.status == 0 else {
                        NSLog("muster: desktop startup: kickstart failed: %@", result.output)
                        return 2
                    }
                } else {
                    // Recheck immediately before repairing stale registration.
                    // A job that appeared meanwhile is left to start normally.
                    guard try !inspectJob().exists else { return 0 }
                    if service.status == .enabled { try service.unregister() }
                    try service.register()
                }
            default: return 2
            }
            NSLog("muster: desktop startup: native_action=%u completed", action)
            return service.status == .requiresApproval ? 1 : 0
        } catch {
            NSLog("muster: desktop startup: native_action=%u failed: %@", action, "\(error)")
            return 2
        }
    }

    private static var jobTarget: String { "gui/\(getuid())/dev.linsheng.musterd" }

    private static func inspectJob() throws -> (exists: Bool, running: Bool) {
        let result = try launchctl(["print", jobTarget])
        if result.status == 0 {
            let running = result.output.range(of: #"(?m)^\s*pid = [1-9][0-9]*\s*$"#, options: .regularExpression) != nil
            return (true, running)
        }
        // Fail closed on authorization or other launchctl failures; they are
        // not proof that registration is stale.
        guard result.output.contains("Could not find service") else {
            throw NSError(domain: "MusterLaunchAgent", code: Int(result.status), userInfo: [NSLocalizedDescriptionKey: result.output])
        }
        return (false, false)
    }

    private static func launchctl(_ arguments: [String]) throws -> (status: Int32, output: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = arguments
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus, String(decoding: data, as: UTF8.self))
    }
}
