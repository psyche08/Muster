#if canImport(SwiftUI)
import SwiftUI
import MusterCore

/// The core supplies the queue, order, actions and outcomes. This view renders it.
struct QueuedMessages: View {
    let messages: [Muster_V1_PendingInteraction]
    let submitting: Bool
    let error: String
    let outcomeUnknown: Bool
    let sendNow: (String, String) -> Void
    @State private var contentHeight: CGFloat = 82
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("待发消息 · \(messages.count)", systemImage: "text.line.first.and.arrowtriangle.forward")
                .font(.caption.weight(.medium)).foregroundStyle(palette.secondary)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(messages, id: \.id) { message in
                        HStack(alignment: .center, spacing: 12) {
                            // Selectable Text already exposes its contents to AX. A label
                            // override can recurse through AppKit in optimized macOS builds.
                            Text(message.prompt)
                                .font(.subheadline).foregroundStyle(palette.text)
                                .lineLimit(3).frame(maxWidth: .infinity, alignment: .leading)
                                .textSelection(.enabled)
                            if !message.options.isEmpty {
                                Button("Send now") {
                                    if let option = message.options.first { sendNow(message.id, option) }
                                }
                                .font(.caption.weight(.semibold)).buttonStyle(.bordered)
                                .fixedSize(horizontal: true, vertical: false)
                                .disabled(submitting || outcomeUnknown)
                            }
                        }
                        .padding(.vertical, 8)
                        if message.id != messages.last?.id { Divider() }
                    }
                }
                .background(GeometryReader { geometry in
                    Color.clear.preference(key: QueuedContentHeight.self, value: geometry.size.height)
                })
            }
            .onPreferenceChange(QueuedContentHeight.self) { contentHeight = $0 }
            // ScrollView has no intrinsic height inside the fixed-size composer.
            // Reserve a visible viewport even for one queued message.
            .frame(height: min(max(contentHeight, 1), 180))
            if !error.isEmpty {
                Label(error, systemImage: outcomeUnknown ? "clock.badge.questionmark" : "exclamationmark.circle")
                    .font(.caption).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .background(palette.canvas, in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(palette.secondary.opacity(0.22)))
    }
}
private struct QueuedContentHeight: PreferenceKey {
    static let defaultValue: CGFloat = 82
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
#endif
