import SwiftUI

@main
struct SatchelApp: App {
    @StateObject private var model = PhoneModel()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            PhoneView(model: model)
                .onAppear {
                    model.openIfNeeded()
                }
        }
        .onChange(of: phase) { now in
            if now == .active {
                model.startWakes()
            } else {
                model.stopWakes()
            }
        }
    }
}

struct PhoneView: View {
    @ObservedObject var model: PhoneModel
    @State private var pickingFolder = false

    var body: some View {
        screenView
            .fileImporter(isPresented: $pickingFolder, allowedContentTypes: [.folder]) { result in
                if case .success(let url) = result {
                    model.chooseFolder(url)
                }
            }
    }

    @ViewBuilder
    private var screenView: some View {
        switch model.screen {
        case .loading:
            StatusPage(title: String(localized: "Catalog", bundle: .copy, locale: .copy), message: String(localized: "Reading the catalog…", bundle: .copy, locale: .copy))
        case .failed:
            StatusPage(
                title: String(localized: "Catalog", bundle: .copy, locale: .copy),
                message: String(localized: "Couldn't open the catalog. Check the network and try again.", bundle: .copy, locale: .copy),
                retry: { model.openCatalog() }
            )
        case .ready(let rows, _):
            CatalogList(
                rows: rows,
                canPair: model.canPair,
                zone: .current,
                canCreate: model.canPair,
                onCreate: { model.showCreate() },
                canInvite: model.canInvite,
                onInvite: { model.showInvite() },
                onChangeRecovery: { model.showRecoveryChange() },
                folder: model.canPair
                    ? nil
                    : FolderCardState(name: model.folderName, status: model.folderStatus, busy: model.folderBusy),
                onChooseFolder: { pickingFolder = true },
                onSyncFolder: { model.syncFolder() },
                onPair: {
                    model.showPairing()
                },
                onOpen: { path in
                    model.openFile(path: path)
                }
            )
        case .creating(let draft):
            CreatePage(
                draft: draft,
                onChange: { model.update($0) },
                onBack: { model.openCatalog() },
                onSubmit: { model.create(draft) }
            )
        case .changingRecovery(let busy, let failed):
            RecoveryChangePage(
                busy: busy,
                failed: failed,
                onChange: { model.changeRecovery() },
                onBack: { model.openCatalog() }
            )
        case .recovery(let password, let note):
            RecoveryPage(password: password, note: note, onDone: { model.finishRecovery() })
        case .holding(let state):
            HoldPage(
                state: state,
                onApprove: { model.decideHold(approve: true) },
                onDeny: { model.decideHold(approve: false) },
                onBack: { model.closeHold() }
            )
            .task {
                while !Task.isCancelled {
                    await model.pollHold()
                    try? await Task.sleep(nanoseconds: 500_000_000)
                }
            }
        case .file(let path, let rows, let busy):
            FilePage(
                path: path,
                rows: rows,
                busy: busy,
                localDeviceId: model.catalogDeviceId,
                zone: .current,
                onBack: {
                    model.closeFile()
                },
                onChoose: { follow in
                    model.choose(path: path, follow: follow)
                }
            )
        case .pairing(let address, let code, let fingerprint, let name, let busy, let failed):
            PairForm(
                address: address,
                code: code,
                fingerprint: fingerprint,
                name: name,
                busy: busy,
                failed: failed,
                onScan: {
                    model.showScanner(
                        address: address,
                        code: code,
                        fingerprint: fingerprint,
                        name: name
                    )
                },
                onBack: {
                    model.openCatalog()
                },
                onSubmit: { address, code, fingerprint, name in
                    model.join(
                        address: address,
                        code: code,
                        fingerprint: fingerprint,
                        name: name
                    )
                }
            )
        case .scanning(let address, let code, let fingerprint, let name):
            ScannerPage(onCode: { raw in
                model.applyScan(raw, previous: (address, code, fingerprint, name))
            }, onBack: {
                model.screen = .pairing(
                    address: address,
                    code: code,
                    fingerprint: fingerprint,
                    name: name,
                    busy: false,
                    failed: false
                )
            })
        }
    }
}

/// The folder card on the catalog. `name` is nil until a folder is picked.
struct FolderCardState {
    let name: String?
    let status: String?
    let busy: Bool
}

struct StatusPage: View {
    let title: String
    let message: String
    /// Shown as Tentar de novo under a failure. A wait has none.
    var retry: (() -> Void)?

    var body: some View {
        ProductScreen(title: title) {
            VStack(spacing: 20) {
                if retry == nil {
                    ProgressView()
                        .tint(SatchelInk.secondary)
                }
                Text(message)
                    .font(.body)
                    .foregroundStyle(retry == nil ? SatchelInk.secondary : SatchelInk.ink)
                    .multilineTextAlignment(.center)
                if let retry {
                    WideButton(title: String(localized: "Try again", bundle: .copy, locale: .copy), id: "status-retry", action: retry)
                }
            }
            .frame(maxWidth: .infinity, minHeight: 280)
        }
    }
}

struct CatalogList: View {
    let rows: [CatalogRow]
    let canPair: Bool
    var zone: TimeZone = .current
    var canCreate: Bool = false
    var onCreate: () -> Void = {}
    var canInvite: Bool = false
    var onInvite: () -> Void = {}
    var onChangeRecovery: () -> Void = {}
    var folder: FolderCardState?
    var onChooseFolder: () -> Void = {}
    var onSyncFolder: () -> Void = {}
    let onPair: () -> Void
    let onOpen: (String) -> Void

    var body: some View {
        ProductScreen(title: String(localized: "Catalog", bundle: .copy, locale: .copy)) {
            if canCreate {
                actionCard(
                    String(localized: "Create repository", bundle: .copy, locale: .copy),
                    String(localized: "This device keeps the recovery password. The other one joins with a code.", bundle: .copy, locale: .copy),
                    id: "create-repository",
                    action: onCreate
                )
            }
            if canInvite {
                actionCard(
                    String(localized: "Show invite", bundle: .copy, locale: .copy),
                    String(localized: "The recovery password is not in the invite.", bundle: .copy, locale: .copy),
                    id: "show-invite",
                    action: onInvite
                )
                actionCard(
                    String(localized: "Change recovery password", bundle: .copy, locale: .copy),
                    String(localized: "Only this device, which created the repository, can change it.", bundle: .copy, locale: .copy),
                    id: "change-recovery",
                    action: onChangeRecovery
                )
            }
            if let folder {
                if let name = folder.name {
                    actionCard(
                        String(localized: "Phone folder: \(name)", bundle: .copy, locale: .copy),
                        folder.status ?? folderReadyNote,
                        id: "folder-sync",
                        action: onSyncFolder
                    )
                    .disabled(folder.busy)
                    actionCard(
                        String(localized: "Choose another folder", bundle: .copy, locale: .copy),
                        folderSwitchNote,
                        id: "folder-switch",
                        action: onChooseFolder
                    )
                    .disabled(folder.busy)
                } else {
                    actionCard(folderTitle, folderChooseNote, id: "folder-choose", action: onChooseFolder)
                }
            }
            if canPair {
                Button(action: onPair) {
                    ProductCard {
                        HStack(alignment: .center, spacing: 12) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(String(localized: "Join on this device", bundle: .copy, locale: .copy))
                                    .font(.headline)
                                    .foregroundStyle(SatchelInk.ink)
                                Text(String(localized: "Scan the QR shown on the desktop, or enter the invite.", bundle: .copy, locale: .copy))
                                    .font(.subheadline)
                                    .foregroundStyle(SatchelInk.secondary)
                            }
                            Spacer(minLength: 12)
                            Image(systemName: "chevron.right")
                                .font(.footnote.weight(.semibold))
                                .foregroundStyle(SatchelInk.secondary)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
            if rows.isEmpty {
                ProductCard {
                    Text(String(localized: "No files yet. What this or another device sends shows up here.", bundle: .copy, locale: .copy))
                        .foregroundStyle(SatchelInk.secondary)
                }
            }
            ForEach(rows) { row in
                Button {
                    onOpen(row.path)
                } label: {
                    CatalogRowLabel(row: row, zone: zone)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("catalog-row-\(row.path)")
            }
        }
    }
}

extension CatalogList {
    fileprivate func actionCard(
        _ title: String,
        _ detail: String,
        id: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            ProductCard {
                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(title)
                            .font(.headline)
                            .foregroundStyle(SatchelInk.ink)
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(SatchelInk.secondary)
                    }
                    Spacer(minLength: 12)
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(SatchelInk.secondary)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(id)
    }
}

struct CatalogRowLabel: View {
    let row: CatalogRow
    var zone: TimeZone = .current

    var body: some View {
        ProductCard {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: row.isDirectory ? "folder" : "doc")
                    .font(.title3)
                    .foregroundStyle(SatchelInk.accent)
                    .accessibilityHidden(true)
                    .frame(width: 28, alignment: .center)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(row.path)
                            .font(.headline)
                            .foregroundStyle(SatchelInk.ink)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(followLabel(row.follow))
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(SatchelInk.accent)
                            .layoutPriority(1)
                    }
                    Text(catalogMeta(row, zone: zone))
                        .font(.subheadline)
                        .foregroundStyle(SatchelInk.secondary)
                    Text(deviceCountLabel(row.devices))
                        .font(.footnote)
                        .foregroundStyle(SatchelInk.secondary)
                }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

struct FilePage: View {
    let path: String
    let rows: [CatalogRow]
    let busy: Bool
    let localDeviceId: String
    var zone: TimeZone = .current
    let onBack: () -> Void
    let onChoose: (String) -> Void

    var body: some View {
        ProductScreen(title: path, backLabel: String(localized: "Catalog", bundle: .copy, locale: .copy), backDisabled: busy, onBack: onBack) {
            if let row = listedRow(path, rows) {
                ProductCard {
                    factLine(String(localized: "Size", bundle: .copy, locale: .copy), row.isDirectory ? String(localized: "Folder", bundle: .copy, locale: .copy) : byteCount(row.size))
                    factLine(String(localized: "Synced", bundle: .copy, locale: .copy), syncedWhen(row.versionUnix, zone: zone))
                }
                ProductCard {
                    Text(String(localized: "On this device", bundle: .copy, locale: .copy))
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(SatchelInk.secondary)
                    stateButton(String(localized: "View", bundle: .copy, locale: .copy), "see", row: row)
                    stateButton(String(localized: "Download now", bundle: .copy, locale: .copy), "once", row: row)
                    stateButton(String(localized: "Always sync", bundle: .copy, locale: .copy), "always", row: row)
                    if busy {
                        Text(String(localized: "Saving the choice…", bundle: .copy, locale: .copy))
                            .font(.subheadline)
                            .foregroundStyle(SatchelInk.secondary)
                    }
                }
                ProductCard {
                    Text(String(localized: "Devices", bundle: .copy, locale: .copy))
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(SatchelInk.secondary)
                    if row.devices.isEmpty {
                        Text(String(localized: "No device keeps a copy.", bundle: .copy, locale: .copy))
                            .foregroundStyle(SatchelInk.secondary)
                    }
                    ForEach(Array(row.devices.enumerated()), id: \.offset) { _, device in
                        DeviceFacts(
                            deviceId: device,
                            card: holderCard(device, row.holders),
                            local: device == localDeviceId,
                            zone: zone
                        )
                    }
                }
            } else {
                ProductCard {
                    Text(String(localized: "No files yet. What this or another device sends shows up here.", bundle: .copy, locale: .copy))
                        .foregroundStyle(SatchelInk.secondary)
                }
            }
        }
    }

    private func factLine(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(SatchelInk.secondary)
            Text(value)
                .font(.body)
                .foregroundStyle(SatchelInk.ink)
        }
    }

    private func stateButton(_ label: String, _ follow: String, row: CatalogRow) -> some View {
        let selected = row.follow == follow
        return Button {
            onChoose(follow)
        } label: {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(label)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(SatchelInk.ink)
                    Text(followDetail(follow, directory: row.isDirectory))
                        .font(.subheadline)
                        .foregroundStyle(SatchelInk.secondary)
                }
                Spacer(minLength: 12)
                if selected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(SatchelInk.accent)
                        .accessibilityHidden(true)
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(selected ? SatchelInk.selected : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .accessibilityIdentifier("follow-\(follow)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

func holderCard(_ id: String, _ cards: [DeviceCard]) -> DeviceCard? {
    for card in cards {
        if card.id == id {
            return card
        }
    }
    return nil
}

struct DeviceFacts: View {
    let deviceId: String
    let card: DeviceCard?
    let local: Bool
    var zone: TimeZone = .current

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.headline)
                .foregroundStyle(SatchelInk.ink)
            if let name = card?.name, title != name {
                Text(String(localized: "Nickname \(name)", bundle: .copy, locale: .copy))
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if let os = card?.os {
                Text(String(localized: "System \(os)", bundle: .copy, locale: .copy))
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if let enrolled = card?.enrolledUnix {
                Text(String(localized: "Registered \(syncedWhen(enrolled, zone: zone))", bundle: .copy, locale: .copy))
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if let last = card?.lastUnix {
                Text(String(localized: "Last sent \(syncedWhen(last, zone: zone))", bundle: .copy, locale: .copy))
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if title != groupedDeviceId(deviceId) {
                Text(groupedDeviceId(deviceId))
                    .font(.footnote.monospaced())
                    .foregroundStyle(SatchelInk.secondary)
                    .textSelection(.enabled)
            }
        }
        .padding(.top, 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(local ? "device-card-local" : "device-card-other")
    }

    private var title: String {
        if local {
            return String(localized: "This device", bundle: .copy, locale: .copy)
        }
        if let name = card?.name, !name.isEmpty {
            return name
        }
        return groupedDeviceId(deviceId)
    }
}

func listedRow(_ path: String, _ rows: [CatalogRow]) -> CatalogRow? {
    for row in rows {
        if row.path == path {
            return row
        }
    }
    return nil
}

struct PairForm: View {
    @State var address: String
    @State var code: String
    @State var fingerprint: String
    @State var name: String
    let busy: Bool
    let failed: Bool
    let onScan: () -> Void
    let onBack: () -> Void
    let onSubmit: (String, String, String, String) -> Void

    var body: some View {
        ProductScreen(
            title: String(localized: "Join on this device", bundle: .copy, locale: .copy),
            backLabel: String(localized: "Back", bundle: .copy, locale: .copy),
            backDisabled: busy,
            onBack: onBack
        ) {
            Text(String(localized: "The desktop shows the QR. The recovery password is not part of the invite.", bundle: .copy, locale: .copy))
                .font(.subheadline)
                .foregroundStyle(SatchelInk.secondary)
            ProductCard {
                ProductField(title: String(localized: "Address", bundle: .copy, locale: .copy), text: $address, suppressesAutocorrection: true)
                ProductField(title: String(localized: "Code", bundle: .copy, locale: .copy), text: $code)
                ProductField(
                    title: String(localized: "Fingerprint", bundle: .copy, locale: .copy),
                    text: $fingerprint,
                    suppressesAutocorrection: true
                )
                ProductField(title: String(localized: "Nickname", bundle: .copy, locale: .copy), text: $name)
            }
            if failed {
                Text(String(localized: "Couldn't join. Check the code and that the other device still shows the invite.", bundle: .copy, locale: .copy))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(SatchelInk.danger)
            }
            Button(action: onScan) {
                Text(String(localized: "Scan QR", bundle: .copy, locale: .copy))
                    .font(.body.weight(.semibold))
                    .foregroundStyle(SatchelInk.ink)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .background(SatchelInk.card)
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .stroke(SatchelInk.line, lineWidth: 1)
                    )
            }
            .buttonStyle(.plain)
            .disabled(busy)
            Button {
                onSubmit(address, code, fingerprint, name)
            } label: {
                Text(String(localized: "Join", bundle: .copy, locale: .copy))
                    .font(.body.weight(.semibold))
                    .foregroundStyle(SatchelInk.onAccent)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .background(SatchelInk.accent)
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
            .buttonStyle(.plain)
            .disabled(busy)
        }
    }
}

struct ScannerPage: View {
    let onCode: (String) -> Void
    let onBack: () -> Void

    var body: some View {
        ProductScreen(title: String(localized: "Scan QR", bundle: .copy, locale: .copy), backLabel: String(localized: "Back", bundle: .copy, locale: .copy), onBack: onBack) {
            Text(String(localized: "Point the camera at the QR on the desktop.", bundle: .copy, locale: .copy))
                .font(.subheadline)
                .foregroundStyle(SatchelInk.secondary)
            QrScannerView(onCode: onCode)
                .frame(minHeight: 320)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
    }
}

private struct WideButton: View {
    let title: String
    var primary: Bool = true
    var disabled: Bool = false
    let id: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.body.weight(.semibold))
                .foregroundStyle(primary ? SatchelInk.onAccent : SatchelInk.ink)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(primary ? SatchelInk.accent : SatchelInk.card)
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .stroke(primary ? Color.clear : SatchelInk.line, lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .accessibilityIdentifier(id)
    }
}

struct CreatePage: View {
    let draft: CreateDraft
    let onChange: (CreateDraft) -> Void
    let onBack: () -> Void
    let onSubmit: () -> Void

    private func bind(_ keyPath: WritableKeyPath<CreateDraft, String>) -> Binding<String> {
        Binding(
            get: { draft[keyPath: keyPath] },
            set: { value in
                var next = draft
                next[keyPath: keyPath] = value
                next.failed = false
                onChange(next)
            }
        )
    }

    var body: some View {
        ProductScreen(
            title: String(localized: "Create repository", bundle: .copy, locale: .copy),
            backLabel: String(localized: "Back", bundle: .copy, locale: .copy),
            backDisabled: draft.busy,
            onBack: onBack
        ) {
            Text(
                String(localized: "This device will keep the recovery password and the maintenance credential. The other device joins with a code and does not type the account again.", bundle: .copy, locale: .copy)
            )
            .font(.subheadline)
            .foregroundStyle(SatchelInk.secondary)
            HStack(spacing: 8) {
                ForEach([("folder", String(localized: "Folder", bundle: .copy, locale: .copy)), ("s3", "S3"), ("r2", "R2"), ("b2", "B2")], id: \.0) { item in
                    Button {
                        var next = draft
                        next.storage = item.0
                        next.failed = false
                        onChange(next)
                    } label: {
                        Text(item.1)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(SatchelInk.ink)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 10)
                            .background(draft.storage == item.0 ? SatchelInk.selected : SatchelInk.card)
                            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .disabled(draft.busy)
                    .accessibilityIdentifier("storage-\(item.0)")
                    .accessibilityAddTraits(draft.storage == item.0 ? .isSelected : [])
                }
            }
            if draft.storage == "folder" {
                Text(String(localized: "The folder stays on this device only. It has no object versioning.", bundle: .copy, locale: .copy))
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            } else {
                Text(
                    String(localized: "The storage needs versioning turned on and no lifecycle rule that deletes objects. The device key is only limited to deleting its own locks if the storage applies a per-key policy (AWS S3, MinIO, RustFS). On R2 and B2 that key can delete everything.", bundle: .copy, locale: .copy)
                )
                .font(.subheadline)
                .foregroundStyle(SatchelInk.secondary)
                ProductCard {
                    ProductField(title: "Endpoint", text: bind(\.endpoint), suppressesAutocorrection: true)
                    ProductField(title: "Bucket", text: bind(\.bucket), suppressesAutocorrection: true)
                    ProductField(title: String(localized: "Region", bundle: .copy, locale: .copy), text: bind(\.region), suppressesAutocorrection: true)
                    ProductField(title: String(localized: "Prefix (can be empty)", bundle: .copy, locale: .copy), text: bind(\.prefix), suppressesAutocorrection: true)
                    ProductField(title: String(localized: "Maintenance access key", bundle: .copy, locale: .copy), text: bind(\.accessKeyId), suppressesAutocorrection: true)
                    ProductField(title: String(localized: "Maintenance secret", bundle: .copy, locale: .copy), text: bind(\.secretAccessKey), suppressesAutocorrection: true, secure: true)
                    ProductField(title: String(localized: "Device access key", bundle: .copy, locale: .copy), text: bind(\.deviceAccessKeyId), suppressesAutocorrection: true)
                    ProductField(title: String(localized: "Device secret", bundle: .copy, locale: .copy), text: bind(\.deviceSecretAccessKey), suppressesAutocorrection: true, secure: true)
                }
            }
            if draft.failed {
                Text(createFailureCopy(draft.reason))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(SatchelInk.danger)
            }
            WideButton(
                title: String(localized: "Create", bundle: .copy, locale: .copy),
                disabled: draft.busy || !draft.complete,
                id: "create-submit",
                action: onSubmit
            )
        }
    }
}

struct RecoveryChangePage: View {
    let busy: Bool
    let failed: Bool
    let onChange: () -> Void
    let onBack: () -> Void

    var body: some View {
        ProductScreen(title: String(localized: "Change recovery password", bundle: .copy, locale: .copy)) {
            Text(
                String(localized: "Makes a new password and shows it once. The other devices keep opening with their own key. Anyone who kept the old key file, or reads old versions in a bucket with versioning, still opens with the old password: the data key does not change.", bundle: .copy, locale: .copy)
            )
            .font(.subheadline)
            .foregroundStyle(SatchelInk.secondary)
            if failed {
                Text(String(localized: "Couldn't change it. The old password still works. Check the connection to the storage.", bundle: .copy, locale: .copy))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(SatchelInk.danger)
                    .accessibilityIdentifier("recovery-change-failed")
            }
            WideButton(title: busy ? String(localized: "Changing…", bundle: .copy, locale: .copy) : String(localized: "Change now", bundle: .copy, locale: .copy), id: "recovery-change-now", action: onChange)
                .disabled(busy)
            WideButton(title: String(localized: "Back", bundle: .copy, locale: .copy), id: "recovery-change-back", action: onBack)
                .disabled(busy)
        }
    }
}

struct RecoveryPage: View {
    let password: String
    let note: String?
    let onDone: () -> Void

    var body: some View {
        ProductScreen(title: String(localized: "Recovery password", bundle: .copy, locale: .copy)) {
            Text(
                String(localized: "Keep this password in a password manager that opens without this repository. It shows only now, and official restic uses this password.", bundle: .copy, locale: .copy)
            )
            .font(.subheadline)
            .foregroundStyle(SatchelInk.secondary)
            if let note {
                Text(note)
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            ProductCard {
                Text(password)
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(SatchelInk.ink)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("recovery-password")
            }
            WideButton(title: String(localized: "I saved it", bundle: .copy, locale: .copy), id: "recovery-done", action: onDone)
        }
    }
}

struct HoldPage: View {
    let state: PhoneModel.HoldState
    let onApprove: () -> Void
    let onDeny: () -> Void
    let onBack: () -> Void

    var body: some View {
        ProductScreen(
            title: String(localized: "Invite", bundle: .copy, locale: .copy),
            backLabel: String(localized: "Back", bundle: .copy, locale: .copy),
            backDisabled: state.busy,
            onBack: onBack
        ) {
            if let approval = state.approval {
                ProductCard {
                    Text(String(localized: "Device \(approval.deviceName)", bundle: .copy, locale: .copy))
                    Text(String(localized: "System \(approval.os)", bundle: .copy, locale: .copy))
                    Text(String(localized: "Address \(approval.peerAddress)", bundle: .copy, locale: .copy))
                    Text(String(localized: "Code \(approval.code)", bundle: .copy, locale: .copy))
                }
                .foregroundStyle(SatchelInk.ink)
                if state.failed {
                    Text(String(localized: "Couldn't approve. Close the invite and open it again.", bundle: .copy, locale: .copy))
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(SatchelInk.danger)
                }
                WideButton(title: String(localized: "Approve", bundle: .copy, locale: .copy), disabled: state.busy, id: "hold-approve", action: onApprove)
                WideButton(title: String(localized: "Deny", bundle: .copy, locale: .copy), primary: false, disabled: state.busy, id: "hold-deny", action: onDeny)
            } else {
                Text(String(localized: "Waiting for the other device to enter the code.", bundle: .copy, locale: .copy))
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
                ProductCard {
                    Text(String(localized: "Code \(state.offer.code)", bundle: .copy, locale: .copy))
                        .accessibilityIdentifier("hold-code")
                    Text(String(localized: "Address \(state.offer.address)", bundle: .copy, locale: .copy))
                        .accessibilityIdentifier("hold-address")
                    Text(String(localized: "Fingerprint \(state.offer.fingerprint)", bundle: .copy, locale: .copy))
                        .accessibilityIdentifier("hold-fingerprint")
                }
                .foregroundStyle(SatchelInk.ink)
                if let data = state.qr, let image = UIImage(data: data) {
                    Image(uiImage: image)
                        .interpolation(.none)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 240, height: 240)
                        .accessibilityLabel(String(localized: "Invite QR", bundle: .copy, locale: .copy))
                }
                Text(String(localized: "The recovery password is not in the invite.", bundle: .copy, locale: .copy))
                    .font(.footnote)
                    .foregroundStyle(SatchelInk.secondary)
            }
        }
    }
}

func createFailureCopy(_ reason: String) -> String {
    switch reason {
    case "protection":
        return String(localized: "The storage refused: turn on bucket versioning and remove lifecycle rules that delete objects.", bundle: .copy, locale: .copy)
    case "denied":
        return String(localized: "The storage did not let this key check versioning. Give it admin permission on the bucket. R2 does not expose versioning through the S3 API and is always refused.", bundle: .copy, locale: .copy)
    case "exists":
        return String(localized: "There is already a repository here. Choose another prefix, or join it with a code.", bundle: .copy, locale: .copy)
    case "unreachable":
        return String(localized: "Couldn't reach the storage. Check the endpoint, region and keys.", bundle: .copy, locale: .copy)
    case "fields":
        return String(localized: "Check the fields. The device key must be different from the maintenance key.", bundle: .copy, locale: .copy)
    default:
        return String(localized: "Couldn't create. Check the fields, versioning and lifecycle.", bundle: .copy, locale: .copy)
    }
}
