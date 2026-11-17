// The launchd side of musterd on a Mac (ADR 0227). launchd runs musterd
// from the system LaunchAgent /Library/LaunchAgents/dev.linsheng.musterd.plist,
// a byte-for-byte copy of the template the app bundle ships. Writing that
// file is the one step that needs the administrator's password; loading,
// starting and stopping the job happen in the user's own launchd domain.
//
// Only OS operations live here. Shared Rust (muster-app-core
// desktop_startup) decides what to show and when to call each action.
#if os(macOS)
import Foundation
import ServiceManagement

public enum DaemonAgent {
    /// Where launchd reads the system LaunchAgent.
    public static let systemPath = "/Library/LaunchAgents/\(MacInstall.daemonLabel).plist"
    /// The SMAppService plist of older installations, kept in the bundle
    /// only so the app can unregister that job.
    static let legacyPlist = "\(MacInstall.daemonLabel).plist"

    /// The template `systemPath` must equal.
    public static var template: URL {
        Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/SystemLaunchAgent/\(MacInstall.daemonLabel).plist")
    }

    // MARK: Facts (the C ABI's encoding; see muster.h)

    public enum AgentFile: UInt32 { case missing = 0, current = 1, outdated = 2 }
    public enum JobState: UInt32 { case absent = 0, stopped = 1, running = 2 }
    public enum JobOrigin: UInt32 { case none = 0, system = 1, other = 2 }

    public struct Job: Equatable {
        public var state: JobState
        public var origin: JobOrigin
    }

    static func encode(agent: AgentFile, job: Job) -> UInt32 {
        agent.rawValue | job.state.rawValue << 2 | job.origin.rawValue << 4
    }

    /// The installed file against the template: the same bytes, owned by
    /// root and writable by nobody else (launchd refuses anything else).
    static func agentFile(installed: Data?, owner: uid_t, mode: mode_t, template: Data) -> AgentFile {
        guard let installed else { return .missing }
        return installed == template && owner == 0 && mode & 0o022 == 0 ? .current : .outdated
    }

    /// `launchctl print gui/<uid>/<label>`. Nil when launchctl failed for
    /// any reason other than a missing job: that is not proof of absence.
    static func job(status: Int32, output: String, systemPath: String = systemPath) -> Job? {
        guard status == 0 else {
            return output.contains("Could not find service") ? Job(state: .absent, origin: .none) : nil
        }
        let running = output.range(of: #"(?m)^\s*pid = [1-9][0-9]*\s*$"#, options: .regularExpression) != nil
        // The job's own path line comes first; nested blocks name theirs
        // with a prefix ("stdout path = ").
        var origin = JobOrigin.none
        if let line = output.range(of: #"(?m)^\s*path = .*$"#, options: .regularExpression) {
            let path = output[line].trimmingCharacters(in: .whitespaces).dropFirst("path = ".count)
            origin = path == systemPath ? .system : .other
        }
        return Job(state: running ? .running : .stopped, origin: origin)
    }

    // MARK: The callback Rust calls

    /// 0 register, 1 recover a stopped job, 2 install, 3 inspect, 4 reload.
    /// Inspect returns the facts; the others 0 done, 1 macOS wants the
    /// owner's approval, 3 the owner cancelled the password, 2 failed.
    public static func perform(_ action: UInt32) -> Int32 {
        do {
            let code: Int32
            switch action {
            case 3: code = Int32(try inspect())
            case 2: code = try install()
            case 0: code = try register()
            case 1: code = try recoverStopped()
            case 4: code = try reload()
            default: code = 2
            }
            if action != 3 { NSLog("muster: desktop startup: native_action=%u result=%d", action, code) }
            return code
        } catch {
            NSLog("muster: desktop startup: native_action=%u failed: %@", action, "\(error)")
            return action == 3 ? -2 : 2
        }
    }

    static func inspect() throws -> UInt32 {
        encode(agent: try currentAgentFile(), job: try inspectJob())
    }

    private static func currentAgentFile() throws -> AgentFile {
        let template = try Data(contentsOf: Self.template)
        var info = stat()
        guard stat(systemPath, &info) == 0 else {
            guard errno == ENOENT else { throw posixError("stat \(systemPath)") }
            return .missing
        }
        let installed = try Data(contentsOf: URL(fileURLWithPath: systemPath))
        return agentFile(installed: installed, owner: info.st_uid, mode: info.st_mode, template: template)
    }

    /// The administrator's password: macOS's own prompt, through osascript.
    /// The paths travel as arguments, never inside the script's source.
    private static func install() throws -> Int32 {
        guard MacInstall.isInstalled else { return 2 }
        let script = [
            "on run argv",
            #"do shell script "/bin/mkdir -p /Library/LaunchAgents && /usr/bin/install -o root -g wheel -m 0644 " & quoted form of item 1 of argv & " " & quoted form of item 2 of argv with prompt "Muster 要把后台服务 musterd 的登录代理写入 /Library/LaunchAgents。" with administrator privileges"#,
            "end run",
        ]
        let result = try run("/usr/bin/osascript", script.flatMap { ["-e", $0] } + [template.path, systemPath])
        guard result.status == 0 else {
            if result.output.contains("(-128)") { return 3 }
            NSLog("muster: desktop startup: install failed: %@", result.output)
            return 2
        }
        return try currentAgentFile() == .current ? 0 : 2
    }

    /// launchd runs musterd from the system LaunchAgent; a running system
    /// job is left alone, a job registered any other way is replaced.
    private static func register() throws -> Int32 {
        let job = try inspectJob()
        switch (job.state, job.origin) {
        case (.absent, _): return try bootstrap()
        case (.running, .system): return 0
        case (.stopped, .system): return try kickstart()
        default:
            try removeLegacy()
            return try bootstrap()
        }
    }

    private static func recoverStopped() throws -> Int32 {
        let job = try inspectJob()
        switch job.state {
        case .running:
            NSLog("muster: desktop startup: native=running_job_preserved")
            return 0
        case .stopped: return try kickstart()
        case .absent:
            guard try currentAgentFile() == .current else { return 2 }
            return try bootstrap()
        }
    }

    /// After an install changed the definition of a loaded job.
    private static func reload() throws -> Int32 {
        if try inspectJob().state != .absent {
            _ = try launchctl(["bootout", MacInstall.daemonService])
            guard try waitUntilAbsent() else { return 2 }
        }
        return try bootstrap()
    }

    private static func bootstrap() throws -> Int32 {
        let result = try launchctl(["bootstrap", "gui/\(getuid())", systemPath])
        if result.status == 0 { return 0 }
        // Someone else loaded it meanwhile: start it if it is not running.
        if let job = try? inspectJob(), job.state != .absent {
            if job.state == .running { return 0 }
            return try kickstart()
        }
        NSLog("muster: desktop startup: bootstrap failed: %@", result.output)
        // Turned off under Login Items: launchd will not load it.
        if SMAppService.statusForLegacyPlist(at: URL(fileURLWithPath: systemPath)) == .requiresApproval { return 1 }
        return 2
    }

    /// No -k: never kill an existing daemon or its sessions.
    private static func kickstart() throws -> Int32 {
        let result = try launchctl(["kickstart", MacInstall.daemonService])
        guard result.status == 0 else {
            NSLog("muster: desktop startup: kickstart failed: %@", result.output)
            return 2
        }
        return 0
    }

    /// The SMAppService agent (or a `launchctl submit` job) of an older
    /// installation: unregister it, then make sure launchd let it go.
    private static func removeLegacy() throws {
        let service = SMAppService.agent(plistName: legacyPlist)
        if service.status != .notRegistered && service.status != .notFound {
            do { try service.unregister() } catch {
                NSLog("muster: desktop startup: legacy unregister failed: %@", "\(error)")
            }
        }
        if try inspectJob().state != .absent {
            _ = try launchctl(["bootout", MacInstall.daemonService])
        }
        guard try waitUntilAbsent() else {
            throw NSError(domain: "MusterLaunchAgent", code: 2, userInfo: [NSLocalizedDescriptionKey: "the old musterd job did not unload"])
        }
    }

    private static func waitUntilAbsent() throws -> Bool {
        for _ in 0..<100 {
            if try inspectJob().state == .absent { return true }
            usleep(100_000)
        }
        return false
    }

    private static func inspectJob() throws -> Job {
        let result = try launchctl(["print", MacInstall.daemonService])
        guard let job = job(status: result.status, output: result.output) else {
            throw NSError(domain: "MusterLaunchAgent", code: Int(result.status), userInfo: [NSLocalizedDescriptionKey: result.output])
        }
        return job
    }

    private static func launchctl(_ arguments: [String]) throws -> (status: Int32, output: String) {
        try run("/bin/launchctl", arguments)
    }

    private static func run(_ path: String, _ arguments: [String]) throws -> (status: Int32, output: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        process.standardInput = FileHandle.nullDevice
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus, String(decoding: data, as: UTF8.self))
    }

    private static func posixError(_ what: String) -> NSError {
        NSError(domain: NSPOSIXErrorDomain, code: Int(errno), userInfo: [NSLocalizedDescriptionKey: "\(what): \(String(cString: strerror(errno)))"])
    }
}
#endif
