// The one object the views observe: the latest ViewModel, and the effects
// the platform is in the middle of (a scan, a local authentication). It
// performs effects and forwards intents; it holds no state of its own
// beyond what an effect needs to finish.
#if canImport(SwiftUI)
import Foundation
import MusterCore
import SwiftUI
#if canImport(UserNotifications)
import UserNotifications
#endif
#if canImport(LocalAuthentication)
import LocalAuthentication
#endif
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
import IOKit.pwr_mgt
#endif

@MainActor
public final class MusterModel: ObservableObject {
    @Published public private(set) var vm = ViewModel()
    @Published public private(set) var started = false
    @Published public var startError: String?
    /// An effect waiting for the camera (iOS) or a pasted invite (macOS).
    @Published public var scanRequest: UInt64?
    /// Local authentication in progress: the reason shown to the user.
    @Published public private(set) var authenticating: String?

    public private(set) var core: MusterCore?
    public let keys: MusterKeys
    private var config: MusterConfig?
    private var preparation: (@Sendable () async throws -> Void)?
    private var startupTask: Task<Void, Never>?

    public init(keys: MusterKeys? = nil) {
        if let keys {
            self.keys = keys
        } else {
            #if canImport(CryptoKit) && canImport(LocalAuthentication)
            do {
                self.keys = try SecureEnclaveKeys()
            } catch {
                NSLog("muster: Secure Enclave keys unavailable, using a software key: %@", "\(error)")
                self.keys = SoftwareKeys()
            }
            #else
            self.keys = SoftwareKeys()
            #endif
        }
    }

    /// Starts the core once; errors land in `startError`.
    public func start(config: MusterConfig, prepare: (@Sendable () async throws -> Void)? = nil) {
        guard core == nil, startupTask == nil else { return }
        self.config = config
        preparation = prepare
        startError = nil
        guard let prepare else { createCore(config: config); return }
        startupTask = Task { [weak self] in
            defer { self?.startupTask = nil }
            do {
                try await prepare()
                try Task.checkCancellation()
                self?.createCore(config: config)
            } catch {
                self?.startError = "\(error)"
            }
        }
    }

    private func createCore(config: MusterConfig) {
        #if os(macOS)
        PrivacyCover.shared.onHotKey = { [weak self] in
            guard let self, let core = self.core else { return }
            core.setCover(!self.vm.self_p.state.covered)
        }
        #endif
        do {
            core = try MusterCore(
                config: config,
                keys: keys,
                onViewModel: { [weak self] vm in
                    Task { @MainActor [weak self] in self?.vm = vm }
                },
                onEffect: { [weak self] id, effect in
                    Task { @MainActor [weak self] in self?.perform(id: id, effect: effect) }
                }
            )
            started = true
        } catch {
            startError = "\(error)"
        }
    }

    /// Tries again with the same configuration (a desktop whose musterd was
    /// not running yet).
    public func retry() {
        guard let config, core == nil else { return }
        start(config: config, prepare: preparation)
    }

    public func lifecycle(_ phase: ScenePhase) {
        switch phase {
        case .active: core?.lifecycle(.foreground)
        case .background: core?.lifecycle(.background)
        default: break
        }
    }

    public func networkChanged() {
        core?.lifecycle(.networkChanged)
    }

    /// The scanner or paste sheet finished.
    public func scanned(_ text: String?) {
        guard let id = scanRequest else { return }
        scanRequest = nil
        if let text, !text.isEmpty {
            core?.answer(effect: id, ok: true, text: text)
        } else {
            core?.answer(effect: id, ok: false)
        }
    }

    // MARK: Effects

    private func perform(id: UInt64, effect: Effect) {
        guard let core else { return }
        switch effect.ef {
        case .sendDiagnostic:
            break // Stored by the core adapter before UI dispatch.
        case .activateNetwork(let request):
            #if os(iOS)
            NetworkActivation.start(address: request.address, port: request.port, timeoutMS: request.timeoutMs) { ok, error in
                core.answer(effect: id, ok: ok, error: error)
            }
            #else
            core.answer(effect: id, ok: false, error: "No network activation adapter on this platform")
            #endif
        case .scanQr:
            #if os(macOS)
            // A Mac has no camera flow: an invite on the pasteboard is taken
            // at once, otherwise the paste sheet asks for one.
            if let text = NSPasteboard.general.string(forType: .string)?
                .trimmingCharacters(in: .whitespacesAndNewlines),
               text.hasPrefix("muster1:")
            {
                core.answer(effect: id, ok: true, text: text)
                return
            }
            #endif
            scanRequest = id
        case .authenticate(let request):
            authenticating = request.reason
            Task {
                defer { self.authenticating = nil }
                do {
                    let signature = try await keys.presenceSign(request.message, reason: request.reason)
                    core.answer(effect: id, ok: true, data: signature)
                } catch {
                    core.answer(effect: id, ok: false, error: "\(error)")
                }
            }
        case .notify(let notify):
            Notifier.post(title: notify.title, body: notify.body, thread: notify.session)
            core.answer(effect: id, ok: true)
        case .cover(let cover):
            #if os(macOS)
            PrivacyCover.shared.set(cover.on)
            core.answer(effect: id, ok: true)
            #else
            core.answer(effect: id, ok: false, error: "no cover on this platform")
            #endif
        case .localAuth(let auth):
            Task {
                do {
                    try await LocalAuth.evaluate(reason: auth.reason)
                    core.answer(effect: id, ok: true)
                } catch {
                    core.answer(effect: id, ok: false, error: "\(error)")
                }
            }
        case .prepareDesktopUpdate:
            #if os(macOS)
            Task {
                do {
                    try await DesktopUpdateLifecycle.prepare()
                    core.answer(effect: id, ok: true)
                } catch { core.answer(effect: id, ok: false, error: error.localizedDescription) }
            }
            #else
            core.answer(effect: id, ok: false, error: "Requires desktop update adapter")
            #endif
        case .finishDesktopUpdate(let finish):
            #if os(macOS)
            if finish.exit {
                core.answer(effect: id, ok: true)
                NSApplication.shared.terminate(nil)
            } else {
                DesktopUpdateLifecycle.restoreService()
                core.answer(effect: id, ok: true)
            }
            #else
            core.answer(effect: id, ok: false, error: "Requires desktop update adapter")
            #endif
        case .restartDesktop:
            #if os(macOS)
            do {
                try DesktopRestart.schedule()
                core.answer(effect: id, ok: true)
                NSApplication.shared.terminate(nil)
            } catch {
                core.answer(effect: id, ok: false, error: error.localizedDescription)
            }
            #else
            core.answer(effect: id, ok: false, error: "Requires desktop restart adapter")
            #endif
        case .authorizeDesktopHelper:
            #if os(macOS)
            do {
                try DesktopHelper.register(openSettings: true)
                core.answer(effect: id, ok: true)
            } catch {
                core.answer(effect: id, ok: false, error: error.localizedDescription)
            }
            #else
            core.answer(effect: id, ok: false, error: "Requires macOS")
            #endif
        case .openURL(let open):
            if let url = URL(string: open.url) {
                #if os(iOS)
                UIApplication.shared.open(url)
                #elseif os(macOS)
                NSWorkspace.shared.open(url)
                #endif
                core.answer(effect: id, ok: true)
            } else {
                core.answer(effect: id, ok: false, error: "bad url")
            }
        case .none:
            core.answer(effect: id, ok: false, error: "unknown effect")
        }
    }
}

// MARK: - Platform pieces the effects use

enum Notifier {
    static func post(title: String, body: String, thread: String) {
        #if canImport(UserNotifications)
        let center = UNUserNotificationCenter.current()
        center.requestAuthorization(options: [.alert, .sound]) { granted, _ in
            guard granted else { return }
            let content = UNMutableNotificationContent()
            content.title = title
            content.body = body
            content.threadIdentifier = thread
            content.sound = .default
            center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }
        #endif
    }
}

/// Holds the Mac awake while the privacy cover is up; released on deinit.
final class KeepAwake {
    #if os(macOS)
    private var assertion: IOPMAssertionID = 0
    private var system: IOPMAssertionID = 0
    /// `system`: also keep the machine from idle-sleeping (the privacy
    /// cover, where nobody is at the keyboard for a long time).
    init(system: Bool = false) {
        IOPMAssertionCreateWithName(
            kIOPMAssertionTypeNoDisplaySleep as CFString,
            IOPMAssertionLevel(kIOPMAssertionLevelOn),
            "Muster" as CFString,
            &assertion
        )
        if system {
            IOPMAssertionCreateWithName(
                kIOPMAssertionTypeNoIdleSleep as CFString,
                IOPMAssertionLevel(kIOPMAssertionLevelOn),
                "Muster privacy cover" as CFString,
                &self.system
            )
        }
    }
    deinit {
        IOPMAssertionRelease(assertion)
        if system != 0 { IOPMAssertionRelease(system) }
    }
    #else
    init() {}
    #endif
}

/// Touch ID / password through LocalAuthentication; the answer is yes or no.
enum LocalAuth {
    static func evaluate(reason: String) async throws {
        #if canImport(LocalAuthentication)
        let context = LAContext()
        context.localizedReason = reason
        try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
        #else
        throw MusterKeyError.unavailable("no local authentication on this platform")
        #endif
    }
}

#if os(macOS)
import Carbon.HIToolbox

/// The privacy cover (the old app's "locked use"): a black, non-activating
/// panel above everything on every display, the machine kept awake, and a
/// global ⌃⌥⌘L to raise it or ask to drop it. The session is not locked,
/// so the desktop apps stay reachable for remote control; only the daemon
/// decides whether the cover is up, this class draws what it is told.
@MainActor
final class PrivacyCover {
    static let shared = PrivacyCover()
    var onHotKey: (() -> Void)?
    private var panels: [CGDirectDisplayID: NSPanel] = [:]
    private var awake: KeepAwake?
    private var hotKey: EventHotKeyRef?
    private var handler: EventHandlerRef?
    private var screensObserver: Any?

    private init() {
        registerHotKey()
        screensObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { [weak self] _ in Task { @MainActor in self?.reconcile() } }
    }

    func set(_ on: Bool) {
        if on {
            if awake == nil { awake = KeepAwake(system: true) }
            reconcile()
            NSCursor.hide()
        } else {
            panels.values.forEach { $0.orderOut(nil) }
            panels.removeAll()
            awake = nil
            NSCursor.unhide()
        }
    }

    private func reconcile() {
        guard awake != nil else { return }
        let screens = Dictionary(uniqueKeysWithValues: NSScreen.screens.map { (displayID(of: $0), $0) })
        for id in panels.keys where screens[id] == nil {
            panels.removeValue(forKey: id)?.orderOut(nil)
        }
        for (id, screen) in screens {
            let panel = panels[id] ?? makePanel(for: screen)
            panels[id] = panel
            panel.setFrame(screen.frame, display: true)
            panel.orderFrontRegardless()
        }
    }

    private func makePanel(for screen: NSScreen) -> NSPanel {
        let panel = NSPanel(
            contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered, defer: false, screen: screen
        )
        panel.backgroundColor = .black
        panel.isOpaque = true
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        panel.level = NSWindow.Level(rawValue: Int(CGShieldingWindowLevel()) + 1)
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.sharingType = .none
        return panel
    }

    private func displayID(of screen: NSScreen) -> CGDirectDisplayID {
        (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value ?? 0
    }

    /// ⌃⌥⌘L, registered with Carbon: no permission needed, works while covered.
    private func registerHotKey() {
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let status = InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in
            Task { @MainActor in PrivacyCover.shared.onHotKey?() }
            return noErr
        }, 1, &spec, nil, &handler)
        guard status == noErr else { return }
        let id = EventHotKeyID(signature: OSType(0x4D555354), id: 1)  // "MUST"
        RegisterEventHotKey(UInt32(kVK_ANSI_L), UInt32(controlKey | optionKey | cmdKey), id, GetApplicationEventTarget(), 0, &hotKey)
    }
}
#endif

/// The device name and a state directory the platform provides.
public enum MusterPaths {
    public static var stateDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        let dir = base.appendingPathComponent("Muster", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    #if os(macOS)
    /// musterd's state directory on this Mac, the same the daemon computes
    /// (musterd::paths): $MUSTER_DIR, else ~/Library/Application Support/Muster.
    public static var daemonDirectory: URL {
        if let dir = ProcessInfo.processInfo.environment["MUSTER_DIR"], !dir.isEmpty {
            return URL(fileURLWithPath: dir, isDirectory: true)
        }
        return stateDirectory
    }

    /// musterd's socket on this Mac.
    public static var daemonSocket: URL {
        daemonDirectory.appendingPathComponent("musterd.sock")
    }

    #endif

    public static var deviceName: String {
        #if os(iOS)
        return UIDevice.current.name
        #elseif os(macOS)
        return Host.current().localizedName ?? "Mac"
        #else
        return "device"
        #endif
    }
}
#endif
