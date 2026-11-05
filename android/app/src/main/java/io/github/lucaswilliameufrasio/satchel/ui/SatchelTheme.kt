package io.github.lucaswilliameufrasio.satchel.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.lucaswilliameufrasio.satchel.R

/** The palette in docs/identidade-visual.md, the same names as `SatchelInk` on iOS. */
@Immutable
data class SatchelColors(
    val canvas: Color,
    val card: Color,
    val ink: Color,
    val secondary: Color,
    val accent: Color,
    val field: Color,
    val line: Color,
    val danger: Color,
    val selected: Color,
)

internal val LightSatchel = SatchelColors(
    canvas = Color(0xFFF3EEE4),
    card = Color(0xFFFFFCF7),
    ink = Color(0xFF241C16),
    secondary = Color(0xFF6B5E52),
    accent = Color(0xFF8C4A24),
    field = Color(0xFFEBE4D8),
    line = Color(0xFF241C16).copy(alpha = 0.14f),
    danger = Color(0xFFA3261B),
    selected = Color(0xFF8C4A24).copy(alpha = 0.14f),
)

internal val DarkSatchel = SatchelColors(
    canvas = Color(0xFF17130F),
    card = Color(0xFF221C17),
    ink = Color(0xFFF2ECE3),
    secondary = Color(0xFFB8AB9C),
    accent = Color(0xFFE0A070),
    field = Color(0xFF2D261F),
    line = Color(0xFFF2ECE3).copy(alpha = 0.14f),
    danger = Color(0xFFF08A7E),
    selected = Color(0xFFE0A070).copy(alpha = 0.18f),
)

internal val LocalSatchelColors = staticCompositionLocalOf { LightSatchel }

/** Colors of the current theme. */
internal val satchel: SatchelColors
    @Composable get() = LocalSatchelColors.current

private fun scheme(colors: SatchelColors, dark: Boolean) = if (dark) {
    darkColorScheme(
        primary = colors.accent,
        onPrimary = colors.card,
        background = colors.canvas,
        onBackground = colors.ink,
        surface = colors.canvas,
        onSurface = colors.ink,
        surfaceVariant = colors.field,
        onSurfaceVariant = colors.secondary,
        surfaceContainer = colors.card,
        surfaceContainerHigh = colors.card,
        error = colors.danger,
        outline = colors.line,
    )
} else {
    lightColorScheme(
        primary = colors.accent,
        onPrimary = colors.card,
        background = colors.canvas,
        onBackground = colors.ink,
        surface = colors.canvas,
        onSurface = colors.ink,
        surfaceVariant = colors.field,
        onSurfaceVariant = colors.secondary,
        surfaceContainer = colors.card,
        surfaceContainerHigh = colors.card,
        error = colors.danger,
        outline = colors.line,
    )
}

/** Follows the system's light or dark mode. Text sizes follow the system font size. */
@Composable
fun SatchelTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val colors = if (dark) DarkSatchel else LightSatchel
    val base = Typography()
    val typography = base.copy(
        headlineLarge = base.headlineLarge.copy(fontSize = 32.sp, fontWeight = FontWeight.SemiBold),
        titleMedium = base.titleMedium.copy(fontSize = 17.sp, fontWeight = FontWeight.SemiBold),
        bodyMedium = base.bodyMedium.copy(fontSize = 15.sp),
        labelMedium = base.labelMedium.copy(fontSize = 13.sp, fontWeight = FontWeight.SemiBold),
    )
    CompositionLocalProvider(LocalSatchelColors provides colors) {
        MaterialTheme(colorScheme = scheme(colors, dark), typography = typography, content = content)
    }
}

/** The bag on dark green and the name, at the top of the catalog only. */
@Composable
fun BrandMark() {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Image(
            painter = painterResource(R.drawable.satchel_mark),
            contentDescription = null,
            modifier = Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)),
        )
        Text("Satchel", style = MaterialTheme.typography.titleMedium, color = satchel.ink)
    }
}

/** Screen title. A heading for the screen reader. */
@Composable
fun ScreenTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.headlineLarge,
        color = satchel.ink,
        modifier = Modifier.semantics { heading() },
    )
}

/** Card with radius 18 and a hairline border. The whole card is the button when [onClick] is set. */
@Composable
fun ProductCard(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    enabled: Boolean = true,
    content: @Composable ColumnScope.() -> Unit,
) {
    val shape = RoundedCornerShape(18.dp)
    Surface(
        shape = shape,
        color = satchel.card,
        contentColor = satchel.ink,
        border = BorderStroke(1.dp, satchel.line),
        modifier = modifier.fillMaxWidth(),
    ) {
        val clickable = if (onClick != null) {
            Modifier.clip(shape).clickable(enabled = enabled, onClick = onClick)
        } else {
            Modifier
        }
        Column(
            modifier = clickable.heightIn(min = 48.dp).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
            content = content,
        )
    }
}

/** Card title and the supporting line under it. */
@Composable
fun CardText(title: String, note: String? = null) {
    Text(title, style = MaterialTheme.typography.titleMedium, color = satchel.ink)
    if (note != null) {
        Text(note, style = MaterialTheme.typography.bodyMedium, color = satchel.secondary)
    }
}

/** The one main action of a screen: full width, accent fill. */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, enabled: Boolean = true, modifier: Modifier = Modifier) {
    Button(
        onClick = onClick,
        enabled = enabled,
        shape = RoundedCornerShape(14.dp),
        colors = ButtonDefaults.buttonColors(containerColor = satchel.accent, contentColor = satchel.card),
        modifier = modifier.fillMaxWidth().heightIn(min = 48.dp),
    ) {
        Text(text, fontWeight = FontWeight.SemiBold)
    }
}

/** A failure line: danger color, said once. */
@Composable
fun FailureText(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        fontWeight = FontWeight.SemiBold,
        color = satchel.danger,
        modifier = modifier,
    )
}
