import FileProvider
import XCTest

@testable import Satchel

/// The Files extension over a real repository: listing, opening, and refusing writes.
final class FilesProviderTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("satchel-files-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    func testTheCatalogBecomesFoldersAndVersions() {
        let rows = [
            CatalogRow(path: "notes/2026/plan.txt", isDirectory: false, size: 12, versionUnix: 1_700_000_000,
                       devices: [], holders: [], follow: "see"),
            CatalogRow(path: "b.txt", isDirectory: false, size: 3, versionUnix: 1_700_000_100,
                       devices: [], holders: [], follow: "see"),
            CatalogRow(path: "a.pdf", isDirectory: false, size: 9, versionUnix: 1_700_000_200,
                       devices: [], holders: [], follow: "always"),
        ]
        let entries = ProviderCatalog.entries(from: rows)
        XCTAssertEqual(entries["notes"]?.isDirectory, true, "a folder the path implies")
        XCTAssertEqual(entries["notes/2026"]?.isDirectory, true)
        XCTAssertEqual(
            ProviderCatalog.children(of: "", in: entries).map(\.name),
            ["notes", "a.pdf", "b.txt"],
            "folders first, then by name"
        )
        XCTAssertEqual(ProviderCatalog.children(of: "notes/2026", in: entries).map(\.path), ["notes/2026/plan.txt"])

        let plan = ProviderItem(entries["notes/2026/plan.txt"]!)
        XCTAssertEqual(plan.filename, "plan.txt")
        XCTAssertEqual(plan.parentItemIdentifier.rawValue, "notes/2026")
        XCTAssertEqual(plan.documentSize, 12)
        XCTAssertTrue(plan.capabilities.contains(.allowsReading))
        for refused: NSFileProviderItemCapabilities in [.allowsDeleting, .allowsRenaming, .allowsReparenting, .allowsTrashing] {
            XCTAssertFalse(plan.capabilities.contains(refused), "Files offers no change")
        }
        XCTAssertFalse(ProviderItem(entries["notes"]!).capabilities.contains(.allowsAddingSubItems))
        XCTAssertEqual(ProviderItem(entries["notes"]!).parentItemIdentifier, .rootContainer)
        XCTAssertEqual(ProviderItem(entries["a.pdf"]!).contentType, .pdf)

        let before = ProviderCatalog.anchor(of: entries)
        XCTAssertEqual(before, ProviderCatalog.anchor(of: ProviderCatalog.entries(from: rows)))
        var later = rows
        later[1] = CatalogRow(path: "b.txt", isDirectory: false, size: 3, versionUnix: 1_700_000_999,
                              devices: [], holders: [], follow: "see")
        XCTAssertNotEqual(before, ProviderCatalog.anchor(of: ProviderCatalog.entries(from: later)),
                          "a new version of a file moves the anchor")
        XCTAssertNil(ProviderCatalog.path(for: .workingSet))
        XCTAssertEqual(ProviderCatalog.path(for: .rootContainer), "")
    }

    func testTheExtensionListsOpensAndRefusesWrites() throws {
        let grant = try repositoryWithFiles(["hello.txt": "from the phone", "docs/plan.md": "# plan"])
        let scratch = root.appendingPathComponent("provider", isDirectory: true)
        let source = ProviderSource(grant: { grant }, scratch: scratch)
        let files = FileProviderExtension(domain: FilesDomain.domain, source: source)

        let top = try enumerate(files, .rootContainer)
        XCTAssertEqual(top.map(\.filename), ["docs", "hello.txt"])
        let docs = try enumerate(files, NSFileProviderItemIdentifier("docs"))
        XCTAssertEqual(docs.map(\.filename), ["plan.md"])
        let everything = try enumerate(files, .workingSet)
        XCTAssertEqual(Set(everything.map(\.itemIdentifier.rawValue)), ["docs", "docs/plan.md", "hello.txt"])

        let opened = expectation(description: "fetch")
        var fetched: URL?
        _ = files.fetchContents(
            for: NSFileProviderItemIdentifier("docs/plan.md"), version: nil, request: NSFileProviderRequest()
        ) { url, item, error in
            XCTAssertNil(error)
            XCTAssertEqual(item?.filename, "plan.md")
            fetched = url
            opened.fulfill()
        }
        wait(for: [opened], timeout: 30)
        XCTAssertEqual(try String(contentsOf: XCTUnwrap(fetched), encoding: .utf8), "# plan")
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: scratch.appendingPathComponent("work/docs/plan.md").path),
            "opening a file writes no work copy"
        )

        let missing = expectation(description: "missing")
        _ = files.item(for: NSFileProviderItemIdentifier("nope.txt"), request: NSFileProviderRequest()) { item, error in
            XCTAssertNil(item)
            XCTAssertEqual((error as NSError?)?.code, NSFileProviderError.noSuchItem.rawValue)
            missing.fulfill()
        }
        let refused = expectation(description: "delete")
        _ = files.deleteItem(
            identifier: NSFileProviderItemIdentifier("hello.txt"),
            baseVersion: NSFileProviderItemVersion(),
            options: [],
            request: NSFileProviderRequest()
        ) { error in
            XCTAssertEqual((error as NSError?)?.code, NSFeatureUnsupportedError)
            refused.fulfill()
        }
        wait(for: [missing, refused], timeout: 30)

        let unpaired = FileProviderExtension(
            domain: FilesDomain.domain, source: ProviderSource(grant: { nil }, scratch: scratch)
        )
        XCTAssertThrowsError(try enumerate(unpaired, .rootContainer)) { error in
            XCTAssertEqual((error as NSError).code, NSFileProviderError.notAuthenticated.rawValue)
        }
    }

    func testANewSnapshotExpiresTheAnchor() throws {
        let folder = root.appendingPathComponent("picked", isDirectory: true)
        let grant = try repositoryWithFiles(["one.txt": "1"], folder: folder)
        let source = ProviderSource(grant: { grant }, scratch: root.appendingPathComponent("provider"))
        let enumerator = ProviderEnumerator(container: .workingSet, source: source)
        let anchor = try currentAnchor(enumerator)
        XCTAssertEqual(try changes(enumerator, from: anchor), .finished, "nothing moved")
        try Data("2".utf8).write(to: folder.appendingPathComponent("two.txt"))
        try round(grant: grant, folder: folder)
        XCTAssertEqual(try changes(enumerator, from: anchor), .expired, "Files lists again")
    }

    // MARK: - Helpers

    private func repositoryWithFiles(_ files: [String: String], folder: URL? = nil) throws -> String {
        let picked = folder ?? root.appendingPathComponent("picked", isDirectory: true)
        for (path, text) in files {
            let url = picked.appendingPathComponent(path)
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(text.utf8).write(to: url)
        }
        let id = try XCTUnwrap(SatchelCore.newDeviceId())
        let spec = try String(
            data: JSONSerialization.data(withJSONObject: [
                "kind": "folder", "device_id": id,
                "directory": root.appendingPathComponent("repository").path,
            ]),
            encoding: .utf8
        )
        let created = try XCTUnwrap(SatchelCore.createRepository(
            spec: XCTUnwrap(spec), work: root.appendingPathComponent("work").path
        ))
        let split = try XCTUnwrap(created.firstIndex(of: "\n"))
        let grant = String(created[created.index(after: split)...])
        try round(grant: grant, folder: picked)
        return grant
    }

    private func round(grant: String, folder: URL) throws {
        XCTAssertNotNil(SatchelCore.syncFolder(
            grant: grant,
            folder: folder.path,
            manifest: root.appendingPathComponent("folder.manifest").path,
            name: "Teste",
            os: "ios"
        ))
    }

    private func enumerate(
        _ files: FileProviderExtension, _ container: NSFileProviderItemIdentifier
    ) throws -> [NSFileProviderItem] {
        let enumerator = try files.enumerator(for: container, request: NSFileProviderRequest())
        let observer = Collect()
        enumerator.enumerateItems(for: observer, startingAt: NSFileProviderPage.initialPageSortedByName as NSFileProviderPage)
        if let error = observer.error {
            throw error
        }
        return observer.items
    }

    private func currentAnchor(_ enumerator: ProviderEnumerator) throws -> NSFileProviderSyncAnchor {
        var anchor: NSFileProviderSyncAnchor?
        enumerator.currentSyncAnchor { anchor = $0 }
        return try XCTUnwrap(anchor)
    }

    private enum Change { case finished, expired }

    private func changes(_ enumerator: ProviderEnumerator, from anchor: NSFileProviderSyncAnchor) throws -> Change {
        let observer = Changes()
        enumerator.enumerateChanges(for: observer, from: anchor)
        if observer.finished {
            return .finished
        }
        if (observer.error as NSError?)?.code == NSFileProviderError.syncAnchorExpired.rawValue {
            return .expired
        }
        throw try XCTUnwrap(observer.error)
    }
}

private final class Collect: NSObject, NSFileProviderEnumerationObserver {
    var items: [NSFileProviderItem] = []
    var error: Error?

    func didEnumerate(_ updatedItems: [NSFileProviderItemProtocol]) {
        items.append(contentsOf: updatedItems)
    }

    func finishEnumerating(upTo nextPage: NSFileProviderPage?) {}

    func finishEnumeratingWithError(_ error: Error) {
        self.error = error
    }
}

private final class Changes: NSObject, NSFileProviderChangeObserver {
    var finished = false
    var error: Error?

    func didUpdate(_ updatedItems: [NSFileProviderItemProtocol]) {}

    func didDeleteItems(withIdentifiers deletedItemIdentifiers: [NSFileProviderItemIdentifier]) {}

    func finishEnumeratingChanges(upTo anchor: NSFileProviderSyncAnchor, moreComing: Bool) {
        finished = true
    }

    func finishEnumeratingWithError(_ error: Error) {
        self.error = error
    }
}
