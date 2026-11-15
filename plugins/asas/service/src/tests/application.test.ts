import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { Application } from '../application';
import { saveJson } from '../storage';
import type { Engine, Engines } from '../broker';
import type { TaskInput } from '../protocol';

const projectId = 'a'.repeat(64), peer = { id: 'B', tokenSha256: 'b'.repeat(64), projects: [projectId] };
const task = (engine: TaskInput['engine']): TaskInput => ({ id: randomUUID(), sessionId: randomUUID(), projectId,
  engine, prompt: 'Explicit protocol fixture; no model inference', model: 'fixture', mode: 'develop' });

test('one host runs separate Codex/Claude fixture engines without cross-engine session reuse', { timeout: 10000 }, async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-engine-'));
  let claudeCalls = 0, codexCalls = 0, release!: () => void;
  const ready = new Promise<void>(resolve => { release = resolve; });
  const run: Engine = async context => { if (context.input.engine === 'claude') claudeCalls++; else codexCalls++;
    await ready; await context.text(context.input.engine + ' fixture'); };
  const engines: Engines = { claude: { models: ['fixture'], run }, codex: { models: ['fixture'], run } };
  const app = await Application.open(root, [peer], engines);
  t.after(async () => { release(); await app.close(); await rm(root, { recursive: true, force: true }); });
  const claude = task('claude'), codex = task('codex');
  await app.dispatch('B', 'submit', claude); await app.dispatch('B', 'submit', codex);
  await assert.rejects(app.dispatch('B', 'submit', { ...task('codex'), sessionId: claude.sessionId }), /403/);
  await assert.rejects(app.dispatch('unknown', 'submit', task('claude')), /403/);
  assert.deepEqual(Object.keys(app.health().engines).sort(), ['claude', 'codex']);
  const capabilities = await app.dispatch('B', 'capabilities', {}) as { peerId: string; projects: string[] };
  assert.equal(capabilities.peerId, 'B'); assert.deepEqual(capabilities.projects, [projectId]);
  await assert.rejects(app.dispatch('unknown', 'capabilities', {}), /403/);
  release();
  for (const input of [claude, codex]) {
    let after = 0, status = 'running';
    while (status === 'running') { const page = await app.broker.events(peer, input.id, after); status = page.status; after += page.events.length; }
    assert.equal(status, 'completed');
  }
  assert.equal(claudeCalls, 1); assert.equal(codexCalls, 1);
});

test('exclusive data ownership and malformed durable records fail closed', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-data-'));
  const engines: Engines = { claude: { models: ['fixture'], run: async () => {} } };
  const app = await Application.open(root, [peer], engines);
  try { await assert.rejects(Application.open(root, [peer], engines), /locked/); }
  finally { await app.close(); }
  const input = task('claude');
  await saveJson(path.join(root, 'tasks', input.id + '.json'), { ...input, peerId: 'B', status: 'completed',
    events: [{ taskId: input.id, sequence: 2, type: 'completed' }] });
  try { await assert.rejects(Application.open(root, [peer], engines), /sequence/); }
  finally { await rm(root, { recursive: true, force: true }); }
});

test('concurrent duplicate tool receipts are durable and resolve the engine only once', { timeout: 10000 }, async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-receipt-'));
  let resumed = 0;
  const app = await Application.open(root, [peer], { claude: { models: ['fixture'], run: async context => {
    await context.call('file_read', { path: 'fixture.txt' }); resumed++;
  } } });
  t.after(async () => { await app.close(); await rm(root, { recursive: true, force: true }); });
  const input = task('claude'); await app.dispatch('B', 'submit', input);
  let after = 0, callId: string | undefined;
  while (!callId) { const page = await app.broker.events(peer, input.id, after); after += page.events.length;
    callId = page.events.find(event => event.type === 'tool')?.callId; }
  const result = { callId, ok: true, output: '工具结果 中文 😀' };
  await Promise.all([app.broker.result(peer, input.id, result), app.broker.result(peer, input.id, result)]);
  await assert.rejects(app.broker.result(peer, input.id, { ...result, output: 'conflict' }), /409/);
  assert.equal(app.broker.get(peer, input.id).events.filter(event => event.type === 'tool_result').length, 1);
  while (app.broker.get(peer, input.id).status === 'running') await app.broker.events(peer, input.id, app.broker.get(peer, input.id).events.length);
  assert.equal(resumed, 1);
});
