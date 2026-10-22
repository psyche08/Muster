// Text fields that type with the in-app keyboard (or the system keyboard
// when Settings asks for it): the keyboard is the field's UIKit input view,
// and edits go through UITextInput, so the field keeps its caret and
// selection. SwiftUI's TextField cannot take an input view; these wrappers
// replace it on iOS where a field should offer the Muster keyboard.
#if canImport(SwiftUI) && os(iOS)
import MusterCore
import SwiftUI
import UIKit

/// The keyboard as a UIKit input view, sized for the window it is in.
final class KeyboardInputView: UIInputView {
    private let host: UIHostingController<AnyView>
    private let compact: Bool
    private let raw: Bool
    private var height: CGFloat = 0
    private var heightConstraint: NSLayoutConstraint?

    init(target: KeyboardTarget, enterLabel: String, compact: Bool) {
        self.compact = compact
        raw = target.key != nil
        host = UIHostingController(rootView: AnyView(MusterKeyboard(target: target, enterLabel: enterLabel, compact: compact)))
        super.init(frame: CGRect(x: 0, y: 0, width: 320, height: 280), inputViewStyle: .keyboard)
        allowsSelfSizing = true
        host.view.backgroundColor = .clear
        host.view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(host.view)
        NSLayoutConstraint.activate([
            host.view.leadingAnchor.constraint(equalTo: leadingAnchor),
            host.view.trailingAnchor.constraint(equalTo: trailingAnchor),
            host.view.topAnchor.constraint(equalTo: topAnchor),
            host.view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        heightConstraint = heightAnchor.constraint(equalToConstant: 280)
        heightConstraint?.priority = UILayoutPriority(999)
        heightConstraint?.isActive = true
        update(width: UIScreen.main.bounds.width)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override var intrinsicContentSize: CGSize { CGSize(width: UIView.noIntrinsicMetric, height: height) }

    override func layoutSubviews() {
        super.layoutSubviews()
        update(width: window?.bounds.width ?? bounds.width)
    }

    private func update(width: CGFloat) {
        let wanted = KeyboardMetrics.height(wide: width >= KeyboardMetrics.macMinWidth, compact: compact, raw: raw)
        if wanted != height {
            height = wanted
            heightConstraint?.constant = wanted
            frame.size.height = wanted
            invalidateIntrinsicContentSize()
        }
    }
}

/// Edits the field the keyboard is attached to, through UITextInput: the
/// field keeps the caret, and the keyboard's arrow, word and line keys move
/// it by the same arithmetic as the HarmonyOS shell (TextEdits).
@MainActor
final class TextInputTarget {
    weak var input: (UIView & UITextInput)?
    /// What Enter does; nil inserts a line break in a multiline field and closes a single-line one.
    var submit: (() -> Void)?
    var multiline = true
    /// Called after a programmatic replacement, so a SwiftUI binding can catch up.
    var changed: (() -> Void)?
    /// The selection: the anchor stays put while Shift-arrows move the head.
    private var anchor = 0
    private var head = 0

    func target(key: ((String) -> Void)? = nil) -> KeyboardTarget {
        KeyboardTarget(
            insert: { [weak self] text in self?.withEditableInput { $0.insertText(text) } },
            backspace: { [weak self] in self?.withEditableInput { $0.deleteBackward() } },
            enter: { [weak self] in
                guard let self else { return }
                withEditableInput { input in
                    if let submit = self.submit { submit() } else if self.multiline { input.insertText("\n") } else { input.resignFirstResponder() }
                }
            },
            hide: { [weak self] in self?.input?.resignFirstResponder() },
            key: key,
            edit: { [weak self] edit, extend in self?.withEditableInput { _ in self?.apply(edit, extend) } }
        )
    }

    private func withEditableInput(_ action: (UIView & UITextInput) -> Void) {
        guard let input, input.window != nil else { return }
        if let view = input as? UITextView, !view.isEditable { return }
        if let field = input as? UITextField, !field.isEnabled { return }
        if !input.isFirstResponder { input.becomeFirstResponder() }
        action(input)
    }

    private var text: String {
        guard let input, let range = input.textRange(from: input.beginningOfDocument, to: input.endOfDocument) else { return "" }
        return input.text(in: range) ?? ""
    }

    private func offset(_ position: UITextPosition) -> Int {
        guard let input else { return 0 }
        return input.offset(from: input.beginningOfDocument, to: position)
    }

    /// The field's selection, ordered; the keyboard's own moves keep their direction.
    private func sync() {
        guard let input, let range = input.selectedTextRange else { return }
        let start = offset(range.start), end = offset(range.end)
        if min(anchor, head) != start || max(anchor, head) != end { anchor = start; head = end }
    }

    private func place(_ anchor: Int, _ head: Int? = nil) {
        let head = head ?? anchor
        self.anchor = anchor
        self.head = head
        guard let input,
              let start = input.position(from: input.beginningOfDocument, offset: min(anchor, head)),
              let end = input.position(from: input.beginningOfDocument, offset: max(anchor, head)) else { return }
        input.selectedTextRange = input.textRange(from: start, to: end)
    }

    private func replace(_ a: Int, _ b: Int, _ text: String) {
        guard let input,
              let start = input.position(from: input.beginningOfDocument, offset: a),
              let end = input.position(from: input.beginningOfDocument, offset: b),
              let range = input.textRange(from: start, to: end) else { return }
        input.replace(range, withText: text)
        place(a + text.utf16.count)
        changed?()
    }

    private func substring(_ value: String, _ a: Int, _ b: Int) -> String {
        let units = Array(value.utf16)
        return String(utf16CodeUnits: Array(units[min(a, units.count)..<min(b, units.count)]), count: max(0, min(b, units.count) - min(a, units.count)))
    }

    private func moved(_ value: String, _ edit: KeyboardEdit, _ pos: Int) -> Int {
        switch edit {
        case .left: TextEdits.charLeft(value, pos)
        case .right: TextEdits.charRight(value, pos)
        case .up: TextEdits.lineUp(value, pos)
        case .down: TextEdits.lineDown(value, pos)
        case .wordLeft: TextEdits.wordLeft(value, pos)
        case .wordRight: TextEdits.wordRight(value, pos)
        case .lineStart: TextEdits.lineStart(value, pos)
        case .lineEnd: TextEdits.lineEnd(value, pos)
        case .start: 0
        default: value.utf16.count
        }
    }

    private func apply(_ edit: KeyboardEdit, _ extend: Bool) {
        guard input != nil else { return }
        let value = text
        sync()
        let a = max(0, min(anchor, head, value.utf16.count))
        let b = min(value.utf16.count, max(anchor, head))
        switch edit {
        case .selectAll: place(0, value.utf16.count); return
        case .copy: if a < b { UIPasteboard.general.string = substring(value, a, b) }; return
        case .cut:
            if a < b {
                UIPasteboard.general.string = substring(value, a, b)
                replace(a, b, "")
            }
            return
        case .tab: if multiline { replace(a, b, "\t") }; return
        case .deleteForward: replace(a, a < b ? b : TextEdits.charRight(value, a), ""); return
        case .deleteWordBack: replace(a < b ? a : TextEdits.wordLeft(value, a), b, ""); return
        case .deleteToLineStart:
            let start = TextEdits.lineStart(value, a)
            replace(a < b ? a : (start == a ? TextEdits.charLeft(value, a) : start), b, "")
            return
        case .deleteToLineEnd:
            let end = TextEdits.lineEnd(value, b)
            replace(a, a < b ? b : (end == b ? TextEdits.charRight(value, b) : end), "")
            return
        default: break
        }
        if extend {
            place(min(anchor, value.utf16.count), moved(value, edit, min(head, value.utf16.count)))
            return
        }
        // A selection collapses to its own edge first, as on a Mac.
        if a < b && edit == .left { place(a); return }
        if a < b && edit == .right { place(b); return }
        let backward = [KeyboardEdit.left, .up, .wordLeft, .lineStart].contains(edit)
        place(moved(value, edit, backward ? a : b))
    }
}

/// A persistent on-screen keyboard still edits through UITextInput; UIKit
/// focus controls the caret, not whether this lower-pane view exists.
@MainActor
final class KeyboardDock: ObservableObject {
    let target = TextInputTarget()
}

private struct KeyboardDockKey: EnvironmentKey {
    static let defaultValue: KeyboardDock? = nil
}

extension EnvironmentValues {
    var keyboardDock: KeyboardDock? {
        get { self[KeyboardDockKey.self] }
        set { self[KeyboardDockKey.self] = newValue }
    }
}

/// Attaches the Muster keyboard to a UIKit field, or returns it to the system keyboard.
@MainActor
final class KeyboardAttachment {
    private(set) var target = TextInputTarget()
    private var inputView: KeyboardInputView?
    private let suppressedInput = UIView(frame: .zero)
    private var attachedCompact = false
    private var attachedLabel = ""

    func attach(to field: UIView & UITextInput, systemKeyboard: Bool, enterLabel: String, compact: Bool,
                submit: (() -> Void)?, multiline: Bool, changed: (() -> Void)?, dock: KeyboardDock? = nil) {
        let active = dock?.target ?? target
        active.input = field
        active.submit = submit
        active.multiline = multiline
        active.changed = changed
        let current = currentInputView(of: field)
        if dock != nil {
            if current !== suppressedInput { setInputView(suppressedInput, of: field) }
            return
        }
        if systemKeyboard {
            if current != nil { setInputView(nil, of: field) }
            return
        }
        if inputView == nil || attachedCompact != compact || attachedLabel != enterLabel {
            inputView = KeyboardInputView(target: target.target(), enterLabel: enterLabel, compact: compact)
            attachedCompact = compact
            attachedLabel = enterLabel
        }
        if current !== inputView { setInputView(inputView, of: field) }
    }

    private func currentInputView(of field: UIView) -> UIView? {
        (field as? UITextView)?.inputView ?? (field as? UITextField)?.inputView
    }

    private func setInputView(_ view: UIView?, of field: UIView) {
        if let textView = field as? UITextView { textView.inputView = view }
        else if let textField = field as? UITextField { textField.inputView = view }
        // Let SwiftUI finish its view transaction before UIKit presents the
        // newly assigned keyboard.
        if field.isFirstResponder {
            DispatchQueue.main.async { [weak field] in
                guard let field, field.isFirstResponder else { return }
                field.reloadInputViews()
            }
        }
    }
}

/// A single-line field. Enter submits when the owner handles it, otherwise closes the keyboard.
struct MusterTextField: UIViewRepresentable {
    @Binding var text: String
    var placeholder = ""
    var monospace = false
    var enterLabel = "完成"
    var onSubmit: (() -> Void)?
    @ObservedObject private var prefs = KeyboardPrefs.shared

    init(_ placeholder: String, text: Binding<String>, monospace: Bool = false, enterLabel: String = "完成", onSubmit: (() -> Void)? = nil) {
        self.placeholder = placeholder
        _text = text
        self.monospace = monospace
        self.enterLabel = enterLabel
        self.onSubmit = onSubmit
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.delegate = context.coordinator
        field.addTarget(context.coordinator, action: #selector(Coordinator.changed(_:)), for: .editingChanged)
        field.borderStyle = .none
        field.returnKeyType = .done
        field.autocapitalizationType = .none
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return field
    }

    func updateUIView(_ field: UITextField, context: Context) {
        context.coordinator.parent = self
        if field.text != text { field.text = text }
        field.placeholder = placeholder
        let size = UIFont.preferredFont(forTextStyle: .body).pointSize
        field.font = monospace ? .monospacedSystemFont(ofSize: size, weight: .regular) : .preferredFont(forTextStyle: .body)
        field.autocorrectionType = monospace ? .no : .default
        context.coordinator.keyboard.attach(to: field, systemKeyboard: prefs.systemKeyboard, enterLabel: enterLabel, compact: false,
                                            submit: onSubmit, multiline: false, changed: { [weak field] in field.map { context.coordinator.changed($0) } })
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: MusterTextField
        let keyboard = KeyboardAttachment()
        init(_ parent: MusterTextField) { self.parent = parent }

        @objc func changed(_ field: UITextField) { parent.text = field.text ?? "" }

        func textFieldShouldReturn(_ field: UITextField) -> Bool {
            if let submit = parent.onSubmit { submit() } else { field.resignFirstResponder() }
            return true
        }
    }
}

/// A multi-line field that grows with its text, up to a maximum height.
struct MusterTextEditor: UIViewRepresentable {
    @Binding var text: String
    var placeholder = ""
    var monospace = false
    var minimumHeight: CGFloat = 36
    var maximumHeight: CGFloat = 180
    /// Take focus (and so show the keyboard) on appearing and whenever this turns true.
    var autoFocus = false
    /// A shorter keyboard that fits below a half-fold crease.
    var compactKeyboard = false
    @ObservedObject private var prefs = KeyboardPrefs.shared

    init(_ placeholder: String, text: Binding<String>, monospace: Bool = false, minimumHeight: CGFloat = 36,
         maximumHeight: CGFloat = 180, autoFocus: Bool = false, compactKeyboard: Bool = false) {
        self.placeholder = placeholder
        _text = text
        self.monospace = monospace
        self.minimumHeight = minimumHeight
        self.maximumHeight = maximumHeight
        self.autoFocus = autoFocus
        self.compactKeyboard = compactKeyboard
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> PlaceholderTextView {
        let view = PlaceholderTextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        view.textContainerInset = UIEdgeInsets(top: 8, left: 4, bottom: 8, right: 4)
        view.isScrollEnabled = true
        view.autocapitalizationType = .none
        if autoFocus { view.requestKeyboardFocus() }
        return view
    }

    func updateUIView(_ view: PlaceholderTextView, context: Context) {
        context.coordinator.parent = self
        if view.text != text { view.text = text; view.placeholderChanged() }
        view.placeholder = placeholder
        let size = UIFont.preferredFont(forTextStyle: .body).pointSize
        view.font = monospace ? .monospacedSystemFont(ofSize: size, weight: .regular) : .preferredFont(forTextStyle: .body)
        view.autocorrectionType = monospace ? .no : .default
        if autoFocus && !context.coordinator.focused { context.coordinator.focused = true; view.requestKeyboardFocus() }
        if !autoFocus { context.coordinator.focused = false }
        context.coordinator.keyboard.attach(to: view, systemKeyboard: prefs.systemKeyboard, enterLabel: "换行", compact: compactKeyboard,
                                            submit: nil, multiline: true, changed: { [weak view] in view.map { context.coordinator.textViewDidChange($0) } })
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: PlaceholderTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 300
        let height = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        return CGSize(width: width, height: min(maximumHeight, max(minimumHeight, height)))
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MusterTextEditor
        var focused = false
        let keyboard = KeyboardAttachment()
        init(_ parent: MusterTextEditor) { self.parent = parent }

        func textViewDidChange(_ textView: UITextView) {
            parent.text = textView.text
            (textView as? PlaceholderTextView)?.placeholderChanged()
        }
    }
}

/// Defer focus until UIKit has attached the field to a window. A single
/// dispatch from make/updateUIView can run too early during navigation.
class KeyboardTextView: UITextView {
    private var pendingKeyboardFocus = false

    func requestKeyboardFocus() {
        pendingKeyboardFocus = true
        restoreKeyboardFocus()
    }

    func restoreKeyboardFocus() {
        guard pendingKeyboardFocus, window != nil, isEditable else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self, self.pendingKeyboardFocus, self.window != nil, self.isEditable else { return }
            if self.becomeFirstResponder() { self.pendingKeyboardFocus = false }
        }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        restoreKeyboardFocus()
    }
}

/// A UITextView with a placeholder label, as UITextField has.
final class PlaceholderTextView: KeyboardTextView {
    private let label = UILabel()
    var placeholder = "" {
        didSet { label.text = placeholder }
    }

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        label.textColor = .placeholderText
        label.numberOfLines = 1
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 9),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 8),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override var font: UIFont? {
        didSet { label.font = font }
    }

    func placeholderChanged() { label.isHidden = !text.isEmpty }
}
#endif
