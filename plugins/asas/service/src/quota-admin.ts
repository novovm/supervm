import { createServer, createConnection, type Socket } from 'node:net';
import { chmod, readFile, rm } from 'node:fs/promises';
import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import path from 'node:path';
import { z } from 'zod';
import { saveJson } from './storage';
import { QuotaCommand, type QuotaStore } from './quota';

const Descriptor = z.object({ version: z.literal(1), address: z.string().min(1), token: z.string().regex(/^[a-f0-9]{64}$/) }).strict();
const descriptorFile = (directory: string) => path.join(directory, 'quota-admin.json');
const hash = (text: string) => createHash('sha256').update(text).digest();

/** Owner-only local IPC. Never registered in the NOVONET/B operation allowlist. */
export async function startQuotaAdmin(directory: string, meter: QuotaStore, peerIds: string[]) {
  const key = randomBytes(32).toString('hex');
  const address = process.platform === 'win32' ? `\\\\.\\pipe\\asas-quota-${key}` : path.join(directory, 'quota-admin.sock');
  if (process.platform !== 'win32' && Buffer.byteLength(address) > 100) throw Error('Private ASAS directory too long for quota IPC socket');
  // Called only after acquiring the existing exclusive application owner lock.
  if (process.platform !== 'win32') await rm(address, { force: true });
  const sockets = new Set<Socket>();
  const server = createServer(socket => {
    if (sockets.size >= 8) { socket.destroy(); return; }
    sockets.add(socket); socket.once('close', () => sockets.delete(socket)); socket.on('error', () => {}); socket.setTimeout(3000, () => socket.destroy());
    let buffer = Buffer.alloc(0), dispatched = false;
    socket.on('data', chunk => {
      if (dispatched) { socket.destroy(); return; }
      buffer = Buffer.concat([buffer, chunk]);
      if (buffer.length > 16384) { socket.destroy(); return; }
      const end = buffer.indexOf(10); if (end < 0) return;
      dispatched = true;
      void (async () => {
        const frame = z.object({ token: z.string().max(128), command: QuotaCommand }).strict().parse(JSON.parse(buffer.subarray(0, end).toString('utf8')));
        if (!timingSafeEqual(hash(frame.token), hash(key))) throw Error('Unauthorized local operator');
        if (frame.command.operation !== 'report' && !peerIds.includes(frame.command.peerId)) throw Error('Unknown application principal');
        const result = await meter.command(frame.command), reply = JSON.stringify({ result });
        if (Buffer.byteLength(reply) > 1024 * 1024) throw Error('Quota report exceeds local IPC limit');
        socket.end(reply + '\n');
      })().catch(() => socket.end(JSON.stringify({ error: 'Quota command rejected; inspect cycle identity, active tasks and private configuration' }) + '\n'));
    });
  });
  const close = async () => {
    for (const socket of sockets) socket.destroy();
    if (server.listening) await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    await rm(descriptorFile(directory), { force: true });
    if (process.platform !== 'win32') await rm(address, { force: true });
  };
  try {
    await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(address, resolve); });
    if (process.platform !== 'win32') await chmod(address, 0o600);
    await saveJson(descriptorFile(directory), { version: 1, address, token: key });
    return { close };
  } catch (error) { await close(); throw error; }
}

export async function quotaAdminRequest(directory: string, command: unknown): Promise<unknown> {
  const input = QuotaCommand.parse(command), descriptor = Descriptor.parse(JSON.parse(await readFile(descriptorFile(directory), 'utf8')));
  return new Promise((resolve, reject) => {
    const socket = createConnection(descriptor.address); let buffer = Buffer.alloc(0);
    const fail = (error: Error) => { socket.destroy(); reject(error); };
    socket.setTimeout(4000, () => fail(Error('Local quota owner did not answer'))); socket.on('error', fail);
    socket.once('connect', () => socket.write(JSON.stringify({ token: descriptor.token, command: input }) + '\n'));
    socket.on('data', chunk => {
      buffer = Buffer.concat([buffer, chunk]); if (buffer.length > 1024 * 1024) { fail(Error('Quota report too large')); return; }
      const end = buffer.indexOf(10); if (end < 0) return;
      try {
        const frame = JSON.parse(buffer.subarray(0, end).toString('utf8'));
        if (frame.error) throw Error(String(frame.error));
        socket.destroy(); resolve(frame.result);
      } catch (error) { fail(error as Error); }
    });
    socket.once('end', () => { if (!buffer.includes(10)) fail(Error('Incomplete quota reply')); });
  });
}
