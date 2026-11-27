# Unified JS Runtime (`boa_engine`), Render/Route Hooks, Extension Secrets & Dev Live-Edit

* **Author:** Elephant (Antigravity)
* **Status:** Verified
* **Date:** 2026-11-26
* **Baseline:** `main@66ebd73` · **Verified On:** `main@745cd36` (`PR #586`, `#587`, `#589`, `#590`, `#591`, `#592`)
* **Approved By:** Owner
* **Approval Evidence:** Owner chat 2026-11-26 ("Duyệt Spec B, nhưng bạn tự triển khai full Slice S01 → S04 nhé" — duyệt kiến trúc `boa_engine`, `functions.js`/`plugin.js`, Render Filters, Custom Routes `/p/` + `/api/p/` + `/api/public/p/`, Dev Live-Edit và 4 điểm nới DX/bảo mật)
* **Approval Gate Note:** Thêm dependency `boa_engine` vào `cms/Cargo.toml`, thêm migration `0053`, thay đổi phạm vi V1 trong `docs/product/PRODUCT_REQUIREMENTS.md` §5/§7 và `docs/architecture/TECH_STACK.md` — đã được `Owner` phê duyệt trực tiếp theo [ADR-0010](../decisions/ADR-0010-agent-project-and-merge-authority.md).
* **Goldfish Review:** 3-agent independent review loop (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`) + readiness & delta re-verify `READY` · 2026-11-26 · reviewed `main@66ebd73` (0 Blocker / 0 Major)
* **Verification & Drift Closure:** [Biên bản kiểm chứng](../verification/2026-11-26-unified-js-runtime-verified.md) · [Goldfish Drift Report](../../.agents/research/2026-11-26-unified-js-runtime-drift-report.md) (3 vòng audit `38199ce8` → `ea75ff3b` → `2af31f4f` trên `main@745cd36`: `0 Blocker / 0 Major / 0 Minor / 0 Nit`)
* **Related Specs:**
  * [Plugin Hooks, Host Ops v1.1, Owner Settings (`SecretStore`) & Write-Time Projections](plugin-hooks-host-ops-and-projections.md)
  * [Dual Guest SDK (`AssemblyScript` & `opendash-plugin-sdk`) & Reference Examples](plugin-dual-sdk-and-examples.md)
  * [Trusted Theme HTML (Unrestricted Inline SVG & JS) & WordPress-Style Asset Enqueue System](trusted-theme-js-svg-and-asset-enqueue.md)
  * [Phase 2 Legacy Retirement: Studio Pattern Cutover, M3–M5 Route Removal, Theme v3 Enforcement & PRD Sync](phase2-legacy-builder-theme-v3-cleanup.md)

---

## Overview

Sau [Spec A](trusted-theme-js-svg-and-asset-enqueue.md) (HTML/SVG/JS frontend & Asset Enqueue) và [Phase 2 Legacy Retirement](phase2-legacy-builder-theme-v3-cleanup.md), OpenDash CMS có nền tảng Theme v3 và Plugin runtime sạch nhưng vẫn tồn tại ba rào cản lớn cho **Developer và AI Agent**:

1. **Bắt buộc bước build `.wasm`:** Tác giả theme hoặc plugin muốn viết hook, action hay filter phía server đều phải cài toolchain Rust/AssemblyScript và compile ra `.wasm`, không thể sửa file code trực tiếp như `functions.php` của WordPress.
2. **Chưa có móc Render HTML/CSS động & Custom Routes:** Logic JS/Wasm hiện chỉ xuất dữ liệu JSON qua write-time projection (`filter:seo/head`, `filter:entry/view`), chưa thể dùng logic phức tạp (vòng lặp, truy vấn `od.content.list`, đọc options/KV) để xuất HTML/CSS/SVG/script động vào trang hoặc mở custom endpoint/page cho theme & plugin (`EnvelopeKind::Route`).
3. **Chu kỳ sửa–đóng gói–upload chậm khi phát triển:** Mọi thay đổi HTML/CSS/JS trong theme hoặc plugin đều phải đóng lại thành file `.zip` và cài đặt thành revision bất biến mới.

Đặc tả này (**Spec B**) hợp nhất toàn bộ năng lực mở rộng backend của **Theme (`functions.js`)** và **Plugin (`plugin.js` hoặc `plugin.wasm`)** trên một kiến trúc duy nhất:

- **Unified Worker Runtime (`boa_engine = "=0.22.0"` + Wasmi `=2.0.0`):** Chạy JavaScript ES2022+ thuần Rust bên trong **chính tiến trình cô lập `--plugin-worker`** hiện có (giữ nguyên IPC nhị phân `u32 LE`, `HostPolicy`, 15 Host Ops, `StagedCommitGuard`, `AsyncHookQueue`, bổ sung giới hạn RAM OS `128 MiB` qua `operations_worker_process`, và giữ tương thích 100% với `.wasm`). Kèm file định nghĩa kiểu **`cms/sdk/js/opendash.d.ts`** cho SDK đồng nhất `globalThis.od`.
- **Hỗ trợ đầy đủ cho Active Theme (`functions.js`):** Migration `0053_theme_functions_runtime.sql` (`+0` net schema objects) cô lập theo `owner_kind`, monotonic fencing `(revision_id, selection_version)`, `entry.theme_ext`, Theme Actions (`POST /api/admin/themes/active/actions/{action_id}`), Theme Secrets (`SecretStore` + Backup/Restore `ExtensionSecret`), và Admin UI.
- **Render Hooks (`{{ od_hook(...) }}`, `filter:render/head`, `filter:render/footer`) + `ExtensionRenderCache`:** Cho phép Theme/Plugin sinh HTML/CSS động (`render_trusted_html: true` mặc định cho Theme & Plugin v3; vẫn hỗ trợ opt-out `false` qua strict C-SEC) với cơ chế chuẩn hóa độc lập từng subscriber, bảo toàn bất biến `SeoView`/`final_noindex`/`IconNonceTable`, và cache theo `SiteCacheEpoch::global()`.
- **Scoped Custom Routes (`EnvelopeKind::Route`):** Kích hoạt `/p/{pkg}/{*subpath}` (`GET` read-only), `/api/p/{pkg}/{*subpath}` (`GET`/`POST` có session + CSRF + `AdmittedMutation`), và `/api/public/p/{pkg}/{*subpath}` (`POST` công khai cho AJAX/Webhook với Origin/RateLimit + `AdmittedMutation`), dự phòng tiền tố `"p"` trong `RESERVED_SYSTEM_ROOTS` và hợp nhất SSRF denylist `/p/`, `/api/p/`, `/api/public/p/`.
- **Dev Live-Edit Overlay (`--dev-theme <dir>` / `--dev-plugin <dir>`):** Đọc trực tiếp templates/HTML/CSS/assets/blocks/`functions.js`/`plugin.js` từ thư mục phát triển kèm bust cache `dev-<64-hex>`, một nút/API đồng bộ schema thủ công (`POST /api/admin/dev/sync`), tuyệt đối tách biệt khỏi production/Safe Mode/Backup.

---

## The Problem

1. **Rào cản công cụ & chi phí lặp lại của `.wasm`:** AI agent hoặc web developer muốn thêm một filter chuẩn hóa dữ liệu hay một shortcode/hook động cho theme/plugin phải thiết lập môi trường biên dịch Rust/AssemblyScript.
2. **Độ trễ worker & trần bộ nhớ khi chạy Render/Route hooks:** `PluginRuntime` hiện spawn OS process cho mỗi invocation; nếu gọi lại trên mỗi HTTP `GET` public mà không có `ExtensionRenderCache`, `SiteCacheEpoch`, giới hạn `HostCalls`, và tách semaphore công khai thì trang web sẽ bị nghẽn; đồng thời `boa_engine::Context::RuntimeLimits` chỉ giới hạn loop/recursion nên cần trần RAM cấp OS cho `--plugin-worker`.
3. **Rủi ro bảo mật & mất đồng bộ cache/schema:** Cho phép JS xuất HTML động dễ mở đường cho XSS nếu plugin untrusted đi qua đường `normalize_public_document(..., false)` vốn chưa kiểm tra `javascript:`/`style=` URLs, hoặc nếu `filter:render/head` ghi đè `<title>`/`robots`/`canonical`; và trong Dev Mode việc đọc file sống dễ lệch với SQLite `content_types`/`builder_block_definitions`/`resolve_enqueued_assets`.

---

## Goals

1. **Nhúng `boa_engine = "=0.22.0"` vào `--plugin-worker` (`0` C dependency):**
   - Tách `wasmi::StoreLimits` khỏi `HostContext` (`plugin/host_ops.rs`) để Wasmi (`.wasm`) và Boa (`.js`) gọi chung một `handle_host_call()` với `HostPolicy` đồng nhất.
   - Bổ sung giới hạn bộ nhớ cấp OS cho `--plugin-worker` (`PLUGIN_WORKER_MEMORY_LIMIT_BYTES = 128 MiB`, hợp nhất cross-OS từ `operations_worker_process.rs` và `backup_worker.rs`) chạy *trước* khi đọc IPC/khởi tạo JS/Wasm, kèm Boa `RuntimeLimits` (`loop_iteration_limit = 5_000_000`, `recursion_limit = 128`, `stack_size_limit = 512`), và parse kiểm cú pháp tĩnh không thực thi trong `--check-theme` / `--check-plugin`.
   - Chuẩn hóa một API duy nhất `globalThis.od` (`od.on`, `od.addFilter`, `od.addAction`, `od.addRoute`, `od.kv.*`, `od.content.*`, `od.taxonomy.*`, `od.media.*`, `od.site.*`, `od.settings.*`, `od.secret.*`, `od.http.*`, `od.log.*`, `od.escapeHtml`, `od.escapeAttr`) kèm `cms/sdk/js/opendash.d.ts`.
2. **Mở rộng Active Theme (`functions.js`) & Cô lập Storage (`0053`, `+0` Schema Objects):**
   - Theme v3 hỗ trợ đúng một file tùy chọn `functions.js` ở root (tối đa `2 MiB`) + khối `"functions"` trong `theme.json`, chia sẻ `owner_kind IN ('plugin', 'theme')` trên `plugin_kv`, `plugin_entry_projections`, `plugin_logs` và `owner_kind IN ('theme', 'plugin', 'theme_functions')` trên `theme_option_*`; **mặc định cấp cho Active Theme `functions.js` các quyền đọc công khai + `kv_read`/`kv_write`** trong namespace của theme (chỉ đòi khai báo tường minh cho quyền nhạy cảm: `http`/`network`, `plugin_secrets_read`, `content_write`, `content_delete`, `*_private`).
   - Monotonic fencing `(revision_id, theme_active_selection.version)` áp dụng đồng bộ trên KV, `content_save`, `content_delete` và `plugin_entry_projections`; `entry.theme_ext` tách khỏi `entry.ext`; ưu tiên SEO `Admin > Plugin > Theme`.
   - Hỗ trợ Theme Actions, Theme Function Settings (`SecretStore`), Backup/Restore `ExtensionSecret` (giữ nguyên `<64-hex>.json` tại `database/secrets/{plugins,themes}/...`), và Safe Mode (`theme_activate` vẫn đổi được theme, mọi thực thi Wasm/JS bị chặn).
3. **Render Hooks (`od_hook`, `filter:render/head`, `filter:render/footer`) & `ExtensionRenderCache`:**
   - Thêm chỉ thị tĩnh `{{ od_hook("slot_name") }}` hoặc `{{ od_hook("slot_name", expr) }}` trong `template_sec.rs` C-SEC AST + 3 `HookEvent` (`FilterRenderHead`, `FilterRenderFooter`, `FilterRenderSlot`).
   - Chạy worker với `HookExecutionMode::FilterRender` (`0` writes, `0` secret reads, `MAX_RENDER_HOST_CALLS = 64`, timeout `1000ms`/call, phase budget `1500ms`/request, cho phép tối đa `2` `http_request` khi bật `render_cache != None`, truyền sẵn snapshot context), tách riêng `public_render_semaphore` (4 slot, timeout `200ms`, fail-open thành chuỗi rỗng khi timeout mà không cache lỗi).
   - Chuẩn hóa **riêng từng subscriber**: Theme (`functions.js`) và Plugin có `permissions.render_trusted_html: true` (**mặc định `true`**) qua `document::normalize_public_document_with_options(..., true)`; Plugin đặt `render_trusted_html: false` bắt buộc qua **cả** bộ kiểm tra C-SEC URL/CSS chặt (`javascript:`/`data:`/`style=`) **lẫn** `normalize_public_document_with_options(..., false)`; `OD_HOOK_<32hex>` thế chỗ *sau* normalize ngoài và *trước* `finalize_document_icons`; `filter:render/head` lọc sạch `<title>`/`robots`/`canonical`/`http-equiv`/`json-ld`.
   - Triển khai `SiteCacheEpoch::global()` (bơm ở mọi commit handler của `storage/*` và `PluginRuntime`) + `ExtensionRenderCache` (`Public`-only, 3 policy `"by_route"` | `"global"` | `"none"`, key dùng query đã chuẩn hóa qua `parse_query`, 404 dùng chung key `"not_found"` hoặc bypass).
4. **Scoped Custom Routes (`EnvelopeKind::Route`):**
   - Thêm `"p"` vào `RESERVED_SYSTEM_ROOTS`, hợp nhất SSRF denylist `/p/`, `/api/p/`, `/api/public/p/` trong `egress.rs` & `host_ops.rs`.
   - Mở `GET /p/{pkg}/{*subpath}` (`RouteGet`, read-only, tối đa `64` host calls, cho phép `2` HTTP, timeout `1s`), `GET`/`POST` `/api/p/{pkg}/{*subpath}` (`RoutePost` yêu cầu `AdmittedMutation` + `require_session_with_csrf`/`require_admin_with_csrf`), và `POST /api/public/p/{pkg}/{*subpath}` (`RoutePost` công khai cho `access: "public"` kèm Origin check + RateLimit, hoặc `access: "webhook"` kèm RateLimit + header chữ ký HMAC để JS xác thực, được audit riêng bởi `check-standards.py` + `M8_WRITER_INVENTORY.md`).
5. **Dev Live-Edit (`--dev-theme <dir>` / `--dev-plugin <dir>`):**
   - Chỉ kích hoạt qua cờ CLI / biến môi trường, từ chối lúc khởi động nếu `recovery_safe_mode == true` hoặc `trusted_public_origin.is_some()` khi chưa bật `--allow-dev-remote`, không lưu vào `config/site.json`, và tự ngưng khi runtime bật Safe Mode.
   - Đọc trực tiếp templates/`assets/*`/`blocks/`/`locales/`/`patterns/`/`functions.js`/`plugin.js` từ ổ đĩa (lọc đường dẫn hợp đồng, bỏ qua `.git`/`.DS_Store` khỏi làm fail walk nhưng tuyệt đối chặn đọc file ẩn/`.env`/symlink), cache-bust `dev-<64-hex>` hỗ trợ trong `resolve_enqueued_assets` & `/theme/dev-.../` (`Cache-Control: no-store`), đóng băng tự động sửa SQLite tới khi gọi `POST /api/admin/dev/sync`, và phát hiện qua `GET /api/health` (`dev_mode: true`) + badge Admin.

---

## Non-Goals

- Không hỗ trợ npm/Node `require()`, filesystem trực tiếp, hay socket tùy ý trong JS (mọi tài nguyên đi qua 15 Host Ops có kiểm soát quyền).
- Không cho phép Theme/Plugin chiếm root path tùy ý ngoài `/p/{pkg}/{*subpath}`, `/api/p/{pkg}/{*subpath}` và `/api/public/p/{pkg}/{*subpath}`.
- Không cho phép route `access: "webhook"` chạy mà không đi qua `AdmittedMutation`, RateLimit và kiểm tra cấu hình chữ ký trong JS; route `/api/p/{pkg}/*` vẫn giữ CSRF guard đầy đủ.
- Không hỗ trợ `filter:render/document` bọc đè toàn bộ HTML hoặc `theme/after_deactivate`.
- Không triển khai warm worker pool tái sử dụng heap giữa các request trong Spec B (giữ per-call worker isolation + `ExtensionRenderCache`).

---

## Critical User Journeys (CUJs)

### CUJ 1: Developer / AI viết `functions.js` & `plugin.js` không cần build (Hooks, Filters, Projections, Actions)
* **Given:** Một Theme v3 có `theme.json` (khai báo `"functions"`) + `functions.js`, và một Plugin có `manifest.json` (`entrypoint: "plugin.js"`) + `plugin.js` cùng dùng SDK `od.*` (tham chiếu `cms/sdk/js/opendash.d.ts`).
* **When:** Chạy `opendash --check-theme` / `--check-plugin`, cài đặt, kích hoạt theme & bật plugin, lưu/xuất bản bài viết, lưu `secret` và chạy Action.
* **Then:**
  - `--check-theme` và `--check-plugin` kiểm tra parse tĩnh JS (không chạy code, exit `0`; báo lỗi cú pháp nếu JS hỏng).
  - Khi lưu/xuất bản bài viết: `filter:content/prepare_save`, `filter:seo/head`, `filter:entry/view` thực thi trong `--plugin-worker` chạy `boa_engine`; `{{ entry.theme_ext.<k> }}` và `{{ entry.ext.<plugin_id>.<k> }}` hiển thị trên trang công khai với **`0` worker spawn**.
  - Theme Secrets đọc qua `od.secret.get(...)`, ẩn dưới `{"configured": bool}` ở Admin API, và được bảo toàn qua Backup/Restore (`ExtensionSecret`).

### CUJ 2: Render HTML/CSS động (`od_hook`, `filter:render/head`, `filter:render/footer`) & Custom Route (`/p/`, `/api/p/`, `/api/public/p/`)
* **Given:** Theme `functions.js` (không cần khai báo `permissions` rườm rà vẫn đọc được CPT/KV) đăng ký `od.addFilter("filter:render/head", ...)` và `od.addFilter("filter:render/slot", (env) => env.payload.slot === "featured_bar" ? ... : "")`, template `home.html` gọi `{{ od_hook("featured_bar", { limit: 3 }) }}`; một Plugin `plugin.js` (mặc định `render_trusted_html: true`) đăng ký route `GET /p/my-plugin/report`, `POST /api/p/my-plugin/refresh` (`access: "session"`), và `POST /api/public/p/my-plugin/webhook` (`access: "webhook"`).
* **When:** Khách mở `/` hai lần liên tiếp, sau đó Admin sửa một bài viết và khách mở lại `/`; tiếp theo gọi `GET /p/my-plugin/report`, `POST /api/p/my-plugin/refresh`, và `POST /api/public/p/my-plugin/webhook`.
* **Then:**
  - Lần đầu mở `/`: worker chạy JS render HTML/CSS/SVG/inline-script hợp lệ (giữ nguyên Core `<title>`/`robots`/`canonical`), kết quả cache vào `ExtensionRenderCache` theo `SiteCacheEpoch::global()`.
  - Lần hai mở `/`: hit cache, **`0` worker spawn**. Sau khi Admin lưu bài viết, `SiteCacheEpoch::global()` tăng và cache tự làm mới ở lượt truy cập kế tiếp.
  - `GET /p/my-plugin/report` trả HTML hợp lệ từ JS (được gọi tối đa `2` HTTP request ngoài nếu bật cache); `POST /api/p/my-plugin/refresh` yêu cầu session + CSRF + `AdmittedMutation` (`401` nếu anonymous); `POST /api/public/p/my-plugin/webhook` truyền `raw_body` + header chữ ký đã allowlist cho JS xác thực HMAC và ghi được `od.kv.set` qua `StagedCommitGuard` + `AdmittedMutation`.

### CUJ 3: Dev & AI Agent chỉnh sửa trực tiếp bằng `--dev-theme` / `--dev-plugin`
* **Given:** Server khởi động với `--dev-theme ./my-theme --dev-plugin ./my-plugin` (không bật `recovery_safe_mode` và không có `trusted_public_origin`).
* **When:** AI agent sửa trực tiếp `my-theme/header.html`, `my-theme/assets/app.css`, `my-theme/blocks/card/render.html`, `my-theme/functions.js`, và `my-plugin/plugin.js` trên ổ đĩa, rồi F5 trình duyệt; sau đó sửa `my-theme/theme.json` thêm một CPT field mới và gọi `POST /api/admin/dev/sync`.
* **Then:**
  - Mỗi lần sửa HTML/CSS/asset/block/JS trên ổ đĩa đều phản ánh **ngay lập tức** trên trang public (`/theme/dev-<64-hex>/assets/...` có `Cache-Control: no-store`) mà không cần đóng ZIP hay đụng tới SQLite `theme_revisions`.
  - Sửa `theme.json`/`manifest.json` chỉ đồng bộ vào SQLite khi Admin gọi `POST /api/admin/dev/sync` (không hỏng dữ liệu khi file JSON đang sửa dở).
  - Trong Admin hiện huy hiệu `DEV MODE` (từ `/api/health`); đường dẫn `.env`/`.git`/symlink đều bị chặn; tắt cờ `--dev-*` quay lại đúng revision bất biến trong DB.

---

## Technical Plan (Architecture)

### A. Unified Worker (`boa_engine` + Wasmi), OS Memory Cap, `HostContext` & `opendash.d.ts`

1. **Dependency & Standards (`cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-server/Cargo.toml`):**
   - Pin **`boa_engine = "=0.22.0"`** trong `[workspace.dependencies]` và `cms/crates/opendash-server/Cargo.toml` (`opendash-core/Cargo.toml` giữ nguyên 0 dependency mới, tuân thủ `check-standards.py`).
   - Cập nhật `docs/architecture/TECH_STACK.md`, `docs/contracts/PLUGIN_ABI.md`, `docs/contracts/M5_THEME_TEMPLATES.md`, `docs/contracts/M6_PLUGIN_EXTENSIONS.md` và `docs/product/PRODUCT_REQUIREMENTS.md` §4–5 & §7 (`Owner` duyệt).
2. **Cross-OS Memory Limit & Unified `--plugin-worker` (`operations_worker_process.rs`, `backup_worker.rs`, `plugin/{host_ops,worker,runtime}.rs`, `main.rs`):**
   - Hợp nhất helper giới hạn bộ nhớ OS (`128 MiB` cho `--plugin-worker`, `512 MiB` cho `--operations-worker`) trong `operations_worker_process.rs` & `backup_worker.rs`:
     - **macOS (`posix_spawnattr_setjetsam_ext`):** tổng quát hóa `spawn_with_args(exe, args, env_vars, memory_mib, bootstrap_proof, inherit_stderr)` dùng chung `BOOTSTRAP_FD = 3` (proof riêng `b"opendash-plugin-jetsam-128mib-v1"` cho `--plugin-worker`) và trả `GuardedChild { Std(std::process::Child) | Mac(OperationsChild) }` (hỗ trợ `stdin`, `stdout`, `kill`, `wait` đồng nhất trong `ProcessGuard` ở `plugin/runtime.rs:78`).
     - **Windows (Job Object) & Linux (`RLIMIT_AS`):** chuyển `enforce_native_memory_limit_bytes(limit_bytes)` vào `operations_worker_process.rs` (gọi `512 MiB` từ `backup_worker.rs` và `128 MiB` từ `worker.rs::run_worker_from_stdio` *trước* `read_ipc_frame` / Boa / Wasmi init, và hỗ trợ `test_worker_entry` trong `cfg!(test)` khi spawn qua cùng helper có proof/env test).
   - Gỡ `pub limits: wasmi::StoreLimits` khỏi `HostContext` (`plugin/host_ops.rs:41`); chuyển `StoreLimits` sang struct cục bộ của Wasmi runner. Thêm `pub owner: RuntimeOwner` (`{ kind: ExtensionOwnerKind, id: String }`) trên `HostContext`.
   - Mở rộng `WorkerInvocationRequest` (`plugin/worker.rs:28`) bằng `pub runtime_kind: ExtensionRuntimeKind` (`Wasm | Js`) và truyền payload code (`wasm_bytes` hoặc UTF-8 `js_source`, tối đa `MAX_WASM_BYTES = 2 MiB`) thống nhất qua `IPC_STREAM_MAGIC` + `u32 LE`.
3. **Engine JS `boa_engine` & SDK `globalThis.od` (`plugin/js_runtime.rs`, `cms/sdk/js/opendash.d.ts`):**
   - Giới hạn an toàn Boa cố định trên từng fresh `Context`:
     - `RuntimeLimits`: `set_loop_iteration_limit(5_000_000)`, `set_recursion_limit(128)`, `set_stack_size_limit(512)`.
     - Không cài đặt `fetch`, `require`, `fs`, `process` hoặc bất kỳ Web API ngoại lai nào; output giới hạn `1 MiB` (`MAX_IPC_FRAME_BYTES = 2 MiB`).
   - API `globalThis.od` duy nhất (wrapping trực tiếp `host_ops::handle_host_call` + `read_ipc` đồng bộ):
     - **Registry (top-level chỉ đăng ký handler, 0 Host I/O ở top-level):**
       - `od.on(event, (env) => result)`
       - `od.addFilter(filterName, (env) => result)` (với slot template: `od.addFilter("filter:render/slot", (env) => ...)`, kiểm `env.payload.slot`)
       - `od.addAction(actionId, (env) => result)`
       - `od.addRoute(routeId, (env) => response)`
     - **Host Ops & Quy ước giá trị trả về/lỗi:**
       - `od.kv.get(key) -> unknown | null` (`null` nếu `!found`); `od.kv.set(key, val) -> void`; `od.kv.delete(key) -> void`; `od.kv.list(prefix?, limit?) -> { items }`.
       - Mọi Host Op còn lại (`od.content.{get,list,save,delete}`, `od.taxonomy.list`, `od.media.get`, `od.site.options`, `od.settings.get`, `od.secret.get`, `od.http.request`) trả trực tiếp `.data` khi `HostOperationResponse.ok == true` và **throw JS `Error("[code] message")`** khi `ok == false`.
       - `od.log.{info,warn,error}(msg)` và pure helpers `od.escapeHtml(text)`, `od.escapeAttr(text)`.
   - Hàm `validate_js_module_syntax(source: &str) -> Result<(), String>` chỉ chạy `boa_engine::Source::from_bytes` + parse AST (`Context::parse`) trong `--check-theme` / `--check-plugin` (**không** thực thi code).
   - Lưu file contract **`cms/sdk/js/opendash.d.ts`** trong repo.

### B. Manifest Schema, Theme Functions Runtime & Storage Isolation (`0053`, `+0` Schema Objects)

1. **`opendash-core` (`hooks.rs`, `plugin.rs`, `theme.rs`, `backup.rs`):**
   - `hooks.rs`:
     - Thêm `ExtensionOwnerKind` (`Plugin | Theme`), `ExtensionRuntimeKind` (`Wasm | Js`).
     - Thêm biến thể `HookEvent`: `ThemeAfterActivate` (`"theme/after_activate"`, `is_async_hook() == true`), `FilterRenderHead` (`"filter:render/head"`, `is_filter() == true`), `FilterRenderFooter` (`"filter:render/footer"`, `is_filter() == true`), `FilterRenderSlot` (`"filter:render/slot"`, `is_filter() == true`). Cập nhật test `L253`.
     - Thêm `HookExecutionMode::{FilterRender, RouteGet, RoutePost}` và mở `EnvelopeKind::Route` trong dispatcher (giữ từ chối `EnvelopeKind::Cron`).
   - `plugin.rs`:
     - Thêm `pub render_trusted_html: bool` (**mặc định `true`** cả trong JSON `serde(default = "default_true")` lẫn Rust `impl Default for PluginPermissions`, **không** đưa vào `uses_v3_features` để không phá test `manifest_schema 1..=2`, cho phép opt-out `"render_trusted_html": false` nếu plugin muốn strict C-SEC) và `#[serde(default)] pub content_read_all_public: bool` (đưa vào `uses_v3_features` yêu cầu `manifest_schema >= 3`, cho phép Plugin v3 opt-in đọc mọi public CPT mà không cần liệt kê từng `type_key`, vẫn không mở `*_private` fields) trên `PluginPermissions`.
     - Thêm `RenderCachePolicy { #[default] ByRoute, Global, None }`, `RouteMethod { Get, Post }`, `RouteAccess { #[default] Public, Session, Admin, Webhook }`, và `ExtensionRouteDecl { id: String, method: RouteMethod, path: String, access: RouteAccess, capabilities: Vec<String>, cache: RenderCachePolicy }`:
       - Quy tắc `validate_routes()`: tối đa `32` routes/manifest, `id` kebab-case `1..=64` duy nhất, `path` bắt đầu bằng `/` dài `1..=128` ký tự ASCII (`[a-zA-Z0-9/_:-]+`, tối đa 8 segments, không `..`, không trailing `/` trừ `"/"`), duy nhất `(method, path)`, **cấm `method == Get && access == Webhook`** (webhook chỉ dành cho `Post`), và `capabilities` hợp lệ (khớp single-write-scope guard như `actions`).
     - Mở rộng `PluginManifest`: `entrypoint` chấp nhận `"plugin.wasm"` (mặc định) hoặc `"plugin.js"` (bắt buộc `manifest_schema >= 3`), thêm `routes: Vec<ExtensionRouteDecl>`, `render_cache: RenderCachePolicy`, cấm `ThemeAfterActivate`, tách shared validators (`validate_min_host_version`, `validate_permissions`, `validate_actions`, `validate_routes`, `validate_settings_schema`), cập nhật `can_read_content_type` kiểm tra `self.permissions.content_read_all_public || ...`, cập nhật `manager.rs:355` `"content_read"` action capability check sang `!manifest.permissions.content_read.is_empty() || manifest.permissions.content_read_all_public`, và tham số hóa `plugin_can_{read,write}_field_for_owner` theo `ExtensionOwnerKind`.
     - Cập nhật mọi struct literal `PluginManifest` / `PluginPermissions` trong `plugin/host_ops.rs` (`L1037, L1091, L1139`), `plugin/tests.rs`, `plugin/review_regressions.rs`, và `tests/plugin_egress_ssrf.rs` (`L20`) dùng `..Default::default()` từ ngay Slice S01 để không gãy biên dịch qua các slice.
   - `theme.rs`:
     - Thêm `ThemeFunctionsSpec` trên `ThemeV3Manifest.functions: Option<ThemeFunctionsSpec>` (`api_version = 1`, `min_host_version`, `hooks`, `filters`, `hook_priority = 100`, `permissions: Option<PluginPermissions>`, `actions`, `routes`, `render_cache`, `settings_schema`); cập nhật `tests::v3_manifest()` (`L885`).
     - Validate `ThemeFunctionsSpec` bằng shared validators; thêm `ThemeV3Manifest::to_runtime_manifest(&self) -> Option<PluginManifest>` (đặt cố định `entrypoint: "functions.js"`, bypass check `"plugin.wasm"` của `PluginManifest::validate()`). Khi khối `functions` trong `theme.json` bỏ qua `permissions` hoặc để mặc định các trường đọc/KV cơ bản, `to_runtime_manifest()` **tự động cấp sẵn cho Active Theme**: `sqlite` thêm `["kv_read", "kv_write"]` (trong `HostPolicy`, `sqlite:kv_read` đã cấp phép cả `kv_get` lẫn `kv_list`), `content_read_all_public = true`, `taxonomy_read = true`, `media_read = true`, `site_options_read = true`, `plugin_settings_read = true`, `render_trusted_html = true` (trừ khi chủ theme set tường minh `false`); các quyền nhạy cảm (`http`, `network`, `plugin_secrets_read`, `content_write`, `content_delete`, `content_read_private`, `content_write_private`) vẫn yêu cầu khai báo rõ.
     - Trong `resolve_enqueued_assets`: chấp nhận cả `revision_digest` 64-hex thường lẫn `"dev-<64-hex>"` (cho Dev Live-Edit).
   - `backup.rs`: thêm `BackupFileKind::ExtensionSecret` kiểm tra `secrets/{plugins,themes}/<package_id>/<64-hex>.json` qua `valid_package_id` + `is_valid_sha256_hex`.
2. **Package & CLI (`plugin/package.rs`, `theme.rs`, `storage/plugin.rs`, `storage/theme.rs`, `storage/admin_extensions.rs`):**
   - `plugin/package.rs`: tách `validate_wasm_module_bytes`; hỗ trợ `plugin.js` (`<= 2 MiB`, UTF-8, `validate_js_module_syntax`) **hoặc** `plugin.wasm` (cấm chứa cả hai file).
   - `storage/plugin.rs` (`L173, L304`): cột `plugins.wasm_blob` giữ nguyên tên cột và lưu bytes entrypoint (`plugin.wasm` hoặc UTF-8 `plugin.js`); trên đĩa `stage_and_publish_plugin_revision_with_blocks` & `verify_plugin_revision_durability` đọc/ghi `rev_dir.join(&manifest.entrypoint)` và chạy đúng validator theo entrypoint.
   - `theme.rs`: hỗ trợ đúng một file root tùy chọn `functions.js` (`<= 2 MiB`, UTF-8 + `validate_js_module_syntax`), đối xứng `manifest.functions.is_some() == files.contains_key("functions.js")`, thêm `"functions.js"` vào `declared`, cấm `.js`/`.wasm` root trong `manifest.assets` / `theme_asset_mime`, thêm `has_functions: bool` (`skip_serializing_if = "Not::not"`) trên `ThemeCheckSummary`.
   - `storage/theme.rs` (`L401, L917`) & `storage/admin_extensions.rs` (`L475-490`): truyền danh sách `action_id` từ `functions.actions` vào `ext.validate("theme", &action_ids)` / `sync_owner_in_conn` và tra active theme manifest khi `reg.owner_kind == "theme"` để `admin_extensions` gọi được Theme Actions.
3. **Migration `0053_theme_functions_runtime.sql` (`+0` Net Schema Objects) & Storage Isolation:**
   - Rebuild `plugin_kv` (`owner_kind IN ('plugin', 'theme')`, PK `(owner_kind, plugin_id, key)`, bỏ FK `plugins(id)`).
   - Rebuild `plugin_entry_projections` (`owner_kind IN ('plugin', 'theme')`, PK `(entry_id, owner_kind, plugin_id)`, giữ FK `content_entries(id) ON DELETE CASCADE`, index `(owner_kind, plugin_id, locale)`).
   - Rebuild `plugin_logs` (`owner_kind IN ('plugin', 'theme')`, bỏ FK `plugins(id)`).
   - Rebuild `theme_option_{values,versions,media_refs}` mở rộng `CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions'))` và **tái tạo `idx_theme_option_values_owner`**. Tổng delta = **`+0` net schema objects** (tổng objects vẫn `< 256`).
   - Test filter (`storage/settings.rs:4614`): đổi `!name.starts_with("0050_")` thành **`!name.starts_with("0050_") && name.as_str() < "0053_"`** (giữ nguyên `0051` & `0052` trong `pre_dir` cho test `+2` của `0050`) và bổ sung `migration_0053_schema_delta_is_zero` (`name.as_str() < "0053_"`).
   - Cập nhật mock `CREATE TABLE plugin_kv` trong `plugin/review_regressions.rs` (`L29, 156, 290, 413, 527, 665`) thêm `owner_kind TEXT NOT NULL DEFAULT 'plugin'` + `PRIMARY KEY (owner_kind, plugin_id, key)`, và chuyển 2 câu `INSERT INTO plugin_kv VALUES (...)` 4 cột không liệt kê cột tại `review_regressions.rs:165, 297` sang liệt kê rõ cột `(plugin_id, key, value_json, updated_at)`.
   - Scope toàn bộ query trên `plugin_kv.rs` (giữ wrapper tương thích ở S01), `storage/plugin.rs` (skip `last_error` khi `Theme`, thêm `verify_active_theme_fence_in_tx`, `delete_plugin_owned_rows_in_tx`), `storage/plugin_projections.rs` (mỗi `ComputedPluginProjection` mang `owner_kind` + `theme_fence`, kiểm fence từng dòng theme trong `write_entry_projections_in_tx`, lọc xóa cũ theo `(owner_kind, plugin_id)`, `batch_load_entry_projections_in_conn` join nhánh `plugins` hoặc `theme_active_selection -> theme_revisions`, SEO precedence `Admin > Plugin > Theme`, `MergedEntryProjection.theme_ext`), và `storage/theme.rs::activate_revision` (`DELETE FROM plugin_entry_projections WHERE owner_kind = 'theme'` + kiểm tra tương thích `functions.settings_schema` cấm đổi `secret` ↔ non-`secret`).
4. **Monotonic Fencing & `PluginManager` (`plugin/{manager,runtime}.rs`, `storage/entries.rs`, `hooks.rs`, `main.rs`, `content_render.rs`, `routes/{public_content,themes}.rs`):**
   - `RuntimeOwner { kind: ExtensionOwnerKind, id: String }`, `OwnerFence { Plugin { generation }, Theme { revision_id, selection_version } }` kiểm trên **host** (`PluginRuntime`, `StagedCommitGuard`, `storage/entries.rs`, `storage/plugin.rs`, `storage/plugin_projections.rs`).
   - Nối `themes_root: PathBuf` vào `PluginManager` (`main.rs:178`), thêm `active_theme_cache` (xác minh `.complete`, no-symlink, SHA-256 `file_digests_json["functions.js"]`), đổi `active_action_leases` sang `HashSet<RuntimeOwner>`, tổng quát hóa `execute_owner_action`.
   - `routes/themes.rs::theme_activate`: thêm extractors `State(plugins)`, `State(hooks)`, `State(coordinator)`, `Extension(recovery)`, truyền `lease.clone()` vào `AsyncHookJob` (`ThemeAfterActivate`), gọi `plugins.invalidate_active_theme_cache().await`, spawn `spawn_recompute_owner_projections_batched`, và bỏ qua JS/Wasm khi `recovery.is_safe_mode()`.
   - `EntryView.theme_ext` (`BTreeMap<String, Value>`) gán tại đủ 3 điểm `routes/public_content.rs` (archive, single/home, taxonomy).

### C. Render Hooks (`od_hook`, `filter:render/*`), `ExtensionRenderCache` & Scoped Routes

1. **Template Directive & Render Pipeline (`template_sec.rs`, `document.rs`, `content_render.rs`, `routes/public*.rs`):**
   - Trong `template_sec.rs`: thêm chỉ thị C-SEC `{{ od_hook("slot_name") }}` hoặc `{{ od_hook("slot_name", expr) }}` (slot literal `[a-z0-9_-]{1..=64}`, cấm trong `<script>`/`<style>`/URL attr), xuất placeholder `OD_HOOK_<32hex>` trước `finalize_document_icons`.
   - Hợp đồng JSON cho `FilterRender*`:
     - Input `payload`: `{ slot?: String, args?: Value, route_kind: String, route_path?: String, locale: String, site: Value, options: Value, entry?: Value, entries?: Value }` (không chứa secret hay token preview).
     - Output JS: `null` / `""` (bỏ qua) hoặc chuỗi HTML hoặc `{ html?: String, styles?: Vec<String>, head_scripts?: Vec<String> }`, tổng cap `256 KiB`/subscriber.
   - `HookDispatcher::dispatch_render_*`:
     - Chạy ở `HookExecutionMode::FilterRender` (`MAX_RENDER_HOST_CALLS = 64`, cho phép `kv_list` với limit `<= 50`, `0` writes, `0` secrets, tối đa `2` `http_request` chỉ khi `render_cache != RenderCachePolicy::None`, deadline `1000ms`/call, phase budget `1500ms`/request), qua `public_render_semaphore` riêng (`4` slots bên cạnh `4` admin + `2` background = tối đa `10` workers, timeout chờ `200ms` **fail-open** thành chuỗi rỗng và không cache lỗi).
   - **Sanitization riêng từng subscriber & Chống nâng quyền (XSS Guard):**
     - **Subscriber Theme (`owner_kind == Theme`) và Plugin mặc định `render_trusted_html == true`:** normalize qua `document::normalize_public_document_with_options(&raw, true)`.
     - **Subscriber Plugin đặt `render_trusted_html == false`:** từ chối/loại bỏ các trường `styles` và `head_scripts` trả về từ JS, và bắt buộc đưa `html` qua bộ kiểm tra chặt **C-SEC HTML/URL/CSS** (tách helper dùng chung từ `template_sec.rs` cấm `<script>`, `<svg>`, `<style>`, `on*`, `javascript:`/`data:` URLs, `formaction`/`action`, `style="..."` `url()`/`expression()`) **sau đó** qua `document::normalize_public_document_with_options(&raw, false)`.
     - **Thứ tự thay thế `OD_HOOK_<32hex>`:** thực hiện *sau* `compose_document_with_seo_with_assets` (nên fragment untrusted đã lọc không bao giờ bị promote theo `active_v3.is_some()`), nối `authorized_styles`/`authorized_head_scripts` của các hook đã qua kiểm duyệt vào `<head>` trước `<!-- opendash-head-end -->`, sau đó mới chạy `finalize_document_icons`. (`od_hook` không được tự ý bật `allow_form_self` cho `form-action`; `<video ` vẫn kiểm trên document cuối.)
     - **`sanitize_render_head_fragment` (cho `filter:render/head`):** strip toàn bộ `<title>` (kể cả trong `<svg>`), `<meta name="robots">`, `<meta http-equiv>`, `<link rel="canonical">`, `<link rel="alternate">`, và `<script type="application/ld+json">` để Core `SeoView` & `final_noindex` luôn bất khả xâm phạm.
2. **`SiteCacheEpoch::global()` & `ExtensionRenderCache` (`cache.rs`, `app.rs`, `storage/*`):**
   - Triển khai `SiteCacheEpoch::global()` (singleton `OnceLock<AtomicU64>` gắn thêm vào `AppState` & `PluginRuntime`) và gọi `SiteCacheEpoch::global().bump()` tại mọi hàm commit trong **tất cả 14 module**: `storage/{entries,patterns,taxonomy,content,overrides,menu,settings,theme,plugin,media_lifecycle,import,seo,builder,page}.rs` + post-commit `PluginRuntime` (`kv_set`, `kv_delete`, `content_save`, `content_delete`).
   - `ExtensionRenderCache` bounded LRU (`512` entries, max `256 KiB`/entry):
     - Chỉ kích hoạt cho `ExecutionPrincipal::Public` (Preview, Admin, Safe Mode luôn bypass).
     - Cache key chuẩn hóa một tên thống nhất: `(owner_kind, owner_id, revision_digest_or_generation, hook_or_slot_name, arg_hash, locale, canonical_route_and_query_or_global, origin_host, site_data_epoch)`.
     - Query trong `canonical_route_and_query_or_global` chỉ lấy output đã lọc của `public_context::parse_query` (bỏ qua tracking params như `?utm_source=`); mọi response `404` dùng chung key `"not_found"` (hoặc bypass cache) để chống LRU thrashing.
     - Cache lưu fragment đã normalize *trước* `finalize_document_icons` cùng `(authorized_styles, authorized_head_scripts)`.
3. **Scoped Custom Routes (`EnvelopeKind::Route` — `public_context.rs`, `plugin/{egress,host_ops}.rs`, `routes/{mod,contact,extensions}.rs`, `app.rs`, `cms/scripts/check-standards.py`):**
   - Thêm `"p"` vào `RESERVED_SYSTEM_ROOTS` (`public_context.rs:14`). Hợp nhất một hàm `egress::is_forbidden_internal_path(path)` kiểm tra `/api/admin`, `/save`, `/preview`, `/theme`, `/api/p/`, `/api/public/p/`, `/p` (`/p/`) dùng chung trong cả `egress.rs` lẫn `host_ops.rs` (**giữ nguyên chuỗi `"/api/p/"` trong thông báo lỗi** để bảo toàn test `host_ops.rs:1172`). Cập nhật `host_ops::check_read_policy` cho phép `HostReadRequest::KvList` trong `Standard | AsyncPostCommit | FilterRender | RouteGet | RoutePost`.
   - Khớp route & payload:
     - `GET /p/{pkg}/{*subpath}` (`HookExecutionMode::RouteGet`, `MAX_RENDER_HOST_CALLS = 64`, `0` writes, `0` secrets, cho phép tối đa `2` `http_request`, tối đa `1 MiB` response, deadline `1000ms`, `503` khi worker timeout/concurrency); HTML trả về chạy qua normalize (theo `render_trusted_html`) + `compose_document_with_seo_with_assets` + `nosniff`.
     - `GET` & `POST` `/api/p/{pkg}/{*subpath}` (`POST` chạy `HookExecutionMode::RoutePost` cho `access: Session | Admin`, `DefaultBodyLimit::max(MAX_PLUGIN_JSON_BYTES)` = `1 MiB`, `AdmittedMutation(lease)` + `require_session_with_csrf` hoặc `require_admin_with_csrf` gọi **trực tiếp** trong top-level handler để pass `check-standards.py`, hỗ trợ cả `kv_set`/`kv_delete` lẫn `content_save`/`content_delete` có `ActionActorFence` qua helper `commit_staged_content_with_actor` của `PluginManager` + `SiteCacheEpoch::global().bump()`).
     - `POST /api/public/p/{pkg}/{*subpath}` (`HookExecutionMode::RoutePost` cho `access: Public | Webhook`, `DefaultBodyLimit::max(MAX_PLUGIN_JSON_BYTES)` = `1 MiB`):
       - **Middleware `app.rs` (`L720-752`):** mở rộng nhánh kiểm `is_public_contact_submit` thêm `extension_public_post` (`/api/public/p/`) để không chặn trước server-to-server Webhook không có `Origin` header.
       - **Bên trong `routes::extensions::extension_public_post`:** giải quyết manifest route (`Public` hoặc `Webhook`, **từ chối `Session | Admin`** với `403`); kiểm tra `ExtensionRouteRateLimiter` (sliding-window bucket như `ContactAdmissionLimiter`); nếu `access == Public`: gọi `crate::routes::contact::check_strict_contact_origin` (`pub(crate)`) / `trusted_origin_match`, trả `403 FORBIDDEN` nếu sai; nếu `access == Webhook`: cho phép không có `Origin`, truyền `raw_body: String` và allowlist signature headers (`x-signature*`, `x-hub-signature*`, `stripe-signature`, `x-webhook-*`) vào `payload` để JS kiểm chứng HMAC; sau các check admission đó mới gọi `coordinator.admit_mutation()` và **giữ nguyên `MutationLease` xuyên suốt lệnh chạy `RoutePost`** (vì `PluginRuntime` commit `staged_kv` ngay trong invocation); ép `ExecutionPrincipal::Public` và **từ chối `content_save`/`content_delete`** ở chế độ `Public | Webhook` (cho phép `kv_set`, `kv_delete`, `http_request`, `plugin_secret_get`, `log`).
       - **`cms/scripts/check-standards.py`:** Đăng ký `PUBLIC_EXTENSION_POST = 'crate::routes::extensions::extension_public_post'` tương tự `PUBLIC_ORIGIN`, xác minh cấu trúc kiểm Origin (`Public`), RateLimit (`Webhook`), từ chối `Session | Admin` trên đường public, và bổ sung negative regression vào `check-standards.py --self-test`.
   - Input `EnvelopeKind::Route`: `{ route_id, method, path, subpath, params, query, body?, raw_body?, signature_headers? }` (không bao giờ truyền `Cookie` hoặc `Authorization` session token cho JS).
   - Output `od.addRoute`: `{ status?: u16 (200..=599), content_type?: "text/html" | "application/json" | "text/plain", body?: string, json?: Value, title?: string }`.

### D. Theme Actions, Secrets (`SecretStore`), Backup/Restore & Dev Live-Edit

1. **Theme Actions, Secrets, `media_refs.rs` & Backup/Restore (`paths.rs`, `secrets.rs`, `storage/{backup,media_refs,restore}.rs`, `backup_worker.rs`, `restore.rs`, `routes/themes.rs`, `app.rs`):**
   - `paths.rs`: thêm `theme_secrets_dir(&self, theme_id: &str)`. `secrets.rs:71`: sửa kiểm ACL thành `matches!(..., Some("plugins") | Some("themes"))`.
   - `storage/media_refs.rs`:
     - Trong `collect_pins_in_tx` (`L191, L212`): tách `"theme_functions_option"` vs `"theme_option"` vs `"plugin_option"`; trên `plugin_kv` (`L302`) gắn `"theme_kv"` khi `owner_kind == "theme"`.
     - Trong `storage/backup.rs` & `storage/media_refs.rs`: bổ sung `extension_secret_refs: Vec<ExtensionSecretInventoryItem>` (`{ owner_subdir: "plugins" | "themes", package_id, secret_ref }`) bằng cách parse đúng cấu trúc JSON object `{"secret_ref": "<64-hex>"}` từ `theme_option_values WHERE owner_kind IN ('plugin', 'theme_functions')` (tuyệt đối **không** dùng `extract_64hex_tokens` để tránh nhầm với `Media` 64-hex IDs), ánh xạ `'theme_functions'` → `"themes"`.
   - `restore.rs::resolve_logical_dest`: ánh xạ `BackupFileKind::ExtensionSecret` vào **`stage_root.join("database").join(segments)`**, ghi giữ nguyên tên `<64-hex>.json` qua `ensure_private_directory_tree` + `create_private_file` + `verify_file_acl`.
   - `storage/restore.rs::verify_restored_db_references`: giữ nhánh SMTP riêng và kiểm riêng `ExtensionSecret` tại `site_root.join("database").join("secrets").join(&item.owner_subdir).join(&item.package_id).join(format!("{}.json", item.secret_ref))`.
   - **Hợp đồng HTTP 4 Admin Endpoints (`routes/themes.rs`, `app.rs`, `docs/contracts/M8_WRITER_INVENTORY.md`):**
     - `GET /api/admin/themes/active/functions/settings`: Admin auth; `Cache-Control: no-store`; nếu active theme có `functions`, trả `200` + `ETag: "vN"` + `{ theme_id, revision_id, has_functions: true, version, schema, values (mọi trường secret redacted thành {"configured": bool}, che secret_ref lạc kiểu), actions }`; nếu chưa có theme/functions, trả `200` + `{ theme_id, revision_id, has_functions: false, version: 0, schema: [], values: {}, actions: [] }`.
     - `PUT /api/admin/themes/active/functions/settings`: Admin + CSRF + `AdmittedMutation(lease)` + `If-Match` + `DefaultBodyLimit::max(MAX_PLUGIN_SETTINGS_JSON_BYTES)` (`1 MiB`), `Cache-Control: no-store`, `ETag: "vN"`, skip `SettingsAfterSave`/recompute khi `recovery.is_safe_mode()`.
     - `POST /api/admin/themes/active/actions/{action_id}`: `require_session_with_csrf` + `AdmittedMutation(lease)` + `DefaultBodyLimit::max(MAX_PLUGIN_JSON_BYTES)` (`404` nếu theme không có functions/action, `403` RBAC, `503` Safe Mode), gọi `execute_owner_action` + `hooks.enqueue_plugin_action_hooks`.
     - `GET /api/admin/themes/active/functions/logs`: Admin auth, `Cache-Control: no-store`, `?limit=` (`1..=100`, mặc định `50`), trả `200` `{ theme_id, items: [{ id, correlation_id, level, message, created_at }] }` từ `owner_kind = 'theme'`.
2. **Dev Live-Edit Overlay (`dev_mode.rs`, `main.rs`, `storage/theme.rs`, `page.rs`, `routes/{health,themes,operations,public*,seo,extensions}.rs`, `plugin/manager.rs`, `cms/admin/`):**
   - **Cờ & Rào chắn an toàn:** `--dev-theme <dir>` / `OPENDASH_DEV_THEME_DIR` và `--dev-plugin <dir>` / `OPENDASH_DEV_PLUGIN_DIR`. Từ chối khởi động nếu `recovery_safe_mode == true` hoặc **`runtime_cfg.portable.trusted_public_origin.is_some()`** mà chưa bật `--allow-dev-remote`; tự ngưng tức thì khi runtime bật Safe Mode; không lưu vào `config/site.json` hay `theme_revisions`.
   - **Đọc an toàn từ ổ đĩa:** Chỉ đọc các đường dẫn hợp đồng (`theme.json`, `manifest.json`, `functions.js`, `plugin.js`, `plugin.wasm`, `header.html`, `footer.html`, templates, `template-part/*.html`, `page-layouts/*.html`, `blocks/<slug>/{block.json,render.html}`, `patterns/*.json`, `locales/*.json`, `assets/*`). Bỏ qua `.git`/`.DS_Store` khỏi làm fail thư mục dev, nhưng từ chối đọc bất kỳ đường dẫn ẩn/`.env`/`node_modules` hoặc symlink (`ensure_no_reparse_or_symlink` mọi segment trong `canonical_dev_root`).
   - **Tích hợp Cache, `resolve_enqueued_assets`, `BlockTemplateCache`, `routes/themes.rs` & Fences:**
     - Hash `dev-<64-hex-fingerprint>` trên `(rel_path, len, mtime_nanos)` các file hợp đồng → `opendash_core::theme::resolve_enqueued_assets` chấp nhận `"dev-<64-hex>"`, `routes/themes.rs::theme_asset` chấp nhận `revision_digest` `"dev-<64-hex>"` khi có `DevThemeOverlay` và trả `Cache-Control: no-store` (thay vì `immutable`).
     - `storage/theme.rs::{capture_active_theme_v3_in_conn, load_active_theme_v3_in_conn}`, `storage/overrides.rs`, `storage/page.rs::load_unified_block_catalog_in_conn` & `page.rs::BlockTemplateCache::get_or_validate` (truyền digest dev của từng block / `dev-<64-hex>` thay vì digest DB cũ), và `PluginManager::ensure_active_theme_cache` / `run_plugin_with_principal` phân giải từ `DevOverlayConfig::global()` (khởi tạo cùng `AppState` tại `main.rs`), bỏ qua `.complete` *chỉ* cho gói dev đang gắn, và gắn fence `theme_active_selection.version` (hoặc skip upsert projection nếu active theme DB khác ID).
   - **Discovery API, Đồng bộ Schema, Backup & Admin UI:**
     - `GET /api/health` (`routes/health.rs`): thêm `dev_mode: bool`, `dev_theme: Option<String>`, `dev_plugins: Vec<String>`.
     - `POST /api/admin/dev/sync` (`routes/themes.rs`, `require_admin_with_csrf` + `AdmittedMutation(lease)` + `DefaultBodyLimit::max(4096)`, trả `404` khi Dev Mode tắt, `503` khi Safe Mode): đồng bộ tường minh `theme.json`/`manifest.json` declarations, blocks, hooks/filters/permissions/actions vào SQLite và trả `{ synced_theme, synced_plugins }`.
     - `create_backup` (`routes/operations.rs`): luôn cho phép tạo snapshot sạch **chỉ gồm DB committed & immutable `revisions/`** và gắn trường cảnh báo `dev_mode_active: true` trong job metadata (không bao giờ đóng gói file thô từ thư mục dev).
     - Admin UI: cập nhật `App.tsx`, `api/{client,themes}.ts`, `OwnerOptionsForm.tsx` (`'theme' | 'plugin' | 'theme_functions'`), `ThemesPanel.tsx`, `PluginsPanel.tsx`, `AdminShell.tsx` (badge `DEV MODE` + nút `"Đồng bộ Dev Manifest"`), và đầy đủ khóa dịch trong `cms/admin/src/i18n/locales/{en,vi}.ts` (`check-i18n.mjs --strict` PASS).

---

## Alternatives Considered

| Hạng mục | Phương án thay thế | Lý do bác bỏ |
|---|---|---|
| Engine JS | Chỉ dùng `.wasm` (Rust/AS) hoặc dùng `rquickjs` | `.wasm` đòi hỏi bước biên dịch bất tiện cho Dev & AI; `rquickjs` cần C compiler đa nền tảng; **`boa_engine = "=0.22.0"`** thuần Rust 100%, giữ binary single-file build mượt trên Win/Mac/Linux musl. |
| Worker Model | Warm reusable JS worker pool | Nguy cơ rò rỉ state/heap/globalThis giữa các plugin/theme hoặc đọc sót frame IPC khi cancel; per-call `--plugin-worker` + `ExtensionRenderCache` đơn giản, cô lập 100% và đạt `0` spawn trên cache hit. |
| Cache Epoch | 4 epoch rời hoặc truyền tham số vào mọi hàm `storage/*` | Làm xáo trộn chữ ký hàng loạt route; `SiteCacheEpoch::global()` là một `AtomicU64` singleton giúp mọi commit trong 14 module `storage/*` bump epoch O(1). |
| Custom Routes | Ghi đè path bất kỳ ở root (`/`...) hoặc mở endpoint không gắn `AdmittedMutation`/RateLimit | Dễ đụng `permalinks`, `301` redirect, `sitemap.xml`; tách rạch ròi `/p/{pkg}/*` (`GET`), `/api/p/{pkg}/*` (`Session`/`Admin` CSRF) và `/api/public/p/{pkg}/*` (`Public` Origin + RateLimit / `Webhook` HMAC + RateLimit, kiểm bởi `check-standards.py`). |
| Dev Live-Edit | Tự động đóng ZIP & ghi `theme_revisions` mỗi lần lưu file (`--sync-theme`) | Làm phình DB/đĩa với revision rác, gây CAS `412` ở Studio và chạy `BEGIN IMMEDIATE` liên tục mỗi lần sửa CSS. |

---

## Non-Functional Requirements

- **Stack & Single Binary:** Chỉ thêm `boa_engine = "=0.22.0"` vào `opendash-server` (`opendash-core` giữ nguyên 0 dependency ngoài mới).
- **OS Memory Isolation:** `--plugin-worker` áp dụng trần RAM `128 MiB` cấp OS (`operations_worker_process.rs`) *trước* khi khởi tạo engine + Boa `RuntimeLimits` + `ProcessGuard` kill khi quá deadline.
- **Zero Public Regression:** Trang không dùng `od_hook` / `filter:render/*` giữ nguyên hiệu năng; trang có render hook đạt `0` worker spawn khi hit `ExtensionRenderCache` và fail-open khi quá `200ms`/`1000ms`.
- **SQLite Schema Budget:** Migration `0053_theme_functions_runtime.sql` có delta = **`+0` net schema objects** (tổng objects `< 256`).
- **Security & Governance Compliance:** 100% SQL nằm trong `storage/`, 100% mutation routes tuân thủ `check-standards.py` + `AdmittedMutation` (`/api/p/*` gọi trực tiếp `require_*_with_csrf`; `/api/public/p/*` kiểm Origin cho `Public` và RateLimit + `raw_body`/HMAC headers cho `Webhook`), plugin đặt `render_trusted_html: false` đi qua strict C-SEC HTML/URL/CSS validator, và Core SEO/CSP (`final_noindex`, `<title>`, `canonical`) bất khả xâm phạm.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | Reason |
|---|---|---|
| `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-server/Cargo.toml` | Modify | Pin và thêm `boa_engine = "=0.22.0"` cho `opendash-server`. |
| `cms/sdk/js/opendash.d.ts`, `cms/examples/{themes/theme-js-starter/,plugins/plugin-js-example/}` | Create | SDK type contract `od.*` cho Dev & AI cùng ví dụ tham chiếu `functions.js` & `plugin.js`. |
| `cms/crates/opendash-core/src/{hooks.rs,plugin.rs,theme.rs,backup.rs}` | Modify | `ExtensionOwnerKind`, `ExtensionRuntimeKind`, `HookEvent`, `HookExecutionMode`, `EnvelopeKind::Route`, `PluginPermissions::{render_trusted_html(default true), content_read_all_public}`, `ThemeFunctionsSpec` + default quyền đọc/KV cho Active Theme + `to_runtime_manifest()`, `ExtensionRouteDecl` (`RouteAccess::Webhook`), `RenderCachePolicy`, `resolve_enqueued_assets` `dev-<64-hex>`, `BackupFileKind::ExtensionSecret`, `plugin_can_{read,write}_field_for_owner`. |
| `cms/migrations/0053_theme_functions_runtime.sql`, `cms/migrations/README.md` | Create / Modify | Rebuild 6 bảng với `owner_kind` + tái tạo `idx_theme_option_values_owner` (`+0` objects); cập nhật tài liệu số migrations. |
| `cms/crates/opendash-server/src/{operations_worker_process.rs,backup_worker.rs}` | Modify | Hợp nhất giới hạn RAM OS (`128 MiB` plugin / `512 MiB` backup) trên macOS/Linux/Windows và hỗ trợ `GuardedChild` trong `ProcessGuard`. |
| `cms/crates/opendash-server/src/plugin/{mod.rs,js_runtime.rs,runtime.rs,worker.rs,host_ops.rs,manager.rs,package.rs,egress.rs,tests.rs,review_regressions.rs}` | Modify / Create | Tách `StoreLimits` khỏi `HostContext`, `js_runtime.rs` (`boa_engine` + `od.*` + `RuntimeLimits`), `validate_wasm_module_bytes`, `plugin.js`, `RuntimeOwner`, `OwnerFence`, `themes_root` + durability, `public_render_semaphore`, nâng `MAX_RENDER_HOST_CALLS = 64` + HTTP trong `RouteGet`/`FilterRender` cached, `is_forbidden_internal_path`, và cập nhật tests/mock DDL. |
| `cms/crates/opendash-server/src/{theme.rs,template_sec.rs,document.rs,content_render.rs,public_context.rs,hooks.rs,cache.rs,dev_mode.rs,page.rs}` | Modify / Create | Root `functions.js` + `has_functions`, `{{ od_hook(...) }}` + validator C-SEC URL/CSS dùng chung + `sanitize_render_head_fragment`, `ExtensionRenderCache` + `SiteCacheEpoch::global()`, `EntryView.theme_ext`, `"p"` trong `RESERVED_SYSTEM_ROOTS`, `DevOverlayConfig` & `BlockTemplateCache` dev bypass. |
| `cms/crates/opendash-server/src/storage/{plugin_kv,plugin_projections,plugin,theme,entries,settings,overrides,backup,media_refs,restore,content,menu,taxonomy,media_lifecycle,import,seo,builder,page,admin_extensions}.rs` | Modify | Cô lập `owner_kind`, fencing, SEO precedence, sửa filter test `0050` (`!starts_with("0050_") && < "0053_"`) + test `0053`, `extension_secret_refs` & verify `database/secrets/`, Theme `admin_extensions`, `storage/page.rs::load_unified_block_catalog_in_conn` dev block overlay, và `SiteCacheEpoch::global().bump()`. |
| `cms/crates/opendash-server/src/{paths.rs,secrets.rs,restore.rs,routes/{mod.rs,contact.rs,themes.rs,public.rs,public_content.rs,extensions.rs,operations.rs,plugin.rs,health.rs,seo.rs},app.rs,main.rs}`, `cms/scripts/check-standards.py` | Modify / Create | `SecretStore` ACL cho `themes/`, `ExtensionSecret` restore `database/secrets/`, Theme Functions & Custom Routes (`routes/mod.rs`, `contact::check_strict_contact_origin` `pub(crate)`, `/p/`, `/api/p/`, `/api/public/p/` + audit rule + `--self-test`), `--dev-theme`/`--dev-plugin` CLI & `dev/sync` & `/api/health`, cập nhật `AppState` ở `main.rs`/`plugin.rs`/`operations.rs`. |
| `cms/admin/src/{App.tsx,api/{client,themes}.ts,features/{shared/OwnerOptionsForm.tsx,themes/ThemesPanel.tsx,plugins/PluginsPanel.tsx},shell/AdminShell.tsx,i18n/locales/{en,vi}.ts}` | Modify | UI **Theme Functions**, badge `DEV MODE` từ `/api/health`, nút **"Đồng bộ Dev Manifest"**, `OwnerKind`, và đủ khóa i18n `--strict`. |
| `docs/{architecture/TECH_STACK.md,contracts/{CORE_FOUNDATION,PLUGIN_ABI,M5_THEME_TEMPLATES,M6_PLUGIN_EXTENSIONS,M8_WRITER_INVENTORY}.md,product/PRODUCT_REQUIREMENTS.md}` & `cms/crates/opendash-server/README.md` | Modify | Đồng bộ hợp đồng stack `boa_engine`, Render/Route extensibility, `M8_WRITER_INVENTORY`, `PRODUCT_REQUIREMENTS` §4–5/§7. |
| `cms/crates/opendash-server/tests/{m6_s04_t01_egress_ssrf,unified_js_runtime_s01_test,render_hooks_cache_s02_test,custom_routes_s03_test,theme_functions_dev_e2e_test}.rs` | Modify / Create | Thêm `..Default::default()` cho `m6_s04_t01_egress_ssrf.rs` (`L20`), cùng bộ kiểm chứng tự động 4 tầng phủ trọn CUJ 1, CUJ 2, CUJ 3. |

---

### Slice S01: Unified `boa_engine` Worker, `functions.js`/`plugin.js` Core Hooks, `0053` Storage Isolation & `opendash.d.ts`

* **Dependencies:** None (baseline `main@66ebd73`).
* **File Ownership:** `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-server/Cargo.toml`, `cms/sdk/js/opendash.d.ts`, `cms/crates/opendash-core/src/{hooks.rs,plugin.rs,theme.rs}`, `cms/migrations/0053_theme_functions_runtime.sql`, `cms/crates/opendash-server/src/{main.rs,operations_worker_process.rs,backup_worker.rs,theme.rs,content_render.rs,hooks.rs,plugin/{mod.rs,js_runtime.rs,package.rs,host_ops.rs,worker.rs,runtime.rs,manager.rs,tests.rs,review_regressions.rs},storage/{plugin_kv.rs,plugin_projections.rs,plugin.rs,theme.rs,entries.rs,settings.rs,overrides.rs,admin_extensions.rs},routes/{public_content.rs,themes.rs}}`, `cms/crates/opendash-server/tests/{m6_s04_t01_egress_ssrf.rs,unified_js_runtime_s01_test.rs}`.
* **Công việc chính:**
  1. Pin `boa_engine = "=0.22.0"` trong `Cargo.toml`/`Cargo.lock`; hợp nhất giới hạn RAM OS (`128 MiB` plugin / `512 MiB` operations) trong `operations_worker_process.rs` & `backup_worker.rs` (cùng `GuardedChild` trong `ProcessGuard` & `test_worker_entry`), gọi trước đọc IPC ở `--plugin-worker`; tách `wasmi::StoreLimits` khỏi `HostContext`.
  2. Xây dựng `plugin/js_runtime.rs` (fresh Boa `Context`, `RuntimeLimits` `5_000_000`/`128`/`512`, `globalThis.od.*` trả `null`/data hoặc throw JS `Error`, `od.escapeHtml`/`escapeAttr`, `od.on`/`addFilter`/`addAction`, `validate_js_module_syntax`); viết `cms/sdk/js/opendash.d.ts`.
  3. Mở rộng `ExtensionOwnerKind`, `ExtensionRuntimeKind`, `HookEvent::ThemeAfterActivate`, `ThemeFunctionsSpec` + default quyền đọc công khai & `kv_write` (`sqlite: ["kv_read", "kv_write"]`) cho Active Theme (`content_read_all_public`, `taxonomy_read`, `media_read`, `site_options_read`, `plugin_settings_read`), `render_trusted_html` mặc định `true` (ngoài `uses_v3_features`), cập nhật `manager.rs:355` (`content_read_all_public`) + `to_runtime_manifest()` (cố định `entrypoint: "functions.js"`), `plugin_can_{read,write}_field_for_owner`; thêm `..Default::default()` trên các literal `PluginManifest`/`PluginPermissions` trong `plugin/{host_ops,tests,review_regressions}.rs` và `tests/plugin_egress_ssrf.rs`; cập nhật `v3_manifest()` (`theme.rs:885`) và `spec2_events_helpers_and_envelope_v2_roundtrip` (`hooks.rs:253`).
  4. Tách `validate_wasm_module_bytes` (`plugin/package.rs`); hỗ trợ `plugin.js` (`storage/plugin.rs` ghi/đọc `rev_dir.join(&manifest.entrypoint)` và lưu bytes trong `plugins.wasm_blob`) và `functions.js` (`theme.rs`, `--check-theme`, `ThemeCheckSummary.has_functions`); kết nối Theme `admin_extensions` với `functions.actions`.
  5. Tạo `0053_theme_functions_runtime.sql` (`+0` objects, bỏ FK `plugins(id)` trên `plugin_kv`, `plugin_entry_projections`, `plugin_logs`, tái tạo `idx_theme_option_values_owner`); sửa filter `settings.rs:4614` sang `!name.starts_with("0050_") && name.as_str() < "0053_"` + thêm `migration_0053_schema_delta_is_zero`; sửa mock `CREATE TABLE plugin_kv` trong `plugin/review_regressions.rs` (`L29, 156, 290, 413, 527, 665`) và liệt kê rõ tên cột `INSERT` tại `L165, 297`; cô lập query + wrapper tương thích + fence `(revision_id, selection_version)` + `.complete`/SHA-256 `active_theme_cache` + `theme_activate` extractors (`plugins`, `hooks`, `coordinator`, `recovery`) + `Admin > Plugin > Theme` SEO + `EntryView.theme_ext`.
* **Acceptance Criteria:**
  - [ ] `functions.js` và `plugin.js` chạy không cần build trên `boa_engine`, tái sử dụng đủ 15 Host Ops; tất cả plugin `.wasm` hiện có PASS 100%.
  - [ ] `migration_0050` PASS (`+2`), `migration_0053` PASS (`+0`), `plugin/review_regressions.rs` PASS, fence `(revision_id, selection_version)` chặn `A → B → A`, OS `128 MiB` + `RuntimeLimits` chặn DoS, Safe Mode chặn JS/Wasm nhưng cho phép `theme_activate`.
  - [ ] `unified_js_runtime_s01_test.rs` & toàn bộ `cargo test --workspace --release --locked` PASS 100% (`0` worker spawn trên public `GET`).
* **Rollback:** `git revert` commit Slice S01 (trước khi triển khai production DB đã chạy `0053`).

### Slice S02: Render Hooks (`od_hook`, `filter:render/head`, `filter:render/footer`), `ExtensionRenderCache` & `SiteCacheEpoch::global()`

* **Dependencies:** Slice `S01`.
* **File Ownership:** `cms/crates/opendash-core/src/{hooks.rs,plugin.rs,theme.rs}`, `cms/crates/opendash-server/src/{main.rs,app.rs,cache.rs,template_sec.rs,document.rs,content_render.rs,hooks.rs,plugin/{host_ops.rs,runtime.rs,manager.rs},routes/{public.rs,public_content.rs,seo.rs,plugin.rs,operations.rs},storage/{entries,patterns,taxonomy,content,overrides,menu,settings,theme,plugin,media_lifecycle,import,seo,builder,page}.rs}`, `cms/crates/opendash-server/tests/render_hooks_cache_s02_test.rs`.
* **Công việc chính:**
  1. Thêm `HookEvent::{FilterRenderHead, FilterRenderFooter, FilterRenderSlot}`, `HookExecutionMode::FilterRender`, `RenderCachePolicy`.
  2. Hỗ trợ `{{ od_hook("slot_name", expr?) }}` trong `template_sec.rs` phát placeholder `OD_HOOK_<32hex>`; định nghĩa wire JSON input/output; tách validator C-SEC URL/CSS chặt (`javascript:`/`data:`/`style=`) cho plugin opt-out `render_trusted_html: false` (đồng thời loại bỏ `styles`/`head_scripts` khi `false`; mặc định `true` giữ nguyên HTML/SVG/JS tin cậy); chuẩn hóa từng subscriber độc lập, thế `OD_HOOK_<32hex>` *sau* normalize ngoài và *trước* `finalize_document_icons`; lọc sạch `<title>`/`robots`/`canonical`/`http-equiv`/`json-ld` trong `sanitize_render_head_fragment`.
  3. Chạy `FilterRender` qua `public_render_semaphore` (`4` slots, `200ms` timeout fail-open không cache lỗi, `MAX_RENDER_HOST_CALLS = 64`, mở `kv_list` trong `check_read_policy` với limit `<= 50`, `0` writes, `0` secrets, tối đa `2` `http_request` khi bật `render_cache != None`, timeout `1000ms`/call, phase `1500ms`).
  4. Triển khai `SiteCacheEpoch::global()` trong `AppState`, `PluginRuntime`, và tất cả 14 module `storage/*`; triển khai `ExtensionRenderCache` (`Public`-only, LRU 512, key dùng typed `parse_query` + `"not_found"` chung cho 404, bypass ở Preview/Safe Mode); cập nhật `AppState` ở `main.rs`, `app.rs`, `routes/plugin.rs`, `routes/operations.rs`.
* **Acceptance Criteria:**
  - [ ] `od_hook`, `filter:render/head`, `filter:render/footer` hoạt động mặc định với Theme & Plugin (`render_trusted_html: true`), trong khi plugin đặt `"render_trusted_html": false` vẫn chặn sạch `javascript:`/`style=` XSS.
  - [ ] Core SEO/CSP/IconNonce bất khả xâm phạm; `ExtensionRenderCache` đạt `0` worker spawn khi hit, tự làm mới qua `SiteCacheEpoch::global().bump()`, không dính Preview/404 thrashing, hỗ trợ `http_request` khi có cache và fail-open an toàn khi timeout.
  - [ ] `render_hooks_cache_s02_test.rs` & toàn bộ workspace tests PASS 100%.
* **Rollback:** `git revert` commit Slice S02.

### Slice S03: Scoped Custom Routes (`EnvelopeKind::Route` — `/p/{pkg}/*`, `/api/p/{pkg}/*` & `/api/public/p/{pkg}/*`)

* **Dependencies:** Slice `S02`.
* **File Ownership:** `cms/crates/opendash-core/src/{hooks.rs,plugin.rs,theme.rs}`, `cms/crates/opendash-server/src/{public_context.rs,app.rs,hooks.rs,plugin/{egress.rs,host_ops.rs,js_runtime.rs,runtime.rs,manager.rs},routes/{mod.rs,contact.rs,extensions.rs}}`, `cms/scripts/check-standards.py`, `cms/crates/opendash-server/tests/custom_routes_s03_test.rs`.
* **Công việc chính:**
  1. Thêm `"p"` vào `RESERVED_SYSTEM_ROOTS` (`public_context.rs:14`); hợp nhất `egress::is_forbidden_internal_path` chặn `/api/admin`, `/save`, `/preview`, `/theme`, `/api/p/`, `/api/public/p/`, `/p` (`/p/`) trong cả `egress.rs` & `host_ops.rs` (giữ `"/api/p/"` trong error string cho `host_ops.rs:1172`); mở `kv_list` cho `RouteGet | RoutePost` trong `check_read_policy`.
  2. Kích hoạt `EnvelopeKind::Route`, `HookExecutionMode::{RouteGet, RoutePost}`, `ExtensionRouteDecl` (`RouteAccess::{Public, Session, Admin, Webhook}`, cấm `Get + Webhook`), và `od.addRoute(...)` trong `js_runtime.rs`.
  3. Cài `routes/extensions.rs` (`pub mod extensions;` trong `routes/mod.rs`, `pub(crate) check_strict_contact_origin` trong `routes/contact.rs`) & gắn vào `app.rs` + `cms/scripts/check-standards.py`:
     - `GET /p/{pkg}/{*subpath}` (`RouteGet`, tối đa `64` host calls, `2` HTTP, `1000ms`).
     - `GET` & `POST /api/p/{pkg}/{*subpath}` (`Session | Admin`, `DefaultBodyLimit::max(MAX_PLUGIN_JSON_BYTES)`, `AdmittedMutation(lease)` + gọi trực tiếp `require_session_with_csrf` / `require_admin_with_csrf`, hỗ trợ KV + `content_save`/`content_delete` có `ActionActorFence`).
     - `POST /api/public/p/{pkg}/{*subpath}` (`Public` Origin + RateLimit hoặc `Webhook` RateLimit + `raw_body`/HMAC signature headers; giữ `MutationLease` từ `coordinator.admit_mutation()` xuyên suốt lệnh chạy và chặn `content_save`/`content_delete` với `principal == Public`; thêm quy tắc `PUBLIC_EXTENSION_POST` + negative regression trong `check-standards.py --self-test`).
* **Acceptance Criteria:**
  - [ ] `check-standards.py` (`--self-test`) PASS 100%; SSRF chặn `/p/`, `/api/p/` và `/api/public/p/`.
  - [ ] `GET /p/...` read-only & không rò header auth/cookie; `POST /api/p/...` kiểm đúng session/CSRF (`401`/`403`) và hỗ trợ KV/content có `ActionActorFence`; `POST /api/public/p/...` chạy được AJAX `Public` (kiểm Origin + RateLimit) và `Webhook` (truyền `raw_body` + HMAC headers không cần Origin), ghi được `KV` qua `StagedCommitGuard` + `MutationLease` và từ chối `content_save`/`content_delete` với `principal == Public`; `EnvelopeKind::Cron` tiếp tục bị từ chối.
  - [ ] `custom_routes_s03_test.rs` & workspace tests PASS 100%.
* **Rollback:** `git revert` commit Slice S03.

### Slice S04: Theme Actions & Secrets (`SecretStore`), Backup/Restore `ExtensionSecret`, Dev Live-Edit (`--dev-theme` / `--dev-plugin`), Admin UI, Examples & Docs Closure

* **Dependencies:** Slice `S03`.
* **File Ownership:** `cms/crates/opendash-core/src/{backup.rs,theme.rs}`, `cms/crates/opendash-server/src/{paths.rs,secrets.rs,backup_worker.rs,restore.rs,dev_mode.rs,page.rs,main.rs,app.rs,theme.rs,plugin/manager.rs,storage/{page.rs,theme.rs,overrides.rs,settings.rs,backup.rs,media_refs.rs,restore.rs},routes/{health.rs,themes.rs,operations.rs,plugin.rs,public.rs,public_content.rs,seo.rs,extensions.rs}}`, `cms/admin/src/{App.tsx,api/{client,themes}.ts,features/{shared/OwnerOptionsForm.tsx,themes/ThemesPanel.tsx,plugins/PluginsPanel.tsx},shell/AdminShell.tsx,i18n/locales/{en,vi}.ts}`, `cms/examples/{themes/theme-js-starter/,plugins/plugin-js-example/}`, `cms/migrations/README.md`, `cms/crates/opendash-server/README.md`, `docs/{architecture/TECH_STACK.md,contracts/{CORE_FOUNDATION,PLUGIN_ABI,M5_THEME_TEMPLATES,M6_PLUGIN_EXTENSIONS,M8_WRITER_INVENTORY}.md,product/PRODUCT_REQUIREMENTS.md}`, `cms/crates/opendash-server/tests/theme_functions_dev_e2e_test.rs`.
* **Công việc chính:**
  1. `SitePaths::theme_secrets_dir`, `secrets.rs:71` (`plugins | themes`), `media_refs.rs` (`theme_functions_option`, `theme_kv`, parse riêng `{"secret_ref": "<64-hex>"}`), `BackupFileKind::ExtensionSecret` (`backup_worker.rs`, `restore.rs` vào `database/secrets/`, `storage/restore.rs` kiểm riêng `ExtensionSecret` vs SMTP), và 4 endpoints Theme Functions (`/api/admin/themes/active/functions/{settings,logs}`, `/api/admin/themes/active/actions/{action_id}`).
  2. Xây dựng `dev_mode.rs` + `--dev-theme` / `--dev-plugin` (`main.rs`), khởi tạo `DevOverlayConfig::global()`, từ chối khi `recovery_safe_mode == true` hoặc `trusted_public_origin.is_some()` chưa bật `--allow-dev-remote`, tắt động nếu Safe Mode kích hoạt, hỗ trợ `dev-<64-hex>` trong `resolve_enqueued_assets`, `theme_asset` (`no-store`), `capture_active_theme_v3_in_conn`/`load_active_theme_v3_in_conn`, `storage/page.rs::load_unified_block_catalog_in_conn` & `page.rs::BlockTemplateCache::get_or_validate`, `PluginManager`, `GET /api/health` (`dev_mode`), `POST /api/admin/dev/sync`, và `create_backup` snapshot sạch kèm cảnh báo `dev_mode_active: true`.
  3. Cập nhật Admin UI (`App.tsx`, `AdminShell.tsx`, `ThemesPanel.tsx`, `PluginsPanel.tsx`, `OwnerOptionsForm.tsx`, `i18n/locales/{en,vi}.ts`), tạo ví dụ `theme-js-starter` & `plugin-js-example`, cập nhật docs/contracts/`M8_WRITER_INVENTORY.md`/`PRODUCT_REQUIREMENTS.md`, và hoàn thiện `theme_functions_dev_e2e_test.rs`.
* **Acceptance Criteria:**
  - [ ] Theme Secrets (`GET`/`PUT` redact `{"configured": bool}`, `{"clear": true}`), Theme Actions (RBAC + Safe Mode `503`), và Backup/Restore `ExtensionSecret` vào `database/secrets/` PASS 100%.
  - [ ] `--dev-theme` / `--dev-plugin` hot-reload HTML/CSS/assets/blocks/`functions.js`/`plugin.js` (`no-store`), chặn symlink/`.env`/`.git` & `trusted_public_origin`, đồng bộ schema qua `POST /api/admin/dev/sync`, và hiển thị `DEV MODE` trong Admin.
  - [ ] `check-standards.py` (`--self-test`), `check-plans.py` (`--self-test`), `check-agent-artifacts.py`, `node cms/scripts/check-i18n.mjs --strict`, `npm --prefix cms/admin run typecheck && build`, và `cargo test --workspace --release --locked` PASS 100%.
* **Rollback:** `git revert` commit Slice S04.

---

### Verification Plan

Chạy đầy đủ từ gốc repository sau mỗi slice:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
git diff --check
```

Kèm kiểm chứng bổ sung ở Slice S04:
- `node cms/scripts/check-i18n.mjs --strict`
- `npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build`
- `cargo test --manifest-path cms/Cargo.toml --release --locked --test unified_js_runtime_s01_test --test render_hooks_cache_s02_test --test custom_routes_s03_test --test theme_functions_dev_e2e_test`
- `opendash --check-theme cms/examples/themes/theme-js-starter` (exit `0`, `"has_functions": true`) và `opendash --check-plugin cms/examples/plugins/plugin-js-example` (exit `0`).
