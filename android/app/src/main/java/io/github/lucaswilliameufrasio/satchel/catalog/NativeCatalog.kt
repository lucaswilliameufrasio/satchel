package io.github.lucaswilliameufrasio.satchel.catalog

import org.json.JSONArray

/**
 * Catalog backed by the Rust core.
 *
 * The JSON has no file bytes. Marks live in this session. They are not written
 * back as a new snapshot.
 */
class CoreCatalog(private val root: String) : CatalogSource {
    private val present = mutableSetOf<String>()
    private val saved = mutableMapOf<String, Follow>()

    override fun listMetadata(): List<CatalogRow> {
        val text = NativeCatalog.sampleCatalog(root)
        val array = JSONArray(text)
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
        val bytes = NativeCatalog.readFile(root, path)
        present.add(path)
        return bytes
    }

    override fun remember(path: String, follow: Follow) {
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
}
