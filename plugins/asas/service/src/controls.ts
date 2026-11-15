import { readFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import type { Query, Options } from '@anthropic-ai/claude-agent-sdk' with { 'resolution-mode': 'import' };
import { EngineControls, ReasoningEffort, type ModelControls } from './protocol';

export function cachedControls(load: () => Promise<EngineControls>): () => Promise<EngineControls> {
  let cached: EngineControls | undefined, expires = 0, pending: Promise<EngineControls> | undefined;
  return () => {
    if (cached && Date.now() < expires) return Promise.resolve(cached);
    return pending ??= load().then(value => {
      cached = EngineControls.parse(value); expires = Date.now() + 60000; return cached;
    }).catch(() => {
      cached = { version: 1, models: {} }; expires = Date.now() + 15000; return cached;
    }).finally(() => { pending = undefined; });
  };
}

export const isolatedClaudeOptions = (cwd: string, executable?: string): Options => ({ cwd,
  ...(executable ? { pathToClaudeCodeExecutable: executable } : {}),
  tools: [], settingSources: [], settings: { disableAllHooks: true },
});

export async function readClaudeControls(stream: Pick<Query, 'supportedModels' | 'setModel' | 'setPermissionMode'>,
  models: string[]): Promise<EngineControls> {
  const native = await stream.supportedModels(), result: EngineControls = { version: 1, models: {} };
  for (const model of models) {
    const row = native.find(row => row.value === model || row.resolvedModel === model);
    if (!row) continue;
    const efforts = row.supportsEffort === true ? [...new Set(row.supportedEffortLevels || [])] : [];
    const controls: ModelControls = { efforts, permissionModes: ['default'] };
    if (row.supportsAutoMode === true) {
      try {
        await stream.setModel(model); await stream.setPermissionMode('auto');
        controls.permissionModes.push('auto');
      } catch { /* Account/model restrictions take precedence over catalog hints. */ }
      finally { await stream.setPermissionMode('default'); }
    }
    result.models[model] = controls;
  }
  return EngineControls.parse(result);
}

export async function discoverClaudeControls(directory: string, models: string[], executable?: string): Promise<EngineControls> {
  const { query } = await import('@anthropic-ai/claude-agent-sdk');
  const cwd = path.join(directory, 'isolated', 'capabilities'); await mkdir(cwd, { recursive: true, mode: 0o700 });
  const controller = new AbortController();
  // Initialize the official CLI without ever supplying a prompt or requesting inference.
  async function* emptyInput() {
    if (!controller.signal.aborted) await new Promise<void>(resolve => controller.signal.addEventListener('abort', () => resolve(), { once: true }));
  }
  const stream = query({ prompt: emptyInput(), options: { ...isolatedClaudeOptions(cwd, executable),
    persistSession: false, abortController: controller, permissionMode: 'default' } });
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([readClaudeControls(stream, models), new Promise<never>((_, reject) => {
      timer = setTimeout(() => { controller.abort(); stream.close(); reject(Error('Claude control discovery timed out')); }, 8000);
    })]);
  } finally { clearTimeout(timer); controller.abort(); stream.close(); }
}

export async function readCodexControls(catalog: string, models: string[], configuredEffort?: ModelControls['defaultEffort']): Promise<EngineControls> {
  const bytes = await readFile(catalog); if (bytes.length > 8 * 1024 * 1024) throw Error('Model catalog exceeds limit');
  const rows = JSON.parse(bytes.toString('utf8')).models;
  if (!Array.isArray(rows)) throw Error('Invalid native model catalog');
  const result: EngineControls = { version: 1, models: {} };
  for (const model of models) {
    const row = rows.find(row => row.slug === model); if (!row) continue;
    const efforts = [...new Set((Array.isArray(row.supported_reasoning_levels) ? row.supported_reasoning_levels : [])
      .map((level: any) => ReasoningEffort.safeParse(level.effort)).filter((parsed: any) => parsed.success).map((parsed: any) => parsed.data))] as ModelControls['efforts'];
    const fallback = configuredEffort ?? row.default_reasoning_level;
    result.models[model] = { efforts, permissionModes: ['default'],
      ...(efforts.includes(fallback) ? { defaultEffort: fallback } : {}) };
  }
  return EngineControls.parse(result);
}
