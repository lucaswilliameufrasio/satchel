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
import androidx.compose.ui.res.stringResource
import io.github.lucaswilliameufrasio.satchel.catalog.holderFor
import io.github.lucaswilliameufrasio.satchel.catalog.deviceCardLines
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.background
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.Alignment
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogRow
import io.github.lucaswilliameufrasio.satchel.ui.BrandMark
import io.github.lucaswilliameufrasio.satchel.ui.CardText
import io.github.lucaswilliameufrasio.satchel.ui.FailureText
import io.github.lucaswilliameufrasio.satchel.ui.PrimaryButton
import io.github.lucaswilliameufrasio.satchel.ui.ProductTextField
import io.github.lucaswilliameufrasio.satchel.ui.ProductCard
import io.github.lucaswilliameufrasio.satchel.ui.SatchelTheme
import io.github.lucaswilliameufrasio.satchel.ui.ScreenTitle
import io.github.lucaswilliameufrasio.satchel.ui.satchel
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogSession
import io.github.lucaswilliameufrasio.satchel.catalog.CatalogSource
import io.github.lucaswilliameufrasio.satchel.catalog.CoreCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.ENTRY_FIELDS
import io.github.lucaswilliameufrasio.satchel.catalog.Follow
import io.github.lucaswilliameufrasio.satchel.catalog.GrantCatalog
import io.github.lucaswilliameufrasio.satchel.catalog.PhoneFolder
import io.github.lucaswilliameufrasio.satchel.catalog.NativeCatalog
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
            SatchelTheme {
                val words = rememberWords()
                var phoneId by remember { mutableStateOf(initialPhoneId) }
                var screen by remember {
                    mutableStateOf<CatalogScreenState>(
                        if (showPair) {
                            CatalogScreenState.Pairing(
                                "",
                                "",
                                "",
                                openingApelido(apelido(), android.os.Build.MODEL),
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
                val folderName = remember(folderUri, words) {
                    folderUri?.let { folderDisplayName(contentResolver, it, words.text(R.string.folder)) }
                }
                val runFolder: () -> Unit = runFolder@{
                    val active = session
                    if (folderBusy || active == null || !hasGrant || !ordinary) {
                        return@runFolder
                    }
                    folderBusy = true
                    folderStatus = words.text(R.string.folder_syncing)
                    scope.launch {
                        val (summary, next) = withContext(Dispatchers.IO) {
                            synchronized(active) {
                                val said = folderRound(vault.read().orEmpty(), openFolder(), apelido(), words)
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
                val lifecycle = LocalLifecycleOwner.current.lifecycle
                LaunchedEffect(hasGrant) {
                    if (!ordinary || !hasGrant) {
                        return@LaunchedEffect
                    }
                    lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                        listenForWakes(wakeStateDir(filesDir)) {
                            val active = session
                            if (folderUri != null) {
                                runFolder()
                            } else if (active != null && screen is CatalogScreenState.Ready) {
                                val next = withContext(Dispatchers.IO) { openCatalog(active) }
                                if (screen is CatalogScreenState.Ready && next is CatalogScreenState.Ready) {
                                    screen = next
                                }
                            }
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
                            folderStatus = words.text(R.string.folder_lost)
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
                    CatalogScreenState.Loading -> StatusText(stringResource(R.string.catalog_loading))
                    CatalogScreenState.Failed -> StatusText(stringResource(R.string.catalog_open_failed)) {
                        val opened = session
                        if (opened != null) {
                            screen = CatalogScreenState.Loading
                            scope.launch {
                                screen = withContext(Dispatchers.IO) { openCatalog(opened) }
                            }
                        }
                    }
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
                                openingApelido(apelido(), android.os.Build.MODEL),
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
                                    !result.stored -> R.string.vault_failed
                                    form.storage == StorageChoice.Folder -> R.string.create_folder_note
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
                                    CatalogScreenState.Recovery(fresh, R.string.recovery_changed_note)
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

    data class Recovery(val password: String, @androidx.annotation.StringRes val warning: Int?) : CatalogScreenState

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

/**
 * The screen frame: canvas background, an optional back link, the mark on the
 * catalog only, and the title as a heading. The same layout as `ProductScreen` on iOS.
 */
@androidx.compose.runtime.Composable
internal fun PhoneScaffold(
    title: String,
    onBack: (() -> Unit)?,
    backLabel: String? = null,
    backEnabled: Boolean = true,
    showMark: Boolean = false,
    content: @androidx.compose.runtime.Composable (PaddingValues) -> Unit,
) {
    // The system back gesture does what the Back button on screen does, instead of closing the
    // app. While the screen is busy it does nothing, like the disabled button.
    if (onBack != null) {
        androidx.activity.compose.BackHandler(enabled = true) {
            if (backEnabled) {
                onBack()
            }
        }
    }
    Scaffold(
        containerColor = satchel.canvas,
        topBar = {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .statusBarsPadding()
                    .padding(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 12.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (onBack != null) {
                    TextButton(
                        onClick = onBack,
                        enabled = backEnabled,
                        contentPadding = PaddingValues(0.dp),
                    ) {
                        Text(backLabel ?: stringResource(R.string.back), color = if (backEnabled) satchel.accent else satchel.secondary)
                    }
                }
                if (showMark) {
                    BrandMark()
                }
                ScreenTitle(title)
            }
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
    val words = rememberWords()
    PhoneScaffold(title = stringResource(R.string.catalog_title), onBack = null, showMark = true) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (canCreate) {
                item {
                    ProductCard(onClick = onCreate) { CardText(stringResource(R.string.create_title), stringResource(R.string.create_note)) }
                }
            }
            if (canInvite) {
                item {
                    ProductCard(onClick = onInvite) { CardText(stringResource(R.string.show_invite), stringResource(R.string.invite_note)) }
                }
                item {
                    ProductCard(onClick = onChangeRecovery) {
                        CardText(
                            stringResource(R.string.recovery_change_title),
                            stringResource(R.string.recovery_change_only_here),
                        )
                    }
                }
            }
            if (folder != null) {
                item {
                    if (folder.name == null) {
                        ProductCard(onClick = onChooseFolder) {
                            CardText(stringResource(R.string.folder_title), stringResource(R.string.folder_choose_note))
                            Text(stringResource(R.string.folder_choose), color = satchel.accent, fontWeight = FontWeight.SemiBold)
                        }
                    } else {
                        ProductCard(onClick = onSyncFolder, enabled = !folder.busy) {
                            CardText(
                                stringResource(R.string.folder_card_title, folder.name),
                                folder.status ?: stringResource(R.string.folder_ready_note),
                            )
                            Text(
                                stringResource(if (folder.busy) R.string.folder_syncing_short else R.string.folder_sync),
                                color = if (folder.busy) satchel.secondary else satchel.accent,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }
                if (folder.name != null) {
                    item {
                        ProductCard(onClick = onChooseFolder, enabled = !folder.busy) {
                            CardText(stringResource(R.string.folder_choose_other), stringResource(R.string.folder_switch_note))
                        }
                    }
                }
            }
            if (canPair) {
                item {
                    ProductCard(onClick = onPair) { CardText(stringResource(R.string.entry_title), stringResource(R.string.pair_support)) }
                }
            }
            if (rows.isEmpty()) {
                item {
                    ProductCard { Text(stringResource(R.string.catalog_empty), color = satchel.secondary) }
                }
            }
            items(rows, key = { row -> row.path }) { row ->
                ProductCard(onClick = { onOpen(row.path) }) {
                    Row(verticalAlignment = Alignment.Top) {
                        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(rowTitle(row), style = MaterialTheme.typography.titleMedium)
                            Text(catalogMeta(row, zone, words), style = MaterialTheme.typography.bodyMedium, color = satchel.secondary)
                            Text(deviceCountLabel(row.devices, words), style = MaterialTheme.typography.bodyMedium, color = satchel.secondary)
                        }
                        Text(
                            followLabel(row.follow, words),
                            color = satchel.accent,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.padding(start = 12.dp),
                        )
                    }
                }
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
    val words = rememberWords()
    PhoneScaffold(
        title = state.path,
        onBack = onBack,
        backLabel = stringResource(R.string.catalog_title),
        backEnabled = !state.busy,
    ) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (row == null) {
                item {
                    ProductCard { Text(stringResource(R.string.catalog_empty), color = satchel.secondary) }
                }
                return@LazyColumn
            }
            val header = fileHeaderLines(row, zone, words)
            item {
                ProductCard {
                    Text(header[0], style = MaterialTheme.typography.labelMedium, color = satchel.secondary)
                    Text(header[1])
                    Text(
                        header[2],
                        style = MaterialTheme.typography.labelMedium,
                        color = satchel.secondary,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                    Text(header[3])
                }
            }
            item {
                SectionLabel(stringResource(R.string.section_this_device))
            }
            if (state.busy) {
                item {
                    Text(stringResource(R.string.saving_choice), color = satchel.secondary)
                }
            }
            item {
                ProductCard {
                    for (follow in Follow.entries) {
                        val chosen = row.follow == follow
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(12.dp))
                                .background(if (chosen) satchel.selected else Color.Transparent)
                                .semantics { selected = chosen }
                                .clickable(enabled = !state.busy) { onChoose(follow) }
                                .heightIn(min = 48.dp)
                                .padding(horizontal = 12.dp, vertical = 10.dp),
                        ) {
                            // The mark sits on the title line so the detail keeps the full width at large text.
                            Column(modifier = Modifier.weight(1f)) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(
                                        followLabel(follow, words),
                                        fontWeight = FontWeight.SemiBold,
                                        modifier = Modifier.weight(1f),
                                    )
                                    if (chosen) {
                                        Text("✓", color = satchel.accent, fontWeight = FontWeight.SemiBold)
                                    }
                                }
                                Text(
                                    followDetail(follow, row.isDirectory, words),
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = satchel.secondary,
                                )
                            }
                        }
                    }
                }
            }
            item {
                SectionLabel(stringResource(R.string.section_devices))
            }
            if (row.devices.isEmpty()) {
                item {
                    ProductCard { Text(deviceSectionLines(row, state.localDeviceId, zone, words)[0], color = satchel.secondary) }
                }
            }
            items(row.devices, key = { device -> device }) { device ->
                val local = device == state.localDeviceId && state.localDeviceId.isNotEmpty()
                val lines = deviceCardLines(device, holderFor(device, row.holders), local, zone, words)
                ProductCard {
                    lines.forEachIndexed { index, line ->
                        when {
                            index == 0 -> Text(line, style = MaterialTheme.typography.titleMedium)
                            GROUPED_ID.matches(line) -> Text(
                                line,
                                fontFamily = FontFamily.Monospace,
                                style = MaterialTheme.typography.bodyMedium,
                                color = satchel.secondary,
                            )
                            else -> Text(line, style = MaterialTheme.typography.bodyMedium, color = satchel.secondary)
                        }
                    }
                }
            }
        }
    }
}

/** A device id shown in groups of four hex characters. */
private val GROUPED_ID = Regex("^([0-9a-f]{4} ){7}[0-9a-f]{4}$")

@androidx.compose.runtime.Composable
private fun SectionLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelMedium,
        color = satchel.secondary,
        modifier = Modifier.padding(top = 8.dp).semantics { heading() },
    )
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
    PhoneScaffold(title = stringResource(R.string.entry_title), onBack = onBack, backEnabled = !state.busy) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp)
                .verticalScroll(rememberScrollState()),
        ) {
        Text(stringResource(R.string.entry_note), color = satchel.secondary)
        ProductTextField(
            value = state.address,
            onValueChange = onAddress,
            enabled = !state.busy,
            label = stringResource(ENTRY_FIELDS[0]),
            keyboardOptions = plain,
            modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
        )
        ProductTextField(
            value = state.code,
            onValueChange = onCode,
            enabled = !state.busy,
            label = stringResource(ENTRY_FIELDS[1]),
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        ProductTextField(
            value = state.fingerprint,
            onValueChange = onFingerprint,
            enabled = !state.busy,
            label = stringResource(ENTRY_FIELDS[2]),
            keyboardOptions = plain,
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        ProductTextField(
            value = state.apelido,
            onValueChange = onApelido,
            enabled = !state.busy,
            label = stringResource(ENTRY_FIELDS[3]),
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
        )
        if (state.failed) {
            FailureText(
                stringResource(R.string.join_failed),
                modifier = Modifier.padding(top = 12.dp),
            )
        }
        TextButton(
            onClick = onScan,
            enabled = !state.busy,
            modifier = Modifier.padding(top = 16.dp),
        ) {
            Text(stringResource(R.string.scan_qr))
        }
        PrimaryButton(
            stringResource(R.string.join),
            onClick = onSubmit,
            enabled = !state.busy,
            modifier = Modifier.padding(top = 8.dp, bottom = 32.dp),
        )
        }
    }
}

@androidx.compose.runtime.Composable
private fun StatusText(message: String, onRetry: (() -> Unit)? = null) {
    PhoneScaffold(title = stringResource(R.string.catalog_title), onBack = null, showMark = true) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp, vertical = 48.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            if (onRetry == null) {
                CircularProgressIndicator(color = satchel.secondary)
            }
            Text(
                message,
                color = if (onRetry == null) satchel.secondary else satchel.ink,
                textAlign = TextAlign.Center,
            )
            if (onRetry != null) {
                PrimaryButton(stringResource(R.string.try_again), onClick = onRetry)
            }
        }
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
