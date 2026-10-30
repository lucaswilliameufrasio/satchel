package io.github.lucaswilliameufrasio.satchel

import org.junit.Assert.assertEquals
import org.junit.Test

class SourceForTest {
    @Test
    fun fixtureIgnoresAStorePath() {
        val source = sourceFor(true, "unused", "/data/spec.json")
        val rows = source.listMetadata()
        assertEquals("notes/a.txt", rows[0].path)
        assertEquals("photos", rows[1].path)
    }
}
