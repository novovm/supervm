import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { AppServerClient } from '../codex/rpc';
import { dynamicTools, remoteOnlyConfig } from '../codex/engine';
import { Configuration, enginesFor } from '../config';
import { configureCodex } from '../codex/setup';

test('A bootstrap accepts opaque B project IDs, not local B project directories', async () => {
  await assert.rejects(configureCodex({ directory: '/not-created', executable: '/official/codex', loginHome: '/private/login',
    model: 'model', projectIds: ['/B/workspace'] }), /64-hex B project IDs/);
});

test('Codex tool declarations are ACODE-only; server-native tool families are disabled', () => {
  const tools = dynamicTools(); assert.equal(tools.length, 5);
  assert.ok(tools.every(tool => tool.name.startsWith('acode_') && tool.inputSchema.type === 'object'));
  const config = remoteOnlyConfig('/private/catalog.json');
  assert.equal(config.features.shell_tool, false); assert.equal(config.features.apps, false);
  assert.equal(config.features.multi_agent, false); assert.deepEqual(config.mcp_servers, {});
  assert.equal(config.web_search, 'disabled');
});

test('explicit Codex configuration does not silently register a Claude fallback', () => {
  const config = Configuration.parse({ directory: '/private/asas', peers: [{ id: 'B', tokenSha256: 'a'.repeat(64), projects: ['b'.repeat(64)] }],
    codex: { executable: '/runtime/codex', executableSha256: 'c'.repeat(64), home: '/private/codex', catalog: '/private/catalog.json', models: ['selected-native-model'] } });
  assert.deepEqual(Object.keys(enginesFor(config)), ['codex']);
  assert.throws(() => Configuration.parse({ ...config, unknownCredential: 'not-accepted' }));
});

test('reused Codex transport preserves split UTF-8 and fails pending RPC on disconnect', async () => {
  const input = new PassThrough(), output = new PassThrough(), client = new AppServerClient(input, output, { timeoutMs: 1000 });
  output.on('data', chunk => {
    const value = JSON.parse(chunk.toString());
    if (value.method === 'initialize') {
      const response = Buffer.from(JSON.stringify({ id: value.id, result: { codexHome: '/private/中文' } }) + '\n');
      const index = response.indexOf(Buffer.from('中文')) + 1; input.write(response.subarray(0, index)); input.write(response.subarray(index));
    }
  });
  try {
    assert.equal((await client.initialize()).codexHome, '/private/中文');
    const pending = client.request('account/read', { refreshToken: false });
    input.end(); await assert.rejects(pending, /disconnect|closed/);
  } finally { client.close(); input.destroy(); output.destroy(); }
});
