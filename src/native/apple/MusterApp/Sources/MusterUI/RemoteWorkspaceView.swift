#if os(macOS) || os(iOS)
import SwiftUI
import ImageIO
import MusterCore

struct RemoteWorkspaceView: View {
    let core: MusterCore
    let workspace: Muster_App_RemoteWorkspaceView

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
                RemoteSurface(frame: workspace.jpeg, mouse: workspace.mouse) { kind, x, y, scroll in
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
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func disconnect() { core.send(.endDesktopSession(workspace.selectedDevice)) }
    private func send(_ action: Muster_V1_DesktopSessionRequest.OneOf_Action) {
        core.send(.desktopSession(.with { $0.device = workspace.selectedDevice; $0.action = action }))
    }
}

private typealias PointerKind = Muster_V1_DesktopPointer.Kind
private typealias RemoteInput = (PointerKind, UInt32, UInt32, Int32) -> Void

private struct RemoteSurface: View {
    let frame: Data
    let mouse: Bool
    let input: RemoteInput
    @State private var image: CGImage?
    @State private var heldPoint: (UInt32, UInt32)?
    @State private var lastMove = Date.distantPast

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.black
                if let image {
                    let scale = min(geometry.size.width / CGFloat(image.width), geometry.size.height / CGFloat(image.height))
                    let size = CGSize(width: CGFloat(image.width) * scale, height: CGFloat(image.height) * scale)
                    Image(decorative: image, scale: 1)
                        .resizable()
                        .frame(width: size.width, height: size.height)
                        .overlay {
                            if mouse {
                                #if os(macOS)
                                MacPointer { kind, point, lines in forward(kind, point, size, lines) }
                                #else
                                TouchPointer { kind, point, lines in forward(kind, point, size, lines) }
                                #endif
                            }
                        }
                }
            }
        }
        .onDisappear {
            if let point = heldPoint {
                input(.dragEnd, point.0, point.1, 0)
                heldPoint = nil
            }
        }
        .onChange(of: frame, initial: true) { _, data in
            guard let source = CGImageSourceCreateWithData(data as CFData, nil),
                  CGImageSourceGetType(source) as String? == "public.jpeg",
                  let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [String: Any],
                  let width = props[kCGImagePropertyPixelWidth as String] as? Int,
                  let height = props[kCGImagePropertyPixelHeight as String] as? Int,
                  (1...1280).contains(width), (1...1280).contains(height) else { image = nil; return }
            image = CGImageSourceCreateImageAtIndex(source, 0, nil)
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
private struct TouchPointer: UIViewRepresentable {
    let action: (PointerKind, CGPoint, Int32) -> Void
    func makeUIView(context: Context) -> Surface { Surface() }
    func updateUIView(_ view: Surface, context: Context) { view.action = action }
    final class Surface: UIView {
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
