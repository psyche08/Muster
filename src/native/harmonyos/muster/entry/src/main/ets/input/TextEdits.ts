// Caret arithmetic for the keyboard's arrow, word and line keys, on plain
// text: lines are the text's own lines (not soft wraps), and offsets are
// UTF-16 indices that never split a surrogate pair. No platform calls, so the
// host tests run it as is.

function isLowSurrogate(value: string, index: number): boolean {
  const code = value.charCodeAt(index);
  return code >= 0xDC00 && code <= 0xDFFF;
}

export function charLeft(value: string, pos: number): number {
  if (pos <= 0) return 0;
  return pos >= 2 && isLowSurrogate(value, pos - 1) ? pos - 2 : pos - 1;
}

export function charRight(value: string, pos: number): number {
  if (pos >= value.length) return value.length;
  return pos + 1 < value.length && isLowSurrogate(value, pos + 1) ? pos + 2 : pos + 1;
}

export function lineStart(value: string, pos: number): number {
  return pos <= 0 ? 0 : value.lastIndexOf('\n', pos - 1) + 1;
}

export function lineEnd(value: string, pos: number): number {
  const index = value.indexOf('\n', pos);
  return index < 0 ? value.length : index;
}

/** The same column on the line above, or the start of the text from the first line. */
export function lineUp(value: string, pos: number): number {
  const start = lineStart(value, pos);
  if (start === 0) return 0;
  return Math.min(lineStart(value, start - 1) + pos - start, start - 1);
}

/** The same column on the line below, or the end of the text from the last line. */
export function lineDown(value: string, pos: number): number {
  const end = lineEnd(value, pos);
  if (end === value.length) return value.length;
  return Math.min(end + 1 + pos - lineStart(value, pos), lineEnd(value, end + 1));
}

const SEPARATOR = new RegExp('[\\s.,;:!?\'"`~@#$%^&*()\\[\\]{}<>/\\\\|+=\\-，。；：！？、（）【】《》「」“”‘’…—·]');

function isWord(value: string, index: number): boolean {
  return !SEPARATOR.test(value.charAt(index));
}

/** Back over separators, then to the start of the word before them. */
export function wordLeft(value: string, pos: number): number {
  let index = Math.min(pos, value.length);
  while (index > 0 && !isWord(value, index - 1)) index--;
  while (index > 0 && isWord(value, index - 1)) index--;
  return index;
}

/** On over separators, then to the end of the word after them. */
export function wordRight(value: string, pos: number): number {
  let index = Math.max(pos, 0);
  while (index < value.length && !isWord(value, index)) index++;
  while (index < value.length && isWord(value, index)) index++;
  return index;
}
