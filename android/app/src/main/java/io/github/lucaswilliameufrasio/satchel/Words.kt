package io.github.lucaswilliameufrasio.satchel

import android.content.res.Resources
import androidx.annotation.PluralsRes
import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext

/**
 * Looks up the app's strings by resource id.
 *
 * Logic that builds text takes this instead of a `Context`, so it runs in JVM
 * tests against the same `strings.xml` files the app ships.
 */
interface Words {
    fun text(@StringRes id: Int, vararg args: Any): String

    fun count(@PluralsRes id: Int, count: Int, vararg args: Any): String
}

/** [Words] from Android resources, in the language the system picked. */
class ResourceWords(private val resources: Resources) : Words {
    override fun text(id: Int, vararg args: Any): String {
        return resources.getString(id, *args)
    }

    override fun count(id: Int, count: Int, vararg args: Any): String {
        return resources.getQuantityString(id, count, *args)
    }
}

/** [Words] for the current composition. A language change gives a new one. */
@Composable
fun rememberWords(): Words {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    return remember(context, configuration) { ResourceWords(context.resources) }
}
