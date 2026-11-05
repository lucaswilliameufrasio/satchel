package io.github.lucaswilliameufrasio.satchel

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import io.github.lucaswilliameufrasio.satchel.catalog.FolderTree
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.PhoneFolder
import io.github.lucaswilliameufrasio.satchel.catalog.SafTree
import io.github.lucaswilliameufrasio.satchel.catalog.folderSummary
import java.io.File

/**
 * The folder the person picked, remembered as its tree URI.
 *
 * The URI and the manifest stay in the app's private files, which are not backed
 * up. Picking another folder drops the manifest: the old folder's files are not
 * deletions in the new one.
 */
internal class FolderChoice(filesDir: File) {
    private val uriFile = File(filesDir, "folder-uri")
    val manifest = File(filesDir, "folder.manifest")

    fun read(): Uri? {
        if (!uriFile.isFile) {
            return null
        }
        val text = uriFile.readText().trim()
        if (text.isEmpty()) {
            return null
        }
        return Uri.parse(text)
    }

    fun choose(resolver: ContentResolver, uri: Uri) {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        resolver.takePersistableUriPermission(uri, flags)
        val previous = read()
        manifest.delete()
        uriFile.writeText(uri.toString())
        if (previous != null && previous != uri) {
            runCatching { resolver.releasePersistableUriPermission(previous, flags) }
        }
    }

    fun clear(resolver: ContentResolver) {
        val previous = read()
        manifest.delete()
        uriFile.delete()
        if (previous != null) {
            val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            runCatching { resolver.releasePersistableUriPermission(previous, flags) }
        }
    }

    /** The folder, or null when none is picked or Android no longer lets the app open it. */
    fun open(resolver: ContentResolver): PhoneFolder? {
        val uri = read() ?: return null
        val held = resolver.persistedUriPermissions.any {
            it.uri == uri && it.isReadPermission && it.isWritePermission
        }
        if (!held) {
            return null
        }
        return PhoneFolder(SafTree(resolver, uri), manifest.absolutePath)
    }
}

/** Name the folder shows in the system picker, or the last part of its id, or [fallback]. */
internal fun folderDisplayName(resolver: ContentResolver, uri: Uri, fallback: String): String {
    val id = runCatching { DocumentsContract.getTreeDocumentId(uri) }.getOrNull().orEmpty()
    val document = DocumentsContract.buildDocumentUriUsingTree(uri, id)
    val named = runCatching {
        resolver.query(
            document,
            arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            null,
            null,
            null,
        )?.use { if (it.moveToFirst()) it.getString(0) else null }
    }.getOrNull()
    if (!named.isNullOrBlank()) {
        return named
    }
    return id.substringAfterLast(':').substringAfterLast('/').ifEmpty { fallback }
}

/** Run one round and say what happened. Never throws. */
internal fun folderRound(grant: String, folder: PhoneFolder?, apelido: String, words: Words): String {
    if (folder == null) {
        return words.text(R.string.folder_lost)
    }
    return folderRoundOn(grant, folder.tree, folder.manifest, apelido, words)
}

internal fun folderRoundOn(
    grant: String,
    tree: FolderTree,
    manifest: String,
    apelido: String,
    words: Words,
): String {
    return try {
        val label = io.github.lucaswilliameufrasio.satchel.catalog.markRequest(
            "",
            io.github.lucaswilliameufrasio.satchel.catalog.Follow.See,
            apelido,
        )
        folderSummary(NativeCatalog.syncFolder(grant, tree, manifest, "", "", label.name, label.os), words)
    } catch (_: RuntimeException) {
        words.text(R.string.folder_sync_failed)
    }
}
