package io.github.lucaswilliameufrasio.satchel

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lucaswilliameufrasio.satchel.catalog.FileTree
import io.github.lucaswilliameufrasio.satchel.catalog.FolderTree
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File
import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The folder round through JNI: the Rust core calls back into a Kotlin folder. */
@RunWith(AndroidJUnit4::class)
class PhoneFolderDeviceTest {
    @Test
    fun aPickedFolderGoesUpAndASecondFolderReceivesIt() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "phone-folder")
        root.deleteRecursively()
        root.mkdirs()
        createRepositoryFile(root, CreateForm())
        val grant = SecureGrant(root).read()!!
        val first = File(root, "picked")
        File(first, "trip").mkdirs()
        File(first, "a.txt").writeText("alpha")
        File(first, "trip/day1.txt").writeText("day one")
        val manifest = File(root, "folder.manifest").absolutePath
        val pt = wordsIn("pt-BR")
        val en = wordsIn("en")
        val unavailable = pt.text(R.string.folder_unavailable)
        assertEquals(
            "A pasta não respondeu ou ficou vazia de repente. Nada foi apagado. Confira a pasta ou escolha de novo.",
            unavailable,
        )

        assertEquals("2 mudanças enviadas.", folderRoundOn(grant, FileTree(first), manifest, "Pixel", pt))
        val listed = NativeCatalog.catalogGrant(grant)
        assertTrue(listed.contains("\"a.txt\"") && listed.contains("\"trip/day1.txt\""))
        assertEquals("Em dia.", folderRoundOn(grant, FileTree(first), manifest, "Pixel", pt))
        assertEquals("Up to date.", folderRoundOn(grant, FileTree(first), manifest, "Pixel", en))

        // A folder that suddenly lists nothing deletes nothing.
        File(first, "a.txt").delete()
        File(first, "trip/day1.txt").delete()
        assertEquals(unavailable, folderRoundOn(grant, FileTree(first), manifest, "Pixel", pt))
        assertEquals(
            "The folder did not answer or was suddenly empty. Nothing was deleted. Check the folder or choose it again.",
            folderRoundOn(grant, FileTree(first), manifest, "Pixel", en),
        )
        assertTrue(NativeCatalog.catalogGrant(grant).contains("\"a.txt\""))

        // A Kotlin exception in the folder is a refusal, not a crash.
        val broken = object : FolderTree {
            override fun list(): String = throw IOException("permission revoked")
            override fun read(path: String): ByteArray = throw IOException("no")
            override fun write(path: String, bytes: ByteArray) = throw IOException("no")
            override fun remove(path: String) = throw IOException("no")
        }
        assertEquals(unavailable, folderRoundOn(grant, broken, manifest, "Pixel", pt))

        // Picking another folder: a fresh manifest, and the synced files arrive.
        val second = File(root, "other")
        second.mkdirs()
        File(manifest).delete()
        File(second, "new.txt").writeText("from the second folder")
        assertEquals(
            "1 mudança enviada, 2 mudanças recebidas.",
            folderRoundOn(grant, FileTree(second), manifest, "Pixel", pt),
        )
        assertEquals("alpha", File(second, "a.txt").readText())
        assertEquals("day one", File(second, "trip/day1.txt").readText())
        assertTrue(NativeCatalog.catalogGrant(grant).contains("\"new.txt\""))
        assertFalse(File(first, "a.txt").exists())
    }
}
