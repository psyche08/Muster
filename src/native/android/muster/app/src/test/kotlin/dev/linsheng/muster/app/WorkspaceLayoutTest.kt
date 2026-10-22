package dev.linsheng.muster.app

import dev.linsheng.muster.app.display.FlexPreference
import dev.linsheng.muster.app.display.FoldGeometry
import dev.linsheng.muster.app.display.WorkspaceArea
import dev.linsheng.muster.app.display.workspaceLayout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The half-fold workspace geometry (the same checks as the HarmonyOS shell's tests, without its OS adapter). */
class WorkspaceLayoutTest {
    private val area = WorkspaceArea(0.0, 76.0, 400.0, 824.0)
    private val crease = WorkspaceArea(0.0, 460.0, 900.0, 12.0)
    private val fold = FoldGeometry(halfOpen = true, creases = listOf(crease))
    private val wide = WorkspaceArea(0.0, 76.0, 800.0, 824.0)
    private val book = FoldGeometry(halfOpen = true, creases = listOf(WorkspaceArea(380.0, 0.0, 40.0, 2000.0)))

    @Test fun horizontalHingeExcludesPhysicalCreaseAndPaddingNotAClampedFraction() {
        val r = workspaceLayout(area, fold, FlexPreference.AUTOMATIC)
        assertTrue(r.flex); assertEquals(376.0, r.readingHeight, 0.0); assertEquals(28.0, r.hingeHeight, 0.0)
        assertEquals(crease.top + crease.height + 8, area.top + r.readingHeight + r.hingeHeight, 0.0)
    }

    @Test fun windowAndContentOffsetsAreBothAccountedFor() {
        val r = workspaceLayout(area.copy(left = 10.0, top = 60.0, height = 800.0), fold.copy(windowTop = 30.0, windowLeft = 50.0), FlexPreference.AUTOMATIC)
        assertEquals(362.0, r.readingHeight, 0.0)
    }

    @Test fun keyboardOnlyReducesLowerPaneReadingBoundaryStaysFixed() {
        val r = workspaceLayout(area.copy(height = 550.0), fold.copy(keyboardHeight = 274.0), FlexPreference.AUTOMATIC)
        assertTrue(r.flex); assertEquals(376.0, r.readingHeight, 0.0)
        assertTrue(workspaceLayout(area.copy(height = 470.0), fold.copy(keyboardHeight = 354.0), FlexPreference.AUTOMATIC).flex)
        assertFalse(workspaceLayout(area.copy(height = 450.0), fold.copy(keyboardHeight = 374.0), FlexPreference.AUTOMATIC).flex)
    }

    @Test fun ordinaryDevicesNeverInferPostureFromDimensions() {
        assertFalse(workspaceLayout(area, fold.copy(halfOpen = false), FlexPreference.AUTOMATIC).flex)
    }

    @Test fun previewPreservesUpperPaneWhileKeyboardOpens() {
        val ordinary = fold.copy(halfOpen = false)
        val full = workspaceLayout(area, ordinary, FlexPreference.ALWAYS)
        val ime = workspaceLayout(area.copy(height = 624.0), ordinary.copy(keyboardHeight = 200.0), FlexPreference.ALWAYS)
        assertTrue(full.flex); assertTrue(ime.flex); assertEquals(full.readingHeight, ime.readingHeight, 0.0)
    }

    @Test fun disabledPreferenceWinsEvenOnHalfFoldedHardware() {
        assertFalse(workspaceLayout(area, fold, FlexPreference.DISABLED).flex)
    }

    @Test fun offscreenVerticalPartialWidthAndAmbiguousCreasesNeverProduceAFalseHinge() {
        for (c in listOf(crease.copy(top = 900.0), crease.copy(top = 50.0), crease.copy(width = 10.0, height = 800.0),
            crease.copy(width = 200.0), crease.copy(left = 1.0), crease.copy(height = -1.0), crease.copy(top = Double.NaN))) {
            assertFalse(workspaceLayout(area, fold.copy(creases = listOf(c)), FlexPreference.AUTOMATIC).flex)
        }
        for (creases in listOf(emptyList(), listOf(crease, crease))) {
            assertFalse(workspaceLayout(area, fold.copy(creases = creases), FlexPreference.ALWAYS).flex)
        }
    }

    @Test fun tooSmallAndInvalidWindowsFallBackEvenWhenForced() {
        for (height in listOf(0.0, -1.0, Double.NaN, Double.POSITIVE_INFINITY, 400.0)) {
            assertFalse(workspaceLayout(area.copy(height = height), fold.copy(halfOpen = false), FlexPreference.ALWAYS).flex)
        }
        assertFalse(workspaceLayout(area, fold.copy(windowTop = Double.NaN), FlexPreference.AUTOMATIC).flex)
    }

    @Test fun automaticPutsTheListBesideTheDetailAcrossAVerticalCrease() {
        val r = workspaceLayout(wide, book, FlexPreference.AUTOMATIC)
        assertTrue(r.side); assertFalse(r.flex)
        assertEquals(372.0, r.listWidth, 0.0); assertEquals(56.0, r.gapWidth, 0.0)
        assertEquals(380.0 + 40 + 8, wide.left + r.listWidth + r.gapWidth, 0.0)
    }

    @Test fun sideBySideSplitsAtTheMiddleWithoutACreaseAndNeverUnder280APane() {
        val flat = FoldGeometry()
        val r = workspaceLayout(wide, flat, FlexPreference.SIDE)
        assertTrue(r.side); assertEquals(392.0, r.listWidth, 0.0); assertEquals(16.0, r.gapWidth, 0.0)
        assertFalse(workspaceLayout(wide.copy(width = 560.0), flat, FlexPreference.SIDE).side)
        assertFalse(workspaceLayout(wide, flat, FlexPreference.AUTOMATIC).side)
    }

    @Test fun sideBySideRefusesHorizontalPartialAndAmbiguousCreases() {
        assertFalse(workspaceLayout(wide, fold, FlexPreference.SIDE).side)
        assertFalse(workspaceLayout(wide, book.copy(creases = listOf(book.creases[0].copy(top = 200.0))), FlexPreference.SIDE).side)
        assertFalse(workspaceLayout(wide, book.copy(creases = listOf(book.creases[0], book.creases[0])), FlexPreference.SIDE).side)
        assertFalse(workspaceLayout(wide, book.copy(creases = listOf(book.creases[0].copy(width = 40.0, height = 40.0))), FlexPreference.AUTOMATIC).side)
    }

    @Test fun stackedAndSideBySideAreExclusiveAndOffSplitsNothing() {
        assertFalse(workspaceLayout(wide, book, FlexPreference.ALWAYS).flex); assertFalse(workspaceLayout(wide, book, FlexPreference.ALWAYS).side)
        assertFalse(workspaceLayout(area, fold, FlexPreference.SIDE).flex)
        assertFalse(workspaceLayout(wide, book, FlexPreference.DISABLED).side)
    }

    @Test fun unknownSavedValuesRestoreAutomatic() {
        assertEquals(FlexPreference.ALWAYS, FlexPreference.from("always"))
        assertEquals(FlexPreference.DISABLED, FlexPreference.from("disabled"))
        assertEquals(FlexPreference.SIDE, FlexPreference.from("side"))
        assertEquals(FlexPreference.AUTOMATIC, FlexPreference.from("future"))
        assertEquals(FlexPreference.AUTOMATIC, FlexPreference.from(null))
    }
}
