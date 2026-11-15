import { z } from 'zod';
import path from 'node:path';
import { CodexProfile } from './codex/process';
import { claudeEngine } from './claude';
import { codexEngine } from './codex/engine';
import type { Engines } from './broker';
import { cachedControls, discoverClaudeControls, readCodexControls } from './controls';

const models = z.array(z.string().min(1).max(200)).min(1).max(100);
export const Configuration = z.object({ directory: z.string().refine(path.isAbsolute),
  host: z.string().default('127.0.0.1'), port: z.number().int().min(0).max(65535).default(7843),
  models: models.default(['default']), executable: z.string().optional(),
  peers: z.array(z.object({ id: z.string().min(1).max(200), tokenSha256: z.string().regex(/^[a-f0-9]{64}$/),
    projects: z.array(z.string().regex(/^[a-f0-9]{64}$/)).min(1).max(100) }).strict()).min(1).max(100),
  codex: CodexProfile.optional(), claude: z.object({ models, executable: z.string().optional() }).strict().optional(),
  tlsFiles: z.object({ cert: z.string(), key: z.string() }).strict().optional(),
}).strict();
export function enginesFor(config: z.infer<typeof Configuration>): Engines {
  const engines: Engines = {};
  if (config.codex) {
    const profile = config.codex;
    engines.codex = { models: profile.models, run: codexEngine(config.directory, profile),
      controls: cachedControls(() => readCodexControls(profile.catalog, profile.models, profile.effort)) };
  }
  const claude = config.claude || (!config.codex ? { models: config.models, executable: config.executable } : undefined);
  if (claude) engines.claude = { models: claude.models, run: claudeEngine(config.directory, claude.executable),
    controls: cachedControls(() => discoverClaudeControls(config.directory, claude.models, claude.executable)) };
  return engines;
}
