#if canImport(SwiftUI)
import MarkdownUI
import SwiftUI

// A local rendering preference, independent of the session's agent/provider.
// It does not change Rust-owned messages, authorization or send state.
enum ConversationStyle: String, CaseIterable, Identifiable {
    case codex, claude
    static let preferenceKey = "conversationStyle"
    var id: String { rawValue }
    var title: String { self == .codex ? "Codex" : "Claude" }
    var columnWidth: CGFloat { self == .codex ? 760 : 740 }
    var messageRadius: CGFloat { self == .codex ? 20 : 12 }
    var composerRadius: CGFloat { self == .codex ? 24 : 16 }
    var bodyScale: CGFloat { self == .codex ? 1 : 1.125 }
    var lineSpacing: CGFloat { self == .codex ? 0.28 : 0.38 }
    // Claude matches Claude.app's new-chat composer (claude.ai i18n `ye34lid+ov`).
    var placeholder: String { self == .codex ? "Do anything" : "How can I help you today?" }
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
        .frame(minWidth: 190, maxWidth: 300)
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
    var canvas: Color { style == .codex ? color(0xffffff, 0x212121) : color(0xfaf9f5, 0x141414) }
    var surface: Color { style == .codex ? color(0xf4f4f4, 0x303030) : color(0xf0eee6, 0x212121) }
    var userSurface: Color {
        #if os(iOS)
        style == .codex ? color(0xedf2ff, 0x253454) : surface
        #else
        surface
        #endif
    }
    var userText: Color {
        #if os(iOS)
        style == .codex ? color(0x3456c5, 0xb7caff) : text
        #else
        text
        #endif
    }
    var action: Color { color(0x3d63ed, 0x6285ff) }
    var composer: Color {
        #if os(iOS)
        style == .codex ? color(0xfafafa, 0x303030) : color(0xffffff, 0x212121)
        #else
        style == .codex ? surface : color(0xffffff, 0x212121)
        #endif
    }
    var text: Color { style == .codex ? color(0x171717, 0xececec) : color(0x30302e, 0xe8e6df) }
    var secondary: Color { style == .codex ? color(0x6b6b6b, 0xa3a3a3) : color(0x73726c, 0x9d9c95) }
    var border: Color { style == .codex ? color(0xe5e5e5, 0x424242) : color(0xd7d4cb, 0x50504e) }
    var code: Color { style == .codex ? color(0xf7f7f7, 0x171717) : color(0xf0eee6, 0x1b1b1a) }
    var link: Color { style == .codex ? color(0x2563eb, 0x8ab4f8) : color(0x956043, 0xdda382) }
    var accent: Color { style == .codex ? text : color(0xb65d3d, 0xd48a6a) }
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
                    .markdownMargin(top: 0, bottom: style == .claude ? 16 : 12)
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
