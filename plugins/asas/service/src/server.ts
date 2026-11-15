import { createServer as httpServer, type Server, type IncomingMessage, type ServerResponse } from 'node:http';
import { createServer as httpsServer } from 'node:https';
import { createHash, timingSafeEqual } from 'node:crypto';
import { type Peer, type Engine, type Engines } from './broker';
import { Application } from './application';
import { MAX_BYTES, VERSION, id } from './protocol';

export type ServiceConfig = { directory: string; host: string; port: number; peers: Peer[]; models: string[]; tls?: { cert: string; key: string }; executable?: string };
export async function startService(config: ServiceConfig, engine: Engine | Engines) {
  if (!['127.0.0.1', '::1', 'localhost'].includes(config.host) && !config.tls) throw Error('Non-loopback ASAS listeners require TLS');
  if (!config.peers.length || config.peers.some(peer => !peer.id || !/^[a-f0-9]{64}$/.test(peer.tokenSha256) || !peer.projects.length || peer.projects.some(project => !/^[a-f0-9]{64}$/.test(project)))
    || new Set(config.peers.map(peer => peer.id)).size !== config.peers.length || new Set(config.peers.map(peer => peer.tokenSha256)).size !== config.peers.length) throw Error('Explicit unique peer identities, token hashes and project grants are required');
  if (!config.models.length || config.models.some(model => typeof model !== 'string' || !model.trim() || model.length > 200)) throw Error('Explicit model allowlist required');
  const engines = typeof engine === 'function' ? { claude: { models: config.models, run: engine } } : engine;
  const application = await Application.open(config.directory, config.peers, engines), broker = application.broker;
  let requests = 0;
  const handler = async (request: IncomingMessage, response: ServerResponse) => {
    response.setHeader('Content-Type', 'application/json'); response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    const reply = (status: number, value: unknown) => { if (!response.destroyed) { response.statusCode = status; response.end(JSON.stringify(value)); } };
    let admitted = false;
    try {
      if (request.headers.origin) throw Error('403 browser-origin requests are not supported');
      const auth = request.headers.authorization || '';
      if (!/^Bearer [^\s]{32,512}$/.test(auth)) throw Error('401 invalid peer credential');
      const hash = createHash('sha256').update(auth.slice(7)).digest();
      const peer = config.peers.find(peer => timingSafeEqual(hash, Buffer.from(peer.tokenSha256, 'hex')));
      if (!peer) throw Error('401 unknown peer');
      if (requests >= 64) throw Error('429 request concurrency exceeded'); requests++; admitted = true;
      const url = new URL(request.url || '/', 'http://asas.invalid');
      const body = async () => { let bytes = 0; const chunks: Buffer[] = [];
        for await (const data of request) { bytes += data.length; if (bytes > MAX_BYTES) throw Error('413 input too large'); chunks.push(data); }
        return JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}'); };
      if (url.pathname === '/v1/health' && request.method === 'GET') { reply(200, { ...application.health(), version: VERSION, peerId: peer.id, projects: peer.projects, models: config.models }); return; }
      if (url.pathname === '/models' && request.method === 'GET') { reply(200, { data: config.models.map(model => ({ id: model })) }); return; }
      if (url.pathname === '/v1/tasks' && request.method === 'POST') { const task = await broker.submit(peer, await body()); reply(200, { id: task.id, status: task.status }); return; }
      const match = /^\/v1\/tasks\/([^/]+)\/(events|results|cancel)$/.exec(url.pathname);
      if (!match) throw Error('404 endpoint not found'); id.parse(match[1]);
      if (match[2] === 'events' && request.method === 'GET') { reply(200, await broker.events(peer, match[1], Number(url.searchParams.get('after') || '0'))); return; }
      if (match[2] === 'results' && request.method === 'POST') { await broker.result(peer, match[1], await body()); reply(200, { accepted: true }); return; }
      if (match[2] === 'cancel' && request.method === 'POST') { await broker.cancel(peer, match[1]); reply(200, { cancelRequested: true }); return; }
      throw Error('405 method not allowed');
    } catch (error) {
      const message = String(error), status = Number(message.match(/(?:Error: )?([45]\d\d) /)?.[1]) || (/ZodError|SyntaxError/.test(message) ? 400 : 500);
      reply(status, { error: `ASAS request rejected (${status}); no automatic replay` });
    } finally { if (admitted) requests--; }
  };
  const server = (config.tls ? httpsServer({ cert: config.tls.cert, key: config.tls.key, minVersion: 'TLSv1.3' }, (req, res) => void handler(req, res)) : httpServer((req, res) => void handler(req, res))) as Server;
  server.requestTimeout = 35000; server.headersTimeout = 10000; server.maxHeadersCount = 32;
  try { await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(config.port, config.host, resolve); }); }
  catch (error) { await application.close(); throw error; }
  return { server, broker, port: (server.address() as { port: number }).port, close: async () => { await application.close(); server.closeAllConnections(); await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve())); } };
}
