package io.github.lucaswilliameufrasio.satchel

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.fail
import org.junit.Test

class GrantFileTest {
    @Test
    fun assembleGrantKeepsTheThreeFieldsAndAddsTheDevice() {
        val joined = """{"bucket_credential":"phone-fixture","device_key_password":"device-key-password","endpoint":"http://127.0.0.1:9"}"""
        val text = assembleGrant(joined, "fedcba9876543210fedcba9876543210", "/data/work")
        assertEquals(
            """{"endpoint":"http://127.0.0.1:9","bucket_credential":"phone-fixture","device_key_password":"device-key-password","device_id":"fedcba9876543210fedcba9876543210","work":"/data/work"}""",
            text,
        )
        assertFalse(text.contains("recovery"))
    }

    @Test
    fun assembleGrantKeepsACredentialThatIsItselfJson() {
        val joined = """{"endpoint":"http://127.0.0.1:9","bucket_credential":"{\"access_key_id\":\"phone-key\",\"secret_access_key\":\"phone-secret\"}","device_key_password":"device-key-password"}"""
        val text = assembleGrant(joined, "fedcba9876543210fedcba9876543210", "/data/work")
        assertEquals(
            """{"endpoint":"http://127.0.0.1:9","bucket_credential":"{\"access_key_id\":\"phone-key\",\"secret_access_key\":\"phone-secret\"}","device_key_password":"device-key-password","device_id":"fedcba9876543210fedcba9876543210","work":"/data/work"}""",
            text,
        )
    }

    @Test
    fun assembleGrantRejectsARecoveryField() {
        val joined = """{"endpoint":"http://127.0.0.1:9","bucket_credential":"phone-fixture","device_key_password":"device-key-password","recovery":"secret-value"}"""
        try {
            assembleGrant(joined, "fedcba9876543210fedcba9876543210", "/data/work")
            fail("expected rejection")
        } catch (err: IllegalArgumentException) {
            assertEquals("catalog failed", err.message)
            val message = err.message ?: ""
            assertFalse(message.contains("secret-value"))
        }
    }
}
