#if os(macOS)
import SwiftUI

/// Standard app-menu entry; navigation remains owned by Shared Rust.
public struct MusterSettingsCommands: Commands {
    @ObservedObject private var model: MusterModel

    public init(model: MusterModel) { self.model = model }

    public var body: some Commands {
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { model.core?.navigate(to: .settings) }
                .keyboardShortcut(",", modifiers: .command)
                .disabled(!model.started || model.core == nil)
        }
    }
}
#endif
