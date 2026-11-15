import { readFile } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import path from 'node:path';
import { z } from 'zod';
import { saveJson } from './storage';
import { NodeId, OwnQuota, TokenCounts, id } from './protocol';

const count = z.number().int().nonnegative().safe();
const Identity = z.object({ peerId: z.string().min(1).max(200), nodeId: NodeId.nullable() }).strict();
type Identity = z.infer<typeof Identity>;
const Cycle = Identity.extend({ id, limit: count.nullable(), createdAt: z.string().datetime(), updatedAt: z.string().datetime(), lastSeenAt: z.string().datetime().nullable().default(null) }).strict();
const Entry = Identity.extend({ taskId: id, cycleId: id, counts: TokenCounts, final: z.boolean(), active: z.boolean(), updatedAt: z.string().datetime() }).strict();
const Window = z.object({ name: z.string().min(1).max(200), usedPercent: z.number().min(0).max(100).nullable(), resetsAt: z.string().nullable() }).strict();
export const PlanSnapshot = z.object({ source: z.literal('claude-sdk/get_usage'), observedAt: z.string().datetime(),
  available: z.boolean(), windows: z.array(Window).max(32) }).strict();
export type PlanSnapshot = z.infer<typeof PlanSnapshot>;
const State = z.object({ version: z.literal(1), cycles: z.array(Cycle), entries: z.array(Entry), plan: PlanSnapshot.nullable() }).strict();
export const QuotaCommand = z.discriminatedUnion('operation', [
  z.object({ operation: z.literal('report') }).strict(),
  Identity.extend({ operation: z.literal('set'), limitTokens: count.nullable() }).strict(),
  Identity.extend({ operation: z.literal('reset'), expectedCycleId: id }).strict(),
]);
const zero = (): TokenCounts => ({ input: 0, output: 0, cacheRead: 0, cacheWrite: 0 });
export const tokenTotal = (counts: TokenCounts) => {
  const value = counts.input + counts.output + counts.cacheRead + counts.cacheWrite;
  if (!Number.isSafeInteger(value)) throw Error('Token count overflow'); return value;
};
const same = (a: Identity, b: Identity) => a.peerId === b.peerId && a.nodeId === b.nodeId;

/** Local free-service budgets; not a wallet, vendor quota reset or billing ledger. */
export class QuotaStore {
  private state: z.infer<typeof State> = { version: 1, cycles: [], entries: [], plan: null };
  private queue: Promise<unknown> = Promise.resolve();
  private failed = false;
  constructor(private directory: string) {}
  private get file() { return path.join(this.directory, 'usage-v1.json'); }
  async load() {
    try { this.state = State.parse(JSON.parse(await readFile(this.file, 'utf8'))); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    if (new Set(this.state.cycles.map(cycle => cycle.id)).size !== this.state.cycles.length
      || new Set(this.state.entries.map(entry => entry.taskId)).size !== this.state.entries.length
      || this.state.entries.some(entry => !this.state.cycles.some(cycle => cycle.id === entry.cycleId && same(cycle, entry)))) throw Error('Invalid quota ownership records');
    // Lost producers are not assumed to have consumed zero tokens.
    for (const entry of this.state.entries) entry.active = false;
    await saveJson(this.file, this.state);
  }
  private mutate<T>(change: (state: z.infer<typeof State>) => T): Promise<T> {
    const work = this.queue.then(async () => {
      if (this.failed) throw Error('503 usage storage requires operator recovery');
      const next = structuredClone(this.state), result = change(next);
      try { await saveJson(this.file, next); } catch (error) { this.failed = true; throw error; }
      this.state = next; return result;
    });
    this.queue = work.catch(() => undefined); return work;
  }
  private current(owner: Identity, state = this.state) {
    for (let index = state.cycles.length - 1; index >= 0; index--) if (same(state.cycles[index], owner)) return state.cycles[index];
    return undefined;
  }
  async touch(peerId: string, nodeId?: string) {
    await this.mutate(state => {
      const owner = { peerId, nodeId: nodeId ?? null }, cycle = this.current(owner, state), now = new Date().toISOString();
      if (cycle) cycle.lastSeenAt = now;
      else state.cycles.push({ ...owner, id: randomUUID(), limit: null, createdAt: now, updatedAt: now, lastSeenAt: now });
    });
  }
  view(peerId: string, nodeId?: string): OwnQuota {
    const owner = { peerId, nodeId: nodeId ?? null }, cycle = this.current(owner);
    const entries = this.state.entries.filter(entry => entry.cycleId === cycle?.id);
    const usedTokens = entries.reduce((sum, entry) => sum + tokenTotal(entry.counts), 0);
    if (!Number.isSafeInteger(usedTokens)) throw Error('Quota aggregate overflow');
    const activeTasks = entries.filter(entry => entry.active).length, unsettledTasks = entries.filter(entry => !entry.final && !entry.active).length;
    const limitTokens = cycle?.limit ?? null;
    return OwnQuota.parse({ engine: 'claude', unit: 'tokens', cycleId: cycle?.id ?? null, limitTokens, usedTokens,
      remainingTokens: limitTokens === null ? null : unsettledTasks ? null : Math.max(0, limitTokens - usedTokens),
      activeTasks, unsettledTasks, status: unsettledTasks ? 'unknown' : limitTokens === null ? 'unlimited' : usedTokens >= limitTokens ? 'exhausted' : 'available',
      updatedAt: [...entries.map(entry => entry.updatedAt), ...(cycle ? [cycle.updatedAt] : [])].sort().at(-1) ?? null });
  }
  async admit(peerId: string, nodeId: string | undefined, taskId: string) {
    await this.mutate(state => {
      if (state.entries.some(entry => entry.taskId === taskId)) throw Error('409 usage task already admitted; no replay');
      const view = this.view(peerId, nodeId);
      if (view.limitTokens !== null && (view.status === 'unknown' || view.status === 'exhausted' || view.activeTasks))
        throw Error('429 Claude allocation exhausted, unsettled or busy');
      const owner = { peerId, nodeId: nodeId ?? null }, now = new Date().toISOString();
      let cycle = this.current(owner, state);
      if (!cycle) { cycle = { ...owner, id: randomUUID(), limit: null, createdAt: now, updatedAt: now, lastSeenAt: null }; state.cycles.push(cycle); }
      state.entries.push({ ...owner, taskId, cycleId: cycle.id, counts: zero(), final: false, active: true, updatedAt: now });
    });
  }
  async observe(taskId: string, counts: TokenCounts, final: boolean) {
    TokenCounts.parse(counts); tokenTotal(counts);
    return this.mutate(state => {
      const entry = state.entries.find(entry => entry.taskId === taskId);
      if (!entry || !entry.active) throw Error('409 usage producer not active');
      if (entry.final) {
        if (!final || JSON.stringify(entry.counts) !== JSON.stringify(counts)) throw Error('409 conflicting final usage');
        return;
      }
      if (Object.keys(counts).some(key => counts[key as keyof TokenCounts] < entry.counts[key as keyof TokenCounts])) throw Error('409 non-monotonic task usage');
      entry.counts = counts; entry.final = final; entry.updatedAt = new Date().toISOString();
    });
  }
  async finish(taskId: string) {
    await this.mutate(state => { const entry = state.entries.find(entry => entry.taskId === taskId); if (entry) { entry.active = false; entry.updatedAt = new Date().toISOString(); } });
  }
  async plan(snapshot: PlanSnapshot) { await this.mutate(state => { state.plan = PlanSnapshot.parse(snapshot); }); }
  async command(value: unknown) {
    const input = QuotaCommand.parse(value);
    if (input.operation === 'report') {
      await this.queue;
      const owners = new Map(this.state.cycles.map(cycle => [createHash('sha256').update(JSON.stringify([cycle.peerId, cycle.nodeId])).digest('hex'), cycle]));
      const plan = this.state.plan;
      const observedTokens = this.state.entries.reduce((sum, entry) => sum + tokenTotal(entry.counts), 0);
      if (!Number.isSafeInteger(observedTokens)) throw Error('Quota aggregate overflow');
      return { version: 1, charging: false, observedTokens, unsettledTasks: this.state.entries.filter(entry => !entry.final && !entry.active).length,
        plan: plan ? { ...plan, stale: Date.now() - Date.parse(plan.observedAt) > 300000,
        windows: plan.windows.map(window => ({ ...window, remainingPercent: window.usedPercent === null ? null : 100 - window.usedPercent,
          resetPending: window.resetsAt !== null && Date.parse(window.resetsAt) <= Date.now() })) } : null,
        clients: [...owners.values()].map(owner => ({ peerId: owner.peerId, nodeId: owner.nodeId, lastSeenAt: owner.lastSeenAt,
          lifetimeUsedTokens: this.state.entries.filter(entry => same(entry, owner)).reduce((sum, entry) => sum + tokenTotal(entry.counts), 0),
          ...this.view(owner.peerId, owner.nodeId ?? undefined) })),
        history: structuredClone(this.state.cycles.slice(-100)), recentTasks: structuredClone(this.state.entries.slice(-100)) };
    }
    await this.mutate(state => {
      const owner = { peerId: input.peerId, nodeId: input.nodeId }, cycle = this.current(owner, state), now = new Date().toISOString();
      if (input.operation === 'set') {
        if (cycle) { cycle.limit = input.limitTokens; cycle.updatedAt = now; }
        else state.cycles.push({ ...owner, id: randomUUID(), limit: input.limitTokens, createdAt: now, updatedAt: now, lastSeenAt: null });
      } else {
        if (!cycle || cycle.id !== input.expectedCycleId) throw Error('409 quota cycle changed; reload before reset');
        if (state.entries.some(entry => entry.cycleId === cycle.id && entry.active)) throw Error('409 cannot reset an active allocation');
        state.cycles.push({ ...owner, id: randomUUID(), limit: cycle.limit, createdAt: now, updatedAt: now, lastSeenAt: cycle.lastSeenAt });
      }
    });
    return this.view(input.peerId, input.nodeId ?? undefined);
  }
  async close() { await this.queue; }
}
