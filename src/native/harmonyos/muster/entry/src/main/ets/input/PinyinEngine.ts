// Pinyin to Chinese for the in-app keyboard: splits typed letters into
// syllables (full ones, a trailing partial one, or bare initials as an
// abbreviation), looks words up in the bundled dictionary and composes a
// sentence when no single word covers everything. Pure text in, text out:
// no platform calls, so the host tests run it as is.

export class PinyinEntry {
  syllables: string[] = [];
  word: string = '';
  freq: number = 0;
}

/** One piece of the typed letters: a whole syllable, or a prefix of one. */
export class Segment {
  text: string = '';
  full: boolean = true;
  start: number = 0;
  end: number = 0;
}

export class Candidate {
  word: string = '';
  /** How many typed characters (letters and apostrophes) the word replaces. */
  consumed: number = 0;
}

const INITIALS: string[] = ['b', 'p', 'm', 'f', 'd', 't', 'n', 'l', 'g', 'k', 'h', 'j', 'q', 'x',
  'zh', 'ch', 'sh', 'r', 'z', 'c', 's', 'y', 'w'];
const MAX_CANDIDATES = 80;

export class PinyinDict {
  readonly syllables: Set<string> = new Set<string>();
  private readonly prefixes: Set<string> = new Set<string>();
  /** First letter of each syllable → entries, highest frequency first. */
  private readonly bySignature: Map<string, PinyinEntry[]> = new Map<string, PinyinEntry[]>();

  /** Lines of "syl'syl<TAB>word<TAB>frequency"; "#" starts a comment. */
  static parse(text: string): PinyinDict {
    const dict = new PinyinDict();
    for (const line of text.split('\n')) {
      if (line === '' || line.startsWith('#')) continue;
      const parts = line.split('\t');
      if (parts.length < 3) continue;
      const entry = new PinyinEntry();
      entry.syllables = parts[0].split("'");
      entry.word = parts[1];
      entry.freq = Number(parts[2]) || 1;
      if (entry.syllables.length === 0 || entry.syllables.length !== Array.from(entry.word).length) continue;
      for (const syllable of entry.syllables) {
        if (!dict.syllables.has(syllable)) {
          dict.syllables.add(syllable);
          for (let i = 1; i <= syllable.length; i++) dict.prefixes.add(syllable.substring(0, i));
        }
      }
      const signature = entry.syllables.map((s: string): string => s.charAt(0)).join('');
      const bucket = dict.bySignature.get(signature);
      if (bucket === undefined) dict.bySignature.set(signature, [entry]); else bucket.push(entry);
    }
    dict.bySignature.forEach((bucket: PinyinEntry[]): void => {
      bucket.sort((a: PinyinEntry, b: PinyinEntry): number => b.freq - a.freq);
    });
    return dict;
  }

  isPrefix(text: string): boolean { return this.prefixes.has(text); }

  /** Entries matching the segments: equal syllables for full ones, prefixes for partial ones. */
  match(segments: Segment[]): PinyinEntry[] {
    const bucket = this.bySignature.get(segments.map((s: Segment): string => s.text.charAt(0)).join(''));
    if (bucket === undefined) return [];
    return bucket.filter((entry: PinyinEntry): boolean => segments.every((segment: Segment, i: number): boolean =>
      segment.full ? entry.syllables[i] === segment.text : entry.syllables[i].startsWith(segment.text)));
  }
}

class Step {
  cost: number = 0;
  segments: Segment[] = [];
}

/**
 * Splits lowercase letters into syllables. An apostrophe forces a break.
 * Prefers whole syllables, then a partial final one, then bare initials
 * (abbreviations such as "zg" for zhong'guo). Null when the letters cannot
 * be read as pinyin.
 */
export function segment(dict: PinyinDict, letters: string): Segment[] | null {
  const out: Segment[] = [];
  let offset = 0;
  for (const chunk of letters.split("'")) {
    if (chunk !== '') {
      const parts = segmentChunk(dict, chunk, offset);
      if (parts === null) return null;
      out.push(...parts);
    }
    offset += chunk.length + 1;
  }
  return out.length > 0 ? out : null;
}

function segmentChunk(dict: PinyinDict, chunk: string, offset: number): Segment[] | null {
  const best: (Step | null)[] = new Array<Step | null>(chunk.length + 1).fill(null);
  best[0] = new Step();
  for (let i = 0; i < chunk.length; i++) {
    const from = best[i];
    if (from === null) continue;
    for (let j = i + 1; j <= Math.min(chunk.length, i + 6); j++) {
      const piece = chunk.substring(i, j);
      let cost = 0;
      let full = true;
      if (dict.syllables.has(piece)) {
        cost = 1;
      } else if (j === chunk.length && dict.isPrefix(piece)) {
        cost = 1.5;
        full = false;
      } else if (INITIALS.indexOf(piece) >= 0) {
        cost = 3;
        full = false;
      } else {
        continue;
      }
      const total = from.cost + cost;
      const current = best[j];
      // Ties keep the earlier, longer first syllable ("xian", not "xi'an").
      if (current === null || total < current.cost) {
        const step = new Step();
        step.cost = total;
        const seg = new Segment();
        seg.text = piece;
        seg.full = full;
        seg.start = offset + i;
        seg.end = offset + j;
        step.segments = from.segments.concat([seg]);
        best[j] = step;
      }
    }
  }
  const done = best[chunk.length];
  return done === null ? null : done.segments;
}

/** The typing state of one Chinese input: letters being composed, and words already picked from them. */
export class PinyinComposer {
  private readonly dict: PinyinDict;
  /** Letters (and apostrophes) not yet turned into words. */
  composing: string = '';
  /** Words picked from the start of the letters, committed once nothing is left. */
  picked: string = '';
  private readonly learned: Map<string, number> = new Map<string, number>();

  constructor(dict: PinyinDict) { this.dict = dict; }

  get active(): boolean { return this.composing !== '' || this.picked !== ''; }

  /** The composing letters with syllable breaks, as the candidate bar shows them. */
  display(): string {
    const segments = segment(this.dict, this.composing);
    const letters = segments === null ? this.composing : segments.map((s: Segment): string => s.text).join("'");
    return this.picked + letters;
  }

  type(letter: string): void { this.composing += letter; }

  /** Removes the last letter, or the last picked word when no letters are left. */
  backspace(): void {
    if (this.composing !== '') {
      this.composing = this.composing.substring(0, this.composing.length - 1);
    } else if (this.picked !== '') {
      const chars = Array.from(this.picked);
      chars.pop();
      this.picked = chars.join('');
    }
  }

  clear(): void { this.composing = ''; this.picked = ''; }

  private score(entry: PinyinEntry): number {
    return entry.freq * (1 + (this.learned.get(entry.word) ?? 0));
  }

  private ranked(entries: PinyinEntry[]): PinyinEntry[] {
    return entries.slice().sort((a: PinyinEntry, b: PinyinEntry): number => this.score(b) - this.score(a));
  }

  /** Words for the composing letters: whole matches first, then a composed sentence, then shorter words. */
  candidates(): Candidate[] {
    const segments = segment(this.dict, this.composing);
    if (segments === null) return [];
    const out: Candidate[] = [];
    const seen: Set<string> = new Set<string>();
    const add = (word: string, count: number): void => {
      if (seen.has(word) || out.length >= MAX_CANDIDATES) return;
      seen.add(word);
      const candidate = new Candidate();
      candidate.word = word;
      candidate.consumed = this.consumedBy(segments, count);
      out.push(candidate);
    };
    const whole = this.ranked(this.dict.match(segments));
    const sentence = segments.length > 1 ? this.sentence(segments) : '';
    if (whole.length === 0 && sentence !== '') add(sentence, segments.length);
    // An abbreviation's best guesses come first only when they are common words.
    for (const entry of whole.slice(0, 8)) add(entry.word, segments.length);
    if (whole.length > 0 && sentence !== '') add(sentence, segments.length);
    for (const entry of whole.slice(8)) add(entry.word, segments.length);
    for (let count = segments.length - 1; count >= 1; count--) {
      for (const entry of this.ranked(this.dict.match(segments.slice(0, count)))) add(entry.word, count);
    }
    return out;
  }

  /** Characters of the composing letters covered by the first `count` segments (and a following apostrophe). */
  private consumedBy(segments: Segment[], count: number): number {
    let end = segments[count - 1].end;
    while (end < this.composing.length && this.composing.charAt(end) === "'") end++;
    return end;
  }

  /** Greedy longest-word cover of all segments, or "" when some segment has no character at all. */
  private sentence(segments: Segment[]): string {
    let text = '';
    let i = 0;
    while (i < segments.length) {
      let chosen = '';
      let length = 0;
      for (let k = Math.min(4, segments.length - i); k >= 1 && chosen === ''; k--) {
        const best = this.ranked(this.dict.match(segments.slice(i, i + k)));
        if (best.length > 0) { chosen = best[0].word; length = k; }
      }
      if (chosen === '') return '';
      text += chosen;
      i += length;
    }
    return text;
  }

  /**
   * Picks a candidate. Returns the text to commit when the composition is
   * finished, or null while letters remain to be turned into words.
   */
  pick(candidate: Candidate): string | null {
    this.learned.set(candidate.word, (this.learned.get(candidate.word) ?? 0) + 1);
    this.picked += candidate.word;
    this.composing = this.composing.substring(candidate.consumed);
    if (this.composing.replace(new RegExp("'", 'g'), '') !== '') return null;
    const text = this.picked;
    this.clear();
    return text;
  }

  /** Commits whatever is pending: picked words plus the letters as typed. */
  commitRaw(): string {
    const text = this.picked + this.composing.replace(new RegExp("'", 'g'), '');
    this.clear();
    return text;
  }

  /** Commits the first candidate (space), or the raw letters when there is none. */
  commitFirst(): string {
    while (this.composing !== '') {
      const first = this.candidates()[0];
      if (first === undefined) return this.commitRaw();
      const done = this.pick(first);
      if (done !== null) return done;
    }
    return this.commitRaw();
  }
}
