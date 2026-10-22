import XCTest
import SwiftUI
import MusterCore
@testable import MusterUI

final class NativeNavigationTests: XCTestCase {
    @MainActor
    func testCorePushAndNativePopUseTheSameStack() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let model = MusterModel(keys: SoftwareKeys())
        model.start(config: .phone(dir: directory, name: "Navigation fixture"))
        let core = try XCTUnwrap(model.core)
        defer { core.shutdown(); try? FileManager.default.removeItem(at: directory) }
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        let host = UIHostingController(rootView: MusterRootView(model: model))
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        waitUntil { model.vm.navigationPath == [.onboarding] }
        core.navigate(to: .settings)
        waitUntil { model.vm.navigationPath == [.onboarding, .settings] }
        core.navigate(to: .join)
        waitUntil { model.vm.navigationPath == [.onboarding, .settings, .join] }
        let navigation = try XCTUnwrap(findNavigation(host))
        waitUntil { navigation.viewControllers.count == 3 }
        XCTAssertFalse(navigation.navigationBar.isHidden)
        XCTAssertNotNil(navigation.interactivePopGestureRecognizer)
        navigation.popToRootViewController(animated: false)
        waitUntil { model.vm.screen == .onboarding && navigation.viewControllers.count == 1 }
        XCTAssertEqual(model.vm.navigationPath, [.onboarding])
    }

    @MainActor
    private func findNavigation(_ controller: UIViewController) -> UINavigationController? {
        if let navigation = controller as? UINavigationController { return navigation }
        return controller.children.lazy.compactMap { self.findNavigation($0) }.first
    }

    @MainActor
    private func waitUntil(_ condition: () -> Bool, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(5)
        while !condition() && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        XCTAssertTrue(condition(), file: file, line: line)
    }
}
