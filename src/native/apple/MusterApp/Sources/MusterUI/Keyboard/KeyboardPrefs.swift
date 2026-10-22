// The in-app keyboard's shared choices, its dictionary, and what a key does
// to the field it is bound to. iOS only: a Mac has a hardware keyboard.
#if canImport(SwiftUI) && os(iOS)
import Combine
import Foundation
import MusterCore

/// Keyboard choices shared by every field: the system keyboard instead of
/// the in-app one, the same choice for terminals, and the last language. In
/// text fields the in-app keyboard is off until it has been verified on a
/// device, so typing never depends on it; a terminal uses it by default,
/// since the system keyboard has no Esc, F-keys, Ctrl or Alt.
@MainActor
final class KeyboardPrefs: ObservableObject {
    static let shared = KeyboardPrefs()
    @Published var systemKeyboard: Bool { didSet { defaults.set(systemKeyboard, forKey: "systemKeyboard") } }
    /// The system keyboard in a terminal instead of the in-app one.
    @Published var terminalSystemKeyboard: Bool { didSet { defaults.set(terminalSystemKeyboard, forKey: "terminalSystemKeyboard") } }
    @Published var chinese: Bool { didSet { defaults.set(chinese, forKey: "keyboardChinese") } }
    private let defaults = UserDefaults.standard

    private init() {
        systemKeyboard = defaults.object(forKey: "systemKeyboard") as? Bool ?? true
        terminalSystemKeyboard = defaults.object(forKey: "terminalSystemKeyboard") as? Bool ?? false
        chinese = defaults.object(forKey: "keyboardChinese") as? Bool ?? true
    }
}

/// The dictionary loads once, off the first keyboard, and one composer keeps what the user picks.
@MainActor
enum Pinyin {
    static var composer: PinyinComposer?
    private static var loading = false
    private static var waiting: [(PinyinComposer) -> Void] = []

    static func load(_ done: @escaping (PinyinComposer) -> Void) {
        if let composer { done(composer); return }
        waiting.append(done)
        if loading { return }
        loading = true
        DispatchQueue.global(qos: .userInitiated).async {
            let dict = try? PinyinDict.bundled()
            DispatchQueue.main.async {
                loading = false
                let callbacks = waiting
                waiting.removeAll()
                guard let dict else { return }
                let composer = PinyinComposer(dict: dict)
                self.composer = composer
                callbacks.forEach { $0(composer) }
            }
        }
    }
}

/// Caret moves, deletions and clipboard edits a text field takes from the MacBook layout.
enum KeyboardEdit {
    case left, right, up, down, wordLeft, wordRight, lineStart, lineEnd, start, end
    case deleteForward, deleteWordBack, deleteToLineStart, deleteToLineEnd, selectAll, copy, cut, tab
}

/// What a key does to the field it is bound to.
struct KeyboardTarget {
    var insert: (String) -> Void
    var backspace: () -> Void
    var enter: () -> Void
    var hide: () -> Void
    /// Named keys for a terminal (escape, f5, ctrl-c, alt-b…); fields without it get no Esc or F-keys.
    var key: ((String) -> Void)?
    /// Text fields: arrows, word and line moves, deletions and the clipboard; `extend` (Shift) moves only the selection's head.
    var edit: ((KeyboardEdit, Bool) -> Void)?
}

/// The keyboard's heights, so its UIKit input view can size itself before SwiftUI lays it out.
enum KeyboardMetrics {
    static let keyHeight: CGFloat = 42
    static let compactKeyHeight: CGFloat = 34
    /// From this width the Mac keyboard shows full corner legends.
    static let macMinWidth: CGFloat = 560

    static func height(wide: Bool, compact: Bool, raw: Bool) -> CGFloat {
        let key = compact ? compactKeyHeight : keyHeight
        let gap: CGFloat = compact ? 4 : 6
        let top: CGFloat = compact ? 36 : 46
        let function: CGFloat = (key * 0.7).rounded() + gap
        return 4 + top + gap + function + 5 * key + 4 * gap + (compact ? 4 : 8)
    }
}
#endif
