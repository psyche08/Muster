// Muster's bridge into DeepSeek Harness (ADR 0074).
//
// A Cordis plugin that runs inside DeepSeek Harness's desktop host and
// serves one owner-only Unix socket, `$DSH_HOME/muster/bridge.sock`, to the
// Muster daemon on the same Mac. It adapts the app's own Host services and
// decides nothing: which caller may send, approve or answer, and whether a
// write was delivered, stay in Muster's Shared Rust.
//
// - Live state: the app's agent registry (which sessions run), and the
//   approvals and question cards waiting for a person.
// - Prompts and interrupts: `sessionController.prompt` and `.cancel`, the
//   path the app's own window takes; a prompt's `requestId` becomes the
//   recorded message's `rpcId`.
// - Approvals and question cards: the first listener of the
//   `approval/request` and `user-questions/request` waterfalls. It offers
//   the request to Muster and passes it on to the window at once; whichever
//   answers first wins. An answer from Muster withdraws the window's prompt
//   through the request's signal, which then also follows Muster's answer.
//
// One request per line, one JSON response per request:
//   {"id":1,"op":"hello"}                    -> {"id":1,"ok":true,"result":{...}}
//   {"id":2,"op":"state"}                    -> {"id":2,"ok":true,"result":{...}}
//   {"id":3,"op":"prompt","sessionId","requestId","text","mode"}
//   {"id":4,"op":"cancel","sessionId"}
//   {"id":5,"op":"approve","approvalId","outcome":"allowed-once"|"rejected"}
//   {"id":6,"op":"answer","questionId","answers":[{"id","selected","custom"}]}
// A failure is {"id":n,"ok":false,"error":"..."}.

import { randomUUID } from 'node:crypto'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'

export const name = 'muster-dsh-bridge'
export const inject = ['agents', 'sessionController', 'sessionQuery']

/** The wire protocol; Muster refuses a bridge that speaks another. */
export const PROTOCOL = 1

// Additive local contract. The outer protocol and legacy methods remain stable.
export const CONTROL_VERSION = 1
const CAPABILITIES = ['prompt', 'cancel', 'approve', 'answer', 'prompt.lookup']
const MAX_EVIDENCE = 4096

/** A request line longer than this closes the connection. */
const MAX_LINE = 1 << 20

/** The longest Unix socket path macOS binds (`sun_path` less its NUL). */
const MAX_SOCKET_PATH = 103

/** How long the app may take to admit a prompt. */
const PROMPT_TIMEOUT_MS = 30_000

/**
 * Muster polls every second; one silent this long is gone, and a request
 * the window could not answer then fails closed as it would without Muster.
 */
const PRESENCE_MS = 10_000

const OUTCOMES = new Set(['allowed-once', 'rejected'])

/** Where the socket lives: `$DSH_HOME/muster/bridge.sock`. */
export function socketPath(env = process.env) {
  const home = env.DSH_HOME || path.join(os.homedir(), '.dsh')
  return path.join(home, 'muster', 'bridge.sock')
}

/** The bridge's state and operations, apart from the socket. */
export class Bridge {
  constructor(ctx, desktopHost = process.argv[1]?.endsWith('/@deepseek-ai/dsh-desktop-host/lib/index.js') === true) {
    this.desktopHost = desktopHost
    this.ctx = ctx
    this.epoch = randomUUID()
    /** Waiting approvals and question cards by their bridge id. */
    this.approvals = new Map()
    this.questions = new Map()
    /** When Muster last asked anything: only a present Muster is waited for. */
    this.lastSeen = -Infinity
  }

  present(now = Date.now()) {
    return now - this.lastSeen < PRESENCE_MS
  }

  contract() {
    return { version: CONTROL_VERSION, domain: 'app', provider: 'deepseek',
      epoch: this.epoch, capabilities: [...CAPABILITIES, ...(this.desktopHost && this.ctx.sessionQuery?.observeSession &&
        this.ctx.sessionController?.create && this.ctx.sessionController?.resolveAgent
        ? ['task.create.v1', 'task.resume.v1', 'task.inspect.v1'] : [])] }
  }

  // Read host-owned facts only. Never resume a cold session or call prompt.
  lookup(sessionId, requestId) {
    if (typeof sessionId !== 'string' || !sessionId ||
        typeof requestId !== 'string' || !requestId || requestId.length > 128) {
      throw new Error('lookup needs a sessionId and requestId')
    }
    const agent = this.ctx.agents.get?.(sessionId)
    const records = []
    const add = (message, location) => {
      if (message?.source?.kind === 'user' && message.source.rpcId === requestId) {
        records.push({ source: { kind: 'user', rpcId: requestId }, location })
      }
    }
    for (const message of (agent?.inbox?.nextTurn ?? []).slice(-MAX_EVIDENCE)) add(message, 'next-turn')
    for (const message of (agent?.inbox?.nextStep ?? []).slice(-MAX_EVIDENCE)) add(message, 'next-step')
    const events = agent?.session?.snapshotEvents?.() ?? []
    for (const event of events.slice(-MAX_EVIDENCE)) {
      if (event.type === 'user/message') add(event.data, 'session-event')
    }
    return { sessionId, requestId, records: records.slice(0, 16) }
  }

  validateControl(request) {
    const control = request.control
    if (control === undefined && request.op?.startsWith('task.')) throw new Error('task session requires an App operation binding')
    if (control === undefined) return // Existing daemon's local protocol.
    if (control?.version !== CONTROL_VERSION || control.domain !== 'app' ||
        control.epoch !== this.epoch || typeof control.operationId !== 'string' ||
        !control.operationId || control.operationId.length > 256 ||
        typeof control.sessionId !== 'string' || !control.sessionId ||
        control.sessionId.length > 128) throw new Error('invalid or expired App operation binding')
    if (!this.contract().capabilities.includes(request.op)) throw new Error('capability is not offered')
    if (request.sessionId !== undefined && request.sessionId !== control.sessionId) {
      throw new Error('operation addresses another session')
    }
    const interaction = request.op === 'approve' ? this.approvals.get(request.approvalId)
      : request.op === 'answer' ? this.questions.get(request.questionId) : undefined
    if (interaction !== undefined && interaction.sessionId !== control.sessionId) {
      throw new Error('interaction belongs to another session')
    }
  }

  state() {
    const sessions = this.ctx.agents
      .list()
      .map((agent) => ({ id: agent.id, running: agent.status === 'running' }))
    const pending = (table) => [...table.values()].map(({ settle, ...visible }) => visible)
    return {
      protocol: PROTOCOL,
      pid: process.pid,
      control: this.contract(),
      sessions,
      approvals: pending(this.approvals),
      questions: pending(this.questions),
    }
  }

  async handle(request) {
    if (request === null || typeof request !== 'object' || Array.isArray(request)) {
      throw new Error('request must be an object')
    }
    this.validateControl(request)
    this.lastSeen = Date.now()
    switch (request.op) {
      case 'hello':
        return { protocol: PROTOCOL, pid: process.pid, control: this.contract() }
      case 'task.create.v1': case 'task.resume.v1': case 'task.inspect.v1':
        return this.taskSession(request)
      case 'prompt.lookup':
        return this.lookup(request.sessionId, request.requestId)
      case 'state':
        return this.state()
      case 'prompt': {
        const { sessionId, requestId, text } = request
        const mode = request.mode ?? 'queue'
        if (typeof sessionId !== 'string' || typeof requestId !== 'string' || !requestId) {
          throw new Error('prompt needs a sessionId and a requestId')
        }
        if (typeof text !== 'string' || !text.trim()) throw new Error('prompt needs text')
        if (mode !== 'queue' && mode !== 'steer') throw new Error(`unknown prompt mode ${mode}`)
        return await hostCall(() => this.ctx.sessionController.prompt(
          { requestId, sessionId, mode, content: [{ type: 'text', text }] },
          AbortSignal.timeout(PROMPT_TIMEOUT_MS),
        ))
      }
      case 'cancel': {
        if (typeof request.sessionId !== 'string') throw new Error('cancel needs a sessionId')
        return await hostCall(() => this.ctx.sessionController.cancel({ sessionId: request.sessionId }))
      }
      case 'approve': {
        const approval = this.approvals.get(request.approvalId)
        if (approval === undefined) throw new Error('no such approval is waiting')
        if (!OUTCOMES.has(request.outcome)) throw new Error(`unknown outcome ${request.outcome}`)
        this.approvals.delete(request.approvalId)
        approval.settle(request.outcome)
        return { accepted: true }
      }
      case 'answer': {
        const question = this.questions.get(request.questionId)
        if (question === undefined) throw new Error('no such question is waiting')
        const answers = parseAnswers(question.questions, request.answers)
        this.questions.delete(request.questionId)
        question.settle({ answers })
        return { accepted: true }
      }
      default:
        throw new Error(`unknown op ${request.op}`)
    }
  }

  async taskSession({ op, sessionId, cwd, store }) {
    if (!/^session-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(sessionId) ||
        typeof cwd !== 'string' || !path.isAbsolute(cwd) || fs.realpathSync(cwd) !== cwd ||
        !fs.statSync(cwd).isDirectory() || store !== fs.realpathSync(path.dirname(path.dirname(socketPath())))) {
      throw new Error('Task profile or directory does not match')
    }
    const check = header => {
      if (header?.id !== sessionId || header?.cwd !== cwd) throw new Error('Native session identity differs')
      return { sessionId: header.id, cwd: header.cwd, store }
    }
    const observe = async () => {
      const observation = await this.ctx.sessionQuery.observeSession(sessionId)
      try { return check(observation.header) } finally { observation[Symbol.dispose]() }
    }
    if (op === 'task.create.v1') {
      // Only a definite not-found permits create. Never reuse a blank personal
      // session or mistake storage/observation failure for a fresh identity.
      try { await observe(); throw new Error('Native session already exists') }
      catch (error) { if (error.code !== 'SESSION_QUERY_SESSION_NOT_FOUND') throw error }
      return hostCall(async () => {
        const result = await this.ctx.sessionController.create({ sessionId, cwd })
        if (result?.sessionId !== sessionId) throw new Error('Host created a different identity')
        return check(this.ctx.agents.get(sessionId)?.session?.header)
      })
    }
    const facts = await observe()
    if (op === 'task.inspect.v1') return facts
    return hostCall(async () => {
      const result = await this.ctx.sessionController.resolveAgent(sessionId)
      if (result?.error) throw new Error('Original App session cannot be restored')
      return check(result?.agent?.session?.header)
    })
  }

  /**
   * The first listener of an answer waterfall: offer `req` to Muster while
   * the window gets it too, and return whichever answer comes first.
   * `describe` names what Muster sees of it; `unanswered` recognizes the
   * window's "nobody can answer" result.
   */
  async offer(table, req, next, describe, unanswered) {
    const sessionId = req?.agent?.id
    if (typeof sessionId !== 'string') return next()
    // The window's prompt follows the request's signal; give it one that
    // Muster's answer can end too. A request that cannot carry it is left
    // to the window alone.
    const original = req.signal
    const ours = new AbortController()
    const signal = original === undefined ? ours.signal : AbortSignal.any([original, ours.signal])
    try {
      req.signal = signal
    } catch {
      return next()
    }
    if (req.signal !== signal) return next()
    const id = randomUUID()
    const muster = Promise.withResolvers()
    table.set(id, { id, sessionId, ...describe(req), settle: muster.resolve })
    let watch
    try {
      const window = Promise.resolve().then(next)
      // When the window cannot answer (none is open), wait for a present
      // Muster instead; once Muster is gone, give the window's result.
      const gaveUp = (result) => {
        if (!this.present()) return result()
        return new Promise((resolve) => {
          watch = setInterval(() => {
            if (!this.present()) resolve(result())
          }, 1000)
          muster.promise.then(() => clearInterval(watch))
        })
      }
      const fromWindow = window.then(
        (value) => (unanswered(value) ? gaveUp(() => value) : value),
        (error) => gaveUp(() => Promise.reject(error)),
      )
      const cancelled = new Promise((_, reject) => {
        if (original === undefined) return
        const abort = () => reject(original.reason ?? new Error('cancelled'))
        if (original.aborted) abort()
        else original.addEventListener('abort', abort, { once: true })
      })
      const first = await Promise.race([
        muster.promise.then((value) => ({ fromMuster: true, value })),
        fromWindow.then((value) => ({ fromMuster: false, value })),
        cancelled,
      ])
      if (first.fromMuster) {
        window.catch(() => {})
        ours.abort(new Error('answered in Muster'))
      }
      return first.value
    } finally {
      clearInterval(watch)
      table.delete(id)
    }
  }
}

// A host exception may follow acceptance. Only Rust decides whether receipt
// evidence settles it; validation failures before entering the host are separate.
async function hostCall(call) {
  try { return await call() } catch (cause) {
    const error = new Error(String(cause?.message ?? cause))
    error.delivery = 'unknown'
    throw error
  }
}

/** Muster's answers, checked against the questions they answer. */
export function parseAnswers(questions, answers) {
  if (!Array.isArray(answers)) throw new Error('answers must be a list')
  const known = new Map(questions.map((q) => [q.id, q]))
  const seen = new Set()
  return answers.map((answer) => {
    const question = known.get(answer?.id)
    if (question === undefined) throw new Error(`no question ${answer?.id}`)
    if (seen.has(question.id)) throw new Error(`question ${question.id} answered twice`)
    seen.add(question.id)
    const labels = new Set((question.options ?? []).map((o) => o.label))
    const selected = Array.isArray(answer.selected) ? answer.selected : []
    if (selected.some((label) => !labels.has(label))) {
      throw new Error(`question ${question.id} has no such option`)
    }
    if (!question.multiSelect && selected.length > 1) {
      throw new Error(`question ${question.id} takes one option`)
    }
    const custom = typeof answer.custom === 'string' && answer.custom.trim() ? answer.custom : undefined
    return { id: question.id, selected, ...(custom === undefined ? {} : { custom }) }
  })
}

/**
 * Serve the bridge on `socket`: one JSON request per line. Never throws: a
 * socket that cannot be served leaves the bridge off and the app running.
 */
export function serve(bridge, socket, log = () => {}) {
  const off = async () => {}
  if (Buffer.byteLength(socket) > MAX_SOCKET_PATH) {
    log(`not serving: the socket path is longer than ${MAX_SOCKET_PATH} bytes (${socket})`)
    return off
  }
  try {
    fs.mkdirSync(path.dirname(socket), { recursive: true, mode: 0o700 })
    fs.chmodSync(path.dirname(socket), 0o700)
    fs.rmSync(socket, { force: true })
  } catch (error) {
    log(`not serving: ${error.message}`)
    return off
  }
  const server = net.createServer((connection) => {
    let buffer = ''
    connection.setEncoding('utf8')
    connection.on('data', (chunk) => {
      buffer += chunk
      if (Buffer.byteLength(buffer) > MAX_LINE) {
        connection.destroy()
        return
      }
      let newline
      while ((newline = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, newline)
        buffer = buffer.slice(newline + 1)
        respond(bridge, line).then((reply) => {
          if (!connection.destroyed) connection.write(`${JSON.stringify(reply)}\n`)
        })
      }
    })
    connection.on('error', () => {})
  })
  server.on('error', (error) => log(`not serving: ${error.message}`))
  server.listen(socket, () => {
    try {
      fs.chmodSync(socket, 0o600)
      log(`listening on ${socket}`)
    } catch (error) {
      log(`not serving: ${error.message}`)
      server.close()
    }
  })
  return () =>
    new Promise((resolve) => {
      server.close(() => resolve())
      fs.rmSync(socket, { force: true })
    })
}

async function respond(bridge, line) {
  let request
  try {
    request = JSON.parse(line)
  } catch {
    return { id: null, ok: false, error: 'not JSON' }
  }
  try {
    return { id: request?.id ?? null, ok: true, result: await bridge.handle(request),
      ...(request?.control === undefined ? {} : { control: request.control }) }
  } catch (error) {
    return { id: request?.id ?? null, ok: false, error: String(error?.message ?? error),
      delivery: error?.delivery ?? 'not_started',
      ...(request?.control === undefined ? {} : { control: request.control }) }
  }
}

const describeApproval = (req) => ({
  toolName: String(req.toolName ?? ''),
  ...(req.callId === undefined ? {} : { callId: String(req.callId) }),
  ...(req.reason === undefined ? {} : { reason: String(req.reason) }),
})

const describeQuestions = (req) => ({
  ...(req.wait?.callId === undefined ? {} : { callId: String(req.wait.callId) }),
  questions: (req.questions ?? []).map((q) => ({
    id: q.id,
    question: q.question,
    ...(q.header === undefined ? {} : { header: q.header }),
    ...(q.detail === undefined ? {} : { detail: q.detail }),
    options: (q.options ?? []).map((o) => ({ label: o.label, description: o.description ?? '' })),
    multiSelect: q.multiSelect === true,
  })),
})

export function apply(ctx) {
  const bridge = new Bridge(ctx)
  const options = { prepend: true, global: true }
  ctx.on(
    'approval/request',
    (req, next) => bridge.offer(bridge.approvals, req, next, describeApproval, (o) => o === 'unavailable'),
    options,
  )
  ctx.on(
    'user-questions/request',
    (req, next) => bridge.offer(bridge.questions, req, next, describeQuestions, () => false),
    options,
  )
  const log = (message) => {
    try {
      ctx.logger?.info?.('muster bridge: %s', message)
    } catch {}
  }
  // Cordis runs a plugin function as a constructor and drops its result:
  // the socket is an effect of the plugin's own fiber, closed when it unloads.
  ctx.effect(() => serve(bridge, socketPath(), log), 'muster bridge: socket')
}
