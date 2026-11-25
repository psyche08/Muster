import XCTest
import AVFoundation
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
    func testRemoteViewportUsesOnePointPerPixelAndMapsPannedInput() throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        let viewport = MobileRemoteViewport.Viewport(frame: CGRect(x: 0, y: 0, width: 220, height: 190))
        window.addSubview(viewport)
        let layer = AVSampleBufferDisplayLayer()
        let pixels = CGSize(width: 1920, height: 1080)
        var pointer: CGPoint?
        viewport.show(layer, pixels: pixels, mouse: true) { _, point, _ in pointer = point }
        viewport.layoutIfNeeded()
        let canvas = try XCTUnwrap(layer.superlayer?.delegate as? MobileRemoteViewport.TouchCanvas)
        XCTAssertGreaterThan(scene.screen.nativeScale, 1)
        XCTAssertEqual(layer.frame.width, pixels.width, accuracy: 0.01)
        XCTAssertEqual(layer.frame.height, pixels.height, accuracy: 0.01)
        XCTAssertEqual(viewport.panGestureRecognizer.minimumNumberOfTouches, 3)
        XCTAssertEqual(viewport.panGestureRecognizer.maximumNumberOfTouches, 3)
        XCTAssertEqual(canvas.gestureRecognizers?.compactMap { $0 as? UIPanGestureRecognizer }
            .map(\.maximumNumberOfTouches).sorted(), [1, 2])
        viewport.contentOffset = CGPoint(x: 37, y: 19)
        let hit = CGPoint(x: viewport.bounds.minX + 13, y: viewport.bounds.minY + 17)
        canvas.action?(.primary, viewport.convert(hit, to: canvas), 0)
        XCTAssertEqual(pointer, CGPoint(x: 50, y: 36))
        viewport.show(layer, pixels: CGSize(width: 96, height: 48), mouse: false) { _, _, _ in }
        viewport.layoutIfNeeded()
        XCTAssertFalse(canvas.isUserInteractionEnabled)
        XCTAssertTrue(viewport.isScrollEnabled)
        XCTAssertEqual(viewport.contentOffset, .zero)
        XCTAssertEqual(canvas.frame.midX, viewport.bounds.midX, accuracy: 0.01)
        XCTAssertEqual(canvas.frame.midY, viewport.bounds.midY, accuracy: 0.01)
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
