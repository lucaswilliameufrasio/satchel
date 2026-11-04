import Foundation

@_silgen_name("satchel_free_string")
private func satchelFreeString(_ value: UnsafeMutablePointer<CChar>?)

@_silgen_name("satchel_free_bytes")
private func satchelFreeBytes(_ value: UnsafeMutablePointer<UInt8>?, _ len: Int)

@_silgen_name("satchel_parse_invite")
private func satchelParseInvite(_ text: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_new_device_id")
private func satchelNewDeviceId() -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_assemble_grant")
private func satchelAssembleGrant(
    _ joined: UnsafePointer<CChar>,
    _ deviceId: UnsafePointer<CChar>,
    _ work: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_join_pairing")
private func satchelJoinPairing(
    _ address: UnsafePointer<CChar>,
    _ code: UnsafePointer<CChar>,
    _ fingerprint: UnsafePointer<CChar>,
    _ deviceName: UnsafePointer<CChar>,
    _ osName: UnsafePointer<CChar>,
    _ deviceId: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_create_repository")
private func satchelCreateRepository(
    _ spec: UnsafePointer<CChar>,
    _ work: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_create_failure")
private func satchelCreateFailure() -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_is_maintainer")
private func satchelIsMaintainer(_ grant: UnsafePointer<CChar>) -> Int32

@_silgen_name("satchel_recovery_prepare")
private func satchelRecoveryPrepare(_ grant: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_recovery_commit")
private func satchelRecoveryCommit(_ old: UnsafePointer<CChar>, _ new: UnsafePointer<CChar>) -> Int32

@_silgen_name("satchel_grant_opens")
private func satchelGrantOpens(_ grant: UnsafePointer<CChar>) -> Int32

@_silgen_name("satchel_hold_start")
private func satchelHoldStart(
    _ grant: UnsafePointer<CChar>,
    _ bind: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_hold_poll")
private func satchelHoldPoll() -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_hold_decide")
private func satchelHoldDecide(_ grant: UnsafePointer<CChar>, _ approve: Int32) -> Int32

@_silgen_name("satchel_hold_close")
private func satchelHoldClose()

@_silgen_name("satchel_invite_png")
private func satchelInvitePng(
    _ invite: UnsafePointer<CChar>,
    _ len: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<UInt8>?

@_silgen_name("satchel_sample_catalog")
private func satchelSampleCatalog(_ root: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_mark_sample")
private func satchelMarkSample(
    _ root: UnsafePointer<CChar>,
    _ path: UnsafePointer<CChar>,
    _ follow: UnsafePointer<CChar>,
    _ name: UnsafePointer<CChar>,
    _ os: UnsafePointer<CChar>
) -> Int32

@_silgen_name("satchel_read_sample")
private func satchelReadSample(
    _ root: UnsafePointer<CChar>,
    _ path: UnsafePointer<CChar>,
    _ len: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<UInt8>?

@_silgen_name("satchel_catalog_grant")
private func satchelCatalogGrant(_ specPath: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("satchel_mark_grant")
private func satchelMarkGrant(
    _ specPath: UnsafePointer<CChar>,
    _ path: UnsafePointer<CChar>,
    _ follow: UnsafePointer<CChar>,
    _ name: UnsafePointer<CChar>,
    _ os: UnsafePointer<CChar>
) -> Int32

@_silgen_name("satchel_read_grant")
private func satchelReadGrant(
    _ specPath: UnsafePointer<CChar>,
    _ path: UnsafePointer<CChar>,
    _ len: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<UInt8>?

struct DeviceCard: Equatable {
    let id: String
    let name: String?
    let os: String?
    let enrolledUnix: Int64?
    let lastUnix: Int64
}

struct CatalogRow: Identifiable, Equatable {
    let path: String
    let isDirectory: Bool
    let size: Int64
    let versionUnix: Int64
    let devices: [String]
    let holders: [DeviceCard]
    let follow: String

    var id: String {
        path
    }
}

struct InviteFields: Equatable {
    let address: String
    let code: String
    let fingerprint: String
}

struct HoldOffer: Equatable {
    let code: String
    let address: String
    let fingerprint: String
    let invite: String
}

struct HoldApproval: Equatable {
    let deviceName: String
    let os: String
    let peerAddress: String
    let code: String
}

enum HoldPoll {
    case waiting
    case asked(HoldApproval)
    case ended
}

enum SatchelCore {
    static func parseInvite(_ text: String) -> InviteFields? {
        let canonical = withCString(text, satchelParseInvite)
        guard let json = takeString(canonical) else {
            return nil
        }
        guard let data = json.data(using: .utf8) else {
            return nil
        }
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        guard let address = object["address"] as? String else {
            return nil
        }
        guard let code = object["code"] as? String else {
            return nil
        }
        guard let fingerprint = object["fingerprint"] as? String else {
            return nil
        }
        return InviteFields(address: address, code: code, fingerprint: fingerprint)
    }

    static func newDeviceId() -> String? {
        takeString(satchelNewDeviceId())
    }

    static func assembleGrant(joined: String, deviceId: String, work: String) -> String? {
        let pointer = joined.withCString { joinedPointer in
            deviceId.withCString { idPointer in
                work.withCString { workPointer in
                    satchelAssembleGrant(joinedPointer, idPointer, workPointer)
                }
            }
        }
        return takeString(pointer)
    }

    static func joinPairing(
        address: String,
        code: String,
        fingerprint: String,
        deviceName: String,
        osName: String,
        deviceId: String
    ) -> String? {
        let pointer = address.withCString { addressPointer in
            code.withCString { codePointer in
                fingerprint.withCString { fingerprintPointer in
                    deviceName.withCString { namePointer in
                        osName.withCString { osPointer in
                            deviceId.withCString { idPointer in
                                satchelJoinPairing(
                                    addressPointer,
                                    codePointer,
                                    fingerprintPointer,
                                    namePointer,
                                    osPointer,
                                    idPointer
                                )
                            }
                        }
                    }
                }
            }
        }
        return takeString(pointer)
    }

    /// Create the repository. Returns `recovery password`, a newline, then the grant JSON for
    /// the Keychain. Nothing is written to disk.
    static func createRepository(spec: String, work: String) -> String? {
        let pointer = spec.withCString { specPointer in
            work.withCString { workPointer in
                satchelCreateRepository(specPointer, workPointer)
            }
        }
        return takeString(pointer)
    }

    /// `protection`, `exists`, `unreachable` or `fields` for the last refused create.
    static func createFailure() -> String {
        takeString(satchelCreateFailure()) ?? ""
    }

    /// First half of a recovery change: the new password and the new grant. Changes nothing.
    static func recoveryPrepare(grant: String) -> (String, String)? {
        let pointer = grant.withCString { satchelRecoveryPrepare($0) }
        guard let text = takeString(pointer), let split = text.firstIndex(of: "\n") else {
            return nil
        }
        return (String(text[text.startIndex..<split]), String(text[text.index(after: split)...]))
    }

    /// Second half: true when the repository now opens with the new grant.
    static func recoveryCommit(old: String, new: String) -> Bool {
        old.withCString { oldPointer in
            new.withCString { satchelRecoveryCommit(oldPointer, $0) == 1 }
        }
    }

    static func grantOpens(_ grant: String) -> Bool {
        grant.withCString { satchelGrantOpens($0) == 1 }
    }

    static func isMaintainer(grant: String) -> Bool {
        grant.withCString { satchelIsMaintainer($0) == 1 }
    }

    /// Offer JSON, or nil when this device cannot show an invite.
    static func holdStart(grant: String, bind: String) -> HoldOffer? {
        let pointer = grant.withCString { grantPointer in
            bind.withCString { bindPointer in
                satchelHoldStart(grantPointer, bindPointer)
            }
        }
        guard let json = takeString(pointer), let data = json.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let code = object["code"] as? String,
              let address = object["address"] as? String,
              let fingerprint = object["fingerprint"] as? String,
              let invite = object["invite"] as? String
        else {
            return nil
        }
        return HoldOffer(code: code, address: address, fingerprint: fingerprint, invite: invite)
    }

    /// `.some(nil)` while nobody has passed the code check. `nil` when the session ended.
    static func holdPoll() -> HoldPoll {
        guard let json = takeString(satchelHoldPoll()) else {
            return .ended
        }
        if json.isEmpty {
            return .waiting
        }
        guard let data = json.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let name = object["deviceName"] as? String,
              let os = object["os"] as? String,
              let peer = object["peerAddress"] as? String,
              let code = object["code"] as? String
        else {
            return .ended
        }
        return .asked(HoldApproval(deviceName: name, os: os, peerAddress: peer, code: code))
    }

    static func holdDecide(grant: String, approve: Bool) -> Bool {
        grant.withCString { satchelHoldDecide($0, approve ? 1 : 0) == 1 }
    }

    static func holdClose() {
        satchelHoldClose()
    }

    static func invitePng(_ invite: String) -> Data? {
        var length = 0
        let pointer = invite.withCString { satchelInvitePng($0, &length) }
        guard let pointer else {
            return nil
        }
        let data = Data(bytes: pointer, count: length)
        satchelFreeBytes(pointer, length)
        return data
    }

    static func sampleCatalog(root: String) -> [CatalogRow]? {
        rows(takeString(withCString(root, satchelSampleCatalog)))
    }

    static func markSample(
        root: String,
        path: String,
        follow: String,
        name: String,
        os: String
    ) -> Bool {
        let status = root.withCString { rootPointer in
            path.withCString { pathPointer in
                follow.withCString { followPointer in
                    name.withCString { namePointer in
                        os.withCString { osPointer in
                            satchelMarkSample(
                                rootPointer,
                                pathPointer,
                                followPointer,
                                namePointer,
                                osPointer
                            )
                        }
                    }
                }
            }
        }
        return status == 0
    }

    static func readSample(root: String, path: String) -> Bool {
        root.withCString { rootPointer in
            path.withCString { pathPointer in
                var len = 0
                let pointer = satchelReadSample(rootPointer, pathPointer, &len)
                if pointer == nil {
                    return false
                }
                satchelFreeBytes(pointer, len)
                return true
            }
        }
    }

    static func catalogGrant(specPath: String) -> [CatalogRow]? {
        rows(takeString(withCString(specPath, satchelCatalogGrant)))
    }

    static func markGrant(
        specPath: String,
        path: String,
        follow: String,
        name: String,
        os: String
    ) -> Bool {
        let status = specPath.withCString { specPointer in
            path.withCString { pathPointer in
                follow.withCString { followPointer in
                    name.withCString { namePointer in
                        os.withCString { osPointer in
                            satchelMarkGrant(
                                specPointer,
                                pathPointer,
                                followPointer,
                                namePointer,
                                osPointer
                            )
                        }
                    }
                }
            }
        }
        return status == 0
    }

    static func readGrant(specPath: String, path: String) -> Bool {
        specPath.withCString { specPointer in
            path.withCString { pathPointer in
                var len = 0
                let pointer = satchelReadGrant(specPointer, pathPointer, &len)
                if pointer == nil {
                    return false
                }
                satchelFreeBytes(pointer, len)
                return true
            }
        }
    }

    private static func rows(_ json: String?) -> [CatalogRow]? {
        guard let json else {
            return nil
        }
        guard let data = json.data(using: .utf8) else {
            return nil
        }
        guard let list = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            return nil
        }
        var rows: [CatalogRow] = []
        for item in list {
            guard let path = item["path"] as? String else {
                return nil
            }
            guard let directory = (item["directory"] as? NSNumber)?.boolValue else {
                return nil
            }
            guard let size = (item["size"] as? NSNumber)?.int64Value else {
                return nil
            }
            guard let versionUnix = (item["versionUnix"] as? NSNumber)?.int64Value else {
                return nil
            }
            guard let follow = item["follow"] as? String else {
                return nil
            }
            guard let deviceValues = item["devices"] as? [String] else {
                return nil
            }
            let holders = deviceCards(item["holders"])
            rows.append(
                CatalogRow(
                    path: path,
                    isDirectory: directory,
                    size: size,
                    versionUnix: versionUnix,
                    devices: deviceValues,
                    holders: holders,
                    follow: follow
                )
            )
        }
        return rows
    }

    private static func deviceCards(_ value: Any?) -> [DeviceCard] {
        guard let list = value as? [[String: Any]] else {
            return []
        }
        var cards: [DeviceCard] = []
        for item in list {
            guard let id = item["id"] as? String else {
                continue
            }
            guard let lastUnix = (item["lastUnix"] as? NSNumber)?.int64Value else {
                continue
            }
            let name = item["name"] as? String
            let os = item["os"] as? String
            let enrolledUnix = (item["enrolledUnix"] as? NSNumber)?.int64Value
            cards.append(
                DeviceCard(
                    id: id,
                    name: name,
                    os: os,
                    enrolledUnix: enrolledUnix,
                    lastUnix: lastUnix
                )
            )
        }
        return cards
    }

    private static func takeString(_ pointer: UnsafeMutablePointer<CChar>?) -> String? {
        guard let pointer else {
            return nil
        }
        let text = String(cString: pointer)
        satchelFreeString(pointer)
        return text
    }

    private static func withCString(
        _ text: String,
        _ body: (UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
    ) -> UnsafeMutablePointer<CChar>? {
        text.withCString { pointer in
            body(pointer)
        }
    }
}

func followLabel(_ follow: String) -> String {
    if follow == "once" {
        return "Baixar agora"
    }
    if follow == "always" {
        return "Sincronizar sempre"
    }
    return "Ver"
}

func followDetail(_ follow: String, directory: Bool) -> String {
    if follow == "once" {
        return "Traz o arquivo uma vez e não acompanha mudanças posteriores."
    }
    if follow == "always" {
        if directory {
            return "Baixa, envia e acompanha. Arquivo novo dentro desta pasta entra nesse estado."
        }
        return "Baixa, envia e acompanha."
    }
    return "O conteúdo fica no storage. Este aparelho não baixa."
}

func byteCount(_ size: Int64) -> String {
    if size < 0 {
        return "0 B"
    }
    let units = ["B", "KiB", "MiB", "GiB", "TiB"]
    var value = Double(size)
    var unit = 0
    while value >= 1024 && unit < units.count - 1 {
        value /= 1024
        unit += 1
    }
    if unit == 0 {
        return "\(size) B"
    }
    let tenths = Int((value * 10).rounded())
    let whole = tenths / 10
    let fraction = tenths % 10
    if fraction == 0 || value >= 10 {
        return "\(Int(value.rounded())) \(units[unit])"
    }
    return "\(whole).\(fraction) \(units[unit])"
}

func catalogMeta(_ row: CatalogRow, zone: TimeZone = .current) -> String {
    let when = syncedWhen(row.versionUnix, zone: zone)
    if row.isDirectory {
        return "Pasta · \(when)"
    }
    return "\(byteCount(row.size)) · \(when)"
}

func deviceCountLabel(_ devices: [String]) -> String {
    if devices.isEmpty {
        return "Nenhum aparelho"
    }
    if devices.count == 1 {
        return "1 aparelho"
    }
    return "\(devices.count) aparelhos"
}

func groupedDeviceId(_ id: String) -> String {
    if id.count != 32 {
        return id
    }
    for character in id {
        if !character.isHexDigit {
            return id
        }
    }
    var grouped = ""
    var index = 0
    for character in id {
        if index > 0 && index.isMultiple(of: 4) {
            grouped += " "
        }
        grouped.append(character)
        index += 1
    }
    return grouped
}

func syncedWhen(_ versionUnix: Int64, zone: TimeZone = .current) -> String {
    let instant = Date(timeIntervalSince1970: TimeInterval(versionUnix))
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = zone
    formatter.dateFormat = "dd/MM/yyyy HH:mm"
    let clock = formatter.string(from: instant)
    return "\(clock) \(shortZoneName(zone, at: instant))"
}

/// Short name of `zone` at `instant`. An explicit UTC zone stays `UTC`.
func shortZoneName(_ zone: TimeZone, at instant: Date) -> String {
    let utcIdentifiers = [
        "UTC",
        "GMT",
        "Etc/UTC",
        "Etc/GMT",
        "Z",
        "GMT+0",
        "GMT-0",
        "UTC+0",
        "UTC-0",
    ]
    if utcIdentifiers.contains(zone.identifier) {
        return "UTC"
    }
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = zone
    formatter.dateFormat = "zzz"
    return formatter.string(from: instant)
}


