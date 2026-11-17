#if os(macOS)
import AppKit
import Darwin

/// OS lifecycle adapter for "重启 Muster" (ADR 0227). Rust decides when a
/// restart is allowed. The app snapshots every process of this user that
/// runs from the installed bundle (musterd, muster-panel and the Locked Use
/// watchdog it hosts, muster-browser-host, add-ons), then hands the list to
/// a detached relauncher that waits for this exact UI PID, stops them,
/// starts musterd and opens the app again. It never touches a process that
/// is not Muster's: a PID is only signalled while it still runs a program of
/// the same name.
enum DesktopRestart {
    /// Left alone: the command line runs the user's own commands, and the
    /// root helper is root's (signalling it would need the password; it
    /// carries no permission that a restart could renew).
    static let spared = ["Contents/Helpers/muster", "Contents/Library/LaunchServices/muster-helper"]

    /// The processes a restart stops, as `pid:name` arguments.
    static func targets(_ processes: [(pid: pid_t, uid: uid_t, path: String)], own: pid_t, uid: uid_t, bundle: String) -> [String] {
        let contents = bundle + "/Contents/"
        let spared = Set(spared.map { bundle + "/" + $0 })
        return processes
            .filter { $0.pid != own && $0.uid == uid && $0.path.hasPrefix(contents) && !spared.contains($0.path) }
            .map { "\($0.pid):\(($0.path as NSString).lastPathComponent)" }
    }

    /// Every process the system lets this user see, with its owner and program.
    static func processes() -> [(pid: pid_t, uid: uid_t, path: String)] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var pids = [pid_t](repeating: 0, count: Int(count) + 64)
        let filled = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<pid_t>.size))
        guard filled > 0 else { return [] }
        var found: [(pid: pid_t, uid: uid_t, path: String)] = []
        var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        for pid in pids.prefix(Int(filled)) where pid > 0 {
            var info = proc_bsdinfo()
            let size = Int32(MemoryLayout<proc_bsdinfo>.size)
            guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size,
                  proc_pidpath(pid, &path, UInt32(path.count)) > 0 else { continue }
            found.append((pid, info.pbi_uid, String(cString: path)))
        }
        return found
    }

    /// The relauncher. Arguments: the app's PID, the launchd service, the
    /// bundle, then `pid:name` targets. Values are separate argv entries,
    /// never shell source. Each target gets SIGTERM and ten seconds, then
    /// SIGKILL. MUSTER_LAUNCHCTL, MUSTER_OPEN and MUSTER_STOP_TICKS (tenths
    /// of a second) stand in for the real tools and wait in tests.
    static let script = #"""
    app=$1; service=$2; bundle=$3; shift 3
    launchctl=${MUSTER_LAUNCHCTL:-/bin/launchctl}
    alive() { n=$(ps -p "$1" -o comm= 2>/dev/null) && [ "$(basename "$n")" = "$2" ]; }
    each() { for t in "$@"; do pid=${t%%:*}; name=${t#*:}; alive "$pid" "$name" && return 0; done; return 1; }
    i=0
    while kill -0 "$app" 2>/dev/null; do
      i=$((i+1)); [ "$i" -lt 600 ] || exit 1
      sleep 0.1
    done
    "$launchctl" kill SIGTERM "$service" 2>/dev/null
    for t in "$@"; do pid=${t%%:*}; name=${t#*:}; alive "$pid" "$name" && kill -TERM "$pid" 2>/dev/null; done
    i=0
    while each "$@"; do
      i=$((i+1)); [ "$i" -lt "${MUSTER_STOP_TICKS:-100}" ] || break
      sleep 0.1
    done
    for t in "$@"; do pid=${t%%:*}; name=${t#*:}; alive "$pid" "$name" && kill -KILL "$pid" 2>/dev/null; done
    "$launchctl" kickstart "$service"
    exec "${MUSTER_OPEN:-/usr/bin/open}" -n "$bundle" --args --muster-restarted
    """#

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
            throw NSError(domain: "Muster", code: 2, userInfo: [NSLocalizedDescriptionKey: "Muster 后台服务未注册，请先完成启动。"])
        }
        let bundle = Bundle.main.bundleURL.path
        let targets = targets(processes(), own: getpid(), uid: getuid(), bundle: bundle)
        NSLog("muster: desktop restart: stopping %@", targets.joined(separator: " "))
        let relaunch = Process()
        relaunch.executableURL = URL(fileURLWithPath: "/bin/sh")
        relaunch.arguments = ["-c", script, "muster-relaunch", String(getpid()), service, bundle] + targets
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
        // The system LaunchAgent when it is there (ADR 0227); otherwise a
        // user job, so an installation that has not moved yet keeps a daemon.
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        child.arguments = FileManager.default.fileExists(atPath: DaemonAgent.systemPath)
            ? ["bootstrap", "gui/\(getuid())", DaemonAgent.systemPath]
            : ["submit", "-l", MacInstall.daemonLabel, "--", MacInstall.daemon, "--log"]
        do { try child.run() } catch { NSLog("Muster service recovery failed: %@", error.localizedDescription) }
    }
}
#endif
