package io.github.lucaswilliameufrasio.satchel

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File
import kotlin.concurrent.thread
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The phone creates a folder repository, shows the invite, and another device joins. */
@RunWith(AndroidJUnit4::class)
class CreateAndInviteTest {
    @Test
    fun phoneCreatesInvitesAndApprovesWithoutSendingTheRecoveryPassword() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "create-invite")
        root.deleteRecursively()
        root.mkdirs()

        val result = createRepositoryFile(root, CreateForm())
        val password = result.password
        assertNotNull(password)
        assertTrue(result.stored)
        assertEquals(64, password!!.length)
        // The grant is sealed in the Keystore: no plaintext file, and the password is not in the sealed one.
        assertFalse(File(root, "grant.json").exists())
        val sealed = File(root, "grant.enc").readBytes()
        assertFalse(String(sealed, Charsets.ISO_8859_1).contains(password))
        val vault = SecureGrant(root)
        val grant = vault.read()!!
        assertTrue(grant.contains(password))
        assertTrue(NativeCatalog.isMaintainer(grant))
        assertEquals("[]", NativeCatalog.catalogGrant(grant))
        assertEquals(null, createRepositoryFile(root, CreateForm()).password)
        assertEquals("exists", NativeCatalog.createFailure())

        val offer = JsonCursor(NativeCatalog.holdStart(grant, "127.0.0.1:0"))
            .parseMixedObject()
        assertFalse(offer.values.any { it.contains(password) })
        assertTrue(NativeCatalog.invitePng(offer.getValue("invite")).size > 8)

        val otherId = NativeCatalog.newDeviceId()
        var joined: String? = null
        val joiner = thread {
            joined = NativeCatalog.joinPairing(
                offer.getValue("address"),
                offer.getValue("code"),
                offer.getValue("fingerprint"),
                "Tablet",
                "Android",
                otherId,
            )
        }
        var shown = ""
        val deadline = System.currentTimeMillis() + 60_000
        while (shown.isEmpty() && System.currentTimeMillis() < deadline) {
            shown = NativeCatalog.holdPoll()
            Thread.sleep(50)
        }
        assertEquals("Tablet", approvalFrom(shown)?.deviceName)
        NativeCatalog.holdDecide(grant, true)
        joiner.join(60_000)
        val text = joined
        assertNotNull(text)
        assertFalse(text!!.contains(password))

        val member = File(root, "member-grant.json")
        member.writeText(assembleGrant(text, otherId, File(root, "member-work").absolutePath))
        assertEquals("[]", NativeCatalog.catalogGrant(member.absolutePath))
        assertFalse(NativeCatalog.isMaintainer(member.absolutePath))
        assertEquals(2, File(root, "repository/keys").list()!!.size)
    }
}
