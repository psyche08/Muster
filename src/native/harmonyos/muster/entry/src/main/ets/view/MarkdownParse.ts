// Markdown blocks and inline spans for ChatMarkdown. Text only: no HTML,
// no images, no fetching.
export class MdBlock {
  kind: string = 'paragraph'; // paragraph | heading | item | quote | code | rule
  text: string = '';
  level: number = 0;
}

export class MdSpan {
  kind: string = 'text'; // text | code | bold | link
  text: string = '';
  url: string = '';
}

const FENCE = new RegExp('^\\s*(```|~~~)');
const HEADING = new RegExp('^(#{1,6})\\s+(.*)$');
const ITEM = new RegExp('^(\\s*)([-*+]|\\d+[.)])\\s+(.*)$');
const QUOTE = new RegExp('^>\\s?(.*)$');
const RULE = new RegExp('^\\s*([-*_])(\\s*\\1){2,}\\s*$');
const INLINE = new RegExp('(`[^`]+`)|(\\*\\*[^*]+\\*\\*)|(!?\\[[^\\]]*\\]\\([^)\\s]+\\))', 'g');
const LINK = new RegExp('^!?\\[([^\\]]*)\\]\\(([^)\\s]+)\\)$');

export function parseMarkdown(text: string): MdBlock[] {
  const blocks: MdBlock[] = [];
  const lines = text.replace(new RegExp('\\r\\n', 'g'), '\n').split('\n');
  let paragraph: string[] = [];
  const flush = (): void => {
    if (paragraph.length > 0) {
      const block = new MdBlock();
      block.text = paragraph.join('\n');
      blocks.push(block);
      paragraph = [];
    }
  };
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (FENCE.test(line)) {
      flush();
      const code: string[] = [];
      i++;
      while (i < lines.length && !FENCE.test(lines[i])) { code.push(lines[i]); i++; }
      const block = new MdBlock();
      block.kind = 'code';
      block.text = code.join('\n');
      blocks.push(block);
      i++;
      continue;
    }
    const heading = HEADING.exec(line);
    const item = ITEM.exec(line);
    const quote = QUOTE.exec(line);
    if (line.trim() === '') {
      flush();
    } else if (RULE.test(line)) {
      flush();
      const block = new MdBlock();
      block.kind = 'rule';
      blocks.push(block);
    } else if (heading !== null) {
      flush();
      const block = new MdBlock();
      block.kind = 'heading';
      block.level = heading[1].length;
      block.text = heading[2];
      blocks.push(block);
    } else if (item !== null) {
      flush();
      const block = new MdBlock();
      block.kind = 'item';
      block.level = Math.floor(item[1].length / 2);
      const marker = item[2];
      block.text = (marker === '-' || marker === '*' || marker === '+' ? '•' : marker) + ' ' + item[3];
      blocks.push(block);
    } else if (quote !== null) {
      flush();
      const block = new MdBlock();
      block.kind = 'quote';
      block.text = quote[1];
      blocks.push(block);
    } else {
      paragraph.push(line);
    }
    i++;
  }
  flush();
  return blocks;
}

export function parseInline(text: string): MdSpan[] {
  const spans: MdSpan[] = [];
  let last = 0;
  INLINE.lastIndex = 0;
  let match = INLINE.exec(text);
  while (match !== null) {
    if (match.index > last) {
      const plain = new MdSpan();
      plain.text = text.substring(last, match.index);
      spans.push(plain);
    }
    const token = match[0];
    const span = new MdSpan();
    if (token.startsWith('`')) {
      span.kind = 'code';
      span.text = token.substring(1, token.length - 1);
    } else if (token.startsWith('**')) {
      span.kind = 'bold';
      span.text = token.substring(2, token.length - 2);
    } else {
      const link = LINK.exec(token);
      span.kind = 'link';
      span.text = link !== null && link[1] !== '' ? link[1] : (token.startsWith('!') ? 'Image' : token);
      span.url = link !== null ? link[2] : '';
    }
    spans.push(span);
    last = match.index + token.length;
    match = INLINE.exec(text);
  }
  if (last < text.length || spans.length === 0) {
    const plain = new MdSpan();
    plain.text = text.substring(last);
    spans.push(plain);
  }
  return spans;
}
