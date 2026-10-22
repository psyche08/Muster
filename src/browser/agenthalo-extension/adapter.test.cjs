const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function fixture() {
  const event = () => ({ listeners: [], addListener(fn) { this.listeners.push(fn); } });
  const sent = [], commands = [], timers = [], detached = [], ports = [];
  let document = 50, attachCount = 0;
  const chrome = {
    runtime: {
      connectNative(name) {
        assert.equal(name, 'dev.linsheng.muster.browser');
        const port = { onMessage: event(), onDisconnect: event(), postMessage: m => sent.push(m) };
        ports.push(port); return port;
      },
      getManifest: () => ({ version: '0.2.0' }), onStartup: event(),
    },
    tabs: { get: async () => ({ url: 'https://example.test/' }), onRemoved: event(), query: async () => [] },
    debugger: {
      onDetach: event(),
      attach: async () => { attachCount++; },
      detach: async ({ tabId }) => detached.push(tabId),
      sendCommand: async (target, method, params) => {
        commands.push({ target, method, params });
        if (method === 'DOM.getDocument') return { root: { backendNodeId: document } };
        if (method === 'Accessibility.getFullAXTree') return { nodes: [{ nodeId: '1' }] };
        if (method === 'DOM.getContentQuads') return { quads: [[0,0,10,0,10,10,0,10]] };
        return {};
      },
    },
  };
  const context = vm.createContext({ chrome, console: { warn() {} }, navigator: { platform: 'Linux', userAgent: 'test' }, TextEncoder, setTimeout: fn => timers.push(fn) });
  vm.runInContext(fs.readFileSync(__dirname + '/sw.js', 'utf8'), context);
  return {
    chrome, sent, commands, detached, ports, timers, context,
    navigate: () => { document++; },
    attachCount: () => attachCount,
    request: async request => { await context.onRequest({ reqId: 1, ...request }, ports.at(-1)); return sent.at(-1); },
  };
}

test('snapshot returns explicit document; stale document never sends input', async () => {
  const f = fixture();
  const snapshot = await f.request({ op: 'snapshot', tabId: 3 });
  assert.equal(snapshot.result.document, 50);
  assert.equal(snapshot.result.tabId, 3);
  assert.equal(f.attachCount(), 1);
  f.navigate();
  const result = await f.request({ op: 'click', tabId: 3, document: 50, ref: 8 });
  assert.match(result.error, /stale document/);
  assert.equal(f.commands.filter(c => c.method.startsWith('Input.')).length, 0);
});

test('clear/type uses platform modifier and actual Input calls, no evaluation', async () => {
  const f = fixture();
  const result = await f.request({ op: 'type', tabId: 3, document: 50, ref: 8, text: '你好', clear: true });
  assert.equal(result.result, null);
  assert.equal(f.commands.find(c => c.params.commands)?.params.modifiers, 2);
  assert.equal(f.commands.find(c => c.method === 'Input.insertText').params.text, '你好');
  assert.ok(f.commands.every(c => !c.method.startsWith('Runtime.')));
});

test('Rust idle release detaches; next explicit snapshot attaches again', async () => {
  const f = fixture();
  await f.request({ op: 'snapshot', tabId: 3 });
  await f.request({ op: 'detachAll' });
  assert.deepEqual(f.detached, [3]);
  await f.request({ op: 'snapshot', tabId: 3 });
  assert.equal(f.attachCount(), 2);
});

test('only own operation names are callable and restricted tabs fail closed', async () => {
  const f = fixture();
  assert.match((await f.request({ op: 'toString' })).error, /unsupported/);
  f.chrome.tabs.get = async () => ({ url: 'chrome://settings' });
  assert.match((await f.request({ op: 'snapshot', tabId: 3 })).error, /HTTP/);
  assert.equal(f.attachCount(), 0);
});

test('native disconnect during async focus cannot continue into text input after reconnect', async () => {
  const f = fixture();
  const original = f.chrome.debugger.sendCommand;
  let release;
  f.chrome.debugger.sendCommand = async (...args) => {
    if (args[1] === 'DOM.focus') await new Promise(resolve => { release = resolve; });
    return original(...args);
  };
  const pending = f.request({ op: 'type', tabId: 3, document: 50, ref: 8, text: 'must not send' });
  while (!release) await new Promise(resolve => setImmediate(resolve));
  f.ports[0].onDisconnect.listeners[0]();
  while (!f.timers.length) await new Promise(resolve => setImmediate(resolve));
  f.timers.shift()();
  assert.match((await f.request({ op: 'ping' })).error, /still finishing/);
  release(); await pending;
  assert.ok(!f.commands.some(c => c.method === 'Input.insertText'));
});

test('preserved public manifest key matches the Rust native host origin', () => {
  const manifest = JSON.parse(fs.readFileSync(__dirname + '/manifest.json', 'utf8'));
  const hash = require('node:crypto').createHash('sha256').update(Buffer.from(manifest.key, 'base64')).digest('hex').slice(0,32);
  const id = [...hash].map(c => String.fromCharCode(97 + parseInt(c,16))).join('');
  assert.equal(id, 'ngfbfehgemldmajkjhjahbechooakidc');
  assert.deepEqual(manifest.permissions, ['debugger', 'nativeMessaging', 'tabs']);
  assert.equal(manifest.externally_connectable, undefined);
  assert.equal(manifest.content_scripts, undefined);
});
