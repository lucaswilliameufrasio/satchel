import SwiftUI

@main
struct SatchelApp: App {
    @StateObject private var model = PhoneModel()

    var body: some Scene {
        WindowGroup {
            PhoneView(model: model)
                .onAppear {
                    model.openIfNeeded()
                }
        }
    }
}

struct PhoneView: View {
    @ObservedObject var model: PhoneModel

    var body: some View {
        NavigationStack {
            switch model.screen {
            case .loading:
                StatusPage(title: "Catálogo", message: "Lendo o catálogo…")
            case .failed:
                StatusPage(title: "Catálogo", message: "Não foi possível abrir o catálogo.")
            case .ready(let rows, _):
                CatalogList(rows: rows, canPair: model.canPair, onPair: {
                    model.showPairing()
                }, onOpen: { path in
                    model.openFile(path: path)
                })
            case .file(let path, let rows, let busy):
                FilePage(
                    path: path,
                    rows: rows,
                    busy: busy,
                    localDeviceId: model.catalogDeviceId,
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
}

private struct StatusPage: View {
    let title: String
    let message: String

    var body: some View {
        Text(message)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding()
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.large)
    }
}

private struct CatalogList: View {
    let rows: [CatalogRow]
    let canPair: Bool
    let onPair: () -> Void
    let onOpen: (String) -> Void

    var body: some View {
        List {
            if canPair {
                Section {
                    Button(action: onPair) {
                        HStack(alignment: .center, spacing: 12) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Entrar neste aparelho")
                                    .font(.headline)
                                    .foregroundStyle(.primary)
                                Text("Ler o QR mostrado no desktop, ou informar o convite.")
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 12)
                            Image(systemName: "chevron.right")
                                .font(.footnote.weight(.semibold))
                                .foregroundStyle(.tertiary)
                        }
                        .padding(.vertical, 4)
                    }
                    .buttonStyle(.plain)
                }
            }
            Section {
                if rows.isEmpty {
                    Text("Nenhum arquivo no catálogo.")
                        .foregroundStyle(.secondary)
                }
                ForEach(rows) { row in
                    Button {
                        onOpen(row.path)
                    } label: {
                        CatalogRowLabel(row: row)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .navigationTitle("Catálogo")
        .navigationBarTitleDisplayMode(.large)
    }
}

private struct CatalogRowLabel: View {
    let row: CatalogRow

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: row.isDirectory ? "folder" : "doc")
                .font(.title3)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
                .frame(width: 28, alignment: .center)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text(row.path)
                        .font(.headline)
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(followLabel(row.follow))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .layoutPriority(1)
                }
                Text(catalogMeta(row))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Text(deviceCountLabel(row.devices))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }
}

private struct FilePage: View {
    let path: String
    let rows: [CatalogRow]
    let busy: Bool
    let localDeviceId: String
    let onBack: () -> Void
    let onChoose: (String) -> Void

    var body: some View {
        List {
            if let row = listedRow(path, rows) {
                Section {
                    LabeledContent("Tamanho") {
                        Text(row.isDirectory ? "Pasta" : byteCount(row.size))
                    }
                    LabeledContent("Sincronizado") {
                        Text(syncedWhen(row.versionUnix))
                    }
                }
                Section("Neste aparelho") {
                    stateButton("Ver", "see", row: row)
                    stateButton("Baixar agora", "once", row: row)
                    stateButton("Sincronizar sempre", "always", row: row)
                    if busy {
                        Text("Salvando a escolha…")
                            .foregroundStyle(.secondary)
                    }
                }
                Section("Aparelhos") {
                    if row.devices.isEmpty {
                        Text("Nenhum aparelho mantém cópia.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(Array(row.devices.enumerated()), id: \.offset) { _, device in
                        DeviceFacts(
                            deviceId: device,
                            card: holderCard(device, row.holders),
                            local: device == localDeviceId
                        )
                    }
                }
            } else {
                Text("Nenhum arquivo no catálogo.")
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle(path)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Catálogo", action: onBack)
                    .disabled(busy)
            }
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
                        .foregroundStyle(.primary)
                    Text(followDetail(follow, directory: row.isDirectory))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 12)
                if selected {
                    Image(systemName: "checkmark")
                        .foregroundStyle(.tint)
                        .accessibilityHidden(true)
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

private func holderCard(_ id: String, _ cards: [DeviceCard]) -> DeviceCard? {
    for card in cards {
        if card.id == id {
            return card
        }
    }
    return nil
}

private struct DeviceFacts: View {
    let deviceId: String
    let card: DeviceCard?
    let local: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
            if let name = card?.name, title != name {
                Text("Apelido \(name)")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if let os = card?.os {
                Text("Sistema \(os)")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if let enrolled = card?.enrolledUnix {
                Text("Registrado \(syncedWhen(enrolled))")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if let last = card?.lastUnix {
                Text("Último envio \(syncedWhen(last))")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if title != groupedDeviceId(deviceId) {
                Text(groupedDeviceId(deviceId))
                    .font(.footnote.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }

    private var title: String {
        if local {
            return "Este aparelho"
        }
        if let name = card?.name, !name.isEmpty {
            return name
        }
        return groupedDeviceId(deviceId)
    }
}

private func listedRow(_ path: String, _ rows: [CatalogRow]) -> CatalogRow? {
    for row in rows {
        if row.path == path {
            return row
        }
    }
    return nil
}

private struct PairForm: View {
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
        Form {
            Section {
                Text("O desktop mostra o QR. A senha de recuperação não faz parte do convite.")
                    .foregroundStyle(.secondary)
            }
            Section {
                TextField("Endereço", text: $address)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Código", text: $code)
                TextField("Impressão digital", text: $fingerprint)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Apelido", text: $name)
            }
            if failed {
                Section {
                    Text("Não foi possível entrar.")
                }
            }
            Section {
                Button("Ler QR", action: onScan)
                Button("Entrar") {
                    onSubmit(address, code, fingerprint, name)
                }
            }
        }
        .disabled(busy)
        .navigationTitle("Entrar neste aparelho")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Voltar", action: onBack)
                    .disabled(busy)
            }
        }
    }
}

private struct ScannerPage: View {
    let onCode: (String) -> Void
    let onBack: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Aponte a câmera para o QR do desktop.")
                .padding(.horizontal)
            QrScannerView(onCode: onCode)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle("Ler QR")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Voltar", action: onBack)
            }
        }
    }
}
