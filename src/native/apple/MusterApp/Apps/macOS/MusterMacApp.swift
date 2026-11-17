// Muster.app on macOS (XcodeGen target MusterMac): a client of musterd
// over its socket. The daemon ships inside the bundle
// (Contents/MacOS/musterd); launchd runs it from the system LaunchAgent
// /Library/LaunchAgents/dev.linsheng.musterd.plist, which the app installs
// from Contents/Resources/SystemLaunchAgent (ADR 0227, DaemonAgent). The
// main page opens only once musterd answers. The daemon keeps its files in
// ~/Library/Application Support/Muster and ~/Library/Logs/Muster.
import MusterCore
import MusterUI
import SwiftUI

@main
struct MusterMacApp: App {
    init() {
        MusterRuntime.desktopUpdateStartup()
        Caffeinate.shared.apply()
    }

    @StateObject private var model = MusterModel()

    var body: some Scene {
        WindowGroup("Muster") {
            MusterRootView(model: model)
                .tint(Color("AccentColor"))
                .frame(minWidth: 720, minHeight: 480)
                .task {
                    MusterRuntime.initFileLogging(level: 3)
                    DesktopHelper.registerIfInstalled()
                    let socket = MusterPaths.daemonSocket.path
                    // Only the installed app touches launchd; a development
                    // build waits for a daemon started by hand.
                    let managed = MacInstall.isInstalled
                    // Muster's own restart or update opened this app: the
                    // daemon it stopped starts again without asking.
                    let arguments = ProcessInfo.processInfo.arguments
                    let resumed = arguments.contains("--muster-restarted") || arguments.contains("--muster-updated")
                    model.start(config: .desktop(dir: MusterPaths.stateDirectory, socket: MusterPaths.daemonSocket), desktop: DesktopLaunch(
                        managed: managed,
                        check: {
                            try await Task.detached {
                                try MusterRuntime.checkDesktop(socket: socket, managed: managed, resumed: resumed, action: { DaemonAgent.perform($0) })
                            }.value
                        },
                        prepare: {
                            try await Task.detached {
                                try MusterRuntime.prepareDesktop(socket: socket, managed: managed, action: { DaemonAgent.perform($0) })
                            }.value
                        }))
                }
        }
        .commands {
            MusterSettingsCommands(model: model)
            KeepAwakeCommands()
        }
    }

}
