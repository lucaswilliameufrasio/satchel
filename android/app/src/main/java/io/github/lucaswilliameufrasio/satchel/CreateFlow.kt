package io.github.lucaswilliameufrasio.satchel

import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File

/** Where the new repository lives. The same fields as the desktop. */
internal enum class StorageChoice(val wire: String, val label: String) {
    Folder("folder", "Pasta"),
    S3("s3", "S3"),
    R2("r2", "R2"),
    B2("b2", "B2"),
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

internal const val CREATE_TITLE = "Criar repositório"

internal const val CREATE_NOTE =
    "Este aparelho passa a guardar a senha de recuperação e a credencial de manutenção. " +
        "O outro aparelho entra com um código e não digita a conta de novo."

internal const val CREATE_FOLDER_NOTE =
    "A pasta fica só neste aparelho. Ela não tem versionamento de objeto."

internal const val CREATE_OBJECT_NOTE =
    "O armazenamento precisa ter versionamento ligado e nenhum ciclo de vida que apague objeto. " +
        "A chave do aparelho lista, lê, grava e só apaga os próprios locks."

internal const val CREATE_REFUSED_COPY =
    "Não foi possível criar. Confira os campos, o versionamento e o ciclo de vida."

internal const val RECOVERY_TITLE = "Senha de recuperação"

internal const val RECOVERY_NOTE =
    "Guarde esta senha num gerenciador de senhas que abre sem este repositório. " +
        "Ela aparece só agora e o restic oficial usa esta senha."

internal const val INVITE_TITLE = "Convite"

internal const val INVITE_WAITING_COPY = "Esperando o outro aparelho digitar o código."

internal const val INVITE_NOTE = "A senha de recuperação não vai no convite."

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

internal const val VAULT_FAILED_COPY =
    "O repositório foi criado, mas este aparelho não conseguiu guardar a concessão no cofre. " +
        "Guarde a senha abaixo agora: sem ela e sem a concessão não dá para abrir de novo por aqui."

/** Why the native side refused, in words a person can act on. */
internal fun createFailureCopy(reason: String): String = when (reason) {
    "protection" ->
        "O armazenamento recusou: ligue o versionamento do bucket e remova regras de ciclo de vida que apagam objetos."
    "exists" ->
        "Já existe um repositório neste lugar. Escolha outro prefixo, ou entre nele com um código."
    "unreachable" ->
        "Não consegui falar com o armazenamento. Confira endpoint, região e as chaves."
    "fields" -> "Confira os campos. A chave dos aparelhos precisa ser diferente da de manutenção."
    else -> CREATE_REFUSED_COPY
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
