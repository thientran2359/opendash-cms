# AssemblyScript Plugin SDK (`cms/sdk/`)

## Overview

`cms/sdk/` contains the official AssemblyScript guest SDK for OpenDash CMS plugins targeting the Wasmi host ABI v1.1 (`opendash_v1`), `PluginEnvelopeV2`, and all 15 Host Ops defined in [PLUGIN_ABI.md](../../docs/contracts/PLUGIN_ABI.md). All v1 helpers (`getInput`, `setOutput`, `hostCall`, `kvGet`, `kvSet`, `httpRequest`, `escapeJsonString`, `stringifyValue`) remain backwards-compatible so legacy v1 plugins such as [`sample-plugin`](../examples/plugins/sample-plugin/README.md) compile and run unchanged.

`cms/sdk/` does not have its own `package.json`; AssemblyScript (`assemblyscript = "=0.27.34"`) and `assemblyscript-json` (`1.1.0`) are pinned in [`../examples/plugins/sample-plugin/package.json`](../examples/plugins/sample-plugin/package.json). (Rust-authored plugins use the sibling workspace crate [`opendash-plugin-sdk`](../crates/opendash-plugin-sdk/src/abi.rs) implementing the same `opendash_v1` wire protocol.)

## Interface

### Files

- [`as/opendash.ts`](as/opendash.ts) — AssemblyScript SDK module (`opendash_v1` imports, `Envelope`/`readEnvelope`, 15 Host Op wrappers, and typed output helpers).
- [`as/test_harness.ts`](as/test_harness.ts) — Guest WASM test harness exercising v1/v2 envelopes, `-2` oversized readback, all 15 Host Ops, and all typed output helpers (`opendash_run`).
- [`as/test_sdk_v11.mjs`](as/test_sdk_v11.mjs) — Node.js WebAssembly mock-host test runner (`runHarness`) that verifies `cms/target/opendash_sdk_test.wasm`.

### Host Imports (`"opendash_v1"`) & Buffer Constants

Declared in [`as/opendash.ts`](as/opendash.ts) and bound by [`runtime.rs`](../crates/opendash-server/src/plugin/runtime.rs):

- `host_get_input_len(): i32`
- `host_read_input(destPtr: usize, maxLen: i32): i32`
- `host_write_output(srcPtr: usize, len: i32): i32`
- `host_call(opPtr: usize, opLen: i32, respPtr: usize, maxRespLen: i32): i32`
- `host_last_resp_len(): i32` (ABI v1.1)
- `host_read_last_resp(destPtr: usize, maxLen: i32): i32` (ABI v1.1)
- Constants: `INITIAL_RESP_BUF_CAP = 65536` (64 KiB), `MAX_IPC_FRAME_BYTES = 2097152` (2 MiB), `BUFFER_TOO_SMALL = -2`.

### Envelope, Host Ops & Output Helpers (`as/opendash.ts`)

- **Core I/O & JSON:** `getInput()`, `setOutput(jsonStr)`, `hostCall(opJson)`, `escapeJsonString(str)`, `stringifyValue(val)`.
- **Unified Input:** `readEnvelope(): Envelope | null` normalizes both `HookEnvelope` v1 (defaulting `envelopeVersion = 1`, `kind = "hook"`, `principal = "system"`, `dispatchDepth = 0`) and `PluginEnvelopeV2` (`envelopeVersion`, `kind`, `event`, `occurredAtMs`, `correlationId`, `locale`, `principal`, `dispatchDepth`, `payload`, `raw`).
- **15 Host Ops** (matching [`host_ops.rs`](../crates/opendash-server/src/plugin/host_ops.rs) and supporting both v1 top-level and `{"ok":true,"data":{...}}` responses):
  - **KV Store:** `kvGet(key)`, `kvSet(key, valueJson)`, `kvDelete(key)`, `kvList(prefix = null, limit = 50)`.
  - **HTTP Egress:** `httpRequest(method, url, headersJson = "{}", body = "")`.
  - **Content & Taxonomy:** `contentGet(typeKey, entryId, locale = null)`, `contentList(typeKey, locale = null, limit = 20, offset = 0, publishedOnly = false)`, `contentSave(opObj)`, `contentDelete(typeKey, entryId, expectedVersion)`, `taxonomyList(kind, locale = null, limit = 50)`.
  - **Media & Settings:** `mediaGet(mediaId)`, `siteOptionsGet(locale = null)`, `pluginSettingsGet()`, `pluginSecretGet(key)`.
  - **Diagnostics:** `log(level, message)`.
- **Typed Output Helpers:** `outputAllow()`, `outputVeto(code, message, fieldErrorsObj = null)`, `outputPrepareSavePatch(patchObj)`, `outputContactAllow()`, `outputContactReject(code, message)`, `outputSeoHead(seoObj)`, `outputEntryView(extObj)`.

Guest modules must export `opendash_run(): i32` (returning `0` on success, non-zero on error).

## Invariants

- **Single-invocation `-2` readback:** When `host_call()` returns `BUFFER_TOO_SMALL` (`-2`), [`as/opendash.ts:hostCall`](as/opendash.ts) reads `host_last_resp_len()`, allocates an exact `ArrayBuffer(fullLen)` (bounded by `MAX_IPC_FRAME_BYTES = 2 MiB`), and calls `host_read_last_resp()` without re-invoking `host_call()` — consuming only one `host_calls_count` quota unit.
- **RFC 8259 JSON safety:** All SDK request builders and output helpers serialize via `JSON.Obj` and `stringifyValue()` / `escapeJsonString()` rather than raw string concatenation.
- **Host permission & mode enforcement:** Capabilities (`permissions.*`, `plugin_settings_read`, `plugin_secrets_read`), `kv_list`/`content_save`/`content_delete` execution-mode restrictions, CAS `expected_version` checks, and DNS-pinned HTTP allowlists are enforced by the host in [`host_ops.rs`](../crates/opendash-server/src/plugin/host_ops.rs) and [`CONTENT_FOUNDATION_2026_11.md`](../../docs/contracts/CONTENT_FOUNDATION_2026_11.md).

## Side Effects

- `cms/sdk/` contains pure guest library and test harness files; importing [`as/opendash.ts`](as/opendash.ts) has no side effects at compile time.
- At runtime inside a Wasmi worker, Host Ops stage KV writes, logs (max 20 per invocation), or content commands (`content_save` / `content_delete` in allowed action modes), or perform bounded HTTP egress through the host sandbox.

## Verification

Install the pinned AssemblyScript toolchain from `sample-plugin`, compile `test_harness.ts`, and execute [`as/test_sdk_v11.mjs`](as/test_sdk_v11.mjs):

```bash
npm --prefix cms/examples/plugins/sample-plugin ci
npm --prefix cms/examples/plugins/sample-plugin run asbuild
./cms/examples/plugins/sample-plugin/node_modules/.bin/asc \
  cms/sdk/as/test_harness.ts \
  --outFile cms/target/opendash_sdk_test.wasm \
  --bindings raw --optimize \
  --lib cms/examples/plugins/sample-plugin/node_modules
node cms/sdk/as/test_sdk_v11.mjs
```
