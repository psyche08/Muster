import Foundation

/// The user's keep-awake choice on a Mac, as the `caffeinate` flags it maps
/// to. A local OS preference like the conversation style: it holds no
/// session, authorization or send state, so it stays in the shell.
public struct KeepAwakePreference: Equatable, Sendable {
    public static let enabledKey = "keepAwake"
    public static let displayKey = "keepAwakeDisplay"

    public var enabled: Bool
    /// Keep the display on too; only meaningful while `enabled`.
    public var display: Bool

    public init(enabled: Bool = true, display: Bool = false) {
        self.enabled = enabled
        self.display = display
    }

    /// Reads the stored choice; keep-awake is on until the user turns it off.
    public init(defaults: UserDefaults) {
        enabled = defaults.object(forKey: Self.enabledKey) as? Bool ?? true
        display = defaults.bool(forKey: Self.displayKey)
    }

    /// `caffeinate` arguments, or nil when nothing should run. `-w` ties
    /// the process to Muster's, so it ends even if Muster is killed.
    public func arguments(watching pid: Int32) -> [String]? {
        guard enabled else { return nil }
        return [display ? "-dimsu" : "-imsu", "-w", String(pid)]
    }
}
