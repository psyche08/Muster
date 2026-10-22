// Presentation geometry only. No session, connection or authorization state.
//
// A foldable can split the workspace two ways at its crease: Stacked keeps
// reading above a horizontal crease and typing below it (laptop posture);
// Side puts a list beside the conversation or terminal across a vertical
// crease (book posture). Automatic follows the posture; the others force a
// layout and Off never splits.
export enum FlexPreference {
  Automatic = 'automatic',
  Always = 'always', // stacked; the stored name predates side by side
  Side = 'side',
  Disabled = 'disabled',
}

export function flexPreference(value: string): FlexPreference {
  if (value === FlexPreference.Always) return FlexPreference.Always;
  if (value === FlexPreference.Side) return FlexPreference.Side;
  if (value === FlexPreference.Disabled) return FlexPreference.Disabled;
  return FlexPreference.Automatic;
}

export class WorkspaceArea {
  left: number = 0;
  top: number = 0;
  width: number = 0;
  height: number = 0;
}

export class FoldGeometry {
  halfOpen: boolean = false;
  // Screen-relative vp, supplied by the OS adapter on the window's display.
  creases: WorkspaceArea[] = [];
  windowLeft: number = 0;
  windowTop: number = 0;
  keyboardHeight: number = 0;
}

export class WorkspaceLayout {
  /** Stacked: reading pane height, then the hinge gap, then the controls. */
  flex: boolean = false;
  readingHeight: number = 0;
  hingeHeight: number = 0;
  /** Side by side: list width, then the hinge gap, then the detail pane. */
  side: boolean = false;
  listWidth: number = 0;
  gapWidth: number = 0;
}

/** Each side pane needs this much width; narrower windows do not split. */
const MIN_PANE_WIDTH = 280;

function finite(values: number[]): boolean {
  return values.every((n: number): boolean => Number.isFinite(n));
}

export function workspaceLayout(area: WorkspaceArea, fold: FoldGeometry, preference: FlexPreference): WorkspaceLayout {
  const result = new WorkspaceLayout();
  if (!finite([area.left, area.top, area.width, area.height, fold.windowLeft, fold.windowTop, fold.keyboardHeight]) ||
    area.width <= 0 || area.height <= 0 || fold.keyboardHeight < 0 || preference === FlexPreference.Disabled) return result;
  // Multiple fold axes are ambiguous; never guess which one is in use.
  const crease = fold.halfOpen && fold.creases.length === 1 ? fold.creases[0] : null;
  if (fold.halfOpen && crease === null) return result;
  if (crease !== null && (!finite([crease.left, crease.top, crease.width, crease.height]) ||
    crease.width < 0 || crease.height < 0 || crease.width === crease.height)) return result;
  const horizontal = crease !== null && crease.width > crease.height;
  if (preference === FlexPreference.Side || (preference === FlexPreference.Automatic && crease !== null && !horizontal)) {
    return sideLayout(area, fold, crease, result);
  }
  if (preference === FlexPreference.Always || (preference === FlexPreference.Automatic && horizontal)) {
    return stackedLayout(area, fold, crease, result);
  }
  return result;
}

function stackedLayout(area: WorkspaceArea, fold: FoldGeometry, crease: WorkspaceArea | null,
  result: WorkspaceLayout): WorkspaceLayout {
  const left = area.left + fold.windowLeft;
  const top = area.top + fold.windowTop;
  let hingeTop = 0;
  let hingeBottom = 0;
  if (crease !== null) {
    if (crease.width <= crease.height || crease.left > left || crease.left + crease.width < left + area.width) return result;
    hingeTop = crease.top - top - 8;
    hingeBottom = crease.top + crease.height - top + 8;
  } else {
    // RESIZE reduces the available area by the keyboard overlap. Restore it
    // only to locate the preview hinge, so typing does not move the upper pane.
    hingeTop = (area.height + fold.keyboardHeight) / 2 - 8;
    hingeBottom = hingeTop + 16;
  }
  // Keep a usable reading pane and scrollable controls; with a keyboard up the
  // lower pane only needs to show the field being typed into.
  const minControls = fold.keyboardHeight > 0 ? 56 : 220;
  if (hingeTop < 220 || area.height - hingeBottom < minControls) return result;
  result.flex = true;
  result.readingHeight = hingeTop;
  result.hingeHeight = hingeBottom - hingeTop;
  return result;
}

function sideLayout(area: WorkspaceArea, fold: FoldGeometry, crease: WorkspaceArea | null,
  result: WorkspaceLayout): WorkspaceLayout {
  const left = area.left + fold.windowLeft;
  const top = area.top + fold.windowTop;
  let gapLeft = 0;
  let gapRight = 0;
  if (crease !== null) {
    // Only a vertical crease running through the whole workspace height.
    if (crease.height <= crease.width || crease.top > top || crease.top + crease.height < top + area.height) return result;
    gapLeft = crease.left - left - 8;
    gapRight = crease.left + crease.width - left + 8;
  } else {
    gapLeft = area.width / 2 - 8;
    gapRight = gapLeft + 16;
  }
  if (gapLeft < MIN_PANE_WIDTH || area.width - gapRight < MIN_PANE_WIDTH) return result;
  result.side = true;
  result.listWidth = gapLeft;
  result.gapWidth = gapRight - gapLeft;
  return result;
}
