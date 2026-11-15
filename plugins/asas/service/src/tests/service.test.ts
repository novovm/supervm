import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { startService } from '../server';
import { AsasClient } from '../client';
import { endpoint, type TaskInput } from '../protocol';

const token = 'explicit-protocol-fixture-token-'.repeat(2), project = 'a'.repeat(64);
const peer = { id: 'B', tokenSha256: createHash('sha256').update(token).digest('hex'), projects: [project] };
const input = (): TaskInput => ({ id: randomUUID(), sessionId: randomUUID(), projectId: project, engine: 'claude', prompt: 'Protocol fixture, no inference', model: 'default', mode: 'develop' });
test('HTTPS is mandatory except exact loopback; URL credentials and query secrets are rejected', () => {
  assert.throws(() => endpoint('http://example.com')); assert.throws(() => endpoint('https://u:p@example.com'));
  assert.throws(() => endpoint('https://example.com/?token=secret')); assert.equal(endpoint('http://127.0.0.1:1234').protocol, 'http:');
});
test('actual HTTP task/tool/result flow is peer-bound, durable and admission-idempotent', { timeout: 10000 }, async t => {
  const directory = await mkdtemp(path.join(tmpdir(), 'asas-service-'));
  let runs = 0, workerCalls = 0;
  const service = await startService({ directory, host: '127.0.0.1', port: 0, peers: [peer], models: ['default'] }, async context => {
    runs++; await context.text('Fixture start');
    const result = await context.call('file_read', { path: 'test.txt' }); assert.equal(result.output, 'actual worker result');
    await context.text('Fixture done');
  }); t.after(async () => { await service.close(); await rm(directory, { recursive: true, force: true }); });
  const client = new AsasClient(`http://127.0.0.1:${service.port}`, token), task = input();
  assert.equal((await client.health()).peerId, 'B');
  const events: string[] = [];
  assert.equal(await client.follow(task, path.join(directory, 'client.json'), async event => {
    workerCalls++; return { callId: event.callId!, ok: true, output: 'actual worker result' };
  }, event => events.push(event.type), new AbortController().signal), 'completed');
  assert.equal(runs, 1); assert.equal(workerCalls, 1); assert.ok(events.includes('completed'));
  await client.submit(task); assert.equal(runs, 1);
  await assert.rejects(client.submit({ ...task, prompt: 'different' }), /409/);
  await assert.rejects(client.submit({ ...input(), projectId: 'b'.repeat(64) }), /403/);
  const unauthorized = new AsasClient(`http://127.0.0.1:${service.port}`, 'different-token-'.repeat(4));
  await assert.rejects(unauthorized.health(), /401/);
  const saved = JSON.parse(await readFile(path.join(directory, 'tasks', task.id + '.json'), 'utf8'));
  assert.equal(saved.status, 'completed'); assert.equal(saved.peerId, 'B');
  // Historical tool events must not cause execution after a terminal task.
  assert.equal(await client.follow(task, path.join(directory, 'another-client.json'), async () => { throw Error('must not replay'); }, () => {}, new AbortController().signal), 'completed');
});
test('same session locks out a second writer and restart reports recovery rather than replay', { timeout: 10000 }, async t => {
  const directory = await mkdtemp(path.join(tmpdir(), 'asas-lock-'));
  const config = { directory, host: '127.0.0.1', port: 0, peers: [peer], models: ['default'] };
  let calls = 0;
  const service = await startService(config, async context => { calls++; await context.call('file_read', { path: 'x' }); });
  let closed = false; let restored: Awaited<ReturnType<typeof startService>> | undefined;
  t.after(async () => { if (!closed) await service.close(); await restored?.close(); await rm(directory, { recursive: true, force: true }); });
  const client = new AsasClient(`http://127.0.0.1:${service.port}`, token), task = input(); await client.submit(task);
  await assert.rejects(client.submit({ ...input(), sessionId: task.sessionId }), /409/);
  let after = 0;
  while (true) { const pending = await client.events(task.id, after); after = pending.events.at(-1)?.sequence || after;
    if (pending.events.some(event => event.type === 'tool')) break; }
  await service.close(); closed = true;
  restored = await startService(config, async () => { calls++; });
  const next = new AsasClient(`http://127.0.0.1:${restored.port}`, token);
  assert.equal((await next.submit(task)).status, 'needs_recovery'); assert.equal(calls, 1);
  await assert.rejects(next.submit({ ...input(), sessionId: task.sessionId }), /409/);
});
