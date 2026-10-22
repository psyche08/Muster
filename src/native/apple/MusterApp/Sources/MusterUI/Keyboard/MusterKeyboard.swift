// The in-app keyboard follows the MacBook's ANSI rows at every width,
// including iPhone. It can be a UIKit input view or a persistent lower pane.
// Fields supply a KeyboardTarget; this view never reads their contents.
#if canImport(SwiftUI) && os(iOS)
import MusterCore
import SwiftUI

private let doubleTap: TimeInterval = 0.35
private let fnArrows = ["left": "home", "right": "end", "up": "pageup", "down": "pagedown"]
/// Control with a letter in a text field: the macOS (Emacs) text bindings.
private let controlEdits: [String: KeyboardEdit] = [
    "a": .lineStart, "e": .lineEnd, "b": .left, "f": .right, "p": .up, "n": .down, "d": .deleteForward, "k": .deleteToLineEnd,
]

/// The modifiers that apply to one key press.
private struct Held {
    var shift = false, ctrl = false, alt = false, cmd = false, fn = false
}

struct MusterKeyboard: View {
    let target: KeyboardTarget
    /// What Enter does in this field, shown on the key.
    var enterLabel = "换行"
    /// Shorter keys, so the keyboard and its field fit below a half-fold crease.
    var compact = false
    var allowsHiding = true
    @ObservedObject private var prefs = KeyboardPrefs.shared
    /// Shift, Ctrl, Alt (option), Command and fn: 0 off, 1 for the next key, 2 locked.
    @State private var shift = 0
    @State private var ctrl = 0
    @State private var alt = 0
    @State private var cmd = 0
    @State private var fn = 0
    @State private var capsLock = false
    @State private var width: CGFloat = 0
    @State private var composing = ""
    @State private var candidates: [PinyinCandidate] = []
    @State private var ready = false
    @State private var lastTap: [String: Date] = [:]
    @State private var punctuation = ChinesePunctuation()

    init(target: KeyboardTarget, enterLabel: String = "换行", compact: Bool = false, allowsHiding: Bool = true) {
        self.target = target
        self.enterLabel = enterLabel
        self.compact = compact
        self.allowsHiding = allowsHiding
    }

    /// Wide enough for full corner legends instead of compact symbols.
    private var wide: Bool { width >= KeyboardMetrics.macMinWidth }
    private var chinese: Bool { prefs.chinese && ready }
    private var keyHeight: CGFloat { compact ? KeyboardMetrics.compactKeyHeight : KeyboardMetrics.keyHeight }
    private var gap: CGFloat { wide ? (compact ? 4 : 5) : 2 }
    private var keyFill: Color { Color(uiColor: .systemBackground) }
    private var actionFill: Color { Color(uiColor: .systemGray4) }
    private var pressedFill: Color { Color(uiColor: .systemGray3) }

    var body: some View {
        VStack(spacing: compact ? 4 : 6) {
            topBar
            macLayout
        }
        .padding(.horizontal, 5).padding(.top, 4).padding(.bottom, compact ? 4 : 8)
        .background(Color(uiColor: .secondarySystemBackground))
        .background(GeometryReader { geo in
            Color.clear.onAppear { width = geo.size.width }.onChange(of: geo.size.width) { _, w in width = w }
        })
        .onAppear(perform: appear)
        .onDisappear(perform: disappear)
    }

    private func appear() {
        if let composer = Pinyin.composer { composer.clear(); ready = true; return }
        Pinyin.load { composer in composer.clear(); ready = true }
    }

    private func disappear() {
        // Leaving a field keeps what was typed: pending pinyin goes in as letters.
        if let composer = Pinyin.composer, composer.active { target.insert(composer.commitRaw()) }
    }

    // MARK: - Keys

    private func refresh() {
        let composer = Pinyin.composer
        composing = composer.map { $0.active ? $0.display() : "" } ?? ""
        candidates = composer.map { $0.composing.isEmpty ? [] : $0.candidates() } ?? []
    }

    /// Commits pending pinyin with its best reading before a space, digit or symbol.
    private func flush() {
        if let composer = Pinyin.composer, composer.active {
            target.insert(composer.commitFirst())
            refresh()
        }
    }

    /// With Ctrl or Alt held, a key goes to the terminal as a named combination.
    private func modified(_ base: String) -> Bool {
        guard let send = target.key, ctrl > 0 || alt > 0 else { return false }
        flush()
        send("\(ctrl > 0 ? "ctrl-" : "")\(alt > 0 ? "alt-" : "")\(base)")
        if ctrl == 1 { ctrl = 0 }
        if alt == 1 { alt = 0 }
        return true
    }

    private func pick(_ candidate: PinyinCandidate) {
        guard let composer = Pinyin.composer else { return }
        if let done = composer.pick(candidate) { target.insert(done) }
        refresh()
    }

    private func backspace() {
        if modified("backspace") { return }
        if let composer = Pinyin.composer, composer.active {
            composer.backspace()
            refresh()
            return
        }
        target.backspace()
    }

    private func space() {
        if modified("space") { return }
        if let composer = Pinyin.composer, composer.active { flush(); return }
        target.insert(" ")
    }

    private func enter() {
        if modified("enter") { return }
        // Enter while composing types the letters themselves, as other pinyin keyboards do.
        if let composer = Pinyin.composer, composer.active {
            target.insert(composer.commitRaw())
            refresh()
            return
        }
        target.enter()
    }

    /// Tap: the next key only; double tap: locked; tap again: off.
    private func cycle(_ state: Int, _ id: String) -> Int {
        let last = lastTap[id] ?? .distantPast
        lastTap[id] = Date()
        if state == 1 && Date().timeIntervalSince(last) < doubleTap { return 2 }
        return state == 0 ? 1 : 0
    }

    private func toggleShift() { shift = cycle(shift, "shift") }
    private func toggleCtrl() { ctrl = cycle(ctrl, "ctrl") }
    private func toggleAlt() { alt = cycle(alt, "alt") }
    private func toggleCmd() { cmd = cycle(cmd, "cmd") }
    private func toggleFn() { fn = cycle(fn, "fn") }

    /// The modifiers for this key press; one-shot ones are released.
    private func take() -> Held {
        let held = Held(shift: shift > 0, ctrl: ctrl > 0, alt: alt > 0, cmd: cmd > 0, fn: fn > 0)
        if shift == 1 { shift = 0 }
        if ctrl == 1 { ctrl = 0 }
        if alt == 1 { alt = 0 }
        if cmd == 1 { cmd = 0 }
        if fn == 1 { fn = 0 }
        return held
    }

    private func macPress(_ key: MacKey) {
        switch key.kind {
        case .letter: macCharacter(key.id, key.id.uppercased(), letter: true)
        case .symbol: macCharacter(key.bottom, key.top, letter: false)
        default: macAction(key.id)
        }
    }

    private func macCharacter(_ plain: String, _ shifted: String, letter: Bool) {
        let held = take()
        let upper = held.shift || (letter && capsLock)
        let char = upper ? shifted : plain
        if held.cmd {
            if letter { command(plain) }
            return
        }
        if let send = target.key, held.ctrl || held.alt {
            flush()
            send("\(held.ctrl ? "ctrl-" : "")\(held.alt ? "alt-" : "")\(char)")
            return
        }
        if target.key == nil && held.ctrl {
            if letter { controlEdit(plain) }
            return
        }
        if let composer = Pinyin.composer, chinese && !upper {
            // Letters compose pinyin; an apostrophe between them separates syllables.
            if letter || (plain == "'" && !composer.composing.isEmpty) {
                composer.type(plain)
                refresh()
                return
            }
        }
        flush()
        target.insert(!letter && chinese ? punctuation.map(char) : char)
    }

    /// Command with a letter in a text field: select all, copy, cut. Terminals have no such shortcuts.
    private func command(_ letter: String) {
        guard let edit = target.edit else { return }
        flush()
        switch letter {
        case "a": edit(.selectAll, false)
        case "c": edit(.copy, false)
        case "x": edit(.cut, false)
        default: break
        }
    }

    private func controlEdit(_ letter: String) {
        flush()
        if letter == "h" { target.backspace() } else if let edit = target.edit, let action = controlEdits[letter] { edit(action, false) }
    }

    private func macAction(_ id: String) {
        switch id {
        case "shift": toggleShift(); return
        case "control": toggleCtrl(); return
        case "option": toggleAlt(); return
        case "command": toggleCmd(); return
        case "fn": toggleFn(); return
        // A tap switches language, as Caps Lock does with macOS Pinyin; holding it locks capitals.
        case "caps": toggleLanguage(); return
        default: break
        }
        let held = take()
        if id == "escape", let composer = Pinyin.composer, composer.active {
            composer.clear()
            refresh()
            return
        }
        if let send = target.key { terminalAction(id, held, send) } else { fieldAction(id, held) }
    }

    private func terminalAction(_ id: String, _ held: Held, _ send: (String) -> Void) {
        let mods = "\(held.ctrl ? "ctrl-" : "")\(held.alt ? "alt-" : "")"
        let shift = held.shift ? "shift-" : ""
        if id == "delete" && !held.fn && !held.cmd && mods.isEmpty { backspace(); return }
        if id == "return" && mods.isEmpty { enter(); return }
        if id == "space" && mods.isEmpty { space(); return }
        flush()
        if id == "delete" { send(held.fn ? "\(mods)delete" : held.cmd ? "ctrl-u" : "\(mods)backspace") }
        else if id == "return" { send("\(mods)enter") }
        else if id == "space" { send("\(mods)space") }
        else if fnArrows[id] != nil { send(terminalArrow(id, held, mods)) }
        else { send("\(mods)\(shift)\(id)") }
    }

    /// Arrows as macOS terminals send them: fn for Home/End/Page Up/Down, ⌘ for the line, ⌥ for words.
    private func terminalArrow(_ id: String, _ held: Held, _ mods: String) -> String {
        let shift = held.shift ? "shift-" : ""
        let horizontal = id == "left" || id == "right"
        if held.fn { return "\(mods)\(shift)\(fnArrows[id] ?? id)" }
        if held.cmd && horizontal { return id == "left" ? "ctrl-a" : "ctrl-e" }
        if held.alt && !held.ctrl && !held.shift && horizontal { return id == "left" ? "alt-b" : "alt-f" }
        return "\(mods)\(shift)\(id)"
    }

    private func fieldAction(_ id: String, _ held: Held) {
        let composing = Pinyin.composer?.active ?? false
        if id == "delete" {
            if composing || target.edit == nil { backspace() }
            else if held.fn { target.edit?(.deleteForward, false) }
            else if held.cmd { target.edit?(.deleteToLineStart, false) }
            else if held.alt { target.edit?(.deleteWordBack, false) }
            else { backspace() }
            return
        }
        if id == "return" { enter(); return }
        if id == "space" { space(); return }
        if id == "escape" { if allowsHiding { target.hide() }; return }
        guard let edit = target.edit else { return }
        flush()
        if id == "tab" { edit(.tab, false); return }
        if let move = fieldArrow(id, held) { edit(move, held.shift) }
    }

    private func fieldArrow(_ id: String, _ held: Held) -> KeyboardEdit? {
        func pick(_ left: KeyboardEdit, _ right: KeyboardEdit, _ up: KeyboardEdit, _ down: KeyboardEdit) -> KeyboardEdit? {
            switch id {
            case "left": left
            case "right": right
            case "up": up
            case "down": down
            default: nil
            }
        }
        if held.fn || held.cmd { return pick(.lineStart, .lineEnd, .start, .end) }
        if held.alt { return pick(.wordLeft, .wordRight, .lineStart, .lineEnd) }
        return pick(.left, .right, .up, .down)
    }

    /// A modifier's state: 0 off, 1 for the next key, 2 locked.
    private func level(_ id: String) -> Int {
        switch id {
        case "shift": shift
        case "control": ctrl
        case "option": alt
        case "command": cmd
        case "fn": fn
        default: 0
        }
    }

    private func lit(_ key: MacKey) -> Bool { key.id == "return" || level(key.id) > 0 }

    /// Return shows what Enter does when it is more than a new line (send, done).
    private var returnHint: String { enterLabel == "换行" || enterLabel == "回车" ? "" : enterLabel }

    private func toggleLanguage() {
        if let composer = Pinyin.composer, composer.active {
            target.insert(composer.commitRaw())
            refresh()
        }
        prefs.chinese.toggle()
    }

    // MARK: - MacBook layout

    // Rows of MacKeys whose widths are shares of the row, so the columns line
    // up as on the real keyboard. Each key's slot carries the gap as padding.
    @ViewBuilder private var macLayout: some View {
        macRow(macFunctionRow, short: true)
        ForEach(0..<macRows.count, id: \.self) { macRow(macRows[$0], short: false) }
    }

    private func macRow(_ keys: [MacKey], short: Bool) -> some View {
        GeometryReader { geo in
            let unit = geo.size.width / macRowUnits
            HStack(spacing: 0) {
                ForEach(Array(keys.enumerated()), id: \.offset) { _, key in
                    macKey(key).frame(width: key.weight * unit)
                }
            }
        }
        .frame(height: short ? (keyHeight * 0.7).rounded() : keyHeight)
    }

    @ViewBuilder private func macKey(_ key: MacKey) -> some View {
        if key.kind == .arrows {
            arrowCluster
        } else {
            Group {
                switch key.kind {
                case .letter: letterCap(key)
                case .symbol: symbolCap(key)
                case .named: namedCap(key)
                default: actionCap(key)
                }
            }
            .padding(.horizontal, gap / 2)
        }
    }

    /// Letters are capitals on the cap, as on a MacBook; they type in lower case without Shift or Caps Lock.
    private func letterCap(_ key: MacKey) -> some View {
        Button(key.id.uppercased()) { macPress(key) }
            .buttonStyle(KeyCapStyle(fill: keyFill, pressed: pressedFill, radius: 6, shadow: true))
            .font(.system(size: compact ? 16 : 18)).foregroundStyle(.primary)
    }

    /// The shifted character above the plain one; Shift brings the upper one forward.
    private func symbolCap(_ key: MacKey) -> some View {
        Button { macPress(key) } label: {
            VStack(spacing: 0) {
                Text(key.top).font(.system(size: compact ? 11 : 12)).foregroundStyle(shift > 0 ? .primary : .tertiary)
                Text(key.bottom).font(.system(size: compact ? 14 : 16)).foregroundStyle(shift > 0 ? .tertiary : .primary)
            }
        }
        .buttonStyle(KeyCapStyle(fill: keyFill, pressed: pressedFill, radius: 6, shadow: true))
    }

    private func namedCap(_ key: MacKey) -> some View {
        Button(key.bottom) { macAction(key.id) }
            .buttonStyle(KeyCapStyle(fill: actionFill, pressed: pressedFill, radius: 5, shadow: false))
            .font(.system(size: 11)).foregroundStyle(.primary)
            .disabled(target.key == nil)
    }

    /// Modifiers and editing keys: small legends in the corners, toward the space bar for ⌃ ⌥ ⌘.
    private func actionCap(_ key: MacKey) -> some View {
        let lit = lit(key)
        let legend: Color = lit ? .white : .primary
        let fill = lit ? Color.accentColor : (key.id == "space" ? keyFill : actionFill)
        let align: HorizontalAlignment = key.end ? .trailing : .leading
        let cap = Group {
          if !wide {
            Text(narrowLegend(key)).font(.system(size: key.id == "fn" ? 10 : 16))
                .foregroundStyle(legend).minimumScaleFactor(0.65).lineLimit(1)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
          } else {
          VStack(alignment: align, spacing: 0) {
            HStack {
                if key.id == "caps" {
                    Circle().fill(capsLock ? Color.green : Color.gray.opacity(0.4)).frame(width: 5, height: 5)
                    Spacer()
                    Text(chinese ? "中" : "英").font(.system(size: 10)).foregroundStyle(.secondary)
                } else if key.id == "return" {
                    Text(returnHint).font(.system(size: 10)).foregroundStyle(.white)
                } else if !key.top.isEmpty {
                    if key.end || key.id == "fn" { Spacer() }
                    Text(key.top).font(.system(size: key.id == "fn" || compact ? 10 : 12)).foregroundStyle(legend)
                    if !(key.end || key.id == "fn") { Spacer() }
                }
            }
            Spacer(minLength: 0)
            if key.id == "fn" {
                Image(systemName: "globe").font(.system(size: 11)).foregroundStyle(legend)
            } else if !key.bottom.isEmpty {
                Text(key.bottom).font(.system(size: compact ? 10 : 11)).lineLimit(1).foregroundStyle(legend)
            }
        }
          }
        }
        .padding(.horizontal, wide ? 6 : 1).padding(.vertical, compact ? 2 : 4)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: key.end ? .bottomTrailing : .bottomLeading)
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(.primary, lineWidth: level(key.id) == 2 ? 2 : 0))
        .accessibilityLabel(key.id == "caps" ? (capsLock ? "Caps lock on" : "Caps lock") : (level(key.id) == 2 ? "\(key.id) locked" : key.id))
        return Group {
            if key.id == "delete" {
                RepeatKey(action: { macAction(key.id) }) { cap }.background(fill, in: RoundedRectangle(cornerRadius: 6))
            } else if key.id == "caps" {
                cap.background(fill, in: RoundedRectangle(cornerRadius: 6)).contentShape(Rectangle())
                    .onTapGesture { macAction(key.id) }
                    .onLongPressGesture(minimumDuration: 0.4) { capsLock.toggle() }
            } else {
                Button { macPress(key) } label: { cap }
                    .buttonStyle(KeyCapStyle(fill: fill, pressed: pressedFill, radius: 6, shadow: false))
            }
        }
    }

    private func narrowLegend(_ key: MacKey) -> String {
        switch key.id {
        case "caps": return chinese ? "中" : "⇪"
        case "tab": return "⇥"
        case "delete": return "⌫"
        case "return": return "↵"
        case "shift": return "⇧"
        case "control": return "⌃"
        case "option": return "⌥"
        case "command": return "⌘"
        case "fn": return "fn"
        case "space": return chinese ? "拼音" : "space"
        default: return key.bottom
        }
    }

    /// Inverted T: full-height ← and →, half-height ↑ over ↓, as on current MacBooks.
    private var arrowCluster: some View {
        HStack(spacing: 0) {
            arrowKey("left", "arrowtriangle.left.fill", full: true)
            VStack(spacing: 2) {
                arrowKey("up", "arrowtriangle.up.fill", full: false)
                arrowKey("down", "arrowtriangle.down.fill", full: false)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            arrowKey("right", "arrowtriangle.right.fill", full: true)
        }
    }

    private func arrowKey(_ id: String, _ symbol: String, full: Bool) -> some View {
        RepeatKey(action: { macAction(id) }) {
            Image(systemName: symbol).font(.system(size: full ? 11 : 9)).foregroundStyle(.primary)
        }
        .background(actionFill, in: RoundedRectangle(cornerRadius: 5))
        .padding(.horizontal, gap / 2)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityLabel(id)
    }

    // MARK: - Top bar

    private var topBar: some View {
        HStack(spacing: 4) {
            if !composing.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    Text(composing).font(.system(size: 13)).foregroundStyle(Color.accentColor).lineLimit(1).padding(.leading, 8)
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 2) {
                            ForEach(Array(candidates.enumerated()), id: \.offset) { index, candidate in
                                Button(candidate.word) { pick(candidate) }
                                    .buttonStyle(KeyCapStyle(fill: .clear, pressed: pressedFill, radius: 8, shadow: false))
                                    .font(.system(size: 20, weight: index == 0 ? .medium : .regular))
                                    .foregroundStyle(index == 0 ? Color.accentColor : .primary)
                                    .padding(.horizontal, 10).padding(.vertical, 2)
                            }
                        }
                    }
                }
            } else {
                Text(chinese ? "拼音" : (prefs.chinese ? "词库加载中…" : "English"))
                    .font(.system(size: 13)).foregroundStyle(.secondary).padding(.leading, 8)
                if wide {
                    Text("caps lock：轻点切换中/英，按住锁定大写").font(.system(size: 11)).foregroundStyle(.tertiary)
                        .lineLimit(1).padding(.leading, 8)
                }
                Spacer()
                if allowsHiding {
                Button { target.hide() } label: {
                    Image(systemName: "keyboard.chevron.compact.down").font(.system(size: 20)).foregroundStyle(.secondary)
                        .frame(width: 40, height: 34)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Hide keyboard")
                }
            }
        }
        .frame(height: compact ? 36 : 46)
    }
}

/// A key cap: its fill, pressed darker, with a MacBook-like shadow on typing keys.
private struct KeyCapStyle: ButtonStyle {
    let fill: Color
    let pressed: Color
    let radius: CGFloat
    let shadow: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(configuration.isPressed ? pressed : fill, in: RoundedRectangle(cornerRadius: radius))
            .shadow(color: shadow ? .black.opacity(0.2) : .clear, radius: 0.5, y: 1)
    }
}

/// A key that fires once on a tap and repeats while held (delete, arrows).
private struct RepeatKey<Label: View>: View {
    let action: () -> Void
    @ViewBuilder let label: () -> Label
    @State private var pressed = false
    @State private var repeated = false
    @State private var timer: Timer?

    var body: some View {
        label()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .opacity(pressed ? 0.7 : 1)
            .gesture(DragGesture(minimumDistance: 0)
                .onChanged { _ in
                    guard !pressed else { return }
                    pressed = true
                    repeated = false
                    timer = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: false) { _ in
                        Task { @MainActor in
                            repeated = true
                            action()
                            timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in Task { @MainActor in action() } }
                        }
                    }
                }
                .onEnded { _ in
                    timer?.invalidate()
                    timer = nil
                    if !repeated { action() }
                    pressed = false
                    repeated = false
                })
    }
}
#endif
