import { z } from 'zod';
import path from 'node:path';
import { CodexProfile } from './codex/process';
import { claudeEngine } from './claude';
import { codexEngine } from './codex/engine';
import type { Engines } from './broker';

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
  if (config.codex) engines.codex = { models: config.codex.models, run: codexEngine(config.directory, config.codex) };
  if (config.claude) engines.claude = { models: config.claude.models, run: claudeEngine(config.directory, config.claude.executable) };
  if (!config.codex && !config.claude) engines.claude = { models: config.models, run: claudeEngine(config.directory, config.executable) };
  return engines;
}
