// OS posture, window geometry and local display preferences. Business state
// remains in MusterCore. All subscriptions have matching window teardown.
// While half-folded, a forced layout also asks for the orientation that puts
// the crease where it needs it: landscape for stacked panes (laptop posture),
// portrait for side-by-side panes (book posture). Stacked can be turned
// 180°, for a phone that stands on its other edge.
import { display, window, KeyboardAvoidMode } from '@kit.ArkUI';
import { common } from '@kit.AbilityKit';
import { preferences } from '@kit.ArkData';
import { FlexPreference, flexPreference, FoldGeometry, WorkspaceArea } from './WorkspaceLayout';

@ObservedV2
export class FoldDisplay {
  @Trace geometry: FoldGeometry = new FoldGeometry();
  @Trace preference: FlexPreference = FlexPreference.Automatic;
  /** Forced top and bottom shows upside down (the other landscape). */
  @Trace flipped: boolean = false;
  @Trace preferenceError: string = '';
  private mainWindow: window.Window | null = null;
  private store: preferences.Preferences | null = null;
  private previousAvoidMode: KeyboardAvoidMode = KeyboardAvoidMode.OFFSET;
  private requestedOrientation: window.Orientation = window.Orientation.UNSPECIFIED;
  private readonly changed = (): void => { this.refresh(); };
  private readonly keyboardChanged = (height: number): void => { this.refresh(height); };

  open(context: common.UIAbilityContext): void {
    try {
      this.store = preferences.getPreferencesSync(context, { name: 'muster-display' });
      this.preference = flexPreference(String(this.store.getSync('flexPreference', 'automatic')));
      this.flipped = this.store.getSync('flexFlipped', false) === true;
    } catch (_) {
      this.preferenceError = 'Display preference could not be loaded.';
    }
  }

  select(value: FlexPreference): void {
    this.preference = value;
    this.orient();
    this.save('flexPreference', value);
  }

  flip(on: boolean): void {
    this.flipped = on;
    this.orient();
    this.save('flexFlipped', on);
  }

  private save(key: string, value: preferences.ValueType): void {
    try {
      if (this.store === null) throw new Error('preferences unavailable');
      this.store.putSync(key, value);
      this.store.flush().then((): void => { this.preferenceError = ''; })
        .catch((): void => { this.preferenceError = 'Display preference applies for this run only.'; });
    } catch (_) {
      this.preferenceError = 'Display preference applies for this run only.';
    }
  }

  attach(w: window.Window): void {
    this.detach();
    this.mainWindow = w;
    try {
      this.previousAvoidMode = w.getUIContext().getKeyboardAvoidMode();
      w.getUIContext().setKeyboardAvoidMode(KeyboardAvoidMode.RESIZE);
    } catch (_) {}
    try { display.on('foldStatusChange', this.changed); } catch (_) {}
    try { display.on('foldDisplayModeChange', this.changed); } catch (_) {}
    try { display.on('change', this.changed); } catch (_) {}
    try { w.on('windowRectChange', this.changed); } catch (_) {}
    try { w.on('keyboardHeightChange', this.keyboardChanged); } catch (_) {}
    this.refresh();
  }

  refresh(keyboardHeightPx?: number): void {
    const next = new FoldGeometry();
    const w = this.mainWindow;
    if (w === null) return;
    try {
      const props = w.getWindowProperties();
      const ui = w.getUIContext();
      next.windowLeft = ui.px2vp(props.windowRect.left);
      next.windowTop = ui.px2vp(props.windowRect.top);
      next.keyboardHeight = keyboardHeightPx === undefined ?
        ui.px2vp(w.getWindowAvoidArea(window.AvoidAreaType.TYPE_KEYBOARD).bottomRect.height) : ui.px2vp(keyboardHeightPx);
      if (display.isFoldable() && display.getFoldStatus() === display.FoldStatus.FOLD_STATUS_HALF_FOLDED) {
        // API 20 live geometry follows rotation and the actual window display.
        const region = display.getDisplayByIdSync(props.displayId).getLiveCreaseRegion();
        if (region.displayId === props.displayId) {
          next.halfOpen = true;
          next.creases = region.creaseRects.map((rect: display.Rect): WorkspaceArea => {
            const area = new WorkspaceArea();
            area.left = ui.px2vp(rect.left);
            area.top = ui.px2vp(rect.top);
            area.width = ui.px2vp(rect.width);
            area.height = ui.px2vp(rect.height);
            return area;
          });
        }
      }
    } catch (_) {
      // Never retain stale hinge coordinates after a failed OS read.
      next.halfOpen = false;
      next.creases = [];
      next.windowTop = Number.NaN;
    }
    this.geometry = next;
    this.orient();
  }

  /** The orientation the preference asks for in the current posture. */
  private wanted(): window.Orientation {
    if (!this.geometry.halfOpen) return window.Orientation.UNSPECIFIED;
    // A fixed landscape: half-folded, the sensor in the half lying flat cannot
    // tell which edge is up. Unflipped is the one auto-rotation settled on
    // standing in laptop posture (display rotation 90 on a Mate X7).
    if (this.preference === FlexPreference.Always) {
      return this.flipped ? window.Orientation.LANDSCAPE : window.Orientation.LANDSCAPE_INVERTED;
    }
    if (this.preference === FlexPreference.Side) return window.Orientation.AUTO_ROTATION_PORTRAIT;
    return window.Orientation.UNSPECIFIED;
  }

  private orient(): void {
    const w = this.mainWindow;
    const wanted = this.wanted();
    if (w === null || wanted === this.requestedOrientation) return;
    this.requestedOrientation = wanted;
    w.setPreferredOrientation(wanted).catch((): void => {});
  }

  detach(): void {
    try { display.off('foldStatusChange', this.changed); } catch (_) {}
    try { display.off('foldDisplayModeChange', this.changed); } catch (_) {}
    try { display.off('change', this.changed); } catch (_) {}
    const w = this.mainWindow;
    if (w !== null) {
      try { w.off('windowRectChange', this.changed); } catch (_) {}
      try { w.off('keyboardHeightChange', this.keyboardChanged); } catch (_) {}
      try { w.getUIContext().setKeyboardAvoidMode(this.previousAvoidMode); } catch (_) {}
      if (this.requestedOrientation !== window.Orientation.UNSPECIFIED) {
        try { w.setPreferredOrientation(window.Orientation.UNSPECIFIED).catch((): void => {}); } catch (_) {}
      }
    }
    this.requestedOrientation = window.Orientation.UNSPECIFIED;
    this.mainWindow = null;
    this.geometry = new FoldGeometry();
  }
}
