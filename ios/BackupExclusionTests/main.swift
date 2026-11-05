import Foundation

let root = FileManager.default.temporaryDirectory.appendingPathComponent(
    "satchel-backup-\(UUID().uuidString)",
    isDirectory: true
)
try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
let support = root.appendingPathComponent("satchel", isDirectory: true)
let work = support.appendingPathComponent("work", isDirectory: true)
try FileManager.default.createDirectory(at: support, withIntermediateDirectories: true)
try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
let grant = support.appendingPathComponent("grant.json")
let device = support.appendingPathComponent("device-id")
let label = support.appendingPathComponent("device-label")
try Data().write(to: grant)
try Data("device".utf8).write(to: device)
try Data("apelido".utf8).write(to: label)

let paths = [support, work, grant, device, label]
for path in paths {
    try BackupExclusion.exclude(path)
}
for path in paths {
    let excluded = try BackupExclusion.isExcluded(path)
    if !excluded {
        fputs("not excluded: \(path.lastPathComponent)\n", stderr)
        exit(1)
    }
}
print("backup-exclusion ok \(paths.count)")
try FileManager.default.removeItem(at: root)
