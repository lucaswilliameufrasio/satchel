package io.github.lucaswilliameufrasio.satchel

/** The three fields a scanned invite fills in. */
internal data class InviteFields(
    val address: String,
    val code: String,
    val fingerprint: String,
)

/**
 * Read the invite JSON the desktop puts in the QR image.
 *
 * A `recovery` field is rejected. The thrown message does not include the payload.
 */
internal fun inviteFields(text: String): InviteFields {
    val cursor = JsonCursor(text)
    val parsed = cursor.parseMixedObject()
    if (!cursor.finished()) {
        throw IllegalArgumentException("catalog failed")
    }
    if (parsed.containsKey("recovery")) {
        throw IllegalArgumentException("catalog failed")
    }
    val names = parsed.keys.toMutableList()
    names.sort()
    if (names != listOf("address", "code", "fingerprint", "v")) {
        throw IllegalArgumentException("catalog failed")
    }
    if (parsed.getValue("v") != "1") {
        throw IllegalArgumentException("catalog failed")
    }
    val address = parsed.getValue("address")
    val code = parsed.getValue("code")
    val fingerprint = parsed.getValue("fingerprint")
    if (address.isEmpty() || code.length != 6 || fingerprint.length != 64) {
        throw IllegalArgumentException("catalog failed")
    }
    for (character in code) {
        if (character !in '0'..'9') {
            throw IllegalArgumentException("catalog failed")
        }
    }
    for (character in fingerprint) {
        val digit = character in '0'..'9'
        val lower = character in 'a'..'f'
        if (!digit && !lower) {
            throw IllegalArgumentException("catalog failed")
        }
    }
    return InviteFields(address, code, fingerprint)
}
