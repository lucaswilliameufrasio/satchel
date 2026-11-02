package io.github.lucaswilliameufrasio.satchel.catalog

import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

/** How this phone treats one file or folder. */
enum class Follow {
    See,
    Once,
    Always,
}

/** Label shown for a follow state. */
fun followLabel(follow: Follow): String {
    return when (follow) {
        Follow.See -> "Ver"
        Follow.Once -> "Baixar agora"
        Follow.Always -> "Sincronizar sempre"
    }
}

/** Wire value stored in a snapshot description. */
internal fun followWire(follow: Follow): String {
    return when (follow) {
        Follow.See -> "see"
        Follow.Once -> "once"
        Follow.Always -> "always"
    }
}

/**
 * One catalog row.
 *
 * The bytes of the file are not part of the row. Building the catalog does
 * not read them.
 */
data class CatalogRow(
    val path: String,
    val isDirectory: Boolean,
    val size: Long,
    val versionUnix: Long,
    val devices: List<String>,
    val follow: Follow,
)

/** Metadata and, only when a state asks for it, file bytes. */
interface CatalogSource {
    fun listMetadata(): List<CatalogRow>

    fun hasContent(path: String): Boolean

    fun readContent(path: String): ByteArray

    fun remember(path: String, follow: Follow)
}

/** First open lists metadata. Content is read only after a mark that needs it. */
class CatalogSession(private val source: CatalogSource) {
    var rows: List<CatalogRow> = emptyList()
        private set

    var contentReads: Int = 0
        private set

    /** Show the catalog. This does not read file bytes. */
    fun open() {
        rows = source.listMetadata()
    }

    /** Remember a mark and read bytes only when that mark needs them. */
    fun choose(path: String, follow: Follow) {
        source.remember(path, follow)
        if (follow != Follow.See) {
            val listed = source.listMetadata()
            for (row in listed) {
                if (row.isDirectory) {
                    continue
                }
                if (!affected(path, row.path)) {
                    continue
                }
                if (row.follow == Follow.See) {
                    continue
                }
                if (source.hasContent(row.path)) {
                    continue
                }
                source.readContent(row.path)
                contentReads += 1
            }
        }
        rows = source.listMetadata()
    }
}

/** Name, sync time, and devices. No file bytes. */
fun rowTitle(row: CatalogRow): String {
    return row.path
}

private val syncedFormatter: DateTimeFormatter =
    DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm 'UTC'").withZone(ZoneOffset.UTC)

/** Sync time shown on the phone, in UTC so the catalog does not depend on the zone. */
fun rowWhen(row: CatalogRow): String {
    return syncedFormatter.format(Instant.ofEpochSecond(row.versionUnix))
}

/** One opaque id per line. Empty means no device keeps this file. */
fun deviceText(row: CatalogRow): String {
    if (row.devices.isEmpty()) {
        return "nenhum"
    }
    var text = ""
    for (device in row.devices) {
        if (text.isNotEmpty()) {
            text += "\n"
        }
        text += device
    }
    return text
}

/** The two metadata lines under the name. */
fun rowDetails(row: CatalogRow): String {
    val whenText = Instant.ofEpochSecond(row.versionUnix).toString()
    val devices = if (row.devices.isEmpty()) {
        "nenhum"
    } else {
        var text = ""
        for (device in row.devices) {
            if (text.isNotEmpty()) {
                text += ", "
            }
            text += device
        }
        text
    }
    return "Sincronizado em $whenText\nAparelhos: $devices"
}

private fun affected(marked: String, candidate: String): Boolean {
    if (candidate == marked) {
        return true
    }
    return candidate.startsWith("$marked/")
}
