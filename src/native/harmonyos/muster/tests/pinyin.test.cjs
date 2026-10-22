// Run with Node and the TypeScript compiler bundled with DevEco Studio.
// Tests the keyboard's pinyin engine against the bundled dictionary.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.TYPESCRIPT_PATH || '/Applications/DevEco-Studio.app/Contents/tools/ohpm/node_modules/typescript');
const root = path.join(__dirname, '../entry/src/main');
const code = ts.transpileModule(fs.readFileSync(path.join(root, 'ets/input/PinyinEngine.ts'), 'utf8'),
  { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } }).outputText;
const py = {};
vm.runInNewContext(code, { exports: py });
const started = Date.now();
const dict = py.PinyinDict.parse(fs.readFileSync(path.join(root, 'resources/rawfile/pinyin.txt'), 'utf8'));
const parseMs = Date.now() - started;
const plain = value => JSON.parse(JSON.stringify(value));
const split = letters => plain((py.segment(dict, letters) ?? []).map(s => (s.full ? '' : '~') + s.text));
const words = letters => { const c = new py.PinyinComposer(dict); c.composing = letters; return c.candidates().map(x => x.word); };
let count = 0;
function test(name, run) { run(); count++; console.log(`ok ${count} - ${name}`); }

test(`the dictionary loads (${parseMs} ms on the host)`, () => {
  assert.ok(dict.syllables.has('zhuang')); assert.ok(dict.syllables.size > 380);
});
test('letters split into whole syllables, preferring longer ones', () => {
  assert.deepEqual(split('nihao'), ['ni', 'hao']);
  assert.deepEqual(split('zhongguoren'), ['zhong', 'guo', 'ren']);
  assert.deepEqual(split('xian'), ['xian']);
  assert.deepEqual(split("xi'an"), ['xi', 'an']);
});
test('a trailing partial syllable and bare initials are prefixes', () => {
  assert.deepEqual(split('nih'), ['ni', '~h']);
  assert.deepEqual(split('zhongg'), ['zhong', '~g']);
  assert.deepEqual(split('zg'), ['~z', '~g']);
  assert.equal(py.segment(dict, 'vvv'), null);
});
test('whole words come first, by frequency', () => {
  assert.equal(words('nihao')[0], '你好');
  assert.equal(words('zhongguo')[0], '中国');
  assert.equal(words('shurufa')[0], '输入法');
  assert.equal(words('de')[0], '的');
});
test('partial syllables and abbreviations find words', () => {
  assert.equal(words('nih')[0], '你好');
  assert.ok(words('zg').slice(0, 5).includes('中国'));
  assert.ok(words('jianp').includes('键盘'));
});
test('a sentence is composed when no single word covers the letters', () => {
  const c = new py.PinyinComposer(dict); c.composing = 'woxihuanzhegejianpan';
  assert.equal(c.candidates()[0].word, '我喜欢这个键盘');
});
test('shorter words follow and consume only their own letters', () => {
  const c = new py.PinyinComposer(dict); c.composing = 'nihaoshijie';
  const ni = c.candidates().find(x => x.word === '你好');
  assert.equal(ni.consumed, 5);
  assert.equal(c.pick(ni), null); assert.equal(c.composing, 'shijie'); assert.equal(c.display(), "你好shi'jie");
  assert.equal(c.pick(c.candidates().find(x => x.word === '世界')), '你好世界');
  assert.equal(c.active, false);
});
test('apostrophes are consumed with the word before them', () => {
  const c = new py.PinyinComposer(dict); c.composing = "xi'an";
  const xi = c.candidates().find(x => x.word === '西');
  assert.equal(xi.consumed, 3); c.pick(xi); assert.equal(c.composing, 'an');
});
test('space commits the best reading; enter commits the letters', () => {
  const c = new py.PinyinComposer(dict); c.composing = 'nihao';
  assert.equal(c.commitFirst(), '你好');
  c.composing = 'hello'; assert.equal(c.commitRaw(), 'hello');
  c.composing = "ni'hao"; assert.equal(c.commitRaw(), 'nihao');
});
test('backspace removes letters, then picked words', () => {
  const c = new py.PinyinComposer(dict); c.composing = 'nihaoshi';
  c.pick(c.candidates().find(x => x.word === '你好'));
  c.backspace(); c.backspace(); c.backspace();
  assert.equal(c.composing, ''); assert.equal(c.picked, '你好');
  c.backspace(); assert.equal(c.picked, '你'); c.backspace(); assert.equal(c.active, false);
});
test('picking a word raises it next time', () => {
  const c = new py.PinyinComposer(dict); c.composing = 'shi';
  const before = c.candidates().map(x => x.word);
  const rare = before[6];
  c.pick(c.candidates().find(x => x.word === rare));
  for (let i = 0; i < 40; i++) { c.composing = 'shi'; c.pick(c.candidates().find(x => x.word === rare)); }
  c.composing = 'shi'; assert.ok(c.candidates().map(x => x.word).indexOf(rare) < 6);
});
console.log(`1..${count}`);
