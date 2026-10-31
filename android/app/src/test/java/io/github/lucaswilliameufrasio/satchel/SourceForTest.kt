package io.github.lucaswilliameufrasio.satchel

import io.github.lucaswilliameufrasio.satchel.catalog.CoreCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.GrantCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.SharedCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SourceForTest {
    @Test
    fun fixtureIgnoresAStorePath() {
        val source = sourceFor(true, "unused", "/data/spec.json", "/data/grant.json")
        val rows = source.listMetadata()
        assertEquals("notes/a.txt", rows[0].path)
        assertEquals("photos", rows[1].path)
    }

    @Test
    fun storePathWinsOverAGrant() {
        val source = sourceFor(false, "unused", "/data/spec.json", "/data/grant.json")
        assertTrue(source is SharedCatalog)
    }

    @Test
    fun grantPathSelectsTheGrantCatalog() {
        val source = sourceFor(false, "unused", null, "/data/grant.json")
        assertTrue(source is GrantCatalog)
    }

    @Test
    fun emptyPathsSelectTheSample() {
        val source = sourceFor(false, "unused", "", "")
        assertTrue(source is CoreCatalog)
    }
}
