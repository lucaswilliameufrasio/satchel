package io.github.lucaswilliameufrasio.satchel

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The grant holds the device key password and, on the device that created the
 * repository, the recovery password. It is kept encrypted with a key that never
 * leaves the Android Keystore, and the native library receives the JSON as text,
 * so no plaintext copy is written to disk.
 */
internal class SecureGrant(
    private val filesDir: File,
    private val name: String = "grant.enc",
) : GrantSlot {
    private val file = File(filesDir, name)

    fun exists(): Boolean = file.isFile

    override fun read(): String? {
        if (!file.isFile) {
            return null
        }
        return try {
            val bytes = file.readBytes()
            if (bytes.size <= IV_BYTES) {
                return null
            }
            val cipher = Cipher.getInstance(TRANSFORM)
            cipher.init(
                Cipher.DECRYPT_MODE,
                key(),
                GCMParameterSpec(TAG_BITS, bytes.copyOfRange(0, IV_BYTES)),
            )
            String(cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES), Charsets.UTF_8)
        } catch (_: Exception) {
            null
        }
    }

    override fun write(text: String) {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.doFinal(text.toByteArray(Charsets.UTF_8))
        val temporary = File(filesDir, "$name.tmp")
        temporary.writeBytes(cipher.iv + sealed)
        if (!temporary.renameTo(file)) {
            temporary.delete()
            throw IllegalStateException("grant not stored")
        }
    }

    override fun clear() {
        file.delete()
    }

    /** Older installs wrote `grant.json` in clear. Seal it and remove the plaintext. */
    fun migrateLegacy() {
        val legacy = File(filesDir, "grant.json")
        if (!legacy.isFile) {
            return
        }
        if (!file.isFile) {
            try {
                write(legacy.readText())
            } catch (_: Exception) {
                return
            }
        }
        legacy.writeBytes(ByteArray(legacy.length().toInt()))
        legacy.delete()
    }

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore")
        store.load(null)
        val existing = store.getKey(ALIAS, null)
        if (existing is SecretKey) {
            return existing
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(
                ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val ALIAS = "satchel-grant"
        const val TRANSFORM = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}
