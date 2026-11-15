import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import { Application } from '../application';
import { AOEM_TOOL_CONTRACT, TOOL_CAPABILITY, ToolCapability, parseTool, type TaskInput } from '../protocol';
import { dynamicTools } from '../codex/engine';
import { claudeToolDefinitions } from '../claude';

test('official Codex and Claude declarations use identical versioned inputs and semantics', () => {
  assert.equal(AOEM_TOOL_CONTRACT.executionNode, 'project-owner');
  const codex = dynamicTools(), claude = claudeToolDefinitions({ toolContract: TOOL_CAPABILITY });
  assert.equal(codex.length, claude.length);
  for (const tool of claude) {
    const other = codex.find(value => value.name === 'acode_' + tool.name)!;
    assert.equal(other.description, tool.description);
    assert.deepEqual(other.inputSchema, z.toJSONSchema(z.object(tool.schema).strict(), { target: 'draft-7' }));
  }
  assert.equal(claudeToolDefinitions({}).some(tool => tool.name === 'command_run'), false);
  assert.throws(() => ToolCapability.parse({ version: 1, tools: ['file_read', 'file_read'] }));
  for (const args of [{ command: 'cargo', timeoutMs: 120001 }, { command: 'cargo', env: { TOKEN: 'not-allowed' } }, { command: 'bad\0' }])
    assert.throws(() => parseTool('command_run', args));
  assert.deepEqual(parseTool('command_run', { command: 'cargo', args: ['test'] }), { command: 'cargo', args: ['test'], path: '.', timeoutMs: 30000, maxBytes: 32768 });
});

test('A policy denies unadvertised commands and plan execution for both engines before routing', async t => {
  const directory = await mkdtemp(path.join(tmpdir(), 'asas-contract-')), projectId = 'a'.repeat(64);
  const peer = { id: 'B', tokenSha256: 'b'.repeat(64), projects: [projectId] }, results: string[] = [];
  const run = async (context: Parameters<import('../broker').Engine>[0]) => {
    const result = await context.call('command_run', { command: 'must-not-spawn' });
    assert.equal(result.ok, false); results.push(result.output);
  };
  const app = await Application.open(directory, [peer], { codex: { models: ['fixture'], run }, claude: { models: ['fixture'], run } });
  t.after(async () => { await app.close(); await rm(directory, { recursive: true, force: true }); });
  for (const engine of ['codex', 'claude'] as const) for (const advertised of [false, true]) {
    const input: TaskInput = { id: randomUUID(), sessionId: randomUUID(), projectId, engine, prompt: 'fixture', model: 'fixture', mode: advertised ? 'plan' : 'develop',
      ...(advertised ? { toolContract: TOOL_CAPABILITY } : {}) };
    await app.broker.submit(peer, input);
    let after = 0, status = 'running';
    while (status === 'running') {
      const page = await app.broker.events(peer, input.id, after); after += page.events.length; status = page.status;
      assert.ok(page.events.every(event => event.type !== 'tool'));
    }
    assert.equal(status, 'completed');
  }
  assert.equal(results.filter(value => value.includes('advertise')).length, 2);
  assert.equal(results.filter(value => value.includes('Plan mode')).length, 2);
  assert.deepEqual(app.health().toolContract, TOOL_CAPABILITY);
});
