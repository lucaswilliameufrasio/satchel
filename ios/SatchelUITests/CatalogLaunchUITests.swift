import XCTest

final class CatalogLaunchUITests: XCTestCase {
    func testSampleCatalogThenDownloadOnce() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-satchel-reset"]
        app.launch()
        let row = app.descendants(matching: .any)["catalog-row-kept.txt"]
        XCTAssertTrue(row.waitForExistence(timeout: 60), "kept.txt row did not appear")
        let label = row.label
        proof("row \(label)")
        XCTAssertTrue(label.contains("kept.txt"))
        XCTAssertTrue(label.contains("Ver"))
        XCTAssertFalse(label.contains("2023-11-14T22:13:20Z"))
        XCTAssertFalse(label.contains("fedcba9876543210fedcba9876543210"))
        XCTAssertFalse(label.contains("0123456789abcdef0123456789abcdef"))
        let date = try NSRegularExpression(pattern: "\\d{2}/\\d{2}/\\d{4} \\d{2}:\\d{2}")
        let range = NSRange(label.startIndex..<label.endIndex, in: label)
        XCTAssertNotNil(date.firstMatch(in: label, range: range))
        let surface = app.descendants(matching: .any)["satchel-product-surface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        XCTAssertEqual(surface.value as? String, "scroll-stack")
        XCTAssertTrue(app.descendants(matching: .any)["satchel-mark"].exists)
        XCTAssertEqual(app.tables.count, 0)
        capture(name: "catalog", text: label)

        row.tap()
        let once = app.descendants(matching: .any)["follow-once"]
        XCTAssertTrue(once.waitForExistence(timeout: 10))
        once.tap()
        let selected = NSPredicate(format: "isSelected == true")
        expectation(for: selected, evaluatedWith: once)
        waitForExpectations(timeout: 40)
        XCTAssertTrue(once.isSelected)
        let back = app.buttons["Catálogo"]
        XCTAssertTrue(back.waitForExistence(timeout: 10))
        back.tap()
        let updated = app.descendants(matching: .any)["catalog-row-kept.txt"]
        XCTAssertTrue(updated.waitForExistence(timeout: 20))
        let updatedLabel = updated.label
        proof("after \(updatedLabel)")
        XCTAssertTrue(updatedLabel.contains("Baixar agora"))
        XCTAssertFalse(updatedLabel.contains("2023-11-14T22:13:20Z"))
        capture(name: "downloaded", text: updatedLabel)
    }

    /// Needs a clean install: the phone must not already hold a repository.
    func testCreateFolderRepositoryShowsTheRecoveryPasswordOnceThenTheInvite() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-satchel-reset"]
        app.launch()
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
        app.launchArguments = ["-satchel-reset"]
        app.launch()
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
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["show-invite"].waitForExistence(timeout: 60))
    }

    /// Needs a local S3 emulator. Start one on 127.0.0.1:4566 with the buckets `ios-unprotected`
    /// (versioning never on) and `ios-protected` (versioning on), then run xcodebuild with
    /// `TEST_RUNNER_SATCHEL_FLOCI=http://127.0.0.1:4566`. Skipped when the variable is absent.
    func testObjectStorageRefusesAnUnprotectedBucketThenCreatesOnAProtectedOne() throws {
        guard let endpoint = ProcessInfo.processInfo.environment["SATCHEL_FLOCI"], !endpoint.isEmpty else {
            throw XCTSkip("SATCHEL_FLOCI is not set")
        }
        let app = XCUIApplication()
        app.launchArguments = ["-satchel-reset"]
        app.launch()
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
        fill(app, "Região", "us-east-1")
        let prefix = "run-" + UUID().uuidString.prefix(8).lowercased()
        fill(app, "Prefixo (pode ficar vazio)", prefix)
        fill(app, "Chave de acesso de manutenção", "test")
        fillSecret(app, "Segredo de manutenção", "test")
        fill(app, "Chave de acesso dos aparelhos", "device-key")
        fillSecret(app, "Segredo dos aparelhos", "device-secret")
        XCTAssertTrue(submit.isEnabled)
        submit.tap()
        let refusal = app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'recusou'")).firstMatch
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
