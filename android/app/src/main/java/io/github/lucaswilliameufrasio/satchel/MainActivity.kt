package io.github.lucaswilliameufrasio.satchel

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogRow
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogSession
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogSource
import io.github.lucaswilliameufrasio.satchel.catalog.CoreCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.Follow
import io.github.lucaswilliameufrasio.satchel.catalog.followLabel
import io.github.lucaswilliameufrasio.satchel.catalog.rowDetails
import io.github.lucaswilliameufrasio.satchel.catalog.rowTitle
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val fixture = intent.getBooleanExtra(EXTRA_FIXTURE, false)
        val sampleRoot = File(cacheDir, "sample").absolutePath
        val session = CatalogSession(sourceFor(fixture, sampleRoot))
        setContent {
            MaterialTheme {
                var screen by remember { mutableStateOf<CatalogScreenState>(CatalogScreenState.Loading) }
                val scope = rememberCoroutineScope()
                LaunchedEffect(fixture) {
                    screen = withContext(Dispatchers.IO) {
                        openCatalog(session)
                    }
                }
                when (val current = screen) {
                    CatalogScreenState.Loading -> StatusText("Lendo o catálogo…")
                    CatalogScreenState.Failed -> StatusText("Não foi possível abrir o catálogo.")
                    is CatalogScreenState.Ready -> CatalogScreen(current.rows, current.busy) { path, follow ->
                        if (current.busy) {
                            return@CatalogScreen
                        }
                        screen = CatalogScreenState.Ready(current.rows, true)
                        scope.launch {
                            screen = withContext(Dispatchers.IO) {
                                chooseCatalog(session, path, follow)
                            }
                        }
                    }
                }
            }
        }
    }
}

private sealed interface CatalogScreenState {
    data object Loading : CatalogScreenState

    data class Ready(val rows: List<CatalogRow>, val busy: Boolean) : CatalogScreenState

    data object Failed : CatalogScreenState
}

internal const val EXTRA_FIXTURE = "satchel.fixture"

internal fun sourceFor(fixture: Boolean, sampleRoot: String): CatalogSource {
    if (fixture) {
        return FixtureCatalog()
    }
    return CoreCatalog(sampleRoot)
}

private fun openCatalog(session: CatalogSession): CatalogScreenState {
    synchronized(session) {
        try {
            session.open()
            return CatalogScreenState.Ready(session.rows, false)
        } catch (_: RuntimeException) {
            return CatalogScreenState.Failed
        }
    }
}

private fun chooseCatalog(session: CatalogSession, path: String, follow: Follow): CatalogScreenState {
    synchronized(session) {
        try {
            session.choose(path, follow)
            return CatalogScreenState.Ready(session.rows, false)
        } catch (_: RuntimeException) {
            return CatalogScreenState.Failed
        }
    }
}

@androidx.compose.runtime.Composable
private fun CatalogScreen(
    rows: List<CatalogRow>,
    busy: Boolean,
    onChoose: (String, Follow) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Text("Catálogo", style = MaterialTheme.typography.headlineSmall)
        if (rows.isEmpty()) {
            Text(
                "Nenhum arquivo no catálogo.",
                modifier = Modifier.padding(top = 16.dp),
            )
            return@Column
        }
        LazyColumn(modifier = Modifier.padding(top = 16.dp)) {
            items(rows, key = { row -> row.path }) { row ->
                CatalogRowView(row, busy, onChoose)
            }
        }
    }
}

@androidx.compose.runtime.Composable
private fun CatalogRowView(
    row: CatalogRow,
    busy: Boolean,
    onChoose: (String, Follow) -> Unit,
) {
    Column(modifier = Modifier.padding(bottom = 20.dp)) {
        Text(rowTitle(row), style = MaterialTheme.typography.titleMedium)
        Text(rowDetails(row))
        Text(followLabel(row.follow))
        Row {
            for (follow in Follow.entries) {
                Button(
                    onClick = { onChoose(row.path, follow) },
                    enabled = !busy,
                    modifier = Modifier.padding(end = 8.dp),
                ) {
                    Text(followLabel(follow))
                }
            }
        }
    }
}

@androidx.compose.runtime.Composable
private fun StatusText(message: String) {
    Text(message, modifier = Modifier.padding(16.dp))
}

/** Rows used only when a test launches the screen with [EXTRA_FIXTURE]. */
private class FixtureCatalog : CatalogSource {
    private val content = "fixture-bytes".toByteArray()
    private var saved = mutableMapOf<String, Follow>()
    private val present = mutableSetOf<String>()

    override fun listMetadata(): List<CatalogRow> {
        return listOf(
            row("notes/a.txt", false, Follow.See),
            row("photos", true, Follow.See),
        )
    }

    override fun hasContent(path: String): Boolean {
        return present.contains(path)
    }

    override fun readContent(path: String): ByteArray {
        present.add(path)
        return content
    }

    override fun remember(path: String, follow: Follow) {
        saved[path] = follow
    }

    private fun row(path: String, directory: Boolean, fallback: Follow): CatalogRow {
        return CatalogRow(
            path = path,
            isDirectory = directory,
            size = if (directory) 0 else 12,
            versionUnix = 1_700_000_000,
            devices = listOf("0123456789abcdef0123456789abcdef"),
            follow = saved[path] ?: fallback,
        )
    }
}
