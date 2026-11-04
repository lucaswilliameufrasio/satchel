import Combine
import Foundation
import Security

/// The grant holds the device key password and, on the phone that created the
/// repository, the recovery password. It lives in the Keychain, readable only while
/// the phone is unlocked and never copied to another device or to iCloud. The
/// library receives the JSON as text, so no plaintext file is written.
enum GrantVault {
    private static let service = "io.github.lucaswilliameufrasio.satchel"
    static let main = "grant"
    /// The new grant while the recovery password changes. See `PhoneModel.changeRecovery`.
    static let pending = "grant-pending"

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
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess,
              let data = found as? Data
        else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    static func write(_ text: String, _ account: String = main) -> Bool {
        SecItemDelete(match(account) as CFDictionary)
        var item = match(account)
        item[kSecValueData as String] = Data(text.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func clear(_ account: String = main) {
        SecItemDelete(match(account) as CFDictionary)
    }
}

@MainActor
final class PhoneModel: ObservableObject {
    enum Screen {
        case loading
        case failed
        case ready(rows: [CatalogRow], busy: Bool)
        case file(path: String, rows: [CatalogRow], busy: Bool)
        case pairing(
            address: String,
            code: String,
            fingerprint: String,
            name: String,
            busy: Bool,
            failed: Bool
        )
        case scanning(address: String, code: String, fingerprint: String, name: String)
        case creating(CreateDraft)
        case recovery(password: String, note: String?)
        case changingRecovery(busy: Bool, failed: Bool)
        case holding(HoldState)
    }

    struct HoldState: Equatable {
        var offer: HoldOffer
        var approval: HoldApproval?
        var qr: Data?
        var busy: Bool
        var failed: Bool
    }

    @Published var screen: Screen = .loading

    let supportURL: URL
    let sampleURL: URL
    /// Older installs wrote `grant.json` in clear. It is moved into the Keychain once.
    let legacyGrantURL: URL
    let deviceURL: URL
    let labelURL: URL
    let workURL: URL

    var canPair: Bool {
        GrantVault.read() == nil
    }

    /// True when this phone created the repository and can show an invite.
    var canInvite: Bool {
        !canPair && SatchelCore.isMaintainer(grant: GrantVault.read() ?? "")
    }

    /// Catalog id of this installation. The sample has no grant, so this phone
    /// is the fixture reader. A paired phone reads the id file and does not
    /// create one just to label the list.
    var catalogDeviceId: String {
        if canPair {
            return "fedcba9876543210fedcba9876543210"
        }
        if let stored = storedDeviceId(at: deviceURL) {
            return stored
        }
        return ""
    }

    func storedLabel() -> (name: String, os: String) {
        guard let existing = try? String(contentsOf: labelURL, encoding: .utf8) else {
            return ("", "")
        }
        let trimmed = existing.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            return ("", "")
        }
        return (trimmed, "iOS")
    }

    init() {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        supportURL = base.appendingPathComponent("satchel", isDirectory: true)
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        sampleURL = caches.appendingPathComponent("sample", isDirectory: true)
        legacyGrantURL = supportURL.appendingPathComponent("grant.json")
        deviceURL = supportURL.appendingPathComponent("device-id")
        labelURL = supportURL.appendingPathComponent("device-label")
        workURL = supportURL.appendingPathComponent("work", isDirectory: true)
        #if targetEnvironment(simulator)
        // UI tests start from an empty phone: no grant in the Keychain, no id, no work copy.
        if CommandLine.arguments.contains("-satchel-reset") {
            GrantVault.clear()
            GrantVault.clear(GrantVault.pending)
            try? FileManager.default.removeItem(at: supportURL)
            try? FileManager.default.removeItem(at: sampleURL)
        }
        #endif
        try? FileManager.default.createDirectory(at: supportURL, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: workURL, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: sampleURL, withIntermediateDirectories: true)
        try? BackupExclusion.exclude(supportURL)
        try? BackupExclusion.exclude(workURL)
        PhoneModel.migrateLegacyGrant(legacyGrantURL)
        // The Keychain outlives the app, but the device id and the work copy do not. A grant
        // without them belongs to a previous install and is dropped instead of reused.
        if GrantVault.read() != nil && storedDeviceId(at: deviceURL) == nil {
            GrantVault.clear()
        }
        try? BackupExclusion.exclude(deviceURL)
        try? BackupExclusion.exclude(labelURL)
    }

    private static func migrateLegacyGrant(_ file: URL) {
        guard let text = try? String(contentsOf: file, encoding: .utf8) else {
            return
        }
        if GrantVault.read() == nil && !GrantVault.write(text) {
            return
        }
        try? Data(count: text.utf8.count).write(to: file)
        try? FileManager.default.removeItem(at: file)
    }

    private var didOpen = false

    func openIfNeeded() {
        if didOpen {
            return
        }
        didOpen = true
        openCatalog()
    }

    func openCatalog() {
        screen = .loading
        let sample = sampleURL.path
        Task {
            // A change that stopped halfway is settled before the grant is read.
            if GrantVault.read(GrantVault.pending) != nil {
                await Task.detached { PhoneModel.settleRecovery() }.value
            }
            let grant = GrantVault.read() ?? ""
            let paired = !grant.isEmpty
            let rows = await Task.detached {
                if paired {
                    return SatchelCore.catalogGrant(specPath: grant)
                }
                return SatchelCore.sampleCatalog(root: sample)
            }.value
            if let rows {
                screen = .ready(rows: rows, busy: false)
            } else {
                screen = .failed
            }
        }
    }

    func showRecoveryChange() {
        screen = .changingRecovery(busy: false, failed: false)
    }

    func changeRecovery() {
        if case .changingRecovery(true, _) = screen {
            return
        }
        screen = .changingRecovery(busy: true, failed: false)
        Task {
            let fresh = await Task.detached { PhoneModel.runRecoveryChange() }.value
            if let fresh {
                screen = .recovery(
                    password: fresh,
                    note: "A senha antiga não abre mais o repositório. Guarde esta no gerenciador de senhas."
                )
            } else {
                screen = .changingRecovery(busy: false, failed: true)
            }
        }
    }

    /// The new grant is in the Keychain as pending before the repository changes,
    /// so a crash in between never leaves this phone without a password that opens it.
    nonisolated static func runRecoveryChange() -> String? {
        guard let old = GrantVault.read(),
              let (password, fresh) = SatchelCore.recoveryPrepare(grant: old),
              GrantVault.write(fresh, GrantVault.pending)
        else {
            return nil
        }
        guard SatchelCore.recoveryCommit(old: old, new: fresh) else {
            GrantVault.clear(GrantVault.pending)
            return nil
        }
        GrantVault.write(fresh)
        GrantVault.clear(GrantVault.pending)
        return password
    }

    /// Keep the main grant if it still opens, promote the pending one if only it opens,
    /// and leave both when neither answers (no network) to try on the next launch.
    nonisolated static func settleRecovery() {
        guard let waiting = GrantVault.read(GrantVault.pending) else {
            return
        }
        if let current = GrantVault.read(), SatchelCore.grantOpens(current) {
            GrantVault.clear(GrantVault.pending)
            return
        }
        if SatchelCore.grantOpens(waiting), GrantVault.write(waiting) {
            GrantVault.clear(GrantVault.pending)
        }
    }

    func showPairing() {
        let saved = storedLabel()
        let name = saved.name.isEmpty ? "iPhone" : saved.name
        screen = .pairing(
            address: "",
            code: "",
            fingerprint: "",
            name: name,
            busy: false,
            failed: false
        )
    }

    func showCreate() {
        screen = .creating(CreateDraft())
    }

    func update(_ draft: CreateDraft) {
        guard case .creating = screen else {
            return
        }
        screen = .creating(draft)
    }

    func create(_ draft: CreateDraft) {
        if draft.busy || !draft.complete {
            return
        }
        var working = draft
        working.busy = true
        working.failed = false
        screen = .creating(working)
        let deviceFile = deviceURL
        let work = workURL.path
        let directory = supportURL.appendingPathComponent("repository", isDirectory: true).path
        Task {
            let made = await Task.detached { () -> (password: String, stored: Bool)? in
                guard let id = deviceId(at: deviceFile) else {
                    return nil
                }
                let spec = draft.specJSON(directory: directory, deviceId: id)
                guard let created = SatchelCore.createRepository(spec: spec, work: work),
                      let split = created.firstIndex(of: "\n")
                else {
                    return nil
                }
                let password = String(created[created.startIndex..<split])
                let grant = String(created[created.index(after: split)...])
                return (password, GrantVault.write(grant))
            }.value
            if let made {
                var note: String?
                if !made.stored {
                    note = "O repositório foi criado, mas este aparelho não conseguiu guardar a concessão no Keychain. Guarde a senha abaixo agora: sem ela e sem a concessão não dá para abrir de novo por aqui."
                } else if draft.storage == "folder" {
                    note = "A pasta fica só neste aparelho. Ela não tem versionamento de objeto."
                }
                screen = .recovery(password: made.password, note: note)
            } else {
                var again = draft
                again.busy = false
                again.failed = true
                again.reason = SatchelCore.createFailure()
                screen = .creating(again)
            }
        }
    }

    func showInvite() {
        let grant = GrantVault.read() ?? ""
        Task {
            let started = await Task.detached { () -> (HoldOffer, Data?)? in
                guard let offer = SatchelCore.holdStart(grant: grant, bind: "") else {
                    return nil
                }
                return (offer, SatchelCore.invitePng(offer.invite))
            }.value
            if let started {
                screen = .holding(
                    HoldState(
                        offer: started.0,
                        approval: nil,
                        qr: started.1,
                        busy: false,
                        failed: false
                    )
                )
            }
        }
    }

    func pollHold() async {
        guard case .holding(let state) = screen, !state.busy else {
            return
        }
        let polled = await Task.detached { SatchelCore.holdPoll() }.value
        guard case .holding(var current) = screen, !current.busy else {
            return
        }
        switch polled {
        case .waiting:
            break
        case .asked(let approval):
            current.approval = approval
            screen = .holding(current)
        case .ended:
            openCatalog()
        }
    }

    func decideHold(approve: Bool) {
        guard case .holding(var state) = screen, !state.busy else {
            return
        }
        state.busy = true
        state.failed = false
        screen = .holding(state)
        let grant = GrantVault.read() ?? ""
        Task {
            let done = await Task.detached {
                SatchelCore.holdDecide(grant: grant, approve: approve)
            }.value
            if done {
                openCatalog()
            } else if case .holding(var again) = screen {
                again.busy = false
                again.failed = true
                screen = .holding(again)
            }
        }
    }

    func closeHold() {
        SatchelCore.holdClose()
        openCatalog()
    }

    func finishRecovery() {
        openCatalog()
    }

    func openFile(path: String) {
        guard case .ready(let rows, let busy) = screen, !busy else {
            return
        }
        screen = .file(path: path, rows: rows, busy: false)
    }

    func closeFile() {
        guard case .file(_, let rows, let busy) = screen, !busy else {
            return
        }
        screen = .ready(rows: rows, busy: false)
    }

    func showScanner(address: String, code: String, fingerprint: String, name: String) {
        screen = .scanning(address: address, code: code, fingerprint: fingerprint, name: name)
    }

    func applyScan(
        _ raw: String,
        previous: (address: String, code: String, fingerprint: String, name: String)
    ) {
        Task {
            let fields = await Task.detached {
                SatchelCore.parseInvite(raw)
            }.value
            if let fields {
                screen = .pairing(
                    address: fields.address,
                    code: fields.code,
                    fingerprint: fields.fingerprint,
                    name: previous.name,
                    busy: false,
                    failed: false
                )
            } else {
                screen = .pairing(
                    address: previous.address,
                    code: previous.code,
                    fingerprint: previous.fingerprint,
                    name: previous.name,
                    busy: false,
                    failed: true
                )
            }
        }
    }

    func join(address: String, code: String, fingerprint: String, name: String) {
        let trimmedAddress = address.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedCode = code.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedFingerprint = fingerprint.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedAddress.isEmpty || trimmedCode.isEmpty || trimmedFingerprint.isEmpty
            || !acceptableLabel(trimmedName)
        {
            screen = .pairing(
                address: trimmedAddress,
                code: trimmedCode,
                fingerprint: trimmedFingerprint,
                name: trimmedName,
                busy: false,
                failed: true
            )
            return
        }
        try? trimmedName.write(to: labelURL, atomically: true, encoding: .utf8)
        try? BackupExclusion.exclude(labelURL)
        screen = .pairing(
            address: trimmedAddress,
            code: trimmedCode,
            fingerprint: trimmedFingerprint,
            name: trimmedName,
            busy: true,
            failed: false
        )
        let deviceFile = deviceURL
        let work = workURL.path
        Task {
            let opened = await Task.detached {
                joinAndOpen(
                    address: trimmedAddress,
                    code: trimmedCode,
                    fingerprint: trimmedFingerprint,
                    deviceName: trimmedName,
                    deviceFile: deviceFile,
                    work: work
                )
            }.value
            if let opened {
                screen = .ready(rows: opened, busy: false)
            } else {
                screen = .pairing(
                    address: trimmedAddress,
                    code: trimmedCode,
                    fingerprint: trimmedFingerprint,
                    name: trimmedName,
                    busy: false,
                    failed: true
                )
            }
        }
    }

    func choose(path: String, follow: String) {
        let rows: [CatalogRow]
        let stayOnFile: Bool
        switch screen {
        case .ready(let listed, let busy):
            if busy {
                return
            }
            rows = listed
            stayOnFile = false
        case .file(let openPath, let listed, let busy):
            if busy || openPath != path {
                return
            }
            rows = listed
            stayOnFile = true
        default:
            return
        }
        if stayOnFile {
            screen = .file(path: path, rows: rows, busy: true)
        } else {
            screen = .ready(rows: rows, busy: true)
        }
        let sample = sampleURL.path
        let grant = GrantVault.read() ?? ""
        let paired = !grant.isEmpty
        let label = storedLabel()
        Task {
            let listed = await Task.detached {
                applyFollow(
                    path: path,
                    follow: follow,
                    sample: sample,
                    grant: grant,
                    paired: paired,
                    name: label.name,
                    os: label.os
                )
            }.value
            if let listed {
                if stayOnFile {
                    screen = .file(path: path, rows: listed, busy: false)
                } else {
                    screen = .ready(rows: listed, busy: false)
                }
            } else {
                screen = .failed
            }
        }
    }
}

struct CreateDraft: Equatable {
    var storage: String = "folder"
    var endpoint: String = ""
    var bucket: String = ""
    var region: String = ""
    var prefix: String = ""
    var accessKeyId: String = ""
    var secretAccessKey: String = ""
    var deviceAccessKeyId: String = ""
    var deviceSecretAccessKey: String = ""
    var busy: Bool = false
    var failed: Bool = false
    var reason: String = ""

    var complete: Bool {
        if storage == "folder" {
            return true
        }
        let needed = [
            endpoint, bucket, region, accessKeyId, secretAccessKey,
            deviceAccessKeyId, deviceSecretAccessKey,
        ]
        for value in needed where value.trimmingCharacters(in: .whitespaces).isEmpty {
            return false
        }
        return accessKeyId.trimmingCharacters(in: .whitespaces)
            != deviceAccessKeyId.trimmingCharacters(in: .whitespaces)
    }

    func specJSON(directory: String, deviceId: String) -> String {
        var object: [String: String] = ["kind": storage, "device_id": deviceId]
        if storage == "folder" {
            object["directory"] = directory
        } else {
            object["endpoint"] = endpoint.trimmingCharacters(in: .whitespaces)
            object["bucket"] = bucket.trimmingCharacters(in: .whitespaces)
            object["region"] = region.trimmingCharacters(in: .whitespaces)
            object["prefix"] = prefix.trimmingCharacters(in: .whitespaces)
            object["access_key_id"] = accessKeyId.trimmingCharacters(in: .whitespaces)
            object["secret_access_key"] = secretAccessKey.trimmingCharacters(in: .whitespaces)
            object["device_access_key_id"] = deviceAccessKeyId.trimmingCharacters(in: .whitespaces)
            object["device_secret_access_key"] =
                deviceSecretAccessKey.trimmingCharacters(in: .whitespaces)
        }
        guard let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8)
        else {
            return "{}"
        }
        return text
    }
}

private func acceptableLabel(_ value: String) -> Bool {
    let count = value.count
    if count == 0 || count > 64 {
        return false
    }
    for scalar in value.unicodeScalars {
        if scalar.value < 32 || scalar.value == 127 {
            return false
        }
    }
    return true
}

private func storedDeviceId(at file: URL) -> String? {
    guard let existing = try? String(contentsOf: file, encoding: .utf8) else {
        return nil
    }
    let trimmed = existing.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.count == 32 {
        return trimmed
    }
    return nil
}

private func deviceId(at file: URL) -> String? {
    if let existing = try? String(contentsOf: file, encoding: .utf8) {
        let trimmed = existing.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.count == 32 {
            try? BackupExclusion.exclude(file)
            return trimmed
        }
    }
    guard let created = SatchelCore.newDeviceId() else {
        return nil
    }
    try? created.write(to: file, atomically: true, encoding: .utf8)
    try? BackupExclusion.exclude(file)
    return created
}

private func joinAndOpen(
    address: String,
    code: String,
    fingerprint: String,
    deviceName: String,
    deviceFile: URL,
    work: String
) -> [CatalogRow]? {
    guard let id = deviceId(at: deviceFile) else {
        return nil
    }
    guard let joined = SatchelCore.joinPairing(
        address: address,
        code: code,
        fingerprint: fingerprint,
        deviceName: deviceName,
        osName: "iOS",
        deviceId: id
    ) else {
        return nil
    }
    guard let stored = SatchelCore.assembleGrant(joined: joined, deviceId: id, work: work) else {
        return nil
    }
    guard GrantVault.write(stored) else {
        return nil
    }
    let rows = SatchelCore.catalogGrant(specPath: stored)
    if rows == nil {
        GrantVault.clear()
    }
    return rows
}

private func applyFollow(
    path: String,
    follow: String,
    sample: String,
    grant: String,
    paired: Bool,
    name: String,
    os: String
) -> [CatalogRow]? {
    if paired {
        if !SatchelCore.markGrant(specPath: grant, path: path, follow: follow, name: name, os: os) {
            return nil
        }
        if follow != "see" && !SatchelCore.readGrant(specPath: grant, path: path) {
            return nil
        }
        return SatchelCore.catalogGrant(specPath: grant)
    }
    if !SatchelCore.markSample(root: sample, path: path, follow: follow, name: name, os: os) {
        return nil
    }
    if follow != "see" && !SatchelCore.readSample(root: sample, path: path) {
        return nil
    }
    return SatchelCore.sampleCatalog(root: sample)
}
