// The keys only the platform can hold (ARCHITECTURE §3.4): the device key,
// usable while the device is locked, and the presence key, usable only
// after local authentication. Both live in the Secure Enclave; the blobs
// kept in the Keychain are useless off this device. Nothing here prompts
// at launch: the device key needs no user presence, and the presence key
// asks for Touch ID / Face ID only when it signs (the core requests that
// for high-risk actions on another device: approving, removing,
// re-scoping members, changing its policy).
import Foundation
#if canImport(CryptoKit) && canImport(LocalAuthentication)
import CryptoKit
import LocalAuthentication
import Security
#endif

public enum MusterKeyError: Error, CustomStringConvertible {
    case unavailable(String)
    case keychain(Int32)
    case cancelled

    public var description: String {
        switch self {
        case .unavailable(let what): return what
        case .keychain(let status): return "keychain error \(status)"
        case .cancelled: return "authentication cancelled"
        }
    }
}

/// Where the two keys come from. The core sees only their public points and
/// signatures.
public protocol MusterKeys: AnyObject {
    var devicePublicKey: Data? { get }
    func deviceSign(_ message: Data) -> Data?
    var presencePublicKey: Data? { get }
    /// Signs after local authentication; the reason is what the prompt says.
    func presenceSign(_ message: Data, reason: String) async throws -> Data
}

/// No hardware keys: the core keeps a software device key (development,
/// Linux tests) and presence proofs are impossible.
public final class SoftwareKeys: MusterKeys {
    public init() {}
    public var devicePublicKey: Data? { nil }
    public func deviceSign(_ message: Data) -> Data? { nil }
    public var presencePublicKey: Data? { nil }
    public func presenceSign(_ message: Data, reason: String) async throws -> Data {
        throw MusterKeyError.unavailable("no presence key on this platform")
    }
}

#if canImport(CryptoKit) && canImport(LocalAuthentication)
/// Secure Enclave keys, blobs in the Keychain under `service`.
public final class SecureEnclaveKeys: MusterKeys {
    private let service: String
    private let lock = NSLock()
    private var device: SecureEnclave.P256.Signing.PrivateKey?
    private var presenceBlob: Data?

    private static let deviceAccount = "device-key"
    private static let presenceAccount = "presence-key"

    /// Loads or creates both keys. Throws when the Secure Enclave is missing.
    public init(service: String = "dev.linsheng.muster") throws {
        self.service = service
        guard SecureEnclave.isAvailable else {
            throw MusterKeyError.unavailable("Secure Enclave not available")
        }
        if let blob = try Self.read(service: service, account: Self.deviceAccount) {
            device = try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: blob)
        } else {
            // After first unlock, no user presence: the link must come up
            // while the phone is locked in a pocket.
            let access = try Self.accessControl([.privateKeyUsage])
            let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
            try Self.write(key.dataRepresentation, service: service, account: Self.deviceAccount)
            device = key
        }
        if let blob = try Self.read(service: service, account: Self.presenceAccount) {
            presenceBlob = blob
        } else {
            // Biometrics or passcode every time it signs: that is the proof.
            // A device without a passcode (or the simulator) cannot hold
            // such a key; the device key still works and presence proofs
            // are refused until one exists.
            do {
                let access = try Self.accessControl([.privateKeyUsage, .userPresence])
                let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
                try Self.write(key.dataRepresentation, service: service, account: Self.presenceAccount)
                presenceBlob = key.dataRepresentation
            } catch {
                NSLog("muster: no presence key on this device: %@", "\(error)")
                presenceBlob = nil
            }
        }
    }

    public var devicePublicKey: Data? {
        device?.publicKey.x963Representation
    }

    public func deviceSign(_ message: Data) -> Data? {
        lock.lock(); defer { lock.unlock() }
        // CryptoKit hashes with SHA-256 itself; the core hands over the
        // unhashed message, which is what ECDSA-P256-SHA256 expects.
        return try? device?.signature(for: message).derRepresentation
    }

    public var presencePublicKey: Data? {
        guard let blob = presenceBlob,
              let key = try? SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: blob)
        else { return nil }
        return key.publicKey.x963Representation
    }

    public func presenceSign(_ message: Data, reason: String) async throws -> Data {
        guard let blob = presenceBlob else {
            throw MusterKeyError.unavailable("no presence key")
        }
        let context = LAContext()
        context.localizedReason = reason
        // The Secure Enclave runs the prompt when the key is used; evaluating
        // first gives a clear cancel path and a reason string on every OS.
        do {
            try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
        } catch {
            throw MusterKeyError.cancelled
        }
        let key = try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: blob, authenticationContext: context)
        return try key.signature(for: message).derRepresentation
    }

    // MARK: Keychain

    private static func accessControl(_ flags: SecAccessControlCreateFlags) throws -> SecAccessControl {
        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            kCFAllocatorDefault, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, flags, &error
        ) else {
            throw MusterKeyError.unavailable("access control: \(String(describing: error?.takeRetainedValue()))")
        }
        return access
    }

    /// The data-protection keychain on every platform: items belong to the
    /// app by its team and bundle id, so a rebuilt or re-signed Muster.app
    /// reads them without the login keychain's password dialog. (An
    /// unsigned development build has no application identifier and gets
    /// errSecMissingEntitlement here; the app then runs with software keys.)
    private static func query(service: String, account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecUseDataProtectionKeychain as String: true,
        ]
    }

    private static func read(service: String, account: String) throws -> Data? {
        var query = query(service: service, account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess: return item as? Data
        case errSecItemNotFound: return nil
        default: throw MusterKeyError.keychain(status)
        }
    }

    private static func write(_ data: Data, service: String, account: String) throws {
        var attributes = query(service: service, account: account)
        attributes[kSecValueData as String] = data
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(attributes as CFDictionary, nil)
        guard status == errSecSuccess else { throw MusterKeyError.keychain(status) }
    }
}
#endif
