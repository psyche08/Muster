// Muster.app on iOS (XcodeGen target MusteriOS): the core embeds its own
// link (D8), keys live in the Secure Enclave, the relay's push token is
// handed to the core so a member can wake this phone.
import MusterCore
import MusterUI
import Network
import SwiftUI
import UIKit

@main
struct MusteriOSApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var model = MusterModel()
    private let monitor = NWPathMonitor()

    var body: some Scene {
        WindowGroup {
            MusterRootView(model: model)
                .onChange(of: scenePhase) { _, phase in
                    PerformanceDiagnostics.shared.setForeground(phase == .active)
                }
                .onAppear {
                    PerformanceDiagnostics.shared.start()
                    MusterRuntime.initLogging(level: 3)
                    model.start(config: .phone(dir: MusterPaths.stateDirectory, name: MusterPaths.deviceName))
                    delegate.model = model
                    UIApplication.shared.registerForRemoteNotifications()
                    monitor.pathUpdateHandler = { path in
                        NSLog("muster: network path status=%@ reason=%@ ipv4=%d ipv6=%d wifi=%d cellular=%d",
                              String(describing: path.status), String(describing: path.unsatisfiedReason),
                              path.supportsIPv4, path.supportsIPv6,
                              path.usesInterfaceType(.wifi), path.usesInterfaceType(.cellular))
                        Task { @MainActor in model.networkChanged() }
                    }
                    monitor.start(queue: DispatchQueue.global(qos: .utility))
                }
        }
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate {
    weak var model: MusterModel?

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in model?.core?.setPushToken(kind: 1, token: deviceToken) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        NSLog("muster: push registration failed: %@", "\(error)")
    }

    /// A silent push from the relay: the core re-registers so the member's
    /// traffic finds this phone.
    func application(_ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any]) async -> UIBackgroundFetchResult {
        await MainActor.run { model?.networkChanged() }
        return .newData
    }
}
