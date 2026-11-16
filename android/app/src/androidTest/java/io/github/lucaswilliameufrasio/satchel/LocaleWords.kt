package io.github.lucaswilliameufrasio.satchel

import android.content.res.Configuration
import androidx.test.platform.app.InstrumentationRegistry
import java.util.Locale

/**
 * The app's own strings in [tag], whatever language the emulator runs in.
 *
 * Tests read expected text through this, so they check the same words on any device.
 */
fun wordsIn(tag: String): Words {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val configuration = Configuration(context.resources.configuration)
    configuration.setLocale(Locale.forLanguageTag(tag))
    return ResourceWords(context.createConfigurationContext(configuration).resources)
}
