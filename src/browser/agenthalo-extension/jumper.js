// Private Native Messaging effects used within a single serialized Rust probe.
// The page owns all HTTP and WebSocket traffic. No network listener is needed.
let jumperSession = null;

async function releaseJumper(retain = false) {
  const session = jumperSession;
  if (!session) return { state: 'released' };
  // Drop locally held bindings before asynchronous cleanup.
  jumperSession = null;
  const tab = await chrome.tabs.get(session.tabId).catch(() => null);
  const url = tab?.url && new URL(tab.url);
  const keepPage = retain || session.auth ||
    (url?.protocol === 'https:' && (url.origin !== session.origin || url.pathname !== '/terminal'));
  if (attached.has(session.tabId)) {
    try {
      await chrome.debugger.detach({ tabId: session.tabId });
    } catch {
      if (keepPage && attached.has(session.tabId)) throw new Error('terminal debugger release unconfirmed');
    }
    attached.delete(session.tabId);
  }
  if (!keepPage) {
    try {
      await chrome.tabs.remove(session.tabId);
    } catch {
      if (await chrome.tabs.get(session.tabId).catch(() => null)) throw new Error('tab cleanup failed');
    }
  }
  return { state: 'released', pageRetained: !!keepPage };
}

async function openJumper({ site, host }) {
  checkChannel();
  if (jumperSession) throw new Error('terminal already bound');
  const port = requestPort;
  const origin = 'https://' + site;
  const { id: tabId } = await chrome.tabs.create({ url: origin + '/terminal' });
  if (port !== currentPort) {
    await chrome.tabs.remove(tabId);
    throw new Error('terminal channel changed');
  }
  jumperSession = { tabId, origin, host, context: null, document: null, bound: false, auth: false };
  return { state: 'opened' };
}

// Runs only in the isolated world of the bound top-level terminal document.
// Keep complete rows and the baseline inside that document. Return only the
// specific evidence the Rust host needs; never return the input's value.
function terminalEffect(origin, host, action, bound) {
  if (location.origin !== origin || location.pathname !== '/terminal') return { state: 'authenticationRequired' };
  const roots = document.querySelectorAll('.xterm');
  const auth = () => !!document.querySelector('input[type=password]') ||
    /(?:请输入.{0,8}(?:验证码|动态口令)|请.{0,10}(?:完成二次认证|确认登录)|认证消息已发送|等待.{0,5}认证|verification code required)/i.test(document.body?.innerText || '');
  if (auth()) return { state: 'authenticationRequired' };
  if (roots.length !== 1) return { state: 'waiting' };
  const root = roots[0];
  const input = root.querySelector('textarea.xterm-helper-textarea');
  const renderer = root.querySelector('.xterm-rows') || root.querySelector('.xterm-accessibility-tree');
  if (!input || !renderer?.children.length) return { state: 'unsupportedRenderer' };
  const rows = Array.from(renderer.children, node => node.textContent.replace(/\u00a0/g, ' ').trimEnd());
  while (rows.length && !rows.at(-1)) rows.pop();
  if (!rows.length) return { state: 'waiting' };

  if (bound && !globalThis.__musterTerminal) return { state: 'documentChanged' };
  const cursor = globalThis.__musterTerminal ??= { root, before: null, last: null, command: null };
  if (cursor.root !== root) return { state: 'documentChanged' };
  const commands = { connect: 'ssh ' + host, whoami: 'whoami', exit: 'exit' };
  const same = (a, b) => a?.length === b.length && b.every((row, i) => row === a[i]);
  if (action !== 'observe') {
    if (!Object.hasOwn(commands, action) || !same(cursor.last, rows)) return { state: 'changedBeforeInput' };
    cursor.before = rows;
    cursor.last = null;
    cursor.command = commands[action];
    input.focus();
    // Dispatch on the exact xterm input, synchronously in this document. A
    // navigation cannot redirect a tab-wide Enter into another application.
    const transfer = new DataTransfer();
    transfer.setData('text/plain', cursor.command);
    input.dispatchEvent(new ClipboardEvent('paste', { clipboardData: transfer, bubbles: true, cancelable: true }));
    input.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true,
    }));
    return { state: 'sent' };
  }
  cursor.last = rows;
  let fresh = rows;
  if (cursor.before) {
    if (same(cursor.before, rows)) return { state: 'waiting' };
    if (cursor.command !== commands.connect) {
      // Identify the line that was at the cursor immediately before this
      // write. Permit scroll-up only when the surviving prefix agrees.
      const before = cursor.before;
      const oldPrompt = before.at(-1).trimEnd();
      const i = rows.findLastIndex((line, i) => i < before.length && (i > 0 || before.length === 1) &&
        line.startsWith(oldPrompt) && line.slice(oldPrompt.length).trim() === cursor.command &&
        same(before.slice(before.length - 1 - i, -1), rows.slice(0, i)));
      if (i < 0) return { state: 'waiting' };
      fresh = rows.slice(i);
    }
  }

  function prompt(line) {
    // Match the complete hostname field, including common bracketed or
    // user@host:path shell prompts. The rest of the line must be a known
    // command or empty. Never export usernames, paths or general banners.
    const m = line.match(/^\[?[\w.-]+@([\w.-]+)(?:(?: |:)[^#$\]\r\n]{0,180})?\]?\s*[#$]\s*(.*?)\s*$/);
    if (!m || (m[1] !== host && m[1].toLowerCase() !== 'jumper') ||
        !['', 'whoami', 'exit'].includes(m[2])) return null;
    return [m[1] === host ? host : 'Jumper', m[2]];
  }
  const result = fresh.map(prompt);
  if (cursor.command === 'whoami' && fresh.length === 3 &&
      result[0]?.[1] === 'whoami' && result[2]?.[0] === host &&
      /^[a-z_][a-z0-9_.-]{0,63}$/i.test(fresh[1])) result[1] = fresh[1];
  return { state: 'screen', rows: result };
}

async function jumperEffect(action) {
  const session = jumperSession;
  if (!session) return { state: 'documentChanged' };
  const { tabId, origin, host } = session;
  const tab = await chrome.tabs.get(tabId);
  if (!tab.url || tab.url === 'about:blank') return { state: 'waiting' };
  const url = new URL(tab.url);
  if (url.origin !== origin || url.pathname !== '/terminal') {
    session.auth = true;
    return { state: 'authenticationRequired' };
  }
  return withDebugger(tabId, async () => {
    const doc = await documentId(tabId);
    if (session.document !== null && session.document !== doc) return { state: 'documentChanged' };
    if (session.context === null) {
      const { frameTree } = await cdp(tabId, 'Page.getFrameTree');
      const { executionContextId } = await cdp(tabId, 'Page.createIsolatedWorld', {
        frameId: frameTree.frame.id, worldName: 'muster-terminal-identity',
      });
      if (doc !== await documentId(tabId)) return { state: 'documentChanged' };
      session.document = doc;
      session.context = executionContextId;
    }
    const args = [origin, host, action, session.bound].map(a => JSON.stringify(a)).join(',');
    let answer;
    try {
      answer = await cdp(tabId, 'Runtime.evaluate', {
        contextId: session.context,
        expression: '(' + terminalEffect.toString() + ')(' + args + ')',
        returnByValue: true,
      });
    } catch {
      const after = await chrome.tabs.get(tabId).catch(() => null);
      if (after?.url && new URL(after.url).origin !== origin) {
        session.auth = true;
        return { state: 'authenticationRequired' };
      }
      return { state: 'documentChanged' };
    }
    if (answer.exceptionDetails) throw new Error('terminal adapter failed');
    const value = answer.result.value;
    if (value.state === 'screen') session.bound = true;
    if (value.state === 'authenticationRequired') session.auth = true;
    return value;
  });
}
