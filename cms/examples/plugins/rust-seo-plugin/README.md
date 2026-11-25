# Rust SEO Plugin Example (`rust-seo-plugin`)

## Overview

Standalone Rust WebAssembly plugin example (`id: "rust-seo-plugin"`, `version: "0.1.0"`, `api_version: 1`, `manifest_schema: 3`) built with [`opendash-plugin-sdk`](../../../crates/opendash-plugin-sdk/src/lib.rs) (ABI v1.1, `Envelope` v2, 15 host ops, and `MockHost`). It conforms to [Plugin ABI](../../../../docs/contracts/PLUGIN_ABI.md) and demonstrates write-time `entry.ext["rust-seo-plugin"]` view projection consumed by [sample-theme-v3](../../themes/sample-theme-v3/README.md).

Directory layout:
- [Cargo.toml](Cargo.toml): Standalone `[workspace]` crate (`crate-type = ["cdylib", "rlib"]`, `edition = "2024"`) targeting `wasm32-unknown-unknown` without modifying the root `cms/Cargo.toml` workspace.
- [manifest.json](manifest.json): `manifest_schema: 3` plugin manifest declaring hooks, filters, scoped permissions, network allowlist, and `settings_schema`.
- `src/`: Rust source and `MockHost` unit tests in [src/lib.rs](src/lib.rs).

## Interface

- **Guest Export:** `export_plugin!(handle)` exposes `opendash_run()` on `wasm32` targets (`[cms/examples/plugins/rust-seo-plugin/src/lib.rs:handle]`, `[cms/crates/opendash-plugin-sdk/src/lib.rs:export_plugin]`).
- **Dispatched Events (`handle(env: Envelope)`):**
  - `filter:seo/head` -> `handle_seo_head`: Reads `brand_name` via `plugin_settings_get()`, truncates `values.summary` / `values.excerpt` to `MAX_SEO_DESCRIPTION_CHARS` (`160`), and returns `SeoHeadOutput` with `title`, `description`, `noindex: Some(false)`, and Schema.org `Article` or `Product` `json_ld`.
  - `filter:entry/view` -> `handle_entry_view`: Reads `default_badge` (fallback `"Mới"`) and `brand_name` (fallback `"OpenDash"`) and emits `EntryViewOutput` (`badge`, `seo_managed`, `brand_name`).
  - `content/after_publish` -> `handle_after_publish`: Increments `published_ping_count` via `kv_get`/`kv_set`, resolves missing `slug` via `content_get`, and—when `plugin_secret_get("webhook_token")` is configured—posts JSON to `https://hooks.example.com/seo-ping` via `http_request` with `Authorization: Bearer <token>`, then logs via `log`.
  - Unrecognized events return `{"ok": true, "ignored_event": <event>}`.
- **Permissions & Settings ([manifest.json](manifest.json)):**
  - `permissions`: `sqlite` (`["kv_read", "kv_write"]`), `content_read: ["*"]`, `plugin_settings_read: true`, `plugin_secrets_read: ["webhook_token"]`, and `network` (`POST https://hooks.example.com:443/seo-ping`).
  - `settings_schema`: `brand_name` (text, default `"OpenDash"`), `default_badge` (text, default `"Mới"`), `webhook_token` (secret, `default: null`).

## Invariants

- **Secret Isolation:** `webhook_token` defaults to `null` in [manifest.json](manifest.json) and must never appear in `write_output` payloads or `log` output (`[cms/examples/plugins/rust-seo-plugin/src/lib.rs:content_after_publish_increments_kv_sends_secret_bearer_without_leaking_secret]`).
- **Deterministic Fallbacks:** Missing or whitespace-only settings fall back to `"OpenDash"` and `"Mới"` (`[cms/examples/plugins/rust-seo-plugin/src/lib.rs:trimmed_setting_or]`), and SEO descriptions never exceed 160 Unicode scalar characters (`[cms/examples/plugins/rust-seo-plugin/src/lib.rs:truncate_chars]`).
- **Workspace Isolation:** The empty `[workspace]` table in [Cargo.toml](Cargo.toml) keeps plugin dependencies and build targets separate from `cms/Cargo.toml` / `cms/Cargo.lock`.

## Side Effects

- **SQLite KV & Outbound HTTPS:** On `content/after_publish`, mutates plugin KV key `"published_ping_count"` and issues a `POST` request to `https://hooks.example.com/seo-ping` if `webhook_token` is present.
- **Host Logs & Build Artifacts:** Emits `LogLevel::Info` host log entries per publish event; local builds write compiled artifacts under `target/`.

## Verification

Run host-side `MockHost` unit tests (no `wasm32-unknown-unknown` toolchain required):

```bash
cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --release
```

Build the WebAssembly module, package `manifest.json` and `plugin.wasm` into a ZIP archive, and validate offline via `[cms/crates/opendash-server/src/plugin/package.rs:check_plugin_path]`:

```bash
rustup target add wasm32-unknown-unknown
cargo build --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --target wasm32-unknown-unknown --release
cp cms/examples/plugins/rust-seo-plugin/target/wasm32-unknown-unknown/release/rust_seo_plugin.wasm /tmp/plugin.wasm
zip -j /tmp/rust-seo-plugin.zip cms/examples/plugins/rust-seo-plugin/manifest.json /tmp/plugin.wasm
./cms/target/release/opendash --check-plugin /tmp/rust-seo-plugin.zip
```
