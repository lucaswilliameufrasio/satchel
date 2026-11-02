package io.github.lucaswilliameufrasio.satchel.catalog

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The activity library reads a private repository on the device. */
@RunWith(AndroidJUnit4::class)
class NativeCatalogTest {
    @Test
    fun sampleOmitsFileBytesUntilReadFile() {
        val root = File(
            InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
            "native-sample",
        )
        root.deleteRecursively()
        val json = NativeCatalog.sampleCatalog(root.absolutePath)
        assertTrue(json.contains("\"path\":\"kept.txt\""))
        assertTrue(json.contains("\"follow\":\"see\""))
        assertFalse(json.contains("payload-bytes"))
        assertFalse(File(root, "work/kept.txt").exists())
        val bytes = NativeCatalog.readFile(root.absolutePath, "kept.txt")
        assertEquals("payload-bytes", String(bytes, Charsets.UTF_8))
    }

    @Test
    fun firstOpenRowUsesDeviceLocalText() {
        val root = freshRoot("native-row")
        val session = CatalogSession(CoreCatalog(root.absolutePath))
        session.open()
        val work = File(root, "work/kept.txt")
        assertFalse(work.exists())
        val row = rowNamed(session, "kept.txt")
        val text = catalogRowText(row, ZoneId.systemDefault())
        Log.i("satchel-row", text)
        File(root, "row-proof.txt").writeText(text + "\nwork-copy-absent\n")
        assertTrue(text.contains("kept.txt"))
        assertTrue(Regex("""\d{2}/\d{2}/\d{4} \d{2}:\d{2}""").containsMatchIn(text))
        assertTrue(text.contains("Ver"))
        assertFalse(text.contains("2023-11-14T22:13:20Z"))
        assertFalse(text.contains("0123456789abcdef0123456789abcdef"))
        assertFalse(text.contains("fedcba9876543210fedcba9876543210"))
    }

    @Test
    fun downloadOnceWritesTheCopyOnceAndTheNextReadStays() {
        val root = freshRoot("native-once-row")
        val session = CatalogSession(CoreCatalog(root.absolutePath))
        session.open()
        val work = File(root, "work/kept.txt")
        assertFalse(work.exists())
        session.choose("kept.txt", Follow.Once)
        assertTrue(work.exists())
        assertEquals("payload-bytes", work.readText())
        val stamp = work.lastModified()
        val bytes = work.readBytes()
        session.open()
        val row = rowNamed(session, "kept.txt")
        val text = catalogRowText(row, ZoneId.systemDefault())
        Log.i("satchel-row", text)
        val still = work.lastModified()
        val body = work.readText()
        File(root, "mark-proof.txt").writeText(
            text + "\n" + body + "\nwritten-at " + stamp + "\nstill-at " + still + "\n",
        )
        assertEquals(Follow.Once, row.follow)
        assertTrue(text.contains("Baixar agora"))
        assertTrue(text.contains("2 aparelhos"))
        assertEquals("payload-bytes", body)
        assertTrue(work.readBytes().contentEquals(bytes))
        assertEquals(stamp, still)
        assertFalse(text.contains("2023-11-14T22:13:20Z"))
    }

    @Test
    fun markOnceWritesTheWorkCopy() {
        val root = File(
            InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
            "native-mark",
        )
        root.deleteRecursively()
        val session = CatalogSession(CoreCatalog(root.absolutePath))
        session.open()
        assertFalse(File(root, "work/kept.txt").exists())
        session.choose("kept.txt", Follow.Once)
        val work = File(root, "work/kept.txt")
        assertTrue(work.exists())
        assertEquals("payload-bytes", work.readText())
        val again = NativeCatalog.sampleCatalog(root.absolutePath)
        assertTrue(again.contains("\"follow\":\"once\""))
        assertFalse(again.contains("payload-bytes"))
    }

    @Test
    fun grantFileListsMetadataAndMarkOnceWritesTheWorkCopy() {
        val root = File(
            InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
            "native-grant",
        )
        root.deleteRecursively()
        NativeCatalog.sampleCatalog(root.absolutePath)
        val work = File(root, "phone-work")
        val grant = File(root, "grant.json")
        val body = """{"endpoint":"${root.absolutePath}/repository","bucket_credential":"phone-fixture","device_key_password":"recovery-password","device_id":"fedcba9876543210fedcba9876543210","work":"${work.absolutePath}"}"""
        grant.writeText(body)
        val json = NativeCatalog.catalogGrant(grant.absolutePath)
        assertTrue(json.contains("\"path\":\"kept.txt\""))
        assertTrue(json.contains("\"follow\":\"see\""))
        assertFalse(json.contains("payload-bytes"))
        assertFalse(File(work, "kept.txt").exists())
        NativeCatalog.markGrant(grant.absolutePath, "kept.txt", "once", "", "")
        val written = File(work, "kept.txt")
        assertTrue(written.exists())
        assertEquals("payload-bytes", written.readText())
    }

    private fun freshRoot(name: String): File {
        val root = File(
            InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
            name,
        )
        root.deleteRecursively()
        return root
    }

    private fun rowNamed(session: CatalogSession, path: String): CatalogRow {
        for (row in session.rows) {
            if (row.path == path) {
                return row
            }
        }
        error("missing $path")
    }
}
