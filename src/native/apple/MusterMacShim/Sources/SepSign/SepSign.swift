// Secure Enclave signing for musterd (muster-platform, `sep` feature). Same key
// shape and access control as the P0 hwquic POC it came from: a P-256 key
// usable after first unlock without user presence, so a LaunchAgent signs
// while the screen is locked. Buffers handed to Rust are malloc'd and freed
// with sepsign_free / sepsign_free_str.
import CryptoKit
import Foundation
import Security

private let lock = NSLock()
private var cached: (blob: Data, key: SecureEnclave.P256.Signing.PrivateKey)?

private func fail(_ err: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>, _ message: String) -> Int32 {
    err.pointee = strdup(message)
    return 1
}

private func emit(_ data: Data, _ out: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>, _ len: UnsafeMutablePointer<Int>) {
    let buffer = malloc(max(data.count, 1)).assumingMemoryBound(to: UInt8.self)
    data.copyBytes(to: buffer, count: data.count)
    out.pointee = buffer
    len.pointee = data.count
}

private func accessControl() throws -> SecAccessControl {
    var cfError: Unmanaged<CFError>?
    guard let ac = SecAccessControlCreateWithFlags(
        kCFAllocatorDefault,
        kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        [.privateKeyUsage],
        &cfError
    ) else {
        throw NSError(domain: "sepsign", code: 1, userInfo: [NSLocalizedDescriptionKey: String(describing: cfError?.takeRetainedValue())])
    }
    return ac
}

private func open(_ blob: Data) throws -> SecureEnclave.P256.Signing.PrivateKey {
    lock.lock(); defer { lock.unlock() }
    if let cached, cached.blob == blob { return cached.key }
    let key = try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: blob)
    cached = (blob, key)
    return key
}

@_cdecl("sepsign_create")
public func sepsign_create(
    _ out: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
    _ len: UnsafeMutablePointer<Int>,
    _ err: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
    guard SecureEnclave.isAvailable else { return fail(err, "Secure Enclave not available") }
    do {
        let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: accessControl())
        emit(key.dataRepresentation, out, len)
        return 0
    } catch {
        return fail(err, "create: \(error)")
    }
}

@_cdecl("sepsign_public_key_raw")
public func sepsign_public_key_raw(
    _ blob: UnsafePointer<UInt8>, _ blobLen: Int,
    _ out: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>, _ len: UnsafeMutablePointer<Int>,
    _ err: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
    do {
        let key = try open(Data(bytes: blob, count: blobLen))
        emit(key.publicKey.x963Representation, out, len)
        return 0
    } catch {
        return fail(err, "open: \(error)")
    }
}

@_cdecl("sepsign_sign")
public func sepsign_sign(
    _ blob: UnsafePointer<UInt8>, _ blobLen: Int,
    _ msg: UnsafePointer<UInt8>, _ msgLen: Int,
    _ out: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>, _ len: UnsafeMutablePointer<Int>,
    _ err: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
    do {
        let key = try open(Data(bytes: blob, count: blobLen))
        // CryptoKit hashes with SHA-256 internally; rustls hands us the
        // unhashed TLS 1.3 transcript message, which is exactly right.
        let signature = try key.signature(for: Data(bytes: msg, count: msgLen))
        emit(signature.derRepresentation, out, len)
        return 0
    } catch {
        return fail(err, "sign: \(error)")
    }
}

@_cdecl("sepsign_free")
public func sepsign_free(_ p: UnsafeMutablePointer<UInt8>?) { free(p) }

@_cdecl("sepsign_free_str")
public func sepsign_free_str(_ p: UnsafeMutablePointer<CChar>?) { free(p) }
