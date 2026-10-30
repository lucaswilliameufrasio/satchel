package io.github.lucaswilliameufrasio.satchel.catalog

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Reads the shared Floci repository. Pass `storeSpec` as an instrumentation argument. */
@RunWith(AndroidJUnit4::class)
class SharedStoreTest {
    @Test
    fun sharedCatalogListsDesktopFileWithoutWritingTheWorkCopy() {
        val specArg = InstrumentationRegistry.getArguments().getString("storeSpec")
        if (specArg.isNullOrEmpty()) {
            throw AssertionError("storeSpec is required")
        }
        val spec = specArg
        val json = NativeCatalog.catalogStore(spec)
        assertTrue(json.contains("\"path\":\"notes/from-desktop.txt\""))
        assertTrue(json.contains("\"follow\":\"see\""))
        assertFalse(json.contains("desk-bytes"))
        val work = File(
            "/data/user/0/io.github.lucaswilliameufrasio.satchel/cache/shared-work/notes/from-desktop.txt",
        )
        assertFalse(work.exists())
        val bytes = NativeCatalog.readStore(spec, "notes/from-desktop.txt")
        assertEquals("desk-bytes", String(bytes, Charsets.UTF_8))
        assertFalse(work.exists())
    }
}
