package io.github.lucaswilliameufrasio.satchel.catalog

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import io.github.lucaswilliameufrasio.satchel.R
import io.github.lucaswilliameufrasio.satchel.Words
import java.io.File
import java.io.IOException
import java.util.Locale
import org.json.JSONArray
import org.json.JSONObject

/**
 * The folder the person picked, as the Rust round sees it.
 *
 * The native side calls these four methods by name. Paths are relative and use `/`.
 * [list] must throw when the folder cannot be read: an empty answer would look
 * like the person deleted every file.
 */
interface FolderTree {
    /** JSON array of `{"path","size","modified"}` for every regular file. */
    fun list(): String

    /** Copy the folder file at [path] to the file [dest], by streams: it is never held whole. */
    fun copyIn(path: String, dest: String)

    /** Create or replace the folder file at [path] from the file [source], creating parent folders. */
    fun copyOut(source: String, path: String)

    /** Delete the file. A missing file is not an error. */
    fun remove(path: String)
}

/** Bytes per read when a file moves between the folder and the work copy. */
private const val COPY_BUFFER = 1 shl 20

internal fun listingJson(files: List<Triple<String, Long, Long>>): String {
    val array = JSONArray()
    for ((path, size, modified) in files) {
        array.put(JSONObject().put("path", path).put("size", size).put("modified", modified))
    }
    return array.toString()
}

private fun parts(path: String): List<String> {
    val pieces = path.split('/')
    for (piece in pieces) {
        if (piece.isEmpty() || piece == "." || piece == "..") {
            throw IOException("unsafe path")
        }
    }
    return pieces
}

/** A folder reached by path. Used by the tests; the app uses [SafTree]. */
class FileTree(private val root: File) : FolderTree {
    override fun list(): String {
        if (!root.isDirectory) {
            throw IOException("folder is gone")
        }
        val files = mutableListOf<Triple<String, Long, Long>>()
        val pending = ArrayDeque<Pair<File, String>>()
        pending.add(root to "")
        while (pending.isNotEmpty()) {
            val (dir, prefix) = pending.removeFirst()
            val children = dir.listFiles() ?: throw IOException("folder is not readable")
            for (child in children) {
                val rel = if (prefix.isEmpty()) child.name else "$prefix/${child.name}"
                if (child.isDirectory) {
                    pending.add(child to rel)
                } else if (child.isFile) {
                    files.add(Triple(rel, child.length(), child.lastModified()))
                }
            }
        }
        return listingJson(files)
    }

    override fun copyIn(path: String, dest: String) {
        resolve(path).inputStream().use { input ->
            File(dest).outputStream().use { output -> input.copyTo(output) }
        }
    }

    override fun copyOut(source: String, path: String) {
        val file = resolve(path)
        file.parentFile?.mkdirs()
        File(source).inputStream().use { input ->
            file.outputStream().use { output -> input.copyTo(output) }
        }
    }

    override fun remove(path: String) {
        resolve(path).delete()
    }

    private fun resolve(path: String): File {
        var file = root
        for (piece in parts(path)) {
            file = File(file, piece)
        }
        return file
    }
}

/**
 * A folder granted through the Storage Access Framework.
 *
 * The app holds a persisted permission on [tree]. Nothing here sees a file path.
 */
class SafTree(private val resolver: ContentResolver, private val tree: Uri) : FolderTree {
    private val rootId = DocumentsContract.getTreeDocumentId(tree)

    override fun list(): String {
        val files = mutableListOf<Triple<String, Long, Long>>()
        val pending = ArrayDeque<Pair<String, String>>()
        pending.add(rootId to "")
        while (pending.isNotEmpty()) {
            val (id, prefix) = pending.removeFirst()
            for (child in children(id)) {
                val rel = if (prefix.isEmpty()) child.name else "$prefix/${child.name}"
                if (child.directory) {
                    pending.add(child.id to rel)
                } else {
                    files.add(Triple(rel, child.size, child.modified))
                }
            }
        }
        return listingJson(files)
    }

    override fun copyIn(path: String, dest: String) {
        val id = find(parts(path)) ?: throw IOException("file is gone")
        val uri = DocumentsContract.buildDocumentUriUsingTree(tree, id)
        val stream = resolver.openInputStream(uri) ?: throw IOException("file is not readable")
        stream.use { input ->
            File(dest).outputStream().use { output -> input.copyTo(output, COPY_BUFFER) }
        }
    }

    override fun copyOut(source: String, path: String) {
        val pieces = parts(path)
        var parent = rootId
        for (piece in pieces.dropLast(1)) {
            val existing = children(parent).firstOrNull { it.name == piece }
            parent = when {
                existing == null -> create(parent, DocumentsContract.Document.MIME_TYPE_DIR, piece)
                existing.directory -> existing.id
                else -> throw IOException("a file is in the way")
            }
        }
        val name = pieces.last()
        val existing = children(parent).firstOrNull { it.name == name }
        val id = when {
            existing == null -> create(parent, "application/octet-stream", name)
            existing.directory -> throw IOException("a folder is in the way")
            else -> existing.id
        }
        val uri = DocumentsContract.buildDocumentUriUsingTree(tree, id)
        val stream = resolver.openOutputStream(uri, "wt") ?: throw IOException("file is not writable")
        stream.use { output ->
            File(source).inputStream().use { input -> input.copyTo(output, COPY_BUFFER) }
        }
    }

    override fun remove(path: String) {
        val id = find(parts(path)) ?: return
        DocumentsContract.deleteDocument(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, id))
    }

    private class Child(val id: String, val name: String, val directory: Boolean, val size: Long, val modified: Long)

    private fun children(parentId: String): List<Child> {
        val uri = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parentId)
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        val cursor = resolver.query(uri, columns, null, null, null)
            ?: throw IOException("folder is not readable")
        val found = mutableListOf<Child>()
        cursor.use {
            while (it.moveToNext()) {
                val name = it.getString(1) ?: continue
                found.add(
                    Child(
                        id = it.getString(0),
                        name = name,
                        directory = it.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR,
                        size = if (it.isNull(3)) 0 else it.getLong(3),
                        modified = if (it.isNull(4)) 0 else it.getLong(4),
                    ),
                )
            }
        }
        return found
    }

    private fun find(pieces: List<String>): String? {
        var id = rootId
        for (piece in pieces) {
            id = children(id).firstOrNull { it.name == piece }?.id ?: return null
        }
        return id
    }

    private fun create(parentId: String, mime: String, name: String): String {
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, parentId)
        val created = DocumentsContract.createDocument(resolver, parent, mime, name)
            ?: throw IOException("could not create")
        val id = DocumentsContract.getDocumentId(created)
        // A provider may rename on a clash or add an extension; that would split
        // one path into two, so refuse instead.
        val actual = children(parentId).firstOrNull { it.id == id }?.name
        if (actual != name) {
            DocumentsContract.deleteDocument(resolver, created)
            throw IOException("the folder renamed the file")
        }
        return id
    }
}

/** What the person reads after a round. */
internal fun folderSummary(reportJson: String, words: Words): String {
    val report = JSONObject(reportJson)
    if (report.optString("status") == "unavailable") {
        return words.text(R.string.folder_unavailable)
    }
    val sent = report.optInt("imported") + report.optInt("deletedHere")
    val received = report.optInt("exported") + report.optInt("deletedThere")
    val parts = mutableListOf<String>()
    if (sent > 0) {
        parts.add(words.count(R.plurals.folder_sent, sent, sent))
    }
    if (received > 0) {
        parts.add(words.count(R.plurals.folder_received, received, received))
    }
    val conflicts = report.optInt("conflicts")
    if (conflicts > 0) {
        parts.add(words.count(R.plurals.folder_conflicts, conflicts, conflicts))
    }
    if (report.optInt("deferred") > 0) {
        parts.add(words.text(R.string.folder_deferred))
    }
    if (parts.isEmpty()) {
        return words.text(R.string.folder_up_to_date)
    }
    val locale = Locale.forLanguageTag(words.text(R.string.format_locale))
    return parts.joinToString(", ").replaceFirstChar { it.titlecase(locale) } + "."
}
