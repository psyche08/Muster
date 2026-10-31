// The bridge against a fake DeepSeek Harness host: `node --test test/*.test.js`.

import assert from 'node:assert/strict'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { test } from 'node:test'

import { Bridge, PROTOCOL, apply, parseAnswers, serve, socketPath } from '../index.js'

function host() {
  const calls = []
  return {
    calls,
    agents: {
      list: () => [
        { id: 'session-a', status: 'running' },
        { id: 'session-b', status: 'idle' },
      ],
    },
    sessionController: {
      async prompt(request, signal) {
        assert.ok(signal instanceof AbortSignal)
        calls.push(['prompt', request])
        return { accepted: true }
      },
      async cancel(request) {
        calls.push(['cancel', request])
        return { accepted: true }
      },
    },
  }
}

const approval = (signal) => ({
  agent: { id: 'session-a' },
  toolName: 'bash',
  callId: 'call-1',
  reason: 'rm -rf build',
  signal,
})

const tick = () => new Promise((resolve) => setTimeout(resolve, 5))

test('state lists live sessions and what waits for a person', async () => {
  const bridge = new Bridge(host())
  const state = await bridge.handle({ op: 'state' })
  assert.deepEqual(state.sessions, [
    { id: 'session-a', running: true },
    { id: 'session-b', running: false },
  ])
  assert.deepEqual(state.approvals, [])
  assert.equal((await bridge.handle({ op: 'hello' })).protocol, PROTOCOL)
})

test('a prompt goes through the session controller with its request id', async () => {
  const ctx = host()
  const bridge = new Bridge(ctx)
  await bridge.handle({ op: 'prompt', sessionId: 'session-b', requestId: 'r1', text: 'hello' })
  assert.deepEqual(ctx.calls[0], [
    'prompt',
    { requestId: 'r1', sessionId: 'session-b', mode: 'queue', content: [{ type: 'text', text: 'hello' }] },
  ])
  await bridge.handle({ op: 'cancel', sessionId: 'session-a' })
  assert.deepEqual(ctx.calls[1], ['cancel', { sessionId: 'session-a' }])
  await assert.rejects(bridge.handle({ op: 'prompt', sessionId: 's', requestId: 'r', text: '  ' }))
  await assert.rejects(bridge.handle({ op: 'prompt', sessionId: 's', text: 'x' }))
  await assert.rejects(
    bridge.handle({ op: 'prompt', sessionId: 's', requestId: 'r', text: 'x', mode: 'later' }),
  )
  await assert.rejects(bridge.handle({ op: 'rename' }))
})

test("Muster's answer wins and withdraws the window's prompt", async () => {
  const bridge = new Bridge(host())
  const req = approval(new AbortController().signal)
  let windowSignal
  const window = Promise.withResolvers()
  const outcome = bridge.offer(
    bridge.approvals,
    req,
    () => {
      windowSignal = req.signal
      return window.promise
    },
    () => ({ toolName: 'bash' }),
    (o) => o === 'unavailable',
  )
  await tick()
  const [waiting] = (await bridge.handle({ op: 'state' })).approvals
  assert.equal(waiting.sessionId, 'session-a')
  assert.equal(waiting.toolName, 'bash')
  await bridge.handle({ op: 'approve', approvalId: waiting.id, outcome: 'rejected' })
  assert.equal(await outcome, 'rejected')
  assert.ok(windowSignal.aborted, 'the window prompt is withdrawn')
  assert.deepEqual((await bridge.handle({ op: 'state' })).approvals, [])
  await assert.rejects(bridge.handle({ op: 'approve', approvalId: waiting.id, outcome: 'rejected' }))
})

test("the window's answer wins and leaves nothing waiting", async () => {
  const bridge = new Bridge(host())
  const req = approval(new AbortController().signal)
  const outcome = await bridge.offer(
    bridge.approvals,
    req,
    async () => 'allowed-once',
    () => ({}),
    (o) => o === 'unavailable',
  )
  assert.equal(outcome, 'allowed-once')
  assert.equal(bridge.approvals.size, 0)
  assert.ok(!req.signal.aborted)
})

test('with no window open, a present Muster still answers', async () => {
  const bridge = new Bridge(host())
  await bridge.handle({ op: 'state' })
  const req = approval(new AbortController().signal)
  const outcome = bridge.offer(
    bridge.approvals,
    req,
    async () => 'unavailable',
    () => ({}),
    (o) => o === 'unavailable',
  )
  await tick()
  const [waiting] = (await bridge.handle({ op: 'state' })).approvals
  await bridge.handle({ op: 'approve', approvalId: waiting.id, outcome: 'allowed-once' })
  assert.equal(await outcome, 'allowed-once')
})

test('with no window and no Muster, the request fails closed at once', async () => {
  const bridge = new Bridge(host())
  const outcome = await bridge.offer(
    bridge.approvals,
    approval(new AbortController().signal),
    async () => 'unavailable',
    () => ({}),
    (o) => o === 'unavailable',
  )
  assert.equal(outcome, 'unavailable')
  const question = bridge.offer(
    bridge.questions,
    { agent: { id: 'session-a' }, questions: [] },
    async () => {
      throw new Error('no user-questions answerer accepted the request')
    },
    () => ({}),
    () => false,
  )
  await assert.rejects(question, /no user-questions answerer/)
})

test('a request cancelled by its tool stops waiting', async () => {
  const bridge = new Bridge(host())
  const abort = new AbortController()
  const outcome = bridge.offer(
    bridge.approvals,
    approval(abort.signal),
    () => new Promise(() => {}),
    () => ({}),
    (o) => o === 'unavailable',
  )
  abort.abort(new Error('tool cancelled'))
  await assert.rejects(outcome, /tool cancelled/)
  assert.equal(bridge.approvals.size, 0)
})

test('a request that cannot carry another signal stays with the window', async () => {
  const bridge = new Bridge(host())
  const req = Object.freeze(approval(new AbortController().signal))
  const outcome = await bridge.offer(bridge.approvals, req, async () => 'rejected', () => ({}), () => false)
  assert.equal(outcome, 'rejected')
  assert.equal(bridge.approvals.size, 0)
})

test('answers name only the questions and options asked', () => {
  const questions = [
    { id: 'q1', options: [{ label: 'A' }, { label: 'B' }], multiSelect: false },
    { id: 'q2', options: [{ label: 'X' }, { label: 'Y' }], multiSelect: true },
  ]
  assert.deepEqual(
    parseAnswers(questions, [
      { id: 'q1', selected: [], custom: 'my own' },
      { id: 'q2', selected: ['X', 'Y'] },
    ]),
    [
      { id: 'q1', selected: [], custom: 'my own' },
      { id: 'q2', selected: ['X', 'Y'] },
    ],
  )
  assert.throws(() => parseAnswers(questions, [{ id: 'q3', selected: [] }]))
  assert.throws(() => parseAnswers(questions, [{ id: 'q1', selected: ['C'] }]))
  assert.throws(() => parseAnswers(questions, [{ id: 'q1', selected: ['A', 'B'] }]))
  assert.throws(() =>
    parseAnswers(questions, [
      { id: 'q1', selected: ['A'] },
      { id: 'q1', selected: ['B'] },
    ]),
  )
})

test('a socket that cannot be served leaves the bridge off without throwing', async () => {
  const messages = []
  const close = serve(new Bridge(host()), `/tmp/${'x'.repeat(120)}/bridge.sock`, (m) => messages.push(m))
  assert.match(messages[0], /longer than/)
  await close()
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'dsh-')), 'file')
  fs.writeFileSync(file, '')
  const blocked = serve(new Bridge(host()), path.join(file, 'muster', 'bridge.sock'), (m) => messages.push(m))
  assert.match(messages[1], /not serving/)
  await blocked()
})

test('the plugin registers its socket as an effect of its fiber', () => {
  const effects = []
  const listeners = []
  const ctx = {
    ...host(),
    on: (name) => listeners.push(name),
    effect: (execute, label) => effects.push(label),
  }
  apply(ctx)
  assert.deepEqual(listeners, ['approval/request', 'user-questions/request'])
  assert.deepEqual(effects, ['muster bridge: socket'])
})

test('a question card is answered through the socket', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dsh-'))
  const socket = socketPath({ DSH_HOME: dir })
  const bridge = new Bridge(host())
  const close = serve(bridge, socket)
  await new Promise((resolve) => setTimeout(resolve, 50))
  assert.equal(fs.statSync(socket).mode & 0o777, 0o600)
  assert.equal(fs.statSync(path.dirname(socket)).mode & 0o777, 0o700)
  const call = (request) =>
    new Promise((resolve, reject) => {
      const client = net.connect(socket)
      let data = ''
      client.setEncoding('utf8')
      client.on('data', (chunk) => {
        data += chunk
        if (data.includes('\n')) {
          client.end()
          resolve(JSON.parse(data))
        }
      })
      client.on('error', reject)
      client.write(`${JSON.stringify(request)}\n`)
    })
  await call({ id: 1, op: 'hello' })
  const answer = bridge.offer(
    bridge.questions,
    { agent: { id: 'session-a' }, questions: [{ id: 'q1', question: 'Which?', options: [{ label: 'A' }] }] },
    () => new Promise(() => {}),
    (req) => ({ questions: req.questions }),
    () => false,
  )
  await tick()
  const state = await call({ id: 2, op: 'state' })
  const [waiting] = state.result.questions
  assert.equal(waiting.questions[0].question, 'Which?')
  const reply = await call({ id: 3, op: 'answer', questionId: waiting.id, answers: [{ id: 'q1', selected: ['A'] }] })
  assert.deepEqual(reply, { id: 3, ok: true, result: { accepted: true } })
  assert.deepEqual(await answer, { answers: [{ id: 'q1', selected: ['A'] }] })
  const refused = await call({ id: 4, op: 'answer', questionId: waiting.id, answers: [] })
  assert.equal(refused.ok, false)
  await close()
  fs.rmSync(dir, { recursive: true, force: true })
})
