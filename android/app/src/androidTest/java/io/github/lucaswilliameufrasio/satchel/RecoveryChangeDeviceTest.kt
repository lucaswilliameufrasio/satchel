package io.github.lucaswilliameufrasio.satchel

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The phone that created the repository changes the recovery password through the Keystore. */
@RunWith(AndroidJUnit4::class)
class RecoveryChangeDeviceTest {
    @Test
    fun phoneChangesTheRecoveryPasswordAndTheOldOneStopsOpening() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "recovery-change")
        root.deleteRecursively()
        root.mkdirs()
        val created = createRepositoryFile(root, CreateForm())
        val oldPassword = created.password
        assertNotNull(oldPassword)
        val vault = SecureGrant(root)
        val pending = SecureGrant(root, PENDING_GRANT)
        val oldGrant = vault.read()!!
        assertTrue(NativeCatalog.grantOpens(oldGrant))

        val fresh = changeRecovery(
            vault,
            pending,
            prepare = { NativeCatalog.recoveryPrepare(it) },
            commit = { old, new -> NativeCatalog.recoveryCommit(old, new) },
        )
        assertNotNull(fresh)
        assertEquals(64, fresh!!.length)
        assertNotEquals(oldPassword, fresh)
        val newGrant = vault.read()!!
        assertTrue(newGrant.contains(fresh))
        assertTrue(NativeCatalog.grantOpens(newGrant))
        assertFalse(NativeCatalog.grantOpens(oldGrant))
        assertFalse(File(root, PENDING_GRANT).exists())
        assertFalse(String(File(root, "grant.enc").readBytes(), Charsets.ISO_8859_1).contains(fresh))
        assertTrue(NativeCatalog.isMaintainer(newGrant))
        assertEquals("[]", NativeCatalog.catalogGrant(newGrant))
        // Nothing pending, so launch has nothing to settle.
        assertFalse(settleRecovery(vault, pending) { NativeCatalog.grantOpens(it) })
    }
}
