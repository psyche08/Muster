// The keyboard's wide layout follows a MacBook (US ANSI): the same keys in
// the same rows with the same relative widths, so every row spans 14.5 key
// units and the columns line up. Pure data, so the host tests check it.

export enum KeyKind {
  /** Types its letter; the cap shows it in upper case, as a MacBook's does. */
  Letter,
  /** Types `bottom`, or `top` with Shift. */
  Symbol,
  /** Escape and F1–F12: a centred label. */
  Named,
  /** Modifiers, editing keys and the space bar: small corner legends. */
  Action,
  /** The inverted-T arrow cluster, three units wide. */
  Arrows,
}

export class MacKey {
  /** The letter, the unshifted character, or the action's name. */
  id: string = '';
  kind: KeyKind = KeyKind.Action;
  /** Width in key units. */
  weight: number = 1;
  top: string = '';
  bottom: string = '';
  /** Legends sit on the right edge: keys right of the letters, and left-hand modifiers by the space bar. */
  end: boolean = false;
}

export const ROW_UNITS = 14.5;

function key(id: string, kind: KeyKind, weight: number, top: string, bottom: string, end: boolean): MacKey {
  const k = new MacKey();
  k.id = id;
  k.kind = kind;
  k.weight = weight;
  k.top = top;
  k.bottom = bottom;
  k.end = end;
  return k;
}

function letters(text: string): MacKey[] {
  return Array.from(text).map((c: string): MacKey => key(c, KeyKind.Letter, 1, '', c, false));
}

/** Pairs of unshifted then shifted characters. */
function symbols(pairs: string): MacKey[] {
  const out: MacKey[] = [];
  const chars = Array.from(pairs);
  for (let i = 0; i + 1 < chars.length; i += 2) out.push(key(chars[i], KeyKind.Symbol, 1, chars[i + 1], chars[i], false));
  return out;
}

function action(id: string, weight: number, top: string, bottom: string, end: boolean): MacKey {
  return key(id, KeyKind.Action, weight, top, bottom, end);
}

/** Esc and F1–F12; a MacBook's Touch ID slot is shared out among the F-keys. */
export const FUNCTION_ROW: MacKey[] = [action('escape', 1.5, '', 'esc', false)].concat(
  [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].map((n: number): MacKey => key(`f${n}`, KeyKind.Named, 13 / 12, '', `F${n}`, false)));

export const MAC_ROWS: MacKey[][] = [
  symbols('`~1!2@3#4$5%6^7&8*9(0)-_=+').concat([action('delete', 1.5, '', 'delete', true)]),
  [action('tab', 1.5, '', 'tab', false)].concat(letters('qwertyuiop'), symbols('[{]}\\|')),
  [action('caps', 1.75, '', 'caps lock', false)].concat(letters('asdfghjkl'), symbols(';:\'"'),
    [action('return', 1.75, '', 'return', true)]),
  [action('shift', 2.25, '⇧', 'shift', false)].concat(letters('zxcvbnm'), symbols(',<.>/?'),
    [action('shift', 2.25, '⇧', 'shift', true)]),
  [
    action('fn', 1, 'fn', '', false),
    action('control', 1, '⌃', 'control', true),
    action('option', 1, '⌥', 'option', true),
    action('command', 1.25, '⌘', 'command', true),
    action('space', 5, '', '', false),
    action('command', 1.25, '⌘', 'command', false),
    action('option', 1, '⌥', 'option', false),
    key('arrows', KeyKind.Arrows, 3, '', '', false),
  ],
];

/** What the ASCII punctuation types in Chinese, as macOS Pinyin does. */
const CHINESE: Map<string, string> = new Map<string, string>([
  ['`', '·'], ['~', '～'], ['!', '！'], ['$', '￥'], ['^', '……'], ['(', '（'], [')', '）'], ['_', '——'],
  ['[', '【'], [']', '】'], ['{', '「'], ['}', '」'], ['\\', '、'], [';', '；'], [':', '：'],
  [',', '，'], ['.', '。'], ['<', '《'], ['>', '》'], ['?', '？'],
]);

/** Chinese punctuation for one keyboard; quotation marks alternate between opening and closing. */
export class ChinesePunctuation {
  private single: boolean = false;
  private double: boolean = false;

  map(char: string): string {
    if (char === '\'') {
      this.single = !this.single;
      return this.single ? '‘' : '’';
    }
    if (char === '"') {
      this.double = !this.double;
      return this.double ? '“' : '”';
    }
    return CHINESE.get(char) ?? char;
  }
}
