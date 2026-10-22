// The half-fold workspace geometry (the same checks as the HarmonyOS
// shell's tests/fold-workspace.test.cjs, without its OS adapter).
import XCTest
@testable import MusterCore

final class WorkspaceLayoutTests: XCTestCase {
    let area = WorkspaceArea(left: 0, top: 76, width: 400, height: 824)
    let crease = WorkspaceArea(left: 0, top: 460, width: 900, height: 12)
    var fold: FoldGeometry { FoldGeometry(halfOpen: true, creases: [crease]) }
    let wide = WorkspaceArea(left: 0, top: 76, width: 800, height: 824)
    var book: FoldGeometry { FoldGeometry(halfOpen: true, creases: [WorkspaceArea(left: 380, top: 0, width: 40, height: 2000)]) }

    private func resolve(_ area: WorkspaceArea, _ fold: FoldGeometry, _ preference: FlexPreference) -> WorkspaceLayout {
        workspaceLayout(area: area, fold: fold, preference: preference)
    }

    func testHorizontalHingeExcludesPhysicalCreaseAndPaddingNotAClampedFraction() {
        let r = resolve(area, fold, .automatic)
        XCTAssertTrue(r.flex)
        XCTAssertEqual(r.readingHeight, 376)
        XCTAssertEqual(r.hingeHeight, 28)
        XCTAssertEqual(area.top + r.readingHeight + r.hingeHeight, crease.top + crease.height + 8)
    }

    func testWindowAndContentOffsetsAreBothAccountedFor() {
        var a = area; a.left = 10; a.top = 60; a.height = 800
        var f = fold; f.windowTop = 30; f.windowLeft = 50
        XCTAssertEqual(resolve(a, f, .automatic).readingHeight, 362)
    }

    func testKeyboardOnlyReducesLowerPaneReadingBoundaryStaysFixed() {
        var a = area; a.height = 550
        var f = fold; f.keyboardHeight = 274
        let r = resolve(a, f, .automatic)
        XCTAssertTrue(r.flex); XCTAssertEqual(r.readingHeight, 376)
        a.height = 470; f.keyboardHeight = 354
        XCTAssertTrue(resolve(a, f, .automatic).flex)
        a.height = 450; f.keyboardHeight = 374
        XCTAssertFalse(resolve(a, f, .automatic).flex)
    }

    func testOrdinaryDevicesNeverInferPostureFromDimensions() {
        var f = fold; f.halfOpen = false
        XCTAssertFalse(resolve(area, f, .automatic).flex)
    }

    func testPreviewPreservesUpperPaneWhileKeyboardOpens() {
        var ordinary = fold; ordinary.halfOpen = false
        let full = resolve(area, ordinary, .always)
        var a = area; a.height = 624
        var f = ordinary; f.keyboardHeight = 200
        let ime = resolve(a, f, .always)
        XCTAssertTrue(full.flex); XCTAssertTrue(ime.flex)
        XCTAssertEqual(full.readingHeight, ime.readingHeight)
    }

    func testDisabledPreferenceWinsEvenOnHalfFoldedHardware() {
        XCTAssertFalse(resolve(area, fold, .disabled).flex)
    }

    func testOffscreenVerticalPartialWidthAndAmbiguousCreasesNeverProduceAFalseHinge() {
        var variants: [WorkspaceArea] = []
        var c = crease; c.top = 900; variants.append(c)
        c = crease; c.top = 50; variants.append(c)
        c = crease; c.width = 10; c.height = 800; variants.append(c)
        c = crease; c.width = 200; variants.append(c)
        c = crease; c.left = 1; variants.append(c)
        c = crease; c.height = -1; variants.append(c)
        c = crease; c.top = .nan; variants.append(c)
        for variant in variants {
            var f = fold; f.creases = [variant]
            XCTAssertFalse(resolve(area, f, .automatic).flex)
        }
        for creases in [[], [crease, crease]] {
            var f = fold; f.creases = creases
            XCTAssertFalse(resolve(area, f, .always).flex)
        }
    }

    func testTooSmallAndInvalidWindowsFallBackEvenWhenForced() {
        var f = fold; f.halfOpen = false
        for height in [0.0, 1, 16, -1, .nan, .infinity] {
            var a = area; a.height = height
            XCTAssertFalse(resolve(a, f, .always).flex)
        }
        var g = fold; g.windowTop = .nan
        XCTAssertFalse(resolve(area, g, .automatic).flex)
    }

    func testForcedStackedLayoutDoesNotDisappearWhenKeyboardOrViewportChanges() {
        for height in [320.0, 400, 600, 824] {
            for keyboard in [0.0, 280, 350] {
                let value = resolve(WorkspaceArea(width: 393, height: height),
                                    FoldGeometry(keyboardHeight: keyboard), .always)
                XCTAssertTrue(value.flex)
                XCTAssertLessThan(value.readingHeight + value.hingeHeight, height)
            }
        }
    }

    func testAutomaticPutsTheListBesideTheDetailAcrossAVerticalCrease() {
        let r = resolve(wide, book, .automatic)
        XCTAssertTrue(r.side); XCTAssertFalse(r.flex)
        XCTAssertEqual(r.listWidth, 372); XCTAssertEqual(r.gapWidth, 56)
        XCTAssertEqual(wide.left + r.listWidth + r.gapWidth, 380 + 40 + 8)
    }

    func testSideBySideSplitsAtTheMiddleWithoutACreaseAndNeverUnder280APane() {
        let flat = FoldGeometry()
        let r = resolve(wide, flat, .side)
        XCTAssertTrue(r.side); XCTAssertEqual(r.listWidth, 392); XCTAssertEqual(r.gapWidth, 16)
        var narrow = wide; narrow.width = 560
        XCTAssertFalse(resolve(narrow, flat, .side).side)
        XCTAssertFalse(resolve(wide, flat, .automatic).side)
    }

    func testSideBySideRefusesHorizontalPartialAndAmbiguousCreases() {
        XCTAssertFalse(resolve(wide, fold, .side).side)
        var partial = book; partial.creases[0].top = 200
        XCTAssertFalse(resolve(wide, partial, .side).side)
        var two = book; two.creases = [book.creases[0], book.creases[0]]
        XCTAssertFalse(resolve(wide, two, .side).side)
        var square = book; square.creases[0].width = 40; square.creases[0].height = 40
        XCTAssertFalse(resolve(wide, square, .automatic).side)
    }

    func testStackedAndSideBySideAreExclusiveAndOffSplitsNothing() {
        XCTAssertFalse(resolve(wide, book, .always).flex); XCTAssertFalse(resolve(wide, book, .always).side)
        XCTAssertFalse(resolve(area, fold, .side).flex)
        XCTAssertFalse(resolve(wide, book, .disabled).side)
    }

    func testUnknownSavedValuesRestoreAutomatic() {
        XCTAssertEqual(FlexPreference(stored: "always"), .always)
        XCTAssertEqual(FlexPreference(stored: "disabled"), .disabled)
        XCTAssertEqual(FlexPreference(stored: "side"), .side)
        XCTAssertEqual(FlexPreference(stored: "future"), .automatic)
    }
}
