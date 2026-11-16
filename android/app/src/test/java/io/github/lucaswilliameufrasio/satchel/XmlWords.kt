package io.github.lucaswilliameufrasio.satchel

import java.io.File
import java.util.Locale
import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Element

/**
 * [Words] read straight from the app's `strings.xml`, for JVM tests.
 *
 * [folder] is a resource folder such as `values` (English) or `values-pt`. Its
 * strings sit on top of `values`, the way Android falls back. Plural choice follows
 * the CLDR rule for the language: Portuguese uses `one` for 0 and 1, English for 1.
 */
class XmlWords(private val folder: String) : Words {
    private val strings = mutableMapOf<String, String>()
    private val plurals = mutableMapOf<String, Map<String, String>>()

    init {
        load("values")
        if (folder != "values") {
            load(folder)
        }
    }

    override fun text(id: Int, vararg args: Any): String {
        val name = nameOf(R.string::class.java, id)
        val value = strings[name] ?: error("missing string $name in $folder")
        return String.format(Locale.ROOT, value, *args)
    }

    override fun count(id: Int, count: Int, vararg args: Any): String {
        val name = nameOf(R.plurals::class.java, id)
        val items = plurals[name] ?: error("missing plurals $name in $folder")
        val portuguese = folder.startsWith("values-pt")
        val quantity = if (portuguese && (count == 0 || count == 1)) {
            "one"
        } else if (!portuguese && count == 1) {
            "one"
        } else {
            "other"
        }
        val value = items[quantity] ?: items.getValue("other")
        return String.format(Locale.ROOT, value, *args)
    }

    private fun load(name: String) {
        val file = File(resourceRoot(), "$name/strings.xml")
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file)
        val root = document.documentElement
        val children = root.childNodes
        for (index in 0 until children.length) {
            val node = children.item(index)
            if (node !is Element) {
                continue
            }
            val key = node.getAttribute("name")
            if (node.tagName == "string") {
                strings[key] = unescape(node.textContent)
            } else if (node.tagName == "plurals") {
                val items = mutableMapOf<String, String>()
                val entries = node.getElementsByTagName("item")
                for (item in 0 until entries.length) {
                    val entry = entries.item(item) as Element
                    items[entry.getAttribute("quantity")] = unescape(entry.textContent)
                }
                plurals[key] = items
            }
        }
    }

    private companion object {
        fun resourceRoot(): File {
            // Gradle runs unit tests from the module folder.
            val direct = File("src/main/res")
            if (direct.isDirectory) {
                return direct
            }
            return File("app/src/main/res")
        }

        fun nameOf(type: Class<*>, id: Int): String {
            for (field in type.fields) {
                if (field.type == Int::class.javaPrimitiveType && field.getInt(null) == id) {
                    return field.name
                }
            }
            error("no resource with id $id")
        }

        fun unescape(raw: String): String {
            val out = StringBuilder()
            var index = 0
            while (index < raw.length) {
                val character = raw[index]
                if (character == '\\' && index + 1 < raw.length) {
                    val next = raw[index + 1]
                    out.append(if (next == 'n') '\n' else next)
                    index += 2
                    continue
                }
                out.append(character)
                index += 1
            }
            return out.toString()
        }
    }
}
