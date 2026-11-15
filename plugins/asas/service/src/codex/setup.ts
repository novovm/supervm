import { mkdir, readFile, realpath, lstat, symlink } from 'node:fs/promises';
import { createHash, randomBytes } from 'node:crypto';
import path from 'node:path';
import { saveJson } from '../storage';
import { CodexProcess, type CodexProfile } from './process';

export async function configureCodex(options: { directory: string; executable: string; loginHome: string; model: string; projectIds: string[] }) {
  if (!options.projectIds.length || options.projectIds.length > 100 || options.projectIds.some(id => !/^[a-f0-9]{64}$/.test(id)))
    throw Error('Explicit 64-hex B project IDs required; A never resolves B workspace paths');
  for (const file of [options.directory, options.executable, options.loginHome]) if (!path.isAbsolute(file)) throw Error('Explicit absolute A paths required');
  await mkdir(options.directory, { recursive: true, mode: 0o700 });
  const root = await realpath(options.directory), source = await realpath(path.join(options.loginHome, 'auth.json'));
  const owner = await lstat(root);
  if (!owner.isDirectory() || process.platform !== 'win32' && ((owner.mode & 0o077) !== 0 || owner.uid !== process.getuid?.())) throw Error('ASAS setup requires an owned private directory');
  const stat = await lstat(source);
  if (!stat.isFile() || process.platform !== 'win32' && ((stat.mode & 0o077) !== 0 || stat.uid !== process.getuid?.())) throw Error('Existing Codex login must be an owned private file');
  const home = path.join(root, 'codex'); await mkdir(home, { mode: 0o700, recursive: true });
  if (await realpath(home) === await realpath(options.loginHome)) throw Error('ASAS and the original Codex home must remain separate');
  const destination = path.join(home, 'auth.json');
  try { if (await realpath(destination) !== source) throw Error('ASAS already references a different login; do not replace it implicitly'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; await symlink(source, destination, 'file'); }
  const cache = await readFile(path.join(options.loginHome, 'models_cache.json'));
  if (cache.length > 8 * 1024 * 1024) throw Error('Native model catalog exceeds limit');
  const metadata = JSON.parse(cache.toString('utf8')).models?.find((model: { slug: string }) => model.slug === options.model);
  if (!metadata) throw Error('Selected model not in this Codex account catalog; no model substituted');
  const catalog = path.join(home, 'asas-catalog.json');
  await saveJson(catalog, { models: [{ ...metadata, shell_type: 'disabled', apply_patch_tool_type: null,
    experimental_supported_tools: [], supports_search_tool: false, tool_mode: 'direct',
    include_skills_usage_instructions: false, include_plugin_usage_instructions: false, include_apps_usage_instructions: false }] });
  const executable = await realpath(options.executable);
  const profile: CodexProfile = { executable, executableSha256: createHash('sha256').update(await readFile(executable)).digest('hex'),
    home, catalog, models: [options.model], effort: 'medium' };
  const cwd = path.join(root, 'check'); await mkdir(cwd, { recursive: true, mode: 0o700 });
  const core = await CodexProcess.start(profile, cwd);
  try {
    const account = await core.client.request<any>('account/read', { refreshToken: false });
    if (!account.account) throw Error('Existing Codex login is not available to the selected official runtime');
    const listed = await core.client.request<any>('model/list', { includeHidden: false });
    if (!listed.data?.some((row: any) => (row.model ?? row.id) === options.model)) throw Error('Model unavailable in native Codex discovery');
  } finally { await core.stop(); }
  const projects = [...new Set(options.projectIds)];
  const configFile = path.join(root, 'service.json');
  try { await lstat(configFile); throw Error('ASAS service configuration already exists; inspect it rather than replacing grants'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  await saveJson(configFile, { directory: path.join(root, 'tasks'), models: [options.model], codex: profile,
    peers: [{ id: 'local-acode', tokenSha256: createHash('sha256').update(randomBytes(32)).digest('hex'), projects }] });
  const entry = path.resolve(__dirname, '..', 'main.js'), hostFile = path.join(root, 'host.json');
  await saveJson(hostFile, { version: 1, node: process.execPath, entry, entry_sha256: createHash('sha256').update(await readFile(entry)).digest('hex'),
    service_config: configFile, shutdown_ms: 10000 });
  return { configFile, hostFile, engine: 'codex', model: options.model, loggedIn: true, inferenceVerified: false, loginCopied: false };
}

if (require.main === module) {
  const main = async () => {
    const args = process.argv.slice(2), values = new Map<string, string>(), projectIds: string[] = [];
    for (let index = 0; index < args.length; index += 2) {
      const name = args[index], value = args[index + 1];
      if (!['--directory', '--executable', '--login-home', '--model', '--project-id'].includes(name) || !value || value.startsWith('--')) throw Error('Invalid setup arguments');
      if (name === '--project-id') projectIds.push(value);
      else { if (values.has(name)) throw Error('Duplicate setup argument'); values.set(name, value); }
    }
    const get = (name: string) => { const value = values.get(name); if (!value) throw Error('Missing setup argument'); return value; };
    return configureCodex({ directory: get('--directory'), executable: get('--executable'), loginHome: get('--login-home'), model: get('--model'), projectIds });
  };
  void main()
    .then(result => console.log(JSON.stringify(result)), () => { console.error('Codex configuration failed; no login browser or model fallback was used'); process.exitCode = 1; });
}
