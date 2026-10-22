import Foundation

/// The JSON `muster_core_new` reads (muster.h). Phones embed the link; a
/// desktop talks to musterd over its socket.
public struct MusterConfig: Codable, Equatable {
    public enum Mode: String, Codable {
        case embedded
        case socket
    }

    public var dir: String
    public var mode: Mode
    public var relay: String?
    public var name: String
    public var kind: String
    public var socket: String?
    public var bind: String
    public var allowLoopback: Bool
    /// The app's release version the core reports (settings, heartbeat).
    public var version: String?

    enum CodingKeys: String, CodingKey {
        case dir, mode, relay, name, kind, socket, bind, version
        case allowLoopback = "allow_loopback"
    }

    /// This bundle's "1.0 (16)": marketing version and build number.
    public static var bundleVersion: String {
        let info = Bundle.main.infoDictionary ?? [:]
        let short = info["CFBundleShortVersionString"] as? String ?? ""
        let build = info["CFBundleVersion"] as? String ?? ""
        return build.isEmpty ? short : "\(short) (\(build))"
    }

    /// A phone: its own link, relay learned from the invite (or given).
    public static func phone(dir: URL, name: String, relay: String? = nil) -> MusterConfig {
        MusterConfig(
            dir: dir.path, mode: .embedded, relay: relay, name: name, kind: "phone",
            socket: nil, bind: "0.0.0.0:0", allowLoopback: false, version: bundleVersion
        )
    }

    /// A desktop app over musterd's socket.
    public static func desktop(dir: URL, socket: URL) -> MusterConfig {
        MusterConfig(
            dir: dir.path, mode: .socket, relay: nil, name: "", kind: "desktop",
            socket: socket.path, bind: "0.0.0.0:0", allowLoopback: false, version: nil
        )
    }

    public func json() throws -> Data {
        try JSONEncoder().encode(self)
    }
}
