package io.github.lucaswilliameufrasio.satchel

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.nio.ByteBuffer
import java.security.SecureRandom
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The phone listens through JNI, and a notice built here with the JVM's own HMAC
 * is accepted: the wire format is the same on both sides.
 */
@RunWith(AndroidJUnit4::class)
class PhoneWakeDeviceTest {
    @After
    fun stop() {
        NativeCatalog.wakeStop()
    }

    @Test
    fun aSignedNoticeWakesTheListenerAndAWrongKeyDoesNot() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = wakeStateDir(File(context.cacheDir, "phone-wake"))
        dir.deleteRecursively()
        dir.mkdirs()
        assertFalse("not started", NativeCatalog.wakeWait(50))
        val address = NativeCatalog.wakeStart(dir.absolutePath, "127.0.0.1")
        assertTrue(address, address.startsWith("127.0.0.1:"))
        val port = address.substringAfterLast(':').toInt()
        val key = hex(JSONObject(File(dir, "wake.json").readText()).getString("key"))

        send(port, notice(ByteArray(32) { 1 }))
        assertFalse("a wrong key is dropped", NativeCatalog.wakeWait(500))
        send(port, notice(key))
        assertTrue(NativeCatalog.wakeWait(2_000))

        // The same port comes back after a restart.
        NativeCatalog.wakeStop()
        assertFalse(NativeCatalog.wakeWait(50))
        assertEquals(address, NativeCatalog.wakeStart(dir.absolutePath, "127.0.0.1"))
    }

    private fun notice(key: ByteArray): ByteArray {
        val nonce = ByteArray(16).also { SecureRandom().nextBytes(it) }
        val signed = ByteBuffer.allocate(64)
            .put("SATWAKE1".toByteArray())
            .putLong(System.currentTimeMillis() / 1000)
            .put(nonce)
            .put("0123456789abcdef0123456789abcdef".toByteArray())
            .array()
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key, "HmacSHA256"))
        return signed + mac.doFinal(signed)
    }

    private fun send(port: Int, bytes: ByteArray) {
        DatagramSocket().use {
            it.send(DatagramPacket(bytes, bytes.size, InetAddress.getByName("127.0.0.1"), port))
        }
    }

    private fun hex(text: String): ByteArray =
        ByteArray(text.length / 2) { text.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
