import { EventEmitter } from 'node:events';
import { readFile, readdir, mkdir } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { z } from 'zod';
import path from 'node:path';
import { saveJson } from './storage';
import { QuotaStore, type PlanSnapshot } from './quota';
import { TaskInput, ToolResult, StoredTask, EngineName, ToolName, TOOL_POLICIES, toolNamesFor, NodeId, SkillListResult, SkillReadResult, TokenCounts, id, parseTool, EngineControls, type AgentEvent, type TaskState } from './protocol';

export type Peer = { id: string; tokenSha256: string; projects: string[]; nodeId?: string };
const SessionSchema = z.object({ id, peerId: z.string().min(1).max(200), projectId: z.string().regex(/^[a-f0-9]{64}$/),
  engine: EngineName, model: z.string().min(1).max(200), workerNodeId: NodeId.optional(), nativeSessionId: z.string().regex(/^[a-zA-Z0-9_-]{1,200}$/).optional(), claudeUsageTotal: TokenCounts.optional(), active: id.optional(), recovery: z.boolean().optional() }).strict();
export type Session = z.infer<typeof SessionSchema>;
export type EngineContext = { input: TaskInput; session: Session; signal: AbortSignal;
  text: (text: string) => Promise<void>; call: (name: string, args: unknown) => Promise<ToolResult>; checkpoint: (id: string) => Promise<void>;
  usage?: (counts: TokenCounts, final: boolean, cumulative?: TokenCounts) => Promise<void>; plan?: (snapshot: PlanSnapshot) => Promise<void> };
export type Engine = (context: EngineContext) => Promise<void>;
export type Engines = Partial<Record<EngineName, { models: string[]; run: Engine; controls?: () => Promise<EngineControls> }>>;
const fingerprint = (value: unknown) => createHash('sha256').update(JSON.stringify(value)).digest('hex');

export class Broker extends EventEmitter {
  private tasks = new Map<string, TaskState>();
  private sessions = new Map<string, Session>();
  private controllers = new Map<string, AbortController>();
  private pending = new Map<string, { taskId: string; resolve: (result: ToolResult) => void; timer: ReturnType<typeof setTimeout> }>();
  private writes: Promise<void> = Promise.resolve();
  private admissions: Promise<unknown> = Promise.resolve();
  private mutations: Promise<unknown> = Promise.resolve();
  private results: Promise<unknown> = Promise.resolve();
  private storageFailed = false;
  readonly meter: QuotaStore;
  constructor(private directory: string, private engines: Engines) { super(); this.meter = new QuotaStore(directory); }
  private file(kind: string, id: string) { return path.join(this.directory, kind, id + '.json'); }
  private save(kind: string, id: string, value: unknown): Promise<void> {
    const snapshot = structuredClone(value);
    const next = this.writes.then(() => saveJson(this.file(kind, id), snapshot));
    this.writes = next.catch(() => { this.storageFailed = true; }); return next;
  }
  async load(): Promise<void> {
    await this.meter.load();
    for (const kind of ['tasks', 'sessions']) {
      await mkdir(path.join(this.directory, kind), { recursive: true, mode: 0o700 });
      for (const name of await readdir(path.join(this.directory, kind))) {
        if (!/^[a-f0-9-]{36}\.json$/.test(name)) { if (name.endsWith('.json')) throw Error('Invalid durable record filename'); continue; }
        const value = JSON.parse(await readFile(path.join(this.directory, kind, name), 'utf8'));
        if (kind === 'tasks') {
          StoredTask.parse(value);
          if (name !== value.id + '.json' || value.events.some((event: AgentEvent, index: number) => event.taskId !== value.id || event.sequence !== index + 1)) throw Error('Invalid persisted task identity or sequence');
          if (value.status === 'running') value.status = 'needs_recovery';
          this.tasks.set(value.id, value); await this.save(kind, value.id, value);
        } else { SessionSchema.parse(value); if (name !== value.id + '.json') throw Error('Invalid persisted session identity'); if (value.active) value.recovery = true; value.active = undefined; this.sessions.set(value.id, value); await this.save(kind, value.id, value); }
      }
    }
    for (const task of this.tasks.values()) {
      const session = this.sessions.get(task.sessionId);
      if (!session || session.peerId !== task.peerId || session.workerNodeId !== task.workerNodeId || session.projectId !== task.projectId || session.engine !== task.engine || session.model !== task.model)
        throw Error('Persisted task/session binding mismatch');
      if (task.status === 'needs_recovery') { session.recovery = true; await this.save('sessions', session.id, session); }
    }
  }
  get(peer: Peer, id: string): TaskState {
    const task = this.tasks.get(id);
    if (!task || task.peerId !== peer.id || task.workerNodeId !== peer.nodeId || !peer.projects.includes(task.projectId)) throw Error('404 task not found');
    return task;
  }
  submit(peer: Peer, value: unknown): Promise<TaskState> {
    const work = this.admissions.then(async () => {
      if (this.storageFailed) throw Error('503 storage requires operator recovery');
      const input = TaskInput.parse(value);
      if (input.workerNodeId !== peer.nodeId || (input.toolContract?.tools.some(name => name.startsWith('skills_')) && !peer.nodeId)) throw Error('403 authenticated worker identity required; body cannot select another B node');
      if (!peer.projects.includes(input.projectId) || !this.engines[input.engine]?.models.includes(input.model)) throw Error('403 project, engine or model not authorized');
      const prior = this.tasks.get(input.id);
      if (prior) {
        this.get(peer, input.id);
        const { peerId: _peer, status: _status, events: _events, ...original } = prior;
        if (fingerprint(original) !== fingerprint(input)) throw Error('409 task identity reused with different input');
        return prior;
      }
      if (input.reasoningEffort || input.enginePermissionMode === 'auto') {
        const controls = EngineControls.parse(await this.engines[input.engine]?.controls?.()).models[input.model];
        if (input.reasoningEffort && !controls?.efforts.includes(input.reasoningEffort)
          || input.enginePermissionMode === 'auto' && (input.mode === 'plan' || !controls?.permissionModes.includes('auto')))
          throw Error('400 requested engine control is unavailable');
      }
      if (this.controllers.size >= 8) throw Error('429 active task limit');
      let session = this.sessions.get(input.sessionId);
      if (session && (session.peerId !== peer.id || session.workerNodeId !== peer.nodeId || session.projectId !== input.projectId || session.model !== input.model || session.engine !== input.engine)) throw Error('403 session binding mismatch');
      if (session?.active || session?.recovery) throw Error('409 session busy or needs operator recovery');
      if (input.engine === 'claude') await this.meter.admit(peer.id, peer.nodeId, input.id);
      session ??= { id: input.sessionId, peerId: peer.id, projectId: input.projectId, model: input.model, engine: input.engine, ...(peer.nodeId ? { workerNodeId: peer.nodeId } : {}) };
      session.active = input.id; this.sessions.set(session.id, session);
      const task: TaskState = { ...input, peerId: peer.id, status: 'running', events: [] };
      this.tasks.set(task.id, task);
      try { await this.save('sessions', session.id, session); await this.save('tasks', task.id, task); }
      catch (error) { session.recovery = true; task.status = 'needs_recovery'; throw error; }
      const controller = new AbortController(); this.controllers.set(task.id, controller);
      void this.run(task, session, controller); return task;
    });
    this.admissions = work.catch(() => undefined); return work;
  }
  private append(task: TaskState, event: Omit<AgentEvent, 'taskId' | 'sequence'>, status = task.status): Promise<void> {
    const work = this.mutations.then(async () => {
      const next = { ...event, taskId: task.id, sequence: task.events.length + 1 };
      if (Buffer.byteLength(JSON.stringify(next)) > 800000) throw Error('413 event exceeds bounded delivery size');
      const events = [...task.events, next];
      if (events.length > 2048 || Buffer.byteLength(JSON.stringify(events)) > 8 * 1024 * 1024) throw Error('Task event budget exhausted');
      await this.save('tasks', task.id, { ...task, events, status });
      task.events = events; task.status = status; this.emit(task.id);
    });
    this.mutations = work.catch(() => undefined); return work;
  }
  private async run(task: TaskState, session: Session, controller: AbortController): Promise<void> {
    try {
      await this.append(task, { type: 'started' });
      await this.engines[task.engine]!.run({ input: task, session, signal: controller.signal,
        usage: async (counts, final, cumulative) => {
          if (task.engine !== 'claude') return;
          await this.meter.observe(task.id, counts, final);
          if (final && cumulative) { session.claudeUsageTotal = TokenCounts.parse(cumulative); await this.save('sessions', session.id, session); }
          if (!final && this.meter.view(task.peerId, task.workerNodeId).status === 'exhausted') controller.abort(Error('Claude allocation reached its token limit'));
        },
        plan: snapshot => this.meter.plan(snapshot),
        text: async text => { controller.signal.throwIfAborted(); if (Buffer.byteLength(text) > 200000) throw Error('Text event too large'); await this.append(task, { type: 'text', text }); },
        checkpoint: async value => { session.nativeSessionId = SessionSchema.shape.nativeSessionId.unwrap().parse(value); await this.save('sessions', session.id, session); },
        call: async (name, args) => {
          controller.signal.throwIfAborted(); const key = ToolName.parse(name), parsed = parseTool(key, args), policy = TOOL_POLICIES[key];
          if (task.engine === 'claude' && this.meter.view(task.peerId, task.workerNodeId).status === 'exhausted') {
            controller.abort(Error('Claude allocation exhausted')); controller.signal.throwIfAborted();
          }
          if (!toolNamesFor(task).includes(key)) return { callId: randomUUID(), ok: false, output: 'B did not advertise this tool capability' };
          if (key === 'skills_list' || key === 'skills_read') {
            const owner = parsed.owner as { nodeId: string; projectId: string };
            if (owner.nodeId !== task.workerNodeId || owner.projectId !== task.projectId) return { callId: randomUUID(), ok: false, output: 'Skill owner does not match the authenticated task worker' };
          }
          if (task.mode === 'plan' && !policy.planAllowed) return { callId: randomUUID(), ok: false, output: 'Plan mode denies mutating tools' };
          const callId = randomUUID();
          const result = new Promise<ToolResult>((resolve, reject) => {
            const abort = () => { session.recovery = true; clearTimeout(timer); this.pending.delete(callId); reject(controller.signal.reason || Error('Task cancelled')); };
            const timer = setTimeout(() => { session.recovery = true; controller.signal.removeEventListener('abort', abort); this.pending.delete(callId); reject(Error('Worker response timeout; tool execution state may be unknown')); }, policy.responseTimeoutMs);
            controller.signal.addEventListener('abort', abort, { once: true });
            this.pending.set(callId, { taskId: task.id, timer, resolve: value => { controller.signal.removeEventListener('abort', abort); resolve(value); } });
          });
          // Attach rejection handling before the durable event write can fail.
          void result.catch(() => undefined);
          await this.append(task, { type: 'tool', callId, tool: name as AgentEvent['tool'], arguments: parsed });
          return result;
        } });
      controller.signal.throwIfAborted();
      if ([...this.pending.values()].some(value => value.taskId === task.id)) throw Error('Engine returned with unresolved tool calls');
      await this.append(task, { type: 'completed' }, 'completed');
    } catch (error) {
      const unresolved = session.recovery || [...this.pending.values()].some(value => value.taskId === task.id);
      const status = unresolved ? 'needs_recovery' : controller.signal.aborted ? 'cancelled' : 'failed';
      if (unresolved) session.recovery = true;
      try { await this.append(task, { type: status, text: 'Engine stopped; inspect private diagnostics or recover the original task' }, status); }
      catch { task.status = 'needs_recovery'; session.recovery = true; await this.save('tasks', task.id, task).catch(() => undefined); }
    } finally {
      if (task.engine === 'claude') await this.meter.finish(task.id).catch(() => { session.recovery = true; this.storageFailed = true; });
      session.active = undefined;
      for (const [id, value] of this.pending) if (value.taskId === task.id) { clearTimeout(value.timer); this.pending.delete(id); }
      await this.save('sessions', session.id, session).catch(() => { session.recovery = true; });
      this.controllers.delete(task.id); this.emit(task.id);
    }
  }
  result(peer: Peer, id: string, value: unknown): Promise<void> {
    const work = this.results.then(() => this.acceptResult(peer, id, value));
    this.results = work.catch(() => undefined); return work;
  }
  private async acceptResult(peer: Peer, id: string, value: unknown): Promise<void> {
    if (this.storageFailed) throw Error('503 storage requires operator recovery');
    const task = this.get(peer, id), result = ToolResult.parse(value);
    const previous = task.events.find(event => event.type === 'tool_result' && event.callId === result.callId);
    const text = JSON.stringify(result);
    if (previous) { if (previous.text !== text) throw Error('409 conflicting tool result'); return; }
    const pending = this.pending.get(result.callId);
    if (task.status !== 'running' || !pending || pending.taskId !== task.id) throw Error('409 tool call is no longer active');
    const call = task.events.find(event => event.type === 'tool' && event.callId === result.callId);
    if (result.ok && (call?.tool === 'skills_list' || call?.tool === 'skills_read')) {
      const receipt = (call.tool === 'skills_list' ? SkillListResult : SkillReadResult).parse(JSON.parse(result.output));
      if (receipt.owner.nodeId !== task.workerNodeId || receipt.owner.projectId !== task.projectId) throw Error('403 skill receipt belongs to another worker');
      if (call.tool === 'skills_read') {
        const value = SkillReadResult.parse(receipt);
        if (value.skill.skillId !== call.arguments?.skillId || value.skill.sha256 !== call.arguments?.sha256 || value.resource !== call.arguments?.resource) throw Error('409 skill receipt identity mismatch');
        const resource = value.resources.find(resource => resource.path === value.resource);
        if (fingerprint(value.resources) !== value.skill.sha256 || new Set(value.resources.map(resource => resource.path)).size !== value.resources.length
          || !resource || resource.sha256 !== value.resourceSha256 || createHash('sha256').update(value.text).digest('hex') !== value.resourceSha256) throw Error('409 skill receipt content digest mismatch');
      }
    }
    await this.append(task, { type: 'tool_result', callId: result.callId, text });
    clearTimeout(pending.timer); this.pending.delete(result.callId); pending.resolve(result);
  }
  async events(peer: Peer, id: string, after: number, waitMs = 20000): Promise<{ status: string; events: AgentEvent[]; activeCallIds: string[] }> {
    const task = this.get(peer, id);
    if (!Number.isSafeInteger(after) || after < 0 || after > task.events.length || !Number.isSafeInteger(waitMs) || waitMs < 0 || waitMs > 20000) throw Error('409 invalid event cursor or wait budget');
    if (task.events.length === after && (task.status === 'running' || this.controllers.has(task.id))) await new Promise<void>(resolve => {
      const done = () => { clearTimeout(timer); this.removeListener(id, done); resolve(); };
      const timer = setTimeout(done, waitMs); this.once(id, done);
    });
    const events: AgentEvent[] = [];
    for (const event of task.events.slice(after, after + 4)) {
      if (events.length && Buffer.byteLength(JSON.stringify([...events, event])) > 900000) break;
      events.push(event);
    }
    return { status: after + events.length < task.events.length || this.controllers.has(task.id) ? 'running' : task.status, events,
      activeCallIds: [...this.pending].filter(([, value]) => value.taskId === id).map(([callId]) => callId) };
  }
  async cancel(peer: Peer, id: string): Promise<void> { const task = this.get(peer, id); this.controllers.get(task.id)?.abort(Error('Cancelled by owning peer')); }
  async close(): Promise<void> { for (const controller of this.controllers.values()) controller.abort(Error('Service shutting down')); while (this.controllers.size) await new Promise(resolve => setTimeout(resolve, 10)); await this.writes; await this.meter.close(); }
}
