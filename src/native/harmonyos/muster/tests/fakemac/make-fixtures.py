#!/usr/bin/env python3
"""Writes synthetic Claude Code and Codex session files for a fake desktop.

Everything here is invented: the user "demo", the projects acme-widgets and
acme-api, the code, the commands and their output. Timestamps are relative to
now so the sessions sort as recent. musterd reads the trees through
CLAUDE_CONFIG_DIR and CODEX_HOME exactly as it reads real ones.

    make-fixtures.py <dir>     -> <dir>/claude, <dir>/codex
"""
import json
import os
import sys
import time
from datetime import datetime, timezone

root = sys.argv[1] if len(sys.argv) > 1 else 'fixtures'
now = time.time()


def ts(minutes_ago):
    return datetime.fromtimestamp(now - minutes_ago * 60, timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.000Z')


def write(path, records):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        for record in records:
            f.write(json.dumps(record, ensure_ascii=False) + '\n')


def usage(inp, out, read=0, write=0):
    return {'input_tokens': inp, 'output_tokens': out, 'cache_read_input_tokens': read, 'cache_creation_input_tokens': write}


# --- Claude Code -----------------------------------------------------------
claude = os.path.join(root, 'claude', 'projects', '-Users-demo-acme-widgets')
cwd = '/Users/demo/acme-widgets'
model = 'claude-sonnet-4-6'


def user(text, minutes, uuid):
    return {'type': 'user', 'uuid': uuid, 'cwd': cwd, 'timestamp': ts(minutes), 'message': {'role': 'user', 'content': text}}


def tool_use(tid, name, args, minutes, req, msg, u):
    return {'type': 'assistant', 'uuid': f'a-{tid}', 'cwd': cwd, 'timestamp': ts(minutes), 'requestId': req,
            'message': {'id': msg, 'role': 'assistant', 'model': model, 'stop_reason': 'tool_use', 'usage': u,
                        'content': [{'type': 'tool_use', 'id': tid, 'name': name, 'input': args}]}}


def tool_result(tid, text, minutes, extra=None):
    record = {'type': 'user', 'uuid': f'r-{tid}', 'cwd': cwd, 'timestamp': ts(minutes),
              'message': {'role': 'user', 'content': [{'type': 'tool_result', 'tool_use_id': tid, 'content': text}]}}
    if extra:
        record['toolUseResult'] = extra
    return record


def reply(text, minutes, req, msg, u):
    return {'type': 'assistant', 'uuid': f'a-{msg}', 'cwd': cwd, 'timestamp': ts(minutes), 'requestId': req,
            'message': {'id': msg, 'role': 'assistant', 'model': model, 'stop_reason': 'end_turn', 'usage': u,
                        'content': [{'type': 'text', 'text': text}]}}


checkout = '11111111-2222-4333-8444-555555555501'
write(os.path.join(claude, f'{checkout}.jsonl'), [
    {'type': 'ai-title', 'sessionId': checkout, 'aiTitle': 'Fix the flaky checkout test'},
    user('The checkout test fails about one run in five. Can you find out why?', 42, 'u1'),
    tool_use('t1', 'Read', {'file_path': f'{cwd}/tests/checkout.test.ts'}, 41, 'r1', 'm1', usage(1800, 60, 0, 1800)),
    tool_result('t1', "it('totals the cart', async () => {\n  const cart = await seedCart();\n  expect(total(cart)).toBe(42.5);\n});", 41),
    tool_use('t2', 'Bash', {'command': 'npm test -- checkout --runInBand', 'description': 'Run the checkout test'}, 40, 'r2', 'm2', usage(40, 45, 1900, 120)),
    tool_result('t2', 'FAIL tests/checkout.test.ts\n  expected 42.5, received 42.49999999999999\nTests: 1 failed, 11 passed', 39),
    tool_use('t3', 'Edit', {'file_path': f'{cwd}/src/cart/total.ts',
                            'old_string': '  return items.reduce((sum, i) => sum + i.price * i.qty, 0);',
                            'new_string': '  const cents = items.reduce((sum, i) => sum + Math.round(i.price * 100) * i.qty, 0);\n  return cents / 100;'},
             37, 'r3', 'm3', usage(60, 210, 2100, 260)),
    tool_result('t3', 'The file src/cart/total.ts has been updated.', 37, {'structuredPatch': [
        {'oldStart': 8, 'oldLines': 1, 'newStart': 8, 'newLines': 2,
         'lines': ['-  return items.reduce((sum, i) => sum + i.price * i.qty, 0);',
                   '+  const cents = items.reduce((sum, i) => sum + Math.round(i.price * 100) * i.qty, 0);',
                   '+  return cents / 100;']}]}),
    tool_use('t4', 'Bash', {'command': 'npm test -- checkout --runInBand', 'description': 'Rerun the checkout test'}, 36, 'r4', 'm4', usage(30, 40, 2400, 90)),
    tool_result('t4', 'PASS tests/checkout.test.ts (3.1 s)\nTests: 12 passed', 35),
    reply("## Cause\n\nThe total was summed in **floating point**, so some seeded carts came out as "
          "`42.49999999999999` instead of `42.5`. The seed data is random, which is why it failed only sometimes.\n\n"
          "## Fix\n\n- Sum in integer cents in [total.ts](src/cart/total.ts:8)\n- Divide once at the end\n\n"
          "```ts\nconst cents = items.reduce((sum, i) => sum + Math.round(i.price * 100) * i.qty, 0);\nreturn cents / 100;\n```\n\n"
          "> The test now passes 50 runs in a row.", 34, 'r5', 'm5', usage(25, 180, 2600, 0)),
    user('Thanks. Please add a regression test with a cart that used to round wrong.', 20, 'u2'),
    tool_use('t5', 'Write', {'file_path': f'{cwd}/tests/total.regression.test.ts',
                             'content': "import { total } from '../src/cart/total';\n\nit('sums 0.1 + 0.2 exactly', () => {\n  expect(total([{ price: 0.1, qty: 1 }, { price: 0.2, qty: 1 }])).toBe(0.3);\n});\n"},
             19, 'r6', 'm6', usage(30, 150, 2700, 200)),
    tool_result('t5', 'File created successfully at: tests/total.regression.test.ts', 19,
                {'type': 'create', 'content': "import { total } from '../src/cart/total';\n\nit('sums 0.1 + 0.2 exactly', () => {\n  expect(total([{ price: 0.1, qty: 1 }, { price: 0.2, qty: 1 }])).toBe(0.3);\n});\n"}),
    reply('Added `tests/total.regression.test.ts`: `0.1 + 0.2` must total exactly `0.3`. It fails on the old code and passes now.',
          18, 'r7', 'm7', usage(20, 60, 2900, 0)),
])

release = '11111111-2222-4333-8444-555555555502'
write(os.path.join(claude, f'{release}.jsonl'), [
    {'type': 'custom-title', 'sessionId': release, 'customTitle': 'Release checklist for 2.4'},
    user('What is left before we can tag 2.4?', 300, 'u1'),
    reply("Three things:\n\n1. Update `CHANGELOG.md`\n2. Bump the version in `package.json`\n3. Run the smoke tests on staging\n\n"
          "Everything else on the checklist is done.", 299, 'q1', 'n1', usage(900, 70, 0, 900)),
])

upgrade = '11111111-2222-4333-8444-555555555503'
write(os.path.join(claude, f'{upgrade}.jsonl'), [
    {'type': 'ai-title', 'sessionId': upgrade, 'aiTitle': 'Upgrade the date library'},
    user('Upgrade date-fns to v4 and fix whatever breaks.', 3, 'u1'),
    {'type': 'assistant', 'uuid': 'a-w1', 'cwd': cwd, 'timestamp': ts(2), 'requestId': 'w1',
     'message': {'id': 'w1', 'role': 'assistant', 'model': model, 'stop_reason': 'tool_use', 'usage': usage(1500, 80, 0, 1500),
                 'content': [{'type': 'text', 'text': 'I will check which modules import date-fns first.'},
                             {'type': 'tool_use', 'id': 'w-t1', 'name': 'Grep', 'input': {'pattern': 'date-fns', 'path': 'src'}}]}},
    tool_result('w-t1', 'src/orders/format.ts\nsrc/reports/range.ts', 2),
    tool_use('w-t2', 'Bash', {'command': 'npm install date-fns@4', 'description': 'Install date-fns 4'}, 1, 'w2', 'w2m', usage(40, 30, 1600, 60)),
])

# --- Codex ------------------------------------------------------------------
day = datetime.fromtimestamp(now, timezone.utc)
codex_dir = os.path.join(root, 'codex', 'sessions', day.strftime('%Y'), day.strftime('%m'), day.strftime('%d'))
orders = '22222222-3333-4444-8555-666666666601'
api = '/Users/demo/acme-api'
write(os.path.join(codex_dir, f'rollout-{day.strftime("%Y-%m-%dT%H-%M-%S")}-{orders}.jsonl'), [
    {'type': 'session_meta', 'timestamp': ts(15), 'payload': {'id': orders, 'cwd': api}},
    {'type': 'turn_context', 'timestamp': ts(15), 'payload': {'cwd': api, 'model': 'gpt-5.1-codex'}},
    {'type': 'event_msg', 'timestamp': ts(15), 'payload': {'type': 'task_started'}},
    {'type': 'response_item', 'timestamp': ts(15), 'payload': {'type': 'message', 'role': 'user',
     'content': [{'type': 'input_text', 'text': 'Add cursor pagination to GET /api/orders.'}]}},
    {'type': 'response_item', 'timestamp': ts(14), 'payload': {'type': 'function_call', 'name': 'shell', 'call_id': 'c1',
     'arguments': json.dumps({'command': ['rg', '-n', 'listOrders', 'src']})}},
    {'type': 'response_item', 'timestamp': ts(14), 'payload': {'type': 'function_call_output', 'call_id': 'c1',
     'output': 'src/orders/routes.ts:12:router.get("/orders", listOrders)\nsrc/orders/service.ts:30:export async function listOrders(req, res) {'}},
    {'type': 'event_msg', 'timestamp': ts(12), 'payload': {'type': 'item_completed', 'item': {'type': 'FileChange', 'status': 'completed', 'id': 'f1',
     'changes': {f'{api}/src/orders/service.ts': {'type': 'update', 'unified_diff': '@@ -30,3 +30,6 @@\n export async function listOrders(req, res) {\n-  const rows = await db.orders.findAll();\n+  const after = req.query.after ?? null;\n+  const rows = await db.orders.page({ after, limit: 50 });\n+  res.set("Next-Cursor", rows.at(-1)?.id ?? "");\n   res.json(rows);\n'}}}}},
    {'type': 'event_msg', 'timestamp': ts(11), 'payload': {'type': 'token_count', 'info': {
     'total_token_usage': {'input_tokens': 5200, 'cached_input_tokens': 4100, 'output_tokens': 640},
     'last_token_usage': {'input_tokens': 5200, 'cached_input_tokens': 4100, 'output_tokens': 640}}}},
    {'type': 'response_item', 'timestamp': ts(11), 'payload': {'type': 'message', 'role': 'assistant',
     'content': [{'type': 'output_text', 'text': 'Done. `GET /api/orders` now takes `?after=<id>` and returns at most 50 rows, with the next cursor in the `Next-Cursor` header.'}]}},
    {'type': 'event_msg', 'timestamp': ts(11), 'payload': {'type': 'task_complete'}},
])
with open(os.path.join(root, 'codex', 'session_index.jsonl'), 'w') as f:
    f.write(json.dumps({'id': orders, 'thread_name': 'Paginate the orders API'}) + '\n')
print(f'fixtures in {root}')
