// Delete only legacy AgentHalo-namespaced items, never return secret data.
// Run as the login user after stopping the retired application.
import Foundation
import Security

// The classic macOS keychain can ignore the query's UI-fail flag for deletion.
// Disable interaction for this process as well; protected items stay untouched.
SecKeychainSetUserInteractionAllowed(false)

var removed = 0
var failures = 0
for itemClass in [kSecClassGenericPassword, kSecClassKey] {
    let query: [CFString: Any] = [kSecClass: itemClass,
        kSecMatchLimit: kSecMatchLimitAll, kSecReturnAttributes: true,
        kSecReturnPersistentRef: true, kSecUseAuthenticationUI: kSecUseAuthenticationUIFail]
    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    if status == errSecItemNotFound { continue }
    guard status == errSecSuccess, let items = result as? [[CFString: Any]] else {
        print("Legacy keychain inventory unavailable: \(status)")
        failures += 1
        continue
    }
    for item in items {
        let names = [kSecAttrService, kSecAttrLabel, kSecAttrApplicationTag].compactMap { key -> String? in
            if let text = item[key] as? String { return text }
            if let data = item[key] as? Data { return String(data: data, encoding: .utf8) }
            return nil
        }
        guard names.contains(where: { $0.hasPrefix("dev.linsheng.agenthalo.") }),
              let reference = item[kSecValuePersistentRef] else { continue }
        let delete: [CFString: Any] = [kSecValuePersistentRef: reference,
            kSecUseAuthenticationUI: kSecUseAuthenticationUIFail]
        let status = SecItemDelete(delete as CFDictionary)
        if status == errSecSuccess { removed += 1 }
        else if status != errSecItemNotFound { failures += 1; print("Legacy item deletion failed: \(status)") }
    }
}
print("Legacy AgentHalo keychain items removed=\(removed), failures=\(failures)")
exit(failures == 0 ? 0 : 1)
