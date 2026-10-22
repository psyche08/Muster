// Run with Node and the TypeScript compiler bundled with DevEco Studio.
// Tests the keyboard's MacBook layout and its caret arithmetic.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.TYPESCRIPT_PATH || '/Applications/DevEco-Studio.app/Contents/tools/ohpm/node_modules/typescript');
const root = path.join(__dirname, '../entry/src/main/ets/input');
function load(file) {
  const code = ts.transpileModule(fs.readFileSync(path.join(root, file), 'utf8'),
    { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } }).outputText;
  const exports = {};
  vm.runInNewContext(code, { exports });
  return exports;
}
const mac = load('MacLayout.ts');
const edit = load('TextEdits.ts');
let count = 0;
function test(name, run) { run(); count++; console.log(`ok ${count} - ${name}`); }
const units = row => row.reduce((sum, key) => sum + key.weight, 0);
const ids = row => row.map(key => key.id).join(' ');

test('every row spans a MacBook row, so the columns line up', () => {
  for (const row of [mac.FUNCTION_ROW, ...mac.MAC_ROWS]) assert.ok(Math.abs(units(row) - mac.ROW_UNITS) < 1e-9, ids(row));
});
test('the rows hold the MacBook keys in order', () => {
  assert.equal(ids(mac.MAC_ROWS[0]), '` 1 2 3 4 5 6 7 8 9 0 - = delete');
  assert.equal(ids(mac.MAC_ROWS[1]), 'tab q w e r t y u i o p [ ] \\');
  assert.equal(ids(mac.MAC_ROWS[2]), "caps a s d f g h j k l ; ' return");
  assert.equal(ids(mac.MAC_ROWS[3]), 'shift z x c v b n m , . / shift');
  assert.equal(ids(mac.MAC_ROWS[4]), 'fn control option command space command option arrows');
  assert.equal(ids(mac.FUNCTION_ROW), 'escape f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12');
});
test('symbol keys carry their shifted character', () => {
  const shifted = mac.MAC_ROWS[0].filter(k => k.kind === mac.KeyKind.Symbol).map(k => k.top).join('');
  assert.equal(shifted, '~!@#$%^&*()_+');
  assert.equal(mac.MAC_ROWS[3].find(k => k.id === '/').top, '?');
});
test('Chinese punctuation follows macOS Pinyin, quotes alternate', () => {
  const zh = new mac.ChinesePunctuation();
  assert.deepEqual([',', '.', '\\', '$', '^', '<', '[', '1', 'a'].map(c => zh.map(c)),
    ['，', '。', '、', '￥', '……', '《', '【', '1', 'a']);
  assert.deepEqual(['"', '"', '\'', '\''].map(c => zh.map(c)), ['“', '”', '‘', '’']);
});
test('arrows move by character without splitting surrogate pairs', () => {
  const text = 'a😀b';
  assert.equal(edit.charRight(text, 1), 3);
  assert.equal(edit.charLeft(text, 3), 1);
  assert.equal(edit.charLeft(text, 0), 0);
  assert.equal(edit.charRight(text, 4), 4);
});
test('up and down keep the column, clamped to shorter lines', () => {
  const text = 'hello\nhi\nworld';
  assert.equal(edit.lineDown(text, 4), 8);   // hell|o → hi|
  assert.equal(edit.lineDown(text, 8), 11);  // hi| → wo|rld
  assert.equal(edit.lineUp(text, 11), 8);
  assert.equal(edit.lineUp(text, 13), 8);
  assert.equal(edit.lineUp(text, 8), 2);
  assert.equal(edit.lineUp(text, 2), 0);
  assert.equal(edit.lineDown(text, 12), text.length);
});
test('line start and end are the text lines', () => {
  const text = '\nab\ncd';
  assert.equal(edit.lineStart(text, 0), 0);
  assert.equal(edit.lineStart(text, 3), 1);
  assert.equal(edit.lineEnd(text, 1), 3);
  assert.equal(edit.lineEnd(text, 5), 6);
});
test('words skip separators then a word, Chinese punctuation included', () => {
  const text = 'git push, origin；你好 world';
  assert.equal(edit.wordLeft(text, text.length), text.length - 5);
  assert.equal(edit.wordLeft(text, 16), 10);
  assert.equal(edit.wordRight(text, 8), 16);
  assert.equal(edit.wordRight(text, 0), 3);
  assert.equal(edit.wordLeft(text, 0), 0);
});
console.log(`1..${count}`);
