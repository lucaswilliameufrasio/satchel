package io.github.lucaswilliameufrasio.satchel

import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext

/**
 * Listen for "look now" notices from paired devices on the mesh while this runs.
 *
 * Call it inside `repeatOnLifecycle(STARTED)`: Android does not keep a socket
 * open for an app in the background, so outside the foreground nothing listens
 * and the next open catches up. Without a Tailscale or NetBird address this
 * returns at once. [ip] is for tests; empty means the mesh address.
 */
internal suspend fun listenForWakes(stateDir: File, ip: String = "", onWake: suspend () -> Unit) {
    stateDir.mkdirs()
    val address = withContext(Dispatchers.IO) {
        runCatching { NativeCatalog.wakeStart(stateDir.absolutePath, ip) }.getOrDefault("")
    }
    if (address.isEmpty()) {
        return
    }
    try {
        while (currentCoroutineContext().isActive) {
            val woke = withContext(Dispatchers.IO) { NativeCatalog.wakeWait(WAKE_POLL_MS) }
            if (woke) {
                onWake()
            }
        }
    } finally {
        NativeCatalog.wakeStop()
    }
}

/** How long one native wait blocks, which bounds how late the listener stops. */
internal const val WAKE_POLL_MS = 1_000L

internal fun wakeStateDir(filesDir: File) = File(filesDir, "wake")
