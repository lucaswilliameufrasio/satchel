package io.github.lucaswilliameufrasio.satchel.catalog

import java.time.ZoneId
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogSessionTest {
    private val publisher = "0123456789abcdef0123456789abcdef"
    private val utc = ZoneOffset.UTC
    private val minus3 = ZoneOffset.ofHours(-3)

    @Test
    fun followWireMatchesTheSnapshotNames() {
        assertEquals("see", followWire(Follow.See))
        assertEquals("once", followWire(Follow.Once))
        assertEquals("always", followWire(Follow.Always))
    }

    @Test
    fun openShowsMetadataAndDoesNotReadContent() {
        val source = RecordingCatalog()
        val session = CatalogSession(source)
        session.open()
        assertEquals(0, source.reads)
        assertEquals(listOf("notes/a.txt", "photos"), session.rows.map { row -> row.path })
        val file = session.rows[0]
        val text = catalogRowText(file, utc)
        assertEquals("notes/a.txt", rowTitle(file))
        assertEquals("Ver", followLabel(file.follow))
        assertEquals("14/11/2023 22:13 UTC", rowWhen(file, utc))
        assertEquals("1 aparelho", deviceCountLabel(file.devices))
        assertTrue(text.contains("notes/a.txt"))
        assertTrue(text.contains("14/11/2023 22:13 UTC"))
        assertFalse(text.contains("2023-11-14T22:13:20Z"))
        assertFalse(text.contains(publisher))
        assertFalse(text.contains("payload-bytes"))
    }

    @Test
    fun utcMinus3ShowsNineteenThirteenAndUtcKeepsTheOldClock() {
        val minus = syncedWhen(1_700_000_000, minus3)
        assertEquals("14/11/2023 19:13 ${shortZoneName(minus3)}", minus)
        assertTrue(minus.startsWith("14/11/2023 19:13 "))
        assertFalse(minus.contains("22:13"))
        assertFalse(minus.endsWith("UTC"))
        assertEquals("14/11/2023 22:13 UTC", syncedWhen(1_700_000_000, utc))
        assertEquals("14/11/2023 22:13 UTC", syncedWhen(1_700_000_000, ZoneId.of("UTC")))
    }

    @Test
    fun listRowHidesTheOpaqueIdAndAFolderSaysPasta() {
        val file = keptFile(Follow.See, listOf(publisher))
        val folder = CatalogRow(
            path = "photos",
            isDirectory = true,
            size = 0,
            versionUnix = 1_700_000_000,
            devices = emptyList(),
            follow = Follow.See,
        )
        val fileText = catalogRowText(file, minus3)
        val folderText = catalogRowText(folder, minus3)
        assertTrue(fileText.contains("13 B · 14/11/2023 19:13"))
        assertTrue(folderText.contains("Pasta · 14/11/2023 19:13"))
        assertFalse(folderText.contains("0 B"))
        assertFalse(fileText.contains(publisher))
        assertFalse(fileText.contains("2023-11-14T22:13:20Z"))
        assertEquals("Nenhum aparelho", deviceCountLabel(emptyList()))
        assertEquals("1 aparelho", deviceCountLabel(listOf(publisher)))
        assertEquals("2 aparelhos", deviceCountLabel(listOf(publisher, SAMPLE_READER_ID)))
        assertEquals(EMPTY_CATALOG, "Nenhum arquivo no catálogo.")
        assertEquals(LOADING_COPY, "Lendo o catálogo…")
        assertEquals(OPEN_FAILURE_COPY, "Não foi possível abrir o catálogo.")
        assertEquals("13 B", byteCount(13))
        assertEquals("1.5 KiB", byteCount(1536))
        assertEquals("2 KiB", byteCount(2048))
        assertEquals("0 B", byteCount(-1))
    }

    @Test
    fun publisherCardAndThisDeviceAfterDownload() {
        val holders = parseHolders(
            """[{"id":"$publisher","name":"Publicador","os":"exemplo","enrolledUnix":1700000000,"lastUnix":1700000000},{"id":"$SAMPLE_READER_ID","lastUnix":1700000000}]""",
        )
        val row = keptFile(Follow.Once, listOf(publisher, SAMPLE_READER_ID)).copy(holders = holders)
        val lines = deviceSectionLines(row, SAMPLE_READER_ID, minus3)
        val text = lines.joinToString("\n")
        assertTrue(text.contains("Publicador"))
        assertTrue(text.contains("Sistema exemplo"))
        assertTrue(text.contains("Registrado 14/11/2023 19:13 ${shortZoneName(minus3)}"))
        assertTrue(text.contains("Último envio 14/11/2023 19:13 ${shortZoneName(minus3)}"))
        assertFalse(text.contains("Apelido"))
        assertTrue(text.contains("Este aparelho"))
        val reader = holders[1]
        assertNull(reader.name)
        assertNull(reader.os)
        assertNull(reader.enrolledUnix)
        assertEquals(1_700_000_000L, reader.lastUnix)
        val readerLines = deviceCardLines(SAMPLE_READER_ID, reader, true, minus3)
        assertEquals("Este aparelho", readerLines[0])
        assertEquals(1, readerLines.count { line -> line.startsWith("Último envio ") })
        assertFalse(readerLines.any { line -> line.startsWith("Apelido") })
        assertFalse(readerLines.any { line -> line.startsWith("Sistema") })
        assertFalse(readerLines.any { line -> line.startsWith("Registrado") })
        assertTrue(readerLines.any { line -> line.startsWith("Último envio ") })
        assertEquals(groupedDeviceId(SAMPLE_READER_ID), readerLines.last())
        assertFalse(catalogRowText(row, minus3).contains(publisher))
        assertFalse(catalogRowText(row, minus3).contains(SAMPLE_READER_ID))
        val bare = parseHolders("""[{"id":"$publisher","lastUnix":1700000000,"extra":"ignored"}]""")
        assertNull(bare[0].name)
        val bareLines = deviceCardLines(publisher, bare[0], false, utc)
        assertEquals(
            listOf(groupedDeviceId(publisher), "Último envio 14/11/2023 22:13 UTC"),
            bareLines,
        )
        val header = fileHeaderLines(row, minus3)
        assertEquals("Tamanho", header[0])
        assertEquals("13 B", header[1])
        assertEquals("Sincronizado", header[2])
        assertTrue(header[3].startsWith("14/11/2023 19:13 "))
        val choices = fileChoiceLines(row, false)
        assertEquals("Ver", choices[0])
        assertEquals("O conteúdo fica no storage. Este aparelho não baixa.", choices[1])
        assertEquals("✓ Baixar agora", choices[2])
        assertEquals("Traz o arquivo uma vez e não acompanha mudanças posteriores.", choices[3])
        assertEquals("Sincronizar sempre", choices[4])
        assertEquals("Baixa, envia e acompanha.", choices[5])
        val folder = CatalogRow(
            path = "photos",
            isDirectory = true,
            size = 0,
            versionUnix = 1_700_000_000,
            devices = emptyList(),
            follow = Follow.Always,
        )
        val folderChoices = fileChoiceLines(folder, true)
        assertEquals(
            "Baixa, envia e acompanha. Arquivo novo dentro desta pasta entra nesse estado.",
            folderChoices[5],
        )
        assertEquals(SAVING_COPY, folderChoices.last())
        assertEquals("Pasta", fileHeaderLines(folder, utc)[1])
        assertEquals(listOf("Nenhum aparelho mantém cópia."), deviceSectionLines(folder, "", utc))
    }

    @Test
    fun emptyApelidoStillMarksWithoutAName() {
        assertEquals("", labelStoredFromEntry("   "))
        val stored = labelStoredFromEntry("")
        assertEquals("", stored)
        val request = markRequest("kept.txt", Follow.Once, stored!!)
        assertEquals("kept.txt", request.path)
        assertEquals("once", request.follow)
        assertEquals("", request.name)
        assertEquals("", request.os)
        val holder = parseHolders("""[{"id":"$SAMPLE_READER_ID","lastUnix":1700000000}]""")[0]
        assertEquals(SAMPLE_READER_ID, holder.id)
        assertNull(holder.name)
        assertEquals(1_700_000_000L, holder.lastUnix)
        val lines = deviceCardLines(SAMPLE_READER_ID, holder, true, utc)
        assertFalse(lines.any { line -> line.startsWith("Apelido") })
        assertTrue(lines.any { line -> line.startsWith("Último envio ") })
    }

    @Test
    fun typedApelidoIsWhatTheMarkSends() {
        assertEquals("Casa", labelStoredFromEntry("  Casa  "))
        val request = markRequest("kept.txt", Follow.Once, labelStoredFromEntry("Casa")!!)
        assertEquals("Casa", request.name)
        assertEquals("Android", request.os)
        assertEquals("once", request.follow)
        assertNull(labelStoredFromEntry("linha\nquebrada"))
        assertNull(labelStoredFromEntry("a".repeat(65)))
        assertEquals("", localDeviceId(true, null))
        assertEquals(SAMPLE_READER_ID, localDeviceId(false, null))
        assertEquals(SAMPLE_READER_ID, localDeviceId(true, SAMPLE_READER_ID))
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

    private fun keptFile(follow: Follow, devices: List<String>): CatalogRow {
        return CatalogRow(
            path = "kept.txt",
            isDirectory = false,
            size = 13,
            versionUnix = 1_700_000_000,
            devices = devices,
            follow = follow,
        )
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
            size = if (directory) 0 else 13,
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
