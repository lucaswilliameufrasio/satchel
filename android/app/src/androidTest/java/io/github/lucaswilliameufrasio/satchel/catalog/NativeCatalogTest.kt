package io.github.lucaswilliameufrasio.satchel.catalog

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
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
        NativeCatalog.markGrant(grant.absolutePath, "kept.txt", "once")
        val written = File(work, "kept.txt")
        assertTrue(written.exists())
        assertEquals("payload-bytes", written.readText())
    }
}
