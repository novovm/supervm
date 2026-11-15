import { z } from 'zod';

export const VERSION = 2;
export const MAX_BYTES = 1024 * 1024;
export const id = z.string().uuid();
export const EngineName = z.enum(['claude', 'codex']);
export type EngineName = z.infer<typeof EngineName>;
export const ToolName = z.enum(['workspace_list', 'file_read', 'file_patch', 'text_search', 'ide_execute']);
export const TaskInput = z.object({
  id, sessionId: id, projectId: z.string().regex(/^[a-f0-9]{64}$/), engine: EngineName,
  prompt: z.string().min(1).max(250000), model: z.string().min(1).max(200),
  mode: z.enum(['develop', 'plan']), context: z.string().max(250000).optional(),
}).strict();
export type TaskInput = z.infer<typeof TaskInput>;
export const ToolResult = z.object({ callId: id, ok: z.boolean(), output: z.string().max(240000) }).strict();
export type ToolResult = z.infer<typeof ToolResult>;
export const Event = z.object({ sequence: z.number().int().positive(), taskId: id,
  type: z.enum(['started', 'text', 'tool', 'tool_result', 'completed', 'failed', 'cancelled', 'needs_recovery']),
  text: z.string().max(240000).optional(), callId: id.optional(), tool: ToolName.optional(), arguments: z.record(z.string(), z.unknown()).optional(),
}).strict();
export type AgentEvent = z.infer<typeof Event>;
export type TaskStatus = 'running' | 'completed' | 'failed' | 'cancelled' | 'needs_recovery';
export type TaskState = TaskInput & { peerId: string; status: TaskStatus; events: AgentEvent[] };
export const StoredTask = TaskInput.extend({ peerId: z.string().min(1).max(200),
  status: z.enum(['running', 'completed', 'failed', 'cancelled', 'needs_recovery']), events: z.array(Event).max(2048) }).strict();
export const ToolShapes = {
  workspace_list: { path: z.string().max(2000).default('.') },
  file_read: { path: z.string().min(1).max(2000), offset: z.number().int().min(0).max(2000000).default(0), limit: z.number().int().min(1).max(100000).default(20000) },
  file_patch: { path: z.string().min(1).max(2000), expectedSha256: z.string().regex(/^[a-f0-9]{64}$/), before: z.string().max(100000), after: z.string().max(100000) },
  text_search: { text: z.string().min(1).max(1000), path: z.string().max(2000).default('.') },
  ide_execute: { request: z.record(z.string(), z.unknown()) },
};
export function parseTool(name: string, args: unknown): Record<string, unknown> {
  const key = ToolName.parse(name);
  return z.object(ToolShapes[key]).strict().parse(args);
}
export function endpoint(value: string): URL {
  const url = new URL(value);
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (url.username || url.password || url.hash || url.search || (url.protocol !== 'https:' && !(url.protocol === 'http:' && local)))
    throw Error('ASAS requires HTTPS; HTTP is allowed only on loopback');
  url.pathname = url.pathname.replace(/\/$/, '') + '/';
  return url;
}
