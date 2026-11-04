@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.lucaswilliameufrasio.satchel

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogRow
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogSession
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogSource
import io.github.lucaswilliameufrasio.satchel.catalog.CoreCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.EMPTY_CATALOG
import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_FIELDS
import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_NOTE
import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_TITLE
import io.github.lucaswilliameufrasio.satchel.catalog.Follow
import io.github.lucaswilliameufrasio.satchel.catalog.FOLDER_CHOOSE_NOTE
import io.github.lucaswilliameufrasio.satchel.catalog.FOLDER_LOST
import io.github.lucaswilliameufrasio.satchel.catalog.FOLDER_TITLE
import io.github.lucaswilliameufrasio.satchel.catalog.GrantCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.PhoneFolder
import io.github.lucaswilliameufrasio.satchel.catalog.LOADING_COPY
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.OPEN_FAILURE_COPY
import io.github.lucaswilliameufrasio.satchel.catalog.PAIR_SUPPORT
import io.github.lucaswilliameufrasio.satchel.catalog.SAVING_COPY
import io.github.lucaswilliameufrasio.satchel.catalog.SharedCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.catalogMeta
import io.github.lucaswilliameufrasio.satchel.catalog.deviceCountLabel
import io.github.lucaswilliameufrasio.satchel.catalog.deviceSectionLines
import io.github.lucaswilliameufrasio.satchel.catalog.fileHeaderLines
import io.github.lucaswilliameufrasio.satchel.catalog.followDetail
import io.github.lucaswilliameufrasio.satchel.catalog.followLabel
import io.github.lucaswilliameufrasio.satchel.catalog.labelStoredFromEntry
import io.github.lucaswilliameufrasio.satchel.catalog.localDeviceId
import io.github.lucaswilliameufrasio.satchel.catalog.openingApelido
import io.github.lucaswilliameufrasio.satchel.catalog.pairingAfterQr
import io.github.lucaswilliameufrasio.satchel.catalog.rowTitle
import java.io.File
import java.time.ZoneId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val fixture = intent.getBooleanExtra(EXTRA_FIXTURE, false)
        val sampleRoot = File(cacheDir, "sample").absolutePath
        val storePath = intent.getStringExtra(EXTRA_STORE)
        val pairRequested = intent.getBooleanExtra(EXTRA_PAIR, false)
        val vault = SecureGrant(filesDir)
        val pendingVault = SecureGrant(filesDir, PENDING_GRANT)
        vault.migrateLegacy()
        val storedGrant = vault.read()
        val canPair = storedGrant == null && !fixture && storePath.isNullOrEmpty()
        val showPair = pairRequested && canPair
        val apelido = { readDeviceLabel(filesDir) }
        val folderChoice = FolderChoice(filesDir)
        val openFolder = { folderChoice.open(contentResolver) }
        val initialPhoneId = localDeviceId(storedGrant != null, readStoredDeviceId(filesDir))
        val initialSession = if (showPair) {
            null
        } else {
            CatalogSession(sourceFor(fixture, sampleRoot, storePath, storedGrant, apelido, openFolder))
        }
        setContent {
            MaterialTheme {
                var phoneId by remember { mutableStateOf(initialPhoneId) }
                var screen by remember {
                    mutableStateOf<CatalogScreenState>(
                        if (showPair) {
                            CatalogScreenState.Pairing(
                                "",
                                "",
                                "",
                                openingApelido(apelido()),
                                false,
                                false,
                            )
                        } else {
                            CatalogScreenState.Loading
                        },
                    )
                }
                var session by remember { mutableStateOf(initialSession) }
                var hasGrant by remember { mutableStateOf(storedGrant != null) }
                val ordinary = !fixture && storePath.isNullOrEmpty()
                val canCreate = ordinary && !hasGrant
                val canInvite = remember(hasGrant) {
                    ordinary && hasGrant &&
                        runCatching { NativeCatalog.isMaintainer(vault.read().orEmpty()) }
                            .getOrDefault(false)
                }
                val scope = rememberCoroutineScope()
                var folderUri by remember { mutableStateOf(folderChoice.read()) }
                var folderStatus by remember { mutableStateOf<String?>(null) }
                var folderBusy by remember { mutableStateOf(false) }
                val folderName = remember(folderUri) {
                    folderUri?.let { folderDisplayName(contentResolver, it) }
                }
                val runFolder: () -> Unit = runFolder@{
                    val active = session
                    if (folderBusy || active == null || !hasGrant || !ordinary) {
                        return@runFolder
                    }
                    folderBusy = true
                    folderStatus = FOLDER_SYNCING
                    scope.launch {
                        val (summary, next) = withContext(Dispatchers.IO) {
                            synchronized(active) {
                                val said = folderRound(vault.read().orEmpty(), openFolder(), apelido())
                                said to openCatalog(active)
                            }
                        }
                        folderStatus = summary
                        folderBusy = false
                        if (screen is CatalogScreenState.Ready && next is CatalogScreenState.Ready) {
                            screen = next
                        }
                    }
                }
                val pickFolder = rememberLauncherForActivityResult(
                    ActivityResultContracts.OpenDocumentTree(),
                ) { picked ->
                    if (picked != null) {
                        val kept = runCatching { folderChoice.choose(contentResolver, picked) }.isSuccess
                        folderUri = folderChoice.read()
                        if (kept) {
                            runFolder()
                        } else {
                            folderStatus = FOLDER_LOST
                        }
                    }
                }
                LaunchedEffect(fixture, storePath, storedGrant) {
                    if (ordinary && pendingVault.exists()) {
                        val moved = withContext(Dispatchers.IO) {
                            settleRecovery(vault, pendingVault) { grant ->
                                runCatching { NativeCatalog.grantOpens(grant) }.getOrDefault(false)
                            }
                        }
                        if (moved) {
                            session = CatalogSession(GrantCatalog(vault.read().orEmpty(), apelido, openFolder))
                        }
                    }
                    val opened = session
                    if (opened != null) {
                        screen = withContext(Dispatchers.IO) {
                            openCatalog(opened)
                        }
                    }
                    if (folderUri != null) {
                        runFolder()
                    }
                }
                when (val current = screen) {
                    CatalogScreenState.Loading -> StatusText(LOADING_COPY)
                    CatalogScreenState.Failed -> StatusText(OPEN_FAILURE_COPY)
                    is CatalogScreenState.Ready -> CatalogScreen(
                        current.rows,
                        canPair && !hasGrant,
                        canCreate = canCreate,
                        onCreate = { screen = CatalogScreenState.Creating(CreateForm()) },
                        canInvite = canInvite,
                        onChangeRecovery = {
                            screen = CatalogScreenState.ChangingRecovery(busy = false, failed = false)
                        },
                        folder = if (ordinary && hasGrant) {
                            FolderCard(folderName, folderStatus, folderBusy)
                        } else {
                            null
                        },
                        onChooseFolder = { pickFolder.launch(folderUri) },
                        onSyncFolder = runFolder,
                        onInvite = {
                            scope.launch {
                                val started = withContext(Dispatchers.IO) {
                                    startHolding(vault.read().orEmpty())
                                }
                                if (started != null) {
                                    screen = started
                                }
                            }
                        },
                        onPair = {
                            screen = CatalogScreenState.Pairing(
                                "",
                                "",
                                "",
                                openingApelido(apelido()),
                                false,
                                false,
                            )
                        },
                        onOpen = { path ->
                            if (!current.busy) {
                                screen = CatalogScreenState.File(path, current.rows, false, phoneId)
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
                                        current.localDeviceId,
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
                                    pairing.apelido,
                                )
                            }
                        },
                        onApelido = { value ->
                            val pairing = screen
                            if (pairing is CatalogScreenState.Pairing) {
                                screen = pairing.copy(apelido = value, failed = false)
                            }
                        },
                        onBack = {
                            val opened = CatalogSession(
                                sourceFor(fixture, sampleRoot, storePath, storedGrant, apelido, openFolder),
                            )
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
                            val storedApelido = labelStoredFromEntry(pairing.apelido)
                            if (pairing.address.isBlank() || pairing.code.isBlank() || pairing.fingerprint.isBlank() || storedApelido == null) {
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
                                apelido = storedApelido,
                                busy = true,
                                failed = false,
                            )
                            scope.launch {
                                val joined = withContext(Dispatchers.IO) {
                                    writeDeviceLabel(filesDir, storedApelido)
                                    if (storedApelido.isEmpty()) {
                                        null
                                    } else {
                                        joinGrant(filesDir, address, code, fingerprint, storedApelido, openFolder)
                                    }
                                }
                                if (joined == null) {
                                    screen = CatalogScreenState.Pairing(
                                        address,
                                        code,
                                        fingerprint,
                                        storedApelido,
                                        false,
                                        true,
                                    )
                                    return@launch
                                }
                                session = joined
                                phoneId = localDeviceId(true, readStoredDeviceId(filesDir))
                                val opened = withContext(Dispatchers.IO) {
                                    openCatalog(joined)
                                }
                                if (opened is CatalogScreenState.Failed) {
                                    vault.clear()
                                    session = null
                                    screen = CatalogScreenState.Pairing(
                                        address,
                                        code,
                                        fingerprint,
                                        storedApelido,
                                        false,
                                        true,
                                    )
                                } else {
                                    screen = opened
                                }
                            }
                        },
                    )
                    is CatalogScreenState.Creating -> CreateScreen(
                        current.form,
                        onForm = { screen = CatalogScreenState.Creating(it) },
                        onBack = {
                            val opened = CatalogSession(
                                sourceFor(fixture, sampleRoot, storePath, storedGrant, apelido, openFolder),
                            )
                            session = opened
                            scope.launch {
                                screen = withContext(Dispatchers.IO) { openCatalog(opened) }
                            }
                        },
                        onSubmit = {
                            val form = current.form
                            if (form.busy || !createFormComplete(form)) {
                                return@CreateScreen
                            }
                            screen = CatalogScreenState.Creating(form.copy(busy = true, failed = false))
                            scope.launch {
                                val result = withContext(Dispatchers.IO) {
                                    createRepositoryFile(filesDir, form)
                                }
                                if (result.password == null) {
                                    screen = CatalogScreenState.Creating(
                                        form.copy(busy = false, failed = true, reason = result.reason),
                                    )
                                    return@launch
                                }
                                hasGrant = result.stored
                                phoneId = localDeviceId(result.stored, readStoredDeviceId(filesDir))
                                val warning = when {
                                    !result.stored -> VAULT_FAILED_COPY
                                    form.storage == StorageChoice.Folder -> CREATE_FOLDER_NOTE
                                    else -> null
                                }
                                screen = CatalogScreenState.Recovery(result.password, warning)
                            }
                        },
                    )
                    is CatalogScreenState.ChangingRecovery -> RecoveryChangeScreen(
                        busy = current.busy,
                        failed = current.failed,
                        onChange = {
                            screen = CatalogScreenState.ChangingRecovery(busy = true, failed = false)
                            scope.launch {
                                val fresh = withContext(Dispatchers.IO) {
                                    changeRecovery(
                                        vault,
                                        pendingVault,
                                        prepare = { NativeCatalog.recoveryPrepare(it) },
                                        commit = { old, new -> NativeCatalog.recoveryCommit(old, new) },
                                    )
                                }
                                screen = if (fresh == null) {
                                    CatalogScreenState.ChangingRecovery(busy = false, failed = true)
                                } else {
                                    CatalogScreenState.Recovery(fresh, RECOVERY_CHANGED_NOTE)
                                }
                            }
                        },
                        onBack = {
                            val opened = session
                            if (opened != null) {
                                scope.launch {
                                    screen = withContext(Dispatchers.IO) { openCatalog(opened) }
                                }
                            }
                        },
                    )
                    is CatalogScreenState.Recovery -> RecoveryScreen(
                        current.password,
                        current.warning,
                        onDone = {
                            val opened = CatalogSession(
                                GrantCatalog(vault.read().orEmpty(), apelido, openFolder),
                            )
                            session = opened
                            scope.launch {
                                screen = withContext(Dispatchers.IO) { openCatalog(opened) }
                            }
                        },
                    )
                    is CatalogScreenState.Holding -> {
                        LaunchedEffect(Unit) {
                            while (isActive) {
                                val shown = withContext(Dispatchers.IO) {
                                    runCatching { NativeCatalog.holdPoll() }.getOrNull()
                                }
                                val now = screen
                                if (now is CatalogScreenState.Holding && !now.busy) {
                                    if (shown == null) {
                                        screen = CatalogScreenState.Ready(session?.rows ?: emptyList(), false)
                                        return@LaunchedEffect
                                    }
                                    val approval = approvalFrom(shown)
                                    if (approval != now.approval) {
                                        screen = now.copy(approval = approval)
                                    }
                                }
                                delay(500)
                            }
                        }
                        HoldScreen(
                            current.offer,
                            current.approval,
                            current.busy,
                            current.failed,
                            current.qr,
                            onApprove = { decideHolding(vault.read().orEmpty(), true, current, { screen = it }, scope, session) },
                            onDeny = { decideHolding(vault.read().orEmpty(), false, current, { screen = it }, scope, session) },
                            onBack = {
                                NativeCatalog.holdClose()
                                screen = CatalogScreenState.Ready(session?.rows ?: emptyList(), false)
                            },
                        )
                    }
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
                                        previous.apelido,
                                        false,
                                        true,
                                    )
                                } else {
                                    val filled = pairingAfterQr(
                                        previous.apelido,
                                        fields.address,
                                        fields.code,
                                        fields.fingerprint,
                                    )
                                    screen = CatalogScreenState.Pairing(
                                        filled.address,
                                        filled.code,
                                        filled.fingerprint,
                                        filled.apelido,
                                        filled.submit,
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
                                current.apelido,
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

internal const val PENDING_GRANT = "grant.pending.enc"

/** The folder card on the catalog. [name] is null until a folder is picked. */
internal data class FolderCard(val name: String?, val status: String?, val busy: Boolean)

internal const val FOLDER_SYNCING = "Sincronizando a pasta…"

internal const val FOLDER_READY_NOTE = "Toque para sincronizar agora. O app também sincroniza ao abrir."

internal const val FOLDER_SWITCH_NOTE =
    "A pasta atual fica como está. A nova recebe os arquivos que este aparelho sincroniza."

private sealed interface CatalogScreenState {
    data object Loading : CatalogScreenState

    data class Ready(val rows: List<CatalogRow>, val busy: Boolean) : CatalogScreenState

    data class File(
        val path: String,
        val rows: List<CatalogRow>,
        val busy: Boolean,
        val localDeviceId: String,
    ) : CatalogScreenState

    data object Failed : CatalogScreenState

    data class Pairing(
        val address: String,
        val code: String,
        val fingerprint: String,
        val apelido: String,
        val busy: Boolean,
        val failed: Boolean,
    ) : CatalogScreenState

    data class Scanning(
        val address: String,
        val code: String,
        val fingerprint: String,
        val apelido: String,
    ) : CatalogScreenState

    data class Creating(val form: CreateForm) : CatalogScreenState

    data class Recovery(val password: String, val warning: String?) : CatalogScreenState

    data class ChangingRecovery(val busy: Boolean, val failed: Boolean) : CatalogScreenState

    data class Holding(
        val offer: HoldOffer,
        val approval: HoldApproval?,
        val busy: Boolean,
        val failed: Boolean,
        val qr: ByteArray?,
    ) : CatalogScreenState
}

private fun startHolding(grantPath: String): CatalogScreenState.Holding? {
    try {
        val fields = JsonCursor(NativeCatalog.holdStart(grantPath, "")).parseMixedObject()
        val offer = HoldOffer(
            fields.getValue("code"),
            fields.getValue("address"),
            fields.getValue("fingerprint"),
            fields.getValue("invite"),
        )
        val qr = try {
            NativeCatalog.invitePng(offer.invite)
        } catch (_: Exception) {
            null
        }
        return CatalogScreenState.Holding(offer, null, false, false, qr)
    } catch (_: Exception) {
        return null
    }
}

internal fun approvalFrom(text: String): HoldApproval? {
    if (text.isEmpty()) {
        return null
    }
    val fields = JsonCursor(text).parseMixedObject()
    return HoldApproval(
        fields.getValue("deviceName"),
        fields.getValue("os"),
        fields.getValue("peerAddress"),
        fields.getValue("code"),
    )
}

private fun decideHolding(
    grantPath: String,
    approve: Boolean,
    current: CatalogScreenState.Holding,
    show: (CatalogScreenState) -> Unit,
    scope: kotlinx.coroutines.CoroutineScope,
    session: CatalogSession?,
) {
    if (current.busy) {
        return
    }
    show(current.copy(busy = true, failed = false))
    scope.launch {
        val done = withContext(Dispatchers.IO) {
            try {
                NativeCatalog.holdDecide(grantPath, approve)
                true
            } catch (_: Exception) {
                false
            }
        }
        if (done) {
            show(CatalogScreenState.Ready(session?.rows ?: emptyList(), false))
        } else {
            show(current.copy(busy = false, failed = true))
        }
    }
}

internal const val EXTRA_FIXTURE = "satchel.fixture"

internal const val EXTRA_STORE = "satchel.store"

internal const val EXTRA_PAIR = "satchel.pair"

internal fun sourceFor(
    fixture: Boolean,
    sampleRoot: String,
    storePath: String?,
    grantPath: String?,
    apelido: () -> String = { "" },
    folder: () -> PhoneFolder? = { null },
): CatalogSource {
    if (fixture) {
        return FixtureCatalog()
    }
    if (!storePath.isNullOrEmpty()) {
        return SharedCatalog(storePath, apelido)
    }
    if (!grantPath.isNullOrEmpty()) {
        return GrantCatalog(grantPath, apelido, folder)
    }
    return CoreCatalog(sampleRoot, apelido)
}

private fun joinGrant(
    filesDir: File,
    address: String,
    code: String,
    fingerprint: String,
    apelido: String,
    folder: () -> PhoneFolder?,
): CatalogSession? {
    try {
        val deviceId = readOrCreateDeviceId(filesDir)
        val work = File(filesDir, "work")
        val raw = NativeCatalog.joinPairing(
            address,
            code,
            fingerprint,
            apelido,
            "Android",
            deviceId,
        )
        val text = assembleGrant(raw, deviceId, work.absolutePath)
        SecureGrant(filesDir).write(text)
        return CatalogSession(GrantCatalog(text, { readDeviceLabel(filesDir) }, folder))
    } catch (_: Exception) {
        return null
    }
}

internal fun readDeviceLabel(filesDir: File): String {
    val file = File(filesDir, "device-label")
    if (!file.isFile) {
        return ""
    }
    return file.readText()
}

internal fun writeDeviceLabel(filesDir: File, label: String) {
    File(filesDir, "device-label").writeText(label)
}

/** Reads a device id that is already on disk. Does not create the file. */
internal fun readStoredDeviceId(filesDir: File): String? {
    val file = File(filesDir, "device-id")
    if (!file.isFile) {
        return null
    }
    val existing = file.readText().trim()
    if (existing.length != 32) {
        return null
    }
    return existing
}

internal fun readOrCreateDeviceId(filesDir: File): String {
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
internal fun PhoneScaffold(
    title: String,
    onBack: (() -> Unit)?,
    backLabel: String = "Voltar",
    backEnabled: Boolean = true,
    content: @androidx.compose.runtime.Composable (PaddingValues) -> Unit,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    if (onBack != null) {
                        TextButton(onClick = onBack, enabled = backEnabled) {
                            Text(backLabel)
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
    canCreate: Boolean,
    onCreate: () -> Unit,
    canInvite: Boolean,
    onChangeRecovery: () -> Unit,
    folder: FolderCard?,
    onChooseFolder: () -> Unit,
    onSyncFolder: () -> Unit,
    onInvite: () -> Unit,
    onPair: () -> Unit,
    onOpen: (String) -> Unit,
) {
    val zone = ZoneId.systemDefault()
    PhoneScaffold(title = "Catálogo", onBack = null) { padding ->
        LazyColumn(modifier = Modifier.fillMaxSize().padding(padding)) {
            if (canCreate) {
                item {
                    ListItem(
                        headlineContent = { Text(CREATE_TITLE) },
                        supportingContent = { Text(CREATE_NOTE) },
                        modifier = Modifier.clickable(onClick = onCreate),
                    )
                }
            }
            if (canInvite) {
                item {
                    ListItem(
                        headlineContent = { Text("Mostrar convite") },
                        supportingContent = { Text(INVITE_NOTE) },
                        modifier = Modifier.clickable(onClick = onInvite),
                    )
                }
                item {
                    ListItem(
                        headlineContent = { Text(RECOVERY_CHANGE_TITLE) },
                        supportingContent = { Text("Só este aparelho, que criou o repositório, pode trocar.") },
                        modifier = Modifier.clickable(onClick = onChangeRecovery),
                    )
                }
            }
            if (folder != null) {
                item {
                    if (folder.name == null) {
                        ListItem(
                            headlineContent = { Text(FOLDER_TITLE) },
                            supportingContent = { Text(FOLDER_CHOOSE_NOTE) },
                            trailingContent = { Text("Escolher") },
                            modifier = Modifier.clickable(onClick = onChooseFolder),
                        )
                    } else {
                        ListItem(
                            headlineContent = { Text("$FOLDER_TITLE: ${folder.name}") },
                            supportingContent = { Text(folder.status ?: FOLDER_READY_NOTE) },
                            trailingContent = {
                                Text(if (folder.busy) "Sincronizando" else "Sincronizar")
                            },
                            modifier = Modifier.clickable(enabled = !folder.busy, onClick = onSyncFolder),
                        )
                    }
                }
                if (folder.name != null) {
                    item {
                        ListItem(
                            headlineContent = { Text("Escolher outra pasta") },
                            supportingContent = { Text(FOLDER_SWITCH_NOTE) },
                            modifier = Modifier.clickable(enabled = !folder.busy, onClick = onChooseFolder),
                        )
                    }
                }
            }
            if (canPair) {
                item {
                    ListItem(
                        headlineContent = { Text(ENTRY_TITLE) },
                        supportingContent = { Text(PAIR_SUPPORT) },
                        modifier = Modifier.clickable(onClick = onPair),
                    )
                }
            }
            if (rows.isEmpty()) {
                item {
                    Text(
                        EMPTY_CATALOG,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
            items(rows, key = { row -> row.path }) { row ->
                ListItem(
                    headlineContent = { Text(rowTitle(row), style = MaterialTheme.typography.titleMedium) },
                    supportingContent = {
                        Text("${catalogMeta(row, zone)}\n${deviceCountLabel(row.devices)}")
                    },
                    trailingContent = {
                        Text(followLabel(row.follow), color = MaterialTheme.colorScheme.onSurfaceVariant)
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
    val zone = ZoneId.systemDefault()
    PhoneScaffold(
        title = state.path,
        onBack = onBack,
        backLabel = "Catálogo",
        backEnabled = !state.busy,
    ) { padding ->
        LazyColumn(modifier = Modifier.fillMaxSize().padding(padding)) {
            if (row == null) {
                item {
                    Text(EMPTY_CATALOG, modifier = Modifier.padding(16.dp))
                }
                return@LazyColumn
            }
            val header = fileHeaderLines(row, zone)
            item {
                ListItem(
                    overlineContent = { Text(header[0]) },
                    headlineContent = { Text(header[1]) },
                )
            }
            item {
                ListItem(
                    overlineContent = { Text(header[2]) },
                    headlineContent = { Text(header[3]) },
                )
            }
            item {
                Text(
                    "Neste aparelho",
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 8.dp),
                )
            }
            if (state.busy) {
                item {
                    Text(SAVING_COPY, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
                }
            }
            items(Follow.entries, key = { follow -> follow.name }) { follow ->
                val chosen = row.follow == follow
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .semantics { selected = chosen }
                        .clickable(enabled = !state.busy) { onChoose(follow) }
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(followLabel(follow))
                        Text(
                            followDetail(follow, row.isDirectory),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    if (chosen) {
                        Text("✓")
                    }
                }
            }
            item {
                Text(
                    "Aparelhos",
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 8.dp),
                )
            }
            val cards = deviceSectionLines(row, state.localDeviceId, zone)
            items(cards.size) { index ->
                Text(
                    cards[index],
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp),
                )
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
    onApelido: (String) -> Unit,
    onScan: () -> Unit,
    onBack: () -> Unit,
    onSubmit: () -> Unit,
) {
    val plain = KeyboardOptions(
        capitalization = KeyboardCapitalization.None,
        autoCorrectEnabled = false,
    )
    PhoneScaffold(title = ENTRY_TITLE, onBack = onBack, backEnabled = !state.busy) { padding ->
        Column(modifier = Modifier.fillMaxSize().padding(padding).padding(16.dp)) {
        Text(ENTRY_NOTE)
        OutlinedTextField(
            value = state.address,
            onValueChange = onAddress,
            enabled = !state.busy,
            label = { Text(ENTRY_FIELDS[0]) },
            keyboardOptions = plain,
            modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
        )
        OutlinedTextField(
            value = state.code,
            onValueChange = onCode,
            enabled = !state.busy,
            label = { Text(ENTRY_FIELDS[1]) },
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        OutlinedTextField(
            value = state.fingerprint,
            onValueChange = onFingerprint,
            enabled = !state.busy,
            label = { Text(ENTRY_FIELDS[2]) },
            keyboardOptions = plain,
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        OutlinedTextField(
            value = state.apelido,
            onValueChange = onApelido,
            enabled = !state.busy,
            label = { Text(ENTRY_FIELDS[3]) },
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
