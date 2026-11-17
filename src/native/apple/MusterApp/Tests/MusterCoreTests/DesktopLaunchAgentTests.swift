#if os(macOS)
import XCTest
import MusterCore
@testable import MusterUI

/// The macOS adapters of ADR 0227: what launchd and the file system report,
/// and the restart that stops every Muster process (ADR 0227 §4).
final class DesktopLaunchAgentTests: XCTestCase {
    func testStartupCodesMatchTheCABI() {
        XCTAssertEqual(DesktopStartup(code: 0), .ready)
        XCTAssertEqual(DesktopStartup(code: 1), .start(needsAdmin: false, automatic: false))
        XCTAssertEqual(DesktopStartup(code: 1 | 16), .start(needsAdmin: true, automatic: false))
        XCTAssertEqual(DesktopStartup(code: 1 | 32), .start(needsAdmin: false, automatic: true))
        XCTAssertEqual(DesktopStartup(code: 2), .migrate(needsAdmin: false))
        XCTAssertEqual(DesktopStartup(code: 2 | 16), .migrate(needsAdmin: true))
        for code: Int32 in [-1, 3, 4, 16, 32, 2 | 32, 64, 1 | 8] {
            XCTAssertNil(DesktopStartup(code: code), "\(code)")
        }
    }

    func testJobOriginComesFromItsPath() {
        let smAppService = """
        gui/501/dev.linsheng.musterd = {
        \tactive count = 2
        \tpath = (submitted by smd.114)
        \ttype = Submitted
        \tmanaged_by = com.apple.xpc.ServiceManagement
        \tstate = running

        \tpid = 4242
        \tstdout path = /dev/null
        }
        """
        XCTAssertEqual(DaemonAgent.job(status: 0, output: smAppService), .init(state: .running, origin: .other))
        let system = """
        gui/501/dev.linsheng.musterd = {
        \tactive count = 0
        \tpath = /Library/LaunchAgents/dev.linsheng.musterd.plist
        \ttype = LaunchAgent
        \tstate = not running
        \tlast exit code = 0
        }
        """
        XCTAssertEqual(DaemonAgent.job(status: 0, output: system), .init(state: .stopped, origin: .system))
        XCTAssertEqual(DaemonAgent.job(status: 113, output: "Bad request.\nCould not find service \"dev.linsheng.musterd\" in domain for user gui: 501\n"),
                       .init(state: .absent, origin: .none))
        // Anything else is not proof the job is gone.
        XCTAssertNil(DaemonAgent.job(status: 1, output: "Operation not permitted"))
    }

    func testOnlyAnIdenticalRootOwnedFileIsCurrent() {
        let template = Data("<plist/>".utf8)
        XCTAssertEqual(DaemonAgent.agentFile(installed: nil, owner: 0, mode: 0o100644, template: template), .missing)
        XCTAssertEqual(DaemonAgent.agentFile(installed: template, owner: 0, mode: 0o100644, template: template), .current)
        XCTAssertEqual(DaemonAgent.agentFile(installed: Data("<old/>".utf8), owner: 0, mode: 0o100644, template: template), .outdated)
        XCTAssertEqual(DaemonAgent.agentFile(installed: template, owner: 501, mode: 0o100644, template: template), .outdated)
        XCTAssertEqual(DaemonAgent.agentFile(installed: template, owner: 0, mode: 0o100664, template: template), .outdated)
        XCTAssertEqual(DaemonAgent.encode(agent: .outdated, job: .init(state: .running, origin: .other)), 2 | 2 << 2 | 2 << 4)
    }

    func testRestartStopsEveryBundleProcessButTheAppTheCommandLineAndRoot() {
        let bundle = "/Applications/Muster.app"
        let processes: [(pid: pid_t, uid: uid_t, path: String)] = [
            (10, 501, bundle + "/Contents/MacOS/Muster"),
            (11, 501, bundle + "/Contents/MacOS/musterd"),
            (12, 501, bundle + "/Contents/Helpers/muster-panel"),
            (13, 501, bundle + "/Contents/Helpers/muster-browser-host"),
            (14, 501, bundle + "/Contents/Helpers/l4forwarder"),
            (15, 501, bundle + "/Contents/Helpers/muster"),
            (16, 0, bundle + "/Contents/Library/LaunchServices/muster-helper"),
            (17, 502, bundle + "/Contents/MacOS/musterd"),
            (18, 501, "/Applications/Muster.app.old/Contents/MacOS/musterd"),
            (19, 501, "/usr/bin/sleep"),
        ]
        XCTAssertEqual(DesktopRestart.targets(processes, own: 10, uid: 501, bundle: bundle),
                       ["11:musterd", "12:muster-panel", "13:muster-browser-host", "14:l4forwarder"])
    }

    /// The relauncher, with launchctl and open stood in: it waits for the
    /// app, stops each listed process (killing one that ignores SIGTERM),
    /// spares a PID that now runs something else, starts musterd and opens
    /// the app marked as restarted.
    func testRelauncherStopsEveryListedProcessThenReopens() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("muster-restart-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let log = dir.appendingPathComponent("log")
        for tool in ["launchctl", "open"] {
            let url = dir.appendingPathComponent(tool)
            try "#!/bin/sh\necho \(tool) \"$@\" >> \"\(log.path)\"\n".write(to: url, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
        }
        for helper in ["muster-panel", "muster-browser-host"] {
            // A copy of a platform binary is killed at exec; a link runs
            // sleep under the helper's name.
            try FileManager.default.createSymbolicLink(atPath: dir.appendingPathComponent(helper).path, withDestinationPath: "/bin/sleep")
        }
        // Children start with default signal handling: a test runner may
        // ignore SIGTERM, and an ignored signal survives exec.
        func spawn(_ path: String, _ arguments: [String]) throws -> pid_t {
            var attr: posix_spawnattr_t?
            posix_spawnattr_init(&attr)
            defer { posix_spawnattr_destroy(&attr) }
            var all = sigset_t()
            sigfillset(&all)
            var none = sigset_t()
            sigemptyset(&none)
            posix_spawnattr_setsigdefault(&attr, &all)
            posix_spawnattr_setsigmask(&attr, &none)
            posix_spawnattr_setflags(&attr, Int16(POSIX_SPAWN_SETSIGDEF | POSIX_SPAWN_SETSIGMASK))
            let argv = ([path] + arguments).map { strdup($0) } + [nil]
            defer { argv.forEach { free($0) } }
            var pid: pid_t = 0
            let status = posix_spawn(&pid, path, nil, &attr, argv, nil)
            guard status == 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(status)) }
            return pid
        }
        /// The signal that ended `pid`, once it has.
        func signalled(_ pid: pid_t) -> Int32? {
            var status: Int32 = 0
            guard waitpid(pid, &status, 0) == pid else { return nil }
            return status & 0x7f
        }
        func running(_ pid: pid_t) -> Bool {
            var status: Int32 = 0
            return waitpid(pid, &status, WNOHANG) == 0
        }
        let app = try spawn("/bin/sleep", ["0.3"])
        let panel = try spawn(dir.appendingPathComponent("muster-panel").path, ["30"])
        // Ignores SIGTERM (an ignored signal survives exec).
        let stubborn = try spawn("/bin/sh", ["-c", "trap '' TERM; exec \"$0\" 30", dir.appendingPathComponent("muster-browser-host").path])
        let decoy = try spawn("/bin/sleep", ["30"])
        defer { for pid in [app, panel, stubborn, decoy] { kill(pid, SIGKILL); var s: Int32 = 0; waitpid(pid, &s, WNOHANG) } }
        Thread.sleep(forTimeInterval: 0.2)

        let relaunch = Process()
        relaunch.executableURL = URL(fileURLWithPath: "/bin/sh")
        relaunch.arguments = ["-c", DesktopRestart.script, "muster-relaunch", String(app),
                              "gui/501/dev.linsheng.musterd", "/Applications/Muster.app",
                              "\(panel):muster-panel", "\(stubborn):muster-browser-host", "\(decoy):muster-panel"]
        relaunch.environment = ["PATH": "/usr/bin:/bin",
                                "MUSTER_LAUNCHCTL": dir.appendingPathComponent("launchctl").path,
                                "MUSTER_OPEN": dir.appendingPathComponent("open").path,
                                "MUSTER_STOP_TICKS": "10"]
        // The relauncher waits for the app to go; reap it as it exits.
        DispatchQueue.global().async { _ = signalled(app) }
        try relaunch.run()
        relaunch.waitUntilExit()
        XCTAssertEqual(relaunch.terminationStatus, 0)
        XCTAssertEqual(signalled(panel), SIGTERM)
        XCTAssertEqual(signalled(stubborn), SIGKILL)
        XCTAssertTrue(running(decoy), "a PID that runs another program is left alone")
        let lines = try String(contentsOf: log, encoding: .utf8).split(separator: "\n").map(String.init)
        XCTAssertEqual(lines, [
            "launchctl kill SIGTERM gui/501/dev.linsheng.musterd",
            "launchctl kickstart gui/501/dev.linsheng.musterd",
            "open -n /Applications/Muster.app --args --muster-restarted",
        ])
    }
}
#endif
