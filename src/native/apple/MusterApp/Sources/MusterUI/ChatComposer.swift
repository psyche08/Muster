#if canImport(SwiftUI)
import ImageIO
import MusterCore
import SwiftUI
import UniformTypeIdentifiers
#if os(iOS)
import PhotosUI
import UIKit
#else
import AppKit
#endif

private struct ComposerAttachment: Identifiable {
    let id = UUID().uuidString
    let value: Muster_V1_AgentAttachment
    var thumbnail: CGImage? { Self.image(value.data, size: 256) }

    static func image(_ data: Data, size: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: size,
            kCGImageSourceShouldCacheImmediately: true,
        ] as CFDictionary)
    }
}

struct ChatComposer: View {
    let core: MusterCore
    let chat: Muster_App_ChatView
    @Environment(\.colorScheme) private var scheme
    @Environment(\.conversationStyle) private var style
    @Environment(\.workspaceLayout) private var layout
    #if os(iOS)
    @Environment(\.keyboardDock) private var keyboardDock
    @ObservedObject private var keyboardPrefs = KeyboardPrefs.shared
    #endif
    @State private var text = ""
    @State private var draftRevision = ""
    @State private var composerRegistration: UUID?
    @State private var submittedRevision = ""
    @State private var attachments: [ComposerAttachment] = []
    @State private var importing = false
    @State private var importer = false
    @State private var error: String?
    @State private var preview: ComposerAttachment?
    @State private var submittedDraft = ""
    #if os(iOS)
    @State private var keyboardFocusRequest = 0
    @State private var photoPicker = false
    @State private var photos: [PhotosPickerItem] = []
    #endif
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }
    private var busy: Bool { chat.sending || chat.completing || importing }
    private var empty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && attachments.isEmpty }
    private var draftText: Binding<String> {
        Binding(get: { text }, set: { value in
            // AppKit may echo the same value when focus or enabled state
            // changes. Only a real edit invalidates the submitted revision.
            guard value != text else { return }
            text = value
            draftRevision = UUID().uuidString
        })
    }

    private var compact: Bool {
        #if os(iOS)
        true
        #else
        false
        #endif
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if chat.hasUnconfirmed { unconfirmed(chat.unconfirmed) }
            if text.isEmpty && chat.canComplete {
                HStack {
                    Button { core.composerComplete() } label: {
                        Label("Read suggestion", systemImage: "text.cursor")
                    }
                    .disabled(busy || chat.completing)
                    if chat.completing { ProgressView().controlSize(.small) }
                }
                if !chat.completionError.isEmpty {
                    Text(chat.completionError).font(.caption).foregroundStyle(.secondary)
                }
            }
            if text.isEmpty && !chat.composerSuggestion.isEmpty {
                HStack(alignment: .center, spacing: 8) {
                    Text(chat.composerSuggestion).textSelection(.enabled)
                        .font(.callout).frame(maxWidth: .infinity, alignment: .leading)
                    Button {
                        draftText.wrappedValue = chat.composerSuggestion
                        #if os(iOS)
                        keyboardFocusRequest += 1
                        #endif
                    } label: {
                        Label("Use", systemImage: "arrow.down.to.line").font(.callout)
                            .frame(minWidth: 44, minHeight: 44).padding(.horizontal, 8)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain).accessibilityLabel("Use suggestion")
                    .disabled(busy).help("Use suggestion")
                }
                .padding(.leading, 12).padding(.vertical, 4)
                .background(palette.composer, in: RoundedRectangle(cornerRadius: 16))
            }
            VStack(alignment: .leading, spacing: 12) {
                if !attachments.isEmpty {
                    ScrollView(.horizontal) {
                        HStack(spacing: 10) {
                            ForEach(attachments) { item in
                                attachment(item)
                            }
                        }.padding(4)
                    }
                }
                if compact {
                    editor.padding(.horizontal, 6).padding(.top, 6)
                    HStack(alignment: .center, spacing: 10) {
                        if acceptsAttachments { attachmentMenu }
                        #if os(iOS)
                        Menu {
                            Button("Muster 内置键盘") { keyboardPrefs.systemKeyboard = false; keyboardFocusRequest += 1 }
                            if keyboardDock == nil {
                                Button("系统键盘") { keyboardPrefs.systemKeyboard = true; keyboardFocusRequest += 1 }
                            }
                        } label: {
                            Image(systemName: "keyboard").frame(width: 44, height: 44)
                        }
                        .accessibilityLabel("选择键盘")
                        .disabled(busy)
                        #endif
                        Text(chat.providerLabel).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        Spacer(minLength: 0)
                        if busy { ProgressView().controlSize(.small) }
                        sendControl
                    }
                } else {
                    editor
                    if style == .codex { controls }
                }
            }
            .padding(compact ? 8 : 16)
            .background(palette.composer, in: RoundedRectangle(cornerRadius: compact ? 22 : style.composerRadius))
            .overlay(RoundedRectangle(cornerRadius: compact ? 22 : style.composerRadius)
                .stroke(palette.border.opacity(style == .claude ? 1 : 0.5)))
            if style == .claude && !compact { controls.padding(.horizontal, 4) }
            if let error { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
        }
        .fileImporter(isPresented: $importer, allowedContentTypes: [.data], allowsMultipleSelection: true) { result in
            switch result {
            case .success(let urls):
                importing = true
                Task { @MainActor in
                    defer { importing = false }
                    for url in urls {
                        do { try addFile(url) } catch { self.error = error.localizedDescription }
                    }
                }
            case .failure(let failure): error = failure.localizedDescription
            }
        }
        .onDrop(of: [.image, .fileURL], isTargeted: nil) { providers in
            guard !busy, acceptsAttachments else { return false }
            load(providers)
            return true
        }
        .onAppear {
            text = chat.draft.text; draftRevision = chat.draft.revision
            if draftRevision.isEmpty { draftRevision = UUID().uuidString }
            // Capture bindings, not the view/core. No callback runs on typing.
            let buffer = $text, revision = $draftRevision, key = chat.draft.key
            composerRegistration = core.composerDraft.register {
                .with { $0.key = key; $0.text = buffer.wrappedValue; $0.revision = revision.wrappedValue }
            }
        }
        .onDisappear {
            if let id = composerRegistration { core.composerDraft.unregister(id) }
            composerRegistration = nil
        }
        .onChange(of: chat.draft) { _, draft in
            // Older snapshots cannot overwrite newer keystrokes. Rust can
            // clear this exact revision when a send finishes after re-entry.
            if draft.revision == draftRevision { text = draft.text }
        }
        .onChange(of: chat.sentDraftID) { _, receipt in
            guard !receipt.isEmpty, receipt == submittedDraft else { return }
            guard draftRevision == submittedRevision else { submittedDraft = ""; return }
            text = ""; attachments = []; submittedDraft = ""; error = nil
        }
        .sheet(item: $preview) { item in
            VStack(spacing: 12) {
                HStack {
                    Text(item.value.name).font(.headline).lineLimit(2)
                    Spacer()
                    Button("Done") { preview = nil }
                }
                if let image = ComposerAttachment.image(item.value.data, size: 2048) {
                    Image(decorative: image, scale: 1).resizable().scaledToFit()
                }
            }.padding(20)
            #if os(macOS)
            .frame(minWidth: 480, idealWidth: 720, minHeight: 360, idealHeight: 560)
            #endif
        }
        #if os(iOS)
        .photosPicker(isPresented: $photoPicker, selection: $photos, matching: .images)
        .onChange(of: photos) { _, items in
            guard !busy else { return }
            importing = true
            Task { @MainActor in
                defer { importing = false; photos = [] }
                for item in items {
                    do {
                        if let data = try await item.loadTransferable(type: Data.self) {
                            let type = item.supportedContentTypes.first(where: { $0.conforms(to: .image) }) ?? .image
                            try add(data, name: "Photo.\(type.preferredFilenameExtension ?? "image")", type: type)
                        }
                    } catch { self.error = error.localizedDescription }
                }
            }
        }
        #endif
    }

    /// Rust sets the limit to zero where attachments are not offered (desktop chats).
    private var acceptsAttachments: Bool { chat.attachmentCountLimit > 0 }

    private var controls: some View {
        HStack(spacing: 14) {
            if acceptsAttachments { attachmentMenu }
            if style == .claude { Spacer() }
            Text(chat.providerLabel)
                .font(.system(size: ChatStyle.secondarySize)).foregroundStyle(palette.secondary)
            if style == .codex { Spacer() }
            if busy { ProgressView().controlSize(.small) }
            sendControl
        }
    }

    private var attachmentMenu: some View {
        Menu {
            Button { importer = true } label: { Label("Add files", systemImage: "paperclip") }
            #if os(iOS)
            Button { photoPicker = true } label: { Label("Photo library", systemImage: "photo") }
            #endif
            Button { pasteImage() } label: { Label("Paste image", systemImage: "doc.on.clipboard") }
        } label: {
            Image(systemName: "plus").font(.system(size: 20))
                .frame(width: compact ? 44 : 30, height: compact ? 44 : 30)
        }
        .menuStyle(.borderlessButton).fixedSize().disabled(busy)
        .accessibilityLabel("Add attachment")
        .foregroundStyle(palette.text)
        .frame(minWidth: compact ? 44 : 30, minHeight: compact ? 44 : 30)
    }

    @ViewBuilder private var sendControl: some View {
        if chat.canInterrupt && empty && !chat.sending {
            Button { core.agentInterrupt() } label: {
                Image(systemName: "stop.fill").font(.system(size: 13))
                    .frame(width: compact ? 44 : 32, height: compact ? 44 : 32)
                    .foregroundStyle(.white).background(style == .claude ? palette.accent : palette.action, in: RoundedRectangle(cornerRadius: style == .claude ? 8 : 18))
            }.buttonStyle(.plain).accessibilityLabel("Stop")
        } else {
            Button(action: send) {
                Image(systemName: "arrow.up").font(.system(size: 17, weight: .semibold))
                    .frame(width: compact ? 44 : 32, height: compact ? 44 : 32)
                    .foregroundStyle(style == .claude ? Color.white : palette.canvas)
                    .background(palette.accent, in: RoundedRectangle(cornerRadius: style == .claude ? 8 : 18))
            }
            .buttonStyle(.plain).disabled(empty || busy)
            .opacity(empty || busy ? 0.35 : 1)
            .keyboardShortcut(.return, modifiers: .command)
            .accessibilityLabel("Send message")
        }
    }

    /// The core's empty-composer hint, else the style's.
    private var placeholder: String {
        if !chat.composerPlaceholder.isEmpty { return chat.composerPlaceholder }
        return compact && style == .codex ? "Follow up" : style.placeholder
    }

    @ViewBuilder private var editor: some View {
        #if os(macOS)
        TextField(placeholder, text: draftText, axis: .vertical)
            // Avoid SwiftUI recursively deriving the multiline field's AX label.
            .accessibilityLabel("Message")
            .textFieldStyle(.plain).lineLimit((style == .claude ? 1 : 3)...8)
            .frame(minHeight: style == .claude ? 28 : 60, alignment: .topLeading)
            .disabled(busy).onSubmit(send)
            .modifier(ComposerReturnKey(send: send))
            .onPasteCommand(of: [.image, .fileURL], perform: load)
        #else
        ZStack(alignment: .topLeading) {
            if text.isEmpty { Text(placeholder).lineLimit(2).foregroundStyle(.secondary).padding(.top, 1) }
            // Top and bottom: the keyboard is up from the start, below the crease.
            PasteTextEditor(text: draftText, disabled: busy, fontSize: ChatStyle.bodySize * style.bodyScale, minimumHeight: compact ? 26 : (style == .claude ? 28 : 60),
                            inAppKeyboard: !keyboardPrefs.systemKeyboard, compactKeyboard: layout.flex, autoFocus: layout.flex, focusRequest: keyboardFocusRequest, dock: keyboardDock) { images in
                for (index, data) in images.enumerated() {
                    do { try add(data, name: "Pasted image \(index + 1).png", type: .png) }
                    catch { self.error = error.localizedDescription }
                }
            }
        }.fixedSize(horizontal: false, vertical: true)
        #endif
    }

    private func attachment(_ item: ComposerAttachment) -> some View {
        HStack(spacing: 8) {
            Button { if item.thumbnail != nil { preview = item } } label: {
                if let image = item.thumbnail {
                    Image(decorative: image, scale: 1).resizable().scaledToFill()
                        .frame(width: 64, height: 64).clipped().clipShape(RoundedRectangle(cornerRadius: 8))
                } else {
                    Image(systemName: "doc").font(.title2).frame(width: 40, height: 56)
                }
            }.buttonStyle(.plain).accessibilityLabel("Preview \(item.value.name)")
            VStack(alignment: .leading, spacing: 4) {
                Text(item.value.name).font(.caption).lineLimit(1).frame(maxWidth: 150, alignment: .leading)
                Text(ByteCountFormatter.string(fromByteCount: Int64(item.value.data.count), countStyle: .file))
                    .font(.caption2).foregroundStyle(palette.secondary)
            }
            Button { attachments.removeAll { $0.id == item.id } } label: {
                Image(systemName: "xmark.circle.fill").foregroundStyle(palette.secondary)
            }.buttonStyle(.plain).disabled(busy).accessibilityLabel("Remove \(item.value.name)")
        }
        .padding(8).background(palette.canvas.opacity(0.65), in: RoundedRectangle(cornerRadius: 12))
    }

    private func send() { send(releasing: "") }

    private func send(releasing token: String) {
        guard !empty && !busy else { return }
        submittedDraft = UUID().uuidString
        submittedRevision = draftRevision
        core.agentSend(text, attachments: attachments.map(\.value), draftID: submittedDraft,
                       releaseUnconfirmed: token)
    }

    private func unconfirmed(_ refusal: Muster_App_UnconfirmedSend) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("上一条消息还没有在 Mac 上确认送达").font(.subheadline.weight(.semibold))
            Text("重试将按正常流程发送输入框中的内容；忽略将关闭此提示。")
                .font(.caption).fixedSize(horizontal: false, vertical: true)
            Text(refusal.message).font(.caption2).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            HStack {
                Button("重试") { send(releasing: refusal.token) }
                    .buttonStyle(.borderedProminent).tint(.orange).controlSize(.small)
                    .disabled(empty || busy)
                Button("忽略") { core.ignoreAgentSend(token: refusal.token) }
                    .buttonStyle(.bordered).controlSize(.small).disabled(busy)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
    }

    private func add(_ data: Data, name: String, type: UTType) throws {
        // Bound OS import memory using the core's advertised input limits.
        guard attachments.count < Int(chat.attachmentCountLimit),
              data.count + attachments.reduce(0, { $0 + $1.value.data.count }) <= Int(chat.attachmentBytesLimit) else {
            throw CocoaError(.fileReadTooLarge, userInfo: [NSLocalizedDescriptionKey: "Up to \(chat.attachmentCountLimit) files, \(ByteCountFormatter.string(fromByteCount: Int64(chat.attachmentBytesLimit), countStyle: .file)) total."])
        }
        let value = Muster_V1_AgentAttachment.with {
            $0.name = name; $0.mediaType = type.preferredMIMEType ?? "application/octet-stream"; $0.data = data
        }
        attachments.append(ComposerAttachment(value: value)); error = nil
    }

    private func addFile(_ url: URL) throws {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let data = try handle.read(upToCount: Int(chat.attachmentBytesLimit) + 1) ?? Data()
        try add(data, name: url.lastPathComponent, type: UTType(filenameExtension: url.pathExtension) ?? .data)
    }

    private func load(_ providers: [NSItemProvider]) {
        guard !busy else { return }
        importing = true
        Task { @MainActor in
            defer { importing = false }
            for provider in providers {
                do {
                    if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                        let data = try await representation(provider, UTType.fileURL.identifier)
                        guard let url = URL(dataRepresentation: data, relativeTo: nil), url.isFileURL else { continue }
                        try addFile(url)
                    } else if let identifier = provider.registeredTypeIdentifiers.first(where: { UTType($0)?.conforms(to: .image) == true }) {
                        var data = try await representation(provider, identifier)
                        let type = UTType(identifier) ?? .image
                        if type.conforms(to: .tiff),
                           let source = CGImageSourceCreateWithData(data as CFData, nil) {
                            // Screenshots often arrive as uncompressed TIFF on macOS.
                            // Encode those as PNG, preserving the original image dimensions.
                            let png = NSMutableData()
                            guard let destination = CGImageDestinationCreateWithData(png, UTType.png.identifier as CFString, 1, nil) else {
                                throw CocoaError(.fileReadCorruptFile)
                            }
                            CGImageDestinationAddImageFromSource(destination, source, 0, nil)
                            guard CGImageDestinationFinalize(destination) else { throw CocoaError(.fileReadCorruptFile) }
                            data = png as Data
                            try add(data, name: "Pasted image.png", type: .png)
                        } else {
                            try add(data, name: "Pasted image.\(type.preferredFilenameExtension ?? "image")", type: type)
                        }
                    }
                } catch { self.error = error.localizedDescription }
            }
        }
    }

    private func representation(_ provider: NSItemProvider, _ identifier: String) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            provider.loadDataRepresentation(forTypeIdentifier: identifier) { data, error in
                if let data { continuation.resume(returning: data) }
                else { continuation.resume(throwing: error ?? CocoaError(.fileReadUnknown)) }
            }
        }
    }

    private func pasteImage() {
        #if os(macOS)
        // Read only on this explicit user action; normal Cmd-V uses onPasteCommand.
        guard let image = NSImage(pasteboard: .general), let tiff = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff), let data = bitmap.representation(using: .png, properties: [:]) else {
            error = "No image on the clipboard"; return
        }
        #else
        guard let data = UIPasteboard.general.image?.pngData() else { error = "No image on the clipboard"; return }
        #endif
        do { try add(data, name: "Pasted image.png", type: .png) } catch { self.error = error.localizedDescription }
    }
}

#if os(iOS)
// UIKit is used only for its native paste event; Rust owns validation and send.
struct PasteTextEditor: UIViewRepresentable {
    @Binding var text: String
    let disabled: Bool
    let fontSize: CGFloat
    let minimumHeight: CGFloat
    /// The Muster keyboard as the view's input view (Settings), compact below a half-fold crease.
    var inAppKeyboard = false
    var compactKeyboard = false
    var autoFocus = false
    var focusRequest = 0
    var dock: KeyboardDock? = nil
    let images: ([Data]) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIView(context: Context) -> PasteTextView {
        let view = PasteTextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.keyboardDismissMode = .interactive
        view.accessibilityLabel = "Message"
        return view
    }
    func updateUIView(_ view: PasteTextView, context: Context) {
        context.coordinator.parent = self
        if view.text != text {
            view.text = text
            view.selectedRange = NSRange(location: (text as NSString).length, length: 0)
        }
        view.isEditable = !disabled
        view.images = images
        view.font = UIFontMetrics.default.scaledFont(for: .systemFont(ofSize: fontSize))
        view.adjustsFontForContentSizeCategory = true
        view.textColor = .label
        context.coordinator.keyboard.attach(to: view, systemKeyboard: !inAppKeyboard, enterLabel: "换行", compact: compactKeyboard,
                                            submit: nil, multiline: true, changed: { [weak view, weak coordinator = context.coordinator] in
                                                if let view { coordinator?.textViewDidChange(view) }
                                            }, dock: dock)
        if autoFocus && !context.coordinator.focused {
            context.coordinator.focused = true
            view.requestKeyboardFocus()
        }
        if context.coordinator.focusRequest != focusRequest {
            context.coordinator.focusRequest = focusRequest
            view.requestKeyboardFocus()
        }
        view.restoreKeyboardFocus()
        if !autoFocus { context.coordinator.focused = false }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, uiView: PasteTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 300
        let height = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        return CGSize(width: width, height: min(160, max(minimumHeight, height)))
    }
    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: PasteTextEditor
        let keyboard = KeyboardAttachment()
        var focused = false
        var focusRequest = 0
        init(_ parent: PasteTextEditor) { self.parent = parent }
        func textViewDidChange(_ textView: UITextView) { parent.text = textView.text }
    }
}

final class PasteTextView: KeyboardTextView {
    var images: (([Data]) -> Void)?
    override func paste(_ sender: Any?) {
        guard isEditable else { return }
        if UIPasteboard.general.hasImages {
            let data = (UIPasteboard.general.images ?? []).compactMap { $0.pngData() }
            if !data.isEmpty { images?(data); return }
        }
        super.paste(sender)
    }
    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)), isEditable, UIPasteboard.general.hasImages { return true }
        return super.canPerformAction(action, withSender: sender)
    }
}
#endif

#endif
