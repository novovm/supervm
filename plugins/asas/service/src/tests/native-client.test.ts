import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { mkdtemp, rm } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { Application } from '../application';
import { NativeClient, NetworkClient } from '../native-client';
import { serveStdio } from '../stdio';
import { OwnUsage } from '../protocol';

test('B allocation schema rejects account totals and inconsistent remaining values', () => {
  const quota = { engine: 'claude', unit: 'tokens', cycleId: randomUUID(), limitTokens: 100, usedTokens: 35,
    remainingTokens: 65, activeTasks: 0, unsettledTasks: 0, status: 'available', updatedAt: null };
  OwnUsage.parse({ claude: quota });
  assert.throws(() => OwnUsage.parse({ claude: quota, account: { used: 900 } }));
  assert.throws(() => OwnUsage.parse({ claude: { ...quota, remainingTokens: 100 } }));
  assert.throws(() => OwnUsage.parse({ claude: { ...quota, remainingTokens: null } }));
  assert.throws(() => OwnUsage.parse({ claude: { ...quota, otherClients: [] } }));
});

test('B network frames contain no caller-supplied principal or A-worker commands', async () => {
  const input = new PassThrough(), output = new PassThrough();
  const client = new NetworkClient(input, output), frames: any[] = [];
  output.on('data', bytes => {
    const frame = JSON.parse(bytes.toString()); frames.push(frame);
    input.write(JSON.stringify({ id: frame.id, result: { version: 2, peerId: 'granted-B', projects: ['a'.repeat(64)], remoteRouteReady: true,
      serverNodeId: 'novovm-mldsa65:' + 'b'.repeat(64), engines: { codex: { models: ['fixture'] } } } }) + '\n');
  });
  try {
    assert.equal((await client.health()).peerId, 'granted-B');
    assert.deepEqual(frames, [{ id: 0, operation: 'capabilities' }]);
    assert.ok(!JSON.stringify(frames).includes('peerId'));
  } finally { client.close(); input.end(); output.end(); }
});
test('B network health rejects a private-pipe health response', async () => {
  const input = new PassThrough(), output = new PassThrough(), client = new NetworkClient(input, output);
  output.on('data', bytes => input.write(JSON.stringify({ id: JSON.parse(bytes.toString()).id, result: { version: 2, remoteRouteReady: false } }) + '\n'));
  try { await assert.rejects(client.health(), /authenticated/); }
  finally { client.close(); input.end(); output.end(); }
});

test('native private pipe enforces peer, returns UTF-8 tools and never replays terminal calls', { timeout: 10000 }, async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-native-')), projectId = 'a'.repeat(64);
  const input = new PassThrough(), output = new PassThrough(); let calls = 0;
  const app = await Application.open(root, [{ id: 'B', tokenSha256: 'b'.repeat(64), projects: [projectId] }],
    { codex: { models: ['fixture'], run: async context => { calls++; const result = await context.call('file_read', { path: 'sample.txt' }); await context.text(result.output); } } });
  const running = serveStdio(app, input, output); void running.catch(() => undefined);
  const client = new NativeClient(output, input, 'B');
  const task = { id: randomUUID(), sessionId: randomUUID(), projectId, engine: 'codex' as const, model: 'fixture', prompt: 'Fixture only', mode: 'develop' as const };
  try {
    assert.equal((await client.health()).remoteRouteReady, false);
    await assert.rejects(client.submit({ ...task, projectId: 'c'.repeat(64) }), /rejected/);
    let executions = 0, text = '';
    assert.equal(await client.follow(task, path.join(root, 'receipt.json'), async event => { executions++; return { callId: event.callId!, ok: true, output: '中文结果 😀' }; },
      event => { if (event.type === 'text') text += event.text; }, new AbortController().signal), 'completed');
    assert.equal(text, '中文结果 😀'); assert.equal(executions, 1);
    assert.equal(await client.follow(task, path.join(root, 'other-receipt.json'), async () => { throw Error('must not replay'); }, () => {}, new AbortController().signal), 'completed');
    assert.equal(calls, 1);
    const next = { ...task, id: randomUUID(), prompt: 'Second fixture turn in the same session' };
    assert.equal(await client.follow(next, path.join(root, 'next.json'), async event => ({ callId: event.callId!, ok: true, output: 'second' }), () => {}, new AbortController().signal), 'completed');
    assert.equal(calls, 2);
  } finally { client.close(); input.end(); await running; await rm(root, { recursive: true, force: true }); }
});
