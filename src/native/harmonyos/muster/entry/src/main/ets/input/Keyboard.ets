// The in-app keyboard: QWERTY with pinyin Chinese and English, bound to text
// fields with customKeyboard() instead of the system IME. Wide windows get a
// MacBook's layout (MacLayout.ts); narrow ones a phone layout with number and
// symbol pages. Fields hand it a KeyboardTarget; it never reads their contents.
import { common } from '@kit.AbilityKit';
import { util } from '@kit.ArkTS';
import { preferences } from '@kit.ArkData';
import { display } from '@kit.ArkUI';
import { pasteboard } from '@kit.BasicServicesKit';
import { ChinesePunctuation, FUNCTION_ROW, KeyKind, MAC_ROWS, MacKey } from './MacLayout';
import { Candidate, PinyinComposer, PinyinDict } from './PinyinEngine';
import { charLeft, charRight, lineDown, lineEnd, lineStart, lineUp, wordLeft, wordRight } from './TextEdits';

/**
 * Keyboard choices shared by every field: the system IME instead, the same
 * choice for terminals, and the last language. A terminal uses the in-app
 * keyboard by default, since the IME has no Esc, F-keys, Ctrl or Alt.
 */
@ObservedV2
export class KeyboardPrefs {
  @Trace systemKeyboard: boolean = false;
  /** The system IME in a terminal instead of the in-app keyboard. */
  @Trace terminalSystemKeyboard: boolean = false;
  @Trace chinese: boolean = true;
  /** Height (vp) of the in-app keyboard while it is up: the half-fold layout keeps the hinge clear of it. */
  @Trace keyboardHeight: number = 0;
  private store: preferences.Preferences | null = null;

  open(context: common.UIAbilityContext): void {
    try {
      this.store = preferences.getPreferencesSync(context, { name: 'muster-display' });
      this.systemKeyboard = this.store.getSync('systemKeyboard', false) === true;
      this.terminalSystemKeyboard = this.store.getSync('terminalSystemKeyboard', false) === true;
      this.chinese = this.store.getSync('keyboardChinese', true) !== false;
    } catch (_) {}
  }

  private save(key: string, value: boolean): void {
    try {
      this.store?.putSync(key, value);
      this.store?.flush().catch((): void => {});
    } catch (_) {}
  }

  useSystem(on: boolean): void { this.systemKeyboard = on; this.save('systemKeyboard', on); }
  useSystemInTerminal(on: boolean): void { this.terminalSystemKeyboard = on; this.save('terminalSystemKeyboard', on); }
  setChinese(on: boolean): void { this.chinese = on; this.save('keyboardChinese', on); }
}

export const keyboardPrefs: KeyboardPrefs = new KeyboardPrefs();

/**
 * Focuses a field by id so its keyboard comes up. requestFocus throws while
 * the field is not laid out, hidden or disabled; try a few times, then give up.
 */
export function focusField(ui: UIContext, id: string, attempts: number = 5): void {
  setTimeout((): void => {
    try {
      ui.getFocusController().requestFocus(id);
    } catch (e) {
      if (attempts > 1) focusField(ui, id, attempts - 1);
    }
  }, 200);
}

/** The dictionary loads once, off the first keyboard, and one composer keeps what the user picks. */
export class Pinyin {
  static composer: PinyinComposer | null = null;
  private static loading: Promise<PinyinComposer> | null = null;

  static load(context: Context): Promise<PinyinComposer> {
    if (Pinyin.loading === null) {
      Pinyin.loading = context.resourceManager.getRawFileContent('pinyin.txt').then((bytes: Uint8Array): PinyinComposer => {
        const text = util.TextDecoder.create('utf-8').decodeToString(bytes);
        Pinyin.composer = new PinyinComposer(PinyinDict.parse(text));
        return Pinyin.composer;
      });
      Pinyin.loading.catch((): void => { Pinyin.loading = null; });
    }
    return Pinyin.loading;
  }
}

/** Caret moves, deletions and clipboard edits a text field takes from the MacBook layout. */
export enum Edit {
  Left, Right, Up, Down, WordLeft, WordRight, LineStart, LineEnd, Start, End,
  DeleteForward, DeleteWordBack, DeleteToLineStart, DeleteToLineEnd, SelectAll, Copy, Cut, Tab,
}

/** What a key does to the field it is bound to. */
export interface KeyboardTarget {
  insert: (text: string) => void;
  backspace: () => void;
  enter: () => void;
  hide: () => void;
  /** Named keys for a terminal (escape, f5, ctrl-c, alt-b…); fields without it get no Esc or F-keys. */
  key?: (name: string) => void;
  /** Text fields: arrows, word and line moves, deletions and the clipboard; `extend` (Shift) moves only the selection's head. */
  edit?: (edit: Edit, extend: boolean) => void;
}

const PLACE_SETTLE_MS = 250;

/** Puts plain text on the system pasteboard; a refusal is ignored. */
export function copyText(text: string): void {
  try {
    pasteboard.getSystemPasteboard().setData(pasteboard.createData(pasteboard.MIMETYPE_TEXT_PLAIN, text))
      .catch((): void => {});
  } catch (_) {}
}

/**
 * Edits a text field's state for the keyboard: inserts at the caret or over
 * the selection (tracked from onTextSelectionChange) and moves the caret.
 */
export class FieldEditor {
  /** The selection: the anchor stays put while Shift-arrows move the head. */
  private anchor: number = 0;
  private head: number = 0;
  private placedAt: number = 0;
  /** What the keyboard last wrote, until the field's text catches up with it. */
  private written: string | null = null;
  private read: () => string;
  private write: (text: string) => void;
  private show: (start: number, end: number) => void;
  private submit: () => void;
  private close: () => void;
  private multiline: boolean;

  constructor(read: () => string, write: (text: string) => void, show: (start: number, end: number) => void,
    submit: () => void, close: () => void, multiline: boolean) {
    this.read = read;
    this.write = write;
    this.show = show;
    this.submit = submit;
    this.close = close;
    this.multiline = multiline;
  }

  /** The field's selection, ordered; the keyboard's own changes come back here and keep their direction. */
  select(start: number, end: number): void {
    const a = Math.min(this.anchor, this.head);
    const b = Math.max(this.anchor, this.head);
    if (a === start && b === end) return;
    // Right after an edit the field re-renders its new text and moves its own
    // caret (often to the end); put the keyboard's back.
    if (Date.now() - this.placedAt < PLACE_SETTLE_MS) {
      this.show(a, b);
      return;
    }
    this.anchor = start;
    this.head = end;
  }

  /**
   * The text to edit. The owner applies onChange on its next render, so a
   * second edit in the same key press (pinyin committed, then punctuation)
   * would read the old text and drop the first.
   */
  private text(): string {
    const value = this.read();
    if (this.written !== null && value !== this.written && Date.now() - this.placedAt < PLACE_SETTLE_MS) return this.written;
    this.written = null;
    return value;
  }

  private from(value: string): number { return Math.max(0, Math.min(this.anchor, this.head, value.length)); }
  private to(value: string): number { return Math.min(value.length, Math.max(this.anchor, this.head)); }

  private place(anchor: number, head: number = anchor): void {
    this.anchor = anchor;
    this.head = head;
    this.placedAt = Date.now();
    setTimeout((): void => { this.show(Math.min(anchor, head), Math.max(anchor, head)); }, 0);
  }

  private replace(value: string, a: number, b: number, text: string): void {
    this.written = value.substring(0, a) + text + value.substring(b);
    this.write(this.written);
    this.place(a + text.length);
  }

  private moved(value: string, edit: Edit, pos: number): number {
    switch (edit) {
      case Edit.Left: return charLeft(value, pos);
      case Edit.Right: return charRight(value, pos);
      case Edit.Up: return lineUp(value, pos);
      case Edit.Down: return lineDown(value, pos);
      case Edit.WordLeft: return wordLeft(value, pos);
      case Edit.WordRight: return wordRight(value, pos);
      case Edit.LineStart: return lineStart(value, pos);
      case Edit.LineEnd: return lineEnd(value, pos);
      case Edit.Start: return 0;
      default: return value.length;
    }
  }

  private apply(edit: Edit, extend: boolean): void {
    const value = this.text();
    const a = this.from(value);
    const b = this.to(value);
    switch (edit) {
      case Edit.SelectAll: this.place(0, value.length); return;
      case Edit.Copy: if (a < b) copyText(value.substring(a, b)); return;
      case Edit.Cut:
        if (a < b) {
          copyText(value.substring(a, b));
          this.replace(value, a, b, '');
        }
        return;
      case Edit.Tab: if (this.multiline) this.replace(value, a, b, '\t'); return;
      case Edit.DeleteForward: this.replace(value, a, a < b ? b : charRight(value, a), ''); return;
      case Edit.DeleteWordBack: this.replace(value, a < b ? a : wordLeft(value, a), b, ''); return;
      case Edit.DeleteToLineStart: {
        const start = lineStart(value, a);
        this.replace(value, a < b ? a : (start === a ? charLeft(value, a) : start), b, '');
        return;
      }
      case Edit.DeleteToLineEnd: {
        const end = lineEnd(value, b);
        this.replace(value, a, a < b ? b : (end === b ? charRight(value, b) : end), '');
        return;
      }
      default: break;
    }
    if (extend) {
      this.place(Math.min(this.anchor, value.length), this.moved(value, edit, Math.min(this.head, value.length)));
      return;
    }
    // A selection collapses to its own edge first, as on a Mac.
    if (a < b && edit === Edit.Left) { this.place(a); return; }
    if (a < b && edit === Edit.Right) { this.place(b); return; }
    const backward = edit === Edit.Left || edit === Edit.Up || edit === Edit.WordLeft || edit === Edit.LineStart;
    this.place(this.moved(value, edit, backward ? a : b));
  }

  target(): KeyboardTarget {
    return {
      insert: (text: string): void => {
        const value = this.text();
        this.replace(value, this.from(value), this.to(value), text);
      },
      backspace: (): void => {
        const value = this.text();
        const a = this.from(value);
        const b = this.to(value);
        if (a === b && a === 0) return;
        this.replace(value, a === b ? charLeft(value, a) : a, b, '');
      },
      enter: (): void => { this.submit(); },
      hide: (): void => { this.close(); },
      edit: (edit: Edit, extend: boolean): void => { this.apply(edit, extend); },
    };
  }
}

enum Page { Letters, Numbers, Symbols }

const LETTER_ROWS: string[][] = [
  ['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'],
  ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'],
  ['z', 'x', 'c', 'v', 'b', 'n', 'm'],
];
const NUMBERS: string[] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
const FUNCTION_KEYS: string[] = ['F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12'];

class NamedKey {
  label: string;
  name: string;
  constructor(label: string, name: string) { this.label = label; this.name = name; }
}

/** The terminal strip: what the terminal's own key bar had, then the function keys. */
const TERMINAL_KEYS: NamedKey[] = [
  new NamedKey('esc', 'escape'), new NamedKey('tab', 'tab'), new NamedKey('←', 'left'), new NamedKey('↓', 'down'),
  new NamedKey('↑', 'up'), new NamedKey('→', 'right'), new NamedKey('home', 'home'), new NamedKey('end', 'end'),
  new NamedKey('pgup', 'pageup'), new NamedKey('pgdn', 'pagedown'),
].concat(FUNCTION_KEYS.map((label: string): NamedKey => new NamedKey(label, label.toLowerCase())));
const ZH_NUMBER_SYMBOLS: string[][] = [['-', '/', '：', '；', '（', '）', '¥', '@', '“', '”'], ['。', '，', '、', '？', '！', '.', '‘', '’']];
const EN_NUMBER_SYMBOLS: string[][] = [['-', '/', ':', ';', '(', ')', '$', '&', '@', '"'], ['.', ',', '?', '!', "'", '%', '*', '#']];
const ZH_MORE_SYMBOLS: string[][] = [['【', '】', '{', '}', '#', '%', '^', '*', '+', '='], ['—', '…', '《', '》', '~', '·', '|', '\\']];
const EN_MORE_SYMBOLS: string[][] = [['[', ']', '{', '}', '#', '%', '^', '*', '+', '='], ['_', '\\', '|', '~', '<', '>', '`', '$']];
const KEY_HEIGHT = 42;
const COMPACT_KEY_HEIGHT = 34;
const DOUBLE_TAP_MS = 350;
/** From this width (vp) the keyboard takes a MacBook's layout. */
const MAC_MIN_WIDTH = 560;
const FN_ARROWS: Map<string, string> = new Map<string, string>([
  ['left', 'home'], ['right', 'end'], ['up', 'pageup'], ['down', 'pagedown'],
]);
/** Control with a letter in a text field: the macOS (Emacs) text bindings. */
const CONTROL_EDITS: Map<string, Edit> = new Map<string, Edit>([
  ['a', Edit.LineStart], ['e', Edit.LineEnd], ['b', Edit.Left], ['f', Edit.Right], ['p', Edit.Up], ['n', Edit.Down],
  ['d', Edit.DeleteForward], ['k', Edit.DeleteToLineEnd],
]);

/** The modifiers that apply to one key press. */
class Held {
  shift: boolean = false;
  ctrl: boolean = false;
  alt: boolean = false;
  cmd: boolean = false;
  fn: boolean = false;
}

class KeyCap {
  label: string;
  constructor(label: string) { this.label = label; }
}

function caps(labels: string[]): KeyCap[] {
  return labels.map((label: string): KeyCap => new KeyCap(label));
}

@ComponentV2
export struct MusterKeyboard {
  @Param @Require target: KeyboardTarget;
  /** What Enter does in this field, shown on the key. */
  @Param enterLabel: string = '换行';
  /** Shorter keys, so the keyboard and its field fit below a half-fold crease. */
  @Param compact: boolean = false;
  @Local page: Page = Page.Letters;
  /** Shift, Ctrl, Alt (option), Command and fn: 0 off, 1 for the next key, 2 locked. */
  @Local shift: number = 0;
  @Local ctrl: number = 0;
  @Local alt: number = 0;
  @Local cmd: number = 0;
  @Local fn: number = 0;
  @Local capsLock: boolean = false;
  /** Wide enough for the MacBook layout. */
  @Local wide: boolean = false;
  @Local composing: string = '';
  @Local candidates: Candidate[] = [];
  @Local ready: boolean = false;
  private lastShiftTap: number = 0;
  private lastCtrlTap: number = 0;
  private lastAltTap: number = 0;
  private lastCmdTap: number = 0;
  private lastFnTap: number = 0;
  private readonly punctuation: ChinesePunctuation = new ChinesePunctuation();

  aboutToAppear(): void {
    try {
      this.wide = this.getUIContext().px2vp(display.getDefaultDisplaySync().width) >= MAC_MIN_WIDTH;
    } catch (_) {}
    if (Pinyin.composer !== null) {
      Pinyin.composer.clear();
      this.ready = true;
      return;
    }
    const context = this.getUIContext().getHostContext();
    if (context === undefined) return;
    Pinyin.load(context).then((composer: PinyinComposer): void => { composer.clear(); this.ready = true; })
      .catch((): void => { this.ready = false; });
  }

  aboutToDisappear(): void {
    keyboardPrefs.keyboardHeight = 0;
    // Leaving a field keeps what was typed: pending pinyin goes in as letters.
    const composer = Pinyin.composer;
    if (composer !== null && composer.active) this.target.insert(composer.commitRaw());
  }

  private chinese(): boolean { return keyboardPrefs.chinese && this.ready; }

  private keyHeight(): number { return this.compact ? COMPACT_KEY_HEIGHT : KEY_HEIGHT; }

  /** A terminal: the keyboard adds Esc, F1–F12, Ctrl and Alt. */
  private raw(): boolean { return this.target.key !== undefined; }

  private refresh(): void {
    const composer = Pinyin.composer;
    this.composing = composer !== null && composer.active ? composer.display() : '';
    this.candidates = composer !== null && composer.composing !== '' ? composer.candidates() : [];
  }

  /** Commits pending pinyin with its best reading before a space, digit or symbol. */
  private flush(): void {
    const composer = Pinyin.composer;
    if (composer !== null && composer.active) {
      this.target.insert(composer.commitFirst());
      this.refresh();
    }
  }

  /** With Ctrl or Alt held, a key goes to the terminal as a named combination. */
  private modified(base: string): boolean {
    const send = this.target.key;
    if (send === undefined || (this.ctrl === 0 && this.alt === 0)) return false;
    this.flush();
    send(`${this.ctrl > 0 ? 'ctrl-' : ''}${this.alt > 0 ? 'alt-' : ''}${base}`);
    if (this.ctrl === 1) this.ctrl = 0;
    if (this.alt === 1) this.alt = 0;
    return true;
  }

  private letter(key: string): void {
    if (this.modified(key)) return;
    const upper = this.shift > 0;
    if (this.shift === 1) this.shift = 0;
    const composer = Pinyin.composer;
    if (this.chinese() && !upper && composer !== null) {
      composer.type(key);
      this.refresh();
      return;
    }
    this.flush();
    this.target.insert(upper ? key.toUpperCase() : key);
  }

  private symbol(text: string): void {
    if (this.modified(text)) return;
    this.flush();
    this.target.insert(text);
  }

  /** Esc and F1–F12, for terminals only. */
  private named(name: string): void {
    if (this.modified(name)) return;
    this.flush();
    this.target.key?.(name);
  }

  private pick(candidate: Candidate): void {
    const composer = Pinyin.composer;
    if (composer === null) return;
    const done = composer.pick(candidate);
    if (done !== null) this.target.insert(done);
    this.refresh();
  }

  private backspace(): void {
    if (this.modified('backspace')) return;
    const composer = Pinyin.composer;
    if (composer !== null && composer.active) {
      composer.backspace();
      this.refresh();
      return;
    }
    this.target.backspace();
  }

  private space(): void {
    if (this.modified('space')) return;
    const composer = Pinyin.composer;
    if (composer !== null && composer.active) { this.flush(); return; }
    this.target.insert(' ');
  }

  private enter(): void {
    if (this.modified('enter')) return;
    const composer = Pinyin.composer;
    // Enter while composing types the letters themselves, as other pinyin keyboards do.
    if (composer !== null && composer.active) {
      this.target.insert(composer.commitRaw());
      this.refresh();
      return;
    }
    this.target.enter();
  }

  /** Tap: the next key only; double tap: locked; tap again: off. */
  private cycle(state: number, last: number): number {
    if (state === 1 && Date.now() - last < DOUBLE_TAP_MS) return 2;
    return state === 0 ? 1 : 0;
  }

  private toggleShift(): void { this.shift = this.cycle(this.shift, this.lastShiftTap); this.lastShiftTap = Date.now(); }
  private toggleCtrl(): void { this.ctrl = this.cycle(this.ctrl, this.lastCtrlTap); this.lastCtrlTap = Date.now(); }
  private toggleAlt(): void { this.alt = this.cycle(this.alt, this.lastAltTap); this.lastAltTap = Date.now(); }
  private toggleCmd(): void { this.cmd = this.cycle(this.cmd, this.lastCmdTap); this.lastCmdTap = Date.now(); }
  private toggleFn(): void { this.fn = this.cycle(this.fn, this.lastFnTap); this.lastFnTap = Date.now(); }

  /** The modifiers for this key press; one-shot ones are released. */
  private take(): Held {
    const held = new Held();
    held.shift = this.shift > 0;
    held.ctrl = this.ctrl > 0;
    held.alt = this.alt > 0;
    held.cmd = this.cmd > 0;
    held.fn = this.fn > 0;
    if (this.shift === 1) this.shift = 0;
    if (this.ctrl === 1) this.ctrl = 0;
    if (this.alt === 1) this.alt = 0;
    if (this.cmd === 1) this.cmd = 0;
    if (this.fn === 1) this.fn = 0;
    return held;
  }

  private macPress(key: MacKey): void {
    if (key.kind === KeyKind.Letter) this.macCharacter(key.id, key.id.toUpperCase(), true);
    else if (key.kind === KeyKind.Symbol) this.macCharacter(key.bottom, key.top, false);
    else this.macAction(key.id);
  }

  private macCharacter(plain: string, shifted: string, letter: boolean): void {
    const held = this.take();
    const upper = held.shift || (letter && this.capsLock);
    const char = upper ? shifted : plain;
    const send = this.target.key;
    if (held.cmd) {
      if (letter) this.command(plain);
      return;
    }
    if (send !== undefined && (held.ctrl || held.alt)) {
      this.flush();
      send(`${held.ctrl ? 'ctrl-' : ''}${held.alt ? 'alt-' : ''}${char}`);
      return;
    }
    if (send === undefined && held.ctrl) {
      if (letter) this.controlEdit(plain);
      return;
    }
    const composer = Pinyin.composer;
    if (composer !== null && this.chinese() && !upper) {
      // Letters compose pinyin; an apostrophe between them separates syllables.
      if (letter || (plain === "'" && composer.composing !== '')) {
        composer.type(plain);
        this.refresh();
        return;
      }
    }
    this.flush();
    this.target.insert(!letter && this.chinese() ? this.punctuation.map(char) : char);
  }

  /** Command with a letter in a text field: select all, copy, cut. Terminals have no such shortcuts. */
  private command(letter: string): void {
    const edit = this.target.edit;
    if (edit === undefined) return;
    this.flush();
    if (letter === 'a') edit(Edit.SelectAll, false);
    else if (letter === 'c') edit(Edit.Copy, false);
    else if (letter === 'x') edit(Edit.Cut, false);
  }

  private controlEdit(letter: string): void {
    const edit = this.target.edit;
    const action = CONTROL_EDITS.get(letter);
    this.flush();
    if (letter === 'h') this.target.backspace();
    else if (edit !== undefined && action !== undefined) edit(action, false);
  }

  private macAction(id: string): void {
    switch (id) {
      case 'shift': this.toggleShift(); return;
      case 'control': this.toggleCtrl(); return;
      case 'option': this.toggleAlt(); return;
      case 'command': this.toggleCmd(); return;
      case 'fn': this.toggleFn(); return;
      // A tap switches language, as Caps Lock does with macOS Pinyin; holding it locks capitals.
      case 'caps': this.toggleLanguage(); return;
      default: break;
    }
    const held = this.take();
    const composer = Pinyin.composer;
    if (id === 'escape' && composer !== null && composer.active) {
      composer.clear();
      this.refresh();
      return;
    }
    const send = this.target.key;
    if (send !== undefined) this.terminalAction(id, held, send); else this.fieldAction(id, held);
  }

  private macLongPress(id: string, repeat: boolean): void {
    if (id === 'delete') this.macAction(id);
    else if (id === 'caps' && !repeat) this.capsLock = !this.capsLock;
  }

  private terminalAction(id: string, held: Held, send: (name: string) => void): void {
    const mods = `${held.ctrl ? 'ctrl-' : ''}${held.alt ? 'alt-' : ''}`;
    const shift = held.shift ? 'shift-' : '';
    if (id === 'delete' && !held.fn && !held.cmd && mods === '') { this.backspace(); return; }
    if (id === 'return' && mods === '') { this.enter(); return; }
    if (id === 'space' && mods === '') { this.space(); return; }
    this.flush();
    if (id === 'delete') send(held.fn ? `${mods}delete` : held.cmd ? 'ctrl-u' : `${mods}backspace`);
    else if (id === 'return') send(`${mods}enter`);
    else if (id === 'space') send(`${mods}space`);
    else if (FN_ARROWS.has(id)) send(this.terminalArrow(id, held, mods));
    else send(`${mods}${shift}${id}`);
  }

  /** Arrows as macOS terminals send them: fn for Home/End/Page Up/Down, ⌘ for the line, ⌥ for words. */
  private terminalArrow(id: string, held: Held, mods: string): string {
    const shift = held.shift ? 'shift-' : '';
    const horizontal = id === 'left' || id === 'right';
    if (held.fn) return `${mods}${shift}${FN_ARROWS.get(id)}`;
    if (held.cmd && horizontal) return id === 'left' ? 'ctrl-a' : 'ctrl-e';
    if (held.alt && !held.ctrl && !held.shift && horizontal) return id === 'left' ? 'alt-b' : 'alt-f';
    return `${mods}${shift}${id}`;
  }

  private fieldAction(id: string, held: Held): void {
    const edit = this.target.edit;
    const composer = Pinyin.composer;
    const composing = composer !== null && composer.active;
    if (id === 'delete') {
      if (composing || edit === undefined) this.backspace();
      else if (held.fn) edit(Edit.DeleteForward, false);
      else if (held.cmd) edit(Edit.DeleteToLineStart, false);
      else if (held.alt) edit(Edit.DeleteWordBack, false);
      else this.backspace();
      return;
    }
    if (id === 'return') { this.enter(); return; }
    if (id === 'space') { this.space(); return; }
    if (id === 'escape') { this.target.hide(); return; }
    if (edit === undefined) return;
    this.flush();
    if (id === 'tab') { edit(Edit.Tab, false); return; }
    const move = this.fieldArrow(id, held);
    if (move !== null) edit(move, held.shift);
  }

  private fieldArrow(id: string, held: Held): Edit | null {
    const pick = (left: Edit, right: Edit, up: Edit, down: Edit): Edit | null =>
      id === 'left' ? left : id === 'right' ? right : id === 'up' ? up : id === 'down' ? down : null;
    if (held.fn || held.cmd) return pick(Edit.LineStart, Edit.LineEnd, Edit.Start, Edit.End);
    if (held.alt) return pick(Edit.WordLeft, Edit.WordRight, Edit.LineStart, Edit.LineEnd);
    return pick(Edit.Left, Edit.Right, Edit.Up, Edit.Down);
  }

  /** A modifier's state: 0 off, 1 for the next key, 2 locked. */
  private level(id: string): number {
    if (id === 'shift') return this.shift;
    if (id === 'control') return this.ctrl;
    if (id === 'option') return this.alt;
    if (id === 'command') return this.cmd;
    if (id === 'fn') return this.fn;
    return 0;
  }

  private lit(key: MacKey): boolean { return key.id === 'return' || this.level(key.id) > 0; }

  /** Return shows what Enter does when it is more than a new line (send, done). */
  private returnHint(): string { return this.enterLabel === '换行' || this.enterLabel === '回车' ? '' : this.enterLabel; }

  private gap(): number { return this.compact ? 4 : 5; }

  private toggleLanguage(): void {
    const composer = Pinyin.composer;
    if (composer !== null && composer.active) {
      this.target.insert(composer.commitRaw());
      this.refresh();
    }
    keyboardPrefs.setChinese(!keyboardPrefs.chinese);
  }

  /** The symbols of a row on the current page, in the current language. */
  private symbols(row: number): KeyCap[] {
    const set = this.page === Page.Numbers ? (this.chinese() ? ZH_NUMBER_SYMBOLS : EN_NUMBER_SYMBOLS) :
      (this.chinese() ? ZH_MORE_SYMBOLS : EN_MORE_SYMBOLS);
    return caps(set[row]);
  }

  build() {
    Column({ space: this.compact ? 4 : 6 }) {
      this.topBar()
      if (this.wide) {
        this.macLayout()
      } else {
        this.phoneLayout()
      }
    }
    .width('100%').padding({ left: 5, right: 5, top: 4, bottom: this.compact ? 4 : 8 })
    .backgroundColor($r('sys.color.background_secondary'))
    .onAreaChange((_: Area, area: Area): void => {
      keyboardPrefs.keyboardHeight = Number(area.height);
      const wide = Number(area.width) >= MAC_MIN_WIDTH;
      if (wide !== this.wide) this.wide = wide;
    })
  }

  @Builder
  phoneLayout() {
    if (this.raw()) { this.functionRow() }
    this.numberRow()
    if (this.page === Page.Letters) {
      this.letterRow(0)
      this.letterRow(1)
      Row({ space: this.compact ? 4 : 5 }) {
        this.shiftKey()
        Blank().width(3)
        ForEach(caps(LETTER_ROWS[2]), (cap: KeyCap): void => { this.charKey(cap.label) }, (cap: KeyCap): string => cap.label)
        Blank().width(3)
        this.deleteKey()
      }
      .width('100%')
    } else {
      this.symbolRows()
    }
    Row({ space: this.compact ? 4 : 5 }) {
      if (this.raw()) {
        this.modifierKey('ctrl')
        this.modifierKey('alt')
      }
      this.pageKey()
      this.languageKey()
      if (!this.raw()) { this.punctuationKey(0) }
      this.spaceKey()
      if (!this.raw()) { this.punctuationKey(1) }
      this.enterKey()
    }
    .width('100%')
  }

  // MacBook layout: rows of MacKeys whose widths are shares of the row, so
  // the columns line up as on the real keyboard. Each key's slot carries the
  // gap as padding; a Row space would skew rows with more keys.
  @Builder
  macLayout() {
    if (this.raw()) { this.macRow(FUNCTION_ROW, true) }
    this.macRow(MAC_ROWS[0], false)
    this.macRow(MAC_ROWS[1], false)
    this.macRow(MAC_ROWS[2], false)
    this.macRow(MAC_ROWS[3], false)
    this.macRow(MAC_ROWS[4], false)
  }

  @Builder
  macRow(keys: MacKey[], short: boolean) {
    Row() {
      ForEach(keys, (key: MacKey): void => { this.macKey(key) }, (key: MacKey): string => `${key.id}-${key.end}`)
    }
    .width('100%').height(short ? Math.round(this.keyHeight() * 0.7) : this.keyHeight())
  }

  @Builder
  macKey(key: MacKey) {
    if (key.kind === KeyKind.Arrows) {
      this.arrowCluster(key.weight)
    } else {
      Stack() {
        if (key.kind === KeyKind.Letter) {
          this.letterCap(key)
        } else if (key.kind === KeyKind.Symbol) {
          this.symbolCap(key)
        } else if (key.kind === KeyKind.Named) {
          this.namedCap(key)
        } else {
          this.actionCap(key)
        }
      }
      .layoutWeight(key.weight).height('100%').padding({ left: this.gap() / 2, right: this.gap() / 2 })
    }
  }

  /** Letters are capitals on the cap, as on a MacBook; they type in lower case without Shift or Caps Lock. */
  @Builder
  letterCap(key: MacKey) {
    Text(key.id.toUpperCase())
      .fontSize(this.compact ? 16 : 18).textAlign(TextAlign.Center).width('100%').height('100%').borderRadius(6)
      .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_primary'))
      .shadow({ radius: 1, color: '#33000000', offsetY: 1 })
      .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
      .onClick((): void => { this.macPress(key); })
  }

  /** The shifted character above the plain one; Shift brings the upper one forward. */
  @Builder
  symbolCap(key: MacKey) {
    Column() {
      Text(key.top).fontSize(this.compact ? 11 : 12)
        .fontColor(this.shift > 0 ? $r('sys.color.font_primary') : $r('sys.color.font_tertiary'))
      Text(key.bottom).fontSize(this.compact ? 14 : 16)
        .fontColor(this.shift > 0 ? $r('sys.color.font_tertiary') : $r('sys.color.font_primary'))
    }
    .justifyContent(FlexAlign.Center).width('100%').height('100%').borderRadius(6)
    .backgroundColor($r('sys.color.comp_background_primary'))
    .shadow({ radius: 1, color: '#33000000', offsetY: 1 })
    .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
    .onClick((): void => { this.macPress(key); })
  }

  @Builder
  namedCap(key: MacKey) {
    Text(key.bottom)
      .fontSize(11).textAlign(TextAlign.Center).width('100%').height('100%').borderRadius(5)
      .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_tertiary'))
      .stateStyles({ pressed: { .opacity(0.7) } })
      .onClick((): void => { this.macAction(key.id); })
  }

  /** Modifiers and editing keys: small legends in the corners, toward the space bar for ⌃ ⌥ ⌘. */
  @Builder
  actionCap(key: MacKey) {
    Column() {
      Row() {
        if (key.id === 'caps') {
          Circle().width(5).height(5).fill(this.capsLock ? '#34C759' : '#40808080')
          Blank()
          Text(this.chinese() ? '中' : '英').fontSize(10).fontColor($r('sys.color.font_secondary'))
        } else if (key.id === 'return') {
          Text(this.returnHint()).fontSize(10).fontColor(Color.White)
        } else if (key.top !== '') {
          Text(key.top).fontSize(key.id === 'fn' || this.compact ? 10 : 12)
            .fontColor(this.lit(key) ? Color.White : $r('sys.color.font_primary'))
        }
      }
      .width('100%').justifyContent(key.end || key.id === 'fn' ? FlexAlign.End : FlexAlign.Start)
      Blank()
      if (key.id === 'fn') {
        this.globe(this.lit(key) ? Color.White : $r('sys.color.font_primary'))
      } else if (key.bottom !== '') {
        Text(key.bottom).fontSize(this.compact ? 10 : 11).maxLines(1)
          .fontColor(this.lit(key) ? Color.White : $r('sys.color.font_primary'))
      }
    }
    .alignItems(key.end ? HorizontalAlign.End : HorizontalAlign.Start)
    .padding({ left: 6, right: 6, top: this.compact ? 2 : 4, bottom: this.compact ? 2 : 4 })
    .width('100%').height('100%').borderRadius(6)
    .backgroundColor(this.lit(key) ? $r('app.color.accent') :
      (key.id === 'space' ? $r('sys.color.comp_background_primary') : $r('sys.color.comp_background_tertiary')))
    .border({ width: this.level(key.id) === 2 ? 2 : 0, color: $r('sys.color.font_primary') })
    .accessibilityText(key.id === 'caps' ? (this.capsLock ? 'Caps lock on' : 'Caps lock') :
      (this.level(key.id) === 2 ? `${key.id} locked` : key.id))
    .stateStyles({ pressed: { .opacity(0.7) } })
    .onClick((): void => { this.macPress(key); })
    .gesture(LongPressGesture({ repeat: key.id === 'delete', duration: 400 })
      .onAction((event: GestureEvent): void => { this.macLongPress(key.id, event.repeat); }))
  }

  /** The 🌐 legend, drawn: the system symbols have no plain globe. */
  @Builder
  globe(color: ResourceColor) {
    Stack() {
      Circle({ width: 13, height: 13 }).fill(Color.Transparent).stroke(color).strokeWidth(1.1)
      Ellipse({ width: 6, height: 13 }).fill(Color.Transparent).stroke(color).strokeWidth(1.1)
      Line().startPoint([0, 6.5]).endPoint([13, 6.5]).width(13).height(13).stroke(color).strokeWidth(1.1)
    }
    .width(13).height(13)
  }

  /** Inverted T: full-height ← and →, half-height ↑ over ↓, as on current MacBooks. */
  @Builder
  arrowCluster(weight: number) {
    Row() {
      this.arrowKey('left', 0, true)
      Column() {
        this.arrowKey('up', 90, false)
        this.arrowKey('down', 270, false)
      }
      .layoutWeight(1).height('100%')
      this.arrowKey('right', 180, true)
    }
    .layoutWeight(weight).height('100%')
  }

  @Builder
  arrowKey(id: string, angle: number, full: boolean) {
    if (full) {
      Stack() { this.arrowCap(id, angle, 11) }
      .layoutWeight(1).height('100%').padding({ left: this.gap() / 2, right: this.gap() / 2 })
    } else {
      Stack() { this.arrowCap(id, angle, 9) }
      .layoutWeight(1).width('100%')
      .padding({ left: this.gap() / 2, right: this.gap() / 2, top: id === 'down' ? 1 : 0, bottom: id === 'up' ? 1 : 0 })
    }
  }

  /** A left-pointing triangle turned to face the key's way; the symbol set has no right-pointing one. */
  @Builder
  arrowCap(id: string, angle: number, size: number) {
    Stack() {
      SymbolGlyph($r('sys.symbol.arrowtriangle_left_fill')).fontSize(size).fontColor([$r('sys.color.font_primary')])
        .rotate({ angle: angle })
    }
    .width('100%').height('100%').borderRadius(5).backgroundColor($r('sys.color.comp_background_tertiary'))
    .accessibilityText(id)
      .stateStyles({ pressed: { .opacity(0.7) } })
      .onClick((): void => { this.macAction(id); })
      .gesture(LongPressGesture({ repeat: true, duration: 400 }).onAction((): void => { this.macAction(id); }))
  }

  @Builder
  topBar() {
    Row({ space: 4 }) {
      if (this.composing !== '') {
        Column({ space: 2 }) {
          Text(this.composing).fontSize(13).fontColor($r('app.color.accent')).maxLines(1).padding({ left: 8 })
          Scroll() {
            Row({ space: 2 }) {
              ForEach(this.candidates, (candidate: Candidate, index: number): void => {
                Text(candidate.word).fontSize(20).padding({ left: 10, right: 10, top: 2, bottom: 2 }).borderRadius(8)
                  .fontColor(index === 0 ? $r('app.color.accent') : $r('sys.color.font_primary'))
                  .fontWeight(index === 0 ? FontWeight.Medium : FontWeight.Normal)
                  .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
                  .onClick((): void => { this.pick(candidate); })
              }, (candidate: Candidate, index: number): string => `${index}-${candidate.word}-${candidate.consumed}`)
            }
          }
          .scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).align(Alignment.Start).width('100%')
        }
        .alignItems(HorizontalAlign.Start).layoutWeight(1)
      } else {
        Text(this.chinese() ? '拼音' : (keyboardPrefs.chinese ? '词库加载中…' : 'English'))
          .fontSize(13).fontColor($r('sys.color.font_secondary')).padding({ left: 8 })
        if (this.wide) {
          Text('caps lock：轻点切换中/英，按住锁定大写').fontSize(11).fontColor($r('sys.color.font_tertiary'))
            .maxLines(1).textOverflow({ overflow: TextOverflow.Ellipsis }).padding({ left: 8 })
        }
        Blank()
        Button({ type: ButtonType.Circle }) {
          SymbolGlyph($r('sys.symbol.keyboard_chevron_compact_up')).fontSize(20).rotate({ angle: 180 })
            .fontColor([$r('sys.color.font_secondary')])
        }
        .width(40).height(34).backgroundColor(Color.Transparent).accessibilityText('Hide keyboard')
        .onClick((): void => { this.target.hide(); })
      }
    }
    .width('100%').height(this.compact ? 36 : 46).alignItems(VerticalAlign.Center)
  }

  @Builder
  functionRow() {
    Scroll() {
      Row({ space: 4 }) {
        ForEach(TERMINAL_KEYS, (key: NamedKey): void => {
          Text(key.label)
            .fontSize(12).textAlign(TextAlign.Center).height(this.compact ? 28 : 32).borderRadius(6)
            .constraintSize({ minWidth: 44 }).padding({ left: 6, right: 6 })
            .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_tertiary'))
            .stateStyles({ pressed: { .opacity(0.7) } })
            .onClick((): void => { this.named(key.name); })
        }, (key: NamedKey): string => key.name)
      }
    }
    .scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).align(Alignment.Start).width('100%')
  }

  @Builder
  numberRow() {
    Row({ space: this.compact ? 4 : 5 }) {
      ForEach(caps(NUMBERS), (cap: KeyCap): void => { this.textKey(cap.label) }, (cap: KeyCap): string => cap.label)
    }
    .width('100%')
  }

  @Builder
  letterRow(row: number) {
    Row({ space: this.compact ? 4 : 5 }) {
      if (row === 1) { Blank().layoutWeight(0.5) }
      ForEach(caps(LETTER_ROWS[row]), (cap: KeyCap): void => { this.charKey(cap.label) }, (cap: KeyCap): string => cap.label)
      if (row === 1) { Blank().layoutWeight(0.5) }
    }
    .width('100%')
  }

  // Builders read state themselves: arguments are copied once and would not follow it.
  @Builder
  symbolRows() {
    Row({ space: this.compact ? 4 : 5 }) {
      ForEach(this.symbols(0), (cap: KeyCap): void => { this.textKey(cap.label) },
        (cap: KeyCap, index: number): string => `${this.page}-${this.chinese()}-${index}-${cap.label}`)
    }
    .width('100%')
    Row({ space: this.compact ? 4 : 5 }) {
      Text(this.page === Page.Numbers ? '#+=' : '符')
        .fontSize(16).textAlign(TextAlign.Center).layoutWeight(1.45).height(this.keyHeight()).borderRadius(8)
        .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_tertiary'))
        .stateStyles({ pressed: { .opacity(0.7) } })
        .onClick((): void => { this.page = this.page === Page.Numbers ? Page.Symbols : Page.Numbers; })
      Blank().width(3)
      ForEach(this.symbols(1), (cap: KeyCap): void => { this.textKey(cap.label) },
        (cap: KeyCap, index: number): string => `${this.page}-${this.chinese()}-${index}-${cap.label}`)
      Blank().width(3)
      this.deleteKey()
    }
    .width('100%')
  }

  @Builder
  charKey(letter: string) {
    Text(this.shift > 0 ? letter.toUpperCase() : letter)
      .fontSize(21).textAlign(TextAlign.Center).layoutWeight(1).height(this.keyHeight()).borderRadius(8)
      .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_primary'))
      .shadow({ radius: 1, color: '#33000000', offsetY: 1 })
      .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
      .onClick((): void => { this.letter(letter); })
  }

  @Builder
  textKey(text: string) {
    Text(text)
      .fontSize(19).textAlign(TextAlign.Center).layoutWeight(1).height(this.keyHeight()).borderRadius(8)
      .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_primary'))
      .shadow({ radius: 1, color: '#33000000', offsetY: 1 })
      .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
      .onClick((): void => { this.symbol(text); })
  }

  @Builder
  spaceKey() {
    Text(this.chinese() ? '拼音' : 'space')
      .fontSize(15).textAlign(TextAlign.Center).layoutWeight(this.raw() ? 3 : 4).height(this.keyHeight()).borderRadius(8)
      .fontColor($r('sys.color.font_secondary')).backgroundColor($r('sys.color.comp_background_primary'))
      .shadow({ radius: 1, color: '#33000000', offsetY: 1 })
      .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
      .onClick((): void => { this.space(); })
  }

  @Builder
  shiftKey() {
    Stack() {
      SymbolGlyph(this.shift === 0 ? $r('sys.symbol.arrowshape_up') : $r('sys.symbol.arrowshape_up_fill')).fontSize(22)
        .fontColor([this.shift === 2 ? $r('app.color.accent') : $r('sys.color.font_primary')])
    }
    .layoutWeight(1.45).height(this.keyHeight()).borderRadius(8).backgroundColor($r('sys.color.comp_background_tertiary'))
    .accessibilityText(this.shift === 2 ? 'Caps lock' : 'Shift')
    .stateStyles({ pressed: { .opacity(0.7) } })
    .onClick((): void => { this.toggleShift(); })
  }

  /** Ctrl or Alt: highlighted while it applies, outlined when locked. */
  @Builder
  modifierKey(name: string) {
    Text(name)
      .fontSize(15).textAlign(TextAlign.Center).layoutWeight(1.1).height(this.keyHeight()).borderRadius(8)
      .fontColor((name === 'ctrl' ? this.ctrl : this.alt) > 0 ? Color.White : $r('sys.color.font_primary'))
      .backgroundColor((name === 'ctrl' ? this.ctrl : this.alt) > 0 ? $r('app.color.accent') : $r('sys.color.comp_background_tertiary'))
      .border({ width: (name === 'ctrl' ? this.ctrl : this.alt) === 2 ? 2 : 0, color: $r('sys.color.font_primary') })
      .accessibilityText((name === 'ctrl' ? this.ctrl : this.alt) === 2 ? `${name} locked` : name)
      .stateStyles({ pressed: { .opacity(0.7) } })
      .onClick((): void => { if (name === 'ctrl') this.toggleCtrl(); else this.toggleAlt(); })
  }

  @Builder
  pageKey() {
    Text(this.page === Page.Letters ? '符' : 'ABC')
      .fontSize(16).textAlign(TextAlign.Center).layoutWeight(1.2).height(this.keyHeight()).borderRadius(8)
      .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_tertiary'))
      .accessibilityText(this.page === Page.Letters ? 'Symbols' : 'Letters')
      .stateStyles({ pressed: { .opacity(0.7) } })
      .onClick((): void => { this.page = this.page === Page.Letters ? Page.Numbers : Page.Letters; })
  }

  /** 中/英 with the current language emphasized. */
  @Builder
  languageKey() {
    Text() {
      Span('中').fontColor(keyboardPrefs.chinese ? $r('sys.color.font_primary') : $r('sys.color.font_tertiary'))
        .fontWeight(keyboardPrefs.chinese ? FontWeight.Bold : FontWeight.Normal)
      Span('/').fontColor($r('sys.color.font_tertiary'))
      Span('英').fontColor(keyboardPrefs.chinese ? $r('sys.color.font_tertiary') : $r('sys.color.font_primary'))
        .fontWeight(keyboardPrefs.chinese ? FontWeight.Normal : FontWeight.Bold)
    }
    .fontSize(16).textAlign(TextAlign.Center).layoutWeight(1.2).height(this.keyHeight()).borderRadius(8)
    .backgroundColor($r('sys.color.comp_background_tertiary'))
    .accessibilityText(keyboardPrefs.chinese ? 'Chinese, switch to English' : 'English, switch to Chinese')
    .stateStyles({ pressed: { .opacity(0.7) } })
    .onClick((): void => { this.toggleLanguage(); })
  }

  @Builder
  punctuationKey(which: number) {
    Text(this.chinese() ? (which === 0 ? '，' : '。') : (which === 0 ? ',' : '.'))
      .fontSize(20).textAlign(TextAlign.Center).layoutWeight(0.9).height(this.keyHeight()).borderRadius(8)
      .fontColor($r('sys.color.font_primary')).backgroundColor($r('sys.color.comp_background_primary'))
      .shadow({ radius: 1, color: '#33000000', offsetY: 1 })
      .stateStyles({ pressed: { .backgroundColor($r('sys.color.interactive_pressed')) } })
      .onClick((): void => { this.symbol(this.chinese() ? (which === 0 ? '，' : '。') : (which === 0 ? ',' : '.')); })
  }

  @Builder
  enterKey() {
    Text(this.enterLabel)
      .fontSize(16).textAlign(TextAlign.Center).layoutWeight(1.6).height(this.keyHeight()).borderRadius(8)
      .fontColor(Color.White).backgroundColor($r('app.color.accent'))
      .stateStyles({ pressed: { .opacity(0.7) } })
      .onClick((): void => { this.enter(); })
  }

  @Builder
  deleteKey() {
    Stack() {
      SymbolGlyph($r('sys.symbol.delete_left')).fontSize(22).fontColor([$r('sys.color.font_primary')])
    }
    .layoutWeight(1.45).height(this.keyHeight()).borderRadius(8).backgroundColor($r('sys.color.comp_background_tertiary'))
    .accessibilityText('Delete')
    .stateStyles({ pressed: { .opacity(0.7) } })
    .onClick((): void => { this.backspace(); })
    .gesture(LongPressGesture({ repeat: true, duration: 350 }).onAction((): void => { this.backspace(); }))
  }
}
