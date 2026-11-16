import Foundation
import Security

/// The grant holds the device key password and, on the phone that created the
/// repository, the recovery password. It lives in the Keychain, readable only while
/// the phone is unlocked and never copied to another device or to iCloud. The
/// library receives the JSON as text, so no plaintext file is written.
///
/// The item sits in a Keychain group the app shares with its Files extension, so
/// the extension reads the same grant without a copy. An item an older build wrote
/// in the app's own group moves to the shared group on the first read.
enum GrantVault {
    private static let service = "io.github.lucaswilliameufrasio.satchel"
    static let main = "grant"
    /// The new grant while the recovery password changes. See `PhoneModel.changeRecovery`.
    static let pending = "grant-pending"

    /// `$(AppIdentifierPrefix)io.github.lucaswilliameufrasio.satchel.shared`, from Info.plist.
    static var sharedGroup: String? {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "SatchelKeychainGroup") as? String,
              !group.isEmpty, !group.hasPrefix("$(")
        else {
            return nil
        }
        return group
    }

    private static func match(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    static func read(_ account: String = main) -> String? {
        var query = match(account)
        query[kSecReturnData as String] = true
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess,
              let attributes = found as? [String: Any],
              let data = attributes[kSecValueData as String] as? Data,
              let text = String(data: data, encoding: .utf8)
        else {
            return nil
        }
        if let group = sharedGroup, attributes[kSecAttrAccessGroup as String] as? String != group {
            write(text, account)
        }
        return text
    }

    @discardableResult
    static func write(_ text: String, _ account: String = main) -> Bool {
        SecItemDelete(match(account) as CFDictionary)
        var item = match(account)
        item[kSecValueData as String] = Data(text.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        if let group = sharedGroup {
            item[kSecAttrAccessGroup as String] = group
        }
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func clear(_ account: String = main) {
        SecItemDelete(match(account) as CFDictionary)
    }
}
