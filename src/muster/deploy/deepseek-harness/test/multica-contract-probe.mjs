// Opt-in P0 probe of the installed App host. No production profile or task.
// Run once with MODE=create, then restart the same isolated profile with
// MODE=lookup. Lookup never creates, resumes or sends a prompt.
import { randomUUID } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { Bridge } from '../index.js'

export const name = 'muster-multica-contract-probe'
export const inject = ['agents', 'sessionController', 'sessionQuery']
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

export function apply(ctx) {
  const home = fs.realpathSync(process.env.DSH_HOME)
  if (!home.startsWith('/private/tmp/muster-dsh-p0-')) {
    throw new Error('P0 probe requires a dedicated temporary DSH_HOME')
  }
  const mode = process.env.MUSTER_DSH_PROBE_MODE
  if (mode !== 'create' && mode !== 'lookup') throw new Error('Explicit create or lookup mode required')
  const requestPath = path.join(home, 'p0-request.json')
  const reportPath = path.join(home, `p0-${mode}.json`)
  const write = (report) => fs.writeFileSync(reportPath, JSON.stringify(report, null, 2), { mode: 0o600 })
  ctx.effect(async () => {
    try {
      if (mode === 'lookup') {
        const request = JSON.parse(fs.readFileSync(requestPath, 'utf8'))
        const observation = await ctx.sessionQuery.observeSession(request.sessionId)
        try {
          const events = observation.events
          write({ mode, headerMatches: observation.header.id === request.sessionId && observation.header.cwd === home,
            nativePromptPersisted: events.some((e) => e.type === 'user/message' && e.data?.source?.rpcId === request.requestId),
            eventTypes: events.map((e) => e.type),
            eventShape: events.filter((e) => ['user/message', 'turn/start', 'turn/end'].includes(e.type)).map((e) => ({ type: e.type, keys: Object.keys(e), dataKeys: Object.keys(e.data ?? {}), turn: e.data?.turn })),
            liveAgentCount: ctx.agents.list().length,
            performedCreate: false, performedResume: false, performedPrompt: false })
        } finally { observation[Symbol.dispose]() }
      } else {
        if (ctx.agents.list().length !== 0) throw new Error('Profile must have no live sessions')
        const request = { sessionId: `session-${randomUUID()}`, requestId: randomUUID() }
        fs.writeFileSync(requestPath, JSON.stringify(request), { flag: 'wx', mode: 0o600 })
        const created = await ctx.sessionController.create({ cwd: home, sessionId: request.sessionId })
        if (created.sessionId !== request.sessionId) throw new Error('Host returned a different native session')
        const agent = ctx.agents.get(created.sessionId)
        const before = agent.session.snapshotEvents()
        const result = await ctx.sessionController.prompt({ ...request, mode: 'queue',
          content: [{ type: 'text', text: 'Synthetic Muster contract probe. Do not run any tools. Reply OK.' }] },
          AbortSignal.timeout(5000))
        const end = Date.now() + 5000
        while (Date.now() < end && !agent.session.snapshotEvents().some((e) => e.type === 'turn/end')) await pause(50)
        const events = agent.session.snapshotEvents()
        const bridge = new Bridge(ctx)
        const report = { mode, nativeIdMatches: created.sessionId === request.sessionId,
          headerMatches: agent.session.header.id === request.sessionId && agent.session.header.cwd === home,
          initiallyBlank: !before.some((e) => e.type === 'turn/start' || e.type === 'user/message'),
          accepted: result?.accepted === true,
          receipt: bridge.lookup(request.sessionId, request.requestId),
          eventTypes: events.map((e) => e.type),
          // Only structural facts, no model output, prompt text or account data.
          turnFacts: events.filter((e) => ['user/message', 'turn/start', 'turn/end'].includes(e.type)).map((e) => ({
            seq: e.seq, type: e.type, turn: e.data?.turn, rpcId: e.data?.source?.rpcId,
            reason: e.type === 'turn/end' ? e.data?.reason?.kind : undefined,
          })),
          modelSelectionMethodPresent: typeof ctx.sessionController.selectModel === 'function',
          taskCredentialCallbackVerified: false,
        }
        write(report)
        if (agent.status === 'running') await ctx.sessionController.cancel({ sessionId: request.sessionId })
      }
    } catch (error) { write({ mode, failed: true, error: String(error) }) }
    return () => {}
  }, 'isolated Multica App contract probe')
}
