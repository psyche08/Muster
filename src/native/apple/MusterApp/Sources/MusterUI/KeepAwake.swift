#if os(macOS)
import AppKit
import MusterCore
import SwiftUI

/// Runs `/usr/bin/caffeinate` while Muster runs and the user keeps the Mac
/// awake (`-imsu`, or `-dimsu` with the screen on). One child at a time; a
/// changed choice replaces it, and one that exits on its own is restarted.
@MainActor
public final class Caffeinate {
    public static let shared = Caffeinate()

    private var process: Process?
    private var running: [String]?

    private init() {
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { _ in MainActor.assumeIsolated { Caffeinate.shared.stop() } }
    }

    /// Brings the child in line with the stored preference.
    public func apply() {
        let wanted = KeepAwakePreference(defaults: .standard)
            .arguments(watching: ProcessInfo.processInfo.processIdentifier)
        guard wanted != running || (wanted != nil && process?.isRunning != true) else { return }
        stop()
        guard let wanted else { return }
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/caffeinate")
        child.arguments = wanted
        child.terminationHandler = { ended in
            Task { @MainActor in
                let shared = Caffeinate.shared
                guard shared.process === ended else { return }
                NSLog("muster: caffeinate exited (%d); restarting", ended.terminationStatus)
                shared.process = nil
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                shared.apply()
            }
        }
        do {
            try child.run()
            process = child
            running = wanted
        } catch {
            NSLog("muster: caffeinate failed to start: %@", error.localizedDescription)
        }
    }

    private func stop() {
        let child = process
        process = nil
        running = nil
        if child?.isRunning == true { child?.terminate() }
    }
}

/// The app menu's Keep Awake and Keep Screen On switches.
public struct KeepAwakeCommands: Commands {
    @AppStorage(KeepAwakePreference.enabledKey) private var enabled = true
    @AppStorage(KeepAwakePreference.displayKey) private var display = false

    public init() {}

    public var body: some Commands {
        CommandGroup(after: .appSettings) {
            Toggle("Keep Awake", isOn: Binding(
                get: { enabled },
                set: { enabled = $0; Caffeinate.shared.apply() }
            ))
            Toggle("Keep Screen On", isOn: Binding(
                get: { display },
                set: { display = $0; Caffeinate.shared.apply() }
            ))
            .disabled(!enabled)
        }
    }
}
#endif
