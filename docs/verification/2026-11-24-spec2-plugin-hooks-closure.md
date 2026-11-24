# Spec 2: Plugin Hooks, Host Ops v1.1, Owner Settings (`SecretStore`) & Write-Time Projections — EGM Closure Verification (`#524`–`#527`, 2026-11-24)

- **Design Spec:** [`docs/designs/plugin-hooks-host-ops-and-projections.md`](../designs/plugin-hooks-host-ops-and-projections.md)
- **Drift Report:** [`.agents/research/2026-11-24_SPEC2_PLUGIN_HOOKS_DRIFT_REPORT.md`](../../.agents/research/2026-11-24_SPEC2_PLUGIN_HOOKS_DRIFT_REPORT.md) (Goldfish `4a0aa7d1` đối chiếu `65dd049` -> `0 Blocker · 2 Major · 5 Minor · 4 Nit`; sau PR hardening này: **`PASS: 0 Blocker · 0 Major · 2 Minor · 2 Nit`**)
- **Merged Implementation PRs (`origin/main@65dd049`):**
  - Slice S01 (`#524`): PR `#536` (`e48a65dd074d6b7cd2f945a5ba0aeb9ac494b71e`) — Manifest v3, ABI v1.1 (`-2` readback), binary IPC, 15 Host Ops & `StagedCommitGuard`
  - Slice S02 (`#525`): PR `#540` (`e6e1934a1d40361a289a5f69def68b5ece119afe`) — Migration `0050`, Owner Settings PK isolation, write-only `SecretStore` & `<OwnerOptionsForm>`
  - Slice S03 (`#526`): PR `#542` (`c3dd2e46ef32a64a8713d22a0401e372ae3cf88a`) — `AsyncHookQueue`, typed filters (`prepare_save`, `contact/before_submit`) & write-time `plugin_entry_projections`
  - Slice S04 (`#527`): PR `#543` (`65dd0499ff3e2d6340429b38ac37e53cba10d0d6`) — Offline `--check-plugin` CLI, `PLUGIN_ABI.md` v1.1, `M8_WRITER_INVENTORY.md` & E2E suites
  - Hardening + Verified Closure: nhánh `fix/spec2-drift-hardening-and-verified-closure` (vá `M-01`, `M-02`, `m-01`, `m-02`, draft reset `m-03`, `m-05`, `n-01`, `n-02`, `n-04`)
- **Environment:** macOS Intel (`x86_64`), release binary `cms/target/release/opendash` (SHA-256: `1504186888587d41b7a82913d397aec3945070b7a8f619e573974d75702d48fd`)
- **Verifiers:** Orion (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`4a0aa7d1-92f2-47e4-82bc-ca434018d37d`) + GitHub Actions CI
- **Overall Result:** **`PASS`** — nâng [`docs/designs/plugin-hooks-host-ops-and-projections.md`](../designs/plugin-hooks-host-ops-and-projections.md) từ `Implemented` lên **`Verified`**.

---

## 1. Automated Verification Sau Hardening trên `origin/main@65dd049`

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | TypeScript check (`tsc --noEmit`) gồm `PluginsPanel.tsx` & `OwnerOptionsForm.tsx` | **PASS** | Exit `0` |
| `npm --prefix cms/admin run build` | Production Admin bundle (`tsc && vite build`) | **PASS** | Exit `0`, `118 modules transformed` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting Rust toàn workspace | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --release --locked` | Release single-binary build sau các bản vá `M-01`/`M-02` | **PASS** | Exit `0`; SHA-256 `1504186888587d41b7a82913d397aec3945070b7a8f619e573974d75702d48fd` |
| `./cms/target/release/opendash --check-plugin cms/examples/plugins/sample-plugin/sample-plugin.zip` | Smoke test offline `--check-plugin` CLI trên package `.zip` | **PASS** | Exit `0`; `{"ok":true,"plugin_id":"sample-plugin","version":"1.0.0","manifest_schema":1,"hooks":[],"filters":[],"actions":[],"blocks":[],"settings_count":0}` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash --release --locked` | Toàn bộ unit tests trong `opendash-server` (gồm `hooks::tests`, `operations::tests`, `host_ops::egress_quota_tests`, `package::block_package_tests`, `plugin::tests`) | **PASS** | `223 passed; 0 failed; 0 ignored` (`98.02s`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_s01_t02_writer_barrier --release --locked` | Xác minh `M8_WRITER_INVENTORY` khớp mọi mutating handler & `AdmittedMutation` | **PASS** | `2 passed; 0 failed; 0 ignored` (`1.46s`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test plugin_extensibility_test --release --locked` | E2E `CUJ-1..4` (`SecretStore`, CAS, `prepare_save` ACL, `contact/before_submit`, JSON-LD escape, `noindex`, `0` Wasm public GET, `--check-plugin`, `s01` readback/IPC/KV tombstone/`content_delete`) | **PASS** | `5 passed; 0 failed; 0 ignored` (`3.16s`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-server --test plugin_s03_pipelines_and_projections --release --locked` | E2E Slice S03 (`prepare_save` patch+ACL, `contact/before_submit`, `AsyncHookQueue`, `seo/head` & `entry/view` projections lifecycle) | **PASS** | `4 passed; 0 failed; 0 ignored` (`3.63s`) |
| `python3 cms/scripts/check-standards.py` + `--self-test` | Ranh giới kiến trúc `core`/`storage`/`routes`/`admin`, SQL & CSRF | **PASS** | Exit `0`, `88/88` mutation handlers resolved (`84` direct + `1` delegated + `1` public Origin + `2` bootstrap exemptions); `43 + 16 + 10` negative regressions PASS |
| `python3 cms/scripts/check-plans.py` + `--self-test` | Liên kết Markdown tương đối, `SITEMAP.md` `Verified` links & self-tests | **PASS** | Exit `0`, `13` fixtures (`valid + 12 failures`) PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` + `git diff --check` | Metadata `.agents/` và kiểm tra whitespace | **PASS** | Exit `0` |

> [!NOTE]
> Trên macOS, `cargo test` sử dụng `TMPDIR=/private/tmp` để tránh `/var` symlink kích hoạt symlink security guard trong các unit test đường dẫn & vận hành.

---

## 2. Xác nhận Khắc phục Hardening trước Closure

1. **`M-01` (`AsyncHookQueue` maintenance drain race):** Đã bổ sung kiểm tra `maint_rx` / `shut_rx` trước `try_send` và spawn drain `shared_rx` khi cờ bảo trì bật ngay sau `try_send`, bảo đảm không làm kẹt `MutationLease` quá thời hạn `DrainTimeout`.
2. **`M-02` (`execute_plugin_action` & `mutate_entry_builder` projections + async hooks):** Đã gắn `recompute_entry_projections` và phát `AsyncHookJob` (`ContentAfterSave`, `ContentAfterPublish` khi `published`, `ContentAfterDelete` khi xóa qua action) nhất quán với các route CRUD chuẩn.
3. **Các điểm `Minor`/`Nit` đã xử lý:** Cổng `if outcome.changed` tại `plugin_put_settings`, dùng `PluginManifest::from_json_str` tại `settings.rs`, phát `ContentAfterPublish` cho `Immediate` CPT tại `entry_create`/`entry_translation_create`, dùng `onUnauthorizedRef` tại `PluginsPanel.tsx`, đếm `dispatched` sau lọc `target_pid` trong `dispatch_projection_filters`, vá encoding tiếng Việt tại `plugin/manager.rs`, `to_ascii_lowercase()` cho `/api/p/` loopback block tại `host_ops.rs`, và cập nhật `site_options_get` tại [`PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md).

---

## 3. Limits & Honest Notes

1. **Phạm vi suite trên máy macOS Intel sau hardening:** Sau khi áp dụng các bản vá trên nhánh `fix/spec2-drift-hardening-and-verified-closure`, đã chạy đầy đủ Admin `typecheck`/`build`, CLI `--check-plugin`, toàn bộ `223` unit tests `--bin opendash`, `m8_s01_t02_writer_barrier`, `plugin_extensibility_test` và `plugin_s03_pipelines_and_projections` trên release build; toàn bộ `--workspace` suite đa nền tảng (`OpenDash CMS Build & Test`, `OpenDash Governance`) được chạy tự động qua GitHub Actions CI trên PR closure trước khi merge.
2. **CodeGraph docs indexer cục bộ (`NOT RUN (tooling)`):** `node cms/scripts/codegraph-index-docs.mjs` chưa chạy cục bộ trên macOS vì thiếu binary `@astudioplus/codegraph-mcp` (`CODEGRAPH_ENGINE_PATH` chưa đặt).
3. **Live Browser E2E riêng cho `<OwnerOptionsForm>`:** Không chạy lại phiên Chrome thủ công mới trên nhánh closure; hành vi giao diện được kiểm chứng qua `typecheck`/`vite build` (`118 modules`) và HTTP contract E2E trong `plugin_extensibility_test`.
