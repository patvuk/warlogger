import Foundation
import Security

enum Keychain {
    private static func query(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "warlogger", kSecAttrAccount as String: account]
    }

    static func save(_ account: String, _ secret: String) {
        SecItemDelete(query(account) as CFDictionary)
        var item = query(account)
        item[kSecValueData as String] = Data(secret.utf8)
        SecItemAdd(item as CFDictionary, nil)
    }

    static func load(_ account: String) -> String? {
        var item = query(account)
        item[kSecReturnData as String] = true
        var out: AnyObject?
        guard SecItemCopyMatching(item as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
