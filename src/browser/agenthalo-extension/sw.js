// Chrome OS/API adapter only. Rust owns request schemas, deadlines, serialization,
// AX projection and idle release. No content scripts or external message listener.
const HOST = 'dev.linsheng.muster.browser';
const attached = new Set();
let currentPort = null;
let requestPort = null;
let busy = false;
let retryMs = 1000;

async function detachAll() {
  await Promise.all([...attached].map(async tabId => {
    attached.delete(tabId);
    await chrome.debugger.detach({ tabId }).catch(() => {});
  }));
}
function connect() {
  const port = chrome.runtime.connectNative(HOST);
  currentPort = port;
  port.onMessage.addListener(message => onRequest(message, port));
  port.onDisconnect.addListener(() => {
    console.warn('Agent Halo host disconnected:', chrome.runtime.lastError?.message);
    if (currentPort === port) currentPort = null;
    // A disconnected request is never replayed. Pending CDP calls check this port.
    detachAll().finally(() => {
      setTimeout(connect, retryMs);
      retryMs = Math.min(retryMs * 2, 8000);
    });
  });
}
async function onRequest({ reqId, op, ...args }, port) {
  if (busy) {
    port.postMessage({ reqId, error: 'previous browser operation still finishing' });
    return;
  }
  busy = true;
  requestPort = port;
  let reply;
  try {
    if (currentPort !== port) throw new Error('native channel changed');
    const handler = Object.hasOwn(OPS, op) && OPS[op];
    if (!handler) throw new Error('unsupported operation');
    reply = { reqId, result: (await handler(args)) ?? null };
    retryMs = 1000;
  } catch (e) {
    reply = { reqId, error: e?.message ?? String(e) };
  }
  if (currentPort !== port) { await detachAll(); busy = false; return; }
  busy = false;
  try {
    // Native host's bounded frame parser is the authority; avoid huge postMessage allocations.
    if (new TextEncoder().encode(JSON.stringify(reply)).length > 8 * 1024 * 1024) {
      reply = { reqId, error: 'browser result too large; narrow the page/query' };
    }
    port.postMessage(reply);
  } catch { /* Native host handles channel loss as an unknown outcome. */ }
}
function checkChannel() {
  if (!requestPort || requestPort !== currentPort) throw new Error('native channel disconnected');
}
const cdp = async (tabId, method, params = {}) => {
  checkChannel();
  const result = await chrome.debugger.sendCommand({ tabId }, method, params);
  checkChannel();
  return result;
};
async function withDebugger(tabId, fn) {
  const tab = await chrome.tabs.get(tabId);
  if (!/^https?:\/\//i.test(tab.url ?? '')) throw new Error('only HTTP(S) tabs support accessibility');
  checkChannel();
  if (!attached.has(tabId)) {
    await chrome.debugger.attach({ tabId }, '1.3');
    attached.add(tabId);
  }
  checkChannel();
  return fn(tabId);
}
async function documentId(tabId) {
  const { root } = await cdp(tabId, 'DOM.getDocument', { depth: 0 });
  return root.backendNodeId;
}
async function readTree(tabId, query) {
  const document = await documentId(tabId);
  const { nodes } = query
    ? await cdp(tabId, 'Accessibility.queryAXTree', { backendNodeId: document, ...query })
    : await cdp(tabId, 'Accessibility.getFullAXTree');
  if (document !== await documentId(tabId)) throw new Error('page navigated during snapshot; read again');
  return { tabId, document, nodes };
}
async function writeNode(args, fn) {
  return withDebugger(args.tabId, async tabId => {
    if (args.document !== await documentId(tabId)) throw new Error('stale document; take a new snapshot');
    return fn(tabId);
  });
}
chrome.debugger.onDetach.addListener(({ tabId }) => attached.delete(tabId));
chrome.tabs.onRemoved.addListener(tabId => attached.delete(tabId));

async function click(tabId, ref) {
  await cdp(tabId, 'DOM.scrollIntoViewIfNeeded', { backendNodeId: ref });
  const { quads } = await cdp(tabId, 'DOM.getContentQuads', { backendNodeId: ref });
  if (!quads.length) throw new Error(`ref ${ref} 不可见，无法点击`);
  const q = quads[0];
  const x = (q[0] + q[2] + q[4] + q[6]) / 4;
  const y = (q[1] + q[3] + q[5] + q[7]) / 4;
  const mouse = { x, y, button: 'left', clickCount: 1 };
  await cdp(tabId, 'Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await cdp(tabId, 'Input.dispatchMouseEvent', { type: 'mousePressed', ...mouse });
  await cdp(tabId, 'Input.dispatchMouseEvent', { type: 'mouseReleased', ...mouse });
  return { x, y };
}

const KEYS = {
  Enter: { code: 'Enter', keyCode: 13, text: '\r' },
  Tab: { code: 'Tab', keyCode: 9 },
  Escape: { code: 'Escape', keyCode: 27 },
  Backspace: { code: 'Backspace', keyCode: 8 },
  Delete: { code: 'Delete', keyCode: 46 },
  Space: { key: ' ', code: 'Space', keyCode: 32, text: ' ' },
  ArrowUp: { code: 'ArrowUp', keyCode: 38 },
  ArrowDown: { code: 'ArrowDown', keyCode: 40 },
  ArrowLeft: { code: 'ArrowLeft', keyCode: 37 },
  ArrowRight: { code: 'ArrowRight', keyCode: 39 },
  Home: { code: 'Home', keyCode: 36 },
  End: { code: 'End', keyCode: 35 },
  PageUp: { code: 'PageUp', keyCode: 33 },
  PageDown: { code: 'PageDown', keyCode: 34 },
};
const SELECT_MODIFIER = /Mac/.test(navigator.platform) ? 4 : 2;

async function dispatchKey(tabId, name, extra = {}) {
  const k = KEYS[name] ??
    ([...name].length === 1 ? { code: '', keyCode: name.toUpperCase().charCodeAt(0), text: name } : null);
  if (!k) throw new Error(`不支持的按键 ${name}，可用：${Object.keys(KEYS).join(', ')} 或单个字符`);
  const base = { key: k.key ?? name, code: k.code, windowsVirtualKeyCode: k.keyCode, ...extra };
  const text = extra.commands ? undefined : k.text;
  await cdp(tabId, 'Input.dispatchKeyEvent', {
    ...base, type: text ? 'keyDown' : 'rawKeyDown', text, unmodifiedText: text,
  });
  await cdp(tabId, 'Input.dispatchKeyEvent', { ...base, type: 'keyUp' });
}

async function type(tabId, ref, text, clear) {
  await cdp(tabId, 'DOM.focus', { backendNodeId: ref });
  if (clear) {
    await dispatchKey(tabId, 'a', { code: 'KeyA', modifiers: SELECT_MODIFIER, commands: ['selectAll'] });
    await dispatchKey(tabId, 'Backspace');
  }
  if (text) await cdp(tabId, 'Input.insertText', { text });
}

async function press(tabId, key, ref) {
  if (ref != null) await cdp(tabId, 'DOM.focus', { backendNodeId: ref });
  await dispatchKey(tabId, key);
}

const OPS = {
  ping: () => ({ capability: 'agenthalo.browser', version: chrome.runtime.getManifest().version, userAgent: navigator.userAgent }),
  tabs: async () => (await chrome.tabs.query({})).map(({ id, windowId, active, title, url }) => ({ id, windowId, active, title, url })),
  sessions: () => [...attached],
  open: async ({ url }) => ({ tabId: (await chrome.tabs.create({ url })).id }),
  close: ({ tabId }) => chrome.tabs.remove(tabId),
  detach: async ({ tabId }) => {
    attached.delete(tabId);
    await chrome.debugger.detach({ tabId });
  },
  detachAll,
  snapshot: ({ tabId }) => withDebugger(tabId, id => readTree(id)),
  find: ({ tabId, role, name }) => withDebugger(tabId, id => readTree(id, {
    ...(role ? { role } : {}), ...(name ? { accessibleName: name } : {})
  })),
  click: args => writeNode(args, id => click(id, args.ref)),
  type: args => writeNode(args, id => type(id, args.ref, args.text, args.clear)),
  press: args => writeNode(args, id => press(id, args.key, args.ref)),
};
chrome.runtime.onStartup.addListener(() => {});
connect();
