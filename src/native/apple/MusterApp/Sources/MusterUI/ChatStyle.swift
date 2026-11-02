#if canImport(SwiftUI)
import MarkdownUI
import SwiftUI

// A local rendering preference, independent of the session's agent/provider.
// It does not change Rust-owned messages, authorization or send state.
enum ConversationStyle: String, CaseIterable, Identifiable {
    case codex, claude, deepseek
    static let preferenceKey = "conversationStyle"
    var id: String { rawValue }
    var title: String {
        switch self {
        case .codex: return "Codex"
        case .claude: return "Claude"
        case .deepseek: return "DeepSeek"
        }
    }
    var columnWidth: CGFloat {
        switch self {
        case .codex: return 760
        case .claude: return 740
        case .deepseek: return 860
        }
    }
    var messageRadius: CGFloat {
        switch self {
        case .codex: return 20
        case .claude: return 12
        case .deepseek: return 20
        }
    }
    var composerRadius: CGFloat {
        switch self {
        case .codex: return 24
        case .claude: return 16
        case .deepseek: return 28
        }
    }
    var bodyScale: CGFloat {
        switch self {
        case .codex: return 1
        case .claude: return 1.125
        case .deepseek: return 1
        }
    }
    var lineSpacing: CGFloat {
        switch self {
        case .codex: return 0.28
        case .claude: return 0.38
        case .deepseek: return 0.30
        }
    }
    // Claude matches Claude.app's new-chat composer (claude.ai i18n `ye34lid+ov`).
    // DeepSeek matches DeepSeek Harness's composer hint.
    var placeholder: String {
        switch self {
        case .codex: return "Do anything"
        case .claude: return "How can I help you today?"
        case .deepseek: return "Message or run a task"
        }
    }
}

private struct ConversationStyleKey: EnvironmentKey {
    static let defaultValue = ConversationStyle.codex
}
extension EnvironmentValues {
    var conversationStyle: ConversationStyle {
        get { self[ConversationStyleKey.self] }
        set { self[ConversationStyleKey.self] = newValue }
    }
}

struct ConversationStyleButton: View {
    @Binding var expanded: Bool

    var body: some View {
        Button { expanded.toggle() } label: {
            Label("Style", systemImage: "paintpalette")
        }
        .accessibilityIdentifier("conversation-style-toggle")
        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
        .help(expanded ? "Hide conversation styles" : "Show conversation styles")
    }
}

struct ConversationStylePicker: View {
    @AppStorage(ConversationStyle.preferenceKey) private var style = ConversationStyle.codex
    var body: some View {
        Picker("Conversation style", selection: $style) {
            ForEach(ConversationStyle.allCases) { Text($0.title).tag($0) }
        }
        .pickerStyle(.segmented)
        .frame(minWidth: 240, maxWidth: 360)
        .accessibilityIdentifier("conversation-style")
    }
}

// Presentation only. Provider messages and timeline grouping remain Rust-owned.
enum ChatStyle {
    #if os(macOS)
    static let bodySize: CGFloat = 14
    static let secondarySize: CGFloat = 12
    static let inset: CGFloat = 24
    #else
    static let bodySize: CGFloat = 17
    static let secondarySize: CGFloat = 15
    static let inset: CGFloat = 16
    #endif
}

struct ChatPalette {
    let scheme: ColorScheme
    var style: ConversationStyle = .codex
    private func color(_ light: UInt32, _ dark: UInt32) -> Color {
        let value = scheme == .dark ? dark : light
        return Color(.sRGB, red: Double((value >> 16) & 255) / 255,
                     green: Double((value >> 8) & 255) / 255,
                     blue: Double(value & 255) / 255, opacity: 1)
    }
    var canvas: Color {
        switch style {
        case .codex: return color(0xffffff, 0x212121)
        case .claude: return color(0xfaf9f5, 0x141414)
        case .deepseek: return color(0xffffff, 0x151517)
        }
    }
    var surface: Color {
        switch style {
        case .codex: return color(0xf4f4f4, 0x303030)
        case .claude: return color(0xf0eee6, 0x212121)
        case .deepseek: return color(0xf5f6f7, 0x2c2c2e)
        }
    }
    var userSurface: Color {
        switch style {
        case .codex:
            #if os(iOS)
            return color(0xedf2ff, 0x253454)
            #else
            return surface
            #endif
        case .claude:
            return surface
        case .deepseek:
            return color(0xedf3fe, 0x2c2c2e)
        }
    }
    var userText: Color {
        switch style {
        case .codex:
            #if os(iOS)
            return color(0x3456c5, 0xb7caff)
            #else
            return text
            #endif
        case .claude, .deepseek:
            return text
        }
    }
    var action: Color {
        switch style {
        case .codex, .claude: return color(0x3d63ed, 0x6285ff)
        case .deepseek: return color(0x4176e6, 0x7aaaff)
        }
    }
    var composer: Color {
        switch style {
        case .codex:
            #if os(iOS)
            return color(0xfafafa, 0x303030)
            #else
            return surface
            #endif
        case .claude:
            return color(0xffffff, 0x212121)
        case .deepseek:
            return color(0xffffff, 0x2c2c2e)
        }
    }
    var text: Color {
        switch style {
        case .codex: return color(0x171717, 0xececec)
        case .claude: return color(0x30302e, 0xe8e6df)
        case .deepseek: return color(0x0f1115, 0xf9fafb)
        }
    }
    var secondary: Color {
        switch style {
        case .codex: return color(0x6b6b6b, 0xa3a3a3)
        case .claude: return color(0x73726c, 0x9d9c95)
        case .deepseek: return color(0x81858c, 0xadb2b8)
        }
    }
    var border: Color {
        switch style {
        case .codex: return color(0xe5e5e5, 0x424242)
        case .claude: return color(0xd7d4cb, 0x50504e)
        case .deepseek: return color(0xe6e6e6, 0x313133)
        }
    }
    var code: Color {
        switch style {
        case .codex: return color(0xf7f7f7, 0x171717)
        case .claude: return color(0xf0eee6, 0x1b1b1a)
        case .deepseek: return color(0xf9fafb, 0x1b1b1c)
        }
    }
    var link: Color {
        switch style {
        case .codex: return color(0x2563eb, 0x8ab4f8)
        case .claude: return color(0x956043, 0xdda382)
        case .deepseek: return color(0x4176e6, 0x7aaaff)
        }
    }
    var accent: Color {
        switch style {
        case .codex: return text
        case .claude: return color(0xb65d3d, 0xd48a6a)
        case .deepseek: return color(0x4176e6, 0x7aaaff)
        }
    }
}

struct ChatMarkdown: View {
    let text: String
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    @ScaledMetric(relativeTo: .body) private var bodySize = ChatStyle.bodySize
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }

    var body: some View {
        Markdown(text)
            .markdownTheme(theme)
            .markdownImageProvider(ChatImageProvider())
            .markdownInlineImageProvider(ChatInlineImageProvider())
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
    }

    private var theme: Theme {
        Theme.basic
            .text {
                FontFamily(style == .claude ? .custom("Georgia") : .system(.default))
                FontSize(bodySize * style.bodyScale)
                FontWeight(.regular)
                ForegroundColor(palette.text)
                BackgroundColor(nil)
            }
            .strong { FontWeight(.semibold) }
            .link { ForegroundColor(palette.link) }
            .code {
                FontFamilyVariant(.monospaced)
                FontSize(.em(0.93))
                BackgroundColor(palette.surface)
            }
            .paragraph { configuration in
                configuration.label
                    .fixedSize(horizontal: false, vertical: true)
                    .relativeLineSpacing(.em(style.lineSpacing))
                    .markdownMargin(top: 0, bottom: style == .claude ? 16 : (style == .deepseek ? 14 : 12))
            }
            .heading1 { configuration in
                configuration.label
                    .markdownTextStyle { FontSize(.em(1.5)); FontWeight(.semibold) }
                    .markdownMargin(top: 20, bottom: 10)
            }
            .heading2 { configuration in
                configuration.label
                    .markdownTextStyle { FontSize(.em(1.25)); FontWeight(.semibold) }
                    .markdownMargin(top: 18, bottom: 8)
            }
            .heading3 { configuration in
                configuration.label
                    .markdownTextStyle { FontSize(.em(1.1)); FontWeight(.semibold) }
                    .markdownMargin(top: 16, bottom: 8)
            }
            .codeBlock { configuration in
                ChatCodeBlock(configuration: configuration, palette: palette)
                    .markdownMargin(top: 4, bottom: 12)
            }
            .blockquote { configuration in
                HStack(spacing: 12) {
                    RoundedRectangle(cornerRadius: 2).fill(palette.border).frame(width: 3)
                    configuration.label.markdownTextStyle { ForegroundColor(palette.secondary) }
                }
                .fixedSize(horizontal: false, vertical: true)
                .markdownMargin(top: 4, bottom: 12)
            }
            .table { configuration in
                configuration.label
                    .markdownTableBorderStyle(.init(color: palette.border))
                    .markdownTableBackgroundStyle(.alternatingRows(.clear, palette.surface))
                    .markdownMargin(top: 4, bottom: 12)
            }
    }
}

// Keep the action outside the horizontal scroll view so it stays at the
// visible top-right edge. Copy MarkdownUI's complete source, never the label.
private struct ChatCodeBlock: View {
    let configuration: CodeBlockConfiguration
    let palette: ChatPalette
    @State private var copyFeedback: UUID?

    private var buttonSize: CGFloat {
        #if os(iOS)
        44
        #else
        28
        #endif
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Spacer(minLength: 0)
                Button {
                    copyToPasteboard(configuration.content)
                    copyFeedback = UUID()
                } label: {
                    Label(copyFeedback == nil ? "Copy" : "Copied",
                          systemImage: copyFeedback == nil ? "doc.on.doc" : "checkmark")
                        .labelStyle(.iconOnly)
                        .font(.system(size: 14))
                        .frame(width: buttonSize, height: buttonSize)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(palette.secondary)
                .help(copyFeedback == nil ? "Copy code" : "Copied")
                .accessibilityIdentifier("code-block-copy")
            }
            .padding(.top, 4)
            .padding(.trailing, 4)
            ScrollView(.horizontal) {
                configuration.label
                    .markdownTextStyle {
                        FontFamilyVariant(.monospaced)
                        FontSize(.em(0.93))
                        BackgroundColor(nil)
                    }
                    .relativeLineSpacing(.em(0.25))
                    .padding(.horizontal, 14)
                    .padding(.bottom, 14)
            }
        }
        .background(palette.code, in: RoundedRectangle(cornerRadius: 12))
        .task(id: copyFeedback) {
            guard copyFeedback != nil else { return }
            do { try await Task.sleep(for: .seconds(2)) }
            catch { return }
            copyFeedback = nil
        }
        .onChange(of: configuration.content) { copyFeedback = nil }
    }
}

#endif
