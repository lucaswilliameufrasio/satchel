import SwiftUI
import UIKit
import XCTest

@testable import Satchel

@MainActor
final class ProductSurfaceTests: XCTestCase {
    private let instant: Int64 = 1_700_000_000
    private let reader = "fedcba9876543210fedcba9876543210"
    private let publisher = "0123456789abcdef0123456789abcdef"
    private var trace: [String] = []

    func testExplicitZonesKeepTheSampleClock() {
        let minus3 = TimeZone(secondsFromGMT: -3 * 60 * 60)!
        let utc = TimeZone(identifier: "UTC")!
        let minusText = syncedWhen(instant, zone: minus3)
        let utcText = syncedWhen(instant, zone: utc)
        proof("utc-minus-3 \(minusText)")
        proof("utc \(utcText)")
        XCTAssertTrue(minusText.hasPrefix("14/11/2023 19:13 "))
        XCTAssertFalse(minusText.contains("22:13"))
        XCTAssertFalse(minusText.hasSuffix("UTC"))
        XCTAssertEqual(utcText, "14/11/2023 22:13 UTC")
        XCTAssertNotEqual(shortZoneName(minus3, at: Date(timeIntervalSince1970: TimeInterval(instant))), "UTC")
    }

    func testCatalogFileAndEntryAreCustomProductSurfaces() {
        let minus3 = TimeZone(secondsFromGMT: -3 * 60 * 60)!
        let when = syncedWhen(instant, zone: minus3)
        let file = keptRow(follow: "see", devices: [publisher], holders: [publisherCard()])
        let folder = CatalogRow(
            path: "notes",
            isDirectory: true,
            size: 0,
            versionUnix: instant,
            devices: [],
            holders: [],
            follow: "see"
        )

        let catalog = render(
            CatalogList(
                rows: [file, folder],
                canPair: true,
                zone: minus3,
                onPair: {},
                onOpen: { _ in }
            )
        )
        proof("product-surface \(catalog.values.joined(separator: ","))")
        proof("identifiers \(catalog.identifiers.joined(separator: ","))")
        let container = catalog.values.contains(ProductSurface.container) ? ProductSurface.container : "missing"
        proof("not-a-grouped-list \(catalog.hasSystemList ? "failed" : container)")
        assertProductSurface(catalog)
        XCTAssertTrue(catalog.identifiers.contains(ProductSurface.mark))
        let joined = catalog.labels.joined(separator: "\n")
        XCTAssertTrue(joined.contains("Catálogo"))
        XCTAssertTrue(joined.contains("Entrar neste aparelho"))
        XCTAssertTrue(joined.contains("Ler o QR mostrado no desktop, ou informar o convite."))
        XCTAssertTrue(joined.contains("kept.txt"))
        XCTAssertTrue(joined.contains("13 B · \(when)"))
        XCTAssertTrue(joined.contains("1 aparelho"))
        XCTAssertTrue(joined.contains("Ver"))
        XCTAssertTrue(joined.contains("Pasta · \(when)"))
        XCTAssertTrue(joined.contains("Nenhum aparelho"))
        XCTAssertFalse(joined.contains("0 B"))
        XCTAssertFalse(joined.contains("2023-11-14T22:13:20Z"))
        XCTAssertFalse(joined.contains(reader))
        XCTAssertFalse(joined.contains(publisher))
        XCTAssertNil(contiguousHex(in: joined))

        let empty = render(
            CatalogList(rows: [], canPair: false, zone: minus3, onPair: {}, onOpen: { _ in })
        )
        assertProductSurface(empty)
        XCTAssertTrue(empty.labels.joined(separator: "\n").contains("Nenhum arquivo no catálogo."))
        XCTAssertTrue(empty.identifiers.contains(ProductSurface.mark))

        let loading = render(StatusPage(title: "Catálogo", message: "Lendo o catálogo…"))
        assertProductSurface(loading)
        XCTAssertTrue(loading.labels.joined(separator: "\n").contains("Lendo o catálogo…"))
        let failed = render(StatusPage(title: "Catálogo", message: "Não foi possível abrir o catálogo."))
        assertProductSurface(failed)
        XCTAssertTrue(failed.labels.joined(separator: "\n").contains("Não foi possível abrir o catálogo."))

        let openFile = render(filePage(file, busy: false, zone: minus3))
        assertProductSurface(openFile)
        let fileText = openFile.labels.joined(separator: "\n")
        XCTAssertTrue(fileText.contains("kept.txt"))
        XCTAssertTrue(fileText.contains("13 B"))
        XCTAssertFalse(fileText.contains("0 B"))
        XCTAssertTrue(fileText.contains(when))
        XCTAssertTrue(fileText.contains("Ver"))
        XCTAssertTrue(fileText.contains("Baixar agora"))
        XCTAssertTrue(fileText.contains("Sincronizar sempre"))
        XCTAssertTrue(fileText.contains("O conteúdo fica no storage. Este aparelho não baixa."))
        XCTAssertTrue(fileText.contains("Traz o arquivo uma vez e não acompanha mudanças posteriores."))
        XCTAssertTrue(fileText.contains("Baixa, envia e acompanha."))
        XCTAssertTrue(fileText.contains("Publicador"))
        XCTAssertTrue(fileText.contains("Sistema exemplo"))
        XCTAssertTrue(fileText.contains("Registrado \(when)"))
        XCTAssertTrue(fileText.contains("Último envio \(when)"))
        XCTAssertTrue(openFile.selected.contains { $0.contains("Ver") })
        XCTAssertFalse(openFile.selected.contains { $0.contains("Baixar agora") })
        XCTAssertEqual(
            firstIndexes(of: ["Ver", "Baixar agora", "Sincronizar sempre"], in: openFile.labels),
            [true]
        )

        let folderPage = render(filePage(folder, busy: false, zone: minus3))
        let folderText = folderPage.labels.joined(separator: "\n")
        XCTAssertTrue(folderText.contains("Pasta"))
        XCTAssertFalse(folderText.contains("0 B"))
        XCTAssertTrue(folderText.contains("Nenhum aparelho mantém cópia."))
        XCTAssertTrue(
            folderText.contains(
                "Baixa, envia e acompanha. Arquivo novo dentro desta pasta entra nesse estado."
            )
        )

        let downloaded = keptRow(
            follow: "once",
            devices: [publisher, reader],
            holders: [
                publisherCard(),
                DeviceCard(
                    id: reader,
                    name: nil,
                    os: nil,
                    enrolledUnix: nil,
                    lastUnix: instant
                ),
            ]
        )
        let after = render(filePage(downloaded, busy: true, zone: minus3))
        assertProductSurface(after)
        let afterText = after.labels.joined(separator: "\n")
        XCTAssertTrue(afterText.contains("Salvando a escolha…"))
        XCTAssertTrue(after.selected.contains { $0.contains("Baixar agora") })
        XCTAssertFalse(after.selected.contains { $0.contains("Ver") && !$0.contains("Baixar agora") })
        let local = cardText(identifier: "device-card-local", dump: after)
        proof("local-card \(local.joined(separator: " | "))")
        XCTAssertTrue(local.contains { $0.contains("Este aparelho") })
        XCTAssertTrue(local.contains { $0.contains("Último envio \(when)") })
        XCTAssertTrue(local.contains { $0.contains(groupedDeviceId(reader)) })
        XCTAssertFalse(local.contains { $0.contains("Apelido") })
        XCTAssertFalse(local.contains { $0.contains("Sistema") })
        XCTAssertFalse(local.contains { $0.contains("Registrado") })
        let other = cardText(identifier: "device-card-other", dump: after)
        XCTAssertTrue(other.contains { $0.contains("Publicador") })
        XCTAssertTrue(other.contains { $0.contains("Sistema exemplo") })

        var submitted = false
        let entry = render(
            PairForm(
                address: "127.0.0.1:9",
                code: "482193",
                fingerprint: String(repeating: "ab", count: 32),
                name: "iPhone",
                busy: false,
                failed: true,
                onScan: {},
                onBack: {},
                onSubmit: { _, _, _, _ in
                    submitted = true
                }
            )
        )
        assertProductSurface(entry)
        XCTAssertFalse(submitted)
        let entryText = entry.labels.joined(separator: "\n")
        XCTAssertTrue(entryText.contains("Entrar neste aparelho"))
        XCTAssertTrue(
            entryText.contains("O desktop mostra o QR. A senha de recuperação não faz parte do convite.")
        )
        XCTAssertTrue(entryText.contains("Não foi possível entrar."))
        XCTAssertTrue(entryText.contains("Ler QR"))
        XCTAssertTrue(entryText.contains("Entrar"))
        XCTAssertEqual(
            firstIndexes(
                of: ["Endereço", "Código", "Impressão digital", "Apelido"],
                in: entry.labels
            ),
            [true]
        )
        let fieldOrder = ["Endereço", "Código", "Impressão digital", "Apelido"]
        let foundFields = entry.labels.filter { fieldOrder.contains($0) }
        proof("entry-fields \(foundFields.joined(separator: " "))")
    }

    func testReadingAQrFillsTheFieldsAndDoesNotSubmit() async {
        let raw = """
        {"v":1,"code":"482193","address":"127.0.0.1:9","fingerprint":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}
        """
        let fields = SatchelCore.parseInvite(raw)
        XCTAssertEqual(fields?.address, "127.0.0.1:9")
        XCTAssertEqual(fields?.code, "482193")
        XCTAssertEqual(
            fields?.fingerprint,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        )
        let model = PhoneModel()
        model.applyScan(raw, previous: ("", "", "", "iPhone"))
        let deadline = Date().addingTimeInterval(5)
        var filled = false
        while Date() < deadline {
            if case .pairing(let address, let code, let fingerprint, let name, let busy, let failed) =
                model.screen
            {
                if address == "127.0.0.1:9"
                    && code == "482193"
                    && fingerprint == fields?.fingerprint
                    && name == "iPhone"
                    && !busy
                    && !failed
                {
                    filled = true
                    break
                }
            }
            if case .ready = model.screen {
                XCTFail("reading the QR submitted the invite")
                return
            }
            try? await Task.sleep(nanoseconds: 50_000_000)
        }
        XCTAssertTrue(filled)
        proof("qr-fills-without-submit")
    }

    func testFolderSummaryMatchesTheAndroidCopy() {
        XCTAssertEqual(folderSummary(nil), folderSyncFailed)
        XCTAssertEqual(folderSummary(#"{"status":"unavailable"}"#), folderUnavailable)
        XCTAssertEqual(
            folderSummary(#"{"status":"ok","imported":0,"exported":0,"deletedHere":0,"deletedThere":0,"deferred":0,"conflicts":0}"#),
            "Em dia."
        )
        XCTAssertEqual(
            folderSummary(#"{"status":"ok","imported":1,"exported":1,"deletedHere":0,"deletedThere":1,"deferred":0,"conflicts":1}"#),
            "1 mudança enviada, 2 mudanças recebidas, 1 conflito guardado no catálogo."
        )
        proof("folder-summary")
    }

    func testSampleKeepsApelidoOutOfTheKeyFile() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("satchel-product-\(UUID().uuidString)", isDirectory: true)
        defer {
            try? FileManager.default.removeItem(at: root)
        }
        let rows = SatchelCore.sampleCatalog(root: root.path)
        XCTAssertNotNil(rows)
        let keys = root.appendingPathComponent("repository/keys", isDirectory: true)
        let files = try FileManager.default.contentsOfDirectory(
            at: keys,
            includingPropertiesForKeys: nil
        )
        XCTAssertFalse(files.isEmpty)
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            XCTAssertTrue(text.contains(publisher))
            XCTAssertFalse(text.contains("Publicador"))
            XCTAssertFalse(text.contains("exemplo"))
            XCTAssertFalse(text.contains("\"created\""))
        }
        proof("keys-omit-apelido")
    }

    private func publisherCard() -> DeviceCard {
        DeviceCard(
            id: publisher,
            name: "Publicador",
            os: "exemplo",
            enrolledUnix: instant,
            lastUnix: instant
        )
    }

    private func keptRow(follow: String, devices: [String], holders: [DeviceCard]) -> CatalogRow {
        CatalogRow(
            path: "kept.txt",
            isDirectory: false,
            size: 13,
            versionUnix: instant,
            devices: devices,
            holders: holders,
            follow: follow
        )
    }

    private func filePage(_ row: CatalogRow, busy: Bool, zone: TimeZone) -> some View {
        FilePage(
            path: row.path,
            rows: [row],
            busy: busy,
            localDeviceId: reader,
            zone: zone,
            onBack: {},
            onChoose: { _ in }
        )
    }

    private func assertProductSurface(_ dump: SurfaceDump, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(dump.identifiers.contains(ProductSurface.identifier), dump.identifiers.joined(separator: ","), file: file, line: line)
        XCTAssertTrue(dump.values.contains(ProductSurface.container), dump.values.joined(separator: ","), file: file, line: line)
        XCTAssertFalse(dump.hasSystemList, dump.listClasses.joined(separator: ","), file: file, line: line)
        XCTAssertFalse(dump.identifiers.contains("grouped-list"), file: file, line: line)
    }

    private func firstIndexes(of needles: [String], in labels: [String]) -> [Bool] {
        var previous = -1
        for needle in needles {
            var found: Int?
            for (index, label) in labels.enumerated() {
                if label == needle || label.hasPrefix(needle) {
                    found = index
                    break
                }
            }
            guard let found, found > previous else {
                return [false]
            }
            previous = found
        }
        return [true]
    }

    private func cardText(identifier: String, dump: SurfaceDump) -> [String] {
        dump.cards[identifier] ?? []
    }

    private func contiguousHex(in text: String) -> String? {
        guard let expression = try? NSRegularExpression(pattern: "[0-9a-fA-F]{32}") else {
            return nil
        }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = expression.firstMatch(in: text, range: range), let swiftRange = Range(match.range, in: text) else {
            return nil
        }
        return String(text[swiftRange])
    }

    private func render<V: View>(_ view: V) -> SurfaceDump {
        trace = []
        var dump = SurfaceDump()
        walk(view, context: WalkContext(), depth: 0, dump: &dump)
        let listClasses = hostedListClasses(view)
        if !listClasses.isEmpty {
            dump.hasSystemList = true
            dump.listClasses.append(contentsOf: listClasses)
        }
        if dump.labels.isEmpty {
            proof("view-tree-empty \(lastTypeName(String(describing: type(of: view))))")
            proof("outline \(trace.joined(separator: " | "))")
        }
        return dump
    }

    fileprivate func note(_ line: String) {
        if trace.count < 40 {
            trace.append(line)
        }
    }

    /// SwiftUI does not publish accessibility labels in a hosted unit test unless an
    /// accessibility client is connected. Read the shipped view value instead.
    /// `View` cannot be extended to add a protocol, so each concrete container below
    /// walks its own generic content.
    fileprivate func walk<V: View>(
        _ view: V,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        if depth > 80 {
            return
        }
        let name = String(describing: type(of: view))
        note("\(depth):\(lastTypeName(name))[\(mirrorLabels(view).joined(separator: ","))]")
        if isSystemListName(name) {
            dump.hasSystemList = true
            dump.listClasses.append(name)
            return
        }
        if isSatchelView(name) {
            walk(view.body, context: context, depth: depth + 1, dump: &dump)
            return
        }
        if let node = view as? any SatchelNode {
            node.satchelWalk(test: self, context: context, depth: depth, dump: &dump)
            return
        }
        walkAny(view, context: context, depth: depth, dump: &dump)
    }

    fileprivate func walkAny(
        _ value: Any,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        if depth > 80 {
            return
        }
        if let node = value as? any SatchelNode {
            node.satchelWalk(test: self, context: context, depth: depth, dump: &dump)
            return
        }
        let name = String(describing: type(of: value))
        if name.contains("->") || name.contains("Binding<") || lastTypeName(name) == "Binding" {
            return
        }
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional {
            if let child = mirror.children.first {
                walkAny(child.value, context: context, depth: depth + 1, dump: &dump)
            }
            return
        }
        if mirror.displayStyle == .tuple || mirror.displayStyle == .enum {
            for child in mirror.children {
                walkAny(child.value, context: context, depth: depth + 1, dump: &dump)
            }
            return
        }
        let last = lastTypeName(name)
        if last == "TupleContent" || last == "Tree" || last == "Pack" || last == "TupleView" {
            for child in mirror.children {
                let childName = String(describing: type(of: child.value))
                if childName.contains("->") || childName.contains("Binding") {
                    continue
                }
                walkAny(child.value, context: context, depth: depth + 1, dump: &dump)
            }
        }
    }

    fileprivate func stored<T>(_ sought: T.Type, in value: Any) -> T? {
        findStored(sought, in: value, depth: 0, skipSelf: true)
    }

    private func findStored<T>(_ sought: T.Type, in value: Any, depth: Int, skipSelf: Bool) -> T? {
        if depth > 6 {
            return nil
        }
        if !skipSelf, let found = value as? T {
            return found
        }
        let name = String(describing: Swift.type(of: value))
        if name.contains("->") || name.contains("Binding") {
            return nil
        }
        for child in Mirror(reflecting: value).children {
            if let found = findStored(sought, in: child.value, depth: depth + 1, skipSelf: false) {
                return found
            }
        }
        return nil
    }

    fileprivate func absorb(modifier: Any, context: inout WalkContext, dump: inout SurfaceDump) {
        let name = String(describing: type(of: modifier))
        if !name.contains("Accessibility") {
            return
        }
        harvestAccessibility(modifier, context: &context, dump: &dump, depth: 0)
        for item in quotedStorageStrings(String(reflecting: modifier)) {
            classifyAccessibilityString(item, context: &context, dump: &dump)
        }
    }

    private func harvestAccessibility(
        _ value: Any,
        context: inout WalkContext,
        dump: inout SurfaceDump,
        depth: Int
    ) {
        if depth > 16 {
            return
        }
        let name = String(describing: type(of: value))
        if lastTypeName(name) == "Text" || name.contains("LocalizedTextStorage") {
            for item in quotedStorageStrings(String(reflecting: value)) {
                classifyAccessibilityString(item, context: &context, dump: &dump)
            }
        }
        if let text = value as? String {
            classifyAccessibilityString(text, context: &context, dump: &dump)
            return
        }
        if let flag = value as? Bool, flag {
            let mirror = Mirror(reflecting: value)
            if mirror.children.isEmpty {
                return
            }
        }
        let mirror = Mirror(reflecting: value)
        for child in mirror.children {
            if child.label?.lowercased().contains("selected") == true {
                if let flag = child.value as? Bool, flag {
                    context.selected = true
                }
            }
            harvestAccessibility(child.value, context: &context, dump: &dump, depth: depth + 1)
        }
    }

    private func classifyAccessibilityString(
        _ text: String,
        context: inout WalkContext,
        dump: inout SurfaceDump
    ) {
        if text == ProductSurface.container {
            dump.values.append(text)
            return
        }
        if text == ProductSurface.identifier
            || text == ProductSurface.mark
            || text.hasPrefix("catalog-row-")
            || text.hasPrefix("follow-")
            || text.hasPrefix("device-card-")
        {
            dump.identifiers.append(text)
            if text == "device-card-local" || text == "device-card-other" {
                context.card = text
            }
            return
        }
        recordLabel(text, context: context, dump: &dump)
    }

    fileprivate func recordLabel(_ text: String, context: WalkContext, dump: inout SurfaceDump) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed.contains("ModifiedContent") || trimmed.contains("SwiftUI.") {
            return
        }
        dump.labels.append(trimmed)
        if context.selected {
            dump.selected.append(trimmed)
        }
        if let card = context.card {
            var lines = dump.cards[card] ?? []
            lines.append(trimmed)
            dump.cards[card] = lines
        }
    }

    fileprivate func containsCheckmark(_ value: Any, depth: Int) -> Bool {
        if depth > 30 {
            return false
        }
        if let text = value as? String {
            return text.contains("checkmark.circle.fill")
        }
        let name = String(describing: type(of: value))
        if name.contains("->") || name.contains("Binding") {
            return false
        }
        if name.contains("Image") || name.contains("Symbol") || name.contains("Provider") {
            if String(reflecting: value).contains("checkmark.circle.fill") {
                return true
            }
        }
        for child in Mirror(reflecting: value).children {
            if containsCheckmark(child.value, depth: depth + 1) {
                return true
            }
        }
        return false
    }

    fileprivate func quotedStorageStrings(_ reflected: String) -> [String] {
        var found: [String] = []
        appendQuoted(after: ">: \"", in: reflected, into: &found)
        appendQuoted(after: "verbatim(\"", in: reflected, into: &found)
        return found
    }

    private func appendQuoted(after marker: String, in reflected: String, into found: inout [String]) {
        var rest = reflected[...]
        while let start = rest.range(of: marker) {
            rest = rest[start.upperBound...]
            guard let end = rest.firstIndex(of: "\"") else {
                return
            }
            let text = String(rest[..<end])
            if !text.isEmpty {
                found.append(text)
            }
            rest = rest[rest.index(after: end)...]
        }
    }

    private func hostedListClasses<V: View>(_ view: V) -> [String] {
        let controller = UIHostingController(rootView: view)
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        let window: UIWindow
        if let scene {
            window = UIWindow(windowScene: scene)
            window.frame = scene.coordinateSpace.bounds
        } else {
            window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        }
        window.rootViewController = controller
        window.makeKeyAndVisible()
        controller.view.frame = window.bounds
        controller.view.setNeedsLayout()
        controller.view.layoutIfNeeded()
        var classes: [String] = []
        func scan(_ view: UIView) {
            if isSystemList(view) {
                classes.append(NSStringFromClass(type(of: view)))
            }
            for subview in view.subviews {
                scan(subview)
            }
        }
        scan(controller.view)
        window.isHidden = true
        return classes
    }

    private func mirrorLabels(_ value: Any) -> [String] {
        var labels: [String] = []
        for child in Mirror(reflecting: value).children {
            labels.append(child.label ?? "?")
        }
        return labels
    }

    private func lastTypeName(_ name: String) -> String {
        let withoutGeneric = name.split(separator: "<", maxSplits: 1).first.map(String.init) ?? name
        return withoutGeneric.split(separator: ".").last.map(String.init) ?? withoutGeneric
    }

    private func isSatchelView(_ name: String) -> Bool {
        switch lastTypeName(name) {
        case "CatalogList", "CatalogRowLabel", "FilePage", "DeviceFacts", "StatusPage", "PairForm",
            "ProductScreen", "ProductCard", "ProductField", "ScannerPage", "PhoneView":
            return true
        default:
            return false
        }
    }

    private func isSystemListName(_ name: String) -> Bool {
        let last = lastTypeName(name)
        return last == "List" || last == "Form"
    }

    private func isSystemList(_ view: UIView) -> Bool {
        if view is UITableView {
            return true
        }
        let name = NSStringFromClass(type(of: view))
        if name.contains("UITableView") || name.contains("ListCollectionView") || name.contains("UpdateCoalescingTableView") {
            return true
        }
        if view.accessibilityContainerType == .list {
            return true
        }
        return false
    }

    private func proof(_ line: String) {
        let text = "satchel-product \(line)\n"
        NSLog("%@", text)
        FileHandle.standardError.write(Data(text.utf8))
    }
}

private struct WalkContext {
    var selected = false
    var card: String?
}

@MainActor
private protocol SatchelNode {
    func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    )
}

extension ModifiedContent: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        var content: Content?
        var modifier: Any?
        for child in Mirror(reflecting: self).children {
            if child.label == "content", let typed = child.value as? Content {
                content = typed
            }
            if child.label == "modifier" {
                modifier = child.value
            }
        }
        var next = context
        if let modifier {
            test.absorb(modifier: modifier, context: &next, dump: &dump)
        }
        if let content {
            test.walk(content, context: next, depth: depth + 1, dump: &dump)
        } else {
            test.note("modified-content-missing")
        }
    }
}

extension VStack: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        guard let content = test.stored(Content.self, in: self) else {
            test.note("vstack-missing")
            return
        }
        test.walk(content, context: context, depth: depth + 1, dump: &dump)
    }
}

extension HStack: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        guard let content = test.stored(Content.self, in: self) else {
            test.note("hstack-missing")
            return
        }
        test.walk(content, context: context, depth: depth + 1, dump: &dump)
    }
}

extension ZStack: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        guard let content = test.stored(Content.self, in: self) else {
            return
        }
        test.walk(content, context: context, depth: depth + 1, dump: &dump)
    }
}

extension ScrollView: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        guard let content = test.stored(Content.self, in: self) else {
            test.note("scroll-missing")
            return
        }
        test.walk(content, context: context, depth: depth + 1, dump: &dump)
    }
}

extension Group: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        guard let content = test.stored(Content.self, in: self) else {
            return
        }
        test.walk(content, context: context, depth: depth + 1, dump: &dump)
    }
}

extension Button: SatchelNode where Label: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        var next = context
        if test.containsCheckmark(self, depth: 0) {
            next.selected = true
        }
        guard let label = test.stored(Label.self, in: self) else {
            test.note("button-missing")
            return
        }
        test.walk(label, context: next, depth: depth + 1, dump: &dump)
    }
}

extension Text: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        let strings = test.quotedStorageStrings(String(reflecting: self))
        for item in strings {
            test.recordLabel(item, context: context, dump: &dump)
        }
    }
}

extension TupleView: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walkAny(value, context: context, depth: depth + 1, dump: &dump)
    }
}

extension EmptyView: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {}
}

extension _ConditionalContent: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        for child in Mirror(reflecting: self).children {
            if child.label == "storage" {
                for branch in Mirror(reflecting: child.value).children {
                    test.walkAny(branch.value, context: context, depth: depth + 1, dump: &dump)
                }
            } else {
                test.walkAny(child.value, context: context, depth: depth + 1, dump: &dump)
            }
        }
    }
}

extension CatalogList: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension CatalogRowLabel: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension FilePage: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension DeviceFacts: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension StatusPage: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension PairForm: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension ProductScreen: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension ProductCard: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension ProductField: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension ScannerPage: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension PhoneView: SatchelNode {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        test.walk(self, context: context, depth: depth, dump: &dump)
    }
}

extension ForEach: SatchelNode where Content: View {
    @MainActor
    fileprivate func satchelWalk(
        test: ProductSurfaceTests,
        context: WalkContext,
        depth: Int,
        dump: inout SurfaceDump
    ) {
        var dataValue: Any?
        var contentValue: Any?
        for child in Mirror(reflecting: self).children {
            if child.label == "data" || child.label == "_data" {
                dataValue = child.value
            }
            if child.label == "content" || child.label == "_content" {
                contentValue = child.value
            }
        }
        guard let dataValue, let contentValue else {
            test.note("foreach-shape")
            return
        }
        guard let data = dataValue as? Data else {
            test.note("foreach-data")
            return
        }
        if let make = contentValue as? (Data.Element) -> Content {
            for element in data {
                test.walk(make(element), context: context, depth: depth + 1, dump: &dump)
            }
            return
        }
        if let make = contentValue as? @MainActor (Data.Element) -> Content {
            for element in data {
                test.walk(make(element), context: context, depth: depth + 1, dump: &dump)
            }
            return
        }
        test.note("foreach-content")
    }
}

private struct SurfaceDump {
    var labels: [String] = []
    var identifiers: [String] = []
    var values: [String] = []
    var selected: [String] = []
    var cards: [String: [String]] = [:]
    var hasSystemList = false
    var listClasses: [String] = []
}
