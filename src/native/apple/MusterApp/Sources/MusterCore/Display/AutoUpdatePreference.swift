import Foundation

/// Whether this Mac installs signed Muster updates unattended. Only the
/// owner's choice lives here; Shared Rust decides when to check, download
/// and install.
public enum AutoUpdatePreference {
    public static let key = "autoUpdate"

    /// On until the owner turns it off.
    public static func enabled(_ defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: key) as? Bool ?? true
    }
}
