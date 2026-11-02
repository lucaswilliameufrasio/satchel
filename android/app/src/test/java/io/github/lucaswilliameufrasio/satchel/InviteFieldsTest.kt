package io.github.lucaswilliameufrasio.satchel

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
