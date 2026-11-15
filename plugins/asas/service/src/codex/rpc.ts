// Reused from AOEM-CODE app-server-client/client.ts; see docs/CODEX.md for provenance.
import { EventEmitter } from 'node:events';
import { StringDecoder } from 'node:string_decoder';
import type { Readable, Writable } from 'node:stream';
type InitializeParams = { clientInfo: { name: string; title: string; version: string }; capabilities: { experimentalApi: boolean; requestAttestation: boolean } };
type InitializeResponse = { codexHome: string };
export type ServerRequest = { id: string | number; method: string; params: Record<string, unknown> };

type Id = string | number;
type Pending = { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: NodeJS.Timeout };
type Options = {
  timeoutMs?: number;
  maxFrameBytes?: number;
  handleRequest?: (request: ServerRequest) => Promise<unknown>;
};

export class RpcError extends Error {
  constructor(readonly code: number, message: string, readonly data?: unknown) {
    super(message);
    this.name = 'RpcError';
  }
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function id(value: unknown): value is Id {
  return typeof value === 'string' || (typeof value === 'number' && Number.isSafeInteger(value));
}

/** A stdio transport, not an Agent loop or a model provider. */
export class AppServerClient extends EventEmitter {
  private state: 'new' | 'initializing' | 'ready' | 'closed' = 'new';
  private nextId = 1;
  private pending = new Map<Id, Pending>();
  private inbound = new Set<Id>();
  private decoder = new StringDecoder('utf8');
  private buffer = '';
  private timeoutMs: number;
  private maxFrameBytes: number;

  get connected(): boolean { return this.state === 'ready'; }

  constructor(private input: Readable, private output: Writable, private options: Options = {}) {
    super();
    this.timeoutMs = options.timeoutMs ?? 30_000;
    this.maxFrameBytes = options.maxFrameBytes ?? 16 * 1024 * 1024;
    if (!Number.isFinite(this.timeoutMs) || this.timeoutMs <= 0 || !Number.isSafeInteger(this.maxFrameBytes) || this.maxFrameBytes <= 0) {
      throw new Error('Invalid transport limits');
    }
    input.on('data', this.onData);
    input.once('end', this.onEnd);
    input.once('close', this.onEnd);
    input.on('error', this.onError);
    output.on('error', this.onError);
    output.once('close', this.onOutputClose);
  }

  async initialize(params: InitializeParams = {
    clientInfo: { name: 'aoem_asas', title: 'ASAS', version: '0.3.0-dev' },
    capabilities: { experimentalApi: false, requestAttestation: false },
  }): Promise<InitializeResponse> {
    if (this.state !== 'new') throw new Error('Connection already initialized or closed');
    this.state = 'initializing';
    try {
      const response = await this.sendRequest('initialize', params) as InitializeResponse;
      this.write({ method: 'initialized' });
      this.state = 'ready';
      return response;
    } catch (error) {
      this.close(error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  request<T = unknown>(method: string, params: Record<string, unknown>): Promise<T> {
    if (this.state !== 'ready') return Promise.reject(new Error('Initialize the connection before requests'));
    return this.sendRequest(method, params) as Promise<T>;
  }

  close(error = new Error('App-server connection closed')): void {
    if (this.state === 'closed') return;
    this.state = 'closed';
    this.input.off('data', this.onData);
    this.input.off('end', this.onEnd);
    this.input.off('close', this.onEnd);
    // A failed write can emit its error after its callback has already closed us.
    for (const stream of [this.input, this.output]) {
      if (!stream.closed) {
        const ignoreLateError = (): void => {};
        stream.on('error', ignoreLateError);
        stream.once('close', () => stream.off('error', ignoreLateError));
      }
      stream.off('error', this.onError);
    }
    this.output.off('close', this.onOutputClose);
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
    this.inbound.clear();
    this.buffer = '';
    this.emit('closed', error);
  }

  private sendRequest(method: string, params: unknown): Promise<unknown> {
    const requestId = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        reject(new Error(`App-server ${method} acknowledgement timed out; execution state is unknown, do not retry automatically`));
      }, this.timeoutMs);
      this.pending.set(requestId, { resolve, reject, timer });
      try {
        this.write({ id: requestId, method, params });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(requestId);
        reject(error);
      }
    });
  }

  private write(message: unknown): void {
    if (this.state === 'closed' || this.output.destroyed || this.output.writableEnded) throw new Error('App-server output is closed');
    const line = `${JSON.stringify(message)}\n`;
    if (Buffer.byteLength(line) > this.maxFrameBytes) throw new Error('App-server frame exceeds transport limit');
    this.output.write(line, error => { if (error) this.close(error); });
  }

  private onError = (error: Error): void => { this.close(error); };
  private onOutputClose = (): void => { this.close(new Error('App-server output closed')); };
  private onEnd = (): void => {
    this.close(new Error(this.buffer.trim() ? 'App-server closed mid-frame' : 'App-server disconnected'));
  };
  private onData = (chunk: Buffer | string): void => {
    try {
      this.buffer += typeof chunk === 'string' ? chunk : this.decoder.write(chunk);
      let newline: number;
      while (this.state !== 'closed' && (newline = this.buffer.indexOf('\n')) >= 0) {
        const line = this.buffer.slice(0, newline);
        this.buffer = this.buffer.slice(newline + 1);
        if (Buffer.byteLength(line) > this.maxFrameBytes) throw new Error('App-server frame exceeds transport limit');
        if (line.trim()) this.receive(JSON.parse(line));
      }
      if (Buffer.byteLength(this.buffer) > this.maxFrameBytes) throw new Error('App-server partial frame exceeds transport limit');
    } catch (error) {
      this.close(error instanceof Error ? error : new Error(String(error)));
    }
  };

  private receive(message: unknown): void {
    if (!record(message) || ('jsonrpc' in message && message.jsonrpc !== '2.0')) throw new Error('Invalid app-server envelope');
    if (typeof message.method === 'string') {
      if ('result' in message || 'error' in message) throw new Error('Ambiguous app-server envelope');
      if (!('id' in message)) {
        this.emit('notification', message);
      } else {
        if (!id(message.id) || this.inbound.has(message.id)) throw new Error('Invalid or duplicate server request ID');
        this.inbound.add(message.id);
        void this.handleServerRequest(message as unknown as ServerRequest, message.id);
      }
      return;
    }
    if (!id(message.id) || ('result' in message) === ('error' in message)) throw new Error('Invalid app-server response');
    const pending = this.pending.get(message.id);
    if (!pending) return;
    if ('error' in message && (!record(message.error) || typeof message.error.code !== 'number' || typeof message.error.message !== 'string')) {
      throw new Error('Invalid app-server error');
    }
    this.pending.delete(message.id);
    clearTimeout(pending.timer);
    if (record(message.error)) pending.reject(new RpcError(message.error.code as number, message.error.message as string, message.error.data));
    else pending.resolve(message.result);
  }

  private async handleServerRequest(request: ServerRequest, requestId: Id): Promise<void> {
    let reply: object;
    try {
      if (!this.options.handleRequest) throw new RpcError(-32601, 'No client handler for this server request');
      const result = await this.options.handleRequest(request);
      if (result === undefined) throw new RpcError(-32603, 'Server request handler returned no result');
      reply = { id: requestId, result };
    } catch (error) {
      reply = { id: requestId, error: error instanceof RpcError
        ? { code: error.code, message: error.message, data: error.data }
        : { code: -32603, message: 'Client request handler failed' } };
    }
    this.inbound.delete(requestId);
    if (this.state !== 'closed') {
      try { this.write(reply); } catch (error) { this.close(error instanceof Error ? error : new Error(String(error))); }
    }
  }
}

