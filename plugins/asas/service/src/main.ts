import { readFile, realpath } from 'node:fs/promises';
import { randomBytes, createHash } from 'node:crypto';
import path from 'node:path';
import { startService } from './server';
import { Configuration, enginesFor } from './config';
import { Application } from './application';
import { serveStdio } from './stdio';
import { quotaAdminRequest } from './quota-admin';

async function main() {
  if (process.argv[2] === 'pair') {
    const directory = await realpath(process.argv[3] || process.cwd()), token = randomBytes(32).toString('base64url');
    console.log(JSON.stringify({ peer: { id: process.argv[4] || 'B', tokenSha256: createHash('sha256').update(token).digest('hex'), projects: [createHash('sha256').update(directory).digest('hex')] }, clientToken: token }, null, 2)); return;
  }
  const stdio = process.argv[2] === '--stdio', quota = process.argv[2] === 'quota';
  const file = process.argv[stdio || quota ? 3 : 2]; if (!file) throw Error('Usage: npm start -- /absolute/config.json; or --stdio CONFIG; or quota CONFIG [set PEER NODE TOKENS|reset PEER NODE CYCLE]; or pair /client/project B');
  const config = Configuration.parse(JSON.parse(await readFile(file, 'utf8')));
  if (quota) {
    const operation = process.argv[4] || 'report';
    const owner = { peerId: process.argv[5], nodeId: process.argv[6] === '-' ? null : process.argv[6] };
    const command = operation === 'report' ? { operation } : operation === 'set' ? { operation, ...owner, limitTokens: process.argv[7] === 'unlimited' ? null : Number(process.argv[7]) }
      : { operation, ...owner, expectedCycleId: process.argv[7] };
    console.log(JSON.stringify(await quotaAdminRequest(config.directory, command), null, 2)); return;
  }
  const engines = enginesFor(config);
  if (!path.isAbsolute(config.directory)) throw Error('ASAS data directory must be absolute');
  if (stdio) {
    const app = await Application.open(config.directory, config.peers, engines);
    await serveStdio(app, process.stdin, process.stdout); return;
  }
  const tls = config.tlsFiles ? { cert: await readFile(config.tlsFiles.cert, 'utf8'), key: await readFile(config.tlsFiles.key, 'utf8') } : undefined;
  const service = await startService({ ...config, tls }, engines);
  console.log(`ASAS listening on ${config.host}:${service.port}; no task or model inference started`);
  let stopping = false; const close = () => { if (!stopping) { stopping = true; void service.close().then(() => process.exit(0), () => process.exit(1)); } };
  process.once('SIGINT', close); process.once('SIGTERM', close);
}
void main().catch(() => { console.error('ASAS startup or IPC failed; inspect private configuration and task state'); process.exitCode = 1; });
