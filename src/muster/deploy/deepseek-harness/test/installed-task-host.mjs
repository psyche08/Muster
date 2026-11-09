// Opt-in acceptance test: run with node on a Mac with the installed DSH App.
// Fork only its bundled desktop host in a fresh temporary profile, never its CLI.
import assert from 'node:assert/strict'
import { fork } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import fs from 'node:fs'
import net from 'node:net'
import path from 'node:path'
import { once } from 'node:events'
import { fileURLToPath } from 'node:url'
import { setTimeout as delay } from 'node:timers/promises'

const contents = '/Applications/DeepSeek Harness.app/Contents'
const executable = path.join(contents, 'MacOS/DeepSeek Harness')
const runtime = path.join(contents, 'Resources/app.asar/dsh')
const entry = path.join(runtime, 'node_modules/@deepseek-ai/dsh-desktop-host/lib/index.js')
const plugin = fileURLToPath(new URL('../index.js', import.meta.url))
const home = fs.mkdtempSync('/private/tmp/muster-dsh-task-')
const profile = path.join(home, 'profiles/desktop')
const sessionId = 'session-' + randomUUID()
const socket = path.join(home, 'muster/bridge.sock')
fs.mkdirSync(profile, { recursive: true })
fs.writeFileSync(path.join(profile, 'package.json'), JSON.stringify({ private: true, dependencies: {},
  dsh: { profile: { bundles: ['@deepseek-ai/dsh-base', '@deepseek-ai/dsh-web-app'] } } }))
fs.writeFileSync(path.join(profile, 'cordis.yml'), '[]\n')
fs.writeFileSync(path.join(profile, 'cordis.patch.yml'), JSON.stringify([{ insert: [{ id: 'task-probe', name: plugin }] }]))

async function call(request) {
  return new Promise((resolve, reject) => {
    const client = net.connect(socket)
    client.setTimeout(8000, () => client.destroy(new Error('socket timeout')))
    let data = ''
    client.setEncoding('utf8')
    client.on('error', reject).on('data', chunk => {
      data += chunk
      if (!data.includes('\n')) return
      client.end()
      const reply = JSON.parse(data)
      if (reply.ok) resolve(reply.result)
      else reject(new Error(reply.error))
    })
    client.on('end', () => reject(new Error('No complete reply')))
    client.write(JSON.stringify({ ...request, id: randomUUID() }) + '\n')
  })
}

async function boot(run) {
  const child = fork(entry, [runtime, profile, path.join(contents, 'Resources/runtime/primary-runtime')], {
    cwd: home, execPath: executable, execArgv: ['--expose-internals'],
    env: { PATH: '/usr/bin:/bin', HOME: home, DSH_HOME: home, ELECTRON_RUN_AS_NODE: '1' },
    stdio: ['ignore', 'ignore', 'ignore', 'ipc'],
  })
  const exited = once(child, 'exit')
  try {
    let state
    for (let i = 0; i < 150 && child.exitCode === null; i++) {
      try { state = await call({ op: 'state' }); break } catch { await delay(100) }
    }
    assert.ok(state, 'isolated App bridge did not start')
    assert.ok(state.control.capabilities.includes('task.create.v1'))
    const invoke = (op, id = sessionId) => call({ op, sessionId: id, cwd: home, store: home,
      control: { version: 1, domain: 'app', epoch: state.control.epoch, operationId: randomUUID(), sessionId: id } })
    await run(state, invoke)
  } finally {
    if (child.connected) child.send({ type: 'shutdown' })
    const timeout = setTimeout(() => child.kill('SIGKILL'), 4000)
    try { await exited } finally { clearTimeout(timeout) }
  }
}

try {
  const facts = { sessionId, cwd: home, store: home }
  await boot(async (state, invoke) => {
    assert.equal(state.sessions.length, 0)
    assert.deepEqual(await invoke('task.create.v1'), facts)
    await assert.rejects(invoke('task.create.v1'), /already exists/)
  })
  await boot(async (state, invoke) => {
    assert.equal(state.sessions.length, 0)
    assert.deepEqual(await invoke('task.inspect.v1'), facts)
    assert.equal((await call({ op: 'state' })).sessions.length, 0)
    await assert.rejects(invoke('task.resume.v1', 'session-' + randomUUID()))
    assert.deepEqual(await invoke('task.resume.v1'), facts)
    assert.deepEqual((await call({ op: 'state' })).sessions.map(s => s.id), [sessionId])
  })
  console.log('Installed isolated App host: created one session, refused duplicate and missing IDs, cold-restored the original.')
} finally {
  fs.rmSync(home, { recursive: true, force: true })
}
