package io.github.lucaswilliameufrasio.satchel

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.lucaswilliameufrasio.satchel.ui.CardText
import io.github.lucaswilliameufrasio.satchel.ui.PrimaryButton
import io.github.lucaswilliameufrasio.satchel.ui.ProductCard
import io.github.lucaswilliameufrasio.satchel.ui.satchel
import com.google.zxing.BinaryBitmap
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/** What the scanner can do right now. */
private enum class CameraAccess { Missing, Ask, Refused, Blocked, Granted }

/**
 * Rear-camera QR reader. The first invite payload is delivered once.
 *
 * Without a camera, or with the permission refused, the screen says so and
 * points to the way out: typing the code, or the app's settings.
 */
@Composable
internal fun QrScanner(onCode: (String) -> Unit, onClose: () -> Unit) {
    val context = LocalContext.current
    val activity = context as? Activity
    val hasCamera = remember {
        context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)
    }
    var access by remember {
        mutableStateOf(
            when {
                !hasCamera -> CameraAccess.Missing
                ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                    PackageManager.PERMISSION_GRANTED -> CameraAccess.Granted
                else -> CameraAccess.Ask
            },
        )
    }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        access = when {
            allowed -> CameraAccess.Granted
            // After a refusal Android stops asking once the rationale is no longer offered.
            activity != null &&
                !ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.CAMERA) ->
                CameraAccess.Blocked
            else -> CameraAccess.Refused
        }
    }
    PhoneScaffold(title = stringResource(R.string.scan_qr), onBack = onClose) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when (access) {
                CameraAccess.Granted -> {
                    Text(
                        stringResource(R.string.qr_point),
                        style = MaterialTheme.typography.bodyMedium,
                        color = satchel.secondary,
                    )
                    val viewfinder = stringResource(R.string.qr_viewfinder)
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .aspectRatio(1f)
                            .clip(RoundedCornerShape(18.dp))
                            .semantics { contentDescription = viewfinder },
                    ) {
                        CameraPreview(onCode)
                    }
                }
                CameraAccess.Missing -> ProductCard {
                    CardText(stringResource(R.string.qr_no_camera_title), stringResource(R.string.qr_no_camera))
                }
                CameraAccess.Ask, CameraAccess.Refused -> {
                    ProductCard {
                        CardText(
                            stringResource(R.string.qr_camera_title),
                            stringResource(
                                if (access == CameraAccess.Ask) R.string.qr_camera_why else R.string.qr_camera_refused,
                            ),
                        )
                    }
                    PrimaryButton(
                        text = stringResource(R.string.qr_allow_camera),
                        onClick = { request.launch(Manifest.permission.CAMERA) },
                    )
                }
                CameraAccess.Blocked -> {
                    ProductCard {
                        CardText(stringResource(R.string.qr_camera_title), stringResource(R.string.qr_camera_blocked))
                    }
                    PrimaryButton(
                        text = stringResource(R.string.qr_open_settings),
                        onClick = {
                            context.startActivity(
                                Intent(
                                    Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                    Uri.fromParts("package", context.packageName, null),
                                ),
                            )
                        },
                    )
                }
            }
            TextButton(onClick = onClose, contentPadding = PaddingValues(0.dp)) {
                Text(stringResource(R.string.qr_type_instead), color = satchel.accent)
            }
        }
    }
    // Coming back from the settings screen with the permission now on.
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME &&
                hasCamera &&
                access != CameraAccess.Granted &&
                ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
            ) {
                access = CameraAccess.Granted
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }
}

@Composable
private fun CameraPreview(onCode: (String) -> Unit) {
    val lifecycleOwner = LocalLifecycleOwner.current
    val delivered = remember { AtomicBoolean(false) }
    val executor = remember { Executors.newSingleThreadExecutor() }
    DisposableEffect(lifecycleOwner) {
        onDispose {
            executor.shutdown()
        }
    }
    AndroidView(
        modifier = Modifier.fillMaxSize(),
        factory = { viewContext ->
            val previewView = PreviewView(viewContext)
            val future = ProcessCameraProvider.getInstance(viewContext)
            future.addListener(
                {
                    val provider = future.get()
                    val preview = Preview.Builder().build()
                    preview.surfaceProvider = previewView.surfaceProvider
                    val analysis = ImageAnalysis.Builder()
                        .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                        .build()
                    analysis.setAnalyzer(executor) { image ->
                        readQr(image, delivered, onCode)
                    }
                    provider.unbindAll()
                    provider.bindToLifecycle(
                        lifecycleOwner,
                        CameraSelector.DEFAULT_BACK_CAMERA,
                        preview,
                        analysis,
                    )
                },
                ContextCompat.getMainExecutor(viewContext),
            )
            previewView
        },
    )
}

private fun readQr(image: ImageProxy, delivered: AtomicBoolean, onCode: (String) -> Unit) {
    try {
        if (delivered.get()) {
            return
        }
        val text = decodeQr(image)
        if (text != null && delivered.compareAndSet(false, true)) {
            onCode(text)
        }
    } finally {
        image.close()
    }
}

private fun decodeQr(image: ImageProxy): String? {
    val plane = image.planes[0]
    val buffer = plane.buffer
    val raw = ByteArray(buffer.remaining())
    buffer.get(raw)
    val packed = packLuma(raw, image.width, image.height, plane.rowStride)
    val rotated = rotateLuma(packed, image.width, image.height, image.imageInfo.rotationDegrees)
    val source = PlanarYUVLuminanceSource(
        rotated.bytes,
        rotated.width,
        rotated.height,
        0,
        0,
        rotated.width,
        rotated.height,
        false,
    )
    val bitmap = BinaryBitmap(HybridBinarizer(source))
    return try {
        QRCodeReader().decode(bitmap).text
    } catch (_: Exception) {
        null
    }
}

private class Luma(val bytes: ByteArray, val width: Int, val height: Int)

private fun packLuma(raw: ByteArray, width: Int, height: Int, rowStride: Int): ByteArray {
    val packed = ByteArray(width * height)
    var offset = 0
    for (row in 0 until height) {
        val start = row * rowStride
        for (column in 0 until width) {
            packed[offset] = raw[start + column]
            offset += 1
        }
    }
    return packed
}

private fun rotateLuma(source: ByteArray, width: Int, height: Int, rotation: Int): Luma {
    val normalized = ((rotation % 360) + 360) % 360
    if (normalized == 0) {
        return Luma(source, width, height)
    }
    if (normalized == 180) {
        val out = ByteArray(source.size)
        var index = 0
        for (pixel in source.size - 1 downTo 0) {
            out[index] = source[pixel]
            index += 1
        }
        return Luma(out, width, height)
    }
    val out = ByteArray(source.size)
    var index = 0
    if (normalized == 90) {
        for (column in 0 until width) {
            for (row in height - 1 downTo 0) {
                out[index] = source[row * width + column]
                index += 1
            }
        }
        return Luma(out, height, width)
    }
    for (column in width - 1 downTo 0) {
        for (row in 0 until height) {
            out[index] = source[row * width + column]
            index += 1
        }
    }
    return Luma(out, height, width)
}
