// OS-only installation adapter. No caller-selected paths, commands or UID.
import Darwin
import Foundation
import Security

enum LockedUseInstall {
    static let app = URL(fileURLWithPath: "/Applications/Muster.app")
    static let callerRequirement = "identifier \"dev.linsheng.musterd\" and anchor apple generic and certificate leaf[subject.OU] = \"89LGY6BD53\""
    static let appRequirement = "identifier \"dev.linsheng.muster\" and anchor apple generic and certificate leaf[subject.OU] = \"89LGY6BD53\""

    static func authorize(code: SecCode?, info: CallerInfo) -> String? {
        guard info.uid > 0, info.path == app.path + "/Contents/MacOS/musterd" else {
            return "plugin installation requires the installed user daemon"
        }
        guard let code else { return "missing caller audit-token code identity" }
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(callerRequirement as CFString, [], &requirement) == errSecSuccess,
              let requirement, SecCodeCheckValidity(code, [], requirement) == errSecSuccess else {
            return "plugin installer caller signature rejected"
        }
        return nil
    }

    // Run only fixed system tools with isolated environment. Bound child lifetime;
    // never leave an installer running after reporting failure to musterd.
    static func run(_ executable: String, _ arguments: [String], directory: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.currentDirectoryURL = directory
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": "/var/root", "TMPDIR": directory.path]
        let finished = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in finished.signal() }
        try process.run()
        if finished.wait(timeout: .now() + 60) == .timedOut {
            process.terminate()
            if finished.wait(timeout: .now() + 2) == .timedOut {
                kill(process.processIdentifier, SIGKILL)
                process.waitUntilExit()
            }
            throw NSError(domain: "MusterHelper", code: 1, userInfo: [NSLocalizedDescriptionKey: "plugin installation tool timed out"])
        }
        guard process.terminationStatus == 0 else {
            throw NSError(domain: "MusterHelper", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "plugin installation tool failed: \(executable) (\(process.terminationStatus))"])
        }
    }

    static func install(uid: uid_t) -> String? {
        guard geteuid() == 0, uid > 0, let user = getpwuid(uid), let home = user.pointee.pw_dir else {
            return "root helper and a local user account are required"
        }
        let socket = String(cString: home) + "/Library/Application Support/Muster/locked-use.sock"
        guard socket.utf8.count < 104, socket.hasPrefix("/") else { return "unsupported user socket path" }
        let fm = FileManager.default
        let base = URL(fileURLWithPath: "/Library/Application Support/Muster/HelperStaging")
        if let hazard = pathChainIsRootOwned(base.path) { return "unsafe staging directory: \(hazard)" }
        do {
            try fm.createDirectory(at: base, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            if let hazard = pathChainIsRootOwned(base.path) { return "unsafe staging directory: \(hazard)" }
            let temporary = base.appendingPathComponent(UUID().uuidString)
            try fm.createDirectory(at: temporary, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
            defer { try? fm.removeItem(at: temporary) }
            // Copy first and verify the root-private snapshot. Never execute the
            // script from the user-writable App after a separate signature check.
            let staged = temporary.appendingPathComponent("Muster.app")
            try fm.copyItem(at: app, to: staged)
            guard let entries = fm.enumerator(at: staged, includingPropertiesForKeys: [.isSymbolicLinkKey]) else {
                return "cannot inspect the staged App"
            }
            for case let entry as URL in entries {
                if try entry.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink == true,
                   !entry.resolvingSymlinksInPath().path.hasPrefix(staged.path + "/") {
                    return "staged App contains an external symlink"
                }
            }
            try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", "-R", "=" + appRequirement, staged.path], directory: temporary)
            let installer = staged.appendingPathComponent("Contents/Resources/LockedUse/install.py")
            let plugin = staged.appendingPathComponent("Contents/PlugIns/MusterLockedUse.bundle")
            guard installer.resolvingSymlinksInPath().path.hasPrefix(staged.path + "/"),
                  plugin.resolvingSymlinksInPath().path.hasPrefix(staged.path + "/") else {
                return "plugin resources escape the signed App"
            }
            try run("/usr/bin/python3", ["-I", installer.path, plugin.path, socket, String(uid), "--install"], directory: temporary)
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}
