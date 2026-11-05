package io.github.lucaswilliameufrasio.satchel.catalog

import io.github.lucaswilliameufrasio.satchel.JsonCursor
import io.github.lucaswilliameufrasio.satchel.R
import io.github.lucaswilliameufrasio.satchel.Words
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale
import kotlin.math.roundToInt

/** How this phone treats one file or folder. */
enum class Follow {
    See,
    Once,
    Always,
}

const val SAMPLE_READER_ID = "fedcba9876543210fedcba9876543210"

/** Labels of the join fields, in the order the screen shows them. */
val ENTRY_FIELDS = listOf(
    R.string.field_address,
    R.string.field_code,
    R.string.field_fingerprint,
    R.string.field_nickname,
)

/** Label shown for a follow state. */
fun followLabel(follow: Follow, words: Words): String {
    return when (follow) {
        Follow.See -> words.text(R.string.follow_see)
        Follow.Once -> words.text(R.string.follow_once)
        Follow.Always -> words.text(R.string.follow_always)
    }
}

/** Support sentence under a follow choice. */
fun followDetail(follow: Follow, directory: Boolean, words: Words): String {
    if (follow == Follow.Once) {
        return words.text(R.string.follow_once_detail)
    }
    if (follow == Follow.Always) {
        if (directory) {
            return words.text(R.string.follow_always_folder_detail)
        }
        return words.text(R.string.follow_always_detail)
    }
    return words.text(R.string.follow_see_detail)
}

/** Wire value stored in a snapshot description. */
internal fun followWire(follow: Follow): String {
    return when (follow) {
        Follow.See -> "see"
        Follow.Once -> "once"
        Follow.Always -> "always"
    }
}

/** Facts the snapshot published for one device. Absent fields stay null. */
data class DeviceHolder(
    val id: String,
    val name: String?,
    val os: String?,
    val enrolledUnix: Long?,
    val lastUnix: Long?,
)

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
    val holders: List<DeviceHolder> = emptyList(),
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

/** Arguments the mark call receives. An empty name omits the published apelido. */
data class MarkRequest(
    val path: String,
    val follow: String,
    val name: String,
    val os: String,
)

/** Name, sync time, and devices. No file bytes. */
fun rowTitle(row: CatalogRow): String {
    return row.path
}

/**
 * `versionUnix` in `zone`, in the language's `clock_pattern` plus that zone's short name.
 *
 * Portuguese reads `dd/MM/yyyy HH:mm`; English reads `yyyy-MM-dd HH:mm`, as on iOS and the desktop, so the
 * day and month are never ambiguous. A UTC zone keeps the short name `UTC`, so the
 * clock does not depend on the machine.
 */
fun syncedWhen(versionUnix: Long, zone: ZoneId, words: Words): String {
    val locale = Locale.forLanguageTag(words.text(R.string.format_locale))
    val formatter = DateTimeFormatter.ofPattern(words.text(R.string.clock_pattern), locale)
    val clock = formatter.withZone(zone).format(Instant.ofEpochSecond(versionUnix))
    return "$clock ${shortZoneName(zone)}"
}

/** Sync time shown on the phone. */
fun rowWhen(row: CatalogRow, zone: ZoneId, words: Words): String {
    return syncedWhen(row.versionUnix, zone, words)
}

/** Short name of `zone`. UTC stays `UTC` rather than `Z`. */
fun shortZoneName(zone: ZoneId): String {
    if (zone.normalized() == ZoneOffset.UTC) {
        return "UTC"
    }
    return zone.getDisplayName(TextStyle.SHORT, Locale.US)
}

/** Size text. Negative sizes, which the JSON does not produce, read as `0 B`. */
fun byteCount(size: Long): String {
    if (size < 0) {
        return "0 B"
    }
    val units = arrayOf("B", "KiB", "MiB", "GiB", "TiB")
    var value = size.toDouble()
    var unit = 0
    while (value >= 1024.0 && unit < units.size - 1) {
        value /= 1024.0
        unit += 1
    }
    if (unit == 0) {
        return "$size B"
    }
    val tenths = (value * 10.0).roundToInt()
    val whole = tenths / 10
    val fraction = tenths % 10
    if (fraction == 0 || value >= 10.0) {
        return "${value.roundToInt()} ${units[unit]}"
    }
    return "$whole.$fraction ${units[unit]}"
}

/** Support line under a catalog row. A folder never shows `0 B`. */
fun catalogMeta(row: CatalogRow, zone: ZoneId, words: Words): String {
    val whenText = syncedWhen(row.versionUnix, zone, words)
    if (row.isDirectory) {
        return "${words.text(R.string.folder)} · $whenText"
    }
    return "${byteCount(row.size)} · $whenText"
}

/** How many devices keep a copy. This is not the opaque id. */
fun deviceCountLabel(devices: List<String>, words: Words): String {
    if (devices.isEmpty()) {
        return words.text(R.string.devices_none)
    }
    return words.count(R.plurals.device_count, devices.size, devices.size)
}

/** The row text the list shows: path, size or folder, date, count, and state. */
fun catalogRowText(row: CatalogRow, zone: ZoneId, words: Words): String {
    return "${rowTitle(row)}\n${catalogMeta(row, zone, words)}\n${deviceCountLabel(row.devices, words)}\n${followLabel(row.follow, words)}"
}

/** Opaque id in groups of four. Anything else is returned unchanged. */
fun groupedDeviceId(id: String): String {
    if (id.length != 32) {
        return id
    }
    for (character in id) {
        val digit = character in '0'..'9'
        val lower = character in 'a'..'f'
        val upper = character in 'A'..'F'
        if (!digit && !lower && !upper) {
            return id
        }
    }
    val grouped = StringBuilder()
    for (index in id.indices) {
        if (index > 0 && index % 4 == 0) {
            grouped.append(' ')
        }
        grouped.append(id[index])
    }
    return grouped.toString()
}

/**
 * This phone's id for card titles.
 *
 * Without a grant, the sample reader is this phone. With a grant, only an id
 * already on disk is used. This does not create the file.
 */
fun localDeviceId(hasGrant: Boolean, storedDeviceId: String?): String {
    if (!hasGrant) {
        return SAMPLE_READER_ID
    }
    if (storedDeviceId != null && opaqueId(storedDeviceId)) {
        return storedDeviceId
    }
    return ""
}

/** Header lines on the file screen: size or folder, then the same date. */
fun fileHeaderLines(row: CatalogRow, zone: ZoneId, words: Words): List<String> {
    val size = if (row.isDirectory) {
        words.text(R.string.folder)
    } else {
        byteCount(row.size)
    }
    return listOf(
        words.text(R.string.header_size),
        size,
        words.text(R.string.header_synced),
        syncedWhen(row.versionUnix, zone, words),
    )
}

/** The three states, in order, with support text and a check on the selected one. */
fun fileChoiceLines(row: CatalogRow, busy: Boolean, words: Words): List<String> {
    val lines = mutableListOf<String>()
    for (follow in Follow.entries) {
        val mark = if (row.follow == follow) {
            "✓ "
        } else {
            ""
        }
        lines.add("$mark${followLabel(follow, words)}")
        lines.add(followDetail(follow, row.isDirectory, words))
    }
    if (busy) {
        lines.add(words.text(R.string.saving_choice))
    }
    return lines
}

/**
 * Lines for one device card.
 *
 * A missing holder field is omitted. This phone is titled This device.
 * Another phone uses its published apelido, or the grouped id when it has none.
 */
fun deviceCardLines(
    deviceId: String,
    card: DeviceHolder?,
    local: Boolean,
    zone: ZoneId,
    words: Words,
): List<String> {
    val lines = mutableListOf<String>()
    val title = deviceTitle(deviceId, card, local, words)
    lines.add(title)
    if (card != null) {
        val name = card.name
        if (name != null && title != name) {
            lines.add(words.text(R.string.card_nickname, name))
        }
        val os = card.os
        if (os != null) {
            lines.add(words.text(R.string.card_system, os))
        }
        val enrolled = card.enrolledUnix
        if (enrolled != null) {
            lines.add(words.text(R.string.card_enrolled, syncedWhen(enrolled, zone, words)))
        }
        val last = card.lastUnix
        if (last != null) {
            lines.add(words.text(R.string.card_last_sent, syncedWhen(last, zone, words)))
        }
    }
    val grouped = groupedDeviceId(deviceId)
    if (title != grouped) {
        lines.add(grouped)
    }
    return lines
}

/** Cards for every device on the row. An empty list says nobody keeps a copy. */
fun deviceSectionLines(row: CatalogRow, localDeviceId: String, zone: ZoneId, words: Words): List<String> {
    if (row.devices.isEmpty()) {
        return listOf(words.text(R.string.devices_none_keep))
    }
    val lines = mutableListOf<String>()
    for (device in row.devices) {
        val card = holderFor(device, row.holders)
        val local = device == localDeviceId && localDeviceId.isNotEmpty()
        lines.addAll(deviceCardLines(device, card, local, zone, words))
    }
    return lines
}

/** Parse the `holders` array. Unknown keys are ignored. A missing field stays absent. */
fun parseHolders(json: String): List<DeviceHolder> {
    val cursor = JsonCursor(json)
    val objects = cursor.parseObjectArray()
    if (!cursor.finished()) {
        throw IllegalArgumentException("catalog failed")
    }
    val cards = mutableListOf<DeviceHolder>()
    for (fields in objects) {
        val id = fields["id"] ?: continue
        if (id.isEmpty()) {
            continue
        }
        val lastText = fields["lastUnix"] ?: continue
        val last = lastText.toLongOrNull() ?: continue
        val name = fields["name"]?.takeIf { value -> value.isNotEmpty() }
        val os = fields["os"]?.takeIf { value -> value.isNotEmpty() }
        val enrolled = fields["enrolledUnix"]?.toLongOrNull()
        cards.add(
            DeviceHolder(
                id = id,
                name = name,
                os = os,
                enrolledUnix = enrolled,
                lastUnix = last,
            ),
        )
    }
    return cards
}

/**
 * What Entrar stores.
 *
 * Empty is stored and still marks. A label outside 1..64 characters, or one
 * with a control character, is rejected and is not sent.
 */
fun labelStoredFromEntry(typed: String): String? {
    val trimmed = typed.trim()
    if (trimmed.isEmpty()) {
        return ""
    }
    if (!acceptableApelido(trimmed)) {
        return null
    }
    return trimmed
}

/** Field default: the saved apelido, or iPhone when nothing valid is saved. */
fun openingApelido(saved: String): String {
    val trimmed = saved.trim()
    if (acceptableApelido(trimmed)) {
        return trimmed
    }
    return "iPhone"
}

/**
 * A scanned invite fills the three invite fields and leaves the apelido.
 * It does not submit.
 */
fun pairingAfterQr(
    apelido: String,
    address: String,
    code: String,
    fingerprint: String,
): PairingDraft {
    return PairingDraft(
        address = address,
        code = code,
        fingerprint = fingerprint,
        apelido = apelido,
        submit = false,
    )
}

data class PairingDraft(
    val address: String,
    val code: String,
    val fingerprint: String,
    val apelido: String,
    val submit: Boolean,
)

/**
 * The mark call for a stored apelido.
 *
 * A valid apelido is sent with the Android system. An empty apelido still
 * marks: name and system are empty, so the holder JSON omits `name`.
 */
fun markRequest(path: String, follow: Follow, storedApelido: String): MarkRequest {
    val trimmed = storedApelido.trim()
    if (acceptableApelido(trimmed)) {
        return MarkRequest(path, followWire(follow), trimmed, "Android")
    }
    return MarkRequest(path, followWire(follow), "", "")
}

private fun deviceTitle(deviceId: String, card: DeviceHolder?, local: Boolean, words: Words): String {
    if (local) {
        return words.text(R.string.this_device)
    }
    val name = card?.name
    if (name != null && name.isNotEmpty()) {
        return name
    }
    return groupedDeviceId(deviceId)
}

internal fun holderFor(deviceId: String, cards: List<DeviceHolder>): DeviceHolder? {
    for (card in cards) {
        if (card.id == deviceId) {
            return card
        }
    }
    return null
}

private fun acceptableApelido(value: String): Boolean {
    val count = value.codePointCount(0, value.length)
    if (count == 0 || count > 64) {
        return false
    }
    var index = 0
    while (index < value.length) {
        val code = value.codePointAt(index)
        if (Character.isISOControl(code)) {
            return false
        }
        index += Character.charCount(code)
    }
    return true
}

private fun opaqueId(value: String): Boolean {
    if (value.length != 32) {
        return false
    }
    for (character in value) {
        val digit = character in '0'..'9'
        val lower = character in 'a'..'f'
        if (!digit && !lower) {
            return false
        }
    }
    return true
}

private fun affected(marked: String, candidate: String): Boolean {
    if (candidate == marked) {
        return true
    }
    return candidate.startsWith("$marked/")
}
