# Sample Transformer Plugin (`sample-plugin`)

## Overview

Legacy v1 AssemblyScript transformer plugin example (`id: "sample-plugin"`, `version: "1.0.0"`, `api_version: 1`) built on the [AssemblyScript SDK](../../../sdk/README.md) (`cms/sdk/as/opendash.ts`) and [Plugin ABI](../../../../docs/contracts/PLUGIN_ABI.md). It transforms an input `text` payload to uppercase, persists an invocation counter in plugin-scoped SQLite KV, and packages a ready-to-upload `sample-plugin.zip` consumed by `cms/scripts/build.ps1`, [demo-website-runner.py](../../../scripts/demo-website-runner.py), and `cms/crates/opendash-server/tests/plugin_extensibility_test.rs`.

Package layout:
- [package.json](package.json): Pinned AssemblyScript toolchain (`assemblyscript = 0.27.34`, `assemblyscript-json = 1.1.0`) and `asbuild` / `build` scripts.
- [asconfig.json](asconfig.json): Release target emitting `build/plugin.wasm` with `bindings: "raw"` and `exportMemory: true`.
- [manifest.json](manifest.json): v1 plugin manifest (no `manifest_schema`, `declarations`, `hooks`, or `actions`).
- `assembly/`: AssemblyScript guest implementation in [assembly/index.ts](assembly/index.ts).
- `scripts/`: Pure-Node ZIP packager in [scripts/bundle.js](scripts/bundle.js).
- `build/` and `sample-plugin.zip`: Compiled WebAssembly artifact and packaged ZIP archive.

## Interface

- **Guest Export:** `opendash_run(): i32` (`[cms/examples/plugins/sample-plugin/assembly/index.ts:opendash_run]`):
  - Calls `opendash.getInput()`, extracts optional string field `"text"` (default `"Hello from OpenDash Plugin"`), increments KV key `"run_count"` via `opendash.kvGet` / `opendash.kvSet`, and writes `{"status":"ok","transformed":"<UPPERCASE>","run_count":<n>}` via `opendash.setOutput` (`0` on success, `1` if `setOutput` returns `< 0`).
- **Manifest Permissions ([manifest.json](manifest.json)):**
  - `sqlite`: `["kv_read", "kv_write"]`.
  - `http`: `["http://127.0.0.1:18766/api/health", "http://localhost:18766/api/health"]`.
- **Build & Bundle Entrypoints:**
  - `npm run asbuild`: compiles `assembly/index.ts` -> `build/plugin.wasm`.
  - `npm run build`: runs `asbuild` then `[cms/examples/plugins/sample-plugin/scripts/bundle.js:createZip]` to generate `sample-plugin.zip` containing `manifest.json` and `plugin.wasm`.
- **v2/C4 Upgrade Path:** For declarative content types, taxonomies, `content/before_save` allow/veto hooks, `content/after_save`, and `content_save` actions, use `opendash.readEnvelope()` and the typed output helpers documented in [AssemblyScript SDK](../../../sdk/README.md) and [Content Foundation Contract](../../../../docs/contracts/CONTENT_FOUNDATION_2026_11.md).

## Invariants

- **v1 Backward Compatibility:** Omitting `manifest_schema`, `declarations`, `hooks`, `filters`, and `actions` preserves legacy manual transformer execution (`POST /api/admin/plugins/sample-plugin/run`).
- **Deterministic Packaging:** `scripts/bundle.js` (`createZip`) fails fast if `build/plugin.wasm` is missing and archives only `manifest.json` and `plugin.wasm` at the ZIP root with DEFLATE and CRC-32 checksums (`[cms/examples/plugins/sample-plugin/scripts/bundle.js:crc32]`).
- **Pinned Compiler Dependencies:** `assemblyscript = 0.27.34` is exact-pinned in [package.json](package.json) to guarantee compatible raw WASM memory/ABI bindings with `cms/sdk/as/opendash.ts`.

## Side Effects

- **Runtime State:** Executing the plugin mutates plugin-scoped SQLite KV key `"run_count"`.
- **Build Outputs:** Running `npm run build` installs/uses `node_modules/` and overwrites `build/plugin.wasm` and `sample-plugin.zip`.

## Verification

Install dependencies, compile WASM, and bundle `sample-plugin.zip`:

```bash
npm --prefix cms/examples/plugins/sample-plugin ci
npm --prefix cms/examples/plugins/sample-plugin run build
```

Validate the archive offline via `[cms/crates/opendash-server/src/plugin/package.rs:check_plugin_path]` (also verified in `cms/crates/opendash-server/tests/plugin_extensibility_test.rs`):

```bash
./cms/target/release/opendash --check-plugin cms/examples/plugins/sample-plugin/sample-plugin.zip
```
