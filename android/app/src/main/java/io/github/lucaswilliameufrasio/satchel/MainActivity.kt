@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.lucaswilliameufrasio.satchel

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
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
import io.github.lucaswilliameufrasio.satchel.catalog.deviceText
import io.github.lucaswilliameufrasio.satchel.catalog.followLabel
import io.github.lucaswilliameufrasio.satchel.catalog.rowTitle
import io.github.lucaswilliameufrasio.satchel.catalog.rowWhen
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
        val canPair = storedGrant == null && !fixture && storePath.isNullOrEmpty()
        val showPair = pairRequested && canPair
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
                    is CatalogScreenState.Ready -> CatalogScreen(
                        current.rows,
                        canPair,
                        onPair = {
                            screen = CatalogScreenState.Pairing("", "", "", false, false)
                        },
                        onOpen = { path ->
                            if (!current.busy) {
                                screen = CatalogScreenState.File(path, current.rows, false)
                            }
                        },
                    )
                    is CatalogScreenState.File -> FileScreen(
                        current,
                        onBack = {
                            if (!current.busy) {
                                screen = CatalogScreenState.Ready(current.rows, false)
                            }
                        },
                        onChoose = { follow ->
                            val active = session
                            if (current.busy || active == null) {
                                return@FileScreen
                            }
                            screen = current.copy(busy = true)
                            scope.launch {
                                val next = withContext(Dispatchers.IO) {
                                    chooseCatalog(active, current.path, follow)
                                }
                                screen = when (next) {
                                    is CatalogScreenState.Ready -> CatalogScreenState.File(
                                        current.path,
                                        next.rows,
                                        false,
                                    )
                                    else -> next
                                }
                            }
                        },
                    )
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
                        onScan = {
                            val pairing = screen
                            if (pairing is CatalogScreenState.Pairing && !pairing.busy) {
                                screen = CatalogScreenState.Scanning(
                                    pairing.address,
                                    pairing.code,
                                    pairing.fingerprint,
                                )
                            }
                        },
                        onBack = {
                            val opened = CatalogSession(sourceFor(fixture, sampleRoot, storePath, storedGrant))
                            session = opened
                            scope.launch {
                                screen = withContext(Dispatchers.IO) {
                                    openCatalog(opened)
                                }
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
                    is CatalogScreenState.Scanning -> QrScanner(
                        onCode = { raw ->
                            val previous = current
                            scope.launch {
                                val fields = withContext(Dispatchers.IO) {
                                    try {
                                        inviteFields(NativeCatalog.parseInvite(raw))
                                    } catch (_: Exception) {
                                        null
                                    }
                                }
                                if (fields == null) {
                                    screen = CatalogScreenState.Pairing(
                                        previous.address,
                                        previous.code,
                                        previous.fingerprint,
                                        false,
                                        true,
                                    )
                                } else {
                                    screen = CatalogScreenState.Pairing(
                                        fields.address,
                                        fields.code,
                                        fields.fingerprint,
                                        false,
                                        false,
                                    )
                                }
                            }
                        },
                        onClose = {
                            screen = CatalogScreenState.Pairing(
                                current.address,
                                current.code,
                                current.fingerprint,
                                false,
                                false,
                            )
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

    data class File(val path: String, val rows: List<CatalogRow>, val busy: Boolean) : CatalogScreenState

    data object Failed : CatalogScreenState

    data class Pairing(
        val address: String,
        val code: String,
        val fingerprint: String,
        val busy: Boolean,
        val failed: Boolean,
    ) : CatalogScreenState

    data class Scanning(
        val address: String,
        val code: String,
        val fingerprint: String,
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

@OptIn(ExperimentalMaterial3Api::class)
@androidx.compose.runtime.Composable
private fun PhoneScaffold(
    title: String,
    onBack: (() -> Unit)?,
    content: @androidx.compose.runtime.Composable (PaddingValues) -> Unit,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    if (onBack != null) {
                        TextButton(onClick = onBack) {
                            Text("Voltar")
                        }
                    }
                },
            )
        },
        content = content,
    )
}

@androidx.compose.runtime.Composable
private fun CatalogScreen(
    rows: List<CatalogRow>,
    canPair: Boolean,
    onPair: () -> Unit,
    onOpen: (String) -> Unit,
) {
    PhoneScaffold(title = "Catálogo", onBack = null) { padding ->
        LazyColumn(modifier = Modifier.fillMaxSize().padding(padding)) {
            if (canPair) {
                item {
                    ListItem(
                        headlineContent = { Text("Entrar neste aparelho") },
                        modifier = Modifier.clickable(onClick = onPair),
                    )
                }
            }
            if (rows.isEmpty()) {
                item {
                    Text(
                        "Nenhum arquivo no catálogo.",
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
            items(rows, key = { row -> row.path }) { row ->
                ListItem(
                    headlineContent = { Text(rowTitle(row), style = MaterialTheme.typography.titleMedium) },
                    overlineContent = { Text(followLabel(row.follow)) },
                    supportingContent = {
                        Text("Sincronizado em ${rowWhen(row)}\nAparelhos: ${deviceText(row)}")
                    },
                    modifier = Modifier.clickable { onOpen(row.path) },
                )
            }
        }
    }
}

@androidx.compose.runtime.Composable
private fun FileScreen(
    state: CatalogScreenState.File,
    onBack: () -> Unit,
    onChoose: (Follow) -> Unit,
) {
    val row = listedRow(state.path, state.rows)
    PhoneScaffold(title = state.path, onBack = onBack) { padding ->
        LazyColumn(modifier = Modifier.fillMaxSize().padding(padding)) {
            if (row == null) {
                item {
                    Text("Nenhum arquivo no catálogo.", modifier = Modifier.padding(16.dp))
                }
                return@LazyColumn
            }
            item {
                ListItem(
                    overlineContent = { Text("Sincronizado") },
                    headlineContent = { Text(rowWhen(row)) },
                )
            }
            item {
                ListItem(
                    overlineContent = { Text("Aparelhos") },
                    headlineContent = { Text(deviceText(row)) },
                )
            }
            item {
                Text(
                    "Estado",
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 8.dp),
                )
            }
            items(Follow.entries, key = { follow -> follow.name }) { follow ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(enabled = !state.busy) { onChoose(follow) }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                ) {
                    RadioButton(
                        selected = row.follow == follow,
                        onClick = { onChoose(follow) },
                        enabled = !state.busy,
                    )
                    Text(
                        followLabel(follow),
                        modifier = Modifier.padding(top = 12.dp),
                    )
                }
            }
        }
    }
}

private fun listedRow(path: String, rows: List<CatalogRow>): CatalogRow? {
    for (row in rows) {
        if (row.path == path) {
            return row
        }
    }
    return null
}

@androidx.compose.runtime.Composable
private fun PairScreen(
    state: CatalogScreenState.Pairing,
    onAddress: (String) -> Unit,
    onCode: (String) -> Unit,
    onFingerprint: (String) -> Unit,
    onScan: () -> Unit,
    onBack: () -> Unit,
    onSubmit: () -> Unit,
) {
    PhoneScaffold(title = "Entrar neste aparelho", onBack = onBack) { padding ->
        Column(modifier = Modifier.fillMaxSize().padding(padding).padding(16.dp)) {
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
        TextButton(
            onClick = onScan,
            enabled = !state.busy,
            modifier = Modifier.padding(top = 16.dp),
        ) {
            Text("Ler QR")
        }
        TextButton(
            onClick = onSubmit,
            enabled = !state.busy,
            modifier = Modifier.padding(top = 8.dp),
        ) {
            Text("Entrar")
        }
        }
    }
}

@androidx.compose.runtime.Composable
private fun StatusText(message: String) {
    PhoneScaffold(title = "Catálogo", onBack = null) { padding ->
        Text(message, modifier = Modifier.padding(padding).padding(16.dp))
    }
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
