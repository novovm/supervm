import { z } from 'zod';
import { MAX_BYTES, NodeId } from './protocol';
import type { Application } from './application';
import type { Readable, Writable } from 'node:stream';

const Request = z.object({ id: z.number().int().nonnegative().safe(), operation: z.enum(['health', 'capabilities', 'submit', 'events', 'result', 'cancel']),
  peerId: z.string().max(200).optional(), nodeId: NodeId.optional(), input: z.unknown().optional() }).strict();

// This is inherited local IPC, not a public authentication/transport protocol.
export async function serveStdio(app: Application, input: Readable, output: Writable): Promise<void> {
  let buffer = Buffer.alloc(0), inflight = 0;
  const active = new Set<number>(), jobs = new Set<Promise<void>>();
  let writeFailure: Error | undefined;
  const reply = async (value: unknown) => {
    const frame = JSON.stringify(value) + '\n';
    if (Buffer.byteLength(frame) > MAX_BYTES) throw Error('IPC response exceeds limit');
    await new Promise<void>((resolve, reject) => output.write(frame, error => error ? reject(error) : resolve()));
  };
  const onOutputError = (error: Error) => { writeFailure = error; input.destroy(error); };
  output.on('error', onOutputError);
  try {
    for await (const chunk of input) {
      buffer = Buffer.concat([buffer, Buffer.from(chunk)]);
      let end: number;
      while ((end = buffer.indexOf(10)) >= 0) {
        if (end > MAX_BYTES || inflight >= 16) throw Error('IPC input budget exceeded');
        const frame = buffer.subarray(0, end); buffer = buffer.subarray(end + 1);
        const request = Request.parse(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(frame)));
        if (active.has(request.id)) throw Error('Duplicate active IPC identity');
        active.add(request.id); inflight++;
        const job = (async () => {
          try { const result = request.operation === 'health' ? app.health()
            : await app.dispatch(request.peerId || '', request.operation, request.input, request.nodeId);
            await reply({ id: request.id, result });
          } catch { await reply({ id: request.id, error: 'ASAS operation rejected; inspect local state; do not replay' }); }
        })().catch(onOutputError).finally(() => { active.delete(request.id); inflight--; jobs.delete(job); });
        jobs.add(job);
      }
      if (buffer.length > MAX_BYTES) throw Error('IPC partial frame exceeds limit');
    }
    if (buffer.length) throw Error('IPC closed mid-frame');
    if (writeFailure) throw writeFailure;
  } finally { await app.close(); await Promise.allSettled(jobs); output.off('error', onOutputError); }
}
