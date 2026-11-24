#if os(macOS) || os(iOS)
import SwiftUI
import AVFoundation
import MusterCore

struct RemoteWorkspaceView: View {
    let core: MusterCore
    let workspace: Muster_App_RemoteWorkspaceView

    @State private var showingSettings = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Picker("Device", selection: Binding(get: { workspace.selectedDevice }, set: { device in
                core.send(.openRemote(.with { $0.deviceID = device }))
            })) {
                Text(workspace.devices.isEmpty ? "No paired desktops" : "Select a desktop").tag("")
                ForEach(workspace.devices, id: \.deviceID) { device in
                    Text(device.connected ? device.name : "\(device.name) · offline").tag(device.deviceID)
                }
            }
            .disabled(workspace.sessionOpen || workspace.devices.isEmpty)
            HStack {
                if workspace.sessionOpen {
                    Button("Disconnect", action: disconnect)
                } else {
                    Button("Stream settings") { showingSettings = true }
                    Button("Connect") { send(.viewNative(1)) }.disabled(!workspace.canConnect)
                }
                Text(workspace.status).foregroundStyle(.secondary)
            }
            if workspace.hasTakeover {
                Text("\(workspace.takeover.controller) is using this desktop.")
                Button("Keep current session", action: disconnect)
                Button("Disconnect and connect") { send(.confirmTakeover(workspace.takeover.confirmation)) }
            }
            if workspace.sessionOpen && !workspace.hasTakeover {
                RemoteSurface(video: core.remoteVideo, session: workspace.nativeSession, mouse: workspace.mouse) { kind, x, y, scroll in
                    send(.pointer(.with {
                        $0.session = workspace.nativeSession
                        $0.kind = kind
                        $0.x = x
                        $0.y = y
                        $0.scroll = scroll
                    }))
                }.id(workspace.nativeSession)
            }
            Spacer(minLength: 0)
        }
        .sheet(isPresented: $showingSettings) {
            StreamSettings(draft: workspace.settings) { core.send(.setDesktopStreamSettings($0)) }
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func disconnect() { core.send(.endDesktopSession(workspace.selectedDevice)) }
    private func send(_ action: Muster_V1_DesktopSessionRequest.OneOf_Action) {
        core.send(.desktopSession(.with { $0.device = workspace.selectedDevice; $0.action = action }))
    }
}

private struct StreamSettings: View {
    @State var draft: Muster_V1_DesktopStreamSettings
    let save: (Muster_V1_DesktopStreamSettings) -> Void
    @Environment(\.dismiss) private var dismiss

    init(draft: Muster_V1_DesktopStreamSettings, save: @escaping (Muster_V1_DesktopStreamSettings) -> Void) {
        _draft = State(initialValue: draft)
        self.save = save
    }

    var body: some View {
        NavigationStack {
            Form {
                picker("Longest edge", $draft.longEdge, [640, 1280, 1920, 2560]) { "\($0) px" }
                picker("Frame rate limit", $draft.fps, [5, 10, 15, 30, 60]) { "\($0) fps" }
                picker("Bitrate limit", $draft.bitrateKbps, [0, 1000, 3000, 5000, 10000, 20000, 50000]) {
                    $0 == 0 ? "Unlimited" : "\($0 / 1000) Mbps"
                }
                picker("Stream quality", $draft.jpegQuality, [45, 65, 85]) { "\($0)%" }
                Toggle("Show host cursor", isOn: $draft.showCursor)
            }
            .formStyle(.grouped)
            .navigationTitle("Stream settings")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save(draft); dismiss() }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 420, minHeight: 320)
        #endif
    }

    private func picker(_ title: String, _ selection: Binding<UInt32>, _ options: [UInt32],
                        label: @escaping (UInt32) -> String) -> some View {
        Picker(title, selection: selection) {
            ForEach(options, id: \.self) { value in Text(label(value)).tag(value) }
        }
    }
}

typealias PointerKind = Muster_V1_DesktopPointer.Kind
private typealias RemoteInput = (PointerKind, UInt32, UInt32, Int32) -> Void

private struct RemoteSurface: View {
    @ObservedObject var video: RemoteVideo
    let session: UInt64
    let mouse: Bool
    let input: RemoteInput
    @State private var heldPoint: (UInt32, UInt32)?
    @State private var lastMove = Date.distantPast

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.black
                if let surface = video.surface, surface.session == session {
                    #if os(iOS)
                    MobileRemoteViewport(surface: surface, mouse: mouse) { kind, pixel, lines in
                        forward(kind, pixel, surface.size, lines)
                    }
                    #else
                    let scale = min(geometry.size.width / surface.size.width, geometry.size.height / surface.size.height)
                    let size = CGSize(width: surface.size.width * scale, height: surface.size.height * scale)
                    VideoLayerView(layer: surface.layer)
                        .frame(width: size.width, height: size.height)
                        .overlay {
                            if mouse {
                                MacPointer { kind, point, lines in forward(kind, point, size, lines) }
                            }
                        }
                    #endif
                }
            }
        }
        .onDisappear {
            if let point = heldPoint {
                input(.dragEnd, point.0, point.1, 0)
                heldPoint = nil
            }
        }
    }

    private func forward(_ kind: PointerKind, _ p: CGPoint, _ size: CGSize, _ lines: Int32 = 0) {
        guard size.width > 1, size.height > 1 else { return }
        if kind == .dragMove || kind == .move {
            let now = Date()
            guard now.timeIntervalSince(lastMove) >= 0.05 else { return }
            lastMove = now
        }
        let x = UInt32(min(1, max(0, p.x / (size.width-1))) * 65535)
        let y = UInt32(min(1, max(0, p.y / (size.height-1))) * 65535)
        if kind == .dragStart || kind == .dragMove { heldPoint = (x, y) }
        if kind == .dragEnd {
            guard heldPoint != nil else { return }
            heldPoint = nil
        }
        input(kind, x, y, lines)
    }
}

#if os(macOS)
private struct VideoLayerView: NSViewRepresentable {
    let layer: AVSampleBufferDisplayLayer
    func makeNSView(context: Context) -> NSView { NSView() }
    func updateNSView(_ view: NSView, context: Context) {
        if view.layer !== layer { view.layer = layer; view.wantsLayer = true }
    }
}
private struct MacPointer: NSViewRepresentable {
    let action: (PointerKind, CGPoint, Int32) -> Void
    func makeNSView(context: Context) -> Surface { Surface() }
    func updateNSView(_ view: Surface, context: Context) { view.action = action }
    final class Surface: NSView {
        var action: ((PointerKind, CGPoint, Int32) -> Void)?
        override var isFlipped: Bool { true }
        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            trackingAreas.forEach(removeTrackingArea)
            addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved,.activeInKeyWindow,.inVisibleRect], owner: self, userInfo: nil))
        }
        func send(_ kind: PointerKind, _ e: NSEvent, _ lines: Int32 = 0) {
            action?(kind, convert(e.locationInWindow, from: nil), lines)
        }
        override func mouseMoved(with e: NSEvent) { send(.move,e) }
        override func mouseDown(with e: NSEvent) { send(.dragStart,e) }
        override func mouseDragged(with e: NSEvent) { send(.dragMove,e) }
        override func mouseUp(with e: NSEvent) { send(.dragEnd,e) }
        override func rightMouseDown(with e: NSEvent) { send(.secondary,e) }
        override func scrollWheel(with e: NSEvent) {
            let delta = e.scrollingDeltaY
            if delta != 0 { send(.scroll,e,delta > 0 ? 1 : -1) }
        }
    }
}
#elseif os(iOS)
struct MobileRemoteViewport: UIViewRepresentable {
    let surface: RemoteVideo.Surface
    let mouse: Bool
    let action: (PointerKind, CGPoint, Int32) -> Void

    func makeUIView(context: Context) -> Viewport { Viewport() }
    func updateUIView(_ view: Viewport, context: Context) {
        view.show(surface.layer, pixels: surface.size, mouse: mouse, action: action)
    }

    final class Viewport: UIScrollView {
        private let canvas = TouchCanvas()
        private var videoLayer: AVSampleBufferDisplayLayer?
        private var pixels = CGSize.zero
        private var pixelScale: CGFloat { window?.windowScene?.screen.nativeScale ?? traitCollection.displayScale }

        override init(frame: CGRect) {
            super.init(frame: frame)
            clipsToBounds = true
            backgroundColor = .black
            contentInsetAdjustmentBehavior = .never
            bounces = false
            delaysContentTouches = false
            showsHorizontalScrollIndicator = false
            showsVerticalScrollIndicator = false
            panGestureRecognizer.minimumNumberOfTouches = 3
            panGestureRecognizer.maximumNumberOfTouches = 3
            addSubview(canvas)
        }
        required init?(coder: NSCoder) { nil }

        func show(_ layer: AVSampleBufferDisplayLayer, pixels newSize: CGSize, mouse: Bool,
                  action: @escaping (PointerKind, CGPoint, Int32) -> Void) {
            if videoLayer !== layer {
                videoLayer?.removeFromSuperlayer()
                canvas.layer.addSublayer(layer)
                videoLayer = layer
                setNeedsLayout()
            }
            if pixels != newSize {
                pixels = newSize
                setNeedsLayout()
            }
            canvas.isUserInteractionEnabled = mouse
            canvas.action = { [weak self] kind, point, lines in
                guard let self else { return }
                let scale = self.pixelScale
                action(kind, CGPoint(x: floor(point.x * scale), y: floor(point.y * scale)), lines)
            }
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            let scale = pixelScale
            guard scale > 0, pixels.width > 0, pixels.height > 0 else { return }
            let size = CGSize(width: pixels.width / scale, height: pixels.height / scale)
            let content = CGSize(width: max(bounds.width, size.width), height: max(bounds.height, size.height))
            if contentSize != content { contentSize = content }
            let origin = CGPoint(x: floor((content.width - size.width) * scale / 2) / scale,
                                 y: floor((content.height - size.height) * scale / 2) / scale)
            canvas.frame = CGRect(origin: origin, size: size)
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            videoLayer?.frame = canvas.bounds
            CATransaction.commit()
        }
    }

    final class TouchCanvas: UIView {
        var action: ((PointerKind, CGPoint, Int32) -> Void)?
        override init(frame: CGRect) {
            super.init(frame: frame)
            let tap = UITapGestureRecognizer(target: self, action: #selector(tap(_:)))
            addGestureRecognizer(tap)
            let secondary = UITapGestureRecognizer(target: self, action: #selector(secondary(_:)))
            secondary.numberOfTouchesRequired = 2
            addGestureRecognizer(secondary)
            let drag = UIPanGestureRecognizer(target: self, action: #selector(drag(_:)))
            drag.maximumNumberOfTouches = 1
            addGestureRecognizer(drag)
            let scroll = UIPanGestureRecognizer(target: self, action: #selector(scroll(_:)))
            scroll.minimumNumberOfTouches = 2
            scroll.maximumNumberOfTouches = 2
            addGestureRecognizer(scroll)
        }
        required init?(coder: NSCoder) { nil }
        @objc func tap(_ g: UITapGestureRecognizer) { action?(.primary, g.location(in: self), 0) }
        @objc func secondary(_ g: UITapGestureRecognizer) { action?(.secondary, g.location(in: self), 0) }
        @objc func drag(_ g: UIPanGestureRecognizer) {
            let point = g.location(in: self)
            switch g.state {
            case .began:
                let delta = g.translation(in: self)
                action?(.dragStart, CGPoint(x: point.x-delta.x, y: point.y-delta.y), 0)
            case .changed: action?(.dragMove, point, 0)
            case .ended, .cancelled, .failed: action?(.dragEnd, point, 0)
            default: break
            }
        }
        @objc func scroll(_ g: UIPanGestureRecognizer) {
            let distance = g.translation(in: self).y
            if abs(distance) >= 12 {
                action?(.scroll, g.location(in: self), distance > 0 ? 1 : -1)
                g.setTranslation(.zero, in: self)
            }
        }
    }
}
#endif
#endif
