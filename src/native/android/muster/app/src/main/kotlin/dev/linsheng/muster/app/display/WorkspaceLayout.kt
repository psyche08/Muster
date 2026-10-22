package dev.linsheng.muster.app.display

// Presentation geometry only. No session, connection or authorization state.
//
// A foldable can split the workspace two ways at its crease: Stacked keeps
// reading above a horizontal crease and typing below it (laptop posture);
// Side puts a list beside the conversation or terminal across a vertical
// crease (book posture). Automatic follows the posture; the others force a
// layout and Off never splits. The same rules as the HarmonyOS shell's
// WorkspaceLayout.ts, in dp.

enum class FlexPreference(val stored: String) {
    AUTOMATIC("automatic"),
    /** Stacked (top and bottom); the stored name predates side by side. */
    ALWAYS("always"),
    SIDE("side"),
    DISABLED("disabled");

    companion object {
        /** Unknown saved values restore automatic. */
        fun from(value: String?): FlexPreference = entries.firstOrNull { it.stored == value } ?: AUTOMATIC
    }
}

data class WorkspaceArea(val left: Double = 0.0, val top: Double = 0.0, val width: Double = 0.0, val height: Double = 0.0)

/** Screen-relative dp, supplied by the OS adapter on the window's display. */
data class FoldGeometry(
    val halfOpen: Boolean = false,
    val creases: List<WorkspaceArea> = emptyList(),
    val windowLeft: Double = 0.0,
    val windowTop: Double = 0.0,
    val keyboardHeight: Double = 0.0,
)

/** Stacked: reading pane height, then the hinge gap, then the controls. Side by side: list width, gap, detail. */
data class WorkspaceLayout(
    val flex: Boolean = false,
    val readingHeight: Double = 0.0,
    val hingeHeight: Double = 0.0,
    val side: Boolean = false,
    val listWidth: Double = 0.0,
    val gapWidth: Double = 0.0,
)

/** Each side pane needs this much width; narrower windows do not split. */
private const val MIN_PANE_WIDTH = 280.0

private fun finite(vararg values: Double): Boolean = values.all { it.isFinite() }

fun workspaceLayout(area: WorkspaceArea, fold: FoldGeometry, preference: FlexPreference): WorkspaceLayout {
    val none = WorkspaceLayout()
    if (!finite(area.left, area.top, area.width, area.height, fold.windowLeft, fold.windowTop, fold.keyboardHeight) ||
        area.width <= 0 || area.height <= 0 || fold.keyboardHeight < 0 || preference == FlexPreference.DISABLED) return none
    // Multiple fold axes are ambiguous; never guess which one is in use.
    val crease = if (fold.halfOpen && fold.creases.size == 1) fold.creases[0] else null
    if (fold.halfOpen && crease == null) return none
    if (crease != null && (!finite(crease.left, crease.top, crease.width, crease.height) ||
            crease.width < 0 || crease.height < 0 || crease.width == crease.height)) return none
    val horizontal = crease != null && crease.width > crease.height
    if (preference == FlexPreference.SIDE || (preference == FlexPreference.AUTOMATIC && crease != null && !horizontal)) {
        return sideLayout(area, fold, crease)
    }
    if (preference == FlexPreference.ALWAYS || (preference == FlexPreference.AUTOMATIC && horizontal)) {
        return stackedLayout(area, fold, crease)
    }
    return none
}

private fun stackedLayout(area: WorkspaceArea, fold: FoldGeometry, crease: WorkspaceArea?): WorkspaceLayout {
    val none = WorkspaceLayout()
    val left = area.left + fold.windowLeft
    val top = area.top + fold.windowTop
    val hingeTop: Double
    val hingeBottom: Double
    if (crease != null) {
        if (crease.width <= crease.height || crease.left > left || crease.left + crease.width < left + area.width) return none
        hingeTop = crease.top - top - 8
        hingeBottom = crease.top + crease.height - top + 8
    } else {
        // The keyboard reduces the available area. Restore it only to locate
        // the preview hinge, so typing does not move the upper pane.
        hingeTop = (area.height + fold.keyboardHeight) / 2 - 8
        hingeBottom = hingeTop + 16
    }
    // Keep a usable reading pane and scrollable controls; with a keyboard up the
    // lower pane only needs to show the field being typed into.
    val minControls = if (fold.keyboardHeight > 0) 56.0 else 220.0
    if (hingeTop < 220 || area.height - hingeBottom < minControls) return none
    return WorkspaceLayout(flex = true, readingHeight = hingeTop, hingeHeight = hingeBottom - hingeTop)
}

private fun sideLayout(area: WorkspaceArea, fold: FoldGeometry, crease: WorkspaceArea?): WorkspaceLayout {
    val none = WorkspaceLayout()
    val left = area.left + fold.windowLeft
    val top = area.top + fold.windowTop
    val gapLeft: Double
    val gapRight: Double
    if (crease != null) {
        // Only a vertical crease running through the whole workspace height.
        if (crease.height <= crease.width || crease.top > top || crease.top + crease.height < top + area.height) return none
        gapLeft = crease.left - left - 8
        gapRight = crease.left + crease.width - left + 8
    } else {
        gapLeft = area.width / 2 - 8
        gapRight = gapLeft + 16
    }
    if (gapLeft < MIN_PANE_WIDTH || area.width - gapRight < MIN_PANE_WIDTH) return none
    return WorkspaceLayout(side = true, listWidth = gapLeft, gapWidth = gapRight - gapLeft)
}
