import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { cachedControls, readClaudeControls, readCodexControls, isolatedClaudeOptions } from '../controls';
import { claudeTaskControls, claudeEngine } from '../claude';
import { Application } from '../application';
import { TaskInput, EngineControls } from '../protocol';

const input = (): TaskInput => ({ id: randomUUID(), sessionId: randomUUID(), projectId: 'a'.repeat(64),
  engine: 'claude', model: 'fixture', mode: 'develop', prompt: 'Protocol fixture, no model inference' });
test('native model controls preserve allowlists and require successful Auto activation, not a model-name guess', async () => {
  const requests: string[] = [];
  const controls = await readClaudeControls({ supportedModels: async () => [
    { value: 'alias', resolvedModel: 'fixture', displayName: 'Fixture', description: '', supportsEffort: true,
      supportedEffortLevels: ['low', 'xhigh'], supportsAutoMode: true },
    { value: 'blocked', displayName: '', description: '', supportsAutoMode: true },
    { value: 'not-granted', displayName: '', description: '', supportsEffort: true, supportedEffortLevels: ['max'] },
  ], setModel: async model => { requests.push(model!); }, setPermissionMode: async mode => {
    if (mode === 'auto' && requests.at(-1) === 'blocked') throw Error('Native account restriction');
    requests.push(mode);
  } }, ['fixture', 'blocked', 'unknown']);
  assert.deepEqual(controls.models.fixture, { efforts: ['low', 'xhigh'], permissionModes: ['default', 'auto'] });
  assert.deepEqual(controls.models.blocked, { efforts: [], permissionModes: ['default'] });
  assert.equal(controls.models.unknown, undefined); assert.equal(controls.models['not-granted'], undefined);
  assert.equal(requests.at(-1), 'default');
});
test('controls coalesce concurrent native initialization and reject malformed metadata', async () => {
  let calls = 0; const get = cachedControls(async () => { calls++; return { version: 1, models: {} }; });
  await Promise.all([get(), get(), get()]); await get(); assert.equal(calls, 1);
  assert.throws(() => EngineControls.parse({ version: 1, models: { fixture: { efforts: ['low'], defaultEffort: 'high', permissionModes: ['default'] } } }));
  assert.equal(TaskInput.safeParse({ ...input(), enginePermissionMode: 'bypassPermissions' }).success, false);
});
test('Claude Auto does not preapprove MCP or override a classifier denial; local tools and hooks stay disabled', async () => {
  const task = { ...input(), reasoningEffort: 'xhigh' as const, enginePermissionMode: 'auto' as const };
  const options = claudeTaskControls(task);
  assert.equal(options.effort, 'xhigh'); assert.deepEqual(options.allowedTools, []);
  assert.equal((await options.canUseTool!('mcp__acode__command_run', {}, { signal: new AbortController().signal, toolUseID: 'fixture', requestId: 'fixture' }))?.behavior, 'deny');
  assert.deepEqual(isolatedClaudeOptions('/private/isolated'), { cwd: '/private/isolated', tools: [], settingSources: [], settings: { disableAllHooks: true } });
  assert.equal(claudeTaskControls({ ...input(), mode: 'plan' }).permissionMode, 'plan');
  assert.throws(() => claudeTaskControls({ ...task, mode: 'plan' }));
});
test('Codex controls read only the pinned native catalog, not another model or provider preset', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-controls-')); const file = path.join(root, 'catalog.json');
  try {
    await writeFile(file, JSON.stringify({ models: [{ slug: 'fixture', supported_reasoning_levels: [{ effort: 'low' }, { effort: 'high' }, { effort: 'invented' }], default_reasoning_level: 'high' }] }));
    assert.deepEqual(await readCodexControls(file, ['fixture', 'unknown']), { version: 1, models: {
      fixture: { efforts: ['low', 'high'], defaultEffort: 'high', permissionModes: ['default'] } } });
    assert.equal((await readCodexControls(file, ['fixture'], 'low')).models.fixture.defaultEffort, 'low');
  } finally { await rm(root, { recursive: true, force: true }); }
});
test('Claude configures actual native controls before releasing the prompt and rejects unsupported Auto without sending it', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-claude-gate-'));
  try {
    for (const available of [true, false]) {
      const order: string[] = []; let closed = false;
      const task = { ...input(), reasoningEffort: 'high' as const, enginePermissionMode: 'auto' as const };
      const run = claudeEngine(root, undefined, ({ prompt, options }) => {
        assert.equal(options.effort, 'high'); assert.deepEqual(options.tools, []);
        const stream = (async function* () {
          for await (const message of prompt) { assert.equal(message.message.content, task.prompt); order.push('prompt'); }
          yield { type: 'result', subtype: 'success', is_error: false, usage: { input_tokens: 0, output_tokens: 0 }, modelUsage: {} };
        })();
        return Object.assign(stream, {
          initializationResult: async () => { order.push('initialize'); return {}; },
          supportedModels: async () => [{ value: 'fixture', supportsEffort: true, supportedEffortLevels: ['high'] }],
          setPermissionMode: async (mode: string) => { assert.equal(mode, 'auto'); order.push('auto'); if (!available) throw Error('Auto unavailable'); },
          close: () => { closed = true; },
        }) as any;
      });
      const work = run({ input: task, session: { id: task.sessionId, peerId: 'B', projectId: task.projectId, engine: 'claude', model: 'fixture' },
        signal: new AbortController().signal, text: async () => {}, checkpoint: async () => {}, call: async () => { throw Error('No tool in SDK fixture'); } });
      if (available) { await work; assert.deepEqual(order, ['initialize', 'auto', 'prompt']); }
      else { await assert.rejects(work, /Auto unavailable/); assert.deepEqual(order, ['initialize', 'auto']); }
      assert.equal(closed, true);
    }
  } finally { await rm(root, { recursive: true, force: true }); }
});
test('admission persists per-turn settings without replacing sessions, rejects unavailable controls before inference and forbids changed retries', { timeout: 10000 }, async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-control-task-'));
  const peer = { id: 'B', tokenSha256: 'b'.repeat(64), projects: ['a'.repeat(64)] }, seen: TaskInput[] = [];
  const app = await Application.open(root, [peer], { claude: { models: ['fixture'], controls: async () => ({ version: 1,
    models: { fixture: { efforts: ['low', 'high'], permissionModes: ['default', 'auto'] } } }), run: async c => { seen.push(c.input); } } });
  const completed = async (id: string) => {
    let cursor = 0;
    while (true) {
      // The public event status stays running until durable session cleanup finishes.
      const batch = await app.broker.events(peer, id, cursor);
      cursor += batch.events.length;
      if (batch.status !== 'running') { assert.equal(batch.status, 'completed'); return; }
    }
  };
  try {
    const caps: any = await app.dispatch('B', 'capabilities', {}); assert.equal(caps.engines.claude.controls.version, 1);
    const first = { ...input(), reasoningEffort: 'high' as const, enginePermissionMode: 'auto' as const };
    await app.broker.submit(peer, first);
    await completed(first.id);
    await app.broker.submit(peer, first); assert.equal(seen.length, 1);
    await assert.rejects(app.broker.submit(peer, { ...first, reasoningEffort: 'low' }), /409/);
    await assert.rejects(app.broker.submit(peer, { ...input(), reasoningEffort: 'max' }), /unavailable/);
    await assert.rejects(app.broker.submit(peer, { ...input(), mode: 'plan', enginePermissionMode: 'auto' }), /unavailable/);
    const next = { ...input(), sessionId: first.sessionId, reasoningEffort: 'low' as const };
    await app.broker.submit(peer, next);
    await completed(next.id);
    assert.equal(seen.length, 2); assert.equal(seen[0].sessionId, seen[1].sessionId);
  } finally { await app.close(); await rm(root, { recursive: true, force: true }); }
});
