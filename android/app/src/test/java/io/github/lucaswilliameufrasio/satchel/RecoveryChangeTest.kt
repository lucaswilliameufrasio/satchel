package io.github.lucaswilliameufrasio.satchel

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private class Slot(var value: String? = null, val failWrite: Boolean = false) : GrantSlot {
    override fun read(): String? = value
    override fun write(text: String) {
        if (failWrite) {
            throw IllegalStateException("no keystore")
        }
        value = text
    }
    override fun clear() {
        value = null
    }
}

class RecoveryChangeTest {
    private val prepare: (String) -> String = { "newpass\n{\"new\":true}" }

    @Test
    fun newGrantIsPendingBeforeCommitThenPromoted() {
        val main = Slot("{\"old\":true}")
        val pending = Slot()
        var pendingDuringCommit: String? = null
        val password = changeRecovery(main, pending, prepare) { old, new ->
            pendingDuringCommit = pending.value
            assertEquals("{\"old\":true}", old)
            assertEquals("{\"new\":true}", new)
            true
        }
        assertEquals("newpass", password)
        assertEquals("{\"new\":true}", pendingDuringCommit)
        assertEquals("{\"new\":true}", main.value)
        assertNull(pending.value)
    }

    @Test
    fun refusedCommitKeepsTheOldGrantAndDropsThePending() {
        val main = Slot("{\"old\":true}")
        val pending = Slot()
        assertNull(changeRecovery(main, pending, prepare) { _, _ -> false })
        assertEquals("{\"old\":true}", main.value)
        assertNull(pending.value)
        assertNull(changeRecovery(main, pending, { throw IllegalStateException("member") }) { _, _ -> true })
        assertEquals("{\"old\":true}", main.value)
    }

    @Test
    fun pendingThatCannotBeSealedNeverReachesTheRepository() {
        var committed = false
        val password = changeRecovery(Slot("{\"old\":true}"), Slot(failWrite = true), prepare) { _, _ ->
            committed = true
            true
        }
        assertNull(password)
        assertFalse(committed)
    }

    @Test
    fun launchSettlesAHalfFinishedChange() {
        // The repository changed before the crash: only the pending grant opens.
        val main = Slot("old")
        val pending = Slot("new")
        assertTrue(settleRecovery(main, pending) { it == "new" })
        assertEquals("new", main.value)
        assertNull(pending.value)

        // The crash came before the change: the old grant still opens.
        val keep = Slot("old")
        val stale = Slot("new")
        assertFalse(settleRecovery(keep, stale) { it == "old" })
        assertEquals("old", keep.value)
        assertNull(stale.value)

        // No network: neither answers, so nothing is thrown away.
        val offline = Slot("old")
        val waiting = Slot("new")
        assertFalse(settleRecovery(offline, waiting) { false })
        assertEquals("old", offline.value)
        assertEquals("new", waiting.value)
    }
}
