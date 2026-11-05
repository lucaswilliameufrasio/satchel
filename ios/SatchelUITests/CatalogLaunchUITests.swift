import XCTest

/// The words a test expects in one language. The app follows the system language,
/// so each launch pins it with `-AppleLanguages` and `-AppleLocale`.
private struct Lang {
    let languages: String
    let locale: String
    let name: String
    let view: String
    let downloadNow: String
    let catalog: String
    let datePattern: String
    let oneSent: String
    let upToDate: String
    let fields: [String]
    let secrets: [String]
    let refused: String

    static let portuguese = Lang(
        languages: "(pt-BR)",
        locale: "pt_BR",
        name: "pt",
        view: "Ver",
        downloadNow: "Baixar agora",
        catalog: "Catálogo",
        datePattern: "\\d{2}/\\d{2}/\\d{4} \\d{2}:\\d{2}",
        oneSent: "1 mudança enviada",
        upToDate: "Em dia.",
        fields: ["Região", "Prefixo (pode ficar vazio)", "Chave de acesso de manutenção", "Chave de acesso dos aparelhos"],
        secrets: ["Segredo de manutenção", "Segredo dos aparelhos"],
        refused: "recusou"
    )

    static let english = Lang(
        languages: "(en)",
        locale: "en_US",
        name: "en",
        view: "View",
        downloadNow: "Download now",
        catalog: "Catalog",
        datePattern: "\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}",
        oneSent: "1 change sent",
        upToDate: "Up to date.",
        fields: ["Region", "Prefix (can be empty)", "Maintenance access key", "Device access key"],
        secrets: ["Maintenance secret", "Device secret"],
        refused: "refused"
    )
}

final class CatalogLaunchUITests: XCTestCase {
    private func launch(_ app: XCUIApplication, _ lang: Lang, _ arguments: [String]) {
        app.launchArguments = arguments + ["-AppleLanguages", lang.languages, "-AppleLocale", lang.locale]
        app.launch()
    }

    func testSampleCatalogThenDownloadOnce() throws {
        try sampleCatalogThenDownloadOnce(.portuguese)
    }

    func testSampleCatalogThenDownloadOnceInEnglish() throws {
        try sampleCatalogThenDownloadOnce(.english)
    }

    private func sampleCatalogThenDownloadOnce(_ lang: Lang) throws {
        let app = XCUIApplication()
        launch(app, lang, ["-satchel-reset"])
        let row = app.descendants(matching: .any)["catalog-row-kept.txt"]
        XCTAssertTrue(row.waitForExistence(timeout: 60), "kept.txt row did not appear")
        let label = row.label
        proof("row \(label)")
        XCTAssertTrue(label.contains("kept.txt"))
        XCTAssertTrue(label.contains(lang.view))
        XCTAssertFalse(label.contains("2023-11-14T22:13:20Z"))
        XCTAssertFalse(label.contains("fedcba9876543210fedcba9876543210"))
        XCTAssertFalse(label.contains("0123456789abcdef0123456789abcdef"))
        let date = try NSRegularExpression(pattern: lang.datePattern)
        let range = NSRange(label.startIndex..<label.endIndex, in: label)
        XCTAssertNotNil(date.firstMatch(in: label, range: range))
        let surface = app.descendants(matching: .any)["satchel-product-surface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        XCTAssertEqual(surface.value as? String, "scroll-stack")
        XCTAssertTrue(app.descendants(matching: .any)["satchel-mark"].exists)
        XCTAssertEqual(app.tables.count, 0)
        capture(name: "catalog-\(lang.name)", text: label)

        row.tap()
        let once = app.descendants(matching: .any)["follow-once"]
        XCTAssertTrue(once.waitForExistence(timeout: 10))
        once.tap()
        let selected = NSPredicate(format: "isSelected == true")
        expectation(for: selected, evaluatedWith: once)
        waitForExpectations(timeout: 40)
        XCTAssertTrue(once.isSelected)
        capture(name: "file-\(lang.name)", text: once.label)
        let back = app.buttons[lang.catalog]
        XCTAssertTrue(back.waitForExistence(timeout: 10))
        back.tap()
        let updated = app.descendants(matching: .any)["catalog-row-kept.txt"]
        XCTAssertTrue(updated.waitForExistence(timeout: 20))
        let updatedLabel = updated.label
        proof("after \(updatedLabel)")
        XCTAssertTrue(updatedLabel.contains(lang.downloadNow))
        XCTAssertFalse(updatedLabel.contains("2023-11-14T22:13:20Z"))
        capture(name: "downloaded-\(lang.name)", text: updatedLabel)
    }

    /// The largest accessibility text size, then Apple's audit (labels, contrast,
    /// clipped text, hit areas) on the catalog, the file, the join form and the scanner.
    func testScreensPassTheAccessibilityAuditAtTheLargestTextSize() throws {
        try auditAtLargestText(.portuguese)
    }

    func testScreensPassTheAccessibilityAuditAtTheLargestTextSizeInEnglish() throws {
        try auditAtLargestText(.english)
    }

    private func auditAtLargestText(_ lang: Lang) throws {
        let app = XCUIApplication()
        launch(app, lang, [
            "-satchel-reset",
            "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL",
        ])
        let row = app.descendants(matching: .any)["catalog-row-kept.txt"]
        XCTAssertTrue(row.waitForExistence(timeout: 60), "kept.txt row did not appear")
        capture(name: "large-catalog-\(lang.name)", text: row.label)
        try audit(app)

        reveal(row, in: app)
        row.tap()
        let once = app.descendants(matching: .any)["follow-once"]
        XCTAssertTrue(once.waitForExistence(timeout: 10))
        capture(name: "large-file-\(lang.name)", text: once.label)
        try audit(app)

        let back = app.buttons[lang.catalog]
        reveal(back, in: app, upward: true)
        back.tap()
        let join = app.buttons["pair-entry"]
        XCTAssertTrue(join.waitForExistence(timeout: 20))
        reveal(join, in: app)
        join.tap()
        let scan = app.buttons["pair-scan"]
        XCTAssertTrue(scan.waitForExistence(timeout: 10))
        capture(name: "large-join-\(lang.name)", text: scan.label)
        try audit(app)

        reveal(scan, in: app)
        scan.tap()
        let typeInstead = app.buttons["scanner-type-instead"]
        XCTAssertTrue(typeInstead.waitForExistence(timeout: 10))
        capture(name: "large-scanner-\(lang.name)", text: typeInstead.label)
        try audit(app)
    }

    /// The audit exists from iOS 17; the app still runs on 16.
    private func audit(_ app: XCUIApplication, fileNames: Set<String> = ["kept.txt"]) throws {
        if #available(iOS 17.0, *) {
            try app.performAccessibilityAudit { issue in
                let element = issue.element
                let label = element?.label ?? ""
                self.proof("audit \(issue.compactDescription) | \(element?.elementType.rawValue ?? 0) | \(element?.identifier ?? "") | \(label)")
                // A file name is the content itself: “kept.txt” reads as written.
                return issue.compactDescription == "Label not human-readable" && fileNames.contains(label)
            }
        } else {
            throw XCTSkip("the accessibility audit needs iOS 17")
        }
    }

    /// Scroll until `element` can be tapped. Large text pushes it below the fold.
    private func reveal(_ element: XCUIElement, in app: XCUIApplication, upward: Bool = false) {
        var tries = 0
        while !element.isHittable && tries < 12 {
            if upward {
                app.swipeDown()
            } else {
                app.swipeUp()
            }
            tries += 1
        }
    }

    /// Needs a clean install: the phone must not already hold a repository.
    func testCreateFolderRepositoryShowsTheRecoveryPasswordOnceThenTheInvite() throws {
        let app = XCUIApplication()
        launch(app, .portuguese, ["-satchel-reset"])
        let create = app.descendants(matching: .any)["create-repository"]
        XCTAssertTrue(create.waitForExistence(timeout: 60), "create card did not appear")
        XCTAssertFalse(app.descendants(matching: .any)["show-invite"].exists)
        create.tap()
        let submit = app.descendants(matching: .any)["create-submit"]
        XCTAssertTrue(submit.waitForExistence(timeout: 10))
        submit.tap()
        let password = app.descendants(matching: .any)["recovery-password"]
        XCTAssertTrue(password.waitForExistence(timeout: 60), "recovery password did not appear")
        let shown = password.label
        proof("recovery-length \(shown.count)")
        let hex = try NSRegularExpression(pattern: "^[0-9a-f]{64}$")
        let range = NSRange(shown.startIndex..<shown.endIndex, in: shown)
        XCTAssertNotNil(hex.firstMatch(in: shown, range: range))
        app.descendants(matching: .any)["recovery-done"].tap()
        let invite = app.descendants(matching: .any)["show-invite"]
        XCTAssertTrue(invite.waitForExistence(timeout: 60), "invite card did not appear")
        XCTAssertFalse(app.descendants(matching: .any)["create-repository"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["recovery-password"].exists)
        invite.tap()
        let code = app.descendants(matching: .any)["hold-code"]
        XCTAssertTrue(code.waitForExistence(timeout: 20), "invite did not open")
        let address = app.descendants(matching: .any)["hold-address"].label
        proof("hold \(code.label) \(address)")
        XCTAssertFalse(address.contains("0.0.0.0"))
        XCTAssertTrue(app.descendants(matching: .any)["hold-fingerprint"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["recovery-password"].exists)
        capture(name: "invite", text: "\(code.label)\n\(address)")
    }

    func testCreatorChangesTheRecoveryPasswordAndSeesTheNewOneOnce() throws {
        let app = XCUIApplication()
        launch(app, .portuguese, ["-satchel-reset"])
        let create = app.descendants(matching: .any)["create-repository"]
        XCTAssertTrue(create.waitForExistence(timeout: 60))
        create.tap()
        let submit = app.descendants(matching: .any)["create-submit"]
        XCTAssertTrue(submit.waitForExistence(timeout: 10))
        submit.tap()
        let first = app.descendants(matching: .any)["recovery-password"]
        XCTAssertTrue(first.waitForExistence(timeout: 60))
        let old = first.label
        app.descendants(matching: .any)["recovery-done"].tap()

        let change = app.descendants(matching: .any)["change-recovery"]
        XCTAssertTrue(change.waitForExistence(timeout: 60), "change card did not appear")
        change.tap()
        let now = app.descendants(matching: .any)["recovery-change-now"]
        XCTAssertTrue(now.waitForExistence(timeout: 10))
        now.tap()
        let second = app.descendants(matching: .any)["recovery-password"]
        XCTAssertTrue(second.waitForExistence(timeout: 60), "new password did not appear")
        let fresh = second.label
        XCTAssertEqual(fresh.count, 64)
        XCTAssertNotEqual(fresh, old)
        proof("recovery-changed \(old.prefix(4))… -> \(fresh.prefix(4))…")
        app.descendants(matching: .any)["recovery-done"].tap()
        // The catalog opens again with the new grant from the Keychain, and the password is gone.
        XCTAssertTrue(app.descendants(matching: .any)["show-invite"].waitForExistence(timeout: 60))
        XCTAssertFalse(app.descendants(matching: .any)["recovery-password"].exists)

        // A relaunch without reset still opens: the Keychain holds the new grant, nothing is pending.
        app.terminate()
        launch(app, .portuguese, [])
        XCTAssertTrue(app.descendants(matching: .any)["show-invite"].waitForExistence(timeout: 60))
    }

    /// The system folder picker runs in another process, so the simulator build takes a
    /// folder from `-satchel-folder-fixture`: a temporary folder holding `hello.txt`.
    func testCreatorsFolderGoesUpOnTheFirstRoundAndThenIsQuiet() throws {
        try creatorsFolderGoesUpOnTheFirstRoundAndThenIsQuiet(.portuguese)
    }

    func testCreatorsFolderGoesUpOnTheFirstRoundAndThenIsQuietInEnglish() throws {
        try creatorsFolderGoesUpOnTheFirstRoundAndThenIsQuiet(.english)
    }

    private func creatorsFolderGoesUpOnTheFirstRoundAndThenIsQuiet(_ lang: Lang) throws {
        let app = XCUIApplication()
        launch(app, lang, ["-satchel-reset", "-satchel-folder-fixture"])
        let create = app.descendants(matching: .any)["create-repository"]
        XCTAssertTrue(create.waitForExistence(timeout: 60))
        XCTAssertFalse(app.descendants(matching: .any)["folder-sync"].exists, "no folder card before a grant")
        create.tap()
        let submit = app.descendants(matching: .any)["create-submit"]
        XCTAssertTrue(submit.waitForExistence(timeout: 10))
        submit.tap()
        let password = app.descendants(matching: .any)["recovery-password"]
        XCTAssertTrue(password.waitForExistence(timeout: 60))
        app.descendants(matching: .any)["recovery-done"].tap()

        let card = app.descendants(matching: .any)["folder-sync"]
        XCTAssertTrue(card.waitForExistence(timeout: 60), "folder card did not appear")
        let sent = NSPredicate(format: "label CONTAINS %@", lang.oneSent)
        expectation(for: sent, evaluatedWith: card)
        waitForExpectations(timeout: 60)
        XCTAssertTrue(card.label.contains("satchel-folder-fixture"))
        XCTAssertTrue(app.descendants(matching: .any)["catalog-row-hello.txt"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.descendants(matching: .any)["folder-switch"].exists)
        proof("folder-first-round \(card.label)")

        card.tap()
        let quiet = NSPredicate(format: "label CONTAINS %@", lang.upToDate)
        expectation(for: quiet, evaluatedWith: card)
        waitForExpectations(timeout: 60)
        proof("folder-second-round \(card.label)")
        capture(name: "folder-\(lang.name)", text: card.label)
    }

    /// Needs a local S3 emulator. Start one on 127.0.0.1:4566 with the buckets `ios-unprotected`
    /// (versioning never on) and `ios-protected` (versioning on), then run xcodebuild with
    /// `TEST_RUNNER_SATCHEL_FLOCI=http://127.0.0.1:4566`. Skipped when the variable is absent.
    func testObjectStorageRefusesAnUnprotectedBucketThenCreatesOnAProtectedOne() throws {
        guard let endpoint = ProcessInfo.processInfo.environment["SATCHEL_FLOCI"], !endpoint.isEmpty else {
            throw XCTSkip("SATCHEL_FLOCI is not set")
        }
        let app = XCUIApplication()
        launch(app, .portuguese, ["-satchel-reset"])
        XCTAssertTrue(app.descendants(matching: .any)["create-repository"].waitForExistence(timeout: 60))
        app.descendants(matching: .any)["create-repository"].tap()
        let r2 = app.descendants(matching: .any)["storage-r2"]
        XCTAssertTrue(r2.waitForExistence(timeout: 10))
        r2.tap()
        XCTAssertTrue(r2.isSelected)
        let submit = app.descendants(matching: .any)["create-submit"]
        XCTAssertFalse(submit.isEnabled, "incomplete object fields must not enable Criar")

        fill(app, "Endpoint", endpoint)
        fill(app, "Bucket", "ios-unprotected")
        let lang = Lang.portuguese
        fill(app, lang.fields[0], "us-east-1")
        let prefix = "run-" + UUID().uuidString.prefix(8).lowercased()
        fill(app, lang.fields[1], prefix)
        fill(app, lang.fields[2], "test")
        fillSecret(app, lang.secrets[0], "test")
        fill(app, lang.fields[3], "device-key")
        fillSecret(app, lang.secrets[1], "device-secret")
        XCTAssertTrue(submit.isEnabled)
        submit.tap()
        let refusal = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", lang.refused)).firstMatch
        XCTAssertTrue(refusal.waitForExistence(timeout: 60), "refusal advice did not appear")
        proof("refused \(refusal.label)")
        XCTAssertFalse(app.descendants(matching: .any)["recovery-password"].exists)

        // Same form, a bucket that keeps versions.
        let bucket = app.textFields["Bucket"]
        bucket.tap()
        bucket.press(forDuration: 1.0)
        if app.menuItems["Select All"].waitForExistence(timeout: 3) {
            app.menuItems["Select All"].tap()
        }
        bucket.typeText("ios-protected")
        XCTAssertTrue(submit.waitForExistence(timeout: 5))
        submit.tap()
        let password = app.descendants(matching: .any)["recovery-password"]
        XCTAssertTrue(password.waitForExistence(timeout: 60), "recovery password did not appear on a protected bucket")
        XCTAssertEqual(password.label.count, 64)
        app.descendants(matching: .any)["recovery-done"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["show-invite"].waitForExistence(timeout: 60))
        proof("object storage created prefix \(prefix)")
    }

    private func fill(_ app: XCUIApplication, _ title: String, _ text: String) {
        let field = app.textFields[title]
        XCTAssertTrue(field.waitForExistence(timeout: 10), "missing field \(title)")
        field.tap()
        field.typeText(text)
    }

    private func fillSecret(_ app: XCUIApplication, _ title: String, _ text: String) {
        let field = app.secureTextFields[title]
        XCTAssertTrue(field.waitForExistence(timeout: 10), "missing secret field \(title)")
        field.tap()
        field.typeText(text)
    }

    private func capture(name: String, text: String) {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("satchel-ui-capture", isDirectory: true)
        try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let shot = XCUIScreen.main.screenshot().pngRepresentation
        let imageURL = root.appendingPathComponent("\(name).png")
        try? shot.write(to: imageURL)
        let textURL = root.appendingPathComponent("\(name).txt")
        try? text.write(to: textURL, atomically: true, encoding: .utf8)
        proof("capture \(root.path)")
    }

    private func proof(_ line: String) {
        let text = "satchel-ui \(line)\n"
        NSLog("%@", text)
        FileHandle.standardError.write(Data(text.utf8))
    }
}
