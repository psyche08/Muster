// Terminals: the list, and one session drawn as a grid of styled cells
// (ARCHITECTURE §3.6.3). The shell draws rows and the cursor and forwards
// keys; the core owns the screen state.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

/// One segmented control in the existing navigation row, no separate header.
struct WorkspaceSwitcher: View {
    let core: MusterCore
    let vm: ViewModel

    /// The tab the core says the screen belongs to (Home counts as Agents).
    private var selected: Muster_App_Screen {
        [.multica, .terminals].contains(vm.workspace) ? vm.workspace : .agents
    }

    var body: some View {
        Picker("Workspace", selection: Binding(get: { selected }, set: { tab in
            switch tab {
            case .multica: core.navigate(to: .multica)
            case .terminals: core.openTerminals(device: vm.agents.deviceID.isEmpty ? "*" : vm.agents.deviceID)
            default: core.openAgents(device: vm.terminals.deviceID.isEmpty ? "*" : vm.terminals.deviceID)
            }
        })) {
            Text("Agents").tag(Muster_App_Screen.agents)
            Text("Terminals").tag(Muster_App_Screen.terminals)
            if vm.multica.installed { Text("Multica").tag(Muster_App_Screen.multica) }
        }
        .pickerStyle(.segmented)
        .frame(width: 270)
        .accessibilityIdentifier("workspace-switcher")
    }
}

extension Muster_V1_TerminalSession {
    var sourceKey: String { "\(deviceID):\(id)" }
}

struct TerminalsView: View {
    let core: MusterCore
    let vm: ViewModel
    @State private var ssh = false

    var body: some View {
        List {
            if vm.terminals.loading && vm.terminals.sessions.isEmpty {
                ProgressView("Loading terminals…")
            } else if vm.terminals.sessions.isEmpty {
                ContentUnavailableView("No terminals", systemImage: "terminal", description: Text("Terminal sessions on the selected devices show up here."))
            }
            ForEach(vm.terminals.errors, id: \.self) { error in
                Label(error, systemImage: "wifi.exclamationmark").font(.caption).foregroundStyle(.secondary)
            }
            ForEach(vm.terminals.sessions, id: \.sourceKey) { session in
                Button { core.openTerminal(session.id, device: session.deviceID) } label: {
                    HStack {
                        Image(systemName: "terminal")
                        VStack(alignment: .leading) {
                            Text(session.title.isEmpty ? session.id : session.title).font(.headline).lineLimit(1)
                            Text("\(session.deviceName) · \(session.source) · \(session.cols)×\(session.rows)").font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            DeviceTabs(devices: vm.serviceDevices, selected: vm.terminals.deviceID, select: { core.openTerminals(device: $0) }, details: { core.selectDevice($0) })
        }
        .navigationTitle("")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .refreshable { core.openTerminals() }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                if vm.terminals.canCreate {
                    Menu {
                        ForEach(vm.terminals.sources, id: \.source) { source in
                            Button("Open in \(source.label)") { core.terminalCreate(source: source.source) }
                        }
                    } label: { Label("New window", systemImage: "plus.rectangle") }
                    .disabled(vm.busy)
                }
                if vm.terminals.sshAvailable {
                    Button { ssh = true } label: { Label("SSH", systemImage: "lock.shield") }
                }
            }
        }
        .sheet(isPresented: $ssh) {
            SshSheet { user, port in core.openSsh(port: port, user: user, cols: 80, rows: 24) }
        }
    }
}

/// Only displays commands supplied by Shared Rust; never types into a live shell.
struct DesktopTerminalAttachSheet: View {
    let terminal: Muster_App_TerminalView
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("Run a command in a new terminal on the Mac hosting this session. Both windows connect to the same running shell. Muster controls the viewport size.")
                        .font(.callout)
                }
                Section("Ghostty or iTerm2") {
                    command(terminal.desktopAttachCommand)
                }
                Section("iTerm2 native tabs and panes") {
                    command(terminal.iterm2AttachCommand)
                }
                Section {
                    Text("Closing this view keeps the shell running. Use End terminal in Muster to stop it.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Connect on the host Mac")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
        .frame(minWidth: 300, minHeight: 360)
    }

    @ViewBuilder private func command(_ text: String) -> some View {
        Text(text).font(.caption.monospaced()).textSelection(.enabled)
        Button("Copy command") { copyToPasteboard(text) }
            .disabled(text.isEmpty || terminal.ended)
    }
}

struct TerminalScreen: View {
    let core: MusterCore
    let vm: ViewModel
    @Environment(\.workspaceLayout) private var layout
    #if os(iOS)
    @ObservedObject private var keyboardPrefs = KeyboardPrefs.shared
    #endif

    var body: some View {
        VStack(spacing: 0) {
            // Top and bottom: the grid stays above the crease; keys and status go below it.
            TerminalGridView(terminal: vm.terminal, core: core)
                .frame(height: layout.flex ? layout.readingHeight : nil)
            if layout.flex { Color.clear.frame(height: layout.hingeHeight) }
            #if os(iOS)
            // Clipboard access stays visible with either keyboard, including when it is hidden.
            KeyBar(core: core, canPaste: vm.terminal.canPaste, showsKeys: keyboardPrefs.terminalSystemKeyboard)
            #endif
            if vm.terminal.ended || !vm.terminal.status.isEmpty {
                HStack {
                    Text(vm.terminal.ended ? "Ended · \(vm.terminal.status)" : vm.terminal.status)
                        .font(.footnote).foregroundStyle(vm.terminal.ended ? .orange : .secondary)
                    Spacer()
                }
                .padding(.horizontal, 8).padding(.vertical, 4)
                .background(.bar)
            }
            if layout.flex { Spacer(minLength: 0) }
        }
        .navigationTitle(vm.terminal.session.isEmpty ? "Terminal" : "\(vm.terminal.source) · \(String(vm.terminal.session.prefix(8)))")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                #if os(macOS)
                TerminalPasteButton(core: core, canPaste: vm.terminal.canPaste)
                #endif
                Button { core.closeTerminal() } label: { Label("Close", systemImage: "xmark.circle") }
            }
        }
    }
}

/// A user-initiated system clipboard read; Rust owns validation and terminal input semantics.
struct TerminalPasteButton: View {
    let core: MusterCore
    let canPaste: Bool

    var body: some View {
        PasteButton(payloadType: String.self) { strings in
            let text = strings.joined(separator: "\n")
            guard !text.isEmpty else { return }
            Task { @MainActor in core.terminalPaste(text) }
        }
        .labelStyle(.titleAndIcon)
        .buttonBorderShape(.capsule)
        .controlSize(.small)
        .disabled(!canPaste)
        .accessibilityIdentifier("terminal-paste")
    }
}

#if os(iOS)
struct KeyBar: View {
    let core: MusterCore
    let canPaste: Bool
    let showsKeys: Bool

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                TerminalPasteButton(core: core, canPaste: canPaste)
                // The built-in keyboard already carries these special keys.
                if showsKeys {
                    key("esc", .escape); key("tab", .tab)
                    key("^C", .ctrlC); key("^D", .ctrlD); key("^Z", .ctrlZ); key("^L", .ctrlL)
                    key("←", .left); key("↓", .down); key("↑", .up); key("→", .right)
                    key("home", .home); key("end", .end); key("pgup", .pageup); key("pgdn", .pagedown)
                }
            }
            .padding(.horizontal, 8).padding(.vertical, 6)
        }
        .background(.bar)
    }

    private func key(_ label: String, _ key: TerminalKey) -> some View {
        Button(label) { core.terminalKey(key) }
            .font(.system(.footnote, design: .monospaced))
            .buttonStyle(.bordered)
            .controlSize(.small)
    }
}
#endif

// MARK: - Grid

enum TerminalMetrics {
    static let fontSize: CGFloat = 13
    /// The smallest font a fitted grid shrinks to before it scrolls.
    static let minimumFontSize: CGFloat = 8

    static func font(_ size: CGFloat) -> Font { .system(size: size, design: .monospaced) }

    /// Width and height of one cell in points at `size`.
    static func cell(_ size: CGFloat) -> CGSize {
        #if os(iOS) || os(macOS)
        #if os(iOS)
        let native = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        #else
        let native = NSFont.monospacedSystemFont(ofSize: size, weight: .regular)
        #endif
        let measured = ("M" as NSString).size(withAttributes: [.font: native])
        return CGSize(width: ceil(measured.width * 2) / 2, height: ceil(measured.height))
        #else
        return CGSize(width: size * 0.6, height: size * 1.2)
        #endif
    }

    static let cell: CGSize = cell(fontSize)

    /// The font that fits `cols` into `width`, between the minimum and the
    /// default size.
    static func fittingFontSize(cols: Int, width: CGFloat) -> CGFloat {
        let ratio = cell(fontSize).width / fontSize
        let ideal = width / (CGFloat(max(cols, 1)) * ratio)
        return min(fontSize, max(minimumFontSize, floor(ideal * 2) / 2))
    }
}

/// The grid. A resizable terminal (our SSH or PTY session) fills the view
/// and reports its size to the core; somebody's iTerm2 window is drawn at
/// the size it has, shrunk to fit the width and scrolled beyond that.
struct TerminalGridView: View {
    let terminal: Muster_App_TerminalView
    let core: MusterCore
    @Environment(\.workspaceLayout) private var layout
    @State private var reported: (UInt32, UInt32) = (0, 0)
    /// Cells selected for copying (a long press then a drag on iOS, a mouse
    /// drag on macOS); display state only.
    @State private var selection: TerminalSelection?

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .topLeading) {
                if terminal.resizable {
                    canvas(fontSize: TerminalMetrics.fontSize)
                        .onAppear { report(geo.size) }
                        .onChange(of: geo.size) { _, size in report(size) }
                } else {
                    let fontSize = TerminalMetrics.fittingFontSize(cols: Int(terminal.cols), width: geo.size.width)
                    let cell = TerminalMetrics.cell(fontSize)
                    ScrollView([.horizontal, .vertical], showsIndicators: true) {
                        canvas(fontSize: fontSize)
                            .frame(
                                width: CGFloat(max(terminal.cols, 1)) * cell.width,
                                height: CGFloat(max(terminal.rows, 1)) * cell.height
                            )
                    }
                }
            }
            .background(Color.black)
        }
        .padding(4)
        .background(Color.black)
        .onChange(of: terminal.session) { _, _ in selection = nil }
    }

    /// The grid, under the view that takes keys, touches and the clipboard.
    private func canvas(fontSize: CGFloat) -> some View {
        Canvas(rendersAsynchronously: false) { context, _ in
            draw(in: &context, fontSize: fontSize)
        }
        .allowsHitTesting(false)
        .overlay {
            TerminalInputView(
                core: core,
                canPaste: terminal.canPaste,
                selectedText: { selection.map { $0.text(in: terminal.lines, columns: Int(terminal.cols)) } },
                onSelect: { phase, point in select(phase, at: point, fontSize: fontSize) },
                onClear: { selection = nil },
                compactKeyboard: layout.flex
            )
        }
    }

    private func select(_ phase: TerminalSelectPhase, at point: CGPoint, fontSize: CGFloat) {
        let size = TerminalMetrics.cell(fontSize)
        let rows = Int(max(terminal.rows, 1)), cols = Int(max(terminal.cols, 1))
        let cell = TerminalCell(
            row: min(max(Int(point.y / size.height), 0), rows - 1),
            col: min(max(Int(point.x / size.width), 0), cols - 1)
        )
        switch phase {
        case .began: selection = TerminalSelection(at: cell)
        case .changed: selection?.head = cell
        case .ended:
            selection?.head = cell
            // A press without a drag selects nothing: its menu offers Paste.
            if selection?.isEmpty == true { selection = nil }
        }
    }

    private func report(_ size: CGSize) {
        let cell = TerminalMetrics.cell
        let cols = UInt32(max(20, Int(size.width / cell.width)))
        let rows = UInt32(max(5, Int(size.height / cell.height)))
        if (cols, rows) != reported {
            reported = (cols, rows)
            core.terminalResize(cols: cols, rows: rows)
        }
    }

    private func draw(in context: inout GraphicsContext, fontSize: CGFloat) {
        let cell = TerminalMetrics.cell(fontSize)
        let font = TerminalMetrics.font(fontSize)
        let cols = Int(max(terminal.cols, 1))
        for line in terminal.lines {
            let y = CGFloat(line.index) * cell.height
            let characters = Array(line.text)
            let styles = TerminalStyleRun.perCell(line.styles, columns: cols)
            var column = 0
            while column < characters.count {
                let style = column < styles.count ? styles[column] : nil
                var end = column + 1
                while end < characters.count, (end < styles.count ? styles[end] : nil) == style { end += 1 }
                let text = String(characters[column..<end])
                let x = CGFloat(column) * cell.width
                let rect = CGRect(x: x, y: y, width: CGFloat(end - column) * cell.width, height: cell.height)
                let (fg, bg) = colors(style)
                if let bg { context.fill(Path(rect), with: .color(bg)) }
                var t = Text(text).font(font).foregroundColor(fg)
                if let style {
                    if style.flags.contains(.bold) { t = t.bold() }
                    if style.flags.contains(.italic) { t = t.italic() }
                    if style.flags.contains(.underline) { t = t.underline() }
                    if style.flags.contains(.strike) { t = t.strikethrough() }
                }
                // One run is one line: never wrap inside the rect.
                context.draw(context.resolve(t), at: CGPoint(x: rect.minX, y: rect.midY), anchor: .leading)
                column = end
            }
        }
        if let selection {
            for row in selection.start.row...selection.end.row {
                guard let columns = selection.columns(inRow: row, columns: cols) else { continue }
                let rect = CGRect(
                    x: CGFloat(columns.lowerBound) * cell.width, y: CGFloat(row) * cell.height,
                    width: CGFloat(columns.count) * cell.width, height: cell.height
                )
                context.fill(Path(rect), with: .color(Color.accentColor.opacity(0.35)))
            }
        }
        if !terminal.ended {
            let cursor = CGRect(
                x: CGFloat(terminal.cursorX) * cell.width, y: CGFloat(terminal.cursorY) * cell.height,
                width: cell.width, height: cell.height
            )
            context.fill(Path(cursor), with: .color(Color.white.opacity(0.6)))
        }
    }

    private func colors(_ style: TerminalStyleRun?) -> (Color, Color?) {
        guard let style else { return (Color.white, nil) }
        var fg = TerminalPalette.color(style.foreground) ?? Color.white
        var bg = TerminalPalette.color(style.background)
        if style.flags.contains(.inverse) {
            let swapped = fg
            fg = bg ?? Color.black
            bg = swapped
        }
        if style.flags.contains(.dim) { fg = fg.opacity(0.6) }
        return (fg, bg)
    }
}

enum TerminalPalette {
    private static let basic: [Color] = [
        Color(red: 0, green: 0, blue: 0), Color(red: 0.8, green: 0.2, blue: 0.2), Color(red: 0.2, green: 0.7, blue: 0.2), Color(red: 0.8, green: 0.7, blue: 0.2),
        Color(red: 0.3, green: 0.4, blue: 0.9), Color(red: 0.7, green: 0.3, blue: 0.7), Color(red: 0.2, green: 0.7, blue: 0.7), Color(red: 0.85, green: 0.85, blue: 0.85),
        Color(red: 0.4, green: 0.4, blue: 0.4), Color(red: 1, green: 0.4, blue: 0.4), Color(red: 0.4, green: 0.9, blue: 0.4), Color(red: 1, green: 0.9, blue: 0.4),
        Color(red: 0.5, green: 0.6, blue: 1), Color(red: 0.9, green: 0.5, blue: 0.9), Color(red: 0.4, green: 0.9, blue: 0.9), Color(red: 1, green: 1, blue: 1),
    ]

    static func color(_ c: TerminalStyleRun.Color) -> Color? {
        switch c {
        case .default: return nil
        case .rgb(let r, let g, let b): return Color(red: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255)
        case .palette(let index):
            let i = Int(index)
            if i < 16 { return basic[i] }
            if i < 232 {
                let n = i - 16
                let steps: [Double] = [0, 0.37, 0.53, 0.68, 0.84, 1]
                return Color(red: steps[n / 36], green: steps[(n / 6) % 6], blue: steps[n % 6])
            }
            let gray = Double(8 + (i - 232) * 10) / 255
            return Color(red: gray, green: gray, blue: gray)
        }
    }
}

// MARK: - Key input

enum TerminalSelectPhase { case began, changed, ended }

#if os(macOS)
struct TerminalInputView: NSViewRepresentable {
    let core: MusterCore
    var canPaste = false
    var selectedText: () -> String? = { nil }
    var onSelect: (TerminalSelectPhase, CGPoint) -> Void = { _, _ in }
    var onClear: () -> Void = {}
    var compactKeyboard = false

    func makeNSView(context: Context) -> KeyCatcher {
        let view = KeyCatcher()
        update(view)
        DispatchQueue.main.async { view.window?.makeFirstResponder(view) }
        return view
    }

    func updateNSView(_ view: KeyCatcher, context: Context) { update(view) }

    private func update(_ view: KeyCatcher) {
        view.core = core
        view.canPaste = canPaste
        view.selectedText = selectedText
        view.onSelect = onSelect
        view.onClear = onClear
    }

    final class KeyCatcher: NSView, NSUserInterfaceValidations {
        var core: MusterCore?
        var canPaste = false
        var selectedText: () -> String? = { nil }
        var onSelect: (TerminalSelectPhase, CGPoint) -> Void = { _, _ in }
        var onClear: () -> Void = {}
        override var acceptsFirstResponder: Bool { true }
        // Top-left origin, like the canvas.
        override var isFlipped: Bool { true }

        override func mouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            onClear()
            onSelect(.began, convert(event.locationInWindow, from: nil))
        }
        override func mouseDragged(with event: NSEvent) { onSelect(.changed, convert(event.locationInWindow, from: nil)) }
        override func mouseUp(with event: NSEvent) { onSelect(.ended, convert(event.locationInWindow, from: nil)) }

        // The Edit menu's Copy and Paste (⌘C, ⌘V) reach the first responder.
        @objc func copy(_ sender: Any?) {
            if let text = selectedText(), !text.isEmpty { copyToPasteboard(text) }
        }
        @objc func paste(_ sender: Any?) {
            guard canPaste, let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
            core?.terminalPaste(text)
        }
        func validateUserInterfaceItem(_ item: NSValidatedUserInterfaceItem) -> Bool {
            switch item.action {
            case #selector(copy(_:)): return selectedText().map { !$0.isEmpty } ?? false
            case #selector(paste(_:)): return canPaste && NSPasteboard.general.string(forType: .string) != nil
            default: return false
            }
        }

        override func keyDown(with event: NSEvent) {
            guard let core else { return }
            if event.modifierFlags.contains(.control),
               let letter = event.charactersIgnoringModifiers?.first,
               let key = TerminalKey.control(letter)
            {
                core.terminalKey(key)
                return
            }
            if let key = special(event.keyCode) {
                core.terminalKey(key)
                return
            }
            if let text = event.characters, !text.isEmpty, !event.modifierFlags.contains(.command) {
                core.terminalText(text)
            }
        }

        private func special(_ keyCode: UInt16) -> TerminalKey? {
            switch keyCode {
            case 36, 76: return .enter
            case 48: return .tab
            case 51: return .backspace
            case 53: return .escape
            case 117: return .delete
            case 115: return .home
            case 119: return .end
            case 116: return .pageup
            case 121: return .pagedown
            case 123: return .left
            case 124: return .right
            case 125: return .down
            case 126: return .up
            default: return nil
            }
        }
    }
}
#elseif os(iOS)
struct TerminalInputView: UIViewRepresentable {
    let core: MusterCore
    var canPaste = false
    var selectedText: () -> String? = { nil }
    var onSelect: (TerminalSelectPhase, CGPoint) -> Void = { _, _ in }
    var onClear: () -> Void = {}
    /// A shorter in-app keyboard, below a half-fold crease.
    var compactKeyboard = false
    @ObservedObject var keyboardPrefs = KeyboardPrefs.shared

    func makeUIView(context: Context) -> KeyCatcher {
        let view = KeyCatcher()
        view.backgroundColor = .clear
        update(view)
        view.addGestureRecognizer(UITapGestureRecognizer(target: view, action: #selector(KeyCatcher.tapped)))
        let press = UILongPressGestureRecognizer(target: view, action: #selector(KeyCatcher.pressed(_:)))
        press.minimumPressDuration = 0.35
        view.addGestureRecognizer(press)
        view.addInteraction(view.menu)
        DispatchQueue.main.async { view.becomeFirstResponder() }
        return view
    }

    func updateUIView(_ view: KeyCatcher, context: Context) { update(view) }

    private func update(_ view: KeyCatcher) {
        view.core = core
        view.canPaste = canPaste
        view.selectedText = selectedText
        view.onSelect = onSelect
        view.onClear = onClear
        view.attachKeyboard(systemKeyboard: keyboardPrefs.terminalSystemKeyboard, compact: compactKeyboard)
    }

    final class KeyCatcher: UIView, UIKeyInput, UIEditMenuInteractionDelegate {
        var core: MusterCore?
        var canPaste = false
        /// The Muster keyboard, when Settings asks for it; the system keyboard otherwise.
        private var keyboard: KeyboardInputView?
        private var keyboardCompact = false
        private var customInputView: UIView?
        override var inputView: UIView? { customInputView }

        /// The in-app keyboard types straight into the session: text as
        /// typed, everything else as named keys the core maps to bytes.
        func attachKeyboard(systemKeyboard: Bool, compact: Bool) {
            if systemKeyboard {
                if customInputView != nil {
                    customInputView = nil
                    if isFirstResponder { reloadInputViews() }
                }
                return
            }
            if keyboard == nil || keyboardCompact != compact {
                keyboardCompact = compact
                keyboard = KeyboardInputView(target: KeyboardTarget(
                    insert: { [weak self] text in self?.core?.terminalText(text) },
                    backspace: { [weak self] in self?.core?.terminalKey(.backspace) },
                    enter: { [weak self] in self?.core?.terminalKey(.enter) },
                    hide: { [weak self] in self?.resignFirstResponder() },
                    key: { [weak self] name in self?.core?.terminalKey(TerminalKey(rawValue: name)) },
                    edit: nil
                ), enterLabel: "回车", compact: compact)
            }
            if customInputView !== keyboard {
                customInputView = keyboard
                if isFirstResponder { reloadInputViews() }
            }
        }
        var selectedText: () -> String? = { nil }
        var onSelect: (TerminalSelectPhase, CGPoint) -> Void = { _, _ in }
        var onClear: () -> Void = {}
        lazy var menu = UIEditMenuInteraction(delegate: self)
        /// The fitted grid's scroll view, held still while a selection is dragged.
        private var held: UIScrollView?
        override var canBecomeFirstResponder: Bool { true }
        var hasText: Bool { true }
        var keyboardType: UIKeyboardType = .asciiCapable
        var autocorrectionType: UITextAutocorrectionType = .no
        var autocapitalizationType: UITextAutocapitalizationType = .none
        var smartQuotesType: UITextSmartQuotesType = .no
        var smartDashesType: UITextSmartDashesType = .no
        var spellCheckingType: UITextSpellCheckingType = .no

        /// A tap ends a selection and brings the keyboard back.
        @objc func tapped() {
            menu.dismissMenu()
            onClear()
            becomeFirstResponder()
        }

        /// A long press starts a selection where it lands; dragging extends
        /// it; lifting the finger shows Copy (and Paste) there.
        @objc func pressed(_ gesture: UILongPressGestureRecognizer) {
            let point = gesture.location(in: self)
            switch gesture.state {
            case .began:
                becomeFirstResponder()
                menu.dismissMenu()
                held = sequence(first: superview, next: { $0?.superview }).compactMap { $0 as? UIScrollView }.first
                held?.panGestureRecognizer.isEnabled = false
                onSelect(.began, point)
            case .changed:
                onSelect(.changed, point)
            case .ended:
                onSelect(.ended, point)
                release()
                DispatchQueue.main.async { [weak self] in
                    self?.menu.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: point))
                }
            default:
                release()
            }
        }

        private func release() {
            held?.panGestureRecognizer.isEnabled = true
            held = nil
        }

        @objc override func copy(_ sender: Any?) {
            if let text = selectedText(), !text.isEmpty { UIPasteboard.general.string = text }
        }

        @objc override func paste(_ sender: Any?) {
            guard canPaste, let text = UIPasteboard.general.string, !text.isEmpty else { return }
            core?.terminalPaste(text)
        }

        // Only Copy and Paste, for the edit menu and a hardware ⌘C / ⌘V.
        override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
            switch action {
            case #selector(copy(_:)): return selectedText().map { !$0.isEmpty } ?? false
            case #selector(paste(_:)): return canPaste && UIPasteboard.general.hasStrings
            default: return false
            }
        }

        func insertText(_ text: String) {
            if text == "\n" { core?.terminalKey(.enter) } else { core?.terminalText(text) }
        }

        func deleteBackward() { core?.terminalKey(.backspace) }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            var handled = false
            for press in presses {
                guard let key = press.key else { continue }
                if key.modifierFlags.contains(.control), let letter = key.charactersIgnoringModifiers.first,
                   let control = TerminalKey.control(letter)
                {
                    core?.terminalKey(control); handled = true; continue
                }
                let special: TerminalKey? = switch key.keyCode {
                case .keyboardUpArrow: .up
                case .keyboardDownArrow: .down
                case .keyboardLeftArrow: .left
                case .keyboardRightArrow: .right
                case .keyboardEscape: .escape
                case .keyboardTab: .tab
                case .keyboardHome: .home
                case .keyboardEnd: .end
                case .keyboardPageUp: .pageup
                case .keyboardPageDown: .pagedown
                case .keyboardDeleteForward: .delete
                default: nil
                }
                if let special { core?.terminalKey(special); handled = true }
            }
            if !handled { super.pressesBegan(presses, with: event) }
        }
    }
}
#else
struct TerminalInputView: View {
    let core: MusterCore
    var canPaste = false
    var selectedText: () -> String? = { nil }
    var onSelect: (TerminalSelectPhase, CGPoint) -> Void = { _, _ in }
    var onClear: () -> Void = {}
    var compactKeyboard = false
    var body: some View { Color.clear }
}
#endif
#endif
