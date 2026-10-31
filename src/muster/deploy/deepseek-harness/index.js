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
export const inject = ['agents', 'sessionController']

/** The wire protocol; Muster refuses a bridge that speaks another. */
export const PROTOCOL = 1

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
  constructor(ctx) {
    this.ctx = ctx
    /** Waiting approvals and question cards by their bridge id. */
    this.approvals = new Map()
    this.questions = new Map()
    /** When Muster last asked anything: only a present Muster is waited for. */
    this.lastSeen = -Infinity
  }

  present(now = Date.now()) {
    return now - this.lastSeen < PRESENCE_MS
  }

  state() {
    const sessions = this.ctx.agents
      .list()
      .map((agent) => ({ id: agent.id, running: agent.status === 'running' }))
    const pending = (table) => [...table.values()].map(({ settle, ...visible }) => visible)
    return {
      protocol: PROTOCOL,
      pid: process.pid,
      sessions,
      approvals: pending(this.approvals),
      questions: pending(this.questions),
    }
  }

  async handle(request) {
    this.lastSeen = Date.now()
    switch (request.op) {
      case 'hello':
        return { protocol: PROTOCOL, pid: process.pid }
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
        return await this.ctx.sessionController.prompt(
          { requestId, sessionId, mode, content: [{ type: 'text', text }] },
          AbortSignal.timeout(PROMPT_TIMEOUT_MS),
        )
      }
      case 'cancel': {
        if (typeof request.sessionId !== 'string') throw new Error('cancel needs a sessionId')
        return await this.ctx.sessionController.cancel({ sessionId: request.sessionId })
      }
      case 'approve': {
        const approval = this.approvals.get(request.approvalId)
        if (approval === undefined) throw new Error('no such approval is waiting')
        if (!OUTCOMES.has(request.outcome)) throw new Error(`unknown outcome ${request.outcome}`)
        approval.settle(request.outcome)
        return { accepted: true }
      }
      case 'answer': {
        const question = this.questions.get(request.questionId)
        if (question === undefined) throw new Error('no such question is waiting')
        const answers = parseAnswers(question.questions, request.answers)
        question.settle({ answers })
        return { accepted: true }
      }
      default:
        throw new Error(`unknown op ${request.op}`)
    }
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
      if (buffer.length > MAX_LINE) {
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
    return { id: request.id ?? null, ok: true, result: await bridge.handle(request) }
  } catch (error) {
    return { id: request.id ?? null, ok: false, error: String(error?.message ?? error) }
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
