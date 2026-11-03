// The DeepSeek conversation style's process rows, after DeepSeek Harness's
// Chat view (`dsh-client-ui-chat` 0.2.1): quiet single-line rows with a 16pt
// glyph box, a 6pt gap and a chevron that follows the label, rows 6pt apart,
// and bodies indented under the label on the code-block surface. Presentation
// only: titles, statuses, grouping and expansion defaults arrive from Shared
// Rust exactly as the other styles receive them. See ADR 0105.
#if canImport(SwiftUI)
import SwiftUI

enum DeepSeekChat {
    static let rowGap: CGFloat = 6
    static let titleGap: CGFloat = 8
    static let glyphBox: CGFloat = 16
    static let glyphGap: CGFloat = 6
    static let bodyIndent: CGFloat = glyphBox + glyphGap
    static let rowMinHeight: CGFloat = 24
    static let bodyRadius: CGFloat = 12
    static let bodyMaxHeight: CGFloat = 260
    /// DSH keeps the user's bubble within 70.2% of its 748pt chat column.
    static let userMaxWidth: CGFloat = 525
    #if os(macOS)
    static let secondarySize: CGFloat = 13
    static let codeSize: CGFloat = 11
    #else
    static let secondarySize: CGFloat = 15
    static let codeSize: CGFloat = 13
    #endif
}

/// One DSH process row header: optional glyph, label, optional status and a
/// chevron that turns over when the row is open.
struct DeepSeekRowTitle: View {
    var systemImage: String?
    let title: String
    var status = ""
    let expanded: Bool
    var running = false
    @Environment(\.colorScheme) private var scheme
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: .deepseek) }

    var body: some View {
        HStack(spacing: DeepSeekChat.glyphGap) {
            if let systemImage {
                Image(systemName: systemImage)
                    .font(.system(size: DeepSeekChat.secondarySize))
                    .frame(width: DeepSeekChat.glyphBox, height: DeepSeekChat.glyphBox)
            }
            Text(title).lineLimit(1).truncationMode(.tail)
            if running { ProgressView().controlSize(.mini).accessibilityLabel("Running") }
            // A completed row carries no mark; failure stays explicit.
            if status != "completed" { ToolStatus(status: status) }
            Image(systemName: "chevron.down")
                .font(.system(size: DeepSeekChat.secondarySize - 3, weight: .semibold))
                .rotationEffect(.degrees(expanded ? 180 : 0))
                .padding(.leading, 2)
            Spacer(minLength: 0)
        }
        .foregroundStyle(palette.secondary)
        .frame(minHeight: DeepSeekChat.rowMinHeight)
        .contentShape(Rectangle())
    }
}

/// The surface DSH puts a row's recorded input and output on.
struct DeepSeekRowBody<Content: View>: View {
    @ViewBuilder let content: () -> Content
    @Environment(\.colorScheme) private var scheme
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: .deepseek) }

    var body: some View {
        content()
            .font(.system(size: DeepSeekChat.codeSize, design: .monospaced))
            .foregroundStyle(palette.secondary)
            .padding(EdgeInsets(top: 10, leading: 12, bottom: 12, trailing: 16))
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(palette.code, in: RoundedRectangle(cornerRadius: DeepSeekChat.bodyRadius))
            .padding(.leading, DeepSeekChat.bodyIndent)
            .padding(.top, 4)
    }
}

/// Recorded text at its own height, scrolling only past `maxHeight`, as DSH
/// caps a tool body. A bare ScrollView would take the whole cap: a hidden copy
/// sizes the frame and the scrolling copy fills it.
struct DeepSeekBoundedText: View {
    let text: String
    let maxHeight: CGFloat
    @Environment(\.colorScheme) private var scheme
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: .deepseek) }

    var body: some View {
        Text(text)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .hidden()
            .frame(maxHeight: maxHeight, alignment: .top)
            // Under a finite proposal a max-height frame grows to its cap.
            .fixedSize(horizontal: false, vertical: true)
            .overlay(alignment: .topLeading) {
                ScrollView(.vertical) {
                    Text(text).foregroundStyle(palette.text)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
    }
}
#endif
