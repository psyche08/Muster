// muster-helper — the privileged half of Muster's virtual network and the
// Locked Use plugin installer (ARCHITECTURE §3.3).
//
// A root LaunchDaemon owning the Mach service dev.linsheng.muster.helper,
// registered by Muster.app through SMAppService.daemon. It exists so that
// musterd can stay an unprivileged launchd USER agent (it must keep using the
// Secure Enclave, which only works in the login session) while the operations
// that genuinely need root — creating the utun device, installing its one
// route, and installing the Locked Use authorization plugin — happen behind an
// authenticated boundary.
//
// Topology:
//     musterd (Rust, uid 501)                 <- the only authorized BINARY
//                                                (not the only possible
//                                                 invoker — see below)
//       │ XPC, machServiceName, privileged
//     muster-helper (this, uid 0)
//       └─ creates the utun, hands the FD back, installs the vnet route
//
// The FD is the data plane: the helper hands the utun descriptor out over XPC
// and then closes its own copy, so musterd reads and writes the utun directly
// and no packet ever passes through this process. Consequently the kernel
// destroys the interface when musterd's descriptor closes — this helper
// cannot and must not hold a reference to it.
//
// Protocol (low-level C XPC — NSXPCConnection's public
// -setCodeSigningRequirement: would be nicer, but xpc_dictionary_set_fd is the
// only fd transport proven end to end in the spike):
//
//   {"op":"ping","nonce":str}
//       -> {ok, nonce, helper_pid, helper_euid, version, degraded,
//           allowed_cidr, requirement, uptime_s}
//   {"op":"open","cidr":str,"address":str,"mtu":int64}
//       -> {ok, ifname, fd}
//   {"op":"addRoute","prefix":str}      -> {ok}
//   {"op":"removeRoute","prefix":str}   -> {ok}
//   {"op":"close"}                      -> {ok}
//
// The route ops are refused (the route is installed at open() from the
// root-owned config); both their spellings are recognised so the refusal
// names them. The failure string is sent as "reason".
//
// Every failure answers {ok:false, reason:str}; the connection is never left
// guessing. A caller that fails code-signature verification gets a reply to
// every op saying so and is then cancelled — it never reaches a privileged
// code path, but it does learn the daemon is alive, which is what makes
// `--ping` a usable liveness probe for the installer.
//
// SECURITY MODEL
//   * The audit token from the XPC connection is the ONLY trustworthy caller
//     attribute. Never processIdentifier: pids are reused, and the race is
//     real. The token goes to SecCodeCopyGuestWithAttributes and then to
//     SecCodeCheckValidity against a pinned SecRequirement.
//   * The default requirement pins Developer ID team 89LGY6BD53 AND the
//     binary identifier, so only musterd's own code runs on the other end of
//     this connection. That is strictly more than a bare unix
//     socket with LOCAL_PEERCRED, which authenticates a uid and nothing else.
//
//     Be precise about what it does NOT buy, because the difference decides
//     how much the argument validation below has to carry: the check
//     authenticates the BINARY, not whoever invoked it. Any code running as
//     the login user — an npm postinstall, an editor extension — can exec the
//     genuine, correctly signed binary and drive it. (Verified against the
//     earlier Go/bridge caller: a well-formed `open` arrived here from a
//     process that was not the agent.)
//
//     So the signature pin bounds WHOSE CODE reaches root; it does not bound
//     WHO CAN ASK. What bounds the damage is everything below: the CIDR
//     confinement, one interface at a time, and one owner per route. Those
//     are load-bearing security controls, not tidiness.
//   * When the caller is only ad-hoc signed the requirement degrades to a
//     cdhash pin, which binds content but not location, plus optional uid and
//     path checks. That state is logged loudly on every connection.
//   * With neither a team nor a cdhash configured the helper FAILS CLOSED and
//     refuses every caller.
//   * Every argument is validated here, server side. An unchecked route API
//     is a full traffic-hijack primitive, so routes are confined to the
//     configured CIDR, 0.0.0.0/0 is rejected by name, and a route can only
//     ever point at the utun this same connection opened.
//   * At most ONE interface is live at a time, and a prefix bound by one
//     connection is never rebound by another. Without both, a second caller
//     could open its own utun and install a more specific route
//     (192.168.14.7/32 beats the agent's /24) to silently intercept the
//     virtual network. See admitOpen/admitRoute; --selftest pins them.
//
// Modes:
//   (no args)     run as the launchd daemon
//   --ping        connect to the service and print a live answer (installer)
//   --check       resolve config + requirement, print them, exit non-zero if
//                 the helper would fail closed
//   --selftest    run the pure argument-validation table (no root, no XPC)
//   --version, --help

import Darwin
import Foundation
import Security
import XPC

// MARK: - Constants the SDK does not export to Swift
//
// SIOC* and CTLIOCGINFO are _IOW/_IOWR function-like macros, which Swift
// cannot import. The values below were read out of a C probe on this SDK
// (macOS 26.6) and are re-derived by --selftest from the struct sizes Swift
// *can* see, so a struct-layout change breaks the test rather than the wire.

let kCTLIOCGINFO: UInt = 0xC064_4E03 // _IOWR('N',  3, struct ctl_info)     100
let kSIOCAIFADDR: UInt = 0x8040_691A // _IOW ('i', 26, struct ifaliasreq)    64
let kSIOCDIFADDR: UInt = 0x8020_6919 // _IOW ('i', 25, struct ifreq)         32
let kSIOCSIFMTU: UInt = 0x8020_6934 // _IOW ('i', 52, struct ifreq)         32
let kSIOCSIFFLAGS: UInt = 0x8020_6910 // _IOW ('i', 16, struct ifreq)         32
let kSIOCGIFFLAGS: UInt = 0xC020_6911 // _IOWR('i', 17, struct ifreq)         32

let kSYSPROTO_CONTROL: Int32 = 2
let kAF_SYS_CONTROL: UInt16 = 2
let kUTUN_OPT_IFNAME: Int32 = 2
let kUTUNControlName = "com.apple.net.utun_control"

// xpc_connection_create_mach_service flags.
let kXPCListener: UInt64 = 1 // XPC_CONNECTION_MACH_SERVICE_LISTENER
let kXPCPrivileged: UInt64 = 2 // XPC_CONNECTION_MACH_SERVICE_PRIVILEGED

let helperVersion = "1.1.0"
let defaultServiceName = "dev.linsheng.muster.helper"
let defaultConfPath = "/opt/muster/etc/nethelper.conf"

// MARK: - Logging

let startedAt = Date()
let isoFormatter: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return f
}()

func log(_ message: String) {
    print("[nethelper \(isoFormatter.string(from: Date())) pid=\(getpid())] \(message)")
}

func cstr(_ pointer: UnsafePointer<CChar>?) -> String {
    pointer.map { String(cString: $0) } ?? ""
}

func errnoText(_ code: Int32) -> String {
    "errno=\(code) (\(String(cString: strerror(code))))"
}

// MARK: - IPv4 prefixes
//
// Hand-rolled rather than using inet_addr: inet_addr accepts "10.1" and
// "0x0a000001", and a permissive parser in front of a route API is how you
// end up installing a prefix nobody asked for. inet_pton is strict.

struct V4Prefix: Hashable, CustomStringConvertible {
    var addr: UInt32 // host byte order
    var bits: Int

    var description: String { "\(V4Prefix.dotted(addr))/\(bits)" }

    static func dotted(_ value: UInt32) -> String {
        "\((value >> 24) & 0xFF).\((value >> 16) & 0xFF).\((value >> 8) & 0xFF).\(value & 0xFF)"
    }

    static func mask(_ bits: Int) -> UInt32 {
        bits <= 0 ? 0 : (bits >= 32 ? 0xFFFF_FFFF : ~UInt32(0) << (32 - bits))
    }

    var network: UInt32 { addr & V4Prefix.mask(bits) }
    var broadcast: UInt32 { network | ~V4Prefix.mask(bits) }
    var isCanonical: Bool { addr == network }
    var masked: V4Prefix { V4Prefix(addr: network, bits: bits) }

    /// True when `other` lies entirely inside this prefix.
    func contains(_ other: V4Prefix) -> Bool {
        other.bits >= bits && (other.network & V4Prefix.mask(bits)) == network
    }

    func containsAddress(_ value: UInt32) -> Bool {
        (value & V4Prefix.mask(bits)) == network
    }
}

/// Strict dotted-quad parse. Returns host byte order.
///
/// Hand-written rather than inet_pton: this SDK's inet_pton still accepts
/// leading zeros ("192.168.014.0"), and two spellings of one address in front
/// of an allow-list check is exactly the kind of gap a validator must not
/// have. Only the canonical form is accepted.
func parseV4Address(_ text: String) -> UInt32? {
    let parts = text.split(separator: ".", omittingEmptySubsequences: false)
    guard parts.count == 4 else { return nil }
    var value: UInt32 = 0
    for part in parts {
        guard !part.isEmpty, part.count <= 3, part.allSatisfy({ $0.isASCII && $0.isNumber }) else { return nil }
        if part.count > 1, part.hasPrefix("0") { return nil }
        guard let octet = UInt32(part), octet <= 255 else { return nil }
        value = (value << 8) | octet
    }
    return value
}

/// Strict "a.b.c.d/n" parse. No whitespace, no bare address, no bits > 32.
func parseV4Prefix(_ text: String) -> V4Prefix? {
    let parts = text.split(separator: "/", omittingEmptySubsequences: false)
    guard parts.count == 2 else { return nil }
    guard let addr = parseV4Address(String(parts[0])) else { return nil }
    let bitsText = String(parts[1])
    // Reject "/024" and "/+24": Int("024") == 24, which would silently accept
    // two spellings of one prefix.
    guard !bitsText.isEmpty, bitsText.allSatisfy({ $0.isASCII && $0.isNumber }),
          bitsText.count <= 2, bitsText == "0" || !bitsText.hasPrefix("0"),
          let bits = Int(bitsText), bits >= 0, bits <= 32
    else { return nil }
    return V4Prefix(addr: addr, bits: bits)
}

/// A two-case result carrying a human-readable failure string. Result<T, String>
/// would need String to conform to Error, and a retroactive conformance on a
/// standard-library type is not worth the blast radius.
enum Outcome<Value> {
    case success(Value)
    case failure(String)
}

// MARK: - Configuration
//
// Compiled defaults are the strict production values. The config file can only
// ever be read when root owns every directory on the way to it (see
// pathChainIsRootOwned); an untrusted config is ignored, not merged, so the
// worst an attacker who controls /opt/muster achieves is the strict default.

struct HelperConfig {
    var allowedCIDR = V4Prefix(addr: 0xC0A8_0D00, bits: 24) // 192.168.13.0/24
    var callerTeam = "89LGY6BD53"
    var callerIdentifier = "dev.linsheng.musterd"
    var callerCDHash = ""
    var callerPath = "/Applications/Muster.app/Contents/MacOS/musterd"
    var callerUID: uid_t? = nil
    var source = "compiled defaults"
}

enum ConfigProblem: Error, CustomStringConvertible {
    case unreadable(String)
    case insecurePath(String)
    case syntax(Int, String)

    var description: String {
        switch self {
        case let .unreadable(why): return why
        case let .insecurePath(why): return why
        case let .syntax(line, why): return "line \(line): \(why)"
        }
    }
}

/// Ancestors of `path`, from `/` downwards.
func ancestorChain(_ path: String) -> [String] {
    var components: [String] = []
    var cursor = URL(fileURLWithPath: path).standardizedFileURL
    while cursor.path != "/" {
        components.append(cursor.path)
        let parent = cursor.deletingLastPathComponent()
        if parent.path == cursor.path { break }
        cursor = parent
    }
    components.append("/")
    return components.reversed()
}

/// Every component from `/` down to `path` must be owned by root and writable
/// by nobody else. Anything less and a non-root user can swap the file out from
/// under a process that reads it as root.
///
/// A symlink is not itself disqualifying — /var and /tmp are root-owned
/// symlinks on every Mac. What matters is that no non-root user can replace the
/// link or anything it points at, so both the lexical chain and the physically
/// resolved chain are checked.
func pathChainIsRootOwned(_ path: String) -> String? {
    func inspect(_ chain: [String]) -> String? {
        for candidate in chain {
            var info = stat()
            guard lstat(candidate, &info) == 0 else {
                // A component that does not exist yet cannot be a live hazard,
                // but everything below it is unverifiable, so stop here.
                return nil
            }
            if info.st_uid != 0 {
                return "\(candidate) is owned by uid \(info.st_uid), not root"
            }
            if (info.st_mode & (S_IWGRP | S_IWOTH)) != 0 {
                return String(format: "%@ is group/other writable (mode %o)", candidate, info.st_mode & 0o7777)
            }
        }
        return nil
    }
    if let hazard = inspect(ancestorChain(path)) { return hazard }
    let resolved = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
    if resolved != path { return inspect(ancestorChain(resolved)) }
    return nil
}

/// `enforceTrust` is passed `geteuid() == 0`. The ownership check exists to stop
/// a non-root user from steering a root process; a helper that is not root has
/// no privilege to steer, so demanding a root-owned chain there would only make
/// the thing untestable outside a real install.
func loadConfig(path: String, enforceTrust: Bool = geteuid() == 0) -> (HelperConfig, [String]) {
    var config = HelperConfig()
    var notes: [String] = []

    var info = stat()
    if lstat(path, &info) != 0 {
        notes.append("config \(path) not present; using compiled defaults")
        return (config, notes)
    }
    if let hazard = pathChainIsRootOwned(path) {
        if enforceTrust {
            notes.append("REFUSING config \(path): \(hazard). Using compiled defaults. "
                + "Fix with: sudo chown root:wheel <dir> && sudo chmod 755 <dir>")
            return (config, notes)
        }
        notes.append("config \(path) is on an untrusted path (\(hazard)), but this process is not root, "
            + "so there is no privilege to protect; reading it anyway")
    }
    guard let text = try? String(contentsOfFile: path, encoding: .utf8) else {
        notes.append("REFUSING config \(path): unreadable. Using compiled defaults.")
        return (config, notes)
    }

    // Parse into a scratch copy: a config with ANY error is rejected whole. A
    // typo must not silently leave a security key at its default while the
    // operator believes they changed it.
    var scratch = config
    var problems: [String] = []
    for (index, rawLine) in text.components(separatedBy: .newlines).enumerated() {
        let line = rawLine.trimmingCharacters(in: .whitespaces)
        if line.isEmpty || line.hasPrefix("#") { continue }
        guard let eq = line.firstIndex(of: "=") else {
            problems.append("line \(index + 1): expected key = value")
            continue
        }
        let key = String(line[line.startIndex..<eq]).trimmingCharacters(in: .whitespaces)
        let value = String(line[line.index(after: eq)...]).trimmingCharacters(in: .whitespaces)
        switch key {
        case "allowed_cidr":
            guard let prefix = parseV4Prefix(value), prefix.isCanonical, prefix.bits >= 8, prefix.bits <= 30 else {
                problems.append("line \(index + 1): allowed_cidr \(value.isEmpty ? "(empty)" : value) is not a canonical IPv4 network with /8../30")
                continue
            }
            scratch.allowedCIDR = prefix
        case "caller_team":
            guard value.isEmpty || (value.count == 10 && value.allSatisfy { $0.isASCII && ($0.isUppercase || $0.isNumber) }) else {
                problems.append("line \(index + 1): caller_team must be 10 uppercase alphanumerics")
                continue
            }
            scratch.callerTeam = value
        case "caller_identifier":
            guard !value.isEmpty, value.count <= 128,
                  value.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "." || $0 == "-" || $0 == "_") })
            else {
                problems.append("line \(index + 1): caller_identifier has characters outside [A-Za-z0-9._-]")
                continue
            }
            scratch.callerIdentifier = value
        case "caller_cdhash":
            let lowered = value.lowercased()
            guard lowered.isEmpty || ((lowered.count == 40 || lowered.count == 64)
                && lowered.allSatisfy { $0.isHexDigit && $0.isASCII })
            else {
                problems.append("line \(index + 1): caller_cdhash must be 40 or 64 hex characters")
                continue
            }
            scratch.callerCDHash = lowered
        case "caller_path":
            guard value.hasPrefix("/") || value.isEmpty else {
                problems.append("line \(index + 1): caller_path must be absolute")
                continue
            }
            scratch.callerPath = value
        case "caller_uid":
            if value.isEmpty { scratch.callerUID = nil; continue }
            guard let parsed = UInt32(value) else {
                problems.append("line \(index + 1): caller_uid must be a number")
                continue
            }
            scratch.callerUID = uid_t(parsed)
        default:
            problems.append("line \(index + 1): unknown key \"\(key)\"")
        }
    }
    if !problems.isEmpty {
        notes.append("REFUSING config \(path): " + problems.joined(separator: "; ") + ". Using compiled defaults.")
        return (config, notes)
    }
    scratch.source = path
    config = scratch
    notes.append("config loaded from \(path)")
    return (config, notes)
}

// MARK: - Caller requirement

struct CallerPolicy {
    var requirement: String
    var degraded: Bool
    var expectPath: String
    var expectUID: uid_t?
}

enum PolicyResolution {
    case ok(CallerPolicy)
    case failClosed(String)
}

func resolvePolicy(_ config: HelperConfig) -> PolicyResolution {
    // Both configured: the team pin is strictly stronger, so it wins and the
    // cdhash is ignored rather than OR-ed in. An OR would let a stale ad-hoc
    // build keep root access after the signed one shipped.
    if !config.callerTeam.isEmpty {
        let requirement = "identifier \"\(config.callerIdentifier)\" "
            + "and anchor apple generic "
            + "and certificate leaf[subject.OU] = \"\(config.callerTeam)\""
        return .ok(CallerPolicy(requirement: requirement, degraded: false,
                                expectPath: "", expectUID: config.callerUID))
    }
    if !config.callerCDHash.isEmpty {
        // A cdhash pin binds CONTENT, not location: the spike proved a copy of
        // the same binary at /tmp still satisfies it. So the path check below
        // is defence in depth, not the gate.
        return .ok(CallerPolicy(requirement: "cdhash H\"\(config.callerCDHash)\"", degraded: true,
                                expectPath: config.callerPath, expectUID: config.callerUID))
    }
    return .failClosed("neither caller_team nor caller_cdhash is configured")
}

/// Compile the requirement now so a malformed string fails at startup rather
/// than silently rejecting (OSStatus -67052) at the first real connection.
func compileRequirement(_ text: String) -> Outcome<SecRequirement> {
    var requirement: SecRequirement?
    let status = SecRequirementCreateWithString(text as CFString, [], &requirement)
    guard status == errSecSuccess, let requirement else {
        return .failure("SecRequirementCreateWithString failed OSStatus=\(status) for: \(text)")
    }
    return .success(requirement)
}

// MARK: - Caller identity from the audit token

typealias GetAuditTokenFn = @convention(c) (xpc_connection_t, UnsafeMutablePointer<audit_token_t>) -> Void
let rtldDefault = UnsafeMutableRawPointer(bitPattern: -2)

/// xpc_connection_get_audit_token is in libxpc but not in the public headers.
/// dlsym is the only way to reach it from the C XPC API; nil means we cannot
/// identify the caller at all, which must fail closed.
func auditToken(of connection: xpc_connection_t) -> audit_token_t? {
    guard let symbol = dlsym(rtldDefault, "xpc_connection_get_audit_token") else { return nil }
    let function = unsafeBitCast(symbol, to: GetAuditTokenFn.self)
    var token = audit_token_t()
    function(connection, &token)
    return token
}

struct CallerInfo {
    var pid: pid_t
    var uid: uid_t
    var path: String
    var cdhash: String
    var teamID: String
}

func describeCaller(_ token: audit_token_t) -> (SecCode?, CallerInfo) {
    var mutable = token
    // audit_token_t.val: [0]=auid [1]=euid [2]=egid [3]=ruid [4]=rgid [5]=pid
    let uid = uid_t(mutable.val.1)
    let pid = pid_t(bitPattern: mutable.val.5)
    let data = withUnsafeBytes(of: &mutable) { Data($0) }
    let attributes: [CFString: Any] = [kSecGuestAttributeAudit: data as CFData]
    var code: SecCode?
    let status = SecCodeCopyGuestWithAttributes(nil, attributes as CFDictionary, [], &code)
    var path = "?", cdhash = "?", team = "-"
    if status == errSecSuccess, let code {
        // SecCodeCopyPath and the signing dictionary only accept a static code.
        var staticCode: SecStaticCode?
        if SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode {
            var info: CFDictionary?
            let flags = SecCSFlags(rawValue: kSecCSSigningInformation | kSecCSInternalInformation)
            if SecCodeCopySigningInformation(staticCode, flags, &info) == errSecSuccess,
               let dictionary = info as? [String: Any] {
                // The cdhash lives under "unique", not "cdhash".
                if let unique = dictionary["unique"] as? Data {
                    cdhash = unique.map { String(format: "%02x", $0) }.joined()
                } else if let hashes = dictionary["cdhashes"] as? [Data], let last = hashes.last {
                    cdhash = last.map { String(format: "%02x", $0) }.joined()
                }
                if let teamID = dictionary["teamid"] as? String { team = teamID }
            }
            var url: CFURL?
            if SecCodeCopyPath(staticCode, [], &url) == errSecSuccess, let url {
                path = (url as URL).path
            }
        }
    } else {
        path = "SecCodeCopyGuestWithAttributes failed OSStatus=\(status)"
    }
    return (code, CallerInfo(pid: pid, uid: uid, path: path, cdhash: cdhash, teamID: team))
}

/// nil means the caller is authorized; a string is the rejection reason.
func rejectionReason(code: SecCode?, info: CallerInfo,
                     requirement: SecRequirement, policy: CallerPolicy) -> String? {
    guard let code else { return "no SecCode for the audit token" }
    var error: Unmanaged<CFError>?
    let status = SecCodeCheckValidityWithErrors(code, [], requirement, &error)
    if status != errSecSuccess {
        let detail = error?.takeRetainedValue().localizedDescription ?? "OSStatus=\(status)"
        return "code signature check failed: \(detail) (OSStatus=\(status))"
    }
    if let expected = policy.expectUID, info.uid != expected {
        return "caller uid \(info.uid) != expected \(expected)"
    }
    if !policy.expectPath.isEmpty, info.path != policy.expectPath {
        return "caller path \(info.path) != expected \(policy.expectPath)"
    }
    return nil
}

// MARK: - utun

struct UTUN {
    var fd: Int32
    var name: String
}

func createUTUN() -> Outcome<UTUN> {
    // socket() and CTLIOCGINFO work unprivileged; only connect() needs root.
    let fd = socket(AF_SYSTEM, SOCK_DGRAM, kSYSPROTO_CONTROL)
    guard fd >= 0 else { return .failure("socket(AF_SYSTEM): \(errnoText(errno))") }
    var info = ctl_info()
    withUnsafeMutableBytes(of: &info.ctl_name) { raw in
        _ = kUTUNControlName.withCString {
            strlcpy(raw.baseAddress!.assumingMemoryBound(to: CChar.self), $0, raw.count)
        }
    }
    guard ioctl(fd, kCTLIOCGINFO, &info) >= 0 else {
        let text = errnoText(errno); close(fd)
        return .failure("ioctl(CTLIOCGINFO \(kUTUNControlName)): \(text)")
    }
    var address = sockaddr_ctl()
    address.sc_len = UInt8(MemoryLayout<sockaddr_ctl>.size)
    address.sc_family = UInt8(AF_SYSTEM)
    address.ss_sysaddr = kAF_SYS_CONTROL
    address.sc_id = info.ctl_id
    address.sc_unit = 0 // let the kernel pick the first free utunN
    let connected = withUnsafePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            connect(fd, $0, socklen_t(MemoryLayout<sockaddr_ctl>.size))
        }
    }
    guard connected >= 0 else {
        let code = errno, text = errnoText(errno); close(fd)
        let hint = code == EPERM ? " (this is the step that requires root — is the helper running as a LaunchDaemon?)" : ""
        return .failure("connect(utun_control): \(text)\(hint)")
    }
    var nameBuffer = [CChar](repeating: 0, count: 64)
    var nameLength = socklen_t(nameBuffer.count)
    guard getsockopt(fd, kSYSPROTO_CONTROL, kUTUN_OPT_IFNAME, &nameBuffer, &nameLength) >= 0 else {
        let text = errnoText(errno); close(fd)
        return .failure("getsockopt(UTUN_OPT_IFNAME): \(text)")
    }
    let name = String(cString: nameBuffer)
    guard name.hasPrefix("utun") else {
        close(fd)
        return .failure("kernel returned unexpected interface name \(name)")
    }
    return .success(UTUN(fd: fd, name: name))
}

func withInterfaceName<T>(_ name: String, _ body: (inout ifreq) -> T) -> T {
    var request = ifreq()
    withUnsafeMutableBytes(of: &request.ifr_name) { raw in
        _ = name.withCString {
            strncpy(raw.baseAddress!.assumingMemoryBound(to: CChar.self), $0, raw.count)
        }
    }
    return body(&request)
}

func inetSockaddr(_ value: UInt32) -> sockaddr_in {
    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_addr.s_addr = value.bigEndian
    return address
}

/// Address, netmask, MTU and IFF_UP, all through ioctl. private-tunnel shelled
/// out to /sbin/ifconfig here; one fewer exec inside a root process is one
/// fewer place for a hijacked PATH or an argument-quoting bug to matter.
func configureInterface(name: String, address: UInt32, bits: Int, mtu: Int32) -> String? {
    let control = socket(AF_INET, SOCK_DGRAM, 0)
    guard control >= 0 else { return "socket(AF_INET): \(errnoText(errno))" }
    defer { close(control) }

    var alias = ifaliasreq()
    withUnsafeMutableBytes(of: &alias.ifra_name) { raw in
        _ = name.withCString {
            strncpy(raw.baseAddress!.assumingMemoryBound(to: CChar.self), $0, raw.count)
        }
    }
    var local = inetSockaddr(address)
    // utun is point-to-point: the destination is the local address itself, the
    // same shape `ifconfig utunN inet A A netmask M` produces.
    var destination = inetSockaddr(address)
    var netmask = inetSockaddr(V4Prefix.mask(bits))
    memcpy(&alias.ifra_addr, &local, MemoryLayout<sockaddr_in>.size)
    memcpy(&alias.ifra_broadaddr, &destination, MemoryLayout<sockaddr_in>.size)
    memcpy(&alias.ifra_mask, &netmask, MemoryLayout<sockaddr_in>.size)
    guard ioctl(control, kSIOCAIFADDR, &alias) >= 0 else {
        return "ioctl(SIOCAIFADDR \(V4Prefix.dotted(address))/\(bits)): \(errnoText(errno))"
    }

    let mtuError: String? = withInterfaceName(name) { request in
        request.ifr_ifru.ifru_mtu = mtu
        return ioctl(control, kSIOCSIFMTU, &request) >= 0 ? nil : "ioctl(SIOCSIFMTU \(mtu)): \(errnoText(errno))"
    }
    if let mtuError { return mtuError }

    let flagsError: String? = withInterfaceName(name) { request in
        guard ioctl(control, kSIOCGIFFLAGS, &request) >= 0 else {
            return "ioctl(SIOCGIFFLAGS): \(errnoText(errno))"
        }
        request.ifr_ifru.ifru_flags |= Int16(IFF_UP)
        return ioctl(control, kSIOCSIFFLAGS, &request) >= 0 ? nil : "ioctl(SIOCSIFFLAGS up): \(errnoText(errno))"
    }
    return flagsError
}

/// Best-effort neutering of an interface at teardown.
///
/// The helper cannot destroy the utun: the kernel keeps it alive while musterd
/// holds the descriptor, and holding a second copy here just to be able to
/// close it would defeat that same lifetime rule. Removing the address and
/// clearing IFF_UP makes the interface inert even if the descriptor outlives
/// the control connection. ENXIO/EADDRNOTAVAIL simply mean the interface has
/// already gone, which is the normal case.
///
/// `address` is the one we assigned. SIOCDIFADDR looks the address up to
/// decide which ifaddr to unlink, so passing the actual value matters: an
/// all-zero sockaddr matches nothing and the delete silently no-ops, leaving a
/// configured address on an interface we believe we released.
func teardownInterface(name: String, address: UInt32?) {
    let control = socket(AF_INET, SOCK_DGRAM, 0)
    guard control >= 0 else { return }
    defer { close(control) }
    var alias = ifreq()
    withUnsafeMutableBytes(of: &alias.ifr_name) { raw in
        _ = name.withCString {
            strncpy(raw.baseAddress!.assumingMemoryBound(to: CChar.self), $0, raw.count)
        }
    }
    withUnsafeMutableBytes(of: &alias.ifr_ifru) { raw in
        var target = inetSockaddr(address ?? 0)
        memcpy(raw.baseAddress!, &target, MemoryLayout<sockaddr_in>.size)
    }
    if ioctl(control, kSIOCDIFADDR, &alias) < 0, errno != ENXIO, errno != EADDRNOTAVAIL {
        log("  teardown: SIOCDIFADDR \(name): \(errnoText(errno))")
    }
    let downError: String? = withInterfaceName(name) { request in
        guard ioctl(control, kSIOCGIFFLAGS, &request) >= 0 else { return nil }
        request.ifr_ifru.ifru_flags &= ~Int16(IFF_UP)
        return ioctl(control, kSIOCSIFFLAGS, &request) >= 0 ? nil : errnoText(errno)
    }
    if let downError { log("  teardown: SIOCSIFFLAGS down \(name): \(downError)") }
}

// MARK: - Routing socket
//
// Ported from tools/private-services/internal/privatenet/routesocket_darwin.go.
// route(8) is deliberately not used: no fork, no text round trip, and no
// argument string that a bug could let a caller influence.

let rtmMsglen = 0, rtmVersion = 2, rtmType = 3, rtmFlags = 8
let rtmAddrs = 12, rtmPid = 16, rtmSeq = 20
let rtMsghdrSize = MemoryLayout<rt_msghdr>.size // 92
let sockaddrInSize = MemoryLayout<sockaddr_in>.size // 16
let sockaddrDlSize = MemoryLayout<sockaddr_dl>.size // 20

var routeSequence: Int32 = 0

func putNative<T: FixedWidthInteger>(_ buffer: inout [UInt8], _ offset: Int, _ value: T) {
    withUnsafeBytes(of: value) { raw in
        for index in 0..<raw.count { buffer[offset + index] = raw[index] }
    }
}

func putInetSockaddr(_ buffer: inout [UInt8], _ offset: Int, _ value: UInt32) {
    buffer[offset] = UInt8(sockaddrInSize)
    buffer[offset + 1] = UInt8(AF_INET)
    let bytes = value.bigEndian
    withUnsafeBytes(of: bytes) { raw in
        for index in 0..<4 { buffer[offset + 4 + index] = raw[index] }
    }
}

/// The sockaddr_dl that means "-interface <name>": an index is all the kernel
/// needs, so no link-layer address is written.
func putLinkSockaddr(_ buffer: inout [UInt8], _ offset: Int, _ index: UInt16) {
    buffer[offset] = UInt8(sockaddrDlSize)
    buffer[offset + 1] = UInt8(AF_LINK)
    putNative(&buffer, offset + 2, index)
}

func marshalRouteChange(type: UInt8, prefix: V4Prefix, interfaceIndex: UInt16, sequence: Int32) -> [UInt8] {
    let canonical = prefix.masked
    var addrs = UInt32(RTA_DST | RTA_NETMASK)
    var size = rtMsghdrSize + 2 * sockaddrInSize
    if interfaceIndex > 0 {
        addrs |= UInt32(RTA_GATEWAY)
        size += sockaddrDlSize
    }
    var message = [UInt8](repeating: 0, count: size)
    putNative(&message, rtmMsglen, UInt16(size))
    message[rtmVersion] = UInt8(RTM_VERSION)
    message[rtmType] = type
    putNative(&message, rtmFlags, UInt32(RTF_UP | RTF_STATIC))
    putNative(&message, rtmAddrs, addrs)
    putNative(&message, rtmPid, UInt32(bitPattern: getpid()))
    putNative(&message, rtmSeq, UInt32(bitPattern: sequence))
    // Sockaddrs follow the header in RTAX order: destination, gateway, netmask.
    var cursor = rtMsghdrSize
    putInetSockaddr(&message, cursor, canonical.network)
    cursor += sockaddrInSize
    if interfaceIndex > 0 {
        putLinkSockaddr(&message, cursor, interfaceIndex)
        cursor += sockaddrDlSize
    }
    putInetSockaddr(&message, cursor, V4Prefix.mask(canonical.bits))
    return message
}

/// Writes one routing message and reports the kernel's synchronous verdict.
/// EEXIST means the prefix is already bound; ESRCH means it is not in the table.
func writeRouteMessage(type: UInt8, prefix: V4Prefix, interfaceIndex: UInt16) -> Int32 {
    let fd = socket(PF_ROUTE, SOCK_RAW, AF_INET)
    guard fd >= 0 else { return errno }
    defer { close(fd) }
    routeSequence &+= 1
    let message = marshalRouteChange(type: type, prefix: prefix,
                                     interfaceIndex: interfaceIndex, sequence: routeSequence)
    let written = message.withUnsafeBytes { write(fd, $0.baseAddress, $0.count) }
    if written < 0 { return errno }
    return 0
}

func addRoute(prefix: V4Prefix, interfaceName: String) -> String? {
    let index = if_nametoindex(interfaceName)
    guard index != 0 else { return "interface \(interfaceName) has no index (already gone?)" }
    var code = writeRouteMessage(type: UInt8(RTM_ADD), prefix: prefix, interfaceIndex: UInt16(index))
    if code == EEXIST {
        // The route table changes underneath us — a VPN utun, interface churn,
        // or a previous generation that has not noticed EOF yet all leave the
        // prefix bound to something stale, and RTM_ADD then fails instead of
        // correcting it. Drop whatever is bound and add ours back.
        let removal = writeRouteMessage(type: UInt8(RTM_DELETE), prefix: prefix, interfaceIndex: 0)
        if removal != 0, removal != ESRCH {
            return "route \(prefix) exists and delete failed: \(errnoText(removal))"
        }
        code = writeRouteMessage(type: UInt8(RTM_ADD), prefix: prefix, interfaceIndex: UInt16(index))
    }
    if code != 0 { return "RTM_ADD \(prefix) -> \(interfaceName): \(errnoText(code))" }
    return nil
}

func removeRoute(prefix: V4Prefix) -> String? {
    let code = writeRouteMessage(type: UInt8(RTM_DELETE), prefix: prefix, interfaceIndex: 0)
    if code == 0 || code == ESRCH { return nil }
    return "RTM_DELETE \(prefix): \(errnoText(code))"
}

// MARK: - Argument validation (pure; exercised by --selftest)

/// The route ops have a hyphenated spelling too (from the earlier Go caller).
/// Both map onto one canonical name so the refusal is the same either way.
/// Pure, so --selftest can pin the contract.
func canonicalOp(_ raw: String) -> String {
    switch raw {
    case "add-route": return "addRoute"
    case "remove-route": return "removeRoute"
    default: return raw
    }
}

enum OpenArgs {
    case ok(prefix: V4Prefix, address: UInt32, mtu: Int32)
    case bad(String)
}

let minMTU: Int64 = 576
let maxMTU: Int64 = 9000

/// Validates the only two things a client may still supply: the address it
/// wants on the interface, and the MTU. The prefix is `allowed` — read from the
/// root-owned config — and is never taken from the caller, so there is no
/// client-supplied prefix anywhere in the routing path.
func validateOpen(address: String, mtu: Int64, allowed: V4Prefix) -> OpenArgs {
    let prefix = allowed
    guard let value = parseV4Address(address) else {
        return .bad("address \"\(address)\" is not a valid IPv4 address")
    }
    guard allowed.containsAddress(value) else {
        return .bad("address \(V4Prefix.dotted(value)) is outside \(allowed)")
    }
    guard value != allowed.network else {
        return .bad("address \(V4Prefix.dotted(value)) is the network address of \(allowed)")
    }
    guard allowed.bits > 30 || value != allowed.broadcast else {
        return .bad("address \(V4Prefix.dotted(value)) is the broadcast address of \(allowed)")
    }
    guard mtu >= minMTU, mtu <= maxMTU else {
        return .bad("mtu \(mtu) is outside \(minMTU)..\(maxMTU)")
    }
    return .ok(prefix: prefix, address: value, mtu: Int32(mtu))
}

enum RouteArgs {
    case ok(V4Prefix)
    case bad(String)
}

/// The most dangerous API in the helper. An unchecked route write is a
/// traffic-hijack primitive: bind 0.0.0.0/0 to a caller-owned utun and every
/// packet the Mac sends goes to the attacker.
func validateRoute(prefix text: String, allowed: V4Prefix) -> RouteArgs {
    guard let prefix = parseV4Prefix(text) else {
        return .bad("prefix \"\(text)\" is not a valid IPv4 prefix")
    }
    if prefix.bits == 0 {
        return .bad("refusing the default route \(prefix): the helper never touches 0.0.0.0/0")
    }
    guard prefix.isCanonical else {
        return .bad("prefix \(prefix) is not a network address")
    }
    guard prefix.bits >= allowed.bits else {
        return .bad("prefix \(prefix) is shorter than the configured virtual network \(allowed)")
    }
    guard allowed.contains(prefix) else {
        return .bad("prefix \(prefix) is outside the configured virtual network \(allowed)")
    }
    return .ok(prefix)
}

// MARK: - Session state

final class Session {
    let peer: xpc_connection_t
    let info: CallerInfo
    var authorized: Bool
    var rejection: String?
    var interfaceName: String?
    var address: UInt32?
    var routes: [V4Prefix] = []
    var closed = false

    init(peer: xpc_connection_t, info: CallerInfo, authorized: Bool, rejection: String?) {
        self.peer = peer
        self.info = info
        self.authorized = authorized
        self.rejection = rejection
    }
}

let workQueue = DispatchQueue(label: "dev.linsheng.muster.nethelper")

/// The one interface this helper has live, if any.
struct LiveInterface {
    var pid: pid_t
    var name: String
    var address: UInt32
}

/// muster's design is one agent, one utun per virtual network, so a SECOND
/// concurrent open is never legitimate — and admitting one is what turns a
/// bounded blast radius into a full hijack of the virtual network.
///
/// The reason this matters is spelled out at the top of this file: the caller
/// check authenticates the BINARY, not whoever invoked it, and any code
/// running as the login user can execute the genuine caller binary as a
/// subprocess. A second caller that got its own utun would need no EEXIST
/// race to steal traffic — a more specific route (192.168.14.7/32 beats the
/// agent's /24) is enough, and the agent would never see it. Denying the
/// interface denies the route, because addRoute requires one on the same
/// connection.
///
/// First open wins; every later one is refused loudly. Last-writer-wins would
/// be a SILENT takeover, which is strictly worse than a logged refusal.
var liveInterface: LiveInterface?

/// Prefix → the pid that installed it. EEXIST recovery in addRoute() drops
/// whatever is bound and rebinds it, which is correct for a route left behind
/// by a dead generation and a hijack primitive for one that is still owned.
/// This table is what tells those two apart.
var routeOwners: [V4Prefix: pid_t] = [:]

/// Pure admission rule for `open`, so --selftest can pin it. nil admits; a
/// string is the refusal reason.
func admitOpen(live: LiveInterface?, requestedBy pid: pid_t, address: UInt32) -> String? {
    guard let live else { return nil }
    if live.address == address {
        return "address \(V4Prefix.dotted(address)) is already live on \(live.name) (held by pid \(live.pid))"
    }
    return "this helper already has \(live.name) live for pid \(live.pid) — one virtual "
        + "interface at a time; refusing a second one for pid \(pid)"
}

/// Pure admission rule for `addRoute`. A prefix bound by a DIFFERENT live
/// connection is never rebound: that is the traffic-steal case.
func admitRoute(owners: [V4Prefix: pid_t], prefix: V4Prefix, requestedBy pid: pid_t) -> String? {
    guard let owner = owners[prefix], owner != pid else { return nil }
    return "route \(prefix) is already bound by pid \(owner); refusing to rebind it"
}

// MARK: - Daemon

func runDaemon(service: String, confPath: String) -> Never {
    setvbuf(stdout, nil, _IOLBF, 0)
    setvbuf(stderr, nil, _IOLBF, 0)

    log("muster-net-helper \(helperVersion) starting: service=\(service) euid=\(geteuid()) uid=\(getuid())")
    if geteuid() != 0 {
        log("WARNING: not running as root. utun creation and route writes will fail with EPERM. "
            + "The helper is only useful as a LaunchDaemon in the system domain.")
    }

    let (config, notes) = loadConfig(path: confPath)
    for note in notes { log(note) }
    log("allowed virtual network = \(config.allowedCIDR) (source: \(config.source))")

    var policy: CallerPolicy?
    var requirement: SecRequirement?
    switch resolvePolicy(config) {
    case let .ok(resolved):
        switch compileRequirement(resolved.requirement) {
        case let .success(compiled):
            policy = resolved
            requirement = compiled
            if resolved.degraded {
                log("############################################################")
                log("# DEGRADED CALLER VERIFICATION")
                log("# No Developer ID team is configured, so the caller is")
                log("# authenticated by a pinned cdhash plus uid/path only:")
                log("#   requirement = \(resolved.requirement)")
                log("#   expect path = \(resolved.expectPath.isEmpty ? "(any)" : resolved.expectPath)")
                log("#   expect uid  = \(resolved.expectUID.map(String.init) ?? "(any)")")
                log("# A cdhash pin binds the binary's CONTENT but not its")
                log("# location. Ship a Developer ID signed musterd and set")
                log("# caller_team to restore full verification.")
                log("############################################################")
            } else {
                log("caller requirement = \(resolved.requirement)")
            }
        case let .failure(why):
            log("FAIL CLOSED: \(why)")
        }
    case let .failClosed(why):
        log("FAIL CLOSED: \(why); every caller will be refused")
    }

    func handle(_ peer: xpc_connection_t) {
        guard let token = auditToken(of: peer) else {
            log("REJECT: xpc_connection_get_audit_token unavailable; cannot identify caller")
            xpc_connection_cancel(peer)
            return
        }
        let (code, info) = describeCaller(token)
        var rejection: String?
        if let policy, let requirement {
            rejection = rejectionReason(code: code, info: info, requirement: requirement, policy: policy)
        } else {
            rejection = "helper is failing closed: no usable caller requirement"
        }
        let session = Session(peer: peer, info: info, authorized: rejection == nil, rejection: rejection)

        log("connection pid=\(info.pid) uid=\(info.uid) team=\(info.teamID)")
        log("  path   = \(info.path)")
        log("  cdhash = \(info.cdhash)")
        if let rejection {
            log("  VERDICT = REJECT (\(rejection))")
        } else {
            log("  VERDICT = ACCEPT\(policy?.degraded == true ? " (DEGRADED verification)" : "")")
        }

        /// Release everything this connection owns. Idempotent, so it is safe
        /// to call from an explicit "close" and again when the connection
        /// invalidates — the second call finds nothing and stays quiet.
        func teardown(_ why: String) {
            session.closed = true
            guard session.interfaceName != nil || !session.routes.isEmpty else { return }
            log("teardown for pid=\(info.pid) (\(why))")
            for prefix in session.routes.reversed() {
                if let error = removeRoute(prefix: prefix) {
                    log("  route \(prefix): \(error)")
                } else {
                    log("  removed route \(prefix)")
                }
                // Only ever forget an entry this session owns: a stale
                // teardown must not release a prefix somebody else now holds.
                if routeOwners[prefix] == info.pid { routeOwners.removeValue(forKey: prefix) }
            }
            session.routes.removeAll()
            if let name = session.interfaceName {
                // Pass the address we assigned: SIOCDIFADDR matches on it.
                teardownInterface(name: name, address: session.address)
                log("  released \(name) (the utun itself disappears when the agent closes its descriptor)")
                // Release the single-interface slot only if it is still ours;
                // a slow teardown must not evict a successor that already
                // took the slot.
                if liveInterface?.name == name { liveInterface = nil }
                session.interfaceName = nil
            }
            session.address = nil
        }

        xpc_connection_set_target_queue(peer, workQueue)
        xpc_connection_set_event_handler(peer) { event in
            if xpc_get_type(event) == XPC_TYPE_ERROR {
                // The liveness contract: when musterd dies its peer
                // connection goes invalid and the interface must not outlive it.
                let description = xpc_copy_description(event)
                teardown(String(cString: description))
                free(description)
                return
            }
            guard xpc_get_type(event) == XPC_TYPE_DICTIONARY else { return }
            guard let reply = xpc_dictionary_create_reply(event) else { return }
            let rawOp = cstr(xpc_dictionary_get_string(event, "op"))
            let op = canonicalOp(rawOp)

            func refuse(_ why: String) {
                xpc_dictionary_set_bool(reply, "ok", false)
                xpc_dictionary_set_string(reply, "reason", why)
            }

            // Installation has its own fixed, non-degradable caller pin. A
            // configurable network caller cannot invoke it.
            if op == "install-locked-use" {
                if let reason = LockedUseInstall.authorize(code: code, info: info) {
                    refuse(reason)
                } else if let reason = LockedUseInstall.install(uid: info.uid) {
                    refuse(reason)
                } else {
                    xpc_dictionary_set_bool(reply, "ok", true)
                }
                xpc_connection_send_message(peer, reply)
                return
            }

            // ping answers even an unauthorized caller so the installer can
            // prove liveness; every other op is gated.
            if op == "ping" {
                xpc_dictionary_set_bool(reply, "ok", session.authorized)
                if let rejection { xpc_dictionary_set_string(reply, "reason", rejection) }
                xpc_dictionary_set_string(reply, "nonce", cstr(xpc_dictionary_get_string(event, "nonce")))
                xpc_dictionary_set_int64(reply, "helper_pid", Int64(getpid()))
                xpc_dictionary_set_int64(reply, "helper_euid", Int64(geteuid()))
                xpc_dictionary_set_string(reply, "version", helperVersion)
                xpc_dictionary_set_bool(reply, "degraded", policy?.degraded ?? false)
                xpc_dictionary_set_string(reply, "allowed_cidr", config.allowedCIDR.description)
                xpc_dictionary_set_string(reply, "config_source", config.source)
                xpc_dictionary_set_string(reply, "requirement", policy?.requirement ?? "(fail closed)")
                xpc_dictionary_set_int64(reply, "uptime_s", Int64(Date().timeIntervalSince(startedAt)))
                xpc_dictionary_set_string(reply, "caller_cdhash", info.cdhash)
                xpc_connection_send_message(peer, reply)
                if !session.authorized {
                    // Do not let an unauthorized peer hold a connection open.
                    workQueue.asyncAfter(deadline: .now() + 0.5) { xpc_connection_cancel(peer) }
                }
                return
            }

            guard session.authorized else {
                refuse("caller verification failed: \(rejection ?? "unauthorized")")
                xpc_connection_send_message(peer, reply)
                workQueue.asyncAfter(deadline: .now() + 0.5) { xpc_connection_cancel(peer) }
                return
            }

            switch op {
            case "open":
                if let existing = session.interfaceName {
                    refuse("this connection already owns \(existing); one interface per connection")
                    break
                }
                // The CIDR is deliberately NOT a parameter. It comes from the
                // root-owned config and nowhere else, so no client input can
                // ever reach the routing table. Split tunnel by construction:
                // this one prefix goes to the utun, everything else is left
                // exactly as it was.
                let address = cstr(xpc_dictionary_get_string(event, "address"))
                guard xpc_dictionary_get_value(event, "mtu") != nil else {
                    refuse("open requires an int64 mtu")
                    break
                }
                let mtu = xpc_dictionary_get_int64(event, "mtu")
                switch validateOpen(address: address, mtu: mtu, allowed: config.allowedCIDR) {
                case let .bad(why):
                    log("  open REFUSED for pid=\(info.pid): \(why)")
                    refuse(why)
                case let .ok(prefix, value, checkedMTU):
                    if let why = admitOpen(live: liveInterface, requestedBy: info.pid, address: value) {
                        log("  open REFUSED for pid=\(info.pid): \(why)")
                        refuse(why)
                        break
                    }
                    switch createUTUN() {
                    case let .failure(why):
                        log("  open FAILED for pid=\(info.pid): \(why)")
                        refuse(why)
                    case let .success(utun):
                        if let why = configureInterface(name: utun.name, address: value,
                                                        bits: prefix.bits, mtu: checkedMTU) {
                            log("  open FAILED for pid=\(info.pid): \(why)")
                            close(utun.fd) // closing the last reference destroys the interface
                            refuse(why)
                            break
                        }
                        session.interfaceName = utun.name
                        session.address = value
                        liveInterface = LiveInterface(pid: info.pid, name: utun.name, address: value)
                        xpc_dictionary_set_bool(reply, "ok", true)
                        xpc_dictionary_set_string(reply, "ifname", utun.name)
                        // XPC dups the descriptor here, so our copy must go:
                        // the interface has to die with the agent's fd, not
                        // linger because the helper kept a reference.
                        // Install the one route this helper owns. Assigning
                        // the address only gives the kernel a host route for
                        // the local address itself; without this the virtual
                        // network has no path and traffic silently follows the
                        // default route instead — which on a machine with a
                        // transparent proxy looks like success until you check
                        // `route get`. The prefix is the root-owned config's,
                        // never the caller's.
                        if let why = addRoute(prefix: prefix, interfaceName: utun.name) {
                            log("  open FAILED for pid=\(info.pid): \(why)")
                            teardownInterface(name: utun.name, address: value)
                            close(utun.fd)
                            refuse("could not install the virtual-network route: \(why)")
                            break
                        }
                        // Record it so teardown removes it again. Without
                        // this the route outlives the interface and the next
                        // open finds the prefix already bound.
                        session.routes.append(prefix)
                        routeOwners[prefix] = info.pid
                        xpc_dictionary_set_fd(reply, "fd", utun.fd)
                        close(utun.fd)
                        log("  opened \(utun.name) \(V4Prefix.dotted(value))/\(prefix.bits) mtu=\(checkedMTU) route=\(prefix) for pid=\(info.pid)")
                    }
                }

            case "addRoute", "removeRoute", "add-route", "remove-route":
                // Removed by design. The helper carries no routing API: the
                // single allowed prefix is installed at open() from the
                // root-owned config and removed at teardown. With no prefix
                // input there is no traffic-hijack surface to validate.
                log("  \(op) REFUSED for pid=\(info.pid): the helper has no routing API")
                refuse("this helper has no routing API; the virtual-network route is installed at open() from root-owned config")

            case "close":
                // musterd sends this on an orderly shutdown. Losing
                // the connection would release the same resources anyway, so
                // this is about promptness, not correctness — and it must
                // answer ok even when nothing was open, or a retry loop on the
                // agent side would spin on a shutdown path.
                teardown("close requested by pid=\(info.pid)")
                xpc_dictionary_set_bool(reply, "ok", true)

            default:
                refuse("unknown op \"\(rawOp)\"")
            }
            xpc_connection_send_message(peer, reply)
        }
        xpc_connection_resume(peer)
    }

    let listener = xpc_connection_create_mach_service(service, workQueue, kXPCListener)
    xpc_connection_set_event_handler(listener) { event in
        let type = xpc_get_type(event)
        if type == XPC_TYPE_ERROR {
            let description = xpc_copy_description(event)
            log("LISTENER ERROR: \(String(cString: description))")
            free(description)
            // A mach-service listener only works when launchd registered the
            // name; a foreground process cannot own one (proven in the spike).
            log("the Mach service could not be checked in — this binary must be launched by launchd "
                + "from a LaunchDaemon with a matching MachServices key")
            exit(3)
        }
        if type == XPC_TYPE_CONNECTION { handle(event as xpc_connection_t) }
    }
    xpc_connection_resume(listener)
    log("listening on \(service)")
    dispatchMain()
}

// MARK: - Client modes (--ping, --call)

/// Opens the service. The system domain is the default so that a service of
/// the same name registered in some user domain cannot answer for the daemon;
/// `--domain user` exists for testing an instance bootstrapped into gui/<uid>.
func openService(_ service: String, domain: String) -> xpc_connection_t {
    let flags: UInt64 = domain == "user" ? 0 : kXPCPrivileged
    let connection = xpc_connection_create_mach_service(service, nil, flags)
    xpc_connection_set_event_handler(connection) { _ in }
    xpc_connection_resume(connection)
    return connection
}

func runCall(service: String, domain: String, op: String, fields: [String: String], mtu: Int64?) -> Never {
    let connection = openService(service, domain: domain)
    let message = xpc_dictionary_create(nil, nil, 0)
    xpc_dictionary_set_string(message, "op", op)
    for (key, value) in fields { xpc_dictionary_set_string(message, key, value) }
    if let mtu { xpc_dictionary_set_int64(message, "mtu", mtu) }
    let reply = xpc_connection_send_message_with_reply_sync(connection, message)
    if xpc_get_type(reply) == XPC_TYPE_ERROR {
        let description = xpc_copy_description(reply)
        print("call \(op) FAILED: \(String(cString: description))")
        free(description)
        exit(1)
    }
    let ok = xpc_dictionary_get_bool(reply, "ok")
    print("op       = \(op)")
    print("ok       = \(ok)")
    let reason = cstr(xpc_dictionary_get_string(reply, "reason"))
    if !reason.isEmpty { print("reason   = \(reason)") }
    let ifname = cstr(xpc_dictionary_get_string(reply, "ifname"))
    if !ifname.isEmpty { print("ifname   = \(ifname)") }
    if xpc_dictionary_get_value(reply, "fd") != nil {
        let fd = xpc_dictionary_dup_fd(reply, "fd")
        print("fd       = \(fd) (received; closing immediately, which destroys the interface)")
        if fd >= 0 { close(fd) }
    }
    exit(ok ? 0 : 1)
}

func runPing(service: String, domain: String) -> Never {
    var seed = [UInt8](repeating: 0, count: 16)
    _ = SecRandomCopyBytes(kSecRandomDefault, seed.count, &seed)
    let nonce = seed.map { String(format: "%02x", $0) }.joined()

    let connection = openService(service, domain: domain)
    let message = xpc_dictionary_create(nil, nil, 0)
    xpc_dictionary_set_string(message, "op", "ping")
    xpc_dictionary_set_string(message, "nonce", nonce)
    let reply = xpc_connection_send_message_with_reply_sync(connection, message)

    if xpc_get_type(reply) == XPC_TYPE_ERROR {
        let description = xpc_copy_description(reply)
        print("ping FAILED: \(String(cString: description))")
        free(description)
        print("the daemon is not answering on \(service)")
        exit(1)
    }
    let echoed = cstr(xpc_dictionary_get_string(reply, "nonce"))
    guard echoed == nonce else {
        print("ping FAILED: nonce mismatch (sent \(nonce), got \(echoed.isEmpty ? "(none)" : echoed))")
        print("the reply did not come from a live handler")
        exit(1)
    }
    let pid = xpc_dictionary_get_int64(reply, "helper_pid")
    let euid = xpc_dictionary_get_int64(reply, "helper_euid")
    let uptime = xpc_dictionary_get_int64(reply, "uptime_s")
    let accepted = xpc_dictionary_get_bool(reply, "ok")
    print("daemon is LIVE and answered a fresh nonce")
    print("  domain        = \(domain)")
    print("  helper pid    = \(pid)")
    print("  helper euid   = \(euid)\(euid == 0 ? "" : "   <-- NOT ROOT: utun creation will fail")")
    print("  version       = \(cstr(xpc_dictionary_get_string(reply, "version")))")
    print("  uptime        = \(uptime)s")
    print("  allowed cidr  = \(cstr(xpc_dictionary_get_string(reply, "allowed_cidr")))")
    print("  config source = \(cstr(xpc_dictionary_get_string(reply, "config_source")))")
    print("  requirement   = \(cstr(xpc_dictionary_get_string(reply, "requirement")))")
    print("  degraded      = \(xpc_dictionary_get_bool(reply, "degraded"))")
    print("  this caller   = \(cstr(xpc_dictionary_get_string(reply, "caller_cdhash")))")
    if accepted {
        print("  verification  = ACCEPTED this caller")
    } else {
        // Expected: the requirement pins musterd, and this is the
        // helper binary. A refusal still proves the daemon is up AND that
        // caller verification is switched on, which is the stronger signal.
        print("  verification  = REFUSED this caller (expected — the pin names musterd, not the helper)")
        print("                  reason: \(cstr(xpc_dictionary_get_string(reply, "reason")))")
    }
    if euid != 0 { exit(1) }
    exit(0)
}

// MARK: - --check

func runCheck(confPath: String, service: String) -> Never {
    let (config, notes) = loadConfig(path: confPath)
    print("muster-net-helper \(helperVersion)")
    print("service        = \(service)")
    print("config path    = \(confPath)")
    for note in notes { print("note           : \(note)") }
    if let hazard = pathChainIsRootOwned(confPath) {
        print("config trust   = UNTRUSTED (\(hazard))")
        print("                 a root daemon will IGNORE this file and use compiled defaults")
    } else {
        print("config trust   = root-owned chain")
    }
    print("effective src  = \(config.source)")
    print("allowed cidr   = \(config.allowedCIDR)")
    switch resolvePolicy(config) {
    case let .failClosed(why):
        print("policy         = FAIL CLOSED (\(why))")
        exit(2)
    case let .ok(policy):
        print("requirement    = \(policy.requirement)")
        print("degraded       = \(policy.degraded)")
        print("expect path    = \(policy.expectPath.isEmpty ? "(any)" : policy.expectPath)")
        print("expect uid     = \(policy.expectUID.map(String.init) ?? "(any)")")
        switch compileRequirement(policy.requirement) {
        case let .failure(why):
            print("compile        = FAILED: \(why)")
            exit(2)
        case .success:
            print("compile        = ok")
        }
        if policy.degraded {
            print("WARNING: caller verification is DEGRADED to a cdhash pin.")
            exit(0)
        }
    }
    exit(0)
}

// MARK: - --selftest

func runSelfTest() -> Never {
    var failures = 0
    func check(_ name: String, _ condition: Bool) {
        if condition {
            print("  ok    \(name)")
        } else {
            print("  FAIL  \(name)")
            failures += 1
        }
    }

    let installedCaller = CallerInfo(pid: 1, uid: 501, path: "/Applications/Muster.app/Contents/MacOS/musterd", cdhash: "", teamID: "89LGY6BD53")
    check("plugin install rejects missing audit-token identity", LockedUseInstall.authorize(code: nil, info: installedCaller) != nil)
    let wrongCaller = CallerInfo(pid: 1, uid: 0, path: "/tmp/musterd", cdhash: "", teamID: "89LGY6BD53")
    check("plugin install rejects root/arbitrary executable caller", LockedUseInstall.authorize(code: nil, info: wrongCaller) != nil)

    print("struct layout (the hardcoded ioctl numbers encode these sizes):")
    check("sizeof(ifreq) == 32", MemoryLayout<ifreq>.size == 32)
    check("sizeof(ifaliasreq) == 64", MemoryLayout<ifaliasreq>.size == 64)
    check("sizeof(ctl_info) == 100", MemoryLayout<ctl_info>.size == 100)
    check("sizeof(rt_msghdr) == 92", rtMsghdrSize == 92)
    check("sizeof(sockaddr_in) == 16", sockaddrInSize == 16)
    check("sizeof(sockaddr_dl) == 20", sockaddrDlSize == 20)
    // _IOW(g,n,t) = 0x80000000 | ((sizeof(t) & 0x1fff) << 16) | (g << 8) | n
    func iow(_ group: UInt, _ number: UInt, _ size: Int) -> UInt {
        0x8000_0000 | ((UInt(size) & 0x1FFF) << 16) | (group << 8) | number
    }
    func iowr(_ group: UInt, _ number: UInt, _ size: Int) -> UInt {
        0xC000_0000 | ((UInt(size) & 0x1FFF) << 16) | (group << 8) | number
    }
    check("SIOCAIFADDR", kSIOCAIFADDR == iow(0x69, 26, MemoryLayout<ifaliasreq>.size))
    check("SIOCDIFADDR", kSIOCDIFADDR == iow(0x69, 25, MemoryLayout<ifreq>.size))
    check("SIOCSIFMTU", kSIOCSIFMTU == iow(0x69, 52, MemoryLayout<ifreq>.size))
    check("SIOCSIFFLAGS", kSIOCSIFFLAGS == iow(0x69, 16, MemoryLayout<ifreq>.size))
    check("SIOCGIFFLAGS", kSIOCGIFFLAGS == iowr(0x69, 17, MemoryLayout<ifreq>.size))
    check("CTLIOCGINFO", kCTLIOCGINFO == iowr(0x4E, 3, MemoryLayout<ctl_info>.size))

    print("prefix parsing:")
    check("192.168.14.0/24 parses", parseV4Prefix("192.168.14.0/24") == V4Prefix(addr: 0xC0A8_0E00, bits: 24))
    check("bare address rejected", parseV4Prefix("192.168.14.1") == nil)
    check("/33 rejected", parseV4Prefix("192.168.14.0/33") == nil)
    check("/024 rejected", parseV4Prefix("192.168.14.0/024") == nil)
    check("leading-zero octet rejected", parseV4Prefix("192.168.014.0/24") == nil)
    check("short form rejected", parseV4Prefix("192.168.14/24") == nil)
    check("hex form rejected", parseV4Address("0xc0a80e01") == nil)
    check("trailing junk rejected", parseV4Address("192.168.14.1x") == nil)
    check("whitespace rejected", parseV4Address(" 192.168.14.1") == nil)

    let allowed = V4Prefix(addr: 0xC0A8_0E00, bits: 24)
    func openBad(_ address: String, _ mtu: Int64) -> Bool {
        if case .bad = validateOpen(address: address, mtu: mtu, allowed: allowed) { return true }
        return false
    }
    func openOK(_ address: String, _ mtu: Int64) -> Bool {
        if case .ok = validateOpen(address: address, mtu: mtu, allowed: allowed) { return true }
        return false
    }
    print("open() validation (allowed = \(allowed)):")
    check("accepts 192.168.14.2/24 mtu 1280", openOK("192.168.14.2", 1280))
    check("accepts mtu 576", openOK("192.168.14.2", 576))
    check("accepts mtu 9000", openOK("192.168.14.2", 9000))
    // The CIDR is no longer an input: it comes from the root-owned config, so
    // the old "rejects a wider/narrower/0.0.0.0-0 cidr" cases have nothing left
    // to reject — the surface they guarded does not exist. What remains is the
    // address and mtu the client may still supply.
    check("rejects an address outside the allowed cidr", openBad("192.168.13.2", 1280))
    check("rejects an out-of-range address", openBad("10.0.0.1", 1280))
    check("rejects the network address", openBad("192.168.14.0", 1280))
    check("rejects the broadcast address", openBad("192.168.14.255", 1280))
    check("rejects a garbage address", openBad("not-an-address", 1280))
    check("rejects mtu 575", openBad("192.168.14.2", 575))
    check("rejects mtu 9001", openBad("192.168.14.2", 9001))
    check("rejects mtu 0", openBad("192.168.14.2", 0))
    check("rejects negative mtu", openBad("192.168.14.2", -1))

    func routeBad(_ text: String) -> Bool {
        if case .bad = validateRoute(prefix: text, allowed: allowed) { return true }
        return false
    }
    func routeOK(_ text: String) -> Bool {
        if case .ok = validateRoute(prefix: text, allowed: allowed) { return true }
        return false
    }
    print("route validation (allowed = \(allowed)):")
    check("accepts the whole virtual network", routeOK("192.168.14.0/24"))
    check("accepts a contained /32", routeOK("192.168.14.7/32"))
    check("accepts a contained /28", routeOK("192.168.14.16/28"))
    check("REJECTS the default route", routeBad("0.0.0.0/0"))
    check("REJECTS 128.0.0.0/1", routeBad("128.0.0.0/1"))
    check("REJECTS 192.168.0.0/16", routeBad("192.168.0.0/16"))
    check("REJECTS a neighbouring /24", routeBad("192.168.15.0/24"))
    check("REJECTS private-tunnel's 192.168.13.0/24", routeBad("192.168.13.0/24"))
    check("REJECTS 10.0.0.0/8", routeBad("10.0.0.0/8"))
    check("REJECTS a loopback host route", routeBad("127.0.0.1/32"))
    check("REJECTS a non-canonical prefix", routeBad("192.168.14.16/24"))
    check("REJECTS garbage", routeBad("../../etc/passwd"))
    check("REJECTS an empty prefix", routeBad(""))

    print("route message marshalling (against routesocket_darwin.go):")
    let message = marshalRouteChange(type: UInt8(RTM_ADD),
                                     prefix: V4Prefix(addr: 0xC0A8_0E00, bits: 24),
                                     interfaceIndex: 42, sequence: 7)
    check("length is header + dst + gateway + netmask", message.count == 92 + 16 + 20 + 16)
    check("rtm_msglen matches", Int(UInt16(message[0]) | (UInt16(message[1]) << 8)) == message.count)
    check("rtm_version == RTM_VERSION", message[2] == UInt8(RTM_VERSION))
    check("rtm_type == RTM_ADD", message[3] == UInt8(RTM_ADD))
    check("rtm_addrs == DST|GATEWAY|NETMASK",
          message[12] == UInt8(RTA_DST | RTA_GATEWAY | RTA_NETMASK))
    check("dst sockaddr is 192.168.14.0",
          message[92] == 16 && message[93] == UInt8(AF_INET)
              && Array(message[96..<100]) == [192, 168, 14, 0])
    check("gateway is a sockaddr_dl with index 42",
          message[108] == 20 && message[109] == UInt8(AF_LINK)
              && message[110] == 42 && message[111] == 0)
    check("netmask sockaddr is 255.255.255.0",
          message[128] == 16 && Array(message[132..<136]) == [255, 255, 255, 0])
    let hostRoute = marshalRouteChange(type: UInt8(RTM_DELETE),
                                       prefix: V4Prefix(addr: 0xC0A8_0E07, bits: 32),
                                       interfaceIndex: 0, sequence: 8)
    check("delete without a gateway omits the sockaddr_dl", hostRoute.count == 92 + 16 + 16)
    check("delete netmask is 255.255.255.255", Array(hostRoute[112..<116]) == [255, 255, 255, 255])

    // Both spellings of the route ops reach the same refusal.
    print("op names:")
    check("add-route  -> addRoute", canonicalOp("add-route") == "addRoute")
    check("remove-route -> removeRoute", canonicalOp("remove-route") == "removeRoute")
    check("addRoute stays addRoute", canonicalOp("addRoute") == "addRoute")
    check("removeRoute stays removeRoute", canonicalOp("removeRoute") == "removeRoute")
    check("open is untouched", canonicalOp("open") == "open")
    check("close is untouched", canonicalOp("close") == "close")
    check("ping is untouched", canonicalOp("ping") == "ping")
    check("an unknown op is not silently remapped", canonicalOp("addroute") == "addroute")

    // These two rules are the whole containment story once you accept that
    // the caller check authenticates a BINARY and not an invoker: a second
    // caller gets no interface, and therefore no route.
    print("admission (one interface, one owner per route):")
    let held = LiveInterface(pid: 4242, name: "utun9", address: 0xC0A8_0E0B)
    check("first open is admitted", admitOpen(live: nil, requestedBy: 1, address: 0xC0A8_0E0B) == nil)
    check("REFUSES a second interface at another address",
          admitOpen(live: held, requestedBy: 99, address: 0xC0A8_0E63) != nil)
    check("REFUSES a second interface at the same address",
          admitOpen(live: held, requestedBy: 99, address: 0xC0A8_0E0B) != nil)
    check("REFUSES a second interface even for the same pid",
          admitOpen(live: held, requestedBy: 4242, address: 0xC0A8_0E63) != nil)
    let whole = V4Prefix(addr: 0xC0A8_0E00, bits: 24)
    let host = V4Prefix(addr: 0xC0A8_0E07, bits: 32)
    check("an unowned route is admitted", admitRoute(owners: [:], prefix: whole, requestedBy: 7) == nil)
    check("the owner may rebind its own route",
          admitRoute(owners: [whole: 7], prefix: whole, requestedBy: 7) == nil)
    check("REFUSES rebinding a route another pid owns",
          admitRoute(owners: [whole: 7], prefix: whole, requestedBy: 8) != nil)
    check("REFUSES stealing a host route another pid owns",
          admitRoute(owners: [host: 7], prefix: host, requestedBy: 8) != nil)
    check("a different prefix is unaffected",
          admitRoute(owners: [whole: 7], prefix: host, requestedBy: 8) == nil)

    print("caller policy:")
    var strict = HelperConfig()
    if case let .ok(policy) = resolvePolicy(strict) {
        check("default policy is not degraded", !policy.degraded)
        check("default policy pins the team", policy.requirement.contains("89LGY6BD53"))
        check("default policy pins the identifier", policy.requirement.contains("\"\(HelperConfig().callerIdentifier)\""))
        if case .failure = compileRequirement(policy.requirement) {
            check("default requirement compiles", false)
        } else {
            check("default requirement compiles", true)
        }
    } else {
        check("default policy resolves", false)
    }
    strict.callerTeam = ""
    strict.callerCDHash = "98d0b907adf6b2ded3dbb9f74bc8a5629da8efef"
    if case let .ok(policy) = resolvePolicy(strict) {
        check("cdhash-only policy is degraded", policy.degraded)
        check("cdhash-only policy pins the hash", policy.requirement == "cdhash H\"98d0b907adf6b2ded3dbb9f74bc8a5629da8efef\"")
        if case .failure = compileRequirement(policy.requirement) {
            check("cdhash requirement compiles", false)
        } else {
            check("cdhash requirement compiles", true)
        }
    } else {
        check("cdhash-only policy resolves", false)
    }
    strict.callerCDHash = ""
    if case let .failClosed(why) = resolvePolicy(strict) {
        check("no team and no cdhash fails closed", !why.isEmpty)
    } else {
        check("no team and no cdhash fails closed", false)
    }

    print("")
    if failures == 0 {
        print("selftest PASSED")
        exit(0)
    }
    print("selftest FAILED: \(failures) check(s)")
    exit(1)
}

// MARK: - Entry point

let usage = """
muster-net-helper \(helperVersion) — muster's privileged virtual-network helper.

Normally launched by launchd as \(defaultServiceName), registered by Muster.app
through SMAppService from Contents/Library/LaunchDaemons.

usage: muster-net-helper [--service NAME] [--conf PATH] [--domain system|user] [MODE]

modes:
  (none)      run as the launchd daemon, owning the Mach service
  --ping      connect to the service, send a fresh nonce, print the live answer
  --call OP   send one op and print the reply, for debugging the validators:
                --call open --cidr C --address A --mtu N
                --call addRoute --prefix P      (or add-route)
                --call removeRoute --prefix P   (or remove-route)
                --call close
              (subject to the same caller verification as any other client)
  --check     resolve the config and caller requirement, print them
  --selftest  run the pure argument-validation table (no root, no XPC)
  --version   print the version
  --help      this text

options:
  --domain    which bootstrap namespace the client modes look the service up
              in. Default "system" (the LaunchDaemon). "user" is for testing
              an instance bootstrapped into gui/<uid>.

environment:
  MUSTER_NETHELPER_SERVICE  Mach service name (default \(defaultServiceName))
  MUSTER_NETHELPER_CONF     config path (default \(defaultConfPath))
"""

let environment = ProcessInfo.processInfo.environment
var serviceName = environment["MUSTER_NETHELPER_SERVICE"] ?? defaultServiceName
var confPath = environment["MUSTER_NETHELPER_CONF"] ?? defaultConfPath
var mode = "daemon"
var domain = "system"
var callOp = ""
var callFields: [String: String] = [:]
var callMTU: Int64?

var argumentIndex = 1
let arguments = CommandLine.arguments

func nextValue(_ flag: String) -> String {
    argumentIndex += 1
    guard argumentIndex < arguments.count else { print("\(flag) needs a value"); exit(2) }
    return arguments[argumentIndex]
}

while argumentIndex < arguments.count {
    let argument = arguments[argumentIndex]
    switch argument {
    case "--service":
        serviceName = nextValue(argument)
    case "--conf":
        confPath = nextValue(argument)
    case "--domain":
        domain = nextValue(argument)
        guard domain == "system" || domain == "user" else { print("--domain must be system or user"); exit(2) }
    case "--call":
        mode = "call"
        callOp = nextValue(argument)
    case "--cidr", "--address", "--prefix":
        callFields[String(argument.dropFirst(2))] = nextValue(argument)
    case "--mtu":
        let text = nextValue(argument)
        guard let value = Int64(text) else { print("--mtu must be a number"); exit(2) }
        callMTU = value
    case "--ping", "--check", "--selftest":
        mode = String(argument.dropFirst(2))
    case "--version":
        print("muster-net-helper \(helperVersion)")
        exit(0)
    case "--help", "-h":
        print(usage)
        exit(0)
    default:
        print("unknown argument \(argument)\n")
        print(usage)
        exit(2)
    }
    argumentIndex += 1
}

switch mode {
case "ping": runPing(service: serviceName, domain: domain)
case "call": runCall(service: serviceName, domain: domain, op: callOp, fields: callFields, mtu: callMTU)
case "check": runCheck(confPath: confPath, service: serviceName)
case "selftest": runSelfTest()
default: runDaemon(service: serviceName, confPath: confPath)
}
