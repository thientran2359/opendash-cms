#!/usr/bin/env node

// Re-index the project decisions/contracts in CodeGraph's local docs store.
// Run from any directory: node cms/scripts/codegraph-index-docs.mjs
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const npmRoot = process.env.APPDATA
  ? join(process.env.APPDATA, 'npm', 'node_modules')
  : join(homedir(), '.npm-global', 'lib', 'node_modules');
const engine = process.env.CODEGRAPH_ENGINE_PATH || join(
  npmRoot,
  '@astudioplus',
  'codegraph-mcp',
  'bin',
  process.platform === 'win32' ? 'codegraph-server-win32-x64.exe' : 'codegraph-server-linux-x64',
);

// Only official documents belong in the canonical CodeGraph docs index.
// Do not index .agents/ drafts, historical proposal threads or retired skills.
const sources = [
  'docs/product/PRODUCT_REQUIREMENTS.md',
  'docs/architecture/TECH_STACK.md',
  'docs/architecture/CMS_ARCHITECTURE.md',
  'docs/contracts/CORE_FOUNDATION.md',
  'docs/contracts/PLUGIN_ABI.md',
  'docs/decisions/ADR-0002-core-extension-contract.md',
  'docs/verification/2026-11-07-m1-m2-import.md',
  'docs/plans/ROADMAP_V1.md',
  'docs/plans/MODULE_CATALOG_V1.md',
  'docs/plans/M3_STATUS_2026-11-09.md',
  'docs/verification/2026-11-08-m3-1-content-domain.md',
  'docs/verification/2026-11-08-m3-2-content-types-backend.md',
  'docs/verification/2026-11-08-m3-3-content-entry-crud.md',
  'docs/verification/2026-11-08-m3-4-admin-content-ui.md',
  'docs/verification/2026-11-09-m3-5-media-library.md',
  'docs/verification/2026-11-09-m3-6-public-content-rendering.md',
];

// Safe verification, with no installed CodeGraph engine required:
// node cms/scripts/codegraph-index-docs.mjs --list-sources
if (process.argv.includes('--list-sources')) {
  let missing = false;
  for (const source of sources) {
    const exists = existsSync(join(repo, ...source.split('/')));
    console.log(`${exists ? 'OK' : 'MISSING'} ${source}`);
    if (!exists) missing = true;
  }
  process.exit(missing ? 1 : 0);
}

if (!existsSync(engine)) {
  console.error('Không tìm thấy CodeGraph engine. Cài @astudioplus/codegraph-mcp@0.21.0 hoặc đặt CODEGRAPH_ENGINE_PATH.');
  process.exit(1);
}

for (const source of sources) {
  const file = join(repo, ...source.split('/'));
  if (!existsSync(file)) {
    console.error(`Thiếu file: ${source}`);
    process.exitCode = 1;
    continue;
  }

  const result = spawnSync(engine, [
    '--workspace', repo,
    '--embedding-model', 'static',
    '--exclude', 'database', '--exclude', 'uploads',
    '--run-tool', 'codegraph_index_markdown',
    '--tool-args', JSON.stringify({ path: file }),
  ], {
    cwd: repo,
    encoding: 'utf8',
    env: { ...process.env, CODEGRAPH_TELEMETRY: 'off' },
    windowsHide: true,
    timeout: 45_000,
    maxBuffer: 8 * 1024 * 1024,
  });

  if (result.error || result.status !== 0) {
    console.error(`CodeGraph thất bại với ${source}: ${result.error?.message || result.stderr?.trim() || `exit ${result.status}`}`);
    process.exitCode = 1;
    continue;
  }

  const indexed = JSON.parse(result.stdout);
  console.log(`${source}: ${indexed.chunks?.length ?? 0} đoạn đã nạp`);
}
