// The C ABI as a Swift object: bytes in, bytes out, callbacks routed back
// to whoever owns the runtime. Everything typed sits on top (MusterCore.swift).
import CMuster
import Foundation

/// What the platform provides to the core: the device key (or nothing, for a
/// software key in development), the presence key's public half, and where
/// view models and effects go. Callbacks arrive on the core's threads.
public protocol MusterShell: AnyObject {
    func onView(_ bytes: Data)
    func onEffect(id: UInt64, bytes: Data)
    /// 65-byte X9.63 point of the device key, or nil to let the core keep a
    /// software key (development only).
    func hardwarePublicKey() -> Data?
    /// ECDSA-P256-SHA256 over `message` with the device key, DER. Synchronous.
    func hardwareSign(_ message: Data) -> Data?
    /// 65-byte point of the presence key, or nil when the platform has none.
    func presencePublicKey() -> Data?
}

public enum MusterRuntimeError: Error, CustomStringConvertible {
    case create(String)

    public var description: String {
        switch self {
        case .create(let message): return message
        }
    }
}

/// One running core. Thread-safe: every method may be called from any thread.
public final class MusterRuntime {
    private var core: OpaquePointer?
    private let shell: MusterShell
    private var platform: MusterPlatform

    public static var version: String {
        String(cString: muster_version())
    }

    /// Logging to stderr: 0 off, 1 error, 2 warn, 3 info, 4 debug.
    public static func desktopUpdateStartup() { muster_desktop_update_startup() }

    public static func prepareDesktop(socket: String, managed: Bool,
                                      action: @convention(c) (UInt32) -> Int32) throws {
        var error: UnsafeMutablePointer<CChar>?
        let result = socket.withCString { muster_desktop_prepare($0, managed ? 1 : 0, action, &error) }
        defer { muster_string_free(error) }
        guard result == 0 else {
            throw MusterRuntimeError.create(error.map { String(cString: $0) } ?? "Desktop service preparation failed")
        }
    }

    public static func initLogging(level: UInt32) {
        muster_log_init(level)
    }

    public init(configJSON: Data, shell: MusterShell) throws {
        self.shell = shell
        // A strong reference lives for as long as the core can call back.
        var platform = MusterPlatform()
        self.platform = platform
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        platform.ctx = ctx
        platform.on_view = { ctx, bytes, len in
            guard let ctx, let bytes else { return }
            let runtime = Unmanaged<MusterRuntime>.fromOpaque(ctx).takeUnretainedValue()
            runtime.shell.onView(Data(bytes: bytes, count: len))
        }
        platform.on_effect = { ctx, id, bytes, len in
            guard let ctx, let bytes else { return }
            let runtime = Unmanaged<MusterRuntime>.fromOpaque(ctx).takeUnretainedValue()
            runtime.shell.onEffect(id: id, bytes: Data(bytes: bytes, count: len))
        }
        platform.hw_public_key = { ctx, out, cap in
            guard let ctx else { return -1 }
            let runtime = Unmanaged<MusterRuntime>.fromOpaque(ctx).takeUnretainedValue()
            return MusterRuntime.write(runtime.shell.hardwarePublicKey(), to: out, capacity: cap)
        }
        platform.hw_sign = { ctx, msg, msgLen, out, cap in
            guard let ctx, let msg else { return -1 }
            let runtime = Unmanaged<MusterRuntime>.fromOpaque(ctx).takeUnretainedValue()
            let signature = runtime.shell.hardwareSign(Data(bytes: msg, count: msgLen))
            return MusterRuntime.write(signature, to: out, capacity: cap)
        }
        platform.presence_public_key = { ctx, out, cap in
            guard let ctx else { return -1 }
            let runtime = Unmanaged<MusterRuntime>.fromOpaque(ctx).takeUnretainedValue()
            return MusterRuntime.write(runtime.shell.presencePublicKey(), to: out, capacity: cap)
        }
        self.platform = platform

        var error: UnsafeMutablePointer<CChar>?
        let created: OpaquePointer? = configJSON.withUnsafeBytes { buffer in
            withUnsafePointer(to: &self.platform) { platformPointer in
                muster_core_new(
                    buffer.bindMemory(to: UInt8.self).baseAddress,
                    buffer.count,
                    platformPointer,
                    &error
                )
            }
        }
        guard let created else {
            let message = error.map { String(cString: $0) } ?? "core refused to start"
            muster_string_free(error)
            throw MusterRuntimeError.create(message)
        }
        core = created
    }

    deinit {
        shutdown()
    }

    /// Stops the core. Safe to call more than once.
    public func shutdown() {
        guard let core else { return }
        self.core = nil
        muster_core_free(core)
    }

    /// An encoded `muster.app.Intent`.
    public func send(intentBytes: Data) {
        guard let core else { return }
        intentBytes.withUnsafeBytes { buffer in
            muster_core_intent(core, buffer.bindMemory(to: UInt8.self).baseAddress, buffer.count)
        }
    }

    /// An encoded `muster.app.EffectResult` answering effect `id`.
    public func send(effectID: UInt64, resultBytes: Data) {
        guard let core else { return }
        resultBytes.withUnsafeBytes { buffer in
            muster_core_effect_result(core, effectID, buffer.bindMemory(to: UInt8.self).baseAddress, buffer.count)
        }
    }

    /// `muster.app.Lifecycle` as its raw value.
    public func lifecycle(_ event: UInt32) {
        guard let core else { return }
        muster_core_lifecycle(core, event)
    }

    /// This device's key as an OpenSSH public key line, or nil.
    public func sshPublicKeyLine() -> String? {
        guard let core, let line = muster_core_ssh_public_key(core) else { return nil }
        defer { muster_string_free(line) }
        return String(cString: line)
    }

    private static func write(_ data: Data?, to out: UnsafeMutablePointer<UInt8>?, capacity: Int) -> Int {
        guard let data, let out, data.count <= capacity else { return -1 }
        data.copyBytes(to: out, count: data.count)
        return data.count
    }
}
