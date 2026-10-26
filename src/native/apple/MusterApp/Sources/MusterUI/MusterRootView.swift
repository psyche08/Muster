// Rust owns the stack; iOS renders it with native navigation and sends pop intents.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI

public struct MusterRootView: View {
    @State private var pendingBrowserURL: URL?
    @ObservedObject var model: MusterModel
    @ObservedObject private var display = FoldDisplay.shared
    /// The half-fold split for the workspace on screen (iOS; a Mac keeps its own split).
    @State private var layout = WorkspaceLayout()
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage(ConversationStyle.preferenceKey) private var conversationStyle = ConversationStyle.codex

    public init(model: MusterModel) {
        self.model = model
    }

    public var body: some View {
        Group {
            if let error = model.startError {
                ContentUnavailableView {
                    Label("Muster could not start", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("Try again") { model.retry() }.buttonStyle(.borderedProminent)
                }
            } else if !model.started {
                ProgressView("Starting")
            } else if let core = model.core {
                workspaceNavigation(core: core)
                #if os(macOS)
                .sheet(isPresented: Binding(get: { model.vm.desktopSetup.visible }, set: { if !$0 { core.desktopSetup(.dismiss) } })) {
                    DesktopSetupView(setup: model.vm.desktopSetup, busy: model.vm.busy, accessibility: { core.requestAccessibility() }, install: { core.desktopSetup(.install) }, iterm: { core.desktopSetup(.iterm) }, later: { core.desktopSetup(.dismiss) }, refresh: { core.desktopSetup(.check) }, restart: { core.desktopSetup(.restart) })
                }
                #endif
                .overlay(alignment: .bottom) { ToastStack(toasts: model.vm.toasts) { core.dismiss(toast: $0) } }
                .overlay { if let reason = model.authenticating { AuthenticatingOverlay(reason: reason) } }
                .sheet(isPresented: Binding(get: { model.scanRequest != nil }, set: { if !$0 { model.scanned(nil) } })) {
                    InviteInputSheet { model.scanned($0) }
                }
            }
        }
        .environment(\.conversationStyle, conversationStyle)
        .onOpenURL { url in
            guard url.scheme == "muster", url.host == "browser" else { return }
            pendingBrowserURL = url
            deliverBrowserURL()
        }
        .onChange(of: model.started) { _, _ in deliverBrowserURL() }
        .onChange(of: model.vm.serviceDevices.count) { _, _ in deliverBrowserURL() }
        .onChange(of: scenePhase) { _, phase in model.lifecycle(phase) }
    }

    @ViewBuilder
    private func workspaceNavigation(core: MusterCore) -> some View {
        #if os(iOS)
        let path = model.vm.navigationPath
        let root = path.first ?? model.vm.screen
        NavigationStack(path: Binding(
            get: { Array(model.vm.navigationPath.dropFirst()) },
            set: { remaining in
                let current = model.vm.navigationPath
                // UIKit supplies a shorter prefix after Back, an edge swipe,
                // or a selection in the native Back menu. Rust validates it.
                guard model.authenticating == nil,
                      remaining.count < current.count - 1,
                      Array(current.dropFirst().prefix(remaining.count)) == remaining else { return }
                core.back(count: current.count - 1 - remaining.count, expectedPath: current)
            }
        )) {
            navigationPage(core: core, route: root)
                .navigationDestination(for: Muster_App_Screen.self) { route in
                    navigationPage(core: core, route: route)
                }
        }
        .id(root)
        #else
        NavigationStack { page(core: core, vm: model.vm) }
        #endif
    }

    #if os(iOS)
    private func navigationPage(core: MusterCore, route: Muster_App_Screen) -> some View {
        NativeNavigationPage(model: model, route: route) { vm in page(core: core, vm: vm) }
    }
    #endif

    private func page(core: MusterCore, vm: Muster_App_ViewModel) -> some View {
        GeometryReader { geometry in
            content(core: core, vm: vm)
                .frame(width: geometry.size.width, height: geometry.size.height)
                .onAppear { resolveLayout(geometry.size) }
                .onChange(of: geometry.size) { _, size in resolveLayout(size) }
                .onChange(of: display.preference) { _, _ in resolveLayout(geometry.size) }
                .onChange(of: display.keyboardHeight) { _, _ in resolveLayout(geometry.size) }
        }
        .environment(\.workspaceLayout, layout)
        .toolbar {
            if Self.roots.contains(vm.screen) || split(vm) {
                ToolbarItem(placement: .navigation) { WorkspaceSwitcher(core: core, vm: vm) }
                #if os(iOS)
                if vm.screen != .agents && !(split(vm) && vm.workspace == .agents) {
                    ToolbarItem(placement: .primaryAction) {
                        SettingsButton(pending: !vm.pending.isEmpty) { core.navigate(to: .settings) }
                    }
                }
                #endif
            }
            #if os(macOS)
            if vm.canGoBack && !(split(vm) && [.chat, .terminal].contains(vm.screen)) {
                ToolbarItem(placement: .navigation) {
                    Button { core.back() } label: { Label("Back", systemImage: "chevron.left") }
                }
            }
            #endif
            if vm.busy {
                ToolbarItem(placement: .primaryAction) { ProgressView().controlSize(.small) }
            }
        }
    }

    private func deliverBrowserURL() {
        guard !model.vm.serviceDevices.isEmpty, let core = model.core, let url = pendingBrowserURL,
              let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { return }
        pendingBrowserURL = nil
        core.openBrowser(device: query.first { $0.name == "device" }?.value ?? "",
                         draft: query.first { $0.name == "request" }?.value ?? "")
    }

    /// The workspace tabs, where the switcher replaces Back (the core never
    /// shows HOME: it routes it to AGENTS).
    private static let roots: [Muster_App_Screen] = [.desktopChats, .agents, .terminals, .multica]
    /// What a Mac shows to the right of the Chat or Agents list.
    private static let details: [Muster_App_Screen] = [.chat, .files, .file, .git]

    /// On a Mac the Chat and Agents tabs are split: the list on the left
    /// quarter, the conversation (or a welcome page) on the right.
    private func split(_ vm: Muster_App_ViewModel) -> Bool {
        #if os(macOS)
        return [.desktopChats, .agents].contains(vm.workspace)
            && (vm.screen == vm.workspace || Self.details.contains(vm.screen))
        #else
        // The workspace's list on the left, the conversation or terminal on the right.
        return vm.workspace != .multica && layout.side && (Self.roots.contains(vm.screen) || Self.phoneDetails.contains(vm.screen))
        #endif
    }

    /// What a side-by-side split shows beside the workspace's list on a phone.
    private static let phoneDetails: [Muster_App_Screen] = [.chat, .files, .file, .git, .terminal]

    private func resolveLayout(_ size: CGSize) {
        #if os(iOS)
        layout = display.layout(for: size)
        #endif
    }

    @ViewBuilder
    private func content(core: MusterCore, vm: Muster_App_ViewModel) -> some View {
        if split(vm) {
            GeometryReader { geometry in
                HStack(spacing: 0) {
                    Group {
                        if vm.workspace == .desktopChats {
                            DesktopChatsView(core: core, vm: vm)
                        } else if vm.workspace == .terminals {
                            TerminalsView(core: core, vm: vm)
                        } else {
                            HomeView(core: core, vm: vm)
                        }
                    }
                    .frame(width: listWidth(geometry.size.width))
                    Divider().frame(width: gapWidth)
                    Group {
                        if vm.screen == vm.workspace || Self.roots.contains(vm.screen) {
                            WorkspaceWelcome(core: core, vm: vm)
                        } else {
                            screen(core: core, vm: vm)
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        } else {
            screen(core: core, vm: vm)
        }
    }

    private func listWidth(_ total: CGFloat) -> CGFloat {
        #if os(macOS)
        max(240, total / 4)
        #else
        layout.listWidth
        #endif
    }

    private var gapWidth: CGFloat? {
        #if os(macOS)
        nil
        #else
        layout.gapWidth
        #endif
    }

    @ViewBuilder
    private func screen(core: MusterCore, vm: Muster_App_ViewModel) -> some View {
        switch vm.screen {
        case .onboarding: OnboardingView(core: core, vm: vm)
        case .invite: InviteView(core: core, vm: vm)
        case .join: JoinView(core: core, vm: vm)
        case .device: DeviceView(core: core, vm: vm)
        case .home, .agents: HomeView(core: core, vm: vm)
        case .chat: ChatView(core: core, vm: vm).id("\(vm.chat.deviceID):\(vm.chat.session.id)")
        case .desktopChats: DesktopChatsView(core: core, vm: vm)
        case .multica: MulticaView(core: core, status: vm.multica)
        case .terminals: TerminalsView(core: core, vm: vm)
        case .terminal: TerminalScreen(core: core, vm: vm)
        case .browser: BrowserView(core: core, vm: vm)
        case .settings: SettingsView(core: core, vm: vm, model: model)
        case .files: FilesView(core: core, vm: vm)
        case .file: FileView(core: core, vm: vm)
        case .git: GitView(core: core, vm: vm)
        case .UNRECOGNIZED: ContentUnavailableView("Unknown screen", systemImage: "questionmark")
        }
    }
}

#if os(iOS)
/// Keep the last rendered page for UIKit's interactive transitions. This is
/// presentation only: no route or transition is derived from this snapshot.
private struct NativeNavigationPage<Content: View>: View {
    @ObservedObject var model: MusterModel
    let route: Muster_App_Screen
    let content: (Muster_App_ViewModel) -> Content
    @State private var snapshot: Muster_App_ViewModel

    init(model: MusterModel, route: Muster_App_Screen, @ViewBuilder content: @escaping (Muster_App_ViewModel) -> Content) {
        self.model = model
        self.route = route
        self.content = content
        var initial = model.vm
        initial.screen = route
        _snapshot = State(initialValue: initial)
    }

    var body: some View {
        content(model.vm.screen == route ? model.vm : snapshot)
            .onChange(of: model.vm) { old, new in
                if new.screen == route { snapshot = new }
                else if old.screen == route { snapshot = old }
            }
    }
}
#endif

extension View {
    /// The inset-grouped look on every platform; macOS's default form style
    /// is the columnar one, which reads placeholders as row labels.
    func groupedForm() -> some View {
        #if os(macOS)
        return self.formStyle(.grouped)
        #else
        return self
        #endif
    }
}

struct ToastStack: View {
    let toasts: [Muster_App_Toast]
    let dismiss: (String) -> Void

    var body: some View {
        VStack(spacing: 6) {
            ForEach(toasts, id: \.id) { toast in
                HStack {
                    Image(systemName: toast.error ? "exclamationmark.circle.fill" : "info.circle.fill")
                    Text(toast.text).font(.callout)
                    Spacer(minLength: 0)
                }
                .padding(10)
                .background(toast.error ? Color.red.opacity(0.9) : Color.secondary.opacity(0.85), in: RoundedRectangle(cornerRadius: 10))
                .foregroundStyle(.white)
                .onTapGesture { dismiss(toast.id) }
                .task {
                    try? await Task.sleep(for: .seconds(toast.error ? 8 : 4))
                    dismiss(toast.id)
                }
            }
        }
        .padding()
        .animation(.default, value: toasts.map(\.id))
    }
}

struct AuthenticatingOverlay: View {
    let reason: String
    var body: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(reason).font(.callout).multilineTextAlignment(.center)
        }
        .padding(24)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
        .padding()
    }
}

/// The QR scanner on iPhone, a paste field elsewhere. Both hand back the
/// raw invite text; the core validates it.
struct InviteInputSheet: View {
    let done: (String?) -> Void
    @State private var text = ""

    var body: some View {
        NavigationStack {
            Group {
                #if os(iOS)
                if QRScannerView.isAvailable {
                    QRScannerView { done($0) }
                        .ignoresSafeArea(edges: .bottom)
                } else {
                    pasteForm
                }
                #else
                pasteForm
                #endif
            }
            .navigationTitle("Join a network")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { done(nil) } }
            }
        }
        #if os(macOS)
        .frame(minWidth: 420, minHeight: 220)
        #endif
    }

    private var pasteForm: some View {
        Form {
            Section {
                #if os(iOS)
                MusterTextEditor("muster1:…", text: $text, monospace: true, minimumHeight: 80, maximumHeight: 160)
                #else
                TextField("muster1:…", text: $text, axis: .vertical)
                    .lineLimit(3...6)
                    .font(.system(.body, design: .monospaced))
                    .autocorrectionDisabled()
                #endif
                Button("Join") { done(text.trimmingCharacters(in: .whitespacesAndNewlines)) }
                    .disabled(!text.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("muster1:"))
            } header: {
                Text("Invite")
            } footer: {
                Text("Paste the invite text shown on the admin's device.")
            }
        }
        .groupedForm()
    }
}
#endif
