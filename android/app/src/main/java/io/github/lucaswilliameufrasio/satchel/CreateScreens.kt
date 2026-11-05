package io.github.lucaswilliameufrasio.satchel

import io.github.lucaswilliameufrasio.satchel.ui.satchel
import io.github.lucaswilliameufrasio.satchel.ui.ProductCard
import io.github.lucaswilliameufrasio.satchel.ui.PrimaryButton
import io.github.lucaswilliameufrasio.satchel.ui.FailureText
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.foundation.layout.Arrangement
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.BitmapFactory
import android.os.PersistableBundle
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp

/** What the holder screen shows while it waits. */
internal data class HoldOffer(
    val code: String,
    val address: String,
    val fingerprint: String,
    val invite: String,
)

/** The other device, after it passed the code check. */
internal data class HoldApproval(
    val deviceName: String,
    val os: String,
    val peerAddress: String,
    val code: String,
)

@Composable
internal fun CreateScreen(
    form: CreateForm,
    onForm: (CreateForm) -> Unit,
    onBack: () -> Unit,
    onSubmit: () -> Unit,
) {
    val plain = KeyboardOptions(
        capitalization = KeyboardCapitalization.None,
        autoCorrectEnabled = false,
    )
    val enabled = !form.busy
    PhoneScaffold(title = CREATE_TITLE, onBack = onBack, backEnabled = enabled) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            Text(CREATE_NOTE, color = satchel.secondary)
            Row(
                modifier = Modifier.padding(top = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (choice in StorageChoice.entries) {
                    val chosen = form.storage == choice
                    FilterChip(
                        selected = chosen,
                        onClick = { onForm(form.copy(storage = choice, failed = false)) },
                        enabled = enabled,
                        label = { Text((if (chosen) "● " else "") + choice.label) },
                        colors = FilterChipDefaults.filterChipColors(
                            selectedContainerColor = satchel.selected,
                            selectedLabelColor = satchel.ink,
                        ),
                        modifier = Modifier.semantics { selected = chosen },
                    )
                }
            }
            if (form.storage == StorageChoice.Folder) {
                Text(CREATE_FOLDER_NOTE, color = satchel.secondary, modifier = Modifier.padding(top = 8.dp))
            } else {
                Text(CREATE_OBJECT_NOTE, color = satchel.secondary, modifier = Modifier.padding(top = 8.dp))
                Field("Endpoint", form.endpoint, enabled, plain) { onForm(form.copy(endpoint = it, failed = false)) }
                Field("Bucket", form.bucket, enabled, plain) { onForm(form.copy(bucket = it, failed = false)) }
                Field("Região", form.region, enabled, plain) { onForm(form.copy(region = it, failed = false)) }
                Field("Prefixo (pode ficar vazio)", form.prefix, enabled, plain) { onForm(form.copy(prefix = it, failed = false)) }
                Field("Chave de acesso de manutenção", form.accessKeyId, enabled, plain) { onForm(form.copy(accessKeyId = it, failed = false)) }
                Field("Segredo de manutenção", form.secretAccessKey, enabled, plain, secret = true) { onForm(form.copy(secretAccessKey = it, failed = false)) }
                Field("Chave de acesso dos aparelhos", form.deviceAccessKeyId, enabled, plain) { onForm(form.copy(deviceAccessKeyId = it, failed = false)) }
                Field("Segredo dos aparelhos", form.deviceSecretAccessKey, enabled, plain, secret = true) { onForm(form.copy(deviceSecretAccessKey = it, failed = false)) }
            }
            if (form.failed) {
                FailureText(createFailureCopy(form.reason), modifier = Modifier.padding(top = 12.dp))
            }
            PrimaryButton(
                if (form.busy) "Criando…" else "Criar",
                onClick = onSubmit,
                enabled = enabled && createFormComplete(form),
                modifier = Modifier.padding(top = 16.dp, bottom = 32.dp),
            )
        }
    }
}

@Composable
private fun Field(
    label: String,
    value: String,
    enabled: Boolean,
    keyboard: KeyboardOptions,
    secret: Boolean = false,
    onValue: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValue,
        enabled = enabled,
        label = { Text(label) },
        keyboardOptions = keyboard,
        visualTransformation = if (secret) PasswordVisualTransformation() else androidx.compose.ui.text.input.VisualTransformation.None,
        modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
    )
}

@Composable
internal fun RecoveryScreen(password: String, warning: String?, onDone: () -> Unit) {
    // Leaving this screen any other way would lose the only copy the person was shown.
    BackHandler(enabled = true) {}
    val context = LocalContext.current
    PhoneScaffold(title = RECOVERY_TITLE, onBack = null) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(RECOVERY_NOTE, color = satchel.secondary)
            if (warning != null) {
                FailureText(warning)
            }
            ProductCard {
                Text(
                    password,
                    fontFamily = FontFamily.Monospace,
                    style = MaterialTheme.typography.titleMedium,
                )
            }
            TextButton(onClick = { copySensitive(context, password) }) {
                Text("Copiar")
            }
            PrimaryButton("Guardei", onClick = onDone, modifier = Modifier.padding(bottom = 32.dp))
        }
    }
}

@Composable
internal fun HoldScreen(
    offer: HoldOffer,
    approval: HoldApproval?,
    busy: Boolean,
    failed: Boolean,
    qr: ByteArray?,
    onApprove: () -> Unit,
    onDeny: () -> Unit,
    onBack: () -> Unit,
) {
    PhoneScaffold(title = INVITE_TITLE, onBack = onBack, backEnabled = !busy) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            if (approval == null) {
                Text(INVITE_WAITING_COPY, color = satchel.secondary)
                Text("Código ${offer.code}", modifier = Modifier.padding(top = 12.dp))
                Text("Endereço ${offer.address}")
                Text("Impressão digital ${offer.fingerprint}")
                val bitmap = qr?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
                if (bitmap != null) {
                    Image(
                        bitmap = bitmap.asImageBitmap(),
                        contentDescription = "QR do convite",
                        modifier = Modifier.size(240.dp).padding(top = 12.dp),
                    )
                }
                Text(INVITE_NOTE, modifier = Modifier.padding(top = 12.dp))
            } else {
                Text("Aparelho ${approval.deviceName}")
                Text("Sistema ${approval.os}")
                Text("Endereço ${approval.peerAddress}")
                Text("Código ${approval.code}")
                if (failed) {
                    FailureText("Não foi possível aprovar. Feche o convite e abra de novo.", modifier = Modifier.padding(top = 12.dp))
                }
                PrimaryButton("Aprovar", onClick = onApprove, enabled = !busy, modifier = Modifier.padding(top = 16.dp))
                TextButton(onClick = onDeny, enabled = !busy) {
                    Text("Negar")
                }
            }
        }
    }
}

/** Copy [text] and mark the clip sensitive so the system hides it in previews. */
private fun copySensitive(context: Context, text: String) {
    val manager = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    val clip = ClipData.newPlainText("senha de recuperação", text)
    clip.description.extras = PersistableBundle().apply {
        putBoolean("android.content.extra.IS_SENSITIVE", true)
    }
    manager.setPrimaryClip(clip)
}

@Composable
internal fun RecoveryChangeScreen(busy: Boolean, failed: Boolean, onChange: () -> Unit, onBack: () -> Unit) {
    PhoneScaffold(title = RECOVERY_CHANGE_TITLE, onBack = onBack, backEnabled = !busy) { padding ->
        Column(modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp)) {
            Text(RECOVERY_CHANGE_NOTE, color = satchel.secondary)
            if (failed) {
                FailureText(RECOVERY_CHANGE_FAILED, modifier = Modifier.padding(top = 12.dp))
            }
            PrimaryButton(
                if (busy) "Trocando…" else "Trocar agora",
                onClick = onChange,
                enabled = !busy,
                modifier = Modifier.padding(top = 16.dp),
            )
        }
    }
}
