import FileProvider

/// Satchel in the Files app: present while this phone holds a grant, gone without one.
///
/// The extension (`SatchelFiles`) serves the catalog read-only. The app only adds
/// or removes the domain and says when the catalog moved.
enum FilesDomain {
    static let domain = NSFileProviderDomain(
        identifier: NSFileProviderDomainIdentifier("satchel"),
        displayName: "Satchel"
    )

    static func update(hasGrant: Bool) {
        if hasGrant {
            NSFileProviderManager.add(domain) { _ in }
        } else {
            NSFileProviderManager.remove(domain) { _ in }
        }
    }

    /// Files asks the extension for the catalog again.
    static func catalogChanged() {
        NSFileProviderManager(for: domain)?.signalEnumerator(for: .workingSet) { _ in }
    }
}
