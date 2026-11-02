import Foundation

enum BackupExclusion {
    static func exclude(_ url: URL) throws {
        var target = url
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try target.setResourceValues(values)
    }

    static func isExcluded(_ url: URL) throws -> Bool {
        let values = try url.resourceValues(forKeys: [.isExcludedFromBackupKey])
        if let excluded = values.isExcludedFromBackup {
            return excluded
        }
        return false
    }
}
