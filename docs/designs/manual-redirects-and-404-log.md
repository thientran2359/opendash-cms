# Quản lý Redirect Thủ công (301/302), Nhập/Xuất CSV & Nhật ký 404

* **Author:** Orion (Elephant / Antigravity)
* **Status:** Approved
* **Date:** 2026-11-26
* **Baseline:** `origin/main@43c19c2` (đã gồm `0055_theme_option_locale_media_refs.sql` + chuẩn module hóa Feature-First `<= 600 LOC`)
* **Approved By:** Owner (chỉ đạo trực tiếp trong phiên chat 2026-11-27: *"pull code mới nhất về và triển khai tiếp đi"*, kèm Proposed PRD Delta)
* **Approval Evidence:** Owner phê duyệt kèm `Proposed PRD Delta` và kết quả Goldfish sạch qua 5 vòng kiểm tra độc lập (0 Blocker / 0 Major)
* **Goldfish Review:** Round 4 (`egm-implementation-readiness`: 0 Blocker / 0 Major) & Round 5 (`egm-codebase-details`, `egm-design` trên `origin/main@43c19c2`: 0 Blocker / 0 Major sau vá literal `**File Ownership:**`, bỏ `"state.rs"` khỏi `server_routes` và khớp dải dòng `fields.*`) PASS.
* **Goldfish Review History:**
  - Round 1 (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, fresh-context trên `origin/main@646fcf1`, 2026-11-26): xử lý khóa `SiteCoordinator`, Axum layer sau `.fallback()`, bảo vệ `RESERVED_SYSTEM_ROOTS`, RAM draft store qua `401`, `shadowed_by_live_route`, tách lỗi S01/S02.
  - Round 2 (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, fresh-context, 2026-11-26): xử lý quy tắc `%XX` nghiêm ngặt (`%20` + UTF-8 `>= 0x80`), dịch `CsvPreviewRow.error`, `main.rs` pre-drain flush, flush+delete `older_than_days`, CSV aliases & `visited` guard.
  - Round 3 (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, fresh-context, 2026-11-26): xử lý `tm.kind` + `definition_json` trong `find_live_claim_kinds_for_paths_in_conn`, thêm `PrelocalizedErrorResponse` trong `i18n::localize_error_response` + mã dotted `redirects.csv_import_*`, tách giới hạn `512`/`2048` ra khỏi `decode_safe_legacy_request_path`, chi tiết `exclude_id` trên `PUT`, `telemetry_drop_guard` ownership và `App.tsx` guards. Chạy Round 4 để chốt `0 Blocker / 0 Major`.
* **Related Contracts & Specs:**
  * [Yêu cầu Sản phẩm V1](../product/PRODUCT_REQUIREMENTS.md)
  * [Admin Navigation & Edit Screen](../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md)
  * [Core Foundation & Quy chuẩn Code](../contracts/CORE_FOUNDATION.md)
  * [M7 Site Features & SEO](../contracts/M7_SITE_FEATURES.md)
  * [M8 Writer & Recovery Inventory](../contracts/M8_WRITER_INVENTORY.md)
  * [WordPress-Style Permalinks (Phase 1)](wordpress-style-permalinks.md)
  * [Design Sitemap](SITEMAP.md)

---

## Overview

Đặc tả này bổ sung bộ công cụ **Quản lý Redirect thủ công (301/302), Nhập/Xuất CSV & Nhật ký 404** cho OpenDash CMS, giúp quản trị viên chuyển đổi (migrate) website cũ (WordPress, PHP, HTML tĩnh, hệ thống cũ nhiều cấp thư mục hoặc URL có dấu) sang OpenDash mà không mất thứ hạng SEO hay gãy liên kết cũ.

Toàn bộ giao diện quản trị được đặt trong nhóm **`Hệ thống → Công cụ → Chuyển hướng & 404`** (`#/tools/redirects`, hỗ trợ deep link `#/tools/404-log`), chỉ dành cho `Admin`, dùng chung chuẩn bảo mật CSRF/CAS/`AdmittedMutation` + RAM draft store qua `401`/`412`, lưu trữ SQLite thuần túy (`+6` schema objects, giữ tổng `sqlite_schema` dưới trần `256`), không thêm crate Rust hay package npm bên ngoài, và tích hợp trực tiếp vào bộ định tuyến công khai [`public_fallback`](../../cms/crates/opendash-server/src/routes/seo.rs).

---

## The Problem

1. **Chưa có bảng Redirect thủ công & nhập CSV khi chuyển website cũ:**
   - OpenDash hiện chỉ phát sinh redirect `301` tự động cho nội dung đã từng tồn tại trong OpenDash thông qua bảng `permalink_route_redirects` ([`storage/permalinks/`](../../cms/crates/opendash-server/src/storage/permalinks/mod.rs)) và `entry_redirects` ([`storage/seo/`](../../cms/crates/opendash-server/src/storage/seo/mod.rs)).
   - Quản trị viên không thể khai báo redirect `301`/`302` tùy ý cho các URL từ website cũ (ví dụ `/san-pham/ao-thun.html`, `/index.php`, `/blog/2023/10/old-post`, `/Tin_Tuc_Cu`, `/tin-t%E1%BB%A9c`) tới trang mới hoặc tới tên miền/URL ngoài, cũng không có công cụ nhập/xuất hàng loạt bằng CSV.
2. **Validator đường dẫn công khai & custom path hiện tại quá chặt cho URL cũ:**
   - [`validate_custom_entry_path`](../../cms/crates/opendash-server/src/seo.rs) chỉ chấp nhận `1..=5` segment ASCII `kebab-case` và cấm dấu `.`, chữ hoa, `_`.
   - [`validate_public_request_path`](../../cms/crates/opendash-server/src/public_context.rs) từ chối mọi ký tự `%`, nên ngay cả URL legacy chứa UTF-8 percent-encoded (như `/tin-t%E1%BB%A9c`) cũng trả `400 Bad Request` trước khi kịp tra redirect. Tuy nhiên không thể nới lỏng `validate_public_request_path` một cách mù quáng vì C3 yêu cầu chống path aliasing (`%2F`, `%252F`, `%2e%2e`, `%61dmin` phải tiếp tục bị chặn `400`).
   - Ngoài ra, [`is_reserved_system_root`](../../cms/crates/opendash-server/src/public_context.rs) hiện so sánh phân biệt chữ hoa-thường, nên `/Admin/*` không bị loại trừ ở `public_fallback` nếu không chuẩn hóa không phân biệt hoa-thường trước và sau khi giải mã.
3. **Chưa có Nhật ký 404 để phát hiện liên kết gãy sau khi golive:**
   - Khi trả `404` tại [`render_not_found_response_for_locale`](../../cms/crates/opendash-server/src/routes/public_content/mod.rs), hệ thống chưa ghi nhận path nào đang bị lỗi, số lần truy cập hoặc referrer nguồn. Quản trị viên không biết URL cũ nào bị sót để tạo redirect bổ sung.
   - Nếu ghi SQLite đồng bộ trên mọi request `GET` 404 hoặc redirect hit, hệ thống sẽ bị nghẽn ghi (`SQLITE_BUSY`), phình bảng nếu gặp bot scan và cản trở cửa sổ khóa bảo trì `SiteCoordinator::begin_maintenance()` ([`operations.rs`](../../cms/crates/opendash-server/src/operations.rs)).
4. **Ràng buộc ngân sách Schema SQLite (`MAX_SCHEMA_OBJECTS = 256`):**
   - [`validate_restored_db_read_only`](../../cms/crates/opendash-server/src/storage/restore.rs) từ chối bản sao lưu vượt quá `256` đối tượng trong `sqlite_schema`. Baseline `origin/main@43c19c2` hiện có `221` đối tượng từ migrations `0001`–`0055` (`80` tables + `124` indexes + `17` triggers, vì `0055` sửa PK cộng ròng `+0` object; tức `223` đối tượng thực tế trong `sqlite_schema` khi tính thêm bảng `_sqlx_migrations` và `sqlite_autoindex__sqlx_migrations_1`), còn dư `33` chỗ trống; schema mới bắt buộc phải gọn và nằm thoải mái dưới ngưỡng an toàn.

---

## Goals

- **G1 — Quản lý Redirect thủ công (301/302):** Hỗ trợ CRUD đầy đủ (`source_path`, `target_url`, `status_code: 301 | 302`, `enabled`, `note`), đếm `hit_count` & `last_hit_at`, tìm kiếm/lọc theo trạng thái/mã chuyển hướng/từ khóa, cảnh báo chính xác khi `source_path` bị che bởi một route công khai thực sự đang hoạt động (`shadowed_by_live_route` khớp ngữ nghĩa hiển thị của `resolve_public_path`), và kiểm tra CAS `If-Match` / `ETag` khi sửa.
- **G2 — Nhập CSV có Dry-Run Preview & Xuất CSV an toàn:** Cho phép nhập từ file `.csv` hoặc dán nội dung CSV tối đa `2 MiB` / `5.000` dòng mỗi lần, có bước `dry_run` xem trước kết quả từng dòng (hợp lệ / trùng / lỗi cú pháp hoặc vòng lặp, hiển thị đúng ngôn ngữ Admin `en`/`vi`) và chế độ xử lý trùng (`skip`, `overwrite`, `error`) trước khi ghi nguyên tử vào DB; xuất toàn bộ bảng redirect ra `.csv` có UTF-8 BOM (`\xEF\xBB\xBF`) tải bằng `requestBlob` và chống CSV formula injection (`=`, `+`, `-`, `@`, `\t`, `\r`, `\n`). Cả parser lẫn exporter viết thuần Rust trong `opendash-core` (**0 dependency mới**).
- **G3 — Nhật ký 404 gộp theo path & Tạo Redirect 1-click:** Ghi gộp 1 bản ghi cho mỗi `path` duy nhất (`hit_count`, `first_seen_at`, `last_seen_at`, `last_referrer`, `last_user_agent`; **không lưu IP**), tự loại trừ tiền tố hệ thống (kể cả viết hoa và `%XX`) và `/`, trần cứng `5.000` dòng (tự cắt dòng ít/cũ nhất), gom ghi qua bộ đệm RAM giới hạn + flusher bất đồng bộ tôn trọng thứ tự khóa `MutationLease → flush_mutex → TelemetryMaps → SQLite`, cho phép tạo redirect `301`/`302` 1-click tại chỗ (tự xóa dòng 404 và evict khỏi RAM) và xóa từng dòng / xóa hàng loạt / dọn log.
- **G4 — Giao diện Admin tại `Công cụ → Chuyển hướng & 404`:** Đặt mục con mới dưới menu `Công cụ` (`#/tools/redirects` và `#/tools/404-log`), chỉ `Admin` truy cập được (`Editor` bị chặn `403` cả UI và API), bảo toàn bản nháp trong bộ nhớ RAM (`Map` keyed theo `${siteInstanceId}:${user.id}`) khi gặp `401`/`412`, song ngữ `en`/`vi` đầy đủ theo chuẩn `check-i18n.mjs --strict`.
- **G5 — Thứ tự ưu tiên an toàn & Bảo toàn Backup/Restore:** Bảo đảm bất biến `Route hệ thống > Route nội dung đang publish > Redirect thủ công > Auto-301 cũ > 404`, giữ nguyên các test bảo mật chống `%2F`/`%252F`/`%2e%2e`/`%61`, chặn vòng lặp & chuỗi `> 3` hop theo cả hai chiều, chặn open redirect `//`, và chỉ tăng `+6` schema objects (`221 → 227` migration objects, tức `223 → 229 < 256` tổng `sqlite_schema` khi tính cả `_sqlx_migrations`) đồng bộ với `restore.rs`.

## Non-Goals

- Không hỗ trợ pattern wildcard/prefix hoặc regex redirect trong V1 (ví dụ `/old-blog/*` hoặc `^/news/(.*)$`). V1 chỉ khớp **exact normalized path** để giữ tra cứu `O(log N)` trên PK/UNIQUE index và kiểm tra vòng lặp tất định.
- Không hỗ trợ mã HTTP ngoài `301` và `302` (chẳng hạn `307`, `308`, `410 Gone`, `451`).
- Không ghi log truy cập từng request rời rạc (raw access log), không lưu địa chỉ IP khách truy cập, không làm analytics thay thế công cụ phân tích lưu lượng.
- Không cho phép redirect thủ công ghi đè trang/bài viết đang xuất bản (`permalink_route_claims` còn publish/public luôn thắng); nếu muốn chuyển hướng một bài viết đang mở, Admin phải chuyển bài sang nháp/Thùng rác hoặc đổi slug bài đó.
- Không mở quyền quản lý redirect/404 cho `Editor`, không bổ sung Plugin Host Ops mới, không tạo cron background job cho người dùng cấu hình, và không thêm crate Rust hoặc package npm ngoài vào `cms/Cargo.toml` / `cms/admin/package.json`.

---

## Proposed PRD Delta (`docs/product/PRODUCT_REQUIREMENTS.md` & `docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`)

> Ghi chú cổng phê duyệt: Vì phần này bổ sung một mục menu con dưới `Công cụ` (`PRODUCT_REQUIREMENTS.md` §3 & `ADMIN_NAVIGATION_AND_EDIT_SCREEN.md` §2) và mở rộng khả năng SEO/Redirect (`PRODUCT_REQUIREMENTS.md` §6), Spec cần **`Approved By: Owner`** trước khi sửa các tài liệu sản phẩm và triển khai code.

1. **`docs/product/PRODUCT_REQUIREMENTS.md` §3 (`Điều hướng Admin & Màn hình Single Edit`, nhóm `Hệ thống → Công cụ`):**
   - Bổ sung `Chuyển hướng & 404` vào danh sách màn hình con của `Công cụ` (cùng với `Sao lưu & Phục hồi`, `Đối soát media`, `Tình trạng hệ thống`).
2. **`docs/product/PRODUCT_REQUIREMENTS.md` §6 (`Tính năng tích hợp sẵn — Liên hệ & SEO`):**
   - Bổ sung cuối câu SEO: hỗ trợ bảng quản lý redirect thủ công (`301`/`302`), nhập/xuất danh sách CSV (có xem trước trước khi ghi) và nhật ký URL bị `404` kèm thao tác chuyển nhanh thành redirect.
3. **`docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md` §2 (hàng `Hệ thống | Công cụ`):**
   - Bổ sung `Chuyển hướng & 404 (#/tools/redirects)` (alias `#/tools/404-log`), quyền `Admin`.

---

## Critical User Journeys (CUJs)

### CUJ 1: Tạo Redirect thủ công 301/302 cho URL legacy và chuyển hướng công khai
- **Given:** Quản trị viên đăng nhập Admin, mở `Công cụ → Chuyển hướng & 404` (`#/tools/redirects`). Website có một trang đang xuất bản tại `/lien-he`, một bài viết tại `/tin-tuc`, nhưng website cũ dùng `/Lien_He.html` và `/tin-t%E1%BB%A9c`; ngoài ra có một rule `/cu → /tin-tức` và `/promo → https://partner.example.com/sale` (`302`).
- **When:** Quản trị viên tạo 4 rule: `/Lien_He.html → /lien-he (301)`, `/tin-t%E1%BB%A9c → /tin-tuc (301)`, `/cu → /tin-tức (301)` và `/promo → https://partner.example.com/sale (302)`. Sau đó khách truy cập `GET /Lien_He.html/?utm_source=fb`, `GET /tin-t%E1%BB%A9c`, `GET /cu`, và `GET /promo`.
- **Then:**
  - Server chuẩn hóa `source_path` bỏ trailing slash và giải mã UTF-8 an toàn (`/tin-tức`), lưu thành công.
  - Khách truy cập `GET /Lien_He.html/?utm_source=fb` nhận `301 Moved Permanently` với `Location: /lien-he?utm_source=fb` và `Cache-Control: no-store`.
  - Khách truy cập `GET /tin-t%E1%BB%A9c` nhận `301 Moved Permanently` với `Location: /tin-tuc` và `Cache-Control: no-store`.
  - Khách truy cập `GET /cu` nhận `301 Moved Permanently` với `Location: /tin-t%E1%BB%A9c` (tên đường dẫn UTF-8 được percent-encode chuẩn ASCII trong header `Location`, đảm bảo `HeaderValue::from_str` hợp lệ).
  - Khách truy cập `GET /promo` nhận `302 Found` với `Location: https://partner.example.com/sale` và `Cache-Control: no-store`.
  - Bộ đệm `RedirectTelemetryBuffer` ghi nhận lượt hit `GET` (bỏ qua `HEAD`) và đồng bộ tăng `hit_count` + `last_hit_at` cho từng rule.

### CUJ 2: Nhập hàng loạt Redirect từ CSV (Dry-Run Preview) & Xuất CSV
- **Given:** Quản trị viên có tệp CSV migrate gồm 200 dòng (kết hợp dòng hợp lệ có `note` chứa dấu phẩy, dòng có BOM UTF-8, dòng trùng `source_path` với rule sẵn có, và dòng lỗi như vòng lặp hoặc `target_url` dùng scheme `javascript:`).
- **When:** Quản trị viên chọn tệp CSV trên tab `Chuyển hướng`, chạy **Xem trước (Dry-Run)**, kiểm tra tóm tắt và chi tiết từng dòng lỗi/trùng (hiển thị đúng ngôn ngữ CMS đang dùng), chọn chế độ xử lý trùng `Bỏ qua dòng trùng` (`skip`) hoặc `Ghi đè dòng trùng` (`overwrite`), sửa/loại bỏ dòng lỗi rồi bấm **Xác nhận nhập**, sau đó bấm **Xuất CSV**.
- **Then:**
  - Bước `dry_run` chỉ đọc snapshot DB, không ghi bất kỳ dòng nào vào DB.
  - Khi xác nhận nhập ở chế độ `skip`/`overwrite` với `error_count == 0`, giao dịch SQLite `BEGIN IMMEDIATE` áp dụng toàn bộ nguyên tử (nếu còn dòng lỗi hoặc xung đột ở chế độ `error`, trả JSON `CsvImportResponse` kèm mã `422`/`409` và rollback 100%; nếu `overwrite`, giữ nguyên `id`, `hit_count`, `last_hit_at`, `created_at` và tăng `version`).
  - File CSV xuất ra (`GET /api/admin/redirects/export`, tải bằng `requestBlob`) có BOM UTF-8 (`\xEF\xBB\xBF`), chứa đủ cột chuẩn, escape công thức (`'`) cho cả giá trị bắt đầu bằng `\t`/`\r`/`\n` lẫn chuỗi sau `trim_start()` bắt đầu bằng `=`, `+`, `-`, `@`, `\t`, `\r`, `\n`, và có thể nhập ngược lại sạch sẽ (bỏ qua cột `hit_count`, `last_hit_at`, giữ nguyên dấu `'` bảo vệ ở `note`).

### CUJ 3: Theo dõi Nhật ký 404 và Tạo Redirect 1-click
- **Given:** Khách truy cập `GET /san-pham-cu.html` 4 lần và `GET /khuyen-mai-2023` 1 lần khi các URL này chưa có trang và chưa có redirect; đồng thời crawler gọi `GET /admin/khong-ton-tai`, `GET /Admin/wp-login.php` và `GET /%61dmin/config`.
- **When:** Quản trị viên mở `Công cụ → Chuyển hướng & 404` rồi chuyển sang tab `Nhật ký 404` (hoặc truy cập trực tiếp `#/tools/404-log`), xem danh sách 404, sau đó bấm nút **Tạo Redirect** ngay trên dòng `/san-pham-cu.html`, chọn đích `/san-pham` (`301`) và lưu.
- **Then:**
  - Request `GET` công khai trả về trang `404` của theme mà không thực hiện `INSERT`/`UPDATE` SQLite trực tiếp trên luồng request.
  - Khi Admin mở tab `Nhật ký 404`, backend tự động `flush_now()` và hiển thị đúng `/san-pham-cu.html` (`hit_count = 4`), `/khuyen-mai-2023` (`hit_count = 1`); `/admin/khong-ton-tai` và `/Admin/wp-login.php` trả `404` và bị loại trừ khỏi `not_found_logs`, còn `/%61dmin/config` bị chặn ngay với `400 Bad Request` tại `decode_safe_legacy_request_path` (chống aliasing ASCII `%XX`) và không ghi `not_found_logs`.
  - Modal tạo redirect mở tại chỗ trên `#/tools/404-log` với `source_path = "/san-pham-cu.html"`; khi lưu thành công, rule mới được tạo, dòng `/san-pham-cu.html` bị xóa khỏi `not_found_logs` và evict khỏi RAM dưới `flush_mutex` (không hồi sinh ở chu kỳ flush kế tiếp).

### CUJ 4: Bất biến ưu tiên route, Chặn vòng lặp/open redirect & Bảo trì `SiteCoordinator`
- **Given:** Site có bài viết xuất bản tại `/gioi-thieu`, một bài ở trạng thái `draft` tại `/bai-nhap`, một manual redirect `/cu-1 → /cu-2`, và đang có tác vụ Backup hoặc Đối soát Media (`begin_maintenance()`).
- **When:**
  1. Admin tạo manual redirect `/gioi-thieu → /trang-khac` và `/bai-nhap → /trang-khac`;
  2. Admin thử tạo `/cu-2 → /cu-1` (vòng lặp), `/cu-2 → /cu-3 → /cu-4` để tạo chuỗi 4 hop khi đã có chuỗi 3 hop, `/tin-t%E1%BB%A9c → /tin-t%E1%BB%A9c` (tự trỏ dạng encode), hoặc `/a → //evil.com`;
  3. Khách truy cập `/blog/%2Fprivate`, `/blog/%252Fprivate` hoặc `/blog/%2e%2e/private`;
  4. Backup chạy đồng thời với các request 404 công khai; sau đó server nhận `SIGTERM` shutdown.
- **Then:**
  1. `/gioi-thieu` hiển thị cảnh báo **`Đang bị che bởi nội dung công khai`** (`shadowed_by_live_route: true`) và `GET /gioi-thieu` trả `200 OK`; ngược lại `/bai-nhap` có `shadowed_by_live_route: false` (vì bài nháp chưa công khai) và `GET /bai-nhap` chuyển hướng ngay.
  2. Server từ chối `422` với vòng lặp, chuỗi `> 3` hop (tính tổng cả upstream + downstream), self-redirect (kể cả dạng `%XX`) và open redirect `//evil.com`.
  3. Các đường dẫn `%2F`/`%252F`/`%2e%2e` tiếp tục trả `400 Bad Request` và không ghi vào `not_found_logs`.
  4. Trong thời gian `begin_maintenance()`, flusher không giữ `MutationLease` (bỏ qua flush ngay khi `admit_mutation()` báo `MaintenanceActive`), không làm timeout `begin_maintenance()`, backup/restore vượt qua `validate_restored_db_read_only()` (`229 < 256` tổng `sqlite_schema`), và khi shutdown `main.rs` flush hết telemetry trước `drain_for_shutdown()`.

### CUJ 5: Phân quyền Admin-only, Bảo toàn nháp qua `401`/`412` & Đa ngôn ngữ
- **Given:** Tài khoản `Editor` đã đăng nhập; tài khoản `Admin` đang nhập form tạo/sửa redirect (hoặc CSV) bị thay đổi ở tab khác (`412`) hoặc hết phiên (`401`); ngôn ngữ Admin `cms_locale` chuyển giữa `en` và `vi`.
- **When:** `Editor` mở `#/tools/redirects` / `#/tools/404-log` hoặc gọi `/api/admin/redirects*`, `/api/admin/404-logs*`. `Admin` gặp `401` rồi đăng nhập lại cùng tài khoản, hoặc gặp `412`.
- **Then:**
  - `Editor` nhận màn hình `rbac-forbidden` trên UI và `403 Forbidden` trên toàn bộ API.
  - Khi gặp `401` (dù `AdminShell.tsx` unmount `toolsContent`), module-level RAM `Map` draft store (keyed theo `${siteInstanceId}:${user.id}`) giữ nguyên modal đang mở và dữ liệu đang nhập để khôi phục ngay sau khi cùng tài khoản đăng nhập lại; đăng xuất, xác nhận rời màn hình hoặc đổi tài khoản/site sẽ xóa sạch bản nháp. Khi gặp `412`, giữ bản nháp kèm lựa chọn tải lại phiên bản mới nhất.
  - Toàn bộ nhãn UI, tab, modal, `CsvPreviewRow.error` và thông báo lỗi API hỗ trợ đầy đủ tiếng Anh và tiếng Việt (`check-i18n.mjs --strict` và unit test `i18n.rs` PASS).

---

## Technical Plan (Architecture)

### 1. Thứ tự ưu tiên Route công khai & Giải mã đường dẫn an toàn (`routes/seo.rs` + `storage/permalinks/`)

```text
HTTP GET/HEAD <uri>
  ├── Khớp route Axum tĩnh (/ , /admin/*, /api/*, /auth/*, /media/*, /theme/*, /assets/*, /builder/*, /p/*, /robots.txt, /sitemap.xml, /sitemaps/*)
  │     └── Xử lý bởi handler tương ứng (KHÔNG qua redirect thủ công, KHÔNG log 404)
  └── Rơi vào public_fallback(uri):
        ├── Kiểm tra cấu trúc thô ('/', '//', '\\') -> lỗi => 400 (KHÔNG log 404)
        ├── first_seg_lower = first_seg.to_ascii_lowercase()
        │     └── is_reserved_system_root(&first_seg_lower) => render 404 (KHÔNG tra redirect, KHÔNG log 404)
        ├── Chuẩn hóa lookup_path (bỏ tối đa 1 '/' ở cuối nếu len > 1)
        ├── Nhánh A: lookup_path chứa '%'
        │     ├── decode_safe_legacy_request_path(lookup_path)
        │     │     ├── Lỗi (bất kỳ %XX ASCII < 0x80 ngoài %20, hex sai, raw whitespace, control char, UTF-8 sai, segment rỗng/./..) => 400 (KHÔNG log 404)
        │     │     └── Hợp lệ -> decoded_path (VD: /tin-t%E1%BB%A9c -> /tin-tức; /old%20a -> /old%20a)
        │     │           ├── Defense-in-depth: kiểm lại is_reserved_system_root(&decoded_first_seg_lower) => render 404 (KHÔNG log 404)
        │     │           ├── KHÔNG tra permalink_route_claims (bảo toàn bất biến C3 chống aliasing)
        │     │           ├── Tra storage::redirects::find_active_manual_redirect(&db, &decoded_path) (chỉ khớp nếu decoded_path.len() <= 512)
        │     │           │     └── Khớp => nếu method==GET ghi hit vào telemetry + trả 301/302 (Location + Cache-Control: no-store)
        │     │           └── Không khớp => inferred_locale theo decoded_first_seg -> render 404 + nếu method==GET && decoded_path.len()<=512 ghi 404 vào telemetry
        └── Nhánh B: lookup_path không chứa '%'
              ├── validate_public_request_path(lookup_path) -> lỗi => 400 (KHÔNG log 404)
              ├── Tra permalinks::resolve_public_path(&db, lookup_path):
              │     ├── Some(route) với route.redirect == false (LIVE canonical claim còn publish/public)
              │     │     └── Phục vụ trang công khai (hoặc 301 bỏ trailing slash). Thắng mọi manual redirect.
              │     └── None HOẶC Some(route) với route.redirect == true (Auto-301 cũ)
              │           ├── 1. Nếu lookup_path != "/" && lookup_path.len() <= 512: tra storage::redirects::find_active_manual_redirect(&db, lookup_path)
              │           │      └── Khớp => nếu method==GET ghi hit vào telemetry + trả 301/302 (Location + Cache-Control: no-store)
              │           ├── 2. Nếu route.redirect == true => trả 301 auto như hiện hành
              │           └── 3. Không khớp => render 404 + nếu method==GET && lookup_path != "/" && lookup_path.len() <= 512 && không phải secondary locale home bị thiếu (tombstoned home) ghi 404 vào telemetry
```

**Làm rõ các trường hợp biên của `public_fallback`:**
- **Phát hiện nhánh `entry` không còn visible hoặc `archive:post` `ReadingPageState::Missing`:** Khi `resolve_public_path` trả `Some(Entry { is_home, .. })` mà ở ranh giới concurrent race `get_published_by_id` trả `None`: khi `is_home == true` (trang chủ locale phụ ví dụ `/vi`), render 404 và **không** ghi vào `not_found_logs` (cũng không cho manual redirect ghi đè `lookup_path == "/"`). Tương tự, nếu `lookup_path` trùng một secondary locale root đang bật (`enabled_secondary_locale_for_segment`) khi `homepage_page_id` bị mất, không log 404 cho root locale đó. Với `PublicRouteTarget::Archive { type_key: "post", .. }` khi `ReadingPageState::Missing`, `resolve_public_path` trả `Ok(None)`, nên nếu không có manual redirect cho `/posts`, trả 404.
- **Bảo vệ `is_reserved_system_root(&first_seg.to_ascii_lowercase())`:** Ngăn tuyệt đối `/Admin/x`, `/API/x`, `/Assets/x`, `/FAVICON.ICO` không lọt vào `manual_redirects` hay `not_found_logs`. (Các hàm C3 ở `seo.rs` giữ nguyên hành vi đối với `is_reserved_system_root(key)`.)
- **Phục vụ `HEAD`:** Vẫn trả đúng `301`/`302` + `Location` hoặc `404`, nhưng **chỉ ghi telemetry hit/404 khi `method == Method::GET`** để tránh monitor/crawler gọi `HEAD` làm sai lệch bộ đếm.

### 2. Mô hình Domain thuần Rust (`opendash_core::redirects` & `opendash_core::id`)

Theo quy chuẩn `CORE_FOUNDATION.md`, `opendash-core` thuần Rust (`serde`, `serde_json`, `url`), tuyệt đối không chứa SQL, HTTP hay I/O:

1. **Hợp nhất `RESERVED_SYSTEM_ROOTS` vào `opendash_core::id`:**
   - Di chuyển `pub const RESERVED_SYSTEM_ROOTS: &[&str]` (`26` tiền tố: `admin`, `api`, `assets`, `auth`, `builder`, `content`, `favicon.ico`, `forms`, `health`, `media`, `p`, `page`, `pattern`, `preview`, `registry`, `robots.txt`, `save`, `section`, `sitemap.xml`, `sitemaps`, `static`, `studio`, `taxonomy`, `theme`, `themes`, `uploads`) và `pub fn is_reserved_system_root(root: &str) -> bool` từ `public_context.rs` sang `opendash_core::id`.
   - `public_context.rs` và `seo.rs` re-export `opendash_core::id::is_reserved_system_root`.
2. **Module mới `opendash_core::redirects`:**
   - **Hằng số giới hạn:**
     - `MAX_REDIRECT_SOURCE_PATH_BYTES: usize = 512`
     - `MAX_REDIRECT_TARGET_URL_BYTES: usize = 2048`
     - `MAX_REDIRECT_NOTE_CHARS: usize = 256`
     - `MAX_MANUAL_REDIRECTS_TOTAL: usize = 20_000`
     - `MAX_CSV_IMPORT_BYTES: usize = 2 * 1024 * 1024` (`2 MiB`)
     - `MAX_CSV_IMPORT_ROWS: usize = 5_000`
     - `MAX_CSV_PREVIEW_ROWS: usize = 500`
     - `MAX_NOT_FOUND_LOG_ROWS: usize = 5_000`
     - `MAX_NOT_FOUND_PATH_BYTES: usize = 512`
     - `MAX_NOT_FOUND_REFERRER_BYTES: usize = 512`
     - `MAX_NOT_FOUND_UA_BYTES: usize = 256`
     - `MAX_REDIRECT_CHAIN_HOPS: usize = 3`
   - **Typed Error Enums (không dùng `ApiError` trong S01; dịch đầy đủ sang `en`/`vi` ở S02):**
     - `RedirectValidationError` (variants: `EmptySourcePath`, `SourcePathTooLong`, `SourcePathInvalidFormat`, `SourcePathReservedRoot`, `SourcePathUnsafeEncoding`, `EmptyTargetUrl`, `TargetUrlTooLong`, `TargetUrlInvalid`, `TargetUrlUnsafeProtocolRelative`, `SelfRedirect`, `InvalidStatusCode`, `NoteTooLong`, `NoteControlChars`, `CsvEmpty`, `CsvTooLarge`, `CsvTooManyRows`, `CsvMalformedQuote`, `CsvMissingRequiredHeaders`, `CsvDuplicateMappedHeaders`).
     - `CsvRowError` (variants dùng chung cho parse domain & kiểm tra batch: `Validation(RedirectValidationError)`, `InvalidStatusValue`, `InvalidEnabledValue`, `InvalidColumnCount`, `DuplicateSourceInCsv`, `ConflictExistingSource`, `CycleOrChainExceeded`, `QuotaExceeded`) kèm method `.canonical_vi_message() -> &'static str` và `.code() -> &'static str` để `routes/redirects.rs` dịch 100% sang ngôn ngữ UI hiện hành qua `i18n::translate_api_message`.
   - **Kiểu DTO chuẩn (`#[serde(deny_unknown_fields)]`, không dùng `serde(flatten)`):**
     - `RedirectStatusCode` (`301` | `302`, hỗ trợ `TryFrom<u16>` trả `RedirectValidationError::InvalidStatusCode` -> `422`).
     - `ConflictPolicy` (`#[serde(rename_all = "snake_case")]`: `Skip`, `Overwrite`, `Error`).
     - `CsvImportRowStatus` (`#[serde(rename_all = "snake_case")]`: `Create`, `Update`, `SkipExisting`, `Error`).
     - `ManualRedirectInput { source_path: String, target_url: String, status_code: RedirectStatusCode, enabled: bool, note: Option<String> }` (`CreateManualRedirectRequest` & `UpdateManualRedirectRequest` trong `routes/redirects.rs` giải mã `status_code: u16` rồi chuyển qua `RedirectStatusCode::try_from` để mã sai như `307` luôn trả `422 InvalidStatusCode`).
     - `CsvPreviewRow { row_number: usize, source_path: String, target_url: String, status_code: Option<u16>, enabled: Option<bool>, note: String, status: CsvImportRowStatus, #[serde(skip_serializing_if = "Option::is_none")] error_code: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] error: Option<String> }`.
     - `CsvImportRequest { csv_text: String, dry_run: bool, conflict_policy: ConflictPolicy }`.
     - `CsvImportResponse { dry_run: bool, total_rows: usize, create_count: usize, update_count: usize, skip_count: usize, error_count: usize, truncated: bool, rows: Vec<CsvPreviewRow>, #[serde(default, skip_serializing_if = "Option::is_none")] error: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] code: Option<String> }`.
   - **Quy tắc chuẩn hóa & giải mã `%XX` nhất quán (`decode_safe_legacy_request_path`):**
     - **Không** đặt giới hạn độ dài cứng `512` bên trong `decode_safe_legacy_request_path` (chỉ kiểm tra bảo mật & cấu trúc để dùng chung an toàn cho cả `source_path <= 512`, `target_url <= 2048` và `404` telemetry gate `<= 512`).
     - Cho phép **duy nhất** hai dạng `%XX`:
       1) `%20` (khoảng trắng URL cũ) — được chuẩn hóa chữ hoa thành literal `"%20"` (KHÔNG giải mã thành khoảng trắng thô `' '`, giúp đường dẫn `/old%20page.html` round-trip 100% qua Edit UI và CSV);
       2) Các chuỗi byte `>= 0x80` hợp lệ tạo thành UTF-8 đa byte (ví dụ `/tin-t%E1%BB%A9c` -> `/tin-tức`).
     - **Từ chối ngay với `Err(RedirectValidationError::SourcePathUnsafeEncoding)`** (`public_fallback` ánh xạ thành `400 Bad Request`; các endpoint ghi Admin `POST`/`PUT`/`POST /import` ánh xạ toàn bộ lỗi domain trừ `CsvTooLarge -> 413` thành `422 Unprocessable Entity`) đối với:
       - Mọi `%XX` giải mã ra byte `< 0x80` khác `0x20` (tức toàn bộ `%00..%1F`, `%21..%7F` bao gồm `%2F`, `%25`, `%5C`, `%2E`, `%3F`, `%23`, `%61`, v.v. — triệt tiêu hoàn toàn aliasing ký tự ASCII ngay từ đầu vào);
       - Mọi `%` không theo sau bởi 2 ký tự hex hợp lệ, hoặc chuỗi byte `>= 0x80` không phải UTF-8 hợp lệ.
     - Kiểm tra chuỗi sau chuẩn hóa: bắt buộc bắt đầu bằng `/`, không chứa khoảng trắng thô (`is_whitespace`) hoặc ký tự điều khiển (`is_control`), không chứa `\`, `?`, `#`, và không có segment rỗng (`//`), `.` hoặc `..`.
   - **Các hàm chuẩn hóa & build `Location`:**
     - `normalize_redirect_source_path(raw: &str) -> Result<String, RedirectValidationError>`:
       1) Trim khoảng trắng đầu/cuối; reject nếu rỗng, chứa khoảng trắng thô/control char bên trong, `\`, `?`, `#`, `//`;
       2) Cắt tối đa 1 `/` ở cuối nếu `len > 1`; reject nếu bằng `/` hoặc vẫn kết thúc bằng `/`;
       3) Nếu chứa `%`, gọi `decode_safe_legacy_request_path`; nếu không chứa `%`, kiểm tra không segment nào rỗng/`.`/`..`;
       4) Kiểm tra độ dài `<= MAX_REDIRECT_SOURCE_PATH_BYTES (512)` (trả `SourcePathTooLong`);
       5) Kiểm tra segment đầu tiên (`to_ascii_lowercase()`) không thuộc `is_reserved_system_root`.
     - `normalize_redirect_target_url(raw: &str) -> Result<String, RedirectValidationError>`:
       1) Trim khoảng trắng đầu/cuối; reject nếu rỗng, `> 2048` bytes, hoặc chứa ký tự điều khiển (`is_control`);
       2) Nếu bắt đầu bằng `/`:
          - Từ chối `//` hoặc `/\` ở đầu; từ chối `\\` và khoảng trắng thô ở phần path;
          - Tách `(before_frag, opt_frag)` tại `'#'`, rồi tách `(path_part, opt_query)` tại `'?'`;
          - Chuẩn hóa `path_part`: cắt tối đa 1 `/` cuối (trừ `/`), nếu chứa `%` áp dụng `decode_safe_legacy_request_path(path_part)` (trừ `path_part == "/"` — nếu `Err(...)` ánh xạ sang `RedirectValidationError::TargetUrlInvalid` để không báo nhầm lỗi `"đường dẫn nguồn"`), cấm `//`, `.`, `..`;
          - Bỏ dấu `?` rỗng (`opt_query == Some("")`) và bỏ dấu `#` rỗng (`opt_frag == Some("")`); ghép lại thành `normalized`, rồi kiểm tra lại `normalized.len() <= 2048` (trả `TargetUrlTooLong`).
       3) Ngược lại: parse bằng `url::Url::parse(trimmed)`: chỉ chấp nhận scheme `"http"` hoặc `"https"`, bắt buộc có `host_str()`, `username().is_empty()`, `password().is_none()`; lấy `normalized = parsed.to_string()` (tự punycode tên miền IDN và chuẩn hóa đường dẫn) và kiểm tra lại `normalized.len() <= 2048` (trả `TargetUrlTooLong`).
     - `extract_internal_target_path(normalized_target: &str) -> Option<String>`:
       - Khi `normalized_target` bắt đầu bằng `/`, tách phần trước `'?'` và `'#'` để so sánh 1:1 cho self-redirect & cycle check.
     - `extract_effective_target_path(normalized_target: &str, public_origin: Option<&str>) -> Option<String>`:
       - Nếu là `/...`, gọi `extract_internal_target_path`. Nếu là URL `http(s)://...` và trùng `(scheme, host_str, port_or_known_default)` với `public_origin` (parsed), lấy `url.path()`, cắt 1 `/` cuối (trừ `/`), qua `decode_safe_legacy_request_path` nếu chứa `%` (hoặc lấy trực tiếp khi hợp lệ) để bắt cả self-loop/cycle qua URL tuyệt đối của chính website!
     - `check_not_self_redirect(normalized_source: &str, normalized_target: &str, public_origin: Option<&str>) -> Result<(), RedirectValidationError>`:
       - Nếu `extract_effective_target_path(normalized_target, public_origin).as_deref() == Some(normalized_source)` -> trả `Err(SelfRedirect)`.
     - `build_redirect_location(normalized_target: &str, incoming_query: Option<&str>) -> String`:
       1) Làm sạch `incoming_query`: bỏ mọi byte `< 0x20` hoặc `0x7F`;
       2) Tách `normalized_target` tại `'#'` thành `(before_frag, opt_frag)`; chỉ nối `?{clean_query}` vào cuối `before_frag` khi `clean_query` khác rỗng VÀ `!before_frag.contains('?')`;
       3) Ghép lại `#frag` (nếu có);
       4) Duyệt từng byte chuỗi kết quả: bỏ mọi byte `< 0x20` hoặc `0x7F`, percent-encode khoảng trắng (`' '` -> `%20`) và các byte `>= 0x80` (UTF-8), giữ nguyên ASCII in được (`0x21..=0x7E`). Bảo đảm `axum::http::HeaderValue::from_str` không bao giờ lỗi.
     - `normalize_redirect_note(raw: Option<&str>) -> Result<String, RedirectValidationError>`:
       - Trim khoảng trắng; `None` hoặc rỗng -> `""`; từ chối `c.is_control()`; kiểm `chars().count() <= 256`.
     - `sanitize_referer(raw: Option<&str>) -> Option<String>` & `sanitize_user_agent(raw: Option<&str>) -> Option<String>`:
       - `sanitize_referer`: bỏ control chars, trim; nếu parse được bằng `url::Url::parse`, xóa `username`/`password`; cắt theo UTF-8 char boundary `<= 512` bytes.
       - `sanitize_user_agent`: bỏ control chars, trim, cắt theo UTF-8 char boundary `<= 256` bytes.
   - **CSV Parser & Formula-Safe Exporter thuần Rust:**
     - `parse_redirects_csv(raw: &str) -> Result<Vec<Result<ParsedCsvRow, CsvRowError>>, RedirectValidationError>`:
       - Kiểm `raw.len() <= 2 MiB`; bỏ BOM `\u{FEFF}`; parse theo RFC 4180 (`"`, `""`, CRLF/LF); loại bỏ dòng trống hoàn toàn (gồm trailing newline cuối file).
       - **Nhận diện header độc lập thứ tự cột:** Hàng đầu tiên sau parse được coi là header khi không có ô nào bắt đầu bằng `/`, `http://`, `https://` VÀ mọi ô không rỗng đều là tên cột hợp lệ `[a-z0-9_]+` trong đó có ít nhất **2 ô** (hoặc **mọi** ô nếu hàng chỉ có 1 cột như `source_path`) khớp các alias chuẩn (không phân biệt hoa-thường):
         - `source_path`: `source_path`, `source`, `from`, `old_url`, `old_path`
         - `target_url`: `target_url`, `target`, `to`, `new_url`, `destination`
         - `status_code`: `status_code`, `status`, `code`, `http_status`
         - `enabled`: `enabled`, `active`, `is_active`
         - `note`: `note`, `notes`, `comment`, `description`
         - Cột export được cho phép & bỏ qua: `hit_count`, `hits`, `last_hit_at`, `version`, `created_at`, `updated_at` (các cột lạ khác ngoài bộ trên trong CSV có header vẫn bị bỏ qua nếu đã đủ `source_path` & `target_url`).
         - Thiếu `source_path` hoặc `target_url` -> `Err(CsvMissingRequiredHeaders)`; map trùng tên trường -> `Err(CsvDuplicateMappedHeaders)`. Nếu không phải header -> đọc theo vị trí `0: source_path, 1: target_url, 2?: status_code, 3?: enabled, 4?: note`.
       - Sau khi tách header, yêu cầu `1..=5_000` dòng dữ liệu (`0` dòng -> `Err(CsvEmpty)`, `> 5_000` -> `Err(CsvTooManyRows)`).
       - **Quy tắc cột dữ liệu trên mỗi dòng:**
         - `InvalidColumnCount`: CSV không header yêu cầu `2..=5` cột; CSV có header yêu cầu `row.len() <= header.len()` (thiếu các cột tùy chọn ở cuối được coi như chuỗi rỗng; dư cột `> header.len()` -> `InvalidColumnCount`).
         - `status_code`: bỏ trống hoặc thiếu cột mặc định `301`; nhận `"301"` / `"302"`; giá trị khác -> `CsvRowError::InvalidStatusValue`.
         - `enabled`: bỏ trống hoặc thiếu cột mặc định `true`; nhận không phân biệt hoa-thường `true|1|yes` (`true`) và `false|0|no` (`false`); giá trị khác -> `CsvRowError::InvalidEnabledValue`.
       - Mỗi dòng lỗi trường trả `Err(CsvRowError)` theo đúng `row_number` (1-based data row index) để `dry_run` báo từng dòng.
     - `format_redirects_csv(rows: &[ManualRedirectCsvRow]) -> String`:
       - Đầu tệp `\u{FEFF}source_path,target_url,status_code,enabled,note,hit_count,last_hit_at\r\n`.
       - `escape_csv_formula_cell(val)`: kiểm tra **cả** ký tự đầu thô (`val.starts_with(['\t', '\r', '\n'])`) **lẫn** ký tự đầu sau `val.trim_start()` (`['=', '+', '-', '@', '\t', '\r', '\n']`), gắn `'` trước khi bọc RFC 4180 quote.

### 3. Schema SQLite & Kiểm soát Ngân sách `sqlite_schema`

Tạo 1 file migration forward-only tiếp theo: `cms/migrations/0057_manual_redirects_and_404_log.sql` *(renumbered from `0056` sau khi rebase lên `origin/main@5ed3b45`, nơi [Unified Field Engine](unified-field-engine.md) S04 đã chiếm slot `0056_unified_field_content_cutover.sql` với `+0` net schema objects)*.

```sql
CREATE TABLE manual_redirects (
    id INTEGER PRIMARY KEY,
    source_path TEXT NOT NULL COLLATE BINARY UNIQUE,
    target_url TEXT NOT NULL,
    status_code INTEGER NOT NULL DEFAULT 301 CHECK (status_code IN (301, 302)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    note TEXT NOT NULL DEFAULT '',
    hit_count INTEGER NOT NULL DEFAULT 0 CHECK (hit_count >= 0),
    last_hit_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE not_found_logs (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL COLLATE BINARY UNIQUE,
    hit_count INTEGER NOT NULL DEFAULT 1 CHECK (hit_count >= 1),
    first_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_referrer TEXT,
    last_user_agent TEXT
);

CREATE INDEX idx_not_found_logs_last_seen
    ON not_found_logs (last_seen_at DESC, id DESC);

CREATE INDEX idx_not_found_logs_hit_count
    ON not_found_logs (hit_count DESC, last_seen_at DESC);
```

**Kiểm soát Ngân sách `sqlite_schema`:**
- Baseline `origin/main@43c19c2` (`0001`–`0055`): `221` objects từ migrations (`80` tables + `124` indexes + `17` triggers), tương ứng `223` trong `sqlite_schema` thực tế khi tính thêm `_sqlx_migrations` và `sqlite_autoindex__sqlx_migrations_1`.
- `manual_redirects` + autoindex `source_path`: **+2 objects**.
- `not_found_logs` + autoindex `path` + 2 secondary indexes: **+4 objects**.
- **Tổng sau nâng cấp (`0057`):** `227` objects migration (`229` tổng `sqlite_schema` thực tế, vì `0056_unified_field_content_cutover.sql` đóng góp `+0` net object) `< 256` (`MAX_SCHEMA_OBJECTS` trong `storage/restore.rs`). Không dùng `AUTOINCREMENT`, không gắn FK tới `users` hoặc `site_locales`.

### 4. Phát hiện Vòng lặp, Chuỗi Hai Chiều & Cảnh báo Live Route (`storage/redirects/` + `storage/permalinks/`)

1. **Chống vòng lặp & chuỗi `<= 3` hop theo cả hai chiều (đơn lẻ và batch CSV):**
   - Trong `storage/redirects/` (`graph_and_import.rs`), xây dựng kiểm tra đồ thị in-memory `O(N)` bên trong giao dịch (`db.begin()` khi `dry_run == true`, `BEGIN IMMEDIATE` khi ghi):
     - Nạp `manual_redirects WHERE enabled = 1 AND (? IS NULL OR id != ?)` (với `exclude_id = Some(id)` khi `PUT`, `<= 20_000` hàng) vào `manual_edges: HashMap<String, Option<String>>` (dùng `extract_effective_target_path(target_url, public_origin)`).
     - Nạp tập các auto-301 đang còn live từ `permalink_route_redirects` (lọc theo cùng điều kiện `site_locales` + target còn `published`/`trashed_at IS NULL`/active của `resolve_public_path`) vào `auto_edges: HashMap<String, String>`.
     - Xây dựng **`effective_edges`** (`manual_edges` luôn ghi đè `auto_edges` trên cùng key) rồi từ `effective_edges` dựng `reverse_adj` (`target_path -> Vec<source>`). Điều này bảo đảm cả chiều thuận lẫn chiều ngược (`backward_depth`) không bị auto-301 đã bị che bởi manual redirect làm nhiễu độ sâu/chu trình.
     - Duyệt từng dòng hợp lệ trong batch CSV theo đúng thứ tự `row_number` tăng dần:
       - **Kiểm tra trùng nội bộ CSV:** Nếu `source_path` đã xuất hiện trước đó trong cùng batch CSV -> gắn dòng sau là `CsvImportRowStatus::Error` (`CsvRowError::DuplicateSourceInCsv`, code `"redirects.csv_duplicate_in_batch"`), áp dụng thống nhất cho mọi `conflict_policy` (`skip`, `overwrite`, `error`).
       - **Dòng `SkipExisting` (`conflict_policy == Skip` trùng DB):** Giữ nguyên trạng thái `SkipExisting`, **không** thay đổi `manual_edges` và **không** kiểm tra cycle lên target bị bỏ qua trong CSV.
       - **Quota `20_000` trong cả `dry_run` và `execute`:** Trước khi tăng `create_count`, kiểm tra `if existing_count + create_count >= MAX_MANUAL_REDIRECTS_TOTAL (20_000)` (tức cho phép đúng mục thứ `20_000` khi `existing_count + create_count + 1 == 20_000`, và từ chối từ mục thứ `20_001` trở đi); nếu vượt trần, đánh dấu dòng đó (và các dòng `Create` vượt trần kế tiếp) là `CsvImportRowStatus::Error` (`CsvRowError::QuotaExceeded`, code `"redirects.quota_exceeded"`).
       - **Kiểm tra graph cho `Create`/`Update` (`enabled == true`):** Chạy kiểm tra chu trình và độ sâu tổng **`backward_depth(src) + 1 + forward_depth(target) <= MAX_REDIRECT_CHAIN_HOPS (3)`** trên `effective_edges` + `reverse_adj`. Cả `forward_depth` lẫn `backward_depth` đều **bắt buộc dùng tập `visited: HashSet<&str>`** và dừng sớm khi `depth > MAX_REDIRECT_CHAIN_HOPS`.
       - Nếu hợp lệ: cập nhật `manual_edges` (hoặc gỡ nếu `Update` chuyển `enabled: false`) và cập nhật `effective_edges`/`reverse_adj` cho các dòng kế tiếp trong batch; nếu vi phạm cycle/`>3` hops: đánh dấu **chính dòng đó** là `Error` (`CsvRowError::CycleOrChainExceeded`) và giữ nguyên đồ thị trước dòng lỗi.
     - Với CRUD đơn lẻ (`POST`/`PUT`), nếu `enabled == true` thì áp dụng cùng kiểm tra (riêng `PUT` truyền `exclude_id: Some(id)` để cạnh cũ trước khi sửa không gây `422` giả).
2. **Kiểm tra `shadowed_by_live_route` khớp 100% `resolve_public_path` (`storage/permalinks/live_claims.rs`):**
   - Bổ sung `pub(crate) async fn find_live_claim_kinds_for_paths_in_conn(conn: &mut SqliteConnection, paths: &[String]) -> ApiResult<BTreeMap<String, &'static str>>` trong `storage/permalinks/live_claims.rs` (re-export từ `storage/permalinks/mod.rs`) đối chiếu trực tiếp `permalink_route_claims c` kèm đúng bộ lọc hiển thị của `resolve_public_path`:
     - `site_locales sl ON sl.code = c.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)`;
     - `c.target_kind = 'home'`: xác minh `c.path = prefix_for_locale(c.locale, default_locale, "/")`;
     - `c.target_kind = 'entry'`: join `content_entries e ON CAST(e.id AS TEXT) = c.target_id AND e.locale = c.locale` và `content_types t ON t.key = e.content_type_key` với `e.trashed_at IS NULL AND e.publication_status = 'published' AND e.content_type_key NOT IN ('pattern', 'section') AND t.provider_status = 'active' AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1`;
     - `c.target_kind = 'term'`: join `taxonomy_terms tm ON CAST(tm.id AS TEXT) = c.target_id AND tm.locale = c.locale` và `taxonomy_definitions td ON td.key = tm.kind` với `td.public = 1 AND td.provider_status = 'active' AND td.declaration_state = 'declared'`;
     - `c.target_kind = 'archive'`: join `content_types t ON t.key = c.target_id` với `t.key NOT IN ('pattern', 'section', 'page') AND t.provider_status = 'active' AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1 AND COALESCE(json_extract(t.definition_json,'$.settings.has_archive'),1)=1`, đồng thời nếu `c.target_id = 'post'` và `site_reading_settings.posts_page_id` (theo `locale = c.locale`) khác `NULL` thì trang được gán (`p.content_type_key = 'page' AND p.locale = c.locale`) phải đang `p.trashed_at IS NULL AND p.publication_status = 'published' AND pt.provider_status = 'active' AND COALESCE(json_extract(pt.definition_json,'$.settings.public'),1)=1`.
   - Cả rule `enabled = true` lẫn `enabled = false` đều được gán `shadowed_by_live_route` & `shadowed_target_kind` (`Some("home" | "entry" | "archive" | "term")` hoặc `None`); trường `shadowed_count` đếm tổng số **rule đang bật (`enabled = 1`) bị che trên toàn bộ bảng `manual_redirects`**.

### 5. Bộ đệm RAM `RedirectTelemetryBuffer`, Thứ tự khóa `SiteCoordinator` & Shutdown (`src/redirects.rs` + `main.rs`)

- **Thứ tự khóa bất biến toàn hệ thống:**
  ```text
  MutationLease (SiteCoordinator::admit_mutation / AdmittedMutation)
    -> flush_mutex (tokio::sync::Mutex)
      -> TelemetryMaps (std::sync::Mutex, chỉ thao tác RAM không await)
        -> SQLite BEGIN IMMEDIATE
  ```
- **Cấu trúc `RedirectTelemetryBuffer` (`cms/crates/opendash-server/src/redirects.rs`):**
  - Mọi timestamp UTC sinh trong RAM dùng `jiff::Timestamp::now()` format cố định 3 chữ số mili-giây `%Y-%m-%dT%H:%M:%S%.3fZ` (khớp byte-for-byte với `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` của SQLite khi so sánh `MIN`/`MAX` hoặc `older_than_days`).
  - `maps: std::sync::Mutex<TelemetryMaps>`:
    - `pending_redirect_hits: HashMap<i64, PendingRedirectHit>` (`count: u64`, `last_hit_at: String`), trần `MAX_BUFFER_ENTRIES = 512`.
    - `pending_404s: HashMap<String, PendingNotFound>` (`count: u64`, `first_seen_at: String`, `last_seen_at: String`, `last_referrer: Option<String>`, `last_user_agent: Option<String>`), trần `MAX_BUFFER_ENTRIES = 512`.
    - `record_404(...)` tự kiểm tra `path.len() <= MAX_NOT_FOUND_PATH_BYTES (512)`. Khi map đạt `>= 512` keys và xuất hiện key mới chưa có trong map -> giữ nguyên các key đã có, bỏ qua key mới, và gửi `notify.notify_one()` (tôn trọng `MIN_FLUSH_COOLDOWN = 1s`).
  - `flush_mutex: tokio::sync::Mutex<()>` (tuần tự hóa flusher, `flush_now`, và Admin eviction).
  - `notify: Arc<tokio::sync::Notify>` + `closed: Arc<std::sync::atomic::AtomicBool>`.
- **Luồng thực thi cụ thể:**
  1. **`flush_now(&self)` & vòng lặp worker nền (mỗi `2s` / ngưỡng `128` kèm cooldown `>= 1s`, trước Admin GET, và lúc shutdown ở `main.rs`):**
     - Khi worker thức dậy do ngưỡng/bảo hòa `notify` trong lúc `last_flush.elapsed() < MIN_FLUSH_COOLDOWN (1s)`, worker đợi hết `remaining_cooldown` bằng `tokio::select!` cùng tín hiệu shutdown `closed` (tuyệt đối không busy-spin).
     - Kiểm tra nhanh `maps.lock().is_empty()` -> return ngay nếu rỗng.
     - Gọi `let Ok(lease) = coordinator.admit_mutation() else { return; };` **trước** khi lấy `flush_mutex` hay rút dữ liệu từ `TelemetryMaps`! Nếu đang `MaintenanceActive`, dữ liệu nằm nguyên an toàn trong RAM và không cản trở `begin_maintenance()`.
     - Giữ `let _flush_guard = self.flush_mutex.lock().await;`, rút snapshot từ `self.maps.lock()`, chạy `storage::redirects::flush_telemetry_batch(&db, &snapshot).await` (tx `BEGIN IMMEDIATE`, giữ `first_seen_at = MIN(...)`, `last_seen_at = MAX(...)`, `COALESCE(excluded.last_referrer, not_found_logs.last_referrer)` / `last_user_agent`, và cắt tỉa `not_found_logs` giữ tối đa `5.000` hàng theo `hit_count ASC, last_seen_at ASC, id ASC`), rồi drop `_flush_guard` & `lease`.
  2. **Admin Mutation Eviction (dưới `AdmittedMutation` + `flush_mutex`):**
     - `POST /api/admin/redirects` (`resolve_not_found == true`), `PUT /api/admin/redirects/{id}` (khi tắt `enabled: false` hoặc đổi `source_path`), `DELETE /api/admin/redirects/{id}`, `POST /api/admin/redirects/import` (`dry_run == false`), `DELETE /api/admin/404-logs/{id}`, `POST /api/admin/404-logs/bulk-delete`: giữ `let _guard = telemetry.lock_flush().await;` quanh DB tx, dùng `DELETE ... RETURNING path` (khi xóa 404), commit rồi gọi `evict_not_found_paths(...)` / `evict_redirect_id(id)` **trước khi nhả `flush_mutex`**.
     - **Riêng `POST /api/admin/404-logs/clear`:**
       - Khi `older_than_days: None` (xóa tất cả): giữ `flush_mutex` quanh `DELETE FROM not_found_logs`, commit, rồi gọi `clear_pending_404s()`.
       - Khi `older_than_days: Some(n)` (xóa mục cũ hơn N ngày): giữ `flush_mutex`, **rút snapshot `TelemetryMaps` hiện có (nếu khác rỗng) và gọi `flush_telemetry_batch_in_tx(&mut tx, &snapshot)` NGAY ĐẦU cùng giao dịch `BEGIN IMMEDIATE` trước khi chạy `DELETE FROM not_found_logs WHERE last_seen_at < cutoff`**, rồi commit!
  3. **Shutdown & Drop Cleanup (`main.rs` + `app.rs`):**
     - `RedirectTelemetryBuffer::spawn_with_coordinator(db, coordinator) -> (Arc<RedirectTelemetryBuffer>, RedirectTelemetryDropGuard)`: worker đăng ký `let notified = notify.notified();` **trước** khi kiểm tra `closed.load(...)` (khớp `operations.rs:544-552`).
     - Trong `main.rs::async_main`: khởi tạo `(telemetry, telemetry_drop_guard)` ở `main.rs` và truyền `Some(telemetry.clone())` vào `build_router_with_telemetry(...)` (chữ ký `pub fn build_router_with_telemetry(state: AppState, expected_host: String, themes_root: PathBuf, telemetry: Option<Arc<RedirectTelemetryBuffer>>) -> Router`; còn `build_router` và `build_router_with_themes_root` giữ nguyên chữ ký cũ và gọi `build_router_with_telemetry(..., None)`).
     - Khi `telemetry` là `Some(...)` (chạy server thật ở `main.rs`): **chỉ `main.rs` sở hữu `telemetry_drop_guard`** (`app.rs` chỉ gắn `.layer(Extension(telemetry))` sau `.fallback(...)`). Khi nhận `SIGINT`/`SIGTERM`, sau khi `serve_fut` và `cancel_and_drain_all` kết thúc, **TRƯỚC** khi gọi `coordinator.drain_for_shutdown(15s)` (`main.rs:395`), `main.rs` gọi `let _ = tokio::time::timeout(Duration::from_secs(2), telemetry.flush_now()).await;`, rồi `drop(telemetry_drop_guard);`, sau đó mới chạy `coordinator.drain_for_shutdown(15s)` và `db.close().await`.
     - Khi `telemetry` là `None` (test gọi `build_router` / `build_router_with_themes_root`): `app.rs` tự gọi `spawn_with_coordinator` và gắn cả `.layer(Extension(telemetry_buf))` lẫn `.layer(Extension(Arc::new(drop_guard)))` **sau** `.fallback(...)`, nên khi `Router` drop cuối test, `DropGuard` kích hoạt `closed = true` + `notify.notify_waiters()` đánh thức worker thoát tức thì không rò rỉ handle SQLite.

### 6. Hợp đồng HTTP API (`/api/admin/redirects*` & `/api/admin/404-logs*`)

Mọi handler trong `routes/redirects/` (tách theo trách nhiệm `mod.rs`, `rules.rs`, `csv.rs`, `not_found_logs.rs`, mỗi file `<= 600 LOC`):
- **Phân quyền, Bảo mật & Quy tắc ánh xạ HTTP Status:**
  - Read handlers gọi `require_admin(&headers, &db).await?` và dùng `Result<Query<...>, QueryRejection>` ánh xạ lỗi query sang `ApiError::bad_request(...)` đã đăng ký `i18n.rs`.
  - Write handlers khai báo `AdmittedMutation(_lease)`, gọi **trực tiếp** `require_admin_with_csrf(&headers, &db).await?` trong thân hàm (đáp ứng `check-standards.py`), và nhận JSON qua `Result<Json<T>, JsonRejection>` (`PAYLOAD_TOO_LARGE` -> `ApiError(StatusCode::PAYLOAD_TOO_LARGE, ...)` + đăng ký `i18n.rs`; lỗi JSON khác -> `ApiError::bad_request(...)`).
  - **Ánh xạ lỗi `RedirectValidationError` nhất quán:** tại `public_fallback`, mọi lỗi đường dẫn trả `400 Bad Request`; tại `routes/redirects/`, `RedirectValidationError::CsvTooLarge` trả `413 Payload Too Large`, **toàn bộ các variant `RedirectValidationError` còn lại** (bao gồm `SourcePathUnsafeEncoding`, `CsvMalformedQuote`, `CsvMissingRequiredHeaders`, `CsvDuplicateMappedHeaders`, `CsvEmpty`, `CsvTooManyRows`) trả `422 Unprocessable Entity`, còn lỗi extractor/header/bộ lọc (`JsonRejection` khác `413`, `QueryRejection`, `If-Match` sai định dạng, query `status`/`enabled`/`shadowed`/`sort` không hợp lệ) trả `400 Bad Request` (riêng thiếu `If-Match` trả `428 Precondition Required`).
- **Headers & Pre-localized Error Middleware (`i18n.rs`):**
  - Tất cả response đều có `Cache-Control: no-store`; `POST/PUT` trả thêm `ETag: "vN"` qua `no_store_headers(Some(item.version))`.
  - Mọi lỗi chuẩn trả `ApiError` (`text/plain; charset=utf-8`).
  - Riêng `import_redirects` nhận thêm `State(state): State<AppState>`, giải mã `cms_locale` qua `crate::i18n::effective_locale_for_error(&state.db, &state.ui_locale).await` (`pub(crate) async fn`), dịch trực tiếp toàn bộ `CsvPreviewRow.error` (trên cả `200 dry_run` lẫn `422`/`409` rollback) và top-level `error` bằng `translate_api_message`:
    - Mã `code` dùng chuẩn dotted hợp lệ với `i18n.rs::exact_table_has_valid_codes_and_bidirectional_coverage`:
      - `422`: `code = "redirects.csv_import_validation_failed"`, `vi: "Nhập CSV thất bại: có dòng không hợp lệ hoặc tạo vòng lặp."`, `en: "CSV import failed: one or more rows are invalid or create a redirect loop."`
      - `409` (khi toàn bộ lỗi chỉ là xung đột `conflict_policy == Error`): `code = "redirects.csv_import_conflicts"`, `vi: "Nhập CSV thất bại: có đường dẫn nguồn đã tồn tại."`, `en: "CSV import failed: one or more source paths already exist."`
    - Để middleware [`localize_error_response`](../../cms/crates/opendash-server/src/i18n.rs) (`12224–12334`) **không ghi đè** `X-OpenDash-Error-Code` ở dòng `12272–12275` và không bị chặn bởi giới hạn `1 MiB` (`read_bounded_error_body` tại `12280`) với response 500 dòng, bổ sung `#[derive(Clone, Copy)] pub(crate) struct PrelocalizedErrorResponse;` trong `i18n.rs` (gắn lên response `422`/`409` của `import_redirects`) và kiểm tra ngay đầu `localize_error_response` (sau `12232–12243`, trước `12272–12275`): nếu `response.extensions().get::<PrelocalizedErrorResponse>().is_some()`, trả `response` nguyên vẹn ngay! Để giữ `i18n.rs` không vượt trần ratchet `12489` LOC, tách `mod localization_middleware_tests` (`12337–12489`) sang `i18n_tests.rs` và tách bảng + test lỗi redirect sang `i18n_redirects.rs` (`<= 600 LOC`).

| Method & Path | Limit | Input / Query Contract (mặc định & validation) | Output & Status Codes |
|---|---|---|---|
| `GET /api/admin/redirects` | — | Query: `page` (default `1`, `>= 1`), `per_page` (default `50`, `1..=100`), `q` (<= `256` chars, `LIKE ... ESCAPE '\'`), `status` (`301`\|`302`), `enabled` (`true`\|`false`), `shadowed` (`true`\|`false`). Invalid param -> `400`. `ORDER BY updated_at DESC, id DESC`. Gọi `flush_now()` trước khi đọc. | `200` `{ items: ManualRedirectItem[], total, page, per_page, shadowed_count }` (`ManualRedirectItem`: `id, source_path, target_url, status_code: 301\|302, enabled: bool, note, hit_count, last_hit_at: Option<String>, version, created_at, updated_at, shadowed_by_live_route: bool, shadowed_target_kind: Option<String>`) |
| `GET /api/admin/redirects/{id}` | — | Nạp 1 rule theo `id` (phục vụ `412` rebase `O(log N)` không phải quét phân trang); gọi `flush_now()` trước khi đọc. | `200` + `ETag: "vN"` + `ManualRedirectItem`; `404` |
| `POST /api/admin/redirects` | `16 KiB` | `CreateManualRedirectRequest` (`deny_unknown_fields`): `{ source_path, target_url, status_code: u16, enabled? (default true), note?, resolve_not_found? (default true) }`. | `201` + `ETag: "v1"` + `ManualRedirectItem`; `400`/`413`/`409` trùng `source_path`/`422` lỗi hợp lệ (`307` -> `422 InvalidStatusCode`), cycle/chain, quota `20_000` |
| `PUT /api/admin/redirects/{id}` | `16 KiB` | `If-Match: "vN"` (`parse_if_match`: thiếu -> `428`, sai -> `400`). `UpdateManualRedirectRequest` (`deny_unknown_fields`): `{ source_path, target_url, status_code: u16, enabled, note? }`. Thứ tự: `parse_if_match` -> `404` -> `412` stale -> `422` fields -> `409` trùng (`AND id != ?`) -> `422` cycle/chain (`exclude_id = Some(id)`) nếu `enabled == true`. Evict hit ID dưới `flush_mutex` nếu tắt hoặc đổi `source_path`. | `200` + `ETag: "vN+1"` + `ManualRedirectItem`; `400`/`428`/`404`/`412`/`409`/`422` |
| `DELETE /api/admin/redirects/{id}` | — | Không yêu cầu `If-Match`. Xóa rule và gọi `evict_redirect_id(id)` dưới `flush_mutex`. | `204 No Content` + `Cache-Control: no-store`; `404` |
| `POST /api/admin/redirects/import` | `5 MiB` | `CsvImportRequest` (`deny_unknown_fields`): `{ csv_text, dry_run, conflict_policy }`. Body HTTP đặt `DefaultBodyLimit::max(5 MiB)` để mọi CSV `<= 2 MiB` nhiều ngoặc kép/xuống dòng vẫn chạm kiểm tra rõ ràng `csv_text.len() > 2 MiB` -> `413`; `0` hoặc `> 5_000` dòng -> `422`. `dry_run: true` dùng read tx (đánh dấu cả `QuotaExceeded` khi `existing_count + create_count >= 20_000`); `dry_run: false` `ON CONFLICT DO UPDATE` giữ stats nếu `overwrite`, xóa & evict `not_found_logs` cho mọi path đã tạo/cập nhật. | `200` `CsvImportResponse` (`rows` tối đa `500`, ưu tiên dòng `error` trước, `truncated = total_rows > 500`, `dry_run: false` với `skip` không đổi vẫn `200`); `422` (`code: "redirects.csv_import_validation_failed"`) hoặc `409` (`code: "redirects.csv_import_conflicts"`) kèm `CsvImportResponse`, `X-OpenDash-Error-Code` & `PrelocalizedErrorResponse`; `400`/`413` |
| `GET /api/admin/redirects/export` | — | Gọi `flush_now()` trước khi đọc toàn bộ bảng `manual_redirects ORDER BY id ASC` (bỏ qua phân trang/bộ lọc UI). | `200` `text/csv; charset=utf-8` (BOM `\xEF\xBB\xBF` + formula-safe CSV), `Content-Disposition: attachment; filename="opendash-redirects.csv"`, `X-Content-Type-Options: nosniff`, `Cache-Control: no-store` |
| `GET /api/admin/404-logs` | — | Query: `page` (default `1`, `>= 1`), `per_page` (default `50`, `1..=100`), `q` (<= `256` chars, `LIKE ... ESCAPE '\'`), `sort` (`hits_desc` mặc định \| `recent_desc` \| `oldest_asc`). Invalid param -> `400`. Gọi `flush_now()` trước khi đọc. | `200` `{ items: NotFoundLogItem[], total, page, per_page, max_rows: 5000 }` (`NotFoundLogItem`: `id, path, hit_count, first_seen_at, last_seen_at, last_referrer: Option<String>, last_user_agent: Option<String>`) |
| `DELETE /api/admin/404-logs/{id}` | — | Không yêu cầu `If-Match`. Giữ `flush_mutex` quanh tx `DELETE ... RETURNING path` + `evict_not_found_paths`. | `204 No Content` + `Cache-Control: no-store`; `404` |
| `POST /api/admin/404-logs/bulk-delete` | `16 KiB` | `{ ids: Vec<i64> }` (`deny_unknown_fields`). Yêu cầu `1..=100` ID dương **không trùng lặp** (rỗng / `>100` / `<=0` / có ID trùng -> `422`). Giữ `flush_mutex` quanh tx `DELETE ... RETURNING path` + `evict_not_found_paths`. | `200` `{ deleted: usize }` |
| `POST /api/admin/404-logs/clear` | `4 KiB` | `{ older_than_days: Option<u32> }` (`deny_unknown_fields`). `Some(0)` hoặc `Some(> 3650)` -> `422`. `None` -> `DELETE FROM not_found_logs` + `clear_pending_404s()`. `Some(n)` -> trong cùng tx dưới `flush_mutex`, flush pending RAM rồi `DELETE WHERE last_seen_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', printf('-%d days', ?))`. | `200` `{ deleted: usize }` |

### 7. Giao diện Admin tại `Công cụ → Chuyển hướng & 404` (`cms/admin/src/`)

- **Điều hướng (`shell/AdminShellMenu.tsx`, `shell/AdminShell.tsx`, `shell/router.ts`, `App.tsx`):**
  - Trong `AdminShellMenu.tsx` (`buildMenuTree` mục `id: 'tools'`, `adminOnly: true`, dòng `176–187`): thêm mục con **`Chuyển hướng & 404`** (`#/tools/redirects`) đặt giữa `#/tools/media-reconcile` và `#/tools/health` (i18n key `shell.redirects_and_404`).
  - Trong `router.ts`: `parseAdminRoute` nhận `#/tools/redirects` (`sub: 'redirects'`) và `#/tools/404-log` (`sub: '404-log'`), giữ `view: 'tools'` (chỉ Admin qua `isRouteAllowed`).
  - Trong `AdminShell.tsx`: `selectedNavHash` ánh xạ cả `route.sub === '404-log'` sang `#/tools/redirects` để menu bên trái luôn sáng đúng mục; `subLabels` bổ sung `redirects` và `'404-log'`.
  - Trong `App.tsx` (tách hook thao tác plugin `handlePluginUpload`/`handleDisablePlugin`/`handleEnablePlugin`/`handleRunPlugin` ở `App.tsx:560–637` sang `features/plugins/useAdminPlugins.ts` để giữ `App.tsx` nằm dưới trần ratchet `879 LOC` của `cms/scripts/loc-baseline.json`):
    - Render `<RedirectsToolsPanel key={`${siteInstanceId}:${user?.id ?? 'guest'}`} initialTab={route.sub === '404-log' ? 'not-found' : 'redirects'} user={user} siteInstanceId={siteInstanceId} csrfToken={csrfToken} onApiError={(error) => scopedTaxonomyApiError(error, tStatic('app.redirects_error'))} onDirtyChange={markToolsDirty} onNavigateHash={navigateHash} />`.
    - Đưa `toolsDirty` / `toolsDirtyRef` vào toàn bộ vòng đời phiên trong `App.tsx`:
      - `mayLeave`: bổ sung `leavingTools = current.view === 'tools' && (current.sub === 'redirects' || current.sub === '404-log') && acceptedHashRef.current !== destination && toolsDirtyRef.current`, đưa `leavingTools` vào nhánh `tStatic("app.discard_changes")`, và khi được xác nhận rời đi gọi `discardRedirectsDraft(siteInstanceIdRef.current, userRef.current.id)` + `markToolsDirty(false)`.
      - `beforeunload`: thêm `|| toolsDirtyRef.current` vào điều kiện chặn và `toolsDirty` vào dependency array.
      - `handleLogout`: thêm `|| toolsDirtyRef.current` vào điều kiện hỏi `window.confirm(tStatic("app.discard_logout"))`, sau đó gọi `clearRedirectsDrafts()` + `markToolsDirty(false)`.
      - `handleLogin` khi đổi `siteInstanceId` hoặc `user.id` và `handleOwnAccountChanged`: gọi `clearRedirectsDrafts()` + `markToolsDirty(false)`.
      - `hasUnsavedChanges`: thêm `|| toolsDirty`.
- **Bảo toàn nháp qua `401` / `412`, Quản lý Tab & Đồng bộ giữa 2 Tab:**
  - `RedirectsToolsPanel.tsx` / `ManualRedirectsPanel.tsx` duy trì module-level **in-memory `Map<string, RedirectsDraftRecord>`** (tuyệt đối không dùng `localStorage`/`sessionStorage`, tuân thủ `features/README.md:32` & mẫu `PermalinkSettingsPage.tsx`) keyed theo `${siteInstanceId}:${user.id}`, lưu modal đang mở (`create` | `edit` | `from_404` | `csv_import`), `editingId`, `baselineVersion`, input form / `csvText` / `conflictPolicy`, lỗi `412` (`staleConflict: boolean`) và cờ `dirty`. Khi `401` unmount `toolsContent` rồi cùng Admin đăng nhập lại, panel tự khôi phục đúng modal và dữ liệu chưa lưu; khi gặp `412`, hiển thị banner xung đột kèm 2 lựa chọn: **Tải bản mới nhất** (rebase trực tiếp bằng `GET /api/admin/redirects/{id}` `O(log N)` — hoặc báo lỗi bản ghi đã bị xóa nếu `404` — rồi cập nhật `baselineVersion`, các trường form và xóa cờ `staleConflict`) và **Giữ bản nháp để sửa tiếp** (giữ nguyên giá trị đang nhập và khóa nút Lưu ở trạng thái `staleConflict` cho tới khi người dùng bấm rebase hoặc hủy).
  - Tab đồng bộ hai chiều với URL (`useEffect` theo `initialTab`: khi `initialTab` đổi và trong `Map` không còn draft đang mở, tự động đóng modal cục bộ của panel để tránh modal sót lại sau khi `mayLeave` hủy nháp; click tab gọi `onNavigateHash` sang `#/tools/redirects` ↔ `#/tools/404-log`, giữ bảo vệ `mayLeave`).
  - `RedirectsToolsPanel` duy trì cả `redirectsRefreshRevision` (tăng khi `from_404` lưu rule ở tab `404-log`) và `notFoundRefreshRevision` (tăng khi `POST` với `resolve_not_found = true` hoặc `import` thành công ở tab `Chuyển hướng`), giúp cả hai tab dù ẩn trong `AdminTabPanel` luôn đồng bộ ngay không hiển thị dữ liệu cũ.
- **Chi tiết 2 Tab (`AdminTabs` / `AdminTabPanel` từ `AdminPrimitives.tsx`) & `data-testid`:**
  - Container chính: `data-testid="redirects-tools-panel"`, tabs `data-testid="redirects-tools-tabs"` (`redirects` & `not-found`).
  - **Tab 1 — `Chuyển hướng (301/302)` (`ManualRedirectsPanel.tsx`, `data-testid="manual-redirects-panel"`):**
    - Thanh công cụ: ô tìm kiếm (`q`), lọc `301`/`302`, lọc trạng thái (`Tất cả` / `Đang bật` / `Đang tắt` / `Bị che bởi trang công khai`), nút **`+ Thêm chuyển hướng`** (`data-testid="redirect-add-btn"`), **`Nhập CSV`** (`data-testid="redirect-import-btn"`), **`Xuất CSV`** (`data-testid="redirect-export-btn"`).
    - Bảng & phân trang (`per_page = 50`, tự reset `page = 1` khi đổi filter/search): `source_path` (kèm chú thích `Khớp chính xác phân biệt hoa/thường`), `target_url`, `status_code` (`AdminBadge`), trạng thái bật/tắt nhanh & badge cảnh báo vàng nếu `shadowed_by_live_route` (hiển thị banner tổng `shadowed_count` ở đầu bảng nếu `> 0`), `hit_count` & `last_hit_at`, nút `Sửa` và `Xóa` (hộp thoại xác nhận).
    - Modal **Nhập CSV** (`data-testid="csv-import-modal"`): kiểm tra preflight `<= 2 MiB` trước khi đọc/gửi + `oversizeErrorMessage`; chọn `conflict_policy`; mọi thay đổi trên `csvText` hoặc `conflict_policy` đều đặt lại `preview = null` bắt buộc chạy lại **Xem trước (Dry-Run)**; nút **Xác nhận nhập** chỉ bật khi có bản xem trước hợp lệ mới nhất với `error_count == 0` và `create_count + update_count > 0`. Nếu lúc **Xác nhận nhập** gặp lỗi `422`/`409` từ server (do thay đổi đồng thời sau preview), modal hiển thị thông báo lỗi `ApiClientError.message` và đặt lại `preview = null` để Admin chạy Dry-Run mới.
  - **Tab 2 — `Nhật ký 404` (`NotFoundLogPanel.tsx`, `data-testid="not-found-log-panel"`):**
    - Thanh công cụ: ô tìm kiếm, sắp xếp đầy đủ 3 tùy chọn (`hits_desc` Nhiều lượt nhất / `recent_desc` Mới nhất / `oldest_asc` Cũ nhất), **`Làm mới`**, **`Xóa đã chọn`**, **`Dọn nhật ký`** (`data-testid="not-found-clear-btn"`, chọn `Cũ hơn 30 ngày` hoặc `Xóa tất cả` có xác nhận).
    - Bảng & phân trang (`per_page = 50`, reset selection khi đổi trang/filter/xóa): checkbox hàng, `path`, `hit_count`, `last_seen_at`, `first_seen_at`, `last_referrer` / `last_user_agent` rút gọn, nút **`Tạo Redirect`** (`data-testid="not-found-create-redirect-btn"`) và **`Xóa`**.
    - Luồng **1-click Tạo Redirect**: mở modal `data-testid="redirect-form-modal"` **tại chỗ ngay trên tab `Nhật ký 404`** (giữ người dùng ở `#/tools/404-log`), điền sẵn `source_path = row.path` và `resolve_not_found = true`; lưu xong tự xóa dòng 404 đó khỏi bảng, tăng `redirectsRefreshRevision`, và hiển thị thông báo thành công kèm liên kết nhanh chuyển sang `#/tools/redirects`.
- **API Client & i18n:**
  - `cms/admin/src/api/redirects.ts` dùng `requestJson` / `requestBlob` qua [`client.ts`](../../cms/admin/src/api/client.ts) (không cần sửa `client.ts`); export CSV tải `Blob` qua `requestBlob` để giữ nguyên 3 byte UTF-8 BOM `\xEF\xBB\xBF` và gửi cookie `same-origin` tự động (0 raw `fetch` ngoài `client.ts`).
  - `en.ts` và `vi.ts` (đang chạm trần `2346` LOC ở `loc-baseline.json`): tách nhóm `fields.*` (`en.ts:2292–2343`, `vi.ts:2294–2345`) sang `fields.{en,vi}.ts` và nhóm mới (`app.redirects_error`, `shell.redirects_and_404`, `redirects.*`, `not_found_logs.*`) sang `redirects.{en,vi}.ts` (`<= 600 LOC` mỗi file), spread lại vào `en.ts` / `vi.ts` (hạ `en.ts`/`vi.ts` xuống `< 2346` LOC), và cập nhật `cms/scripts/check-i18n.mjs` (`readCatalog` hỗ trợ spread fragment & lọc `*.vi.ts` trong `--strict`) để không sót tiếng Việt cứng (`node cms/scripts/check-i18n.mjs --strict` PASS).

---

## Alternatives Considered

1. **Đặt dưới `Cài đặt` (`#/settings/redirects` hoặc tab trong `SeoPanel`) vs. đặt dưới `Công cụ` (`#/tools/redirects`):**
   - *Quyết định:* Đặt dưới **`Công cụ → Chuyển hướng & 404`** theo chỉ đạo trực tiếp của chủ dự án; phù hợp bản chất công cụ migrate/vận hành cùng `Sao lưu & Phục hồi` và `Đối soát Media`.
2. **Ghi SQLite đồng bộ trên mọi `GET` 404 / Redirect hit vs. Bộ đệm RAM `RedirectTelemetryBuffer`:**
   - *Lý do bác bỏ ghi đồng bộ:* SQLite khóa một writer tại một thời điểm (`BEGIN IMMEDIATE`); ghi trực tiếp trên `GET` công khai gây contention `SQLITE_BUSY` và có thể làm treo cửa sổ `begin_maintenance()`. Thứ tự khóa `MutationLease → flush_mutex → TelemetryMaps → SQLite` kết hợp RAM cap `512`, flush+delete nguyên tử cho `older_than_days` và explicit flush trong `main.rs` trước `drain_for_shutdown()` giải quyết toàn diện bài toán đồng bộ.
3. **Thêm crate `csv` bên ngoài vs. Parser/Exporter RFC 4180 thuần Rust trong `opendash-core`:**
   - *Lý do bác bỏ thêm crate:* Tuân thủ chính sách pin dependency tại [`Cargo.toml`](../../cms/Cargo.toml) (`0` dependency mới), kiểm soát chặt chẽ chống CSV formula injection và báo lỗi per-row khi dry-run.
4. **Cho phép Redirect thủ công ghi đè route đang publish vs. `Live Canonical > Manual Redirect > Auto-301`:**
   - *Lý do bác bỏ ghi đè:* Tránh rủi ro nhập CSV nhầm vô tình che mất trang chủ hoặc trang sản phẩm đang sống. `permalink_route_claims` còn live luôn thắng kèm cảnh báo `shadowed_by_live_route` chính xác (không cờ giả cho bài draft/trash).
5. **Hỗ trợ regex/wildcard redirect ngay V1:**
   - *Lý do bác bỏ:* Regex làm phức tạp phát hiện vòng lặp hai chiều, tạo nguy cơ ReDoS và mất lợi thế tra cứu `O(log N)` trên `UNIQUE COLLATE BINARY`.

---

## Non-Functional Requirements

- **Bảo mật:**
  - Admin-only toàn phần trên UI (`isRouteAllowed`) và server (`require_admin`, `require_admin_with_csrf`, `AdmittedMutation`).
  - Chặn open redirect (`//evil.com`, `/\`), chặn non-http(s) schemes (`javascript:`, `data:`), chặn CRLF header injection trong `Location`, chống CSV formula injection (kiểm tra cả raw leading `\t`/`\r`/`\n` lẫn chuỗi sau `trim_start()`), chống path aliasing (`%2F`, `%252F`, `%2e%2e`, `%61` -> `400`), và kiểm tra `is_reserved_system_root` không phân biệt hoa-thường trước & sau decode UTF-8.
  - Không lưu IP; làm sạch credentials trong `Referer` và giới hạn độ dài `Referer`/`User-Agent`.
- **Hiệu năng & Bộ nhớ:**
  - `O(log N)` lookup trên SQLite B-tree; `0` SQLite write trên hot path HTTP `GET` công khai.
  - Bộ đệm RAM tối đa `512` keys/map, cooldown flush `>= 1s`, cắt tỉa DB giữ tối đa `5.000` 404 logs, trần `20.000` manual redirects, CSV tối đa `2 MiB` / `5.000` dòng.
  - Kiểm tra vòng lặp & chuỗi `<= 3` hop (có `visited` guard) khi import 5.000 dòng chạy in-memory `HashMap` `O(N)` trong `< 10ms` thay vì gọi SQL theo từng dòng.
- **Bảo toàn dữ liệu & Rollback:**
  - Migration forward-only `+6` objects (`221 → 227` migration objects; `223 → 229 < 256` tổng `sqlite_schema` tính cả `_sqlx_migrations`), tương thích 100% `VACUUM INTO` Backup và `restore::validate_restored_db_read_only()`.
  - Rollback sau phát hành bằng phục hồi bản sao lưu trước nâng cấp qua Safe Mode; từng Slice trong quá trình phát triển có thể revert độc lập trước khi áp dụng lên DB production.

---

## Detailed Implementation Plan

### Target Files to Create/Modify (Chuẩn Feature-First & Trần `600 LOC`)

> **Bất biến kích thước module:** Mọi file nguồn mới (`.rs`, `.ts`, `.tsx` trong `cms/`) **không vượt quá `600` LOC** (`cms/scripts/check-standards.py`); các file nằm trong `cms/scripts/loc-baseline.json` (`app.rs` `883`, `i18n.rs` `12489`, `App.tsx` `879`, `en.ts`/`vi.ts` `2346`) không được vượt ngưỡng baseline. Tuyệt đối không đặt tên module chung chung kiểu `part_*`.

1. **Core Domain (`opendash-core`, thuần Rust):**
   - `cms/crates/opendash-core/src/id.rs`: Thêm `pub const RESERVED_SYSTEM_ROOTS: &[&str]` và `pub fn is_reserved_system_root(root: &str) -> bool`.
   - `cms/crates/opendash-core/src/redirects/` *(tạo mới Feature-First, mỗi file `<= 600 LOC`)*:
     - `mod.rs`: Hằng số giới hạn, `RedirectValidationError`, `CsvRowError`, `RedirectStatusCode`, `ConflictPolicy`, `CsvImportRowStatus`, DTO structs (`#[serde(deny_unknown_fields)]`, gồm `error`/`code` trên `CsvImportResponse`) và re-exports.
     - `path.rs`: `decode_safe_legacy_request_path` (chỉ nhận `%20` + UTF-8 `>= 0x80`, không nhúng giới hạn độ dài), `normalize_redirect_source_path`, `normalize_redirect_target_url`, `extract_internal_target_path`, `extract_effective_target_path`, `check_not_self_redirect`, `build_redirect_location`, `normalize_redirect_note`, `sanitize_referer`, `sanitize_user_agent`.
     - `csv.rs`: `parse_redirects_csv`, `format_redirects_csv` (RFC 4180 + chống CSV formula injection) + unit tests tách gọn (`tests/` nếu cần).
   - `cms/crates/opendash-core/src/lib.rs`: Khai báo `pub mod redirects;`.
   - `cms/crates/opendash-core/README.md`: Cập nhật số module (`16 -> 17`) và mô tả `redirects/` + `id::is_reserved_system_root`.

2. **Database Migration:**
   - `cms/migrations/0057_manual_redirects_and_404_log.sql` *(tạo mới, slot kế tiếp ngay sau `0056_unified_field_content_cutover.sql`)*: Tạo bảng `manual_redirects`, `not_found_logs`, và 2 index `idx_not_found_logs_last_seen`, `idx_not_found_logs_hit_count` (`+6` objects -> `227`/`229 < 256`).
   - `cms/migrations/README.md`: Ghi nhận migration `0057` (`+6` objects).

3. **Server Storage, Telemetry, Public Routing & API (`opendash-server`):**
   - `cms/crates/opendash-server/src/public_context.rs` & `cms/crates/opendash-server/src/seo.rs`: Re-export `opendash_core::id::is_reserved_system_root`.
   - `cms/crates/opendash-server/src/storage/permalinks/{mod.rs, live_claims.rs}`: Bổ sung module `live_claims.rs` (giữ `public_resolve.rs` dưới `600 LOC`) với `pub(crate) async fn find_live_claim_kinds_for_paths_in_conn(...)` (dùng `td.key = tm.kind` và `json_extract(t.definition_json, '$.settings.{public,has_archive}')`) và helper lấy tập `permalink_route_redirects` còn đích live.
   - `cms/crates/opendash-server/src/storage/redirects/` *(tạo mới Feature-First, mỗi file `<= 600 LOC`, sở hữu 100% SQL và chỉ trả `RedirectStorageError`)*:
     - `mod.rs`: `RedirectStorageError`, kiểu kết quả storage, `find_active_manual_redirect`, `flush_telemetry_batch` / `flush_telemetry_batch_in_tx`.
     - `rules.rs`: `list_manual_redirects`, `get_manual_redirect_by_id`, `create_manual_redirect`, `update_manual_redirect_cas`, `delete_manual_redirect`, `export_manual_redirects_csv`.
     - `graph_and_import.rs`: `validate_redirect_graph`, `import_manual_redirects_csv` (`ON CONFLICT DO UPDATE` giữ stats khi `overwrite`).
     - `not_found_logs.rs`: `list_not_found_logs`, `delete_not_found_log`, `bulk_delete_not_found_logs`, `clear_not_found_logs`, prune `<= 5_000`.
   - `cms/crates/opendash-server/src/storage/mod.rs` & `cms/crates/opendash-server/src/storage/README.md`: Khai báo `pub mod redirects;` và đồng bộ số lượng (`33 -> 34` submodules, bao gồm vá số `32` cũ trước đó do đã thêm `fields/`).
   - `cms/crates/opendash-server/src/redirects.rs` *(tạo mới, `<= 600 LOC`)*: `RedirectTelemetryBuffer` + `RedirectTelemetryDropGuard` (lock order `MutationLease -> flush_mutex -> TelemetryMaps -> SQLite`, timestamp `%Y-%m-%dT%H:%M:%S%.3fZ`, cap `512`, cooldown `1s` đợi không spin qua `tokio::select!`, `flush_now`, `evict_*`, `Drop` guard đăng ký `notified()` trước `closed.load()`).
   - `cms/crates/opendash-server/src/state.rs` *(tạo mới, `<= 600 LOC`)*: Tách `AppState` và các `impl FromRef<AppState>` (`app.rs:78–160`, re-export `pub use crate::state::AppState;` trong `app.rs`) để dành chỗ trống giữ `app.rs <= 883 LOC`.
   - `cms/crates/opendash-server/src/main.rs`: Khai báo `mod redirects; mod state;`, khởi tạo `(telemetry, telemetry_drop_guard)`, truyền `Some(telemetry.clone())` qua `build_router_with_telemetry(...)`, gọi `timeout(2s, telemetry.flush_now())` rồi `drop(telemetry_drop_guard)` trước `coordinator.drain_for_shutdown(15s)`.
   - `cms/crates/opendash-server/src/routes/redirects/` *(tạo mới Feature-First, mỗi file `<= 600 LOC`)*:
     - `mod.rs`: Ánh xạ `RedirectValidationError`/`CsvRowError`/`RedirectStorageError`/`QueryRejection`/`JsonRejection` sang `ApiError` & `CsvImportResponse` (`X-OpenDash-Error-Code` + `PrelocalizedErrorResponse`).
     - `rules.rs`: `list_redirects`, `get_redirect`, `create_redirect`, `update_redirect`, `delete_redirect`.
     - `csv.rs`: `import_redirects` (`State<AppState>`), `export_redirects`.
     - `not_found_logs.rs`: `list_not_found_logs`, `delete_not_found_log`, `bulk_delete_not_found_logs`, `clear_not_found_logs`.
   - `cms/crates/opendash-server/src/routes/mod.rs` & `cms/crates/opendash-server/src/routes/README.md`: Khai báo `pub mod redirects;` và đồng bộ số module (`24 -> 25`, bao gồm vá số `23` cũ).
   - `cms/crates/opendash-server/src/routes/seo.rs`: Cập nhật `public_fallback` nhận `Option<Extension<Arc<RedirectTelemetryBuffer>>>`, kiểm tra `is_reserved_system_root(&first_seg.to_ascii_lowercase())` trước & sau decode Nhánh A, giới hạn `<= 512` bytes khi tra manual redirect & log 404 ở cả hai nhánh, ưu tiên `Live Canonical > Manual Redirect > Auto-301 > 404`, chỉ ghi telemetry khi `method == Method::GET`.
   - `cms/crates/opendash-server/src/app.rs`: Thêm `build_router_with_telemetry(...)` (giữ nguyên `build_router`, `build_router_with_themes_root`, toàn bộ `.route(...)` trong `app.rs` cho `check-standards.py`, và giới hạn `<= 883` LOC nhờ `state.rs`), đăng ký 8 paths (`11` route handlers) Admin với `DefaultBodyLimit` (`5 MiB` cho `/import`, `16 KiB` CRUD/bulk-delete, `4 KiB` clear), gắn `.layer(Extension(telemetry))` (và `Extension(Arc::new(drop_guard))` chỉ ở nhánh `None`) **sau** `.fallback(...)`.
   - `cms/crates/opendash-server/src/i18n.rs`, `i18n_redirects.rs` *(tạo mới, `<= 600 LOC`)* & `i18n_tests.rs` *(tạo mới, `<= 600 LOC`)*: Tách `mod localization_middleware_tests` (`12337–12489`) sang `i18n_tests.rs` để giữ `i18n.rs <= 12489` LOC; đặt `pub(crate) async fn effective_locale_for_error`, thêm `#[derive(Clone, Copy)] pub(crate) struct PrelocalizedErrorResponse;` và short-circuit trong `localize_error_response` (`12232–12243`, trước `12272–12275`); khai báo toàn bộ bảng `REDIRECT_ERROR_ENTRIES` / `REDIRECT_TEMPLATE_MATCHERS` + unit test trong `i18n_redirects.rs` nối với `translate_api_message` và `exact_table_has_valid_codes_and_bidirectional_coverage` (`RedirectValidationError`, `CsvRowError`, `RedirectStorageError`, `QueryRejection`, `redirects.csv_import_*`, `ApiError(StatusCode::PAYLOAD_TOO_LARGE, ...)`).
   - `cms/crates/opendash-server/README.md`: Đồng bộ danh sách và số lượng module thực tế sau khi thêm `src/redirects.rs`, `src/state.rs`, `routes/redirects/` (`25` routes) và `storage/redirects/` (`34` storage submodules).

4. **React Admin UI (`cms/admin/src/`):**
   - `cms/admin/src/api/redirects.ts` *(tạo mới)*, `cms/admin/src/api/index.ts`, `cms/admin/src/api/README.md` (`21 -> 22` modules): API client qua `api/client.ts` (`requestJson`, `requestBlob` cho CSV export).
   - `cms/admin/src/features/operations/` *(mỗi file `<= 600 LOC`)*: `redirectsDraftStore.ts` (in-memory `Map` + `discardRedirectsDraft` + `clearRedirectsDrafts`), `RedirectsToolsPanel.tsx`, `ManualRedirectsPanel.tsx`, `RedirectFormModal.tsx`, `CsvImportModal.tsx`, `NotFoundLogPanel.tsx`.
   - `cms/admin/src/features/plugins/useAdminPlugins.ts` *(tạo mới, `<= 600 LOC`)*: Tách các handler plugin từ `App.tsx:560–637` để giữ `App.tsx <= 879` LOC theo `loc-baseline.json`.
   - `cms/admin/src/shell/router.ts`, `cms/admin/src/shell/AdminShellMenu.tsx`, `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/shell/README.md`, `cms/admin/src/App.tsx`: Thêm mục `Chuyển hướng & 404` (`#/tools/redirects`) trong `AdminShellMenu.tsx` (`buildMenuTree`), alias `#/tools/404-log`, highlight/breadcrumb trong `AdminShell.tsx`, và nối đủ `toolsDirtyRef` vào `mayLeave`, `beforeunload`, `handleLogout`, `handleLogin`, `handleOwnAccountChanged`, `hasUnsavedChanges` trong `App.tsx`.
   - `cms/admin/src/features/README.md`, `cms/admin/src/i18n/README.md`: Cập nhật danh mục component `operations/` / `plugins/` và số lượng key i18n.
   - `cms/admin/src/i18n/locales/{en.ts, vi.ts, fields.en.ts, fields.vi.ts, redirects.en.ts, redirects.vi.ts}` & `cms/scripts/check-i18n.mjs`: Tách `fields.{en,vi}.ts` và thêm `redirects.{en,vi}.ts` (`<= 600 LOC`, spread trong `en.ts`/`vi.ts` giữ `< 2346 LOC`), cập nhật `check-i18n.mjs` hỗ trợ đọc catalog spread + lọc `--strict` trên tất cả file `*.vi.ts`.

5. **Kiểm thử, Test Runner & Tài liệu:**
   - `cms/crates/opendash-server/tests/site_media_seo.rs` & `cms/crates/opendash-server/tests/site_media_seo/manual_redirects_404/{mod.rs, crud_and_precedence.rs, csv_and_telemetry.rs}` *(tạo mới, đăng ký vào domain binary `site_media_seo`, mỗi file `<= 600 LOC`)*: Integration tests đầy đủ cho CUJ 1–5, concurrency/lock-order/shutdown/eviction, và Backup/Restore fingerprint `< 256`.
   - `cms/crates/opendash-server/tests/ops/writer_barrier.rs`: Bổ sung coverage `AdmittedMutation` cho `routes/redirects/`.
   - `cms/scripts/run-tests.py`: Cập nhật `GROUP_MAP["site-media-seo"]` (trong tuple `GROUPS`) `unit_filters` (`"redirects::"`, `"storage::redirects"`, `"routes::redirects"`), hỗ trợ prefix `core_routes` cho `"redirects/"`, `migrations` `keyword_map` (`"redirect"`, `"404"`), và `server_routes` (prefix matching cho `"redirects.rs"`, `"storage/permalinks/"`, `"storage/redirects/"`, `"routes/redirects/"`; riêng `"state.rs"` cố ý không đưa vào `server_routes` để mọi sửa đổi `AppState` tự động escalate lên `all` như `app.rs`, và `tests/site_media_seo/*` đã tự định tuyến qua `dir_to_group["site_media_seo"]`).
   - `cms/scripts/manual_redirects_404_e2e.py` *(tạo mới)*, `cms/scripts/admin_navigation_e2e.py`, `cms/scripts/README.md`: Live Browser Playwright E2E và kiểm tra RBAC Admin/Editor trên `#/tools/redirects` & `#/tools/404-log`.
   - `cms/README.md`, `docs/contracts/{CORE_FOUNDATION.md,M7_SITE_FEATURES.md,M8_WRITER_INVENTORY.md}`, `docs/product/{PRODUCT_REQUIREMENTS.md,ADMIN_NAVIGATION_AND_EDIT_SCREEN.md}`, `docs/designs/SITEMAP.md`: Đồng bộ hợp đồng sau triển khai (`CORE_FOUNDATION.md` cập nhật `id::is_reserved_system_root`, `17` `opendash_core` modules, `34` storage submodules bao gồm `fields` & `redirects`, `22` API files + `requestBlob`, và `16 -> 17` trong `cms/README.md` vốn đang ghi lệch `15`).

---

### Slice S01: Core Domain, Migration SQLite & Storage Layer

- **Dependencies:** Design Spec được Owner phê duyệt (`Status: Approved`) và đồng bộ `origin/main`.
- **File Ownership:** *(mỗi file mới `<= 600 LOC`)*
  - `cms/crates/opendash-core/src/{id.rs,lib.rs}` & `cms/crates/opendash-core/src/redirects/{mod.rs,path.rs,csv.rs}`
  - `cms/crates/opendash-core/README.md`
  - `cms/migrations/0057_manual_redirects_and_404_log.sql`
  - `cms/migrations/README.md`
  - `cms/crates/opendash-server/src/public_context.rs`
  - `cms/crates/opendash-server/src/seo.rs`
  - `cms/crates/opendash-server/src/storage/{mod.rs,README.md}`
  - `cms/crates/opendash-server/src/storage/permalinks/{mod.rs,live_claims.rs}`
  - `cms/crates/opendash-server/src/storage/redirects/{mod.rs,rules.rs,graph_and_import.rs,not_found_logs.rs}`
  - `cms/scripts/run-tests.py`
- **Acceptance Criteria:**
  - `opendash_core::redirects` sử dụng `RedirectValidationError`/`CsvRowError` (0 `ApiError` trong S01) và vượt qua unit test cho chuẩn hóa path (`%20` giữ nguyên, UTF-8 multibyte giải mã, từ chối mọi ASCII `%XX` khác kể cả `%2F`/`%252F`/`%2e%2e`/`%61`, độ dài kiểm tại caller, target `%XX` bất hợp lệ trả `TargetUrlInvalid`), chặn open redirect `//` và non-http(s), punycode IDN, `build_redirect_location` với `#fragment` + control-strip, kiểm tra self-redirect cả với URL cùng `public_origin`, parser CSV RFC 4180 (BOM/CRLF/arbitrary header order + exact aliases/ignore `hit_count`/`last_hit_at`/per-row errors/default `301` & `enabled=true`/header-only empty/5000 rows/2 MiB) và exporter formula-safe (`'` trước cả raw `\t\r\n` và `trim_start()` `=+-@\t\r\n`).
  - Migration `0057` áp dụng sạch lên DB trống và `cms/fixtures/baseline_0001.sqlite`, giữ `sqlite_schema < 256` (`+6` objects -> `227`/`229`) và vượt qua `restore::validate_restored_db_read_only`.
  - `storage::redirects` cài đặt CRUD + `get_manual_redirect_by_id` + CAS `version`, dùng `permalinks::find_live_claim_kinds_for_paths_in_conn` (`tm.kind` + `definition_json`) cho `shadowed_by_live_route` (không cờ giả draft/trash), bidirectional cycle/chain check `<= 3` hops in-memory O(N) có `visited` guard + `exclude_id` trên `PUT` + `effective_edges` (`manual_edges > auto_edges`) + `SkipExisting` không sửa đồ thị + `QuotaExceeded` trong `dry_run` (`existing_count + create_count >= 20_000` trước khi tăng `create_count`), CSV dry-run (read tx) / import nguyên tử (`ON CONFLICT DO UPDATE` giữ stats khi `overwrite`), và `flush_telemetry_batch` / `not_found_logs` prune `<= 5.000`. (`storage::redirects` chỉ trả `RedirectStorageError`, không chứa `ApiError` literal).
  - `cms/scripts/run-tests.py` cập nhật đủ cấu trúc routing (gồm cả `storage/permalinks/`); `run-tests.py --self-test`, `check-standards.py` (kể cả ngưỡng `600 LOC`), và `run-tests.py --changed` PASS (khi `cms/admin/dist/` đã build sẵn).
- **Rollback:** `git revert` commit của S01.

### Slice S02: Telemetry Buffer, Public Redirect/404 Integration & Admin HTTP APIs

- **Dependencies:** Slice S01 hoàn tất và PASS kiểm tra chuẩn.
- **File Ownership:** *(mỗi file mới `<= 600 LOC`)*
  - `cms/crates/opendash-server/src/{main.rs,app.rs,state.rs,redirects.rs,i18n.rs,i18n_redirects.rs,i18n_tests.rs}`
  - `cms/crates/opendash-server/src/routes/{mod.rs,seo.rs,README.md}` & `cms/crates/opendash-server/src/routes/redirects/{mod.rs,rules.rs,csv.rs,not_found_logs.rs}`
  - `cms/crates/opendash-server/README.md`
  - `cms/crates/opendash-server/tests/site_media_seo.rs` & `cms/crates/opendash-server/tests/site_media_seo/manual_redirects_404/{mod.rs,crud_and_precedence.rs,csv_and_telemetry.rs}`
  - `cms/crates/opendash-server/tests/ops/writer_barrier.rs`
- **Acceptance Criteria:**
  - `public_fallback` thực thi đúng thứ tự `Reserved System Root case-insensitive (404 no-log) > Safe %XX (400 cho %XX ASCII ngoài %20, UTF-8 >= 0x80 tới manual redirect) / Canonical Claim > Manual Redirect 301/302 > Auto-301 > 404`, giữ mọi security regression test (`%2F`, `%252F`, `%2e%2e`, `%61` -> `400`), giới hạn `<= 512` bytes ở cả hai nhánh khi tra redirect & log 404, và chỉ ghi telemetry trên `Method::GET`.
  - `RedirectTelemetryBuffer` tuân thủ nghiêm thứ tự khóa `MutationLease → flush_mutex → TelemetryMaps → SQLite`, timestamp UTC `%Y-%m-%dT%H:%M:%S%.3fZ`, cap `512` (bỏ qua key mới khi đầy), cooldown `1s` (chờ `remaining_cooldown` bằng `tokio::select!`), `.layer(Extension(...))` đặt **sau** `.fallback(...)` ở `app.rs`, `DropGuard` đặt `notified()` trước `closed.load()` (chỉ gắn vào router cho nhánh fallback `None`), `main.rs` flush timeout `2s` + drop guard trước `drain_for_shutdown(15s)`, và evict tức thì dưới `flush_mutex` (`clear` theo `older_than_days` flush pending RAM trong cùng tx trước khi `DELETE`).
  - Toàn bộ 8 API paths (`11` handlers `/api/admin/redirects*`, `/api/admin/404-logs*`, gồm `GET /api/admin/redirects/{id}`) áp dụng `require_admin` / `require_admin_with_csrf`, `AdmittedMutation`, `parse_if_match`, `QueryRejection`/`JsonRejection` localized, `DefaultBodyLimit` (`5 MiB` cho `/import`), quy tắc ánh xạ lỗi thống nhất (`CsvTooLarge -> 413`, mọi `RedirectValidationError` khác -> `422`, extractor/header/filter -> `400`), `Cache-Control: no-store`, trả `403` cho `Editor`, dịch trực tiếp `CsvPreviewRow.error` + top-level `error` kèm `PrelocalizedErrorResponse` (giữ nguyên `X-OpenDash-Error-Code` trên `422`/`409` kể cả payload `> 1 MiB`), và `i18n_redirects.rs` kiểm tra mọi variant lỗi + explicit `413` ở cả `en` và `vi` trong khi `i18n.rs <= 12489` LOC và `app.rs <= 883` LOC.
  - Domain suites `site_media_seo` (bao gồm `manual_redirects_404`) & `ops` (`writer_barrier`) PASS cùng `check-standards.py` và `run-tests.py --changed`.
- **Rollback:** `git revert` commit của S02.

### Slice S03: Admin UI (`Công cụ → Chuyển hướng & 404`), I18n, Browser E2E & Contract Sync

- **Dependencies:** Slice S02 hoàn tất và PASS kiểm tra chuẩn.
- **File Ownership:** *(mỗi file mới `<= 600 LOC`)*
  - `cms/admin/src/api/{redirects.ts,index.ts,README.md}`
  - `cms/admin/src/features/operations/{redirectsDraftStore.ts,RedirectsToolsPanel.tsx,ManualRedirectsPanel.tsx,RedirectFormModal.tsx,CsvImportModal.tsx,NotFoundLogPanel.tsx}`
  - `cms/admin/src/features/plugins/useAdminPlugins.ts`
  - `cms/admin/src/features/README.md`
  - `cms/admin/src/shell/{router.ts,AdminShellMenu.tsx,AdminShell.tsx,README.md}`
  - `cms/admin/src/App.tsx` *(giữ `<= 879 LOC` nhờ tách `useAdminPlugins.ts`)*
  - `cms/admin/src/i18n/{README.md,locales/en.ts,locales/vi.ts,locales/fields.en.ts,locales/fields.vi.ts,locales/redirects.en.ts,locales/redirects.vi.ts}`
  - `cms/scripts/{check-i18n.mjs,admin_navigation_e2e.py,manual_redirects_404_e2e.py,README.md}`
  - `cms/README.md`
  - `docs/product/{PRODUCT_REQUIREMENTS.md,ADMIN_NAVIGATION_AND_EDIT_SCREEN.md}`
  - `docs/contracts/{CORE_FOUNDATION.md,M7_SITE_FEATURES.md,M8_WRITER_INVENTORY.md}`
  - `docs/designs/SITEMAP.md`
- **Acceptance Criteria:**
  - Menu `Công cụ` trong `AdminShellMenu.tsx` hiển thị `Chuyển hướng & 404` (`#/tools/redirects`), alias `#/tools/404-log` đồng bộ tab `Nhật ký 404` hai chiều qua `onNavigateHash` (tự đóng modal khi `mayLeave` hủy nháp), và chỉ `Admin` truy cập được (`Editor` thấy `rbac-forbidden`).
  - Bảo toàn nháp qua `401` (module-level RAM `Map` keyed `${siteInstanceId}:${user.id}`) và `412` (rebase qua `GET /api/admin/redirects/{id}` hoặc giữ nháp với nút Lưu bị khóa tới khi rebase), nối đủ `toolsDirtyRef` vào `mayLeave`, `beforeunload`, `handleLogout`, `handleLogin` site/user switch, và `handleOwnAccountChanged` (giữ `App.tsx <= 879 LOC`).
  - Đầy đủ CRUD redirect, cảnh báo `shadowed_by_live_route`, Dry-Run Preview & Import CSV (preflight `2 MiB`, vô hiệu nút nhập khi sửa text/policy, reset `preview = null` khi commit `422`/`409`), Xuất CSV tải bằng `requestBlob` (giữ nguyên UTF-8 BOM), 404 Log table (3 chế độ sort), 1-click Tạo Redirect mở modal tại chỗ trên `#/tools/404-log` và làm sạch dòng 404 + tăng `redirectsRefreshRevision` (và `notFoundRefreshRevision` ở chiều ngược lại), bulk delete & clear log.
  - `node cms/scripts/check-i18n.mjs --strict` (kèm `en.ts`/`vi.ts <= 2346 LOC`), `check-standards.py`, `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, toàn bộ Playwright Live Browser E2E (`admin_navigation_e2e.py` + `manual_redirects_404_e2e.py`), và Rust tests PASS.
- **Rollback:** `git revert` commit của S03 trên giao diện Admin & tài liệu.

---

### Verification Plan

Chạy theo đúng thứ tự (`npm run build` trước `run-tests.py` để `rust-embed` nhúng bundle mới nhất) từ gốc worktree:

```bash
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
node cms/scripts/check-i18n.mjs --strict
npm --prefix cms/admin ci
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
cargo fmt --manifest-path cms/Cargo.toml --all --check
python3 cms/scripts/run-tests.py --self-test
python3 cms/scripts/run-tests.py --changed
python3 cms/scripts/run-tests.py --group site-media-seo,ops
cargo build --manifest-path cms/Cargo.toml --bin opendash --locked
git diff --check
```

Và chạy kiểm chứng Live Browser E2E + Release gate trước khi đóng Spec sang `Verified`:
- `python3 cms/scripts/run-tests.py --group all --release` (vì sửa `app.rs` & `opendash-core/src/id.rs`).
- Chạy `python3 cms/scripts/admin_navigation_e2e.py --binary cms/target/debug/opendash ...` và `python3 cms/scripts/manual_redirects_404_e2e.py --binary cms/target/debug/opendash ...` kiểm tra toàn bộ CUJ 1–5 ở cả `en` và `vi`.
- Lưu biên bản kiểm chứng tại `docs/verification/`, chạy `/egm-drift-report`, cập nhật `SITEMAP.md` sang `Verified` và chạy `node cms/scripts/codegraph-index-docs.mjs`.
