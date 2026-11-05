package io.github.lucaswilliameufrasio.satchel

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BinaryBitmap
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Rear-camera QR reader. The first invite payload is delivered once.
 */
@Composable
internal fun QrScanner(onCode: (String) -> Unit, onClose: () -> Unit) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED,
        )
    }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        granted = allowed
    }
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Text(stringResource(R.string.qr_point))
        if (!granted) {
            Button(
                onClick = { request.launch(Manifest.permission.CAMERA) },
                modifier = Modifier.padding(top = 16.dp),
            ) {
                Text(stringResource(R.string.qr_allow_camera))
            }
        } else {
            CameraPreview(onCode)
        }
        Button(onClick = onClose, modifier = Modifier.padding(top = 16.dp)) {
            Text(stringResource(R.string.back))
        }
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
        modifier = Modifier.fillMaxSize().padding(top = 16.dp),
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
