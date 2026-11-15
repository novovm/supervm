import type { Readable, Writable } from 'node:stream';
import { Event, id, TaskInput, ToolResult, MAX_BYTES, VERSION, type AgentEvent, type TaskStatus } from './protocol';
import { followTask } from './client';

export class NativeClient {
  private nextId = 0;
  private buffer = Buffer.alloc(0);
  private closed = false;
  private pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void; timer: NodeJS.Timeout }>();
  constructor(private input: Readable, private output: Writable, private peerId: string) {
    input.on('data', this.data); input.once('end', this.end); input.once('close', this.end);
    input.once('error', this.error); output.once('error', this.error); output.once('close', this.end);
  }
  private request(operation: string, value?: unknown, signal?: AbortSignal): Promise<any> {
    if (this.closed || this.pending.size >= 16) return Promise.reject(Error('ASAS IPC unavailable or busy'));
    signal?.throwIfAborted();
    const requestId = this.nextId++, frame = JSON.stringify({ id: requestId, operation, ...(operation === 'health' ? {} : { peerId: this.peerId, input: value }) }) + '\n';
    if (Buffer.byteLength(frame) > MAX_BYTES) return Promise.reject(Error('ASAS IPC request exceeds limit'));
    return new Promise((resolve, reject) => {
      const abort = () => { clearTimeout(timer); this.pending.delete(requestId); reject(Error('ASAS IPC cancelled; task effects may persist')); };
      const timer = setTimeout(() => { signal?.removeEventListener('abort', abort); this.pending.delete(requestId); reject(Error('ASAS IPC timed out; do not replay')); }, 35000);
      this.pending.set(requestId, { timer, resolve: value => { signal?.removeEventListener('abort', abort); resolve(value); }, reject: error => { signal?.removeEventListener('abort', abort); reject(error); } });
      signal?.addEventListener('abort', abort, { once: true });
      this.output.write(frame, error => { if (error) this.close(); });
    });
  }
  private data = (chunk: Buffer) => {
    try {
      this.buffer = Buffer.concat([this.buffer, chunk]); let end: number;
      while ((end = this.buffer.indexOf(10)) >= 0) {
        if (end > MAX_BYTES) throw Error('ASAS IPC response exceeds limit');
        const value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(this.buffer.subarray(0, end)));
        this.buffer = this.buffer.subarray(end + 1);
        if (!Number.isSafeInteger(value.id) || ('result' in value) === ('error' in value)) throw Error('Invalid ASAS IPC envelope');
        const pending = this.pending.get(value.id); if (!pending) continue;
        clearTimeout(pending.timer); this.pending.delete(value.id);
        if ('error' in value) pending.reject(Error('ASAS application rejected request; no automatic replay')); else pending.resolve(value.result);
      }
      if (this.buffer.length > MAX_BYTES) throw Error('ASAS IPC partial frame exceeds limit');
    } catch { this.close(); }
  };
  private end = () => this.close();
  private error = () => this.close();
  close() {
    if (this.closed) return; this.closed = true;
    for (const stream of [this.input, this.output]) if (!stream.closed) {
      const ignoreLateError = () => {}; stream.on('error', ignoreLateError); stream.once('close', () => stream.off('error', ignoreLateError));
    }
    this.input.off('data', this.data); this.input.off('end', this.end); this.input.off('close', this.end);
    this.input.off('error', this.error); this.output.off('error', this.error); this.output.off('close', this.end);
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(Error('ASAS IPC closed; inspect original task')); }
    this.pending.clear();
  }
  async health(signal?: AbortSignal) { const value = await this.request('health', undefined, signal); if (value.version !== VERSION) throw Error('ASAS protocol version mismatch'); return value; }
  async submit(input: TaskInput, signal?: AbortSignal) { return this.request('submit', TaskInput.parse(input), signal); }
  async events(taskId: string, after: number, signal?: AbortSignal): Promise<{ status: TaskStatus; events: AgentEvent[]; activeCallIds: string[] }> {
    const value = await this.request('events', { taskId: id.parse(taskId), after, waitMs: 1000 }, signal);
    if (!['running', 'completed', 'failed', 'cancelled', 'needs_recovery'].includes(value.status) || !Array.isArray(value.events)
      || !Array.isArray(value.activeCallIds) || !value.activeCallIds.every((callId: unknown) => id.safeParse(callId).success)) throw Error('Invalid ASAS task page');
    return { status: value.status, events: value.events.map((event: unknown) => Event.parse(event)), activeCallIds: value.activeCallIds };
  }
  async result(taskId: string, result: ToolResult, signal?: AbortSignal) { await this.request('result', { taskId: id.parse(taskId), result: ToolResult.parse(result) }, signal); }
  async cancel(taskId: string) { await this.request('cancel', { taskId: id.parse(taskId) }); }
  follow(input: TaskInput, receiptFile: string, execute: (event: AgentEvent) => Promise<ToolResult>, receive: (event: AgentEvent) => void, signal: AbortSignal) {
    return followTask(this, input, receiptFile, execute, receive, signal);
  }
}
