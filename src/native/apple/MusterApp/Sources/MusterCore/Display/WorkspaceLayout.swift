// Presentation geometry only. No session, connection or authorization state.
//
// A foldable can split the workspace two ways at its crease: Stacked keeps
// reading above a horizontal crease and typing below it (laptop posture);
// Side puts a list beside the conversation or terminal across a vertical
// crease (book posture). Automatic follows the posture; the others force a
// layout and Off never splits. The same rules as the HarmonyOS shell's
// WorkspaceLayout.ts, in points.
import Foundation

public enum FlexPreference: String, CaseIterable, Sendable {
    case automatic
    /// Stacked (top and bottom); the stored name predates side by side.
    case always
    case side
    case disabled

    /// Unknown saved values restore automatic.
    public init(stored: String) { self = FlexPreference(rawValue: stored) ?? .automatic }
}

public struct WorkspaceArea: Equatable, Sendable {
    public var left: Double
    public var top: Double
    public var width: Double
    public var height: Double

    public init(left: Double = 0, top: Double = 0, width: Double = 0, height: Double = 0) {
        self.left = left; self.top = top; self.width = width; self.height = height
    }
}

public struct FoldGeometry: Equatable, Sendable {
    public var halfOpen: Bool
    /// Screen-relative points, supplied by the OS adapter on the window's display.
    public var creases: [WorkspaceArea]
    public var windowLeft: Double
    public var windowTop: Double
    public var keyboardHeight: Double

    public init(halfOpen: Bool = false, creases: [WorkspaceArea] = [], windowLeft: Double = 0, windowTop: Double = 0, keyboardHeight: Double = 0) {
        self.halfOpen = halfOpen; self.creases = creases; self.windowLeft = windowLeft; self.windowTop = windowTop
        self.keyboardHeight = keyboardHeight
    }
}

public struct WorkspaceLayout: Equatable, Sendable {
    /// Stacked: reading pane height, then the hinge gap, then the controls.
    public var flex = false
    public var readingHeight = 0.0
    public var hingeHeight = 0.0
    /// Side by side: list width, then the hinge gap, then the detail pane.
    public var side = false
    public var listWidth = 0.0
    public var gapWidth = 0.0

    public init() {}
}

/// Each side pane needs this much width; narrower windows do not split.
private let minPaneWidth = 280.0

private func finite(_ values: [Double]) -> Bool { values.allSatisfy(\.isFinite) }

public func workspaceLayout(area: WorkspaceArea, fold: FoldGeometry, preference: FlexPreference) -> WorkspaceLayout {
    let result = WorkspaceLayout()
    if !finite([area.left, area.top, area.width, area.height, fold.windowLeft, fold.windowTop, fold.keyboardHeight])
        || area.width <= 0 || area.height <= 0 || fold.keyboardHeight < 0 || preference == .disabled { return result }
    // Multiple fold axes are ambiguous; never guess which one is in use.
    let crease = fold.halfOpen && fold.creases.count == 1 ? fold.creases[0] : nil
    if fold.halfOpen && crease == nil { return result }
    if let crease, !finite([crease.left, crease.top, crease.width, crease.height])
        || crease.width < 0 || crease.height < 0 || crease.width == crease.height { return result }
    let horizontal = crease.map { $0.width > $0.height } ?? false
    if preference == .side || (preference == .automatic && crease != nil && !horizontal) {
        return sideLayout(area, fold, crease, result)
    }
    if preference == .always || (preference == .automatic && horizontal) {
        return stackedLayout(area, fold, crease, result)
    }
    return result
}

private func stackedLayout(_ area: WorkspaceArea, _ fold: FoldGeometry, _ crease: WorkspaceArea?, _ result: WorkspaceLayout) -> WorkspaceLayout {
    var result = result
    let left = area.left + fold.windowLeft
    let top = area.top + fold.windowTop
    var hingeTop = 0.0
    var hingeBottom = 0.0
    if let crease {
        if crease.width <= crease.height || crease.left > left || crease.left + crease.width < left + area.width { return result }
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
    let minControls = fold.keyboardHeight > 0 ? 56.0 : 220.0
    if crease != nil && (hingeTop < 220 || area.height - hingeBottom < minControls) { return result }
    if crease == nil {
        if area.height <= 16 { return result }
        // A chosen stacked layout must survive keyboard dismissal and small
        // viewports. The UI compresses the reading pane before its controls.
        hingeTop = max(0, min(hingeTop, area.height - 72))
        hingeBottom = hingeTop + 16
    }
    result.flex = true
    result.readingHeight = hingeTop
    result.hingeHeight = hingeBottom - hingeTop
    return result
}

private func sideLayout(_ area: WorkspaceArea, _ fold: FoldGeometry, _ crease: WorkspaceArea?, _ result: WorkspaceLayout) -> WorkspaceLayout {
    var result = result
    let left = area.left + fold.windowLeft
    let top = area.top + fold.windowTop
    var gapLeft = 0.0
    var gapRight = 0.0
    if let crease {
        // Only a vertical crease running through the whole workspace height.
        if crease.height <= crease.width || crease.top > top || crease.top + crease.height < top + area.height { return result }
        gapLeft = crease.left - left - 8
        gapRight = crease.left + crease.width - left + 8
    } else {
        gapLeft = area.width / 2 - 8
        gapRight = gapLeft + 16
    }
    if gapLeft < minPaneWidth || area.width - gapRight < minPaneWidth { return result }
    result.side = true
    result.listWidth = gapLeft
    result.gapWidth = gapRight - gapLeft
    return result
}
