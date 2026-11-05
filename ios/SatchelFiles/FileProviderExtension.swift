import FileProvider
import Foundation
import os

let providerLog = Logger(subsystem: "io.github.lucaswilliameufrasio.satchel.files", category: "provider")

/// The Files app's view of the Satchel catalog. Read-only in this cut: browsing
/// and opening work, and opening a file downloads its newest version.
final class FileProviderExtension: NSObject, NSFileProviderReplicatedExtension {
    private let domain: NSFileProviderDomain
    private let source: ProviderSource
    private let queue = DispatchQueue(label: "io.github.lucaswilliameufrasio.satchel.files")

    required init(domain: NSFileProviderDomain) {
        self.domain = domain
        source = ProviderSource()
        super.init()
    }

    /// For tests: the same extension over another source.
    init(domain: NSFileProviderDomain, source: ProviderSource) {
        self.domain = domain
        self.source = source
        super.init()
    }

    func invalidate() {}

    func item(
        for identifier: NSFileProviderItemIdentifier,
        request: NSFileProviderRequest,
        completionHandler: @escaping (NSFileProviderItem?, Error?) -> Void
    ) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        let source = self.source
        queue.async {
            defer { progress.completedUnitCount = 1 }
            guard let path = ProviderCatalog.path(for: identifier) else {
                completionHandler(nil, NSFileProviderError(.noSuchItem))
                return
            }
            if path.isEmpty {
                completionHandler(ProviderItem.root, nil)
                return
            }
            do {
                guard let entry = try source.entries()[path] else {
                    completionHandler(nil, NSFileProviderError(.noSuchItem))
                    return
                }
                completionHandler(ProviderItem(entry), nil)
            } catch let failure as ProviderSource.Failure {
                completionHandler(nil, failure.providerError)
            } catch {
                completionHandler(nil, error)
            }
        }
        return progress
    }

    func fetchContents(
        for itemIdentifier: NSFileProviderItemIdentifier,
        version requestedVersion: NSFileProviderItemVersion?,
        request: NSFileProviderRequest,
        completionHandler: @escaping (URL?, NSFileProviderItem?, Error?) -> Void
    ) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        let source = self.source
        queue.async {
            defer { progress.completedUnitCount = 1 }
            guard let path = ProviderCatalog.path(for: itemIdentifier), !path.isEmpty else {
                completionHandler(nil, nil, NSFileProviderError(.noSuchItem))
                return
            }
            do {
                guard let entry = try source.entries()[path], !entry.isDirectory else {
                    completionHandler(nil, nil, NSFileProviderError(.noSuchItem))
                    return
                }
                let url = try source.fetch(path)
                completionHandler(url, ProviderItem(entry), nil)
            } catch let failure as ProviderSource.Failure {
                completionHandler(nil, nil, failure.providerError)
            } catch {
                completionHandler(nil, nil, error)
            }
        }
        return progress
    }

    func createItem(
        basedOn itemTemplate: NSFileProviderItem,
        fields: NSFileProviderItemFields,
        contents url: URL?,
        options: NSFileProviderCreateItemOptions = [],
        request: NSFileProviderRequest,
        completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void
    ) -> Progress {
        completionHandler(nil, [], false, Self.readOnly)
        return Progress()
    }

    func modifyItem(
        _ item: NSFileProviderItem,
        baseVersion version: NSFileProviderItemVersion,
        changedFields: NSFileProviderItemFields,
        contents newContents: URL?,
        options: NSFileProviderModifyItemOptions = [],
        request: NSFileProviderRequest,
        completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void
    ) -> Progress {
        completionHandler(nil, [], false, Self.readOnly)
        return Progress()
    }

    func deleteItem(
        identifier: NSFileProviderItemIdentifier,
        baseVersion version: NSFileProviderItemVersion,
        options: NSFileProviderDeleteItemOptions = [],
        request: NSFileProviderRequest,
        completionHandler: @escaping (Error?) -> Void
    ) -> Progress {
        completionHandler(Self.readOnly)
        return Progress()
    }

    func enumerator(
        for containerItemIdentifier: NSFileProviderItemIdentifier,
        request: NSFileProviderRequest
    ) throws -> NSFileProviderEnumerator {
        if containerItemIdentifier != .workingSet && ProviderCatalog.path(for: containerItemIdentifier) == nil {
            throw NSFileProviderError(.noSuchItem)
        }
        return ProviderEnumerator(container: containerItemIdentifier, source: source)
    }

    /// Writing through Files is not in this cut.
    static let readOnly = NSError(domain: NSCocoaErrorDomain, code: NSFeatureUnsupportedError)
}

/// Lists one folder, or every entry for the working set.
///
/// Changes are reported by expiring the anchor: when the catalog moved, the
/// system enumerates again from scratch, which for a catalog is a listing.
final class ProviderEnumerator: NSObject, NSFileProviderEnumerator {
    private let container: NSFileProviderItemIdentifier
    private let source: ProviderSource

    init(container: NSFileProviderItemIdentifier, source: ProviderSource) {
        self.container = container
        self.source = source
    }

    func invalidate() {}

    func enumerateItems(for observer: NSFileProviderEnumerationObserver, startingAt page: NSFileProviderPage) {
        do {
            let entries = try source.entries()
            providerLog.info("enumerate \(self.container.rawValue, privacy: .public): \(entries.count) entries")
            let listed: [ProviderEntry]
            if container == .workingSet {
                listed = entries.values.sorted { $0.path < $1.path }
            } else {
                listed = ProviderCatalog.children(of: ProviderCatalog.path(for: container) ?? "", in: entries)
            }
            observer.didEnumerate(listed.map(ProviderItem.init))
            observer.finishEnumerating(upTo: nil)
        } catch let failure as ProviderSource.Failure {
            providerLog.error("enumerate \(self.container.rawValue, privacy: .public) failed: \(String(describing: failure), privacy: .public)")
            observer.finishEnumeratingWithError(failure.providerError)
        } catch {
            providerLog.error("enumerate failed: \(error.localizedDescription, privacy: .public)")
            observer.finishEnumeratingWithError(error)
        }
    }

    func enumerateChanges(for observer: NSFileProviderChangeObserver, from anchor: NSFileProviderSyncAnchor) {
        do {
            let current = NSFileProviderSyncAnchor(ProviderCatalog.anchor(of: try source.entries()))
            if current == anchor {
                observer.finishEnumeratingChanges(upTo: anchor, moreComing: false)
            } else {
                observer.finishEnumeratingWithError(NSFileProviderError(.syncAnchorExpired))
            }
        } catch let failure as ProviderSource.Failure {
            observer.finishEnumeratingWithError(failure.providerError)
        } catch {
            observer.finishEnumeratingWithError(error)
        }
    }

    func currentSyncAnchor(completionHandler: @escaping (NSFileProviderSyncAnchor?) -> Void) {
        let entries = (try? source.entries()) ?? [:]
        completionHandler(NSFileProviderSyncAnchor(ProviderCatalog.anchor(of: entries)))
    }
}
