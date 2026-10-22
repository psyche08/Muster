#if canImport(SwiftUI)
import MusterCore
import SwiftUI

private struct ExecuteCodeKey: EnvironmentKey { static let defaultValue: (String) -> Void = { _ in } }
extension EnvironmentValues {
    var executeCode: (String) -> Void {
        get { self[ExecuteCodeKey.self] }
        set { self[ExecuteCodeKey.self] = newValue }
    }
}

struct ConversationText: View {
    let text: String
    let parts: [Muster_App_MessagePart]
    var body: some View {
        if parts.isEmpty { ChatMarkdown(text: text) }
        else {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(Array(parts.enumerated()), id: \.offset) { _, part in
                    if part.kind == "code" { ExecutableCodeCard(part: part) }
                    else { ChatMarkdown(text: part.text) }
                }
            }
        }
    }
}

struct ExecutableCodeCard: View {
    let part: Muster_App_MessagePart
    var wraps = false
    @Environment(\.executeCode) private var execute
    @Environment(\.colorScheme) private var scheme
    @State private var copied = false
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "terminal").font(.caption.weight(.semibold))
                Text(part.language.isEmpty ? "CODE" : part.language.uppercased())
                    .font(.caption2.weight(.semibold)).tracking(0.8)
                Spacer()
                Button { copyToPasteboard(part.text); copied = true } label: {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc").frame(width: 36, height: 40)
                }.accessibilityLabel("Copy code")
                #if os(iOS)
                if !part.commandID.isEmpty {
                    Button { execute(part.commandID) } label: {
                        Label("Run", systemImage: "play.fill").font(.caption.weight(.semibold))
                            .padding(.horizontal, 12).frame(height: 30)
                            .background(Color.accentColor.opacity(0.12), in: Capsule())
                    }.foregroundStyle(Color.accentColor).accessibilityLabel("Run in Terminal and return result")
                }
                #endif
            }.buttonStyle(.plain).foregroundStyle(.secondary).padding(.horizontal, 12)
            Divider().opacity(0.5)
            if wraps {
                Text(part.text.trimmingCharacters(in: .newlines))
                    .font(.system(size: 13, design: .monospaced)).lineSpacing(5)
                    .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading).padding(14)
            } else {
                ScrollView(.horizontal) {
                    Text(part.text.trimmingCharacters(in: .newlines))
                        .font(.system(size: 13, design: .monospaced)).lineSpacing(5)
                        .textSelection(.enabled).padding(14).frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .background(scheme == .dark ? Color(white: 0.10) : Color(white: 0.975), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(.secondary.opacity(0.15)))
        .accessibilityIdentifier("executable-code-card")
    }
}

struct ConversationNavigation: View {
    let chat: Muster_App_ChatView
    @Binding var selected: String
    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 7) {
                Circle().fill(chat.session.state == "running" ? Color.green : Color.secondary.opacity(0.4)).frame(width: 6, height: 6)
                Text(chat.providerLabel).fontWeight(.medium)
                Text("·")
                Text(chat.session.deviceName).lineLimit(1)
                Spacer(minLength: 8)
                if !chat.session.cwd.isEmpty {
                    Label(URL(fileURLWithPath: chat.session.cwd).lastPathComponent, systemImage: "folder")
                        .lineLimit(1).truncationMode(.middle)
                }
            }.font(.caption).foregroundStyle(.secondary)
            HStack(spacing: 4) {
                tab("Chat", "bubble.left", "Conversation")
                tab("Activity", "waveform.path", "Activity")
                tab("Terminal", "terminal", "Terminal", disabled: !chat.terminal.available)
            }.padding(4).background(.secondary.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
        }.padding(.horizontal, 16).padding(.top, 6).padding(.bottom, 12)
    }
    private func tab(_ title: String, _ icon: String, _ value: String, disabled: Bool = false) -> some View {
        Button { selected = value } label: {
            Label(title, systemImage: icon).font(.subheadline.weight(selected == value ? .semibold : .regular))
                .frame(maxWidth: .infinity).frame(height: 38)
                .foregroundStyle(selected == value ? Color.primary : Color.secondary)
                .background(selected == value ? Color.accentColor.opacity(0.10) : .clear, in: RoundedRectangle(cornerRadius: 10))
        }.buttonStyle(.plain).disabled(disabled)
    }
}

struct CodeExecutionPreview: View {
    let preview: Muster_App_CodePreview
    var cancel: () -> Void
    var run: () -> Void
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Label("Run in your workspace", systemImage: "terminal").font(.title2.weight(.semibold))
                    VStack(alignment: .leading, spacing: 7) {
                        Text(preview.deviceName).font(.subheadline.weight(.medium))
                        Text(preview.cwd).font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(14).background(.secondary.opacity(0.07), in: RoundedRectangle(cornerRadius: 12))
                    ExecutableCodeCard(part: .with { $0.kind = "code"; $0.language = preview.language; $0.text = preview.code }, wraps: true)
                    Label("The exit code and output will be sent back to this conversation when the command finishes.", systemImage: "arrow.turn.up.left")
                        .font(.subheadline).foregroundStyle(.secondary)
                }.padding(20)
            }
            .safeAreaInset(edge: .bottom) {
                Button(action: run) { Label("Run & return result", systemImage: "play.fill").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 10) }
                    .buttonStyle(.borderedProminent).controlSize(.large).padding(16).background(.bar)
            }
            .navigationTitle("Review command")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel", action: cancel) } }
        }
    }
}

struct CodeRunHistory: View {
    let runs: [Muster_App_CodeRunView]
    let error: String
    var send: (String) -> Void
    @State private var selected: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !error.isEmpty { Text(error).font(.caption).foregroundStyle(.orange) }
            if !runs.isEmpty {
                HStack { Text("RECENT RUNS").font(.caption2.weight(.semibold)).tracking(1); Spacer(); Text("\(runs.count)").font(.caption2) }.foregroundStyle(.secondary)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(runs, id: \.run.id) { item in
                            Button { selected = item.run.id } label: {
                                VStack(alignment: .leading, spacing: 8) {
                                    HStack { Image(systemName: icon(item)); Text(status(item)).font(.caption.weight(.medium)); Spacer(); Image(systemName: "chevron.right").font(.caption2) }
                                    Text(item.run.code).font(.system(.caption, design: .monospaced)).lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
                                }.padding(12).frame(width: 250).background(.secondary.opacity(0.07), in: RoundedRectangle(cornerRadius: 12))
                            }.buttonStyle(.plain)
                        }
                    }
                }
            }
        }.padding(.horizontal, 16).padding(.vertical, runs.isEmpty && error.isEmpty ? 0 : 10)
        .sheet(isPresented: Binding(get: { selected != nil }, set: { if !$0 { selected = nil } })) {
            if let item = runs.first(where: { $0.run.id == selected }) {
                NavigationStack {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 16) {
                            Label(status(item), systemImage: icon(item)).font(.headline)
                            Text(item.run.cwd).font(.caption.monospaced()).foregroundStyle(.secondary)
                            ExecutableCodeCard(part: .with { $0.kind = "code"; $0.language = item.run.language; $0.text = item.run.code })
                            if item.run.exitKnown { Text("Exit code \(item.run.exitCode)").font(.caption.monospaced()) }
                            if item.run.truncated { Text("Output truncated").font(.caption).foregroundStyle(.orange) }
                            Text(item.run.output.isEmpty ? (item.run.state == "completed" ? "No output" : "Waiting for output…") : item.run.output).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                            if !item.error.isEmpty { Text(item.error).font(.caption).foregroundStyle(.orange) }
                            if item.canReturn { Button("Return result to conversation") { send(item.run.id) }.buttonStyle(.borderedProminent) }
                        }.padding(20)
                    }.navigationTitle("Execution result")
                    #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                    #endif
                    .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { selected = nil } } }
                }
            }
        }
    }
    private func status(_ item: Muster_App_CodeRunView) -> String {
        if item.delivery == "sent" { return "Returned to conversation" }
        if item.delivery == "sending" { return "Returning result…" }
        if item.delivery == "waiting" { return "Waiting for agent to finish…" }
        if item.delivery == "unknown" { return "Delivery unconfirmed" }
        if item.delivery == "rejected" { return "Result not delivered" }
        if item.run.state == "completed" { return item.run.exitCode == 0 ? "Completed" : "Exited with error" }
        return item.run.state == "running" ? "Running" : "Outcome unavailable"
    }
    private func icon(_ item: Muster_App_CodeRunView) -> String {
        item.delivery == "sent" ? "arrow.turn.up.left" : item.run.state == "completed" ? "checkmark.circle" : "terminal"
    }
}
#endif
