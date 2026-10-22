// Run with Node and the TypeScript compiler bundled with DevEco Studio.
// Tests the chat Markdown parser (entry/src/main/ets/view/MarkdownParse.ts).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.TYPESCRIPT_PATH || '/Applications/DevEco-Studio.app/Contents/tools/ohpm/node_modules/typescript');
const file = path.join(__dirname, '../entry/src/main/ets/view/MarkdownParse.ts');
const code = ts.transpileModule(fs.readFileSync(file, 'utf8'),
  { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } }).outputText;
const md = {};
vm.runInNewContext(code, { exports: md });
// JSON round trip: arrays made in the vm context have that realm's prototypes.
const plain = value => JSON.parse(JSON.stringify(value));
const blocks = text => plain(md.parseMarkdown(text).map(b => [b.kind, b.text, b.level]));
const spans = text => plain(md.parseInline(text).map(s => s.kind === 'link' ? [s.kind, s.text, s.url] : [s.kind, s.text]));
let count = 0;
function test(name, run) { run(); count++; console.log(`ok ${count} - ${name}`); }

test('paragraphs keep their line breaks and split on blank lines', () => {
  assert.deepEqual(blocks('one\ntwo\n\nthree'), [['paragraph', 'one\ntwo', 0], ['paragraph', 'three', 0]]);
});
test('fenced code is kept verbatim, including Markdown inside it', () => {
  assert.deepEqual(blocks('Run:\n```sh\n# not a heading\n- not an item\n```\nDone'),
    [['paragraph', 'Run:', 0], ['code', '# not a heading\n- not an item', 0], ['paragraph', 'Done', 0]]);
});
test('an unclosed fence runs to the end', () => {
  assert.deepEqual(blocks('```\na\nb'), [['code', 'a\nb', 0]]);
});
test('headings, quotes, rules and list items', () => {
  assert.deepEqual(blocks('## Plan\n> note\n---\n- a\n  * b\n3. c'), [
    ['heading', 'Plan', 2], ['quote', 'note', 0], ['rule', '', 0],
    ['item', '• a', 0], ['item', '• b', 1], ['item', '3. c', 0]]);
});
test('CRLF input parses like LF', () => {
  assert.deepEqual(blocks('a\r\n\r\nb'), [['paragraph', 'a', 0], ['paragraph', 'b', 0]]);
});
test('inline code, bold and links', () => {
  assert.deepEqual(spans('Use `cargo test` and **check** [the docs](https://example.com/x).'), [
    ['text', 'Use '], ['code', 'cargo test'], ['text', ' and '], ['bold', 'check'], ['text', ' '],
    ['link', 'the docs', 'https://example.com/x'], ['text', '.']]);
});
test('images stay links and are never loaded', () => {
  assert.deepEqual(spans('![diagram](https://example.com/a.png)'), [['link', 'diagram', 'https://example.com/a.png']]);
  assert.deepEqual(spans('![](https://example.com/a.png)'), [['link', 'Image', 'https://example.com/a.png']]);
});
test('file links are kept for the core to resolve', () => {
  assert.deepEqual(spans('[main.rs](src/main.rs:12)'), [['link', 'main.rs', 'src/main.rs:12']]);
});
test('the parser is reentrant across calls (global regex state)', () => {
  assert.deepEqual(spans('`a`'), [['code', 'a']]);
  assert.deepEqual(spans('`a`'), [['code', 'a']]);
  assert.deepEqual(spans(''), [['text', '']]);
});
console.log(`1..${count}`);
