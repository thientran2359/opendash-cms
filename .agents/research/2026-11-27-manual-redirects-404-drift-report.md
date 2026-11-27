# EGM Drift Report — Manual Redirects (301/302), CSV Import/Export & 404 Log (2026-11-27)

- **Design Spec:** [`docs/designs/manual-redirects-and-404-log.md`](../../docs/designs/manual-redirects-and-404-log.md)
- **Target Branch / Worktree:** `feat/manual-redirects-and-404-log` (`opendash-redirects-404`)
- **Final Goldfish Verdict:** **`PASS — Ready for Verified`** (`0 Blocker / 0 Major / 0 Minor` sau vòng kiểm tra lại thứ 2)

---

## 1. Fresh-Context Goldfish Rounds Summary

| Round | Subagent ID | Scope Audited | Verdict (Blocker / Major / Minor / Nit) | Key Outcome |
| --- | --- | --- | --- | --- |
| **Round 1** | `18ff02c8` (`egm-codebase-details`) | Toàn bộ Spec S01–S03, `PRODUCT_REQUIREMENTS.md` §3/§6, `ADMIN_NAVIGATION_AND_EDIT_SCREEN.md` §2, `CORE_FOUNDATION.md`, `M8_WRITER_INVENTORY.md`, `0056`, `opendash-core`, `opendash-server`, `cms/admin`, tests & READMEs | `0 Blocker / 2 Major / 4 Minor / 5 Nit` | Code lõi, bảo mật (`%2F`/`%252F`/`%2e%2e`/`%61`), khóa `MutationLease -> flush_mutex -> TelemetryMaps -> SQLite`, SQL boundary, fetch boundary & LOC `<= 600` đạt; phát hiện 2 Major lệch tên ký hiệu doc (`M8_WRITER_INVENTORY.md`, `api/README.md`) và 4 Minor (`site_public_settings.public_origin`, đếm submodule/migrations, ký hiệu `redirectsDraftStore.ts`). |
| **Round 2** | `03914088` (`egm-codebase-details`) | Đối chiếu toàn bộ 7 mục vá drift & sửa lỗi sau Chrome E2E (`ManualRedirectsPanel`, `NotFoundLogPanel`, `storage/redirects`, `crud_and_precedence.rs`, `run-tests.py`, các README & Contracts) | **`0 Blocker / 0 Major / 0 Minor / 1 Nit`** *(cùng 1 Nit nền)* | Khẳng định 100% Major #1–#2, Minor #1–#4, post-E2E bugfixes & Nits đã được khắc phục đúng; Nit #1 duy nhất còn lại (`cms/README.md:35` ghi `36` thay vì `37` scripts) đã sửa trực tiếp ngay sau báo cáo. |

---

## 2. Remediations Applied

1. **Major #1 (`docs/contracts/M8_WRITER_INVENTORY.md`):** Cập nhật hàng `Manual Redirects & 404 Log` dùng đúng tên handler (`routes::redirects::{rules::{create_redirect, update_redirect, delete_redirect}, csv::import_redirects, not_found_logs::{delete_not_found_log_handler, bulk_delete_not_found_logs_handler, clear_not_found_logs_handler}}`) và `RedirectTelemetryBuffer::flush_now()` tại [`main.rs`](../../cms/crates/opendash-server/src/main.rs).
2. **Major #2 (`cms/admin/src/api/README.md`):** Đồng bộ chính xác 11 hàm export trong [`redirects.ts`](../../cms/admin/src/api/redirects.ts) (`importRedirectsCsv`, `exportRedirectsCsvBlob`, `bulkDeleteNotFoundLogs`, `clearNotFoundLogs`, ...).
3. **Minor #1 (`cms/crates/opendash-server/src/storage/redirects/`):** Thêm `resolve_site_origin_in_conn` ưu tiên `site_public_settings.public_origin` (rồi fallback header HTTP) khi kiểm tra self-redirect và vòng lặp/chuỗi hop trong `create_manual_redirect`, `update_manual_redirect_cas`, `import_manual_redirects_csv`, kèm regression test trong [`crud_and_precedence.rs`](../../cms/crates/opendash-server/tests/site_media_seo/manual_redirects_404/crud_and_precedence.rs).
4. **Minor #2–#4 & Nits (`CORE_FOUNDATION.md`, `cms/README.md`, `cms/migrations/README.md`, `storage/README.md`, `features/README.md`, `NotFoundLogPanel.tsx`, `run-tests.py`):**
   - Sửa số lượng storage domain submodule (`34` domain submodules / `35` module files) và migrations (`55` file SQL `0001`–`0056`), đồng bộ `37` scripts trong `cms/README.md`.
   - Sửa tham chiếu symbol trong `features/README.md` sang `getRedirectsDraft` / `setRedirectsDraft` / `discardRedirectsDraft` / `clearRedirectsDrafts`.
   - Hiển thị đồng thời `last_referrer` và `last_user_agent` ở cột `Referrer / User-Agent` trên `NotFoundLogPanel.tsx`.
   - Bổ sung `i18n_redirects.rs` và `i18n_tests.rs` vào `server_routes` trong `run-tests.py`.
   - Vá lỗi tải dữ liệu khi chuyển tab `Chuyển hướng` ↔ `Nhật ký 404` qua hash và dùng `onApiErrorRef` tránh fetch lại mỗi lần gõ phím.
