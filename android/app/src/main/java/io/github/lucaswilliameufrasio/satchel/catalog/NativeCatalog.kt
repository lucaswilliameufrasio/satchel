package io.github.lucaswilliameufrasio.satchel.catalog

import org.json.JSONArray

/**
 * Catalog backed by the Rust core.
 *
 * The JSON has no file bytes. A mark is written into a new snapshot.
 */
class CoreCatalog(
    root: String,
    apelido: () -> String = { "" },
) : CatalogSource by SessionMarks(
    listJson = { NativeCatalog.sampleCatalog(root) },
    readBytes = { path -> NativeCatalog.readFile(root, path) },
    persist = { path, follow ->
        val request = markRequest(path, follow, apelido())
        NativeCatalog.markSample(root, request.path, request.follow, request.name, request.os)
    },
)

/**
 * Catalog of a repository described by a local JSON spec.
 *
 * The spec is a path, not the secret itself. A mark is written into a new snapshot.
 */
class SharedCatalog(
    specPath: String,
    apelido: () -> String = { "" },
) : CatalogSource by SessionMarks(
    listJson = { NativeCatalog.catalogStore(specPath) },
    readBytes = { path -> NativeCatalog.readStore(specPath, path) },
    persist = { path, follow ->
        val request = markRequest(path, follow, apelido())
        NativeCatalog.markStore(specPath, request.path, request.follow, request.name, request.os)
    },
)

/**
 * Catalog opened from a pairing grant.
 *
 * The grant file holds the device key password. It does not hold the recovery password.
 */
class GrantCatalog(
    specPath: String,
    apelido: () -> String = { "" },
    folder: () -> PhoneFolder? = { null },
) : CatalogSource by SessionMarks(
    listJson = { NativeCatalog.catalogGrant(specPath) },
    readBytes = { path -> NativeCatalog.readGrant(specPath, path) },
    persist = { path, follow ->
        val request = markRequest(path, follow, apelido())
        val chosen = folder()
        if (chosen == null) {
            NativeCatalog.markGrant(specPath, request.path, request.follow, request.name, request.os)
        } else {
            // With a folder, the mark runs a folder round so a download lands in it.
            NativeCatalog.syncFolder(
                specPath,
                chosen.tree,
                chosen.manifest,
                request.path,
                request.follow,
                request.name,
                request.os,
            )
        }
    },
)

/** A picked folder and the manifest file the app keeps for it, outside the folder. */
class PhoneFolder(val tree: FolderTree, val manifest: String)

private class SessionMarks(
    private val listJson: () -> String,
    private val readBytes: (String) -> ByteArray,
    private val persist: (String, Follow) -> Unit,
) : CatalogSource {
    private val present = mutableSetOf<String>()
    private val saved = mutableMapOf<String, Follow>()

    override fun listMetadata(): List<CatalogRow> {
        val array = JSONArray(listJson())
        val rows = mutableListOf<CatalogRow>()
        for (index in 0 until array.length()) {
            val item = array.getJSONObject(index)
            val devices = mutableListOf<String>()
            val listed = item.getJSONArray("devices")
            for (deviceIndex in 0 until listed.length()) {
                devices.add(listed.getString(deviceIndex))
            }
            val path = item.getString("path")
            val listedFollow = followFrom(item.getString("follow"))
            val holdersJson = if (item.has("holders") && !item.isNull("holders")) {
                item.getJSONArray("holders").toString()
            } else {
                "[]"
            }
            rows.add(
                CatalogRow(
                    path = path,
                    isDirectory = item.getBoolean("directory"),
                    size = item.getLong("size"),
                    versionUnix = item.getLong("versionUnix"),
                    devices = devices,
                    follow = saved[path] ?: inherited(path, listedFollow),
                    holders = parseHolders(holdersJson),
                ),
            )
        }
        return rows
    }

    override fun hasContent(path: String): Boolean {
        return present.contains(path)
    }

    override fun readContent(path: String): ByteArray {
        val bytes = readBytes(path)
        present.add(path)
        return bytes
    }

    override fun remember(path: String, follow: Follow) {
        persist(path, follow)
        saved[path] = follow
    }

    private fun inherited(path: String, listed: Follow): Follow {
        var best: Follow? = null
        var bestLength = -1
        for ((mark, follow) in saved) {
            if (path.startsWith("$mark/") && mark.length > bestLength) {
                best = follow
                bestLength = mark.length
            }
        }
        if (best != null) {
            return best
        }
        return listed
    }
}

private fun followFrom(text: String): Follow {
    return when (text) {
        "once" -> Follow.Once
        "always" -> Follow.Always
        else -> Follow.See
    }
}

object NativeCatalog {
    init {
        System.loadLibrary("satchel_ffi")
    }

    external fun sampleCatalog(root: String): String

    external fun readFile(root: String, path: String): ByteArray

    external fun markSample(root: String, path: String, follow: String, name: String, os: String)

    external fun markStore(specPath: String, path: String, follow: String, name: String, os: String)

    external fun catalogStore(specPath: String): String

    external fun readStore(specPath: String, path: String): ByteArray

    external fun catalogGrant(specPath: String): String

    external fun markGrant(specPath: String, path: String, follow: String, name: String, os: String)

    external fun readGrant(specPath: String, path: String): ByteArray

    external fun joinPairing(
        address: String,
        code: String,
        fingerprint: String,
        deviceName: String,
        os: String,
        deviceId: String,
    ): String

    external fun newDeviceId(): String

    /**
     * Create the repository. Returns `recovery password`, a newline, then the grant JSON to seal in the Keystore.
     * S3, R2 and B2 are refused when versioning is off or a lifecycle rule deletes objects.
     */
    external fun createRepository(spec: String, work: String): String

    /** `protection`, `exists`, `unreachable` or `fields` for the last refused create. */
    external fun createFailure(): String

    external fun isMaintainer(grant: String): Boolean

    /** Offer JSON: `code`, `address`, `fingerprint`, `invite`. [bind] may be empty. */
    external fun holdStart(grant: String, bind: String): String

    /** Empty until the other device has passed the code check. */
    external fun holdPoll(): String

    external fun holdDecide(grant: String, approve: Boolean)

    external fun holdClose()

    external fun invitePng(invite: String): ByteArray

    /** First half of a recovery change: `new password\nnew grant JSON`. Changes nothing. */
    external fun recoveryPrepare(grant: String): String

    /** Second half: true when the repository now opens with the new grant. */
    external fun recoveryCommit(old: String, new: String): Boolean

    /** True when the grant's password opens the repository. */
    external fun grantOpens(grant: String): Boolean

    /**
     * One folder round: the folder into the work copy, sync, and back.
     * [path] and [follow] may be empty; when set they mark first.
     * Returns report JSON; `"status":"unavailable"` means nothing was deleted.
     */
    external fun syncFolder(
        grant: String,
        tree: FolderTree,
        manifest: String,
        path: String,
        follow: String,
        name: String,
        os: String,
    ): String

    /**
     * Listen for wake notices in [stateDir], a private folder. [ip] empty means the
     * mesh address. Returns the address listened on, or an empty string.
     */
    external fun wakeStart(stateDir: String, ip: String): String

    /** Block up to [timeoutMs] for a notice. Notices within 5 s of the last count once. */
    external fun wakeWait(timeoutMs: Long): Boolean

    /** Stop listening. */
    external fun wakeStop()

    /** Canonical invite JSON. Rejects unknown fields, including a recovery password. */
    external fun parseInvite(text: String): String
}
