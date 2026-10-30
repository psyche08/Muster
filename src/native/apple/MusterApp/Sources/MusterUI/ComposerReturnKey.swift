#if os(macOS)
import AppKit
import SwiftUI

/// Native key adaptation only; the ordinary send callback owns submission.
struct ComposerReturnKey: ViewModifier {
    let send: () -> Void

    func body(content: Content) -> some View {
        content.onKeyPress(.return, phases: .down) { press in
            guard let editor = (NSApp.currentEvent?.window ?? NSApp.keyWindow)?.firstResponder as? NSTextView else { return .ignored }
            return Self.handle(press.modifiers, editor: editor, send: send)
        }
    }

    @MainActor
    static func handle(_ modifiers: EventModifiers, editor: NSTextView, send: () -> Void) -> KeyPress.Result {
        // Return may be accepting an IME candidate rather than submitting text.
        guard !editor.hasMarkedText() else { return .ignored }
        let editingModifiers = modifiers.intersection([.shift, .command, .control, .option])
        if editingModifiers == .shift {
            editor.insertNewlineIgnoringFieldEditor(nil)
            return .handled
        }
        if editingModifiers.isEmpty {
            send()
            return .handled
        }
        return .ignored
    }
}
#endif
