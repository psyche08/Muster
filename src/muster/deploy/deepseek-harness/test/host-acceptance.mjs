// Explicit fixture for an isolated instance of the installed App host.
import fs from 'node:fs'
import path from 'node:path'

export const name = 'muster-local-acceptance'
export const inject = ['agents', 'sessionController']

export function apply(ctx) {
  const home = fs.realpathSync(process.env.DSH_HOME)
  if (!home.startsWith('/private/tmp/muster-dsh-')) {
    throw new Error('Acceptance requires a dedicated temporary DSH_HOME')
  }
  ctx.effect(async () => {
    const created = await ctx.sessionController.create({ cwd: home })
    fs.writeFileSync(path.join(home, 'acceptance-session.json'), JSON.stringify(created), { mode: 0o600 })
    return () => {}
  }, 'create isolated acceptance session')
}
