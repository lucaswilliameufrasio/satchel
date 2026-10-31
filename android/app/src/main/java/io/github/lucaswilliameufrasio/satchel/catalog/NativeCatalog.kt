package io.github.lucaswilliameufrasio.satchel.catalog

import org.json.JSONArray

/**
 * Catalog backed by the Rust core.
 *
 * The JSON has no file bytes. A mark is written into a new snapshot.
 */
class CoreCatalog(root: String) : CatalogSource by SessionMarks(
    listJson = { NativeCatalog.sampleCatalog(root) },
    readBytes = { path -> NativeCatalog.readFile(root, path) },
    persist = { path, follow ->
        NativeCatalog.markSample(root, path, followWire(follow))
    },
)

/**
 * Catalog of a repository described by a local JSON spec.
 *
 * The spec is a path, not the secret itself. A mark is written into a new snapshot.
 */
class SharedCatalog(specPath: String) : CatalogSource by SessionMarks(
    listJson = { NativeCatalog.catalogStore(specPath) },
    readBytes = { path -> NativeCatalog.readStore(specPath, path) },
    persist = { path, follow ->
        NativeCatalog.markStore(specPath, path, followWire(follow))
    },
)

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
            rows.add(
                CatalogRow(
                    path = path,
                    isDirectory = item.getBoolean("directory"),
                    size = item.getLong("size"),
                    versionUnix = item.getLong("versionUnix"),
                    devices = devices,
                    follow = saved[path] ?: inherited(path, listedFollow),
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

    external fun markSample(root: String, path: String, follow: String)

    external fun markStore(specPath: String, path: String, follow: String)

    external fun catalogStore(specPath: String): String

    external fun readStore(specPath: String, path: String): ByteArray
}
