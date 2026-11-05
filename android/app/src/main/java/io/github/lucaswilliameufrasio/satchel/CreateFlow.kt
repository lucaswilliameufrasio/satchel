package io.github.lucaswilliameufrasio.satchel

import androidx.annotation.StringRes
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File

/** Where the new repository lives. The same fields as the desktop. */
internal enum class StorageChoice(val wire: String, @StringRes val label: Int) {
    Folder("folder", R.string.storage_folder),
    S3("s3", R.string.storage_s3),
    R2("r2", R.string.storage_r2),
    B2("b2", R.string.storage_b2),
}

internal data class CreateForm(
    val storage: StorageChoice = StorageChoice.Folder,
    val endpoint: String = "",
    val bucket: String = "",
    val region: String = "",
    val prefix: String = "",
    val accessKeyId: String = "",
    val secretAccessKey: String = "",
    val deviceAccessKeyId: String = "",
    val deviceSecretAccessKey: String = "",
    val apelido: String = "",
    val busy: Boolean = false,
    val failed: Boolean = false,
    val reason: String = "",
)

/** The spec the native side reads. Secrets travel to the library and nowhere else. */
internal fun createSpecJson(form: CreateForm, directory: String, deviceId: String): String {
    val fields = mutableListOf<String>()
    fields.add("\"kind\":" + jsonString(form.storage.wire))
    fields.add("\"device_id\":" + jsonString(deviceId))
    if (form.storage == StorageChoice.Folder) {
        fields.add("\"directory\":" + jsonString(directory))
    } else {
        fields.add("\"endpoint\":" + jsonString(form.endpoint.trim()))
        fields.add("\"bucket\":" + jsonString(form.bucket.trim()))
        fields.add("\"region\":" + jsonString(form.region.trim()))
        fields.add("\"prefix\":" + jsonString(form.prefix.trim()))
        fields.add("\"access_key_id\":" + jsonString(form.accessKeyId.trim()))
        fields.add("\"secret_access_key\":" + jsonString(form.secretAccessKey.trim()))
        fields.add("\"device_access_key_id\":" + jsonString(form.deviceAccessKeyId.trim()))
        fields.add("\"device_secret_access_key\":" + jsonString(form.deviceSecretAccessKey.trim()))
    }
    return "{" + fields.joinToString(",") + "}"
}

/** True when every field the chosen storage needs has text. */
internal fun createFormComplete(form: CreateForm): Boolean {
    if (form.storage == StorageChoice.Folder) {
        return true
    }
    val needed = listOf(
        form.endpoint,
        form.bucket,
        form.region,
        form.accessKeyId,
        form.secretAccessKey,
        form.deviceAccessKeyId,
        form.deviceSecretAccessKey,
    )
    for (value in needed) {
        if (value.isBlank()) {
            return false
        }
    }
    return form.accessKeyId.trim() != form.deviceAccessKeyId.trim()
}

/** Why the native side refused, in words a person can act on. */
@StringRes
internal fun createFailureCopy(reason: String): Int = when (reason) {
    "protection" -> R.string.create_failed_protection
    "denied" -> R.string.create_failed_denied
    "exists" -> R.string.create_failed_exists
    "unreachable" -> R.string.create_failed_unreachable
    "fields" -> R.string.create_failed_fields
    else -> R.string.create_refused
}

internal data class CreateResult(
    /** The recovery password, shown once. Null when the setup was refused. */
    val password: String?,
    /** False when the repository exists but the grant could not be sealed in the Keystore. */
    val stored: Boolean,
    val reason: String,
)

/**
 * Create the repository and seal the grant in the Keystore. A refusal leaves no grant;
 * only the opaque device id stays.
 */
internal fun createRepositoryFile(filesDir: File, form: CreateForm): CreateResult {
    try {
        val deviceId = readOrCreateDeviceId(filesDir)
        val spec = createSpecJson(form, File(filesDir, "repository").absolutePath, deviceId)
        val work = File(filesDir, "work")
        val created = NativeCatalog.createRepository(spec, work.absolutePath)
        val split = created.indexOf('\n')
        val password = created.substring(0, split)
        val grant = created.substring(split + 1)
        val stored = try {
            SecureGrant(filesDir).write(grant)
            true
        } catch (_: Exception) {
            false
        }
        return CreateResult(password, stored, "")
    } catch (_: Exception) {
        val reason = try {
            NativeCatalog.createFailure()
        } catch (_: Throwable) {
            ""
        }
        return CreateResult(null, false, reason)
    }
}
