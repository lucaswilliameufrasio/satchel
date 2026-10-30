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
}
