import AppKit
import SwiftUI

@MainActor final class Draft: ObservableObject {
    @Published var text = "draft"
    var sends = 0
}
struct Harness: View {
    @StateObject var draft = Draft()
    var body: some View {
        TextField("Synthetic keyboard test", text: $draft.text, axis: .vertical)
            .onSubmit { draft.sends += 1 }
            .modifier(ComposerReturnKey { draft.sends += 1 })
            .padding().frame(width: 400, height: 160)
            .task { await run() }
    }
    @MainActor func run() async {
        try? await Task.sleep(for: .seconds(1))
        NSApp.activate(ignoringOtherApps: true)
        guard let window = NSApp.windows.first(where: { $0.contentView != nil }) else { finish(["error": "no window"]); return }
        window.makeKeyAndOrderFront(nil)
        try? await Task.sleep(for: .seconds(1))
        func field(_ view: NSView) -> NSTextField? {
            if let v = view as? NSTextField { return v }
            return view.subviews.lazy.compactMap { field($0) }.first
        }
        guard let input = field(window.contentView!) else { finish(["error": "no field"]); return }
        window.makeFirstResponder(input); input.selectText(nil)
        guard let editor = window.firstResponder as? NSTextView else { finish(["error":"no editor"]); return }
        editor.setSelectedRange(NSRange(location: 5, length: 0))
        func press(_ modifiers: NSEvent.ModifierFlags) {
            let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers,
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                context: nil, characters: "\r", charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36)!
            NSApp.postEvent(event, atStart: false)
        }
        press(.shift)
        try? await Task.sleep(for: .milliseconds(300))
        let shifted = draft.text; let shiftedSends = draft.sends
        press([])
        try? await Task.sleep(for: .milliseconds(300))
        finish(["keyWindow": NSApp.keyWindow === window, "shiftText": shifted,
                "shiftSends": shiftedSends, "enterText": draft.text, "enterSends": draft.sends])
    }
    func finish(_ value: [String: Any]) {
        let data = try! JSONSerialization.data(withJSONObject: value, options: .prettyPrinted)
        try! data.write(to: URL(fileURLWithPath: String(CommandLine.arguments.first { $0.hasPrefix("--result=") }!.dropFirst("--result=".count))))
        NSApp.terminate(nil)
    }
}
@main struct KeyHarness: App {
    init() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 15) { NSApp.terminate(nil) }
    }
    var body: some Scene { WindowGroup { Harness() } }
}
