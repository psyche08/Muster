#if os(macOS)
import AppKit

/// OS lifecycle adapter. Rust decides when a restart is allowed. A detached
/// relauncher waits for this exact UI PID; it never kills other applications.
enum DesktopRestart {
    static func schedule() throws {
        guard MacInstall.isInstalled else {
            throw NSError(domain: "Muster", code: 1, userInfo: [NSLocalizedDescriptionKey: "请从 /Applications/Muster.app 重启。"])
        }
        // Do not terminate the UI if its launchd service cannot be restarted.
        let service = MacInstall.daemonService
        let probe = Process()
        probe.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        probe.arguments = ["print", service]
        probe.standardOutput = FileHandle.nullDevice
        probe.standardError = FileHandle.nullDevice
        try probe.run()
        probe.waitUntilExit()
        guard probe.terminationStatus == 0 else {
            throw NSError(domain: "Muster", code: 2, userInfo: [NSLocalizedDescriptionKey: "Muster 后台服务未注册，请先允许后台运行。"])
        }
        let relaunch = Process()
        relaunch.executableURL = URL(fileURLWithPath: "/bin/sh")
        // Values are separate argv entries, never interpolated shell source.
        relaunch.arguments = ["-c", """
        i=0
        while kill -0 "$1" 2>/dev/null; do
          i=$((i+1)); [ "$i" -lt 60 ] || exit 1
          sleep 1
        done
        /bin/launchctl kill SIGTERM "$2"
        sleep 2
        /bin/launchctl kickstart "$2"
        exec /usr/bin/open -n "$3"
        """, "muster-relaunch", String(getpid()), service, Bundle.main.bundleURL.path]
        relaunch.standardInput = FileHandle.nullDevice
        relaunch.standardOutput = FileHandle.nullDevice
        relaunch.standardError = FileHandle.nullDevice
        try relaunch.run()
    }
}

/// Platform-only launchd lifecycle for the component's bundle replacement.
@MainActor
enum DesktopUpdateLifecycle {
    private static var stopped = false
    static func prepare() async throws {
        guard MacInstall.isInstalled else {
            throw NSError(domain: "Muster", code: 1, userInfo: [NSLocalizedDescriptionKey: "请从 /Applications/Muster.app 安装更新。"])
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = ["bootout", MacInstall.daemonService]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        let status: Int32 = try await withCheckedThrowingContinuation { continuation in
            process.terminationHandler = { child in continuation.resume(returning: child.terminationStatus) }
            do { try process.run() } catch { continuation.resume(throwing: error) }
        }
        guard status == 0 else {
            throw NSError(domain: "Muster", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "无法停止 Muster 后台服务，未安装更新。"])
        }
        stopped = true
    }
    static func restoreService() {
        guard stopped else { return }
        stopped = false
        // A user job also supports installations predating SMAppService.
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        child.arguments = ["submit", "-l", MacInstall.daemonLabel, "--", MacInstall.daemon, "--log"]
        do { try child.run() } catch { NSLog("Muster service recovery failed: %@", error.localizedDescription) }
    }
}
#endif
