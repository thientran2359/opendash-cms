#!/usr/bin/env node

// Query CodeGraph from terminal-based agents (including Chat On Steroids Core).
// Examples:
//   node cms/scripts/codegraph-query.mjs symbol HookDispatcher
//   node cms/scripts/codegraph-query.mjs callgraph cms/crates/opendash-server/src/plugin/host_ops.rs 94
//   node cms/scripts/codegraph-query.mjs docs "plugin ABI"
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const npmRoot = process.env.APPDATA
  ? join(process.env.APPDATA, 'npm', 'node_modules')
  : join(homedir(), '.npm-global', 'lib', 'node_modules');
const engine = process.env.CODEGRAPH_ENGINE_PATH || join(
  npmRoot, '@astudioplus', 'codegraph-mcp', 'bin',
  process.platform === 'win32' ? 'codegraph-server-win32-x64.exe' : 'codegraph-server-linux-x64',
);

const [mode, ...args] = process.argv.slice(2);
const fileUri = (filename) => pathToFileURL(isAbsolute(filename) ? filename : join(repo, filename)).href;
let tool;
let params;

switch (mode) {
  case 'symbol':
    tool = 'codegraph_symbol_search';
    params = { query: args.join(' '), limit: 10 };
    break;
  case 'callgraph':
  case 'callers':
  case 'callees': {
    const line = Number(args[1]);
    if (!args[0] || !Number.isInteger(line) || line < 1) break;
    tool = {
      callgraph: 'codegraph_get_call_graph',
      callers: 'codegraph_get_callers',
      callees: 'codegraph_get_callees',
    }[mode];
    params = { uri: fileUri(args[0]), line, depth: 2 };
    break;
  }
  case 'docs':
    tool = 'codegraph_search_docs';
    params = { query: args.join(' ') };
    break;
  case 'entrypoints':
    tool = 'codegraph_find_entry_points';
    params = {};
    break;
  default:
    break;
}

if (!tool || (['symbol', 'docs'].includes(mode) && !params.query?.trim())) {
  console.error('Usage: codegraph-query.mjs symbol <query> | callgraph|callers|callees <file> <line> | docs <query> | entrypoints');
  process.exit(2);
}
if (!existsSync(engine)) {
  console.error('Không tìm thấy CodeGraph engine. Cài @astudioplus/codegraph-mcp@0.21.0 hoặc đặt CODEGRAPH_ENGINE_PATH.');
  process.exit(1);
}

const result = spawnSync(engine, [
  '--workspace', repo,
  '--embedding-model', 'static',
  '--exclude', 'database', '--exclude', 'uploads',
  '--run-tool', tool,
  '--tool-args', JSON.stringify(params),
], {
  cwd: repo,
  encoding: 'utf8',
  env: { ...process.env, CODEGRAPH_TELEMETRY: 'off' },
  windowsHide: true,
  timeout: 45_000,
  maxBuffer: 8 * 1024 * 1024,
});

if (result.error || result.status !== 0) {
  console.error(result.error?.message || result.stderr?.trim() || `CodeGraph exited ${result.status}`);
  process.exit(result.status || 1);
}
process.stdout.write(result.stdout);
