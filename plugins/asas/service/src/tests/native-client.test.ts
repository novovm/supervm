import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { mkdtemp, rm } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { Application } from '../application';
import { NativeClient } from '../native-client';
import { serveStdio } from '../stdio';

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
