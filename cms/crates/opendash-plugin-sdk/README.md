# OpenDash Plugin SDK (`opendash-plugin-sdk`)

## Overview

Guest-side Rust SDK (`Cargo.toml` dependencies: `serde`, `serde_json` only) for authoring WebAssembly plugins targeting OpenDash's `opendash_v1` host ABI (ABI v1 / v1.1). Provides the `export_plugin!` entrypoint macro, typed host operation wrappers (`kv_*`, `content_*`, `taxonomy_list`, `media_get`, `site_options_get`, `plugin_settings_get`, `plugin_secret_get`, `http_request`, `log`), wire envelope/decision DTOs, and a thread-local `MockHost` harness (`#[cfg(not(target_arch = "wasm32"))]`) for native unit testing without a Wasm runtime.

See [Plugin ABI](../../../docs/contracts/PLUGIN_ABI.md) and [M6 Plugin Extensions](../../../docs/contracts/M6_PLUGIN_EXTENSIONS.md) for host-side enforcement details.

## Interface

- **Entrypoint Macro & Crate Re-exports** ([`src/lib.rs`](src/lib.rs)):
  - `[lib.rs:export_plugin]` macro: generates `#[unsafe(no_mangle)] pub extern "C" fn opendash_run() -> i32` on `wasm32` targets, reads `Envelope`, invokes the handler `fn(Envelope) -> Result<(), SdkError>`, logs errors on failure, and returns `0` / `1`.
  - Re-exports all functions from `host`, all public structs/enums from `types`, `serde_json`, and (`#[cfg(not(target_arch = "wasm32"))]`) `mock::{MockHost, MockHostOutcome, run_in_mock_host}`.
- **Low-Level `opendash_v1` ABI Transport** ([`src/abi.rs`](src/abi.rs)):
  - Constants `[abi.rs:INITIAL_RESP_BUF_CAP]` (`64 KiB`) and `[abi.rs:MAX_IPC_FRAME_BYTES]` (`2 MiB`).
  - `[abi.rs:read_input_bytes]`, `[abi.rs:write_output_bytes]` (1 MiB cap), and `[abi.rs:host_call_raw]` (links `host_get_input_len`, `host_read_input`, `host_write_output`, `host_call`, `host_last_resp_len`, `host_read_last_resp` from `opendash_v1` on `wasm32`, or delegates to `mock` on native targets).
- **Typed Host Operations** ([`src/host.rs`](src/host.rs)):
  - Envelope & output: `[host.rs:read_envelope]`, `[host.rs:write_output]`.
  - KV store: `[host.rs:kv_get]`, `[host.rs:kv_set]`, `[host.rs:kv_delete]`, `[host.rs:kv_list]`.
  - Content & site reads/mutations: `[host.rs:content_get]`, `[host.rs:content_list]`, `[host.rs:content_save]`, `[host.rs:content_delete]`, `[host.rs:taxonomy_list]`, `[host.rs:media_get]`, `[host.rs:site_options_get]`, `[host.rs:plugin_settings_get]`, `[host.rs:plugin_secret_get]`.
  - Egress & diagnostics: `[host.rs:http_request]`, `[host.rs:log]`.
- **Wire Types & Filter Decisions** ([`src/types.rs`](src/types.rs)):
  - `[types.rs:SdkError]`, `[types.rs:Envelope]` (supports v1 defaults and v2 fields), `[types.rs:HostResponse]` (supports legacy v1 fields and Spec 2 `data`), `[types.rs:KvItem]`, `[types.rs:HttpRequestInput]`, `[types.rs:HttpResponse]`.
  - `[types.rs:ContentSaveInput]`, `[types.rs:ContentEntry]`, `[types.rs:ContentListResult]`, `[types.rs:TaxonomyTerm]`, `[types.rs:MediaAsset]`, `[types.rs:LogLevel]`.
  - Filter/output types: `[types.rs:BeforeSaveDecision]`, `[types.rs:ContentPrepareSavePatch]`, `[types.rs:ContactBeforeSubmitDecision]`, `[types.rs:SeoHeadOutput]`, and `[types.rs:EntryViewOutput]`.
- **Native Test Harness** ([`src/mock.rs`](src/mock.rs)):
  - `[mock.rs:MockHost]`, `[mock.rs:MockHttpHandler]`, `[mock.rs:MockHostOutcome]`, and `[mock.rs:run_in_mock_host]` (guarded by `#[cfg(not(target_arch = "wasm32"))]`).

## Invariants

- **Frame & Payload Size Caps**: `read_input_bytes` and `host_call_raw` reject payloads exceeding `MAX_IPC_FRAME_BYTES` (`2 MiB`), and `write_output_bytes` rejects outputs exceeding `1 MiB` (`[abi.rs:read_input_bytes]`, `[abi.rs:write_output_bytes]`, `[abi.rs:host_call_raw]`).
- **Single-Execution Oversized Readback (`-2`)**: When `host_call` returns `-2` (`BUFFER_TOO_SMALL`), `host_call_raw` fetches the buffered response via `host_last_resp_len` and `host_read_last_resp` without re-executing the host operation (`[abi.rs:host_call_raw]`).
- **Dual ABI Compatibility**: `Envelope` defaults `envelope_version = 1`, `kind = "hook"`, and `principal = "system"` when omitted; `HostResponse`, `kv_get`, and `http_request` accept both legacy top-level v1 fields and Spec 2 `data` envelopes (`[types.rs:Envelope]`, `[types.rs:HostResponse]`, `[host.rs:kv_get]`, `[host.rs:http_request]`).
- **Mock CAS & Panic Cleanup**: `MockHost` enforces `expected_version` optimistic concurrency on `content_save` (updates) and `content_delete`, and `MockResetGuard` restores the previous thread-local `ACTIVE_MOCK` state on drop even if guest code panics (`[mock.rs:MockHost::dispatch_call]`, `[mock.rs:run_in_mock_host]`).

## Side Effects

- On `wasm32` targets, invokes `opendash_v1` external host imports (`host_get_input_len`, `host_read_input`, `host_write_output`, `host_call`, `host_last_resp_len`, `host_read_last_resp`) inside `[abi.rs:read_input_bytes]`, `[abi.rs:write_output_bytes]`, and `[abi.rs:host_call_raw]`.
- On native targets, `[mock.rs:run_in_mock_host]` mutates thread-local `ACTIVE_MOCK` state during execution and resets it via `Drop`.

## Verification

Run the SDK unit tests (covers all 15 host operations, `-2` readback, v1/v2 envelopes, CAS conflicts, and panic cleanup) from the repository root:

```powershell
cargo test --manifest-path cms/Cargo.toml -p opendash-plugin-sdk --locked
```
