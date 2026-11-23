# OpenDash Studio UI/UX Upgrade — EGM Closure Verification (`#504`–`#506`, 2026-11-23)

- **Design Spec:** [`docs/designs/studio-builder-ux-upgrade.md`](../designs/studio-builder-ux-upgrade.md)
- **Drift Report:** [`.agents/research/2026-11-23_STUDIO_UX_UPGRADE_DRIFT_REPORT.md`](../../.agents/research/2026-11-23_STUDIO_UX_UPGRADE_DRIFT_REPORT.md) (`PASS: 0 Blocker · 0 Major · 2 Minor · 2 Nit`)
- **Evaluated `origin/main` SHA:** `e71222f9d54f6b2c1e39e9f1df5d5946686fc2d3`
- **Merged Implementation PRs:**
  - Slice S01 (`#504`): PR `#505` (`feat(studio): contextual sidebar builder UX (S01, #504)`, merge `268f2a5f`) — implemented & merged by Harbor
  - Slice S02 (`#506`): PR `#507` (`feat(studio): interactive canvas gear and sidebar menu bridge (S02, #506)`, merge `e71222f9`) — implemented & merged by Harbor
- **Environment:** macOS (Intel x86_64), exact-main clean checkout (`e71222f9d54f6b2c1e39e9f1df5d5946686fc2d3`), release binary `./cms/target/release/opendash` (SHA-256: `3c52e321d811a80c69ca2c03e552b50a5db1338d2d38d49a66fc7e290198ea4e`), live server on `http://127.0.0.1:18766`.
- **Verifier:** Orion (Coordinator) + fresh-context Goldfish (`1b56d55c`) + Live Browser E2E (`51db19d0`).
- **Overall Result:** **`PASS`** — promotes `docs/designs/studio-builder-ux-upgrade.md` to **`Verified`**.

---

## 1. Automated Verification on Exact `origin/main@e71222f9`

| Check / Command | Scope | Result | Evidence / Summary |
| --- | --- | --- | --- |
| `node cms/admin/scripts/test-studio-ux.mjs` | Studio UX logic, AST invariants, `BRIDGE_SCRIPT_SHA256`, no `innerHTML` | **PASS** | `15/15` passed, `0` failed (`26.5ms`) |
| `npm --prefix cms/admin run typecheck` | TypeScript strict check (`tsc --noEmit`) | **PASS** | Exit `0`, `0` errors |
| `npm --prefix cms/admin run build` | Production Admin bundle (`tsc && vite build`) | **PASS** | Exit `0`, `111` modules (`dist/assets/index-CbymGgzG.css`, `dist/assets/index-BXwXG6bs.js`) |
| `python3 cms/scripts/check-standards.py` + `--self-test` | Repo coding & mutation standards | **PASS** | `82/82` mutation handlers resolved, self-test PASS |
| `python3 cms/scripts/check-plans.py` + `--self-test` | Planning & EGM doc guardrails | **PASS** | `13` fixtures PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` | Agent metadata & workflow guardrails | **PASS** | Exit `0` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Rust formatting | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --release --locked` | Embedded Admin + Single-Binary Rust release build | **PASS** | Exit `0`; SHA-256 `3c52e321d811a80c69ca2c03e552b50a5db1338d2d38d49a66fc7e290198ea4e` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | Full Rust workspace unit & integration tests | **PASS** | `79` suites · `410` passed · `0` failed · `0` ignored |
| `git diff --stat 19e21f4..e71222f` | Rust/DB zero-regression scope check | **PASS** | `13` files (`12` in `cms/admin/**`, `1` doc line); `0` files in `cms/crates/` or `cms/migrations/` |

> [!NOTE]
> Trên macOS, `cargo test` được chạy với `TMPDIR=/private/tmp` để tránh `/var` symlink kích hoạt symlink security guard trong `operations::tests` và `paths::tests`.

---

## 2. Live Browser E2E Verification (`http://127.0.0.1:18766/admin/`)

Chạy trên binary release đã build mới (`3c52e321...`) bằng phiên trình duyệt thực (`51db19d0`, host-verified origin `http://127.0.0.1:18766`, `blocked_origins: none`):

| Scenario | Assertions Verified | Result |
| --- | --- | --- |
| **Mode 1 (`#/studio`)** | `[data-testid="studio-topbar"]` = `0`; `[data-testid="studio-sidebar-header"]` = `1` (`OpenDash Studio` + `Thoát`); Footer có đủ `Desktop` / `Tablet` / `Mobile` + Save; không có breadcrumb hay drawer header trùng lặp; mở `Trang chủ (Home)` hiển thị cây `Cấu trúc trang` mặc định (không có 3 tab `Blocks`/`Patterns`/`Media`), có `+ Thêm Section`, `Patterns` và `Media` ở đáy cây; `#canvas` có `0` nút `+ Add`, `2` `.od-canvas-gear` chỉ trên `Section`/`Element` (`0` trên `Row`/`Column`). | **PASS** |
| **Mode 2 (`#/studio/entry/page/8`)** | Header 2 vùng (`OpenDash Studio` + `Thoát` / `← Về danh sách trang` + `Studio E2E Test Page` + badge `Page`); `+ Thêm Section` mở màn `add` với `Section trống` + `Chọn từ Thư viện Pattern…`; thêm `Section 1` → `Row 2 cột` → `Tiêu đề` (`core/heading`). | **PASS** |
| **Interactive Menu & Properties** | Mở menu từ cả icon bánh răng trên cây lẫn `.od-canvas-gear` trong `#canvas` (`Chỉnh sửa`, `Nhân bản`, `Copy options`, `Paste options`, `Di chuyển lên`, `Di chuyển xuống`, `Xóa`); `Chỉnh sửa` chuyển sidebar sang `properties` chỉ hiển thị field + `Field Binding`, tiêu đề `← Cấu trúc trang`, `0` nút Delete và `0` nút Duplicate. | **PASS** |
| **Duplicate, Copy/Paste, Undo/Redo, Viewport & Draft Save** | `Nhân bản` nhân đôi `Tiêu đề` (`1 → 2`); sửa tiêu đề 1 thành `Tiêu đề 1 (Updated)` → `Copy options` → `Paste options` sang tiêu đề 2 (cả hai render đồng bộ trong `#canvas`); `Undo` hoàn tác tiêu đề 2, `Redo` áp dụng lại; chuyển Viewport cập nhật `#canvas` (`Desktop 694px` → `Tablet 834px` → `Mobile 390px` → `Desktop 694px`); `Lưu bản nháp` (`PUT /api/admin/entries/page/8/builder`) trả **HTTP 200** và giữ trạng thái `draft`. | **PASS** |
| **Console & CSP Health** | `0` Console Errors, `0` CSP Violations trong toàn bộ luồng. | **PASS** |

---

## 3. Observed Non-Blocking Notes & Follow-ups

1. **Drift Report (`1b56d55c`) — `0 Blocker / 0 Major / 2 Minor / 2 Nit`:** Chi tiết ghi nhận tại [`.agents/research/2026-11-23_STUDIO_UX_UPGRADE_DRIFT_REPORT.md`](../../.agents/research/2026-11-23_STUDIO_UX_UPGRADE_DRIFT_REPORT.md).
2. **Pre-existing Inspector/Backend behavior observed during E2E (ngoài phạm vi S01/S02):**
   - Xóa trống trường số `Cấp tiêu đề (1–6)` khi đang gõ gửi giá trị rỗng tới `/builder/preview` (HTTP 400) khiến Inspector tạm hiển thị thông báo unresolved trước khi nhập số hợp lệ hoặc bấm `Undo`.
   - Sau khi `Lưu bản nháp`, template mặc định của `page` render song song nội dung Builder và phần fallback `Nội dung` plain text ở phía dưới (hành vi có sẵn của renderer/theme trước S01/S02).
3. **Giới hạn nền tảng:** Live Browser E2E và Rust release test trong biên bản này thực chạy trên **macOS Intel**; Windows/Linux CI (`cms-ci`, `governance-*`) kiểm chứng tự động trên GitHub Actions cho mọi PR (`#505`, `#507` và Closure PR).
