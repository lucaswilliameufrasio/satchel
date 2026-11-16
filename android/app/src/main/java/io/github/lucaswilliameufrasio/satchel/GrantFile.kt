package io.github.lucaswilliameufrasio.satchel

/**
 * Turn the three-field join result into the grant file the catalog opens.
 *
 * A `recovery` field is rejected. The message never includes the joined text.
 */
internal fun assembleGrant(joined: String, deviceId: String, work: String): String {
    if (!isDeviceId(deviceId) || work.isEmpty()) {
        throw IllegalArgumentException("catalog failed")
    }
    val fields = parseStringObject(joined)
    if (fields.containsKey("recovery")) {
        throw IllegalArgumentException("catalog failed")
    }
    val names = fields.keys.toMutableList()
    names.sort()
    if (names != listOf("bucket_credential", "device_key_password", "endpoint")) {
        throw IllegalArgumentException("catalog failed")
    }
    val endpoint = fields.getValue("endpoint")
    val credential = fields.getValue("bucket_credential")
    val password = fields.getValue("device_key_password")
    if (endpoint.isEmpty() || credential.isEmpty() || password.isEmpty()) {
        throw IllegalArgumentException("catalog failed")
    }
    return buildString {
        append("{\"endpoint\":")
        append(jsonString(endpoint))
        append(",\"bucket_credential\":")
        append(jsonString(credential))
        append(",\"device_key_password\":")
        append(jsonString(password))
        append(",\"device_id\":")
        append(jsonString(deviceId))
        append(",\"work\":")
        append(jsonString(work))
        append('}')
    }
}

private fun isDeviceId(value: String): Boolean {
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

private fun parseStringObject(text: String): Map<String, String> {
    val cursor = JsonCursor(text)
    val fields = cursor.parseObject()
    if (!cursor.finished()) {
        throw IllegalArgumentException("catalog failed")
    }
    return fields
}

internal fun jsonString(value: String): String {
    val out = StringBuilder()
    out.append('"')
    for (character in value) {
        when (character) {
            '"' -> out.append("\\\"")
            '\\' -> out.append("\\\\")
            '\n' -> out.append("\\n")
            '\r' -> out.append("\\r")
            '\t' -> out.append("\\t")
            else -> {
                if (character.code < 0x20) {
                    val hex = character.code.toString(16).padStart(4, '0')
                    out.append("\\u")
                    out.append(hex)
                } else {
                    out.append(character)
                }
            }
        }
    }
    out.append('"')
    return out.toString()
}

internal class JsonCursor(private val text: String) {
    private var index = 0

    fun finished(): Boolean {
        skip()
        return index == text.length
    }

    fun parseObject(): Map<String, String> {
        skip()
        expect('{')
        val fields = linkedMapOf<String, String>()
        skip()
        if (peek() == '}') {
            index += 1
            return fields
        }
        while (index < text.length) {
            skip()
            val key = parseString()
            skip()
            expect(':')
            skip()
            val value = parseString()
            if (fields.containsKey(key)) {
                fail()
            }
            fields[key] = value
            skip()
            if (peek() == ',') {
                index += 1
                continue
            }
            if (peek() == '}') {
                index += 1
                return fields
            }
            fail()
        }
        fail()
    }

    /** Array of objects whose values are strings or integers. */
    fun parseObjectArray(): List<Map<String, String>> {
        skip()
        expect('[')
        val items = mutableListOf<Map<String, String>>()
        skip()
        if (peek() == ']') {
            index += 1
            return items
        }
        while (index < text.length) {
            items.add(parseMixedObject())
            skip()
            if (peek() == ',') {
                index += 1
                continue
            }
            if (peek() == ']') {
                index += 1
                return items
            }
            fail()
        }
        fail()
    }

    /** Object whose values are strings or integers. Integers are returned as decimal text. */
    fun parseMixedObject(): Map<String, String> {
        skip()
        expect('{')
        val fields = linkedMapOf<String, String>()
        skip()
        if (peek() == '}') {
            index += 1
            return fields
        }
        while (index < text.length) {
            skip()
            val key = parseString()
            skip()
            expect(':')
            skip()
            val value = parseScalar()
            if (fields.containsKey(key)) {
                fail()
            }
            fields[key] = value
            skip()
            if (peek() == ',') {
                index += 1
                continue
            }
            if (peek() == '}') {
                index += 1
                return fields
            }
            fail()
        }
        fail()
    }

    private fun parseScalar(): String {
        if (peek() == '"') {
            return parseString()
        }
        val start = index
        if (peek() == '-') {
            index += 1
        }
        var digits = 0
        while (index < text.length && text[index] in '0'..'9') {
            index += 1
            digits += 1
        }
        if (digits == 0) {
            fail()
        }
        return text.substring(start, index)
    }

    private fun parseString(): String {
        expect('"')
        val out = StringBuilder()
        while (index < text.length) {
            val character = text[index]
            index += 1
            if (character == '"') {
                return out.toString()
            }
            if (character == '\\') {
                out.append(escape())
                continue
            }
            if (character.code < 0x20) {
                fail()
            }
            out.append(character)
        }
        fail()
    }

    private fun escape(): Char {
        if (index >= text.length) {
            fail()
        }
        val character = text[index]
        index += 1
        return when (character) {
            '"', '\\', '/' -> character
            'b' -> '\b'
            'f' -> '\u000C'
            'n' -> '\n'
            'r' -> '\r'
            't' -> '\t'
            'u' -> unicode()
            else -> fail()
        }
    }

    private fun unicode(): Char {
        if (index + 4 > text.length) {
            fail()
        }
        var code = 0
        for (offset in 0 until 4) {
            val digit = hex(text[index + offset])
            code = (code shl 4) + digit
        }
        index += 4
        return code.toChar()
    }

    private fun hex(character: Char): Int {
        return when (character) {
            in '0'..'9' -> character.code - '0'.code
            in 'a'..'f' -> character.code - 'a'.code + 10
            in 'A'..'F' -> character.code - 'A'.code + 10
            else -> fail()
        }
    }

    private fun expect(character: Char) {
        if (peek() != character) {
            fail()
        }
        index += 1
    }

    private fun peek(): Char {
        if (index >= text.length) {
            fail()
        }
        return text[index]
    }

    private fun skip() {
        while (index < text.length) {
            val character = text[index]
            if (character != ' ' && character != '\n' && character != '\r' && character != '\t') {
                return
            }
            index += 1
        }
    }

    private fun fail(): Nothing {
        throw IllegalArgumentException("catalog failed")
    }
}
