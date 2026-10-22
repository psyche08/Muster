// The macOS app: a client of musterd over its socket (ARCHITECTURE §3.7).
// `swift run MusterDev` runs it unbundled for development; project.yml
// (XcodeGen) builds the signed app with the same view.
#if os(macOS) && canImport(SwiftUI)
import MusterCore
import MusterUI
import SwiftUI

struct MusterMacApp: App {
    @StateObject private var model = MusterModel()

    var body: some Scene {
        WindowGroup("Muster") {
            MusterRootView(model: model)
                .frame(minWidth: 720, minHeight: 480)
                .onAppear {
                    MusterRuntime.initLogging(level: 3)
                    model.start(config: .desktop(dir: MusterPaths.daemonDirectory, socket: MusterPaths.daemonSocket))
                }
        }
        .commands {
            MusterSettingsCommands(model: model)
            CommandGroup(replacing: .newItem) {}
        }
    }
}

MusterMacApp.main()
#else
print("MusterMac is the macOS app; on this platform only MusterCore builds.")
#endif
