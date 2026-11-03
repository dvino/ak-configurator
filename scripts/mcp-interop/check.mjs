#!/usr/bin/env node
// Development-only independent interoperability: no HID or GUI startup.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

assert.equal(process.argv.length, 4, 'usage: check.mjs --binary <mock-binary>');
assert.equal(process.argv[2], '--binary');
const binary = resolve(process.argv[3]);
const expected = ['ak820_devices', 'ak820_session', 'ak820_read', 'ak820_tests', 'ak820_trace', 'ak820_operation'].sort();
class RecordingTransport extends StdioClientTransport {
  setProtocolVersion(version) { this.negotiatedVersion = version; }
}
// The SDK deliberately filters inherited environment; forward the caller's
// dynamic-library path only when one was explicitly supplied for the build.
const libraryEnv = process.env.LD_LIBRARY_PATH ? { LD_LIBRARY_PATH: process.env.LD_LIBRARY_PATH } : {};
const transport = new RecordingTransport({ command: binary, args: ['--mcp-stdio', '--mcp-backend=mock'], env: libraryEnv, stderr: 'pipe' });
const client = new Client({ name: 'ak820-independent-check', version: '1.0.0' }, { capabilities: {} });
const opts = { timeout: 10000 };
let stderr = '';
transport.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-8192); });
try {
  await client.connect(transport, opts);
  assert.ok(transport.negotiatedVersion);
  const tools = (await client.listTools({}, opts)).tools;
  assert.deepEqual(tools.map(t => t.name).sort(), expected);
  assert.ok(tools.every(t => t.inputSchema && t.outputSchema));
  async function call(name, args) {
    const result = await client.callTool({ name, arguments: args }, undefined, opts);
    assert.ok(!result.isError, JSON.stringify(result));
    const value = result.structuredContent;
    assert.equal(value.backend, 'mock');
    assert.equal(value.evidence, 'host_model');
    assert.equal(value.verification.setters_sent, 0);
    return value;
  }
  const inventory = await call('ak820_devices', { refresh: true });
  assert.equal(inventory.data.devices[0].device_id, 'mock-1');
  const opened = await call('ak820_session', { action: 'open', device_id: 'mock-1', load: 'diagnostics' });
  const operation = opened.data.operation_id;
  const deadline = Date.now() + 5000;
  let state;
  do {
    state = (await call('ak820_operation', { action: 'status', operation_id: operation })).data;
    if (state.state === 'running') await new Promise(resolve => setTimeout(resolve, 10));
  } while (state.state === 'running' && Date.now() < deadline);
  assert.equal(state.state, 'completed');
  const session = state.result.device_session_id;
  const read = await call('ak820_read', { device_session_id: session, section: 'identity' });
  assert.equal(read.image.status, 'synthetic');
  await call('ak820_session', { action: 'close', device_session_id: session });
  console.log(`SDK handshake, schemas, fixture identity and status: PASS (protocol ${transport.negotiatedVersion})`);
} catch (error) {
  if (stderr) console.error(`Server stderr: ${stderr}`);
  throw error;
} finally {
  await client.close();
}
const here = dirname(fileURLToPath(import.meta.url));
const inspector = join(here, 'node_modules', '@modelcontextprotocol', 'inspector', 'clients', 'launcher', 'build', 'index.js');
const inspectorEnv = process.env.LD_LIBRARY_PATH ? ['-e', `LD_LIBRARY_PATH=${process.env.LD_LIBRARY_PATH}`] : [];
const output = execFileSync(process.execPath, [inspector, '--cli', '--method', 'tools/list', ...inspectorEnv, binary, '--mcp-stdio', '--mcp-backend=mock'], { encoding: 'utf8', timeout: 15000, maxBuffer: 4 * 1024 * 1024 });
const listed = JSON.parse(output);
assert.deepEqual(listed.tools.map(t => t.name).sort(), expected);
assert.ok(listed.tools.every(t => t.inputSchema && t.outputSchema));
console.log('Inspector CLI discovery/tools-list: PASS');
