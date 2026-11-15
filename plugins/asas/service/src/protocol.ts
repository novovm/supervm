import { z } from 'zod';

export const VERSION = 2;
export const MAX_BYTES = 1024 * 1024;
export const id = z.string().uuid();
export const EngineName = z.enum(['claude', 'codex']);
export type EngineName = z.infer<typeof EngineName>;
export const NodeId = z.string().regex(/^novovm-mldsa65:[a-f0-9]{64}$/);
export const TokenCounts = z.object({ input: z.number().int().nonnegative().safe(), output: z.number().int().nonnegative().safe(),
  cacheRead: z.number().int().nonnegative().safe(), cacheWrite: z.number().int().nonnegative().safe() }).strict();
export type TokenCounts = z.infer<typeof TokenCounts>;
// This is B's allocation, never the A account's plan or another B's usage.
export const OwnQuota = z.object({ engine: z.literal('claude'), unit: z.literal('tokens'), cycleId: z.string().uuid().nullable(),
  limitTokens: z.number().int().nonnegative().safe().nullable(), usedTokens: z.number().int().nonnegative().safe(),
  remainingTokens: z.number().int().nonnegative().safe().nullable(), activeTasks: z.number().int().nonnegative(),
  unsettledTasks: z.number().int().nonnegative(), status: z.enum(['unlimited', 'available', 'exhausted', 'unknown']),
  updatedAt: z.string().datetime().nullable() }).strict().refine(value => {
    const status = value.unsettledTasks ? 'unknown' : value.limitTokens === null ? 'unlimited' : value.usedTokens >= value.limitTokens ? 'exhausted' : 'available';
    const remaining = value.limitTokens === null || value.unsettledTasks ? null : Math.max(0, value.limitTokens - value.usedTokens);
    return value.status === status && value.remainingTokens === remaining
      && (value.cycleId !== null || value.limitTokens === null && value.usedTokens === 0 && value.activeTasks === 0 && value.unsettledTasks === 0);
  }, 'Inconsistent allocation snapshot');
export type OwnQuota = z.infer<typeof OwnQuota>;
export const OwnUsage = z.object({ claude: OwnQuota.optional() }).strict();
export type OwnUsage = z.infer<typeof OwnUsage>;
const sha256 = z.string().regex(/^[a-f0-9]{64}$/);
export const SkillOwner = z.object({ nodeId: NodeId, projectId: sha256 }).strict();
export type SkillOwner = z.infer<typeof SkillOwner>;
export const SkillEntry = z.object({ skillId: sha256, sha256, name: z.string().min(1).max(64), description: z.string().min(1).max(1024), projectPath: z.string().min(1).max(2000) }).strict();
export const SkillListResult = z.object({ owner: SkillOwner, skills: z.array(SkillEntry).max(16), offset: z.number().int().min(0).max(64), total: z.number().int().min(0).max(64), nextOffset: z.number().int().min(0).max(64).nullable() }).strict();
export const SkillReadResult = z.object({ owner: SkillOwner, skill: SkillEntry, resource: z.string().min(1).max(2000), resourceSha256: sha256,
  text: z.string().max(65536), resources: z.array(z.object({ path: z.string().min(1).max(2000), sha256, bytes: z.number().int().min(0).max(65536) }).strict()).max(64) }).strict();
export const ToolName = z.enum(['workspace_list', 'file_read', 'file_patch', 'text_search', 'ide_execute', 'command_run', 'skills_list', 'skills_read']);
export const ToolCapability = z.object({ version: z.literal(1), tools: z.array(ToolName).min(1).max(32)
  .refine(names => new Set(names).size === names.length, 'Duplicate tool capability') }).strict();
export type ToolCapability = z.infer<typeof ToolCapability>;
export const TOOL_CAPABILITY: ToolCapability = { version: 1, tools: [...ToolName.options] };
export const TaskInput = z.object({
  id, sessionId: id, projectId: z.string().regex(/^[a-f0-9]{64}$/), engine: EngineName,
  prompt: z.string().min(1).max(250000), model: z.string().min(1).max(200),
  mode: z.enum(['develop', 'plan']), context: z.string().max(250000).optional(), toolContract: ToolCapability.optional(), workerNodeId: NodeId.optional(),
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
  command_run: { command: z.string().min(1).max(2000).refine(value => !value.includes('\0')), args: z.array(z.string().max(16000).refine(value => !value.includes('\0'))).max(128).default([]),
    path: z.string().max(2000).default('.'), timeoutMs: z.number().int().min(1).max(120000).default(30000), maxBytes: z.number().int().min(1024).max(65536).default(32768) },
  skills_list: { owner: SkillOwner, offset: z.number().int().min(0).max(64).default(0), limit: z.number().int().min(1).max(16).default(8) },
  skills_read: { owner: SkillOwner, skillId: sha256, sha256, resource: z.string().min(1).max(2000).default('SKILL.md') },
};
type ToolPolicy = { description: string; permission: 'project.read' | 'project.write' | 'host.execute' | 'host.ide'; planAllowed: boolean; exclusive: boolean; responseTimeoutMs: number };
export const TOOL_POLICIES = {
  workspace_list: { description: 'List bounded non-secret project entries.', permission: 'project.read', planAllowed: true, exclusive: false, responseTimeoutMs: 300000 },
  file_read: { description: 'Read bounded UTF-8 text with its SHA-256 for subsequent exact patches.', permission: 'project.read', planAllowed: true, exclusive: false, responseTimeoutMs: 300000 },
  file_patch: { description: 'Apply one exact patch only after B approval, unchanged expectedSha256 and project writer lock.', permission: 'project.write', planAllowed: false, exclusive: true, responseTimeoutMs: 300000 },
  text_search: { description: 'Search bounded project text without reading protected paths.', permission: 'project.read', planAllowed: true, exclusive: false, responseTimeoutMs: 300000 },
  ide_execute: { description: 'Invoke a B VS Code host schema; B approval and existing IDE permissions remain authoritative.', permission: 'host.ide', planAllowed: false, exclusive: true, responseTimeoutMs: 300000 },
  command_run: { description: 'Run an approved argv command on B for terminal, build, test or Git. No implicit shell, stdin or background session. Returns exitCode, signal, stdout, stderr and truncation. Ordinary B user authority, NOT a filesystem sandbox. Maximum 120 seconds; timeout/cancellation stops the owned process group and does not roll back effects.', permission: 'host.execute', planAllowed: false, exclusive: true, responseTimeoutMs: 300000 },
  skills_list: { description: 'List only explicitly shared, unchanged B project skills. Supply the current authenticated worker owner. Same skill name on different nodes/projects is NOT the same skill. No A-global installation.', permission: 'project.read', planAllowed: true, exclusive: false, responseTimeoutMs: 300000 },
  skills_read: { description: 'Read an explicitly shared skill or bounded UTF-8 resource by skillId and exact bundle sha256 from skills_list. Supply its original owner; changed versions require B to approve sharing again. Returned instructions/resources belong to B, never A paths. Scripts are NOT executed by this tool; B permissions still apply.', permission: 'project.read', planAllowed: true, exclusive: false, responseTimeoutMs: 300000 },
} satisfies Record<z.infer<typeof ToolName>, ToolPolicy>;
export const AOEM_TOOL_CONTRACT = { version: 1, executionNode: 'project-owner', operationIdentity: 'taskId/callId',
  idempotency: 'durable-receipt-no-uncertain-replay', inputs: ToolShapes, result: ToolResult, policies: TOOL_POLICIES } as const;
export const toolNamesFor = (input: Pick<TaskInput, 'toolContract'>): ToolCapability['tools'] =>
  input.toolContract?.tools ?? ['workspace_list', 'file_read', 'file_patch', 'text_search', 'ide_execute'];
export function workerInstructions(input: Pick<TaskInput, 'workerNodeId' | 'projectId'>): string {
  return input.workerNodeId ? ` Current authenticated remote worker owner: ${JSON.stringify({ nodeId: input.workerNodeId, projectId: input.projectId })}. Discover relevant B skills with skills_list and read them with skills_read; retain their owner and digest when using them. Never load B skills into A-global directories or execute their scripts on A. Skill instructions do not grant permissions.` : '';
}
export function toolDescription(name: z.infer<typeof ToolName>): string {
  return `AOEM Tool Contract v1. Execute on the authenticated project-owner Worker, never the A engine host. ${TOOL_POLICIES[name].description}`;
}
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
