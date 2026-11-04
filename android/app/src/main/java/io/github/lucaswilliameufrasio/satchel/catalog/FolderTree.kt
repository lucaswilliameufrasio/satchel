package io.github.lucaswilliameufrasio.satchel.catalog

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File
import java.io.IOException
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

    fun read(path: String): ByteArray

    /** Create or replace the file, creating parent folders. */
    fun write(path: String, bytes: ByteArray)

    /** Delete the file. A missing file is not an error. */
    fun remove(path: String)
}

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

    override fun read(path: String): ByteArray {
        return resolve(path).readBytes()
    }

    override fun write(path: String, bytes: ByteArray) {
        val file = resolve(path)
        file.parentFile?.mkdirs()
        file.writeBytes(bytes)
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

    override fun read(path: String): ByteArray {
        val id = find(parts(path)) ?: throw IOException("file is gone")
        val uri = DocumentsContract.buildDocumentUriUsingTree(tree, id)
        val stream = resolver.openInputStream(uri) ?: throw IOException("file is not readable")
        return stream.use { it.readBytes() }
    }

    override fun write(path: String, bytes: ByteArray) {
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
        stream.use { it.write(bytes) }
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
internal fun folderSummary(reportJson: String): String {
    val report = JSONObject(reportJson)
    if (report.optString("status") == "unavailable") {
        return FOLDER_UNAVAILABLE
    }
    val sent = report.optInt("imported") + report.optInt("deletedHere")
    val received = report.optInt("exported") + report.optInt("deletedThere")
    val parts = mutableListOf<String>()
    if (sent > 0) {
        parts.add(if (sent == 1) "1 mudança enviada" else "$sent mudanças enviadas")
    }
    if (received > 0) {
        parts.add(if (received == 1) "1 mudança recebida" else "$received mudanças recebidas")
    }
    val conflicts = report.optInt("conflicts")
    if (conflicts > 0) {
        parts.add(if (conflicts == 1) "1 conflito guardado no catálogo" else "$conflicts conflitos guardados no catálogo")
    }
    if (report.optInt("deferred") > 0) {
        parts.add("arquivos editados durante a sincronização vão na próxima")
    }
    if (parts.isEmpty()) {
        return "Em dia."
    }
    return parts.joinToString(", ").replaceFirstChar { it.uppercase() } + "."
}

internal const val FOLDER_TITLE = "Pasta do celular"

internal const val FOLDER_CHOOSE_NOTE =
    "Escolha uma pasta. O que você puser nela vai para o repositório. " +
        "O que vem dos outros aparelhos aparece no catálogo e entra na pasta quando você marca para baixar."

internal const val FOLDER_UNAVAILABLE =
    "A pasta não respondeu ou ficou vazia de repente. Nada foi apagado. Confira a pasta ou escolha de novo."

internal const val FOLDER_LOST =
    "O Android não deixa mais o app abrir esta pasta. Escolha de novo."
