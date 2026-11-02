package io.github.lucaswilliameufrasio.satchel

import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_FIELDS
import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_NOTE
import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_TITLE
import io.github.lucaswilliameufrasio.satchel.catalog.Follow
import io.github.lucaswilliameufrasio.satchel.catalog.SAMPLE_READER_ID
import io.github.lucaswilliameufrasio.satchel.catalog.labelStoredFromEntry
import io.github.lucaswilliameufrasio.satchel.catalog.localDeviceId
import io.github.lucaswilliameufrasio.satchel.catalog.markRequest
import io.github.lucaswilliameufrasio.satchel.catalog.openingApelido
import io.github.lucaswilliameufrasio.satchel.catalog.pairingAfterQr
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.fail
import org.junit.Test

class InviteFieldsTest {
    private val fingerprint = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

    @Test
    fun inviteFieldsReadsTheQrPayload() {
        val text = """{"v":1,"code":"482193","address":"127.0.0.1:9","fingerprint":"$fingerprint"}"""
        val fields = inviteFields(text)
        assertEquals("127.0.0.1:9", fields.address)
        assertEquals("482193", fields.code)
        assertEquals(fingerprint, fields.fingerprint)
    }

    @Test
    fun entryFieldsStayInOrderAndTheQrDoesNotSubmit() {
        assertEquals("Entrar neste aparelho", ENTRY_TITLE)
        assertEquals(listOf("Endereço", "Código", "Impressão digital", "Apelido"), ENTRY_FIELDS)
        assertEquals(
            "O desktop mostra o QR. A senha de recuperação não faz parte do convite.",
            ENTRY_NOTE,
        )
        val filled = pairingAfterQr("Casa", "127.0.0.1:9", "482193", fingerprint)
        assertFalse(filled.submit)
        assertEquals("Casa", filled.apelido)
        assertEquals("127.0.0.1:9", filled.address)
        assertEquals("482193", filled.code)
        assertEquals(fingerprint, filled.fingerprint)
        assertEquals("iPhone", openingApelido(""))
        assertEquals("Casa", openingApelido("  Casa  "))
    }

    @Test
    fun entrarStoresTheTypedApelidoAndAnEmptyOneStillMarks() {
        val dir = File(System.getProperty("java.io.tmpdir"), "satchel-test-label-${System.nanoTime()}")
        dir.mkdirs()
        try {
            val stored = labelStoredFromEntry("  Mesa  ")
            assertEquals("Mesa", stored)
            writeDeviceLabel(dir, stored!!)
            assertEquals("Mesa", readDeviceLabel(dir))
            val request = markRequest("kept.txt", Follow.Once, readDeviceLabel(dir))
            assertEquals("Mesa", request.name)
            assertEquals("Android", request.os)
            assertEquals("once", request.follow)
            writeDeviceLabel(dir, labelStoredFromEntry("")!!)
            val empty = markRequest("kept.txt", Follow.Once, readDeviceLabel(dir))
            assertEquals("", empty.name)
            assertEquals("", empty.os)
            assertEquals("once", empty.follow)
            assertNull(readStoredDeviceId(dir))
            assertFalse(File(dir, "device-id").exists())
            assertEquals("", localDeviceId(true, readStoredDeviceId(dir)))
            assertEquals(SAMPLE_READER_ID, localDeviceId(false, null))
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun inviteFieldsRejectsARecoveryField() {
        val text = """{"v":1,"code":"482193","address":"127.0.0.1:9","fingerprint":"$fingerprint","recovery":"secret-value"}"""
        try {
            inviteFields(text)
            fail("expected rejection")
        } catch (err: IllegalArgumentException) {
            assertEquals("catalog failed", err.message)
            val message = err.message ?: ""
            assertFalse(message.contains("secret-value"))
        }
    }
}
