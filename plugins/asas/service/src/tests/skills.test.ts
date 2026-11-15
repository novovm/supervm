import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { Application } from '../application';
import { TOOL_CAPABILITY, type TaskInput } from '../protocol';
import type { Engine, Peer } from '../broker';

const node = (n: string) => 'novovm-mldsa65:' + n.repeat(64), projectId = 'a'.repeat(64);
const input = (engine: 'claude' | 'codex', workerNodeId: string): TaskInput => ({ id: randomUUID(), sessionId: randomUUID(), projectId, engine,
  model: 'fixture', prompt: 'Explicit fixture, no inference', mode: 'develop', workerNodeId, toolContract: TOOL_CAPABILITY });

test('one A isolates two B nodes even with the same principal/project and same skill name', { timeout: 10000 }, async t => {
  const directory = await mkdtemp(path.join(tmpdir(), 'asas-skill-owner-')), grant = { id: 'grant', tokenSha256: 'b'.repeat(64), projects: [projectId] };
  const seen: string[] = [];
  const run: Engine = async context => {
    const owner = { nodeId: context.input.workerNodeId!, projectId };
    const wrong = await context.call('skills_list', { owner: { ...owner, nodeId: node('f') } }); assert.equal(wrong.ok, false);
    const receipt = await context.call('skills_list', { owner }); seen.push(JSON.parse(receipt.output).owner.nodeId);
  };
  const app = await Application.open(directory, [grant], { codex: { models: ['fixture'], run }, claude: { models: ['fixture'], run } });
  t.after(async () => { await app.close(); await rm(directory, { recursive: true, force: true }); });
  for (const [engine, owner] of [['codex', node('c')], ['claude', node('d')]] as const) {
    const task = input(engine, owner), peer: Peer = { ...grant, nodeId: owner };
    await app.dispatch('grant', 'submit', task, owner);
    await assert.rejects(app.dispatch('grant', 'events', { taskId: task.id, after: 0, waitMs: 0 }, node('e')), /404/);
    await assert.rejects(app.dispatch('grant', 'submit', { ...input(engine, node('e')), sessionId: task.sessionId }, node('e')), /403/);
    let callId: string | undefined, after = 0;
    while (!callId) { const page = await app.broker.events(peer, task.id, after); after += page.events.length; callId = page.events.find(event => event.type === 'tool')?.callId; }
    const result = { callId, ok: true, output: JSON.stringify({ owner: { nodeId: node('e'), projectId }, skills: [], offset: 0, total: 0, nextOffset: null }) };
    await assert.rejects(app.broker.result(peer, task.id, result), /another worker/);
    result.output = JSON.stringify({ owner: { nodeId: owner, projectId }, skills: [{ skillId: '1'.repeat(64), sha256: '2'.repeat(64), name: 'same-name', description: 'fixture', projectPath: '.agents/skills/same-name' }], offset: 0, total: 1, nextOffset: null });
    await app.broker.result(peer, task.id, result);
    while (app.broker.get(peer, task.id).status === 'running') await app.broker.events(peer, task.id, app.broker.get(peer, task.id).events.length);
    assert.equal(app.broker.get(peer, task.id).status, 'completed');
    const caps = await app.dispatch('grant', 'capabilities', {}, owner) as { workerNodeId: string }; assert.equal(caps.workerNodeId, owner);
  }
  assert.deepEqual(seen, [node('c'), node('d')]);
  await assert.rejects(app.dispatch('grant', 'submit', input('codex', node('c')), node('d')), /403/);
  await assert.rejects(app.dispatch('grant', 'submit', input('codex', node('c'))), /403/);
});

test('A verifies skill bundle and text hashes before persisting or forwarding a successful receipt', { timeout: 10000 }, async t => {
  const directory = await mkdtemp(path.join(tmpdir(), 'asas-skill-receipt-')), workerNodeId = node('c');
  const peer: Peer = { id: 'B', tokenSha256: 'b'.repeat(64), projects: [projectId], nodeId: workerNodeId };
  const text = 'Exact skill instruction', resourceSha256 = createHash('sha256').update(text).digest('hex');
  const resources = [{ path: 'SKILL.md', sha256: resourceSha256, bytes: Buffer.byteLength(text) }];
  const sha256 = createHash('sha256').update(JSON.stringify(resources)).digest('hex');
  const owner = { nodeId: workerNodeId, projectId }, skill = { skillId: '1'.repeat(64), sha256, name: 'same-name', description: 'fixture', projectPath: '.agents/skills/same-name' };
  const app = await Application.open(directory, [peer], { codex: { models: ['fixture'], run: async context => {
    assert.equal((await context.call('skills_read', { owner, skillId: skill.skillId, sha256 })).ok, true);
  } } });
  t.after(async () => { await app.close(); await rm(directory, { recursive: true, force: true }); });
  const task = input('codex', workerNodeId); await app.broker.submit(peer, task);
  let callId: string | undefined, after = 0;
  while (!callId) { const page = await app.broker.events(peer, task.id, after); after += page.events.length; callId = page.events.find(event => event.type === 'tool')?.callId; }
  const receipt = { owner, skill, resource: 'SKILL.md', resourceSha256, text, resources };
  await assert.rejects(app.broker.result(peer, task.id, { callId, ok: true, output: JSON.stringify({ ...receipt, text: 'changed' }) }), /digest/);
  await app.broker.result(peer, task.id, { callId, ok: true, output: JSON.stringify(receipt) });
  while (app.broker.get(peer, task.id).status === 'running') await app.broker.events(peer, task.id, app.broker.get(peer, task.id).events.length);
  assert.equal(app.broker.get(peer, task.id).status, 'completed');
});
