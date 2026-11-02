// The Mac's stored choice for unattended updates.
import XCTest
@testable import MusterCore

final class AutoUpdatePreferenceTests: XCTestCase {
    func testOnUntilTheOwnerTurnsItOff() {
        let name = "AutoUpdatePreferenceTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        XCTAssertTrue(AutoUpdatePreference.enabled(defaults))
        defaults.set(false, forKey: AutoUpdatePreference.key)
        XCTAssertFalse(AutoUpdatePreference.enabled(defaults))
        defaults.set(true, forKey: AutoUpdatePreference.key)
        XCTAssertTrue(AutoUpdatePreference.enabled(defaults))
    }
}
