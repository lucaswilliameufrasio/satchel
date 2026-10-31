package io.github.lucaswilliameufrasio.satchel

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import io.github.lucaswilliameufrasio.satchel.catalog.GrantCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.SharedCatalog
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
        val storePath = intent.getStringExtra(EXTRA_STORE)
        val pairRequested = intent.getBooleanExtra(EXTRA_PAIR, false)
        val grantFile = File(filesDir, "grant.json")
        val storedGrant = if (grantFile.isFile) {
            grantFile.absolutePath
        } else {
            null
        }
        val showPair = pairRequested && storedGrant == null && !fixture && storePath.isNullOrEmpty()
        val initialSession = if (showPair) {
            null
        } else {
            CatalogSession(sourceFor(fixture, sampleRoot, storePath, storedGrant))
        }
        setContent {
            MaterialTheme {
                var screen by remember {
                    mutableStateOf<CatalogScreenState>(
                        if (showPair) {
                            CatalogScreenState.Pairing("", "", "", false, false)
                        } else {
                            CatalogScreenState.Loading
                        },
                    )
                }
                var session by remember { mutableStateOf(initialSession) }
                val scope = rememberCoroutineScope()
                LaunchedEffect(fixture, storePath, storedGrant) {
                    val opened = session
                    if (opened != null) {
                        screen = withContext(Dispatchers.IO) {
                            openCatalog(opened)
                        }
                    }
                }
                when (val current = screen) {
                    CatalogScreenState.Loading -> StatusText("Lendo o catálogo…")
                    CatalogScreenState.Failed -> StatusText("Não foi possível abrir o catálogo.")
                    is CatalogScreenState.Ready -> CatalogScreen(current.rows, current.busy) { path, follow ->
                        val active = session
                        if (current.busy || active == null) {
                            return@CatalogScreen
                        }
                        screen = CatalogScreenState.Ready(current.rows, true)
                        scope.launch {
                            screen = withContext(Dispatchers.IO) {
                                chooseCatalog(active, path, follow)
                            }
                        }
                    }
                    is CatalogScreenState.Pairing -> PairScreen(
                        current,
                        onAddress = { value ->
                            val pairing = screen
                            if (pairing is CatalogScreenState.Pairing) {
                                screen = pairing.copy(address = value, failed = false)
                            }
                        },
                        onCode = { value ->
                            val pairing = screen
                            if (pairing is CatalogScreenState.Pairing) {
                                screen = pairing.copy(code = value, failed = false)
                            }
                        },
                        onFingerprint = { value ->
                            val pairing = screen
                            if (pairing is CatalogScreenState.Pairing) {
                                screen = pairing.copy(fingerprint = value, failed = false)
                            }
                        },
                        onSubmit = {
                            val pairing = screen
                            if (pairing !is CatalogScreenState.Pairing || pairing.busy) {
                                return@PairScreen
                            }
                            if (pairing.address.isBlank() || pairing.code.isBlank() || pairing.fingerprint.isBlank()) {
                                screen = pairing.copy(failed = true)
                                return@PairScreen
                            }
                            val address = pairing.address.trim()
                            val code = pairing.code.trim()
                            val fingerprint = pairing.fingerprint.trim()
                            screen = pairing.copy(
                                address = address,
                                code = code,
                                fingerprint = fingerprint,
                                busy = true,
                                failed = false,
                            )
                            scope.launch {
                                val joined = withContext(Dispatchers.IO) {
                                    joinGrant(filesDir, address, code, fingerprint)
                                }
                                if (joined == null) {
                                    screen = CatalogScreenState.Pairing(address, code, fingerprint, false, true)
                                    return@launch
                                }
                                session = joined
                                val opened = withContext(Dispatchers.IO) {
                                    openCatalog(joined)
                                }
                                if (opened is CatalogScreenState.Failed) {
                                    grantFile.delete()
                                    session = null
                                    screen = CatalogScreenState.Pairing(address, code, fingerprint, false, true)
                                } else {
                                    screen = opened
                                }
                            }
                        },
                    )
                }
            }
        }
    }
}

private sealed interface CatalogScreenState {
    data object Loading : CatalogScreenState

    data class Ready(val rows: List<CatalogRow>, val busy: Boolean) : CatalogScreenState

    data object Failed : CatalogScreenState

    data class Pairing(
        val address: String,
        val code: String,
        val fingerprint: String,
        val busy: Boolean,
        val failed: Boolean,
    ) : CatalogScreenState
}

internal const val EXTRA_FIXTURE = "satchel.fixture"

internal const val EXTRA_STORE = "satchel.store"

internal const val EXTRA_PAIR = "satchel.pair"

internal fun sourceFor(
    fixture: Boolean,
    sampleRoot: String,
    storePath: String?,
    grantPath: String?,
): CatalogSource {
    if (fixture) {
        return FixtureCatalog()
    }
    if (!storePath.isNullOrEmpty()) {
        return SharedCatalog(storePath)
    }
    if (!grantPath.isNullOrEmpty()) {
        return GrantCatalog(grantPath)
    }
    return CoreCatalog(sampleRoot)
}

private fun joinGrant(filesDir: File, address: String, code: String, fingerprint: String): CatalogSession? {
    try {
        val deviceId = readOrCreateDeviceId(filesDir)
        val work = File(filesDir, "work")
        val raw = NativeCatalog.joinPairing(
            address,
            code,
            fingerprint,
            "Satchel",
            "Android",
            deviceId,
        )
        val text = assembleGrant(raw, deviceId, work.absolutePath)
        val file = File(filesDir, "grant.json")
        file.writeText(text)
        return CatalogSession(GrantCatalog(file.absolutePath))
    } catch (_: Exception) {
        return null
    }
}

private fun readOrCreateDeviceId(filesDir: File): String {
    val file = File(filesDir, "device-id")
    if (file.isFile) {
        val existing = file.readText().trim()
        if (existing.length == 32) {
            return existing
        }
    }
    val created = NativeCatalog.newDeviceId()
    file.writeText(created)
    return created
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
private fun PairScreen(
    state: CatalogScreenState.Pairing,
    onAddress: (String) -> Unit,
    onCode: (String) -> Unit,
    onFingerprint: (String) -> Unit,
    onSubmit: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Text("Entrar neste aparelho", style = MaterialTheme.typography.headlineSmall)
        OutlinedTextField(
            value = state.address,
            onValueChange = onAddress,
            enabled = !state.busy,
            label = { Text("Endereço") },
            modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
        )
        OutlinedTextField(
            value = state.code,
            onValueChange = onCode,
            enabled = !state.busy,
            label = { Text("Código") },
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        OutlinedTextField(
            value = state.fingerprint,
            onValueChange = onFingerprint,
            enabled = !state.busy,
            label = { Text("Impressão digital") },
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        if (state.failed) {
            Text(
                "Não foi possível entrar.",
                modifier = Modifier.padding(top = 12.dp),
            )
        }
        Button(
            onClick = onSubmit,
            enabled = !state.busy,
            modifier = Modifier.padding(top = 16.dp),
        ) {
            Text("Entrar")
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
