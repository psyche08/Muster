// Desktop Chat mode: the Claude app's and the ChatGPT app's ordinary
// conversations on one Mac. Shared Rust reads and drives them through
// Accessibility; this screen shows what the core lists and forwards taps.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI

extension Muster_App_DesktopChatsView {
    /// The app the core shows chats of, as it names and orders them.
    var app: Muster_App_ChatProvider {
        choices.first { $0.id == provider } ?? choices.first ?? Muster_App_ChatProvider()
    }
}

/// The Chat tab: one app's conversations. There is no device list: every
/// Mac signed in to the account shows the same chats, so the core picks one
/// that can show them now and names it in the footer.
struct DesktopChatsView: View {
    let core: MusterCore
    let view: Muster_App_DesktopChatsView
    let shown: String
    let title: String

    init(core: MusterCore, vm: ViewModel) {
        self.core = core
        view = vm.desktopChats
        shown = vm.screen == .chat ? vm.chat.session.id : ""
        title = vm.screen == .desktopChats ? "Chat" : ""
    }
    private var catalog: Muster_V1_AgentChatCatalog { view.catalog }
    private var app: Muster_App_ChatProvider { view.app }
    private var busy: Bool { !view.opening.isEmpty }
    private var picked: Bool { !view.deviceID.isEmpty }

    var body: some View {
        let core = core, shown = shown, opening = view.opening
        let busy = busy, picked = picked
        List {
            Section {
                Picker("App", selection: Binding(get: { app.id }, set: { core.openDesktopChats(provider: $0) })) {
                    ForEach(view.choices, id: \.id) { Text($0.label).tag($0.id) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .accessibilityIdentifier("chat-app-picker")
                Button { core.desktopChatOpen("") } label: {
                    Label("New chat", systemImage: "square.and.pencil")
                }
                .disabled(busy || !picked)
            } footer: {
                if picked {
                    Text("Through \(view.deviceName). Every Mac shows the same \(app.label) chats; another one is used when this one is busy.")
                } else if view.loading {
                    Text("Finding a Mac that can show \(app.label) chats…")
                }
            }
            if view.loading && view.hasCatalog { ProgressView("Updating conversations…") }
            if !view.error.isEmpty {
                Section {
                    Label(view.error, systemImage: "exclamationmark.triangle").foregroundStyle(.secondary)
                }
            }
            if view.hasCatalog {
                Section {
                    if !catalog.chatMode {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("\(app.label) is not in its Chat mode on \(view.deviceName).")
                                .foregroundStyle(.secondary)
                            Button {
                                core.desktopChatOpen("", onDesktop: true)
                            } label: {
                                if view.opening == "new" { ProgressView() } else { Text("Switch to Chat mode") }
                            }
                            .buttonStyle(.bordered)
                            .disabled(busy || !picked)
                        }
                        .padding(.vertical, 4)
                    } else if catalog.chats.isEmpty {
                        Text("No conversations").foregroundStyle(.secondary)
                    }
                    ForEach(catalog.chats, id: \.id) { chat in
                        Button { core.desktopChatOpen(chat.id) } label: {
                            HStack {
                                Text(chat.title).lineLimit(2)
                                Spacer(minLength: 8)
                                if opening == chat.id {
                                    ProgressView().controlSize(.small)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(busy || !picked)
                        .listRowBackground(shown == chat.id ? Color.accentColor.opacity(0.15) : nil)
                    }
                } header: {
                    Text("Conversations")
                }
            } else if view.loading {
                ProgressView()
            }
        }
        .navigationTitle(title)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .refreshable { core.refresh() }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { core.refresh() } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    .disabled(view.loading)
            }
        }
    }
}

/// The right side of a Mac's split view before anything is chosen.
struct WorkspaceWelcome: View {
    let core: MusterCore
    let vm: ViewModel

    private var chat: Bool { vm.workspace == .desktopChats }
    private var appName: String { vm.desktopChats.app.label }

    var body: some View {
        VStack(spacing: 14) {
            MusterMark(size: 72)
            Text(chat ? "Chat with \(appName)" : "Your agents").font(.title2.weight(.semibold))
            Text(chat
                 ? "Choose a conversation on the left, or start a new one. It is shown in \(appName) on a Mac, and its replies are read from there."
                 : "Choose a Claude or Codex session on the left to follow it and send it messages.")
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            if chat {
                Button { core.desktopChatOpen("") } label: { Label("New chat", systemImage: "square.and.pencil") }
                    .buttonStyle(.borderedProminent)
                    .disabled(vm.desktopChats.deviceID.isEmpty || !vm.desktopChats.opening.isEmpty)
            }
        }
        .frame(maxWidth: 420)
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// A desktop chat's model and effort, as the app on the Mac offers them.
/// Opening reads them there (the Mac shows its picker briefly); a tap
/// chooses by name. What cannot be set through Accessibility is shown with
/// the app's reason.
struct ChatOptionsSheet: View {
    let core: MusterCore
    let chat: Muster_App_ChatView
    let done: () -> Void

    private var options: Muster_V1_AgentChatOptions { chat.chatOptions }

    var body: some View {
        NavigationStack {
            List {
                if !chat.chatOptionsError.isEmpty {
                    Label(chat.chatOptionsError, systemImage: "exclamationmark.triangle").foregroundStyle(.secondary)
                }
                Section("Model") {
                    if options.models.isEmpty && chat.chatOptionsBusy { ProgressView() }
                    ForEach(options.models, id: \.name) { model in
                        Button { core.chatSetOptions(model: model.name) } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(model.name)
                                    if !model.detail.isEmpty {
                                        Text(model.detail).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                if model.selected { Image(systemName: "checkmark").foregroundStyle(Color.accentColor) }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(chat.chatOptionsBusy || model.selected)
                    }
                }
                Section {
                    if options.efforts.isEmpty {
                        LabeledContent("Effort", value: options.effort.isEmpty ? "—" : options.effort)
                    }
                    ForEach(options.efforts, id: \.name) { effort in
                        Button { core.chatSetOptions(effort: effort.name) } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(effort.name)
                                    if !effort.detail.isEmpty {
                                        Text(effort.detail).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                if effort.selected { Image(systemName: "checkmark").foregroundStyle(Color.accentColor) }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(chat.chatOptionsBusy || effort.selected)
                    }
                } header: {
                    Text("Reasoning")
                } footer: {
                    if !options.note.isEmpty { Text(options.note) }
                }
            }
            .navigationTitle("Model and effort")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done", action: done) }
                ToolbarItem(placement: .primaryAction) {
                    if chat.chatOptionsBusy {
                        ProgressView().controlSize(.small)
                    } else {
                        Button { core.chatOptionsLoad() } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    }
                }
            }
        }
        .onAppear { core.chatOptionsLoad() }
        #if os(macOS)
        .frame(minWidth: 380, minHeight: 360)
        #endif
    }
}
#endif
