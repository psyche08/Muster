// The Mac's keep-awake menu choice and the caffeinate flags it maps to.
import XCTest
@testable import MusterCore

final class KeepAwakePreferenceTests: XCTestCase {
    private func defaults() -> UserDefaults {
        let name = "KeepAwakePreferenceTests.\(UUID().uuidString)"
        let d = UserDefaults(suiteName: name)!
        d.removePersistentDomain(forName: name)
        return d
    }

    func testOnByDefaultWithoutTheScreen() {
        let p = KeepAwakePreference(defaults: defaults())
        XCTAssertEqual(p, KeepAwakePreference(enabled: true, display: false))
        XCTAssertEqual(p.arguments(watching: 42), ["-imsu", "-w", "42"])
    }

    func testScreenOnAddsDisplayFlag() {
        let d = defaults()
        d.set(true, forKey: KeepAwakePreference.displayKey)
        XCTAssertEqual(KeepAwakePreference(defaults: d).arguments(watching: 7), ["-dimsu", "-w", "7"])
    }

    func testOffRunsNothingEvenWithScreenOnStored() {
        let d = defaults()
        d.set(false, forKey: KeepAwakePreference.enabledKey)
        d.set(true, forKey: KeepAwakePreference.displayKey)
        XCTAssertNil(KeepAwakePreference(defaults: d).arguments(watching: 7))
    }
}
