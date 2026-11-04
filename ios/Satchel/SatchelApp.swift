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
        switch model.screen {
        case .loading:
            StatusPage(title: "Catálogo", message: "Lendo o catálogo…")
        case .failed:
            StatusPage(title: "Catálogo", message: "Não foi possível abrir o catálogo.")
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

struct StatusPage: View {
    let title: String
    let message: String

    var body: some View {
        ProductScreen(title: title) {
            Text(message)
                .font(.body)
                .foregroundStyle(SatchelInk.secondary)
                .multilineTextAlignment(.center)
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
    let onPair: () -> Void
    let onOpen: (String) -> Void

    var body: some View {
        ProductScreen(title: "Catálogo") {
            if canCreate {
                actionCard(
                    "Criar repositório",
                    "Este aparelho guarda a senha de recuperação. O outro entra com um código.",
                    id: "create-repository",
                    action: onCreate
                )
            }
            if canInvite {
                actionCard(
                    "Mostrar convite",
                    "A senha de recuperação não vai no convite.",
                    id: "show-invite",
                    action: onInvite
                )
                actionCard(
                    "Trocar senha de recuperação",
                    "Só este aparelho, que criou o repositório, pode trocar.",
                    id: "change-recovery",
                    action: onChangeRecovery
                )
            }
            if canPair {
                Button(action: onPair) {
                    ProductCard {
                        HStack(alignment: .center, spacing: 12) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("Entrar neste aparelho")
                                    .font(.headline)
                                    .foregroundStyle(SatchelInk.ink)
                                Text("Ler o QR mostrado no desktop, ou informar o convite.")
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
                    Text("Nenhum arquivo no catálogo.")
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
        ProductScreen(title: path, backLabel: "Catálogo", backDisabled: busy, onBack: onBack) {
            if let row = listedRow(path, rows) {
                ProductCard {
                    factLine("Tamanho", row.isDirectory ? "Pasta" : byteCount(row.size))
                    factLine("Sincronizado", syncedWhen(row.versionUnix, zone: zone))
                }
                ProductCard {
                    Text("Neste aparelho")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(SatchelInk.secondary)
                    stateButton("Ver", "see", row: row)
                    stateButton("Baixar agora", "once", row: row)
                    stateButton("Sincronizar sempre", "always", row: row)
                    if busy {
                        Text("Salvando a escolha…")
                            .font(.subheadline)
                            .foregroundStyle(SatchelInk.secondary)
                    }
                }
                ProductCard {
                    Text("Aparelhos")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(SatchelInk.secondary)
                    if row.devices.isEmpty {
                        Text("Nenhum aparelho mantém cópia.")
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
                    Text("Nenhum arquivo no catálogo.")
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
                Text("Apelido \(name)")
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if let os = card?.os {
                Text("Sistema \(os)")
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if let enrolled = card?.enrolledUnix {
                Text("Registrado \(syncedWhen(enrolled, zone: zone))")
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            }
            if let last = card?.lastUnix {
                Text("Último envio \(syncedWhen(last, zone: zone))")
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
            return "Este aparelho"
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
            title: "Entrar neste aparelho",
            backLabel: "Voltar",
            backDisabled: busy,
            onBack: onBack
        ) {
            Text("O desktop mostra o QR. A senha de recuperação não faz parte do convite.")
                .font(.subheadline)
                .foregroundStyle(SatchelInk.secondary)
            ProductCard {
                ProductField(title: "Endereço", text: $address, suppressesAutocorrection: true)
                ProductField(title: "Código", text: $code)
                ProductField(
                    title: "Impressão digital",
                    text: $fingerprint,
                    suppressesAutocorrection: true
                )
                ProductField(title: "Apelido", text: $name)
            }
            if failed {
                Text("Não foi possível entrar.")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(SatchelInk.accent)
            }
            Button(action: onScan) {
                Text("Ler QR")
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
                Text("Entrar")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Color.white)
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
        ProductScreen(title: "Ler QR", backLabel: "Voltar", onBack: onBack) {
            Text("Aponte a câmera para o QR do desktop.")
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
                .foregroundStyle(primary ? Color.white : SatchelInk.ink)
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
            title: "Criar repositório",
            backLabel: "Voltar",
            backDisabled: draft.busy,
            onBack: onBack
        ) {
            Text(
                "Este aparelho passa a guardar a senha de recuperação e a credencial de manutenção. O outro aparelho entra com um código e não digita a conta de novo."
            )
            .font(.subheadline)
            .foregroundStyle(SatchelInk.secondary)
            HStack(spacing: 8) {
                ForEach([("folder", "Pasta"), ("s3", "S3"), ("r2", "R2"), ("b2", "B2")], id: \.0) { item in
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
                Text("A pasta fica só neste aparelho. Ela não tem versionamento de objeto.")
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
            } else {
                Text(
                    "O armazenamento precisa ter versionamento ligado e nenhum ciclo de vida que apague objeto. A chave dos aparelhos só fica limitada a apagar os próprios locks se o armazenamento aplicar política por chave (S3 da AWS, MinIO, RustFS). No R2 e no B2 essa chave pode apagar tudo."
                )
                .font(.subheadline)
                .foregroundStyle(SatchelInk.secondary)
                ProductCard {
                    ProductField(title: "Endpoint", text: bind(\.endpoint), suppressesAutocorrection: true)
                    ProductField(title: "Bucket", text: bind(\.bucket), suppressesAutocorrection: true)
                    ProductField(title: "Região", text: bind(\.region), suppressesAutocorrection: true)
                    ProductField(title: "Prefixo (pode ficar vazio)", text: bind(\.prefix), suppressesAutocorrection: true)
                    ProductField(title: "Chave de acesso de manutenção", text: bind(\.accessKeyId), suppressesAutocorrection: true)
                    ProductField(title: "Segredo de manutenção", text: bind(\.secretAccessKey), suppressesAutocorrection: true, secure: true)
                    ProductField(title: "Chave de acesso dos aparelhos", text: bind(\.deviceAccessKeyId), suppressesAutocorrection: true)
                    ProductField(title: "Segredo dos aparelhos", text: bind(\.deviceSecretAccessKey), suppressesAutocorrection: true, secure: true)
                }
            }
            if draft.failed {
                Text(createFailureCopy(draft.reason))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(SatchelInk.accent)
            }
            WideButton(
                title: "Criar",
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
        ProductScreen(title: "Trocar senha de recuperação") {
            Text(
                "Gera uma senha nova e a mostra uma vez. Os outros aparelhos continuam abrindo com a chave deles. Quem guardou o arquivo de chave antigo, ou lê versões antigas num bucket com versionamento, ainda abre com a senha velha: a chave dos dados não muda."
            )
            .font(.subheadline)
            .foregroundStyle(SatchelInk.secondary)
            if failed {
                Text("Não foi possível trocar. A senha antiga continua valendo. Confira a conexão com o armazenamento.")
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.ink)
                    .accessibilityIdentifier("recovery-change-failed")
            }
            WideButton(title: busy ? "Trocando…" : "Trocar agora", id: "recovery-change-now", action: onChange)
                .disabled(busy)
            WideButton(title: "Voltar", id: "recovery-change-back", action: onBack)
                .disabled(busy)
        }
    }
}

struct RecoveryPage: View {
    let password: String
    let note: String?
    let onDone: () -> Void

    var body: some View {
        ProductScreen(title: "Senha de recuperação") {
            Text(
                "Guarde esta senha num gerenciador de senhas que abre sem este repositório. Ela aparece só agora e o restic oficial usa esta senha."
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
            WideButton(title: "Guardei", id: "recovery-done", action: onDone)
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
            title: "Convite",
            backLabel: "Voltar",
            backDisabled: state.busy,
            onBack: onBack
        ) {
            if let approval = state.approval {
                ProductCard {
                    Text("Aparelho \(approval.deviceName)")
                    Text("Sistema \(approval.os)")
                    Text("Endereço \(approval.peerAddress)")
                    Text("Código \(approval.code)")
                }
                .foregroundStyle(SatchelInk.ink)
                if state.failed {
                    Text("Não foi possível aprovar.")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(SatchelInk.accent)
                }
                WideButton(title: "Aprovar", disabled: state.busy, id: "hold-approve", action: onApprove)
                WideButton(title: "Negar", primary: false, disabled: state.busy, id: "hold-deny", action: onDeny)
            } else {
                Text("Esperando o outro aparelho digitar o código.")
                    .font(.subheadline)
                    .foregroundStyle(SatchelInk.secondary)
                ProductCard {
                    Text("Código \(state.offer.code)")
                        .accessibilityIdentifier("hold-code")
                    Text("Endereço \(state.offer.address)")
                        .accessibilityIdentifier("hold-address")
                    Text("Impressão digital \(state.offer.fingerprint)")
                        .accessibilityIdentifier("hold-fingerprint")
                }
                .foregroundStyle(SatchelInk.ink)
                if let data = state.qr, let image = UIImage(data: data) {
                    Image(uiImage: image)
                        .interpolation(.none)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 240, height: 240)
                        .accessibilityLabel("QR do convite")
                }
                Text("A senha de recuperação não vai no convite.")
                    .font(.footnote)
                    .foregroundStyle(SatchelInk.secondary)
            }
        }
    }
}

func createFailureCopy(_ reason: String) -> String {
    switch reason {
    case "protection":
        return "O armazenamento recusou: ligue o versionamento do bucket e remova regras de ciclo de vida que apagam objetos."
    case "denied":
        return "O armazenamento não deixou esta chave consultar o versionamento. Dê a ela permissão de administrador no bucket. O R2 não expõe versionamento pela API S3 e será sempre recusado."
    case "exists":
        return "Já existe um repositório neste lugar. Escolha outro prefixo, ou entre nele com um código."
    case "unreachable":
        return "Não consegui falar com o armazenamento. Confira endpoint, região e as chaves."
    case "fields":
        return "Confira os campos. A chave dos aparelhos precisa ser diferente da de manutenção."
    default:
        return "Não foi possível criar. Confira os campos, o versionamento e o ciclo de vida."
    }
}
