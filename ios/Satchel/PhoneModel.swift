import Combine
import Foundation

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
    }

    @Published var screen: Screen = .loading

    let supportURL: URL
    let sampleURL: URL
    let grantURL: URL
    let deviceURL: URL
    let labelURL: URL
    let workURL: URL

    var canPair: Bool {
        !FileManager.default.fileExists(atPath: grantURL.path)
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
        grantURL = supportURL.appendingPathComponent("grant.json")
        deviceURL = supportURL.appendingPathComponent("device-id")
        labelURL = supportURL.appendingPathComponent("device-label")
        workURL = supportURL.appendingPathComponent("work", isDirectory: true)
        try? FileManager.default.createDirectory(at: supportURL, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: workURL, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: sampleURL, withIntermediateDirectories: true)
        try? BackupExclusion.exclude(supportURL)
        try? BackupExclusion.exclude(workURL)
        try? BackupExclusion.exclude(grantURL)
        try? BackupExclusion.exclude(deviceURL)
        try? BackupExclusion.exclude(labelURL)
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
        let grant = grantURL.path
        let paired = FileManager.default.fileExists(atPath: grant)
        Task {
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
        let grantFile = grantURL
        let work = workURL.path
        Task {
            let opened = await Task.detached {
                joinAndOpen(
                    address: trimmedAddress,
                    code: trimmedCode,
                    fingerprint: trimmedFingerprint,
                    deviceName: trimmedName,
                    deviceFile: deviceFile,
                    grantFile: grantFile,
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
        let grant = grantURL.path
        let paired = FileManager.default.fileExists(atPath: grant)
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
    grantFile: URL,
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
    do {
        try stored.write(to: grantFile, atomically: true, encoding: .utf8)
        try BackupExclusion.exclude(grantFile)
    } catch {
        return nil
    }
    let rows = SatchelCore.catalogGrant(specPath: grantFile.path)
    if rows == nil {
        try? FileManager.default.removeItem(at: grantFile)
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
