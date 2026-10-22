#if canImport(SwiftUI)
import SwiftUI
import MusterCore

// Orca-inspired compact disclosures. Classification, pairing, ranges and
// lifecycle labels all arrive from Shared Rust, never from parsing UI text.
struct ReasoningRow: View {
    let block: Muster_App_ChatBlock
    @State var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            ChatMarkdown(text: block.message.text)
                .padding(.leading, 12).padding(.top, 8)
                .overlay(alignment: .leading) {
                    Rectangle().fill(Color.secondary.opacity(0.2)).frame(width: 2)
                }
        } label: {
            Label(block.title, systemImage: "brain")
                .font(.subheadline).foregroundStyle(.secondary)
        }
        .tint(.secondary)
        .padding(.vertical, 4)
    }
}

struct SessionNotice: View {
    let block: Muster_App_ChatBlock
    @State private var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            Text(block.message.text).font(.caption.monospaced())
                .textSelection(.enabled).padding(.top, 6)
        } label: {
            Text(block.title).font(.caption).lineLimit(2)
        }
        .foregroundStyle(.secondary).tint(.secondary)
    }
}

struct ToolStatus: View {
    let status: String

    var body: some View {
        switch status {
        case "running":
            ProgressView().controlSize(.mini).accessibilityLabel("Running")
        case "completed":
            Image(systemName: "checkmark").accessibilityLabel("Completed")
        case "failed":
            Label("Failed", systemImage: "exclamationmark.circle").foregroundStyle(.red)
        case "interrupted":
            Label("Interrupted", systemImage: "stop.circle").foregroundStyle(.secondary)
        default: EmptyView()
        }
    }
}

struct SessionDiffCard: View {
    let file: Muster_App_ChatDiffFile
    @State var expanded = false
    @State private var copied = false
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 8) {
                Button { expanded.toggle() } label: {
                    HStack(alignment: .top, spacing: 8) {
                        Image(systemName: "doc.text")
                        Text(file.path).font(.subheadline.monospaced()).lineLimit(2)
                            .truncationMode(.middle)
                        Spacer(minLength: 0)
                        Text("+\(file.added)").foregroundStyle(.green)
                        Text("−\(file.removed)").foregroundStyle(.red)
                        Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    }.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityValue(expanded ? "Expanded" : "Collapsed")
                Button {
                    copyToPasteboard(file.patch)
                    copied = true
                } label: {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc")
                        .frame(minWidth: 28, minHeight: 28)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(copied ? "Diff copied" : "Copy diff")
                .onChange(of: file.patch) { _, _ in copied = false }
            }
            .font(.caption).padding(10)
            if expanded {
                Divider()
                ScrollView(.vertical) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(file.lines.enumerated()), id: \.offset) { _, line in
                            diffLine(line)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                }
                .frame(maxHeight: 320)
                if file.truncated {
                    Text("Preview shortened · Copy includes the full supplied diff")
                        .font(.caption).foregroundStyle(.secondary).padding(8)
                }
            }
        }
        .background(palette.code, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(palette.border, lineWidth: 0.5))
        .fixedSize(horizontal: false, vertical: true)
    }

    private func diffLine(_ line: Muster_App_ChatDiffLine) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(line.oldLine == 0 ? "" : "\(line.oldLine)")
                .frame(width: 40, alignment: .trailing).foregroundStyle(.secondary)
            Text(line.newLine == 0 ? "" : "\(line.newLine)")
                .frame(width: 40, alignment: .trailing).foregroundStyle(.secondary)
            Text(line.kind == "add" ? "+" : line.kind == "delete" ? "−" : " ")
                .frame(width: 10)
            Text(line.text).fixedSize(horizontal: false, vertical: true)
                .foregroundStyle(line.kind == "meta" ? palette.secondary : palette.text)
            Spacer(minLength: 0)
        }
        .font(.system(size: ChatStyle.secondarySize, design: .monospaced))
        .padding(.vertical, 2).padding(.trailing, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Rectangle().fill(line.kind == "add" ? Color.green.opacity(0.12)
                    : line.kind == "delete" ? Color.red.opacity(0.12) : Color.clear))
    }
}
#endif
