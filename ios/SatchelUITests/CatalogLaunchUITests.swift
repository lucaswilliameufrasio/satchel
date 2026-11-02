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
