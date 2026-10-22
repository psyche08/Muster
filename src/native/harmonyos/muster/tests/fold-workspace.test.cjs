// Run with Node and the TypeScript compiler bundled with DevEco Studio.
// Tests actual presentation code with explicit OS substitutes, not a device.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.TYPESCRIPT_PATH || '/Applications/DevEco-Studio.app/Contents/tools/ohpm/node_modules/typescript');
const root = path.join(__dirname, '../entry/src/main/ets/display');
function load(name, imports = {}) {
  const source = fs.readFileSync(path.join(root, name), 'utf8').replace(/@ObservedV2\s*/g, '').replace(/@Trace\s*/g, '');
  const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } }).outputText;
  const exports = {};
  vm.runInNewContext(code, { exports, require: id => { assert.ok(id in imports, id); return imports[id]; } });
  return exports;
}
const layout = load('WorkspaceLayout.ts');
const { workspaceLayout: resolve, FlexPreference: P, flexPreference } = layout;
const area = { left: 0, top: 76, width: 400, height: 824 };
const crease = { left: 0, top: 460, width: 900, height: 12 };
const fold = { halfOpen: true, creases: [crease], windowLeft: 0, windowTop: 0, keyboardHeight: 0 };
let count = 0;
function test(name, run) { run(); count++; console.log(`ok ${count} - ${name}`); }
test('horizontal hinge excludes physical crease and padding, not a clamped fraction', () => {
  const r = resolve(area, fold, P.Automatic);
  assert.equal(r.flex, true); assert.equal(r.readingHeight, 376); assert.equal(r.hingeHeight, 28);
  assert.equal(area.top + r.readingHeight + r.hingeHeight, crease.top + crease.height + 8);
});
test('window and content offsets are both accounted for', () => {
  const r = resolve({ ...area, left: 10, top: 60, height: 800 }, { ...fold, windowTop: 30, windowLeft: 50 }, P.Automatic);
  assert.equal(r.readingHeight, 362);
});
test('keyboard only reduces lower pane, reading boundary stays fixed', () => {
  const r = resolve({ ...area, height: 550 }, { ...fold, keyboardHeight: 274 }, P.Automatic);
  assert.equal(r.flex, true); assert.equal(r.readingHeight, 376);
  assert.equal(resolve({ ...area, height: 470 }, { ...fold, keyboardHeight: 354 }, P.Automatic).flex, true);
  assert.equal(resolve({ ...area, height: 450 }, { ...fold, keyboardHeight: 374 }, P.Automatic).flex, false);
});
test('ordinary devices never infer posture from dimensions', () => {
  assert.equal(resolve(area, { ...fold, halfOpen: false }, P.Automatic).flex, false);
});
test('preview preserves upper pane while keyboard opens', () => {
  const ordinary = { ...fold, halfOpen: false };
  const full = resolve(area, ordinary, P.Always);
  const ime = resolve({ ...area, height: 624 }, { ...ordinary, keyboardHeight: 200 }, P.Always);
  assert.equal(full.flex, true); assert.equal(ime.flex, true); assert.equal(full.readingHeight, ime.readingHeight);
});
test('disabled preference wins, even on half-folded hardware', () => {
  assert.equal(resolve(area, fold, P.Disabled).flex, false);
});
test('offscreen, vertical, partial-width and ambiguous creases never produce a false hinge', () => {
  for (const c of [ { ...crease, top: 900 }, { ...crease, top: 50 }, { ...crease, width: 10, height: 800 },
    { ...crease, width: 200 }, { ...crease, left: 1 }, { ...crease, height: -1 }, { ...crease, top: NaN } ]) {
    assert.equal(resolve(area, { ...fold, creases: [c] }, P.Automatic).flex, false);
  }
  for (const creases of [[], [crease, crease]]) {
    assert.equal(resolve(area, { ...fold, creases }, P.Always).flex, false);
  }
});
test('too-small and invalid windows fall back even when forced', () => {
  for (const height of [0, -1, NaN, Infinity, 400]) {
    assert.equal(resolve({ ...area, height }, { ...fold, halfOpen: false }, P.Always).flex, false);
  }
  assert.equal(resolve(area, { ...fold, windowTop: NaN }, P.Automatic).flex, false);
});
const book = { halfOpen: true, creases: [{ left: 380, top: 0, width: 40, height: 2000 }], windowLeft: 0, windowTop: 0, keyboardHeight: 0 };
const wide = { left: 0, top: 76, width: 800, height: 824 };
test('automatic puts the list beside the detail across a vertical crease', () => {
  const r = resolve(wide, book, P.Automatic);
  assert.equal(r.side, true); assert.equal(r.flex, false);
  assert.equal(r.listWidth, 372); assert.equal(r.gapWidth, 56);
  assert.equal(wide.left + r.listWidth + r.gapWidth, 380 + 40 + 8);
});
test('side by side splits at the middle without a crease and never under 280vp a pane', () => {
  const flat = { ...book, halfOpen: false, creases: [] };
  const r = resolve(wide, flat, P.Side);
  assert.equal(r.side, true); assert.equal(r.listWidth, 392); assert.equal(r.gapWidth, 16);
  assert.equal(resolve({ ...wide, width: 560 }, flat, P.Side).side, false);
  assert.equal(resolve(wide, flat, P.Automatic).side, false);
});
test('side by side refuses horizontal, partial and ambiguous creases', () => {
  assert.equal(resolve(wide, fold, P.Side).side, false);
  assert.equal(resolve(wide, { ...book, creases: [{ ...book.creases[0], top: 200 }] }, P.Side).side, false);
  assert.equal(resolve(wide, { ...book, creases: [book.creases[0], book.creases[0]] }, P.Side).side, false);
  assert.equal(resolve(wide, { ...book, creases: [{ ...book.creases[0], width: 40, height: 40 }] }, P.Automatic).side, false);
});
test('stacked and side by side are exclusive and Off splits nothing', () => {
  assert.equal(resolve(wide, book, P.Always).flex, false); assert.equal(resolve(wide, book, P.Always).side, false);
  assert.equal(resolve(area, fold, P.Side).flex, false);
  assert.equal(resolve(wide, book, P.Disabled).side, false);
});
test('unknown saved values restore automatic', () => {
  assert.equal(flexPreference('always'), P.Always); assert.equal(flexPreference('disabled'), P.Disabled);
  assert.equal(flexPreference('side'), P.Side);
  assert.equal(flexPreference('future'), P.Automatic);
});
// Real adapter methods, mocked OS events, density 2px/vp.
const displayListeners = new Map(); const windowListeners = new Map();
let status = 3, displayId = 7, fail = false, avoidMode = 0;
const display = {
  FoldStatus: { FOLD_STATUS_HALF_FOLDED: 3 },
  isFoldable: () => true, getFoldStatus: () => status,
  getDisplayByIdSync: id => { assert.equal(id, 7); return { getLiveCreaseRegion: () => { if (fail) throw Error('display gone'); return { displayId, creaseRects: [{ left: 0, top: 920, width: 1800, height: 24 }] }; } }; },
  on: (key, cb) => displayListeners.set(key, cb),
  off: (key, cb) => { assert.equal(displayListeners.get(key) ?? cb, cb); displayListeners.delete(key); },
};
const Orientation = { UNSPECIFIED: 0, LANDSCAPE: 2, LANDSCAPE_INVERTED: 4, AUTO_ROTATION_PORTRAIT: 6 };
const orientations = [];
const values = new Map();
const store = { getSync: (key, fallback) => values.get(key) ?? fallback, putSync: (key, value) => values.set(key, value), flush: () => Promise.resolve() };
const { FoldDisplay } = load('FoldDisplay.ets', {
  './WorkspaceLayout': layout,
  '@kit.ArkUI': { display, window: { AvoidAreaType: { TYPE_KEYBOARD: 4 }, Orientation: Orientation }, KeyboardAvoidMode: { OFFSET: 0, RESIZE: 1 } },
  '@kit.ArkData': { preferences: { getPreferencesSync: () => store } },
});
const w = {
  getWindowProperties: () => ({ displayId: 7, windowRect: { left: 100, top: 40 } }),
  getWindowAvoidArea: () => ({ bottomRect: { height: 0 } }),
  getUIContext: () => ({ px2vp: px => px / 2, getKeyboardAvoidMode: () => avoidMode, setKeyboardAvoidMode: mode => { avoidMode = mode; } }),
  on: (key, cb) => windowListeners.set(key, cb),
  off: (key, cb) => { assert.equal(windowListeners.get(key), cb); windowListeners.delete(key); },
  setPreferredOrientation: o => { orientations.push(o); return Promise.resolve(); },
};
const adapter = new FoldDisplay();
test('adapter maps screen pixels and window position to vp', () => {
  adapter.attach(w);
  assert.equal(adapter.geometry.halfOpen, true); assert.equal(adapter.geometry.creases[0].top, 460);
  assert.equal(adapter.geometry.windowLeft, 50); assert.equal(adapter.geometry.windowTop, 20);
  assert.equal(avoidMode, 1);
});
test('fold, rotation/display, window movement and keyboard events refresh readings', () => {
  assert.deepEqual([...displayListeners.keys()], ['foldStatusChange', 'foldDisplayModeChange', 'change']);
  assert.ok(windowListeners.has('windowRectChange'));
  windowListeners.get('keyboardHeightChange')(500); assert.equal(adapter.geometry.keyboardHeight, 250);
  status = 1; displayListeners.get('foldStatusChange')(); assert.equal(adapter.geometry.halfOpen, false);
  status = 3; displayListeners.get('change')(); assert.equal(adapter.geometry.halfOpen, true);
});
test('other display or OS errors discard stale hinge data', () => {
  displayId = 8; adapter.refresh(); assert.equal(adapter.geometry.halfOpen, false);
  displayId = 7; fail = true; adapter.refresh(); assert.equal(adapter.geometry.creases.length, 0);
  assert.equal(resolve(area, adapter.geometry, P.Always).flex, false);
  fail = false;
});
test('display preference persists independently of business state', () => {
  adapter.open({}); adapter.select(P.Always);
  const reopened = new FoldDisplay(); reopened.open({}); assert.equal(reopened.preference, P.Always);
});
test('a forced layout turns a half-folded screen to put the crease where it needs it', () => {
  adapter.attach(w); adapter.select(P.Automatic); orientations.length = 0;
  adapter.select(P.Always); assert.deepEqual(orientations, [Orientation.LANDSCAPE_INVERTED]);
  adapter.select(P.Side); assert.deepEqual(orientations.slice(1), [Orientation.AUTO_ROTATION_PORTRAIT]);
  adapter.refresh(); assert.equal(orientations.length, 2);
  status = 1; displayListeners.get('foldStatusChange')(); assert.equal(orientations[2], Orientation.UNSPECIFIED);
  status = 3; adapter.refresh(); assert.equal(orientations[3], Orientation.AUTO_ROTATION_PORTRAIT);
  adapter.select(P.Automatic); assert.equal(orientations[4], Orientation.UNSPECIFIED);
  adapter.select(P.Side); adapter.detach(); assert.equal(orientations[orientations.length - 1], Orientation.UNSPECIFIED);
});
test('top and bottom can be turned 180°, and the choice persists', () => {
  adapter.attach(w); adapter.select(P.Always); orientations.length = 0;
  adapter.flip(true); assert.deepEqual(orientations, [Orientation.LANDSCAPE]);
  const reopened = new FoldDisplay(); reopened.open({}); assert.equal(reopened.flipped, true);
  adapter.select(P.Side); adapter.flip(false); assert.deepEqual(orientations.slice(1), [Orientation.AUTO_ROTATION_PORTRAIT]);
  adapter.select(P.Always); assert.equal(orientations[2], Orientation.LANDSCAPE_INVERTED);
  adapter.detach();
});
test('reattach is idempotent and teardown removes all subscriptions', () => {
  adapter.attach(w); assert.equal(windowListeners.size, 2); assert.equal(displayListeners.size, 3);
  adapter.detach(); assert.equal(windowListeners.size, 0); assert.equal(displayListeners.size, 0);
  assert.equal(avoidMode, 0); assert.equal(adapter.geometry.halfOpen, false);
  adapter.detach();
});
console.log(`${count} fold workspace checks passed`);
