import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, readFile, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { QuotaStore } from '../quota';
import { quotaAdminRequest } from '../quota-admin';
import { Application } from '../application';
import { ClaudeUsage, readClaudePlan, modelTokenCounts } from '../claude-usage';
import { OwnUsage, type TaskInput } from '../protocol';

const node = (letter: string) => 'novovm-mldsa65:' + letter.repeat(64);
const b = node('b'), c = node('c'), projectId = 'a'.repeat(64);
const counts = { input: 20, output: 5, cacheRead: 30, cacheWrite: 10 };
const owner = { peerId: 'shared', nodeId: b };
const task = (): TaskInput => ({ id: randomUUID(), sessionId: randomUUID(), projectId, engine: 'claude', prompt: 'Usage protocol fixture, not inference', model: 'fixture', mode: 'develop', workerNodeId: b });

test('three allocations are independent; reset retains history and uses a cycle precondition', async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-quota-')), meter = new QuotaStore(root);
  t.after(() => rm(root, { recursive: true, force: true })); await meter.load();
  for (const nodeId of [b, c, node('d')]) await meter.command({ operation: 'set', peerId: 'shared', nodeId, limitTokens: 100 });
  const id = randomUUID(); await meter.admit('shared', b, id);
  await assert.rejects(meter.admit('shared', b, randomUUID()), /429/);
  await meter.observe(id, counts, true); await meter.observe(id, counts, true);
  assert.equal(meter.view('shared', b).usedTokens, 65); assert.equal(meter.view('shared', b).remainingTokens, 35);
  assert.equal(meter.view('shared', c).usedTokens, 0);
  const old = meter.view('shared', b).cycleId!;
  await assert.rejects(meter.command({ operation: 'reset', ...owner, expectedCycleId: old }), /active/);
  await meter.finish(id); await meter.command({ operation: 'reset', ...owner, expectedCycleId: old });
  assert.equal(meter.view('shared', b).remainingTokens, 100);
  await assert.rejects(meter.command({ operation: 'reset', ...owner, expectedCycleId: old }), /changed/);
  const report = await meter.command({ operation: 'report' }) as any;
  assert.equal(report.recentTasks[0].counts.output, 5); assert.equal(report.clients.length, 3); assert.equal(report.history.length, 4);
  const reopened = new QuotaStore(root); await reopened.load();
  assert.deepEqual(reopened.view('shared', b), meter.view('shared', b));
});

test('unsettled usage never becomes zero or sufficient; limits block admission, not historical recovery', async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-quota-')), meter = new QuotaStore(root);
  t.after(() => rm(root, { recursive: true, force: true })); await meter.load();
  await meter.command({ operation: 'set', ...owner, limitTokens: 80 });
  const id = randomUUID(); await meter.admit('shared', b, id); await meter.observe(id, { ...counts, output: 0 }, false);
  // An A process dies before its final result; the new owner cannot assume no further output.
  const reopened = new QuotaStore(root); await reopened.load();
  const view = reopened.view('shared', b);
  assert.equal(view.status, 'unknown'); assert.equal(view.usedTokens, 60); assert.equal(view.remainingTokens, null); assert.equal(view.activeTasks, 0);
  await assert.rejects(reopened.admit('shared', b, randomUUID()), /429/);
  await reopened.command({ operation: 'reset', ...owner, expectedCycleId: view.cycleId });
  const next = randomUUID(); await reopened.admit('shared', b, next); await reopened.observe(next, { ...counts, output: 25 }, true); await reopened.finish(next);
  assert.equal(reopened.view('shared', b).status, 'exhausted');
  await assert.rejects(reopened.admit('shared', b, randomUUID()), /429/);
  await reopened.command({ operation: 'set', ...owner, limitTokens: 200 });
  assert.equal(reopened.view('shared', b).usedTokens, 85); assert.equal(reopened.view('shared', b).remainingTokens, 115);
});

test('A local operator can allocate live; B cannot see account/other clients, reset, or spend another node allocation', async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'asas-quota-app-'));
  let calls = 0;
  const app = await Application.open(root, [{ id: 'shared', tokenSha256: 'b'.repeat(64), projects: [projectId] }], {
    claude: { models: ['fixture'], run: async context => { calls++; await context.usage!(counts, true); await context.text('fixture'); } },
  });
  t.after(async () => { await app.close(); await rm(root, { recursive: true, force: true }); });
  await quotaAdminRequest(root, { operation: 'set', ...owner, limitTokens: 65 });
  await quotaAdminRequest(root, { operation: 'set', peerId: 'shared', nodeId: c, limitTokens: 999999 });
  await app.broker.meter.plan({ source: 'claude-sdk/get_usage', observedAt: new Date().toISOString(), available: true,
    windows: [{ name: 'five_hour', usedPercent: 82, resetsAt: null }] });
  const input = task(); await app.dispatch('shared', 'submit', input, b);
  let after = 0, status = 'running';
  while (status === 'running') { const page = await app.dispatch('shared', 'events', { taskId: input.id, after }, b) as any; status = page.status; after += page.events.length; }
  const cap = await app.dispatch('shared', 'capabilities', {}, b) as any;
  assert.equal(cap.ownUsage.claude.usedTokens, 65); assert.equal(cap.ownUsage.claude.limitTokens, 65);
  assert.equal(cap.ownUsage.claude.remainingTokens, 0); OwnUsage.parse(cap.ownUsage);
  assert.ok(!JSON.stringify(cap).includes(c)); assert.ok(!JSON.stringify(cap).includes('999999')); assert.ok(!('plan' in cap));
  assert.ok(!('plan' in app.health()));
  const ownPage = await app.dispatch('shared', 'events', { taskId: input.id, after }, b) as any;
  assert.deepEqual(ownPage.ownUsage, cap.ownUsage);
  assert.equal((await app.dispatch('shared', 'capabilities', {}, c) as any).ownUsage.claude.usedTokens, 0);
  await app.dispatch('shared', 'submit', input, b); assert.equal(calls, 1);
  await assert.rejects(app.dispatch('shared', 'submit', task(), b), /429/);
  await assert.rejects(app.dispatch('shared', 'submit', task(), c), /403/);
  await assert.rejects(app.dispatch('shared', 'quota/reset', { taskId: input.id }, b), /unknown/);
  const report = await quotaAdminRequest(root, { operation: 'report' }) as any;
  assert.equal(report.plan.windows[0].remainingPercent, 18); assert.equal(report.clients.length, 2); assert.equal(report.charging, false);
  assert.equal(JSON.parse(await readFile(path.join(root, 'quota-admin.json'), 'utf8')).version, 1);
  if (process.platform !== 'win32') assert.equal((await stat(path.join(root, 'quota-admin.sock'))).mode & 0o777, 0o600);
});

test('streamed parallel blocks count once, final result output replaces placeholders, resumed cumulative modelUsage is unused', () => {
  const meter = new ClaudeUsage();
  const usage = { input_tokens: 20, output_tokens: 1, cache_read_input_tokens: 30, cache_creation_input_tokens: 10 };
  assert.deepEqual(meter.assistant('message-1', usage), { ...counts, output: 0 });
  assert.equal(meter.assistant('message-1', usage), undefined);
  assert.deepEqual(meter.result({ ...usage, output_tokens: 5 }), { counts, final: true });
  assert.equal(meter.result({ input_tokens: 0, output_tokens: 0 }).final, false);
  const nextTurn = new ClaudeUsage();
  assert.deepEqual(nextTurn.result({ input_tokens: 3, output_tokens: 2 }).counts, { input: 3, output: 2, cacheRead: 0, cacheWrite: 0 });
});

test('official plan reads skip local transcript scans; missing, invalid and timed-out data remain unknown', async () => {
  const plan = await readClaudePlan({ usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET: async options => {
    assert.deepEqual(options, { skipBehaviors: true });
    return { rate_limits_available: true, rate_limits: { five_hour: { utilization: 82, resets_at: '2026-11-14T12:00:00Z' }, seven_day: { utilization: null, resets_at: null }, extra_usage: { used_credits: 123 } } };
  } });
  assert.equal(plan.windows[0].usedPercent, 82); assert.equal(plan.windows[1].usedPercent, null); assert.equal(plan.windows.length, 2);
  assert.equal((await readClaudePlan({})).available, false);
  assert.equal((await readClaudePlan({ usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET: async () => { throw Error('not supported'); } })).available, false);
  assert.equal((await readClaudePlan({ usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET: async () => new Promise(() => {}) }, 5)).available, false);
});

test('whole-pipeline totals include compaction, subtract prior session usage, and never count thinking/cache subfields twice', () => {
  const meter = new ClaudeUsage(), usage = { input_tokens: 20, output_tokens: 5, cache_read_input_tokens: 30, cache_creation_input_tokens: 10 };
  meter.assistant('current-message', usage);
  const previous = { input: 500, output: 100, cacheRead: 900, cacheWrite: 200 };
  const models = { main: { inputTokens: 520, outputTokens: 105, cacheReadInputTokens: 930, cacheCreationInputTokens: 210, thinkingTokens: 4, cacheCreationEphemeral5mInputTokens: 10 },
    compaction: { inputTokens: 8, outputTokens: 3, cacheReadInputTokens: 4, cacheCreationInputTokens: 2 } };
  const result = meter.complete(usage, models, previous);
  assert.deepEqual(result.counts, { input: 28, output: 8, cacheRead: 34, cacheWrite: 12 }); assert.equal(result.final, true);
  assert.equal(meter.complete(usage, models, undefined).final, false, 'legacy resume without a baseline is not silently charged the entire prior session');
  assert.equal(meter.complete(usage, {}, previous).final, false, 'a counter reset is not treated as negative spend or new credit');
  assert.equal(modelTokenCounts({ malformed: { inputTokens: -1 } }), undefined);
});
