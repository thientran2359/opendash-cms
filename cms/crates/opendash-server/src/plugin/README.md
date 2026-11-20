# Host plugin Wasmi (`opendash-server/src/plugin/`)

Thực thi plugin WebAssembly qua **Wasmi (`=2.0.0`)** trong tiến trình worker cô lập (`--plugin-worker`) với fuel limit, linear memory cap (`16 MiB`), deadline (`5s`), `StagedCommitGuard` và HTTP egress chống SSRF.

- `mod.rs`, `manager.rs`: Vòng đời cài đặt, bật/tắt, `revision_generation` fencing và kiểm soát đồng thời (4 permits).
- `package.rs`: Kiểm tra ZIP, `manifest.json` v1/v2, chữ ký Wasm module (`opendash_v1`) và staging revisions bất biến (`0022_plugin_revisions.sql`).
- `runtime.rs`, `worker.rs`: Giao thức IPC với tiến trình con `--plugin-worker`, dọn dẹp tiến trình đồng bộ (`ProcessGuard`) và zero late writes.
- `host_ops.rs`: Thực thi 4 host operations (`kv_get`, `kv_set`, `http_request`, `content_save`) theo `PluginPermissions`, `HookExecutionMode` và `ScopedFieldGrant`.
- `egress.rs`: DNS pinning (`reqwest::dns::Resolve`), chặn loopback/private/CGNAT/link-local IP, tắt redirect và giới hạn streaming `1 MiB`.
- `tests.rs`, `review_regressions.rs`: Bộ kiểm thử đơn vị và hồi quy sandbox/commit.

Xem hợp đồng chi tiết tại [Plugin ABI](../../../../../docs/contracts/PLUGIN_ABI.md) và [M6 Plugin Extensions](../../../../../docs/contracts/M6_PLUGIN_EXTENSIONS.md).
