import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { access, realpath, readFile, lstat } from 'node:fs/promises';
import { constants } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { z } from 'zod';
import { AppServerClient, type ServerRequest } from './rpc';

const absolute = z.string().min(1).max(4000).refine(value => path.isAbsolute(value) && !/[\r\n\0]/.test(value));
export const CodexProfile = z.object({ executable: absolute, executableSha256: z.string().regex(/^[a-f0-9]{64}$/),
  home: absolute, catalog: absolute, models: z.array(z.string().min(1).max(200)).min(1).max(100),
  effort: z.enum(['low', 'medium', 'high', 'xhigh']).default('medium') }).strict();
export type CodexProfile = z.infer<typeof CodexProfile>;

export class CodexProcess {
  private stopping?: Promise<void>;
  private constructor(readonly child: ChildProcessWithoutNullStreams, readonly client: AppServerClient,
    private exited: Promise<void>) {}
  static async start(value: CodexProfile, cwd: string, handler?: (request: ServerRequest) => Promise<unknown>) {
    const profile = CodexProfile.parse(value);
    const home = await lstat(profile.home);
    if (!home.isDirectory() || home.isSymbolicLink() || process.platform !== 'win32' && ((home.mode & 0o077) !== 0 || home.uid !== process.getuid?.())) throw Error('Codex home must be an owned private directory');
    const executable = await realpath(profile.executable); await access(executable, constants.X_OK);
    if (/\.(cmd|bat|ps1)$/i.test(executable)) throw Error('Native Codex executable required');
    if (createHash('sha256').update(await readFile(executable)).digest('hex') !== profile.executableSha256) throw Error('Codex executable changed; explicitly refresh the configured runtime');
    const config = [`model_provider="openai"`, `model_catalog_json=${JSON.stringify(profile.catalog)}`,
      'mcp_servers={}', 'cli_auth_credentials_store="file"', 'check_for_update_on_startup=false',
      'analytics.enabled=false', 'otel.exporter="none"', 'features.plugins=false', 'features.apps=false',
      'features.shell_snapshot=false', 'web_search="disabled"'];
    const env: NodeJS.ProcessEnv = { CODEX_HOME: profile.home, CODEX_INTERNAL_APP_SERVER_REMOTE_CONTROL_DISABLED: '1' };
    for (const key of ['PATH', 'HOME', 'USERPROFILE', 'SystemRoot', 'TEMP', 'TMP', 'LANG']) if (process.env[key]) env[key] = process.env[key];
    const child = spawn(executable, ['app-server', '--listen', 'stdio://', ...config.flatMap(value => ['-c', value])],
      { cwd, env, stdio: 'pipe', detached: process.platform !== 'win32' });
    const client = new AppServerClient(child.stdout, child.stdin, { maxFrameBytes: 2 * 1024 * 1024, handleRequest: handler });
    const exited = new Promise<void>(resolve => child.once('close', () => { client.close(); resolve(); }));
    child.once('error', () => client.close(Error('Configured Codex failed to start')));
    child.stderr.resume();
    const core = new CodexProcess(child, client, exited);
    try {
      const initialized = await client.initialize({ clientInfo: { name: 'aoem_asas', title: 'ASAS', version: '0.3.0-dev' },
        capabilities: { experimentalApi: true, requestAttestation: false } });
      if (await realpath(initialized.codexHome) !== await realpath(profile.home)) throw Error('Unexpected Codex home');
      return core;
    } catch (error) { await core.stop(); throw error; }
  }
  stop(): Promise<void> { return this.stopping ??= this.stopOwned(); }
  private async stopOwned() {
    this.client.close(); this.child.stdin.end();
    const wait = async () => {
      let timer: NodeJS.Timeout | undefined;
      try { return await Promise.race([this.exited.then(() => true), new Promise<boolean>(resolve => { timer = setTimeout(() => resolve(false), 1500); })]); }
      finally { clearTimeout(timer); }
    };
    for (const signal of ['SIGTERM', 'SIGKILL'] as const) {
      if (await wait()) return;
      if (this.child.pid) try { if (process.platform === 'win32') this.child.kill(signal); else process.kill(-this.child.pid, signal); }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
    }
    await this.exited;
  }
}
