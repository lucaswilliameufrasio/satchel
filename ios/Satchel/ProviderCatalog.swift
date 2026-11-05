import CryptoKit
import FileProvider
import Foundation
import UniformTypeIdentifiers

/// One path of the catalog as Files shows it.
struct ProviderEntry: Equatable {
    let path: String
    let isDirectory: Bool
    let size: Int64
    let versionUnix: Int64

    var name: String {
        path.split(separator: "/").last.map(String.init) ?? path
    }

    var parent: String {
        guard let slash = path.lastIndex(of: "/") else {
            return ""
        }
        return String(path[path.startIndex..<slash])
    }
}

/// The catalog the app's Files extension serves: read-only, folders and files by path.
///
/// Identifiers are the catalog paths, and the root is the domain's root container.
/// A version is the catalog time and size, so a new snapshot of a file is a new version.
enum ProviderCatalog {
    /// Entries by path, with the folders a path implies even when the catalog lists none.
    static func entries(from rows: [CatalogRow]) -> [String: ProviderEntry] {
        var entries: [String: ProviderEntry] = [:]
        for row in rows {
            entries[row.path] = ProviderEntry(
                path: row.path,
                isDirectory: row.isDirectory,
                size: row.isDirectory ? 0 : row.size,
                versionUnix: row.versionUnix
            )
            var parent = entries[row.path]?.parent ?? ""
            while !parent.isEmpty {
                if entries[parent] == nil {
                    entries[parent] = ProviderEntry(
                        path: parent, isDirectory: true, size: 0, versionUnix: row.versionUnix
                    )
                }
                parent = entries[parent]?.parent ?? ""
            }
        }
        return entries
    }

    /// Direct children of `container`, folders first, then by name.
    static func children(of container: String, in entries: [String: ProviderEntry]) -> [ProviderEntry] {
        entries.values
            .filter { $0.parent == container }
            .sorted { left, right in
                if left.isDirectory != right.isDirectory {
                    return left.isDirectory
                }
                return left.name.localizedStandardCompare(right.name) == .orderedAscending
            }
    }

    /// Changes whenever a path, a size or a version changes.
    static func anchor(of entries: [String: ProviderEntry]) -> Data {
        var hasher = SHA256()
        for path in entries.keys.sorted() {
            guard let entry = entries[path] else {
                continue
            }
            hasher.update(data: Data("\(path)\u{0}\(entry.isDirectory)\u{0}\(entry.size)\u{0}\(entry.versionUnix)\n".utf8))
        }
        return Data(hasher.finalize())
    }

    static func identifier(for path: String) -> NSFileProviderItemIdentifier {
        path.isEmpty ? .rootContainer : NSFileProviderItemIdentifier(path)
    }

    /// The catalog path of an identifier; the root container is the empty path.
    static func path(for identifier: NSFileProviderItemIdentifier) -> String? {
        if identifier == .rootContainer {
            return ""
        }
        if identifier == .workingSet || identifier == .trashContainer {
            return nil
        }
        return identifier.rawValue
    }
}

/// A catalog entry as an `NSFileProviderItem`.
final class ProviderItem: NSObject, NSFileProviderItem {
    let entry: ProviderEntry

    init(_ entry: ProviderEntry) {
        self.entry = entry
    }

    var itemIdentifier: NSFileProviderItemIdentifier {
        ProviderCatalog.identifier(for: entry.path)
    }

    var parentItemIdentifier: NSFileProviderItemIdentifier {
        ProviderCatalog.identifier(for: entry.parent)
    }

    var filename: String {
        entry.path.isEmpty ? "Satchel" : entry.name
    }

    var contentType: UTType {
        if entry.isDirectory {
            return .folder
        }
        let ext = (entry.name as NSString).pathExtension
        return UTType(filenameExtension: ext) ?? .data
    }

    var documentSize: NSNumber? {
        entry.isDirectory ? nil : NSNumber(value: entry.size)
    }

    var contentModificationDate: Date? {
        Date(timeIntervalSince1970: TimeInterval(entry.versionUnix))
    }

    var creationDate: Date? {
        contentModificationDate
    }

    /// Read-only: Files offers no rename, move, delete or write.
    ///
    /// A file without `.allowsWriting` is locked on disk by the system. On the
    /// simulator that lock fails ("cannotSetMetadata", EPERM) and the item never
    /// appears, so there the file stays unlocked; the extension still refuses any
    /// change. A real iPhone keeps the lock; that path is not proven on a device yet.
    var capabilities: NSFileProviderItemCapabilities {
        if entry.isDirectory {
            return [.allowsReading, .allowsContentEnumerating]
        }
        #if targetEnvironment(simulator)
        return [.allowsReading, .allowsWriting]
        #else
        return [.allowsReading]
        #endif
    }

    /// The file on disk stays writable by its owner: the daemon must be able to set its
    /// metadata, and Files still offers no change because `capabilities` says read-only.
    var fileSystemFlags: NSFileProviderFileSystemFlags {
        [.userReadable, .userWritable]
    }

    var itemVersion: NSFileProviderItemVersion {
        let version = Data("\(entry.versionUnix)-\(entry.size)".utf8)
        return NSFileProviderItemVersion(contentVersion: version, metadataVersion: version)
    }

    static let root = ProviderItem(ProviderEntry(path: "", isDirectory: true, size: 0, versionUnix: 0))
}

/// Where the provider reads the catalog and the bytes: the grant in the shared Keychain group.
///
/// The grant's work copy lives in the app's own container, which the extension
/// cannot reach, so the extension's copy of the grant points `work` at its own
/// temporary folder. Nothing is written there: a catalog and a fetch only read.
struct ProviderSource {
    var grant: () -> String? = { GrantVault.read() }
    var scratch: URL = FileManager.default.temporaryDirectory
        .appendingPathComponent("satchel-provider", isDirectory: true)

    enum Failure: Error {
        case noGrant
        case unreachable
        case missing
    }

    private func providerGrant() throws -> String {
        guard let text = grant(), !text.isEmpty,
              let data = text.data(using: .utf8),
              var object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            throw Failure.noGrant
        }
        object["work"] = scratch.appendingPathComponent("work", isDirectory: true).path
        guard let edited = try? JSONSerialization.data(withJSONObject: object),
              let result = String(data: edited, encoding: .utf8)
        else {
            throw Failure.noGrant
        }
        return result
    }

    func entries() throws -> [String: ProviderEntry] {
        guard let rows = SatchelCore.catalogGrant(specPath: try providerGrant()) else {
            throw Failure.unreachable
        }
        return ProviderCatalog.entries(from: rows)
    }

    /// Write the newest version of `path` to a new file and return it.
    func fetch(_ path: String) throws -> URL {
        let folder = scratch.appendingPathComponent("fetched", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let name = path.split(separator: "/").last.map(String.init) ?? "file"
        let target = folder.appendingPathComponent(name)
        guard SatchelCore.fetchGrant(specPath: try providerGrant(), path: path, dest: target.path) else {
            throw Failure.missing
        }
        return target
    }
}

extension ProviderSource.Failure {
    var providerError: NSError {
        switch self {
        case .noGrant:
            return NSFileProviderError(.notAuthenticated) as NSError
        case .unreachable:
            return NSFileProviderError(.serverUnreachable) as NSError
        case .missing:
            return NSFileProviderError(.noSuchItem) as NSError
        }
    }
}
