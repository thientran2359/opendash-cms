# Unified JS & Wasm Extension Host (`opendash-server/src/plugin/`)

## Overview

Host-side unified extension runtime supporting both **WebAssembly (`plugin.wasm` on Wasmi `=2.0.0`)** and **zero-build JavaScript (`plugin.js` / Theme `functions.js` on Boa `=0.22.0`, [`js_runtime.rs`](js_runtime.rs))** with Wasmi fuel metering (`INVOCATION_FUEL = 20_000_000`, `16 MiB` linear memory), Boa `RuntimeLimits` (`5_000_000` loops, `128` recursion, `512` stack), `128 MiB` OS worker memory cap, `5s` invocation deadline, isolated `--plugin-worker` subprocess execution over length-prefixed stdin/stdout IPC, `StagedCommitGuard` atomic commit fencing (`OwnerFence` for Plugins & Active Themes), Scoped Custom Routes resolution, and DNS-pinned SSRF-hardened HTTP egress. Exported via [`mod.rs`](mod.rs) alongside `opendash_core::plugin` as `manifest`.

See [Plugin ABI](../../../../../docs/contracts/PLUGIN_ABI.md) and [M6 Plugin Extensions](../../../../../docs/contracts/M6_PLUGIN_EXTENSIONS.md) for the full ABI specification.

## Interface

- **Module Facade** ([`mod.rs`](mod.rs)):
  - Re-exports `[manager.rs:PluginManager]`, `[manager.rs:PluginSummary]`, `[manifest:PluginManifest]`, `[manifest:PluginPermissions]`, `[package.rs:PluginPackage]`, `[package.rs:PluginCheckSummary]`, `[package.rs:check_plugin_path]`, `[runtime.rs:PluginRuntime]`, and `[runtime.rs:PluginError]`.
- **Lifecycle, Actions, Routes & Hook Routing** ([`manager.rs`](manager.rs)):
  - `[manager.rs:PluginManager::install_plugin]`, `[manager.rs:PluginManager::install_plugin_checked]`, `[manager.rs:PluginManager::enable_plugin_cas]`, `[manager.rs:PluginManager::disable_plugin_cas]`, `[manager.rs:PluginManager::list_plugins]`, `[manager.rs:PluginManager::get_plugin]`.
  - `[manager.rs:PluginManager::execute_plugin_action]`, `[manager.rs:PluginManager::execute_owner_action]`, `[manager.rs:PluginManager::run_plugin]`, `[manager.rs:PluginManager::run_plugin_correlated]`, `[manager.rs:PluginManager::run_plugin_with_mode]`, `[manager.rs:PluginManager::run_plugin_dispatch]`, `[manager.rs:PluginManager::resolve_extension_route]`, `[manager.rs:PluginManager::execute_resolved_route]`.
  - `[manager.rs:PluginManager::hook_subscriptions]`, `[manager.rs:PluginManager::content_hook_subscriptions]`, `[manager.rs:PluginManager::filter_subscriptions]`, `[manager.rs:PluginManager::owner_render_filter_subscriptions]`, `[manager.rs:PluginManager::filter_content_payload_for_owner]`, `[manager.rs:PluginManager::filter_public_content_payload_for_owner]`, and `[manager.rs:PluginManager::invalidate_cache]`.
- **Package & Entrypoint Validation** ([`package.rs`](package.rs)):
  - `[package.rs:PluginPackage::from_zip]`, `[package.rs:PluginPackage::from_dir]`, `[package.rs:PluginPackage::from_path]`, `[package.rs:check_plugin_path]`, `[package.rs:PluginCheckSummary]`, and bounds (`MAX_PLUGIN_ZIP_BYTES = 5 MiB`, `MAX_UNCOMPRESSED_TOTAL_BYTES = 10 MiB`, `MAX_WASM_BYTES = 2 MiB`, `MAX_MANIFEST_BYTES = 64 KiB`, `MAX_PLUGIN_FILES = 72`, `MAX_PLUGIN_DIR_DEPTH = 4`).
- **JS Sandbox Runtime** ([`js_runtime.rs`](js_runtime.rs)):
  - `[js_runtime.rs:validate_js_module_syntax]` (static syntax check without executing top-level code), `[js_runtime.rs:execute_js_worker]` (fresh Boa `Context`, immutable `globalThis.od.*` SDK matching `cms/sdk/js/opendash.d.ts`, top-level I/O rejection).
- **Host Runtime & IPC Worker** ([`runtime.rs`](runtime.rs), [`worker.rs`](worker.rs)):
  - `[runtime.rs:PluginRuntime::run_owner_action_with_principal]`, `[runtime.rs:PluginRuntime::run_plugin_action_with_principal]`, `[runtime.rs:execute_host_read_request]`, `[runtime.rs:ActionActorFence]`, `[runtime.rs:ActionExecutionOutput]`, and three independent concurrency semaphores (`MAX_CONCURRENT_INVOCATIONS = 4`, `MAX_BACKGROUND_PUBLIC_INVOCATIONS = 2`, `public_render_semaphore = 4`, `INVOCATION_DEADLINE = 5s`).
  - `[worker.rs:run_worker_from_stdio]`, `[worker.rs:WorkerInvocationRequest]`, `[worker.rs:IPC_STREAM_MAGIC]`, `[worker.rs:read_ipc_frame]`, `[worker.rs:write_ipc_frame]`.
- **Host Operations & Policy** ([`host_ops.rs`](host_ops.rs)):
  - `[host_ops.rs:RuntimeOwner]`, `[host_ops.rs:HostContext]`, `[host_ops.rs:HostOperationRequest]`, `[host_ops.rs:HostReadRequest]`, `[host_ops.rs:HostOperationResponse]`, `[host_ops.rs:WorkerExecutionResult]`, `[host_ops.rs:check_read_policy]`, `[host_ops.rs:handle_host_call]`, `[host_ops.rs:StagedContentCommand]`, `[host_ops.rs:StagedContentDeleteCommand]`, and `[host_ops.rs:StagedPluginLog]`.
- **SSRF-Hardened HTTP Egress** ([`egress.rs`](egress.rs)):
  - `[egress.rs:execute_scoped_http_request]`, `[egress.rs:EgressResponse]`, `[egress.rs:PinnedDnsResolver]`, `[egress.rs:resolve_and_pin_addrs]`, `[egress.rs:verify_resolved_addrs]`, `[egress.rs:validate_outbound_headers]`, `[egress.rs:redact_headers_for_log]`, `[egress.rs:is_forbidden_ip]`, and `[egress.rs:is_forbidden_header_name]`.

## Invariants

- **Strict ABI, JS Syntax & Import Allowlist**: `PluginPackage::finish_from_raw_parts` verifies exported `opendash_run() -> i32` and `memory` (allowing only the 6 `opendash_v1` host functions plus `env:abort`, rejecting WASI) for `plugin.wasm`, or runs static `validate_js_module_syntax` for `plugin.js` (`[package.rs:PluginPackage::from_zip]`, `[js_runtime.rs:validate_js_module_syntax]`).
- **Process Isolation, OS Memory Cap & Zero Late Writes**: Every execution spawns an isolated `--plugin-worker` child process with a `128 MiB` OS memory cap (`operations_worker_process.rs`) and `ProcessGuard` (`Drop` kills/waits child and joins reader thread) (`[runtime.rs:PluginRuntime::run_owner_action_with_principal]`). `StagedCommitGuard` opens `BEGIN IMMEDIATE`, re-verifies `OwnerFence` (`is_plugin_generation_active` / `verify_active_theme_fence_in_tx`) and `ActionActorFence` (`is_actor_role_active`) before committing staged KV/logs, and closes uncommitted connections on drop.
- **Mixed-Scope & Mode Isolation**: `handle_host_call` and `PluginRuntime` enforce `ERR_MIXED_WRITE_SCOPE` (forbidding combining KV writes/deletes with `content_save` or `content_delete`), permit content mutations only in `HookExecutionMode::{Standard, RoutePost}` (with authenticated actor), forbid KV writes in synchronous hooks/filters/`FilterRender`/`RouteGet`, and forbid DB reads/logs (except `PluginSecretGet`) in `FilterContactBeforeSubmit` (`[host_ops.rs:handle_host_call]`, `[host_ops.rs:check_read_policy]`).
- **Quotas & Frame Bounds**: Per-invocation caps enforce `MAX_HOST_CALLS = 100` (`MAX_RENDER_HOST_CALLS = 64` in `FilterRender`/`RouteGet`), `MAX_STAGED_KV_OPS = 64`, `MAX_STAGED_KV_TOTAL_BYTES = 2 MiB`, `MAX_KV_VALUE_BYTES = 512 KiB`, `MAX_STAGED_LOGS = 20`, `MAX_HTTP_CALLS_PER_INVOCATION = 10` (`1` in `FilterContactBeforeSubmit`, `MAX_RENDER_HTTP_CALLS = 2` in `RouteGet` and cached `FilterRender`, `0` in uncached `FilterRender`), `MAX_IPC_FRAME_BYTES = 2 MiB`, and `1 MiB` guest output (`[host_ops.rs:handle_host_call]`, `[egress.rs:MAX_HTTP_CALLS_PER_INVOCATION]`).
- **Safe Mode & Anti-Recursion**: `PluginManager` blocks plugin/theme actions, dispatches, and custom routes while `RecoveryState::is_safe_mode()` is active, prevents recursive action dispatch via `ActionLeaseGuard`, supports `EnvelopeKind::Route` for scoped custom routes, and rejects `EnvelopeKind::Cron` in V1 (`[manager.rs:PluginManager::execute_owner_action]`, `[manager.rs:PluginManager::run_owner_dispatch]`, `[manager.rs:PluginManager::execute_resolved_route]`).
- **SSRF & Loopback Defense**: `execute_scoped_http_request` validates URLs against `PluginManifest::is_http_allowed_with_method`, blocks internal CMS paths (`/api/admin`, `/save`, `/preview`, `/theme`, `/p/`, `/api/p/`, `/api/public/p/`), pins DNS resolution against loopback/private/CGNAT/link-local/NAT64 IPs (unless `allow_loopback` for tests), disables redirects/proxies, strips forbidden headers, and streams responses up to `1 MiB` (`[egress.rs:execute_scoped_http_request]`, `[host_ops.rs:handle_host_call]`).

## Side Effects

- Spawns `--plugin-worker` OS subprocesses and OS reader threads (`[runtime.rs:PluginRuntime::run_plugin_action_with_principal]`).
- Reads and writes plugin revision blobs on disk under `plugins_root` and reads encrypted plugin secrets from `secrets_root` (`[manager.rs:PluginManager::install_plugin_checked]`, `[runtime.rs:execute_host_read_request]`).
- Executes SQLite reads and `BEGIN IMMEDIATE` staged commits (`plugin_kv`, `plugin_logs`, `content_entries`) and records execution telemetry (`[runtime.rs:PluginRuntime::run_plugin_action_with_principal]`, `[manager.rs:PluginManager::execute_plugin_action]`).
- Performs outbound blocking HTTP/HTTPS requests to manifest-allowlisted hosts (`[egress.rs:execute_scoped_http_request]`).

## Verification

Run the plugin runtime, package, egress, and regression test suites from the repository root:

```powershell
cargo test --manifest-path cms/Cargo.toml -p opendash-server --locked plugin::
```
