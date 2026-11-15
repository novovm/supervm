import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { Event, MAX_BYTES, TaskInput, ToolResult, VERSION, endpoint, type AgentEvent, type TaskStatus } from './protocol';
import { saveJson } from './storage';
export type { AgentEvent, TaskStatus, TaskInput, ToolResult } from './protocol';
export { parseTool } from './protocol';
export { saveJson } from './storage';

export class AsasClient {
  private base: URL;
  constructor(url: string, private token: string) {
    this.base = endpoint(url);
    if (token.length < 32 || token.length > 512 || /\s/.test(token)) throw Error('Invalid ASAS peer token');
  }
  private async request(route: string, method = 'GET', body?: unknown, signal?: AbortSignal): Promise<any> {
    const response = await fetch(new URL(route, this.base), { method, redirect: 'error', signal: AbortSignal.any([signal || new AbortController().signal, AbortSignal.timeout(35000)]),
      headers: { Authorization: `Bearer ${this.token}`, 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    let bytes = 0; const chunks: Buffer[] = [];
    for await (const chunk of response.body || []) { bytes += chunk.length; if (bytes > MAX_BYTES) throw Error('ASAS response exceeds limit'); chunks.push(Buffer.from(chunk)); }
    const text = new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks));
    if (!response.ok) throw Error(`ASAS ${response.status}: ${text.slice(0, 600)}`);
    return JSON.parse(text);
  }
  async health(signal?: AbortSignal): Promise<{ version: number; peerId: string; projects: string[]; models: string[] }> {
    const value = await this.request('v1/health', 'GET', undefined, signal);
    if (value.version !== VERSION || typeof value.peerId !== 'string' || !Array.isArray(value.projects) || !Array.isArray(value.models)) throw Error('Incompatible ASAS server');
    return value;
  }
  async submit(input: TaskInput, signal?: AbortSignal): Promise<{ status: TaskStatus }> {
    return this.request('v1/tasks', 'POST', TaskInput.parse(input), signal);
  }
  async events(taskId: string, after: number, signal?: AbortSignal): Promise<{ status: TaskStatus; events: AgentEvent[]; activeCallIds: string[] }> {
    const result = await this.request(`v1/tasks/${taskId}/events?after=${after}`, 'GET', undefined, signal);
    if (!['running', 'completed', 'failed', 'cancelled', 'needs_recovery'].includes(result.status) || !Array.isArray(result.events)) throw Error('Invalid ASAS task state');
    if (!Array.isArray(result.activeCallIds) || !result.activeCallIds.every((id: unknown) => typeof id === 'string')) throw Error('Invalid pending-call state');
    return { status: result.status, events: result.events.map((event: unknown) => Event.parse(event)), activeCallIds: result.activeCallIds };
  }
  async result(taskId: string, result: ToolResult, signal?: AbortSignal): Promise<void> {
    await this.request(`v1/tasks/${taskId}/results`, 'POST', ToolResult.parse(result), signal);
  }
  async cancel(taskId: string): Promise<void> { await this.request(`v1/tasks/${taskId}/cancel`, 'POST', {}); }
  async follow(input: TaskInput, receiptFile: string, execute: (event: AgentEvent) => Promise<ToolResult>,
    receive: (event: AgentEvent) => void, signal: AbortSignal): Promise<TaskStatus> {
    let after = 0;
    try { const saved = JSON.parse(await readFile(receiptFile, 'utf8')); if (saved.taskId !== input.id || !Number.isSafeInteger(saved.after) || saved.after < 0) throw Error('Task receipt mismatch'); after = saved.after; }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    await saveJson(receiptFile, { taskId: input.id, after });
    // Re-submission is admission-idempotent; the server never reruns an existing task.
    await this.submit(input, signal);
    let failures = 0;
    while (true) {
      signal.throwIfAborted();
      let response;
      try { response = await this.events(input.id, after, signal); failures = 0; }
      catch (error) {
        if (signal.aborted || ++failures > 3 || /ASAS 4\d\d/.test(String(error))) throw error;
        await new Promise<void>((resolve, reject) => { const timer = setTimeout(done, failures * 500);
          function done() { signal.removeEventListener('abort', abort); resolve(); }
          function abort() { clearTimeout(timer); reject(signal.reason); } signal.addEventListener('abort', abort, { once: true }); });
        continue;
      }
      for (const event of response.events) {
        if (event.taskId !== input.id || event.sequence !== after + 1) throw Error('ASAS event identity or sequence gap; query original task, do not restart');
        receive(event);
        if (event.type === 'tool' && response.activeCallIds.includes(event.callId!)) await this.result(input.id, await execute(event), signal);
        after = event.sequence; await saveJson(receiptFile, { taskId: input.id, after });
      }
      if (response.status !== 'running') return response.status;
    }
  }
}
export const projectId = (canonicalDirectory: string): string => createHash('sha256').update(canonicalDirectory).digest('hex');
