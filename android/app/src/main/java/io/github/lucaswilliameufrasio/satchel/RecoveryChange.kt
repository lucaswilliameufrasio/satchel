package io.github.lucaswilliameufrasio.satchel

/** One sealed slot for a grant. [SecureGrant] is the real one; tests use memory. */
internal interface GrantSlot {
    fun read(): String?
    fun write(text: String)
    fun clear()
}

internal const val RECOVERY_CHANGE_TITLE = "Trocar senha de recuperação"

internal const val RECOVERY_CHANGE_NOTE =
    "Gera uma senha nova e a mostra uma vez. Os outros aparelhos continuam abrindo com a chave deles. " +
        "Quem guardou o arquivo de chave antigo, ou lê versões antigas num bucket com versionamento, " +
        "ainda abre com a senha velha: a chave dos dados não muda."

internal const val RECOVERY_CHANGE_FAILED =
    "Não foi possível trocar. A senha antiga continua valendo. Confira a conexão com o armazenamento."

internal const val RECOVERY_CHANGED_NOTE =
    "A senha antiga não abre mais o repositório. Guarde esta no gerenciador de senhas."

/**
 * Change the recovery password held in [main]. The new grant is sealed in [pending]
 * before the repository changes, so a crash in between never leaves this device
 * without a password that opens it. Returns the new password, or null if nothing changed.
 */
internal fun changeRecovery(
    main: GrantSlot,
    pending: GrantSlot,
    prepare: (String) -> String,
    commit: (String, String) -> Boolean,
): String? {
    val old = main.read() ?: return null
    val prepared = runCatching { prepare(old) }.getOrNull() ?: return null
    val split = prepared.indexOf('\n')
    if (split <= 0) {
        return null
    }
    val password = prepared.substring(0, split)
    val fresh = prepared.substring(split + 1)
    if (runCatching { pending.write(fresh) }.isFailure) {
        return null
    }
    if (!runCatching { commit(old, fresh) }.getOrDefault(false)) {
        pending.clear()
        return null
    }
    main.write(fresh)
    pending.clear()
    return password
}

/**
 * Finish a change that stopped halfway. Keeps [main] if it still opens, promotes
 * [pending] if only that one opens, and leaves both alone when neither answers
 * (no network), to try again on the next launch. True when [main] changed.
 */
internal fun settleRecovery(main: GrantSlot, pending: GrantSlot, opens: (String) -> Boolean): Boolean {
    val waiting = pending.read() ?: return false
    val current = main.read()
    if (current != null && runCatching { opens(current) }.getOrDefault(false)) {
        pending.clear()
        return false
    }
    if (runCatching { opens(waiting) }.getOrDefault(false)) {
        main.write(waiting)
        pending.clear()
        return true
    }
    return false
}
