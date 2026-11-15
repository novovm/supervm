import { mkdir, open, rename, rm } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

export async function saveJson(file: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(file), { recursive: true, mode: 0o700 });
  const temporary = file + '.' + randomUUID();
  try {
    const handle = await open(temporary, 'wx', 0o600);
    try { await handle.writeFile(JSON.stringify(value)); await handle.sync(); } finally { await handle.close(); }
    await rename(temporary, file);
  } finally { await rm(temporary, { force: true }); }
  const directory = await open(path.dirname(file), 'r');
  try { await directory.sync(); } catch (error) { if (!['EINVAL', 'EPERM', 'EISDIR'].includes((error as NodeJS.ErrnoException).code || '')) throw error; }
  finally { await directory.close(); }
}
