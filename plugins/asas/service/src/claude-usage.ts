import { TokenCounts } from './protocol';
import type { PlanSnapshot } from './quota';

type Usage = { input_tokens: number; output_tokens: number; cache_read_input_tokens?: number | null; cache_creation_input_tokens?: number | null };
const counts = (usage: Usage, output = usage.output_tokens): TokenCounts => TokenCounts.parse({ input: usage.input_tokens,
  output, cacheRead: usage.cache_read_input_tokens ?? 0, cacheWrite: usage.cache_creation_input_tokens ?? 0 });

export function modelTokenCounts(value: unknown): TokenCounts | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length > 64) return;
  const total: TokenCounts = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
  try {
    for (const model of Object.values(value)) {
      const row = model as Record<string, unknown>;
      const usage = TokenCounts.parse({ input: row.inputTokens, output: row.outputTokens, cacheRead: row.cacheReadInputTokens, cacheWrite: row.cacheCreationInputTokens });
      for (const key of Object.keys(total) as (keyof TokenCounts)[]) total[key] += usage[key];
    }
    return TokenCounts.parse(total);
  } catch { return; }
}

/** Deduplicate main-loop steps; difference session modelUsage for complete totals. */
export class ClaudeUsage {
  private seen = new Set<string>();
  private total: TokenCounts = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
  assistant(id: string, usage: Usage): TokenCounts | undefined {
    if (this.seen.has(id)) return;
    this.seen.add(id);
    const value = counts(usage, 0); // Streaming assistant output is a placeholder.
    for (const key of Object.keys(value) as (keyof TokenCounts)[]) this.total[key] += value[key];
    return TokenCounts.parse({ ...this.total });
  }
  result(usage: Usage): { counts: TokenCounts; final: boolean } {
    const value = counts(usage);
    const final = (Object.keys(value) as (keyof TokenCounts)[]).every(key => value[key] >= this.total[key]);
    return { counts: final ? value : { ...this.total }, final };
  }
  complete(usage: Usage, models: unknown, baseline: TokenCounts | undefined): { counts: TokenCounts; final: boolean; cumulative?: TokenCounts } {
    const main = this.result(usage), cumulative = modelTokenCounts(models);
    if (!baseline || !cumulative || !main.final) return { ...main, final: false };
    const delta = { ...cumulative };
    for (const key of Object.keys(delta) as (keyof TokenCounts)[]) {
      delta[key] -= baseline[key];
      if (delta[key] < main.counts[key]) return { ...main, final: false };
    }
    return { counts: TokenCounts.parse(delta), final: true, cumulative };
  }
}

type UsageSource = { usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET?:
  (options: { skipBehaviors: boolean }) => Promise<{ rate_limits_available: boolean; rate_limits: unknown; session?: { model_usage: unknown } }> };
export async function readClaudeSnapshot(source: UsageSource, timeoutMs = 1500): Promise<{ plan: PlanSnapshot; baseline?: TokenCounts }> {
  const unknown: PlanSnapshot = { source: 'claude-sdk/get_usage', observedAt: new Date().toISOString(), available: false, windows: [] };
  let timer: NodeJS.Timeout | undefined;
  try {
    const method = source.usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET;
    if (!method) return { plan: unknown };
    const result = await Promise.race([method.call(source, { skipBehaviors: true }), new Promise<never>((_, reject) => { timer = setTimeout(() => reject(Error('Usage unavailable')), timeoutMs); })]);
    const baseline = modelTokenCounts(result.session?.model_usage);
    if (!result.rate_limits_available || !result.rate_limits || typeof result.rate_limits !== 'object') return { plan: unknown, baseline };
    const windows: PlanSnapshot['windows'] = [];
    for (const [name, value] of Object.entries(result.rate_limits)) {
      if (!['five_hour', 'seven_day', 'seven_day_oauth_apps', 'seven_day_opus', 'seven_day_sonnet'].includes(name) || !value || typeof value !== 'object') continue;
      const row = value as { utilization?: unknown; resets_at?: unknown };
      windows.push({ name, usedPercent: typeof row.utilization === 'number' && Number.isFinite(row.utilization) && row.utilization >= 0 && row.utilization <= 100 ? row.utilization : null,
        resetsAt: typeof row.resets_at === 'string' && Number.isFinite(Date.parse(row.resets_at)) ? row.resets_at : null });
    }
    return { plan: { ...unknown, observedAt: new Date().toISOString(), available: true, windows }, baseline };
  } catch { return { plan: unknown }; }
  finally { clearTimeout(timer); }
}
export async function readClaudePlan(source: UsageSource, timeoutMs = 1500): Promise<PlanSnapshot> {
  return (await readClaudeSnapshot(source, timeoutMs)).plan;
}
