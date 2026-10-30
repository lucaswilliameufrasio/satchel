package io.github.lucaswilliameufrasio.satchel.catalog

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogSessionTest {
    @Test
    fun openShowsMetadataAndDoesNotReadContent() {
        val source = RecordingCatalog()
        val session = CatalogSession(source)
        session.open()
        assertEquals(0, source.reads)
        assertEquals(listOf("notes/a.txt", "photos"), session.rows.map { row -> row.path })
        val file = session.rows[0]
        assertEquals("notes/a.txt", rowTitle(file))
        assertTrue(rowDetails(file).contains("2023-11-14T22:13:20Z"))
        assertTrue(rowDetails(file).contains("0123456789abcdef0123456789abcdef"))
        assertFalse(rowDetails(file).contains("payload-bytes"))
        assertEquals("Ver", followLabel(file.follow))
    }

    @Test
    fun seeDoesNotReadContent() {
        val source = RecordingCatalog()
        val session = CatalogSession(source)
        session.open()
        session.choose("notes/a.txt", Follow.See)
        assertEquals(0, source.reads)
        assertEquals(Follow.See, source.remembered["notes/a.txt"])
    }

    @Test
    fun downloadOnceReadsTheFileOnce() {
        val source = RecordingCatalog()
        val session = CatalogSession(source)
        session.open()
        session.choose("notes/a.txt", Follow.Once)
        session.choose("notes/a.txt", Follow.Once)
        assertEquals(1, source.reads)
        assertEquals(listOf("notes/a.txt"), source.readPaths)
        assertEquals(Follow.Once, session.rows[0].follow)
    }

    @Test
    fun alwaysOnAFolderReadsTheNewChildAndNotUnmarkedFiles() {
        val source = RecordingCatalog()
        val session = CatalogSession(source)
        session.open()
        session.choose("photos", Follow.Always)
        assertEquals(listOf("photos/new.txt"), source.readPaths)
        assertEquals(Follow.Always, followOf(session, "photos"))
        assertEquals(Follow.Always, followOf(session, "photos/new.txt"))
        assertEquals(Follow.See, followOf(session, "notes/a.txt"))
        assertEquals(1, session.contentReads)
    }
}

private fun followOf(session: CatalogSession, path: String): Follow {
    for (row in session.rows) {
        if (row.path == path) {
            return row.follow
        }
    }
    error("missing $path")
}

private class RecordingCatalog : CatalogSource {
    val reads: Int
        get() = readPaths.size

    val readPaths = mutableListOf<String>()
    val remembered = linkedMapOf<String, Follow>()
    private val present = mutableSetOf<String>()

    override fun listMetadata(): List<CatalogRow> {
        val rows = mutableListOf(
            row("notes/a.txt", false),
            row("photos", true),
        )
        if (remembered["photos"] == Follow.Always) {
            rows.add(row("photos/new.txt", false))
        }
        return rows
    }

    override fun hasContent(path: String): Boolean {
        return present.contains(path)
    }

    override fun readContent(path: String): ByteArray {
        readPaths.add(path)
        present.add(path)
        return "payload-bytes".toByteArray()
    }

    override fun remember(path: String, follow: Follow) {
        remembered[path] = follow
    }

    private fun row(path: String, directory: Boolean): CatalogRow {
        val follow = remembered[path] ?: inherited(path)
        return CatalogRow(
            path = path,
            isDirectory = directory,
            size = if (directory) 0 else 4,
            versionUnix = 1_700_000_000,
            devices = listOf("0123456789abcdef0123456789abcdef"),
            follow = follow,
        )
    }

    private fun inherited(path: String): Follow {
        if (path.startsWith("photos/") && remembered["photos"] == Follow.Always) {
            return Follow.Always
        }
        return Follow.See
    }
}
