package dev.linsheng.muster.app.display

import android.app.Activity
import android.content.Context
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update

// OS posture, window geometry and local display preferences. Business state
// remains in MusterCore. Jetpack WindowManager reports a foldable's hinge as
// a FoldingFeature in window coordinates; the keyboard height comes from the
// window's IME insets (MainActivity) and from the in-app keyboard itself.
class FoldDisplay(context: Context) {
    private val store = context.getSharedPreferences("muster-display", Context.MODE_PRIVATE)
    private val _preference = MutableStateFlow(FlexPreference.from(store.getString("flexPreference", null)))
    val preference: StateFlow<FlexPreference> = _preference
    private val _geometry = MutableStateFlow(FoldGeometry())
    val geometry: StateFlow<FoldGeometry> = _geometry

    fun select(value: FlexPreference) {
        _preference.value = value
        store.edit().putString("flexPreference", value.stored).apply()
    }

    /** The IME's height over the window, in dp. */
    fun keyboard(heightDp: Double) {
        _geometry.update { it.copy(keyboardHeight = heightDp.coerceAtLeast(0.0)) }
    }

    /**
     * Follows the window's layout: half-opened folds become creases in
     * window-relative dp (so the window offsets are zero). Runs until the
     * caller's scope ends; a failed read keeps no stale hinge.
     */
    suspend fun track(activity: Activity) {
        val density = activity.resources.displayMetrics.density.toDouble()
        try {
            WindowInfoTracker.getOrCreate(activity).windowLayoutInfo(activity).collect { info ->
                val folds = info.displayFeatures.filterIsInstance<FoldingFeature>().filter { it.state == FoldingFeature.State.HALF_OPENED }
                val creases = folds.map { fold ->
                    WorkspaceArea(
                        left = fold.bounds.left / density, top = fold.bounds.top / density,
                        width = fold.bounds.width() / density, height = fold.bounds.height() / density,
                    )
                }
                _geometry.update { it.copy(halfOpen = folds.isNotEmpty(), creases = creases, windowLeft = 0.0, windowTop = 0.0) }
            }
        } catch (_: Exception) {
            _geometry.update { it.copy(halfOpen = false, creases = emptyList()) }
        }
    }
}

/** The layout choices as the menu and Settings name them. */
val FlexPreference.title: String
    get() = when (this) {
        FlexPreference.AUTOMATIC -> "Automatic"
        FlexPreference.ALWAYS -> "Top and bottom"
        FlexPreference.SIDE -> "Side by side"
        FlexPreference.DISABLED -> "Single pane"
    }
