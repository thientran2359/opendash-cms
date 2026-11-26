# Đặc tả Thiết kế EGM — Bản địa hóa Giao diện Admin (`cms_locale`), Tách biệt Cài đặt Đa ngôn ngữ & Công cụ Dọn Dữ liệu Mồ côi

- **Status:** Approved
- **Approved By:** Owner
- **Approval Evidence:** Owner confirmed "Duyệt Spec" on 2026-11-25 (authorizing PRD V1 scope expansion for Admin UI & API L10n + Settings Multilingual per ADR-0010)
- **Goldfish Review:** 3/3 APPROVED zero-context (egm-design R3 c7a3b38f: 0 Blocker / 0 Major / 3 Nit applied; egm-codebase-details R3 765fb32d: 0 Blocker / 0 Major / 1 Nit applied; egm-implementation-readiness R4 853c5aa3: 0 Blocker / 0 Major / 0 Nit)
- **Date:** 2026-11-24
- **Author:** Orion (Elephant)
- **Baseline:** `main@f7cc9520dea764214ad055a5a95a72c8515e4536`
- **Requires Owner Approval (PRD Scope Update):** Có — mở rộng [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md) (§1, §3, §6, §7 gỡ bỏ hạn chế sau-V1 *Admin UI & API L10n*) theo [ADR-0010](../decisions/ADR-0010-agent-project-and-merge-authority.md). Cập nhật PRD tiến hành cùng lúc khi Owner phê duyệt Spec và trước/trong Slice S01.
- **Components:** `cms/migrations/0051_admin_i18n_and_multilingual_settings.sql`, `cms/crates/opendash-core/src/{i18n,lib}.rs`, `cms/crates/opendash-server/src/{app,error,i18n,main}.rs`, `cms/crates/opendash-server/src/plugin/runtime.rs`, `cms/crates/opendash-server/src/routes/{operations,plugin,settings}.rs`, `cms/crates/opendash-server/src/storage/{builder_binding,dashboard,entries,entry_table,media_refs,overrides,permalinks,plugin,public_content,seo,settings,sitemap,taxonomy}.rs`, `cms/admin/{index.html,src/{App.tsx,types.ts,i18n/,api/,components/,features/,shell/,studio/,builder/}}`, `cms/scripts/{check-i18n.mjs,admin_i18n_multilingual_e2e.py}` (+ 52 legacy `cms/scripts/*.py` liệt kê ở Slice S05), `docs/{product/{PRODUCT_REQUIREMENTS,ADMIN_NAVIGATION_AND_EDIT_SCREEN},contracts/M8_WRITER_INVENTORY,designs/SITEMAP}.md`, `cms/migrations/README.md`
- **Related:** [`docs/designs/multilingual-content-and-theme-l10n.md`](multilingual-content-and-theme-l10n.md), [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md), [`docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`](../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md), [`docs/contracts/CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`docs/contracts/PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md)

---

## Overview

Đặc tả này triển khai **Lớp 3 (Admin UI & API L10n)** nối tiếp hạ tầng đa ngôn ngữ Lớp 1–2 đã hoàn tất trong [`multilingual-content-and-theme-l10n.md`](multilingual-content-and-theme-l10n.md), đồng thời tách biệt rạch ròi giữa **Ngôn ngữ CMS (`cms_locale`)** và **Ngôn ngữ nội dung (`site_locales`)** tương tự mô hình WordPress Settings:

1. **Tách hoàn toàn hai trục ngôn ngữ:**
   - **Ngôn ngữ CMS (`site_general_settings.cms_locale`):** Điều khiển ngôn ngữ của toàn bộ giao diện React Admin (Auth Setup/Login, Sidebar, Dashboard, Editors, Settings, OpenDash Studio, Toolbar, `Intl.DateTimeFormat`, `<html lang dir>`, `<title>`) và mọi thông báo lỗi HTTP ở phạm vi Admin/Auth/Builder (`/admin`, `/builder`, `/auth/*`, `/api/admin/*`, `/api/themes/*`, `/registry`, `/page`, `/preview`, `/save`). Đây là cấu hình dùng chung toàn site giống WordPress `Settings -> General -> Site Language`, lấy **English (`en`)** làm mặc định cho cả site cài mới lẫn nâng cấp, hỗ trợ bản dịch đầy đủ đầu tiên **Tiếng Việt (`vi`)** cùng danh mục `32` ngôn ngữ BCP-47 chuẩn WordPress (các ngôn ngữ chưa có gói dịch hiển thị sẵn trong `<select>` và tự động fallback sạch về `en`). Không phân mảnh theo từng tài khoản user hay header trình duyệt.
   - **Ngôn ngữ nội dung (`site_locales` + `site_locales_state.multilingual_enabled`):** Điều khiển nội dung công khai (bài viết, trang, CPT, taxonomy terms, menus, SEO trang chủ, URL `/<code>/...`, `hreflang`, `sitemap.xml`, lỗi HTML/text public như contact form).
2. **Màn hình riêng `Cài đặt -> Đa ngôn ngữ` (`Settings -> Multilingual`, `#/settings/multilingual`):**
   - Tách `<ContentLocalesSection />` khỏi [`GeneralSettingsPanel.tsx`](../../cms/admin/src/features/system/GeneralSettingsPanel.tsx) sang màn hình chuyên biệt `cms/admin/src/features/settings/MultilingualSettingsPage.tsx`.
   - Bổ sung checkbox tổng **Enable multilingual content support** (`multilingual_enabled`). Khi tắt (`false`), Admin ẩn toàn bộ tab/công cụ đa ngôn ngữ ở Editor/Taxonomy/Menus/SEO/Studio và website công khai chỉ phục vụ ngôn ngữ mặc định (`404` trên `/en/...`, ẩn `hreflang`, alternate sitemaps và theme language switcher), đồng thời bảo toàn **100%** dữ liệu ngôn ngữ phụ trong SQLite. Khi nâng cấp migration `0051`, các site hiện hữu đang có `COUNT(*) > 1` trong `site_locales` tự động đặt `multilingual_enabled = 1`.
   - Hỗ trợ **thêm, sửa (`label`/`direction`), bật/tắt, đặt mặc định và xóa ngôn ngữ phụ** (`DELETE /api/admin/locales/{code}`).
3. **Công cụ Dọn Dữ liệu Mồ côi của Ngôn ngữ Đã Xóa (Orphaned Locale Data Purge):**
   - Do [`0047_multilingual_content_and_theme_l10n.sql`](../../cms/migrations/0047_multilingual_content_and_theme_l10n.sql) đặt `ON DELETE RESTRICT` trên 13 bảng tham chiếu `site_locales(code)`, thao tác `DELETE /api/admin/locales/{code}` trước hết rút claims tuyến công khai (`permalink_route_claims`), chuyển các `pattern` dùng chung về ngôn ngữ mặc định nếu cần, và xóa cứng ngay nếu ngôn ngữ đó có `0` bản ghi dữ liệu do người dùng/theme tạo, hoặc soft-delete (`deleted_at` + `enabled = 0`) nếu vẫn còn dữ liệu con.
   - Ngay bên dưới bảng Ngôn ngữ nội dung trên `#/settings/multilingual`, công cụ **Orphaned Locale Data** thống kê chính xác số lượng bản ghi còn lại theo từng nhóm (`entries`, `terms`, `settings`, `menu_slots`, `routes`, `projections`, `media_seo_refs`) và chỉ xóa vĩnh viễn theo thứ tự tô-pô an toàn sau hộp thoại xác nhận nhập đúng mã ngôn ngữ (`confirm_code`). Tài nguyên dùng chung toàn site (`content_type_key = 'pattern'`, `media_assets`, `site_menus`) được bảo toàn (riêng `pattern` nếu từng lưu theo mã ngôn ngữ sắp xóa/purge sẽ được chuyển sang ngôn ngữ mặc định hiện hành kèm khử trùng slug; xóa entries sẽ tự động giải phóng pins media qua `collect_pins_in_tx`).
4. **Hàng rào Ngân sách Schema & Zero External Runtime Dependencies:**
   - Migration `0051` chỉ dùng `ALTER TABLE ... ADD COLUMN` (**`+0` đối tượng mới trong `sqlite_schema`**, giữ tổng đối tượng ở `219 <= MAX_SCHEMA_OBJECTS = 256` theo [`storage/restore.rs`](../../cms/crates/opendash-server/src/storage/restore.rs#L14)).
   - React Admin không thêm dependency ngoài nào vào [`cms/admin/package.json`](../../cms/admin/package.json); backend không thêm crate nào vào `cms/Cargo.toml` / `cms/Cargo.lock`.

---

## The Problem

1. **Giao diện Admin & `ApiError` đang cố định tiếng Việt dù nội dung có hỗ trợ đa ngôn ngữ:**
   - `86/95` file `.ts`/`.tsx` trong `cms/admin/src/` (`2,299` dòng), `cms/admin/index.html` và `78` file Rust (`1,786` điểm gọi `ApiError` cùng 18 chuỗi mojibake `?` trong `storage/entries.rs`, JSON vetoes, và một số chuỗi lỗi tiếng Anh rải rác ở `routes/themes.rs`, `routes/builder.rs`, `routes/operations.rs`) chưa có cơ chế bản địa hóa Admin/API theo `cms_locale`.
   - [`GeneralSettingsPanel.tsx`](../../cms/admin/src/features/system/GeneralSettingsPanel.tsx) đang gộp chung cả `<PublicSiteSettingsSection />` lẫn `<ContentLocalesSection />`, chưa có lựa chọn **CMS Language** tách biệt với **Content Language**.
2. **Thiếu công tắc tổng & tính năng xóa ngôn ngữ phụ / dọn dữ liệu mồ côi:**
   - [`site_locales_state`](../../cms/migrations/0047_multilingual_content_and_theme_l10n.sql) mới chỉ có `version` và `rename_in_progress`, chưa có cờ `multilingual_enabled`.
   - [`app.rs`](../../cms/crates/opendash-server/src/app.rs#L309-L321) và [`routes/settings.rs`](../../cms/crates/opendash-server/src/routes/settings.rs#L140-L201) chưa có `PUT /api/admin/locales/multilingual`, `DELETE /api/admin/locales/{code}`, hay endpoint thống kê/purge dữ liệu mồ côi. Các ràng buộc `ON DELETE RESTRICT` (`entry_terms.term_id`, `taxonomy_terms.parent_id`, `site_locales.code`), `PRIMARY KEY (source_path)` trên `permalink_route_redirects`, và trigger `0047` đòi hỏi đúng trình tự cập nhật/xóa tô-pô trong transaction.
3. **Bootstrap chưa xác thực thiếu `cms_locale`:**
   - `GET /auth/me` trả `401 Unauthorized` trước khi đăng nhập; màn hình `AuthPanel` (Setup Admin lần đầu & Login) cần đọc cấu hình `cms_locale` công khai để hiển thị đúng ngôn ngữ ngay trước khi có session.

---

## Goals

1. **Ngôn ngữ CMS (`Settings -> General`):** Cho phép chọn `cms_locale` toàn site (mặc định `en`) từ danh mục chuẩn `32` ngôn ngữ WordPress, tự động dịch 100% React Admin SPA và 100% response lỗi Admin/Auth/Builder HTTP (`text/plain` + `application/json` + `X-OpenDash-Error-Code`) cho cả `en` và `vi`, mọi ngôn ngữ chưa có gói dịch fallback nhất quán về `en`.
2. **Màn hình riêng `Settings -> Multilingual` (`#/settings/multilingual`):** Tách cấu hình ngôn ngữ nội dung khỏi `General`; cung cấp checkbox `multilingual_enabled` (tắt đi giữ nguyên 100% DB, upgrade site `> 1` ngôn ngữ tự bật `1`), CRUD + Edit + Delete ngôn ngữ phụ, và công cụ Purge tô-pô dữ liệu mồ côi có đếm chi tiết và xác nhận nhập mã.
3. **Chính xác, an toàn & hiệu năng cao:** Không query SQLite trong error middleware khi bình thường (`AppState.ui_locale` dùng `Arc<AtomicU8>` cập nhật lúc startup và khi save `general`), không phá `Content-Length`, không rò rỉ `cms_locale` sang route `/api/public/*`, không xóa nhầm `pattern`/`media_assets`/`site_menus`.
4. **Kiểm định tự động & Đồng bộ tài liệu:** Bổ sung `cms/scripts/check-i18n.mjs` kiểm parity `en.ts` ↔ `vi.ts` + chặn tiếng Việt hardcode mới; cập nhật [`PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md), [`ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`](../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md), [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md), [`cms/migrations/README.md`](../../cms/migrations/README.md) và các browser/smoke scripts bị ảnh hưởng bởi `en` default.

## Non-Goals

- **Không** hỗ trợ ngôn ngữ giao diện riêng theo từng user (`users.ui_locale`) hoặc tự động chọn theo `Accept-Language` của trình duyệt.
- **Không** dịch dữ liệu nội dung do người dùng nhập trong DB (tên CPT/field, bài viết, `site_title`/`tagline`, labels declarative plugin/theme).
- **Không** dịch trang HTML public hoặc thông báo lỗi public (`/api/public/contact/*`, `/robots.txt`, `/sitemap.xml`) theo `cms_locale` — phần công khai tiếp tục dùng **Ngôn ngữ nội dung** + Theme L10n Lớp 1–2.
- **Không** xóa file/row `media_assets`, `site_menus` hay `content_type_key = 'pattern'` khi Purge một ngôn ngữ đã xóa.
- **Không** tải gói ngôn ngữ từ mạng lúc chạy (toàn bộ catalog & từ điển nhúng sẵn trong binary).

---

## Critical User Journeys (CUJs)

### CUJ-1: Cài đặt mới & Đổi ngôn ngữ CMS giữa Tiếng Anh và Tiếng Việt
- **Given:** Site mới khởi tạo sau migration `0051` (`cms_locale = 'en'`, `multilingual_enabled = 0`, content locale mặc định `vi`).
- **When:** Chủ site mở `/admin`, tạo admin lần đầu tại Auth Setup, đăng nhập, vào `Settings -> General` đổi **CMS Language** sang `Tiếng Việt (vi)` rồi lưu (với `If-Match`), sau đó kích hoạt một lỗi nhập liệu hoặc `412`.
- **Then:** Ngay từ màn hình Setup/Login, toàn bộ Admin hiển thị **English (`en`)**; khi đổi sang `vi` và lưu thành công, toàn bộ Sidebar, tiêu đề trang, form, OpenDash Studio, định dạng ngày (`Intl.DateTimeFormat`), `<html lang="vi">`, `<title>`, và mọi HTTP `4xx/5xx` Admin/Auth/Builder chuyển tức thì sang tiếng Việt (kèm header `X-OpenDash-Error-Code`) không cần tải lại trang. Ngôn ngữ nội dung công khai `vi` vẫn giữ nguyên.

### CUJ-2: Chọn ngôn ngữ chưa có bản dịch đầy đủ (Fallback an toàn về English)
- **Given:** Admin chọn `日本語 — Japanese (ja)` hoặc `العربية — Arabic (ar)` (`installed: false`) trong `<select data-testid="cms-locale-select">` tại `Settings -> General` và nhấn Lưu.
- **When:** Giao diện Admin tải lại hoặc gửi request lỗi.
- **Then:** DB lưu `site_general_settings.cms_locale = 'ja'` (hoặc `'ar'`), `<select>` hiển thị ngôn ngữ đã chọn kèm ghi chú translation pack chưa có sẵn; đồng thời `effective_locale = "en"`, `effective_bcp47 = "en-US"`, `effective_direction = "ltr"` để UI Admin và thông báo lỗi HTTP hiển thị sạch bằng **English (`en`)**, không bị lật `rtl` nửa chừng và không lộ key thô.

### CUJ-3: Tắt/Bật hỗ trợ đa ngôn ngữ tại `Settings -> Multilingual` mà bảo toàn dữ liệu
- **Given:** Website đang có 2 ngôn ngữ nội dung (`vi` mặc định, `en` phụ) kèm bài viết/category/menu/home SEO cho `en`.
- **When:** Admin mở `#/settings/multilingual`, bỏ tick `[ ] Enable multilingual content support` và xác nhận trên hộp thoại xác nhận; kiểm tra public `/en/posts`, thẻ `hreflang`, `sitemap.xml`, `language-switcher` và Editor trong Admin; sau đó tick bật lại `[x]`.
- **Then:** Khi tắt (`multilingual_enabled = 0`), Editor/Taxonomy/Menus/SEO/Studio ẩn hết tab/badge ngôn ngữ; mọi route phụ công khai (`/en/...`) trả `404`, `hreflang` & alternate sitemaps & theme switcher alternates biến mất, **nhưng 100% row DB được giữ nguyên** (trường `content_language` trên `#/settings/general` vẫn bị khóa vì vẫn còn ngôn ngữ phụ trong DB). Khi tick bật lại `[x]`, toàn bộ dữ liệu và route `/en/...` hoạt động lại bình thường (hoặc báo `409` kèm danh sách xung đột slug nếu có va chạm phát sinh trong lúc tắt).

### CUJ-4: Xóa ngôn ngữ phụ & Dọn dữ liệu mồ côi có xác nhận an toàn
- **Given:** Website bật đa ngôn ngữ có `vi` (default), `en`, `fr`, trong đó `fr` có entries (kèm SEO/media ref/redirects/projections), category cha–con, menu slot và `site_home_seo`; ngoài ra có một `pattern` dùng chung từng tạo lúc mặc định cũ là `fr`.
- **When:** Admin mở `Settings -> Multilingual`, thử xóa mặc định `vi` (bị chặn), xóa một ngôn ngữ rỗng `de` vừa tạo (0 dữ liệu con nhưng có thể từng tạo pattern), rồi xóa `fr` (có dữ liệu con) và thực hiện Purge `fr` sau khi nhập đúng `confirm_code`.
- **Then:** Ngôn ngữ mặc định không thể xóa (`422 locale.cannot_delete_default`); ngôn ngữ rỗng `de` chuyển pattern (nếu có, kèm chống trùng slug) sang mặc định, rút route claim và xóa hẳn khỏi `site_locales` ngay lập tức (`soft_deleted: false`); ngôn ngữ `fr` chuyển ngay sang soft-delete (`deleted_at IS NOT NULL`, biến mất khỏi public/Admin/dropdowns/Dashboard counts, hồi sinh nguyên vẹn nếu Admin thêm lại `fr` trước khi Purge), và xuất hiện trong bảng **Orphaned Locale Data** với số liệu chính xác. Khi Admin bấm Purge và gõ `fr`, hệ thống chuyển `pattern` sang ngôn ngữ mặc định hiện tại rồi xóa sạch mọi dữ liệu thuộc `fr` trong 1 transaction nguyên tử, giữ `media_assets` & `site_menus`, đồng thời giải phóng media pins không còn trang nào dùng.

---

## Technical Plan (Architecture)

### 1. Schema SQLite (`cms/migrations/0051_admin_i18n_and_multilingual_settings.sql`)

Tổng số đối tượng trong `sqlite_schema` sau `0050` đã kiểm chứng thực tế là **`219`** (`80` tables gồm `_sqlx_migrations`, `122` indexes gồm `sqlite_autoindex_*`, `17` triggers), trong ngưỡng [`MAX_SCHEMA_OBJECTS = 256`](../../cms/crates/opendash-server/src/storage/restore.rs#L14). Migration `0051` giữ nguyên delta **`+0`** đối tượng `sqlite_schema` (`219 -> 219`):

```sql
-- 0051_admin_i18n_and_multilingual_settings.sql
-- Net sqlite_schema delta: +0 objects (219 -> 219, <= MAX_SCHEMA_OBJECTS = 256).

ALTER TABLE site_general_settings
    ADD COLUMN cms_locale TEXT NOT NULL DEFAULT 'en'
    CHECK (length(cms_locale) BETWEEN 2 AND 35);

ALTER TABLE site_locales_state
    ADD COLUMN multilingual_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (multilingual_enabled IN (0, 1));

UPDATE site_locales_state
   SET multilingual_enabled = 1
 WHERE id = 1
   AND (SELECT COUNT(*) FROM site_locales) > 1;

ALTER TABLE site_locales
    ADD COLUMN deleted_at TEXT NULL;
```

**Tính nhất quán nâng cấp & bảo toàn dữ liệu:**
- Site mới (`0047` tạo sẵn đúng `1` dòng `'vi'`): `cms_locale = 'en'`, `multilingual_enabled = 0`.
- Site nâng cấp có `> 1` ngôn ngữ trong `site_locales`: `cms_locale = 'en'`, `multilingual_enabled = 1`.
- Không sửa migration `0001`–`0050` (giữ nguyên checksum và `PRAGMA foreign_key_check`).

### 2. Danh mục Ngôn ngữ CMS & Hợp đồng Backend (`opendash-core` + `opendash-server`)

#### 2.1. `opendash-core::i18n` — Single Source of Truth (`CMS_LANGUAGE_CATALOG`)
- Tạo file thuần Rust `cms/crates/opendash-core/src/i18n.rs` (0 dependency ngoài):
  ```rust
  pub const CMS_CATALOG_VERSION: &str = "v1";

  #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
  pub struct CmsLocaleOption {
      pub code: &'static str,
      pub bcp47: &'static str,
      pub native_name: &'static str,
      pub english_name: &'static str,
      pub direction: &'static str,
      pub installed: bool,
      pub translation_coverage: u8,
  }
  ```
- Danh sách cố định `pub const CMS_LANGUAGE_CATALOG: &[CmsLocaleOption]` gồm đúng **`32`** phần tử (ưu tiên `en`, `vi` ở đầu, 30 ngôn ngữ còn lại xếp theo thứ tự `code` ASCII):

| # | `code` | `bcp47` | `native_name` | `english_name` | `direction` | `installed` | `translation_coverage` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `en` | `en-US` | `English` | `English` | `ltr` | `true` | `100` |
| 2 | `vi` | `vi-VN` | `Tiếng Việt` | `Vietnamese` | `ltr` | `true` | `100` |
| 3 | `ar` | `ar` | `العربية` | `Arabic` | `rtl` | `false` | `0` |
| 4 | `bg` | `bg-BG` | `Български` | `Bulgarian` | `ltr` | `false` | `0` |
| 5 | `cs` | `cs-CZ` | `Čeština` | `Czech` | `ltr` | `false` | `0` |
| 6 | `da` | `da-DK` | `Dansk` | `Danish` | `ltr` | `false` | `0` |
| 7 | `de` | `de-DE` | `Deutsch` | `German` | `ltr` | `false` | `0` |
| 8 | `el` | `el` | `Ελληνικά` | `Greek` | `ltr` | `false` | `0` |
| 9 | `es` | `es-ES` | `Español` | `Spanish` | `ltr` | `false` | `0` |
| 10 | `fi` | `fi` | `Suomi` | `Finnish` | `ltr` | `false` | `0` |
| 11 | `fr` | `fr-FR` | `Français` | `French` | `ltr` | `false` | `0` |
| 12 | `he` | `he-IL` | `עברית` | `Hebrew` | `rtl` | `false` | `0` |
| 13 | `hi` | `hi-IN` | `हिन्दी` | `Hindi` | `ltr` | `false` | `0` |
| 14 | `hu` | `hu-HU` | `Magyar` | `Hungarian` | `ltr` | `false` | `0` |
| 15 | `id` | `id-ID` | `Bahasa Indonesia` | `Indonesian` | `ltr` | `false` | `0` |
| 16 | `it` | `it-IT` | `Italiano` | `Italian` | `ltr` | `false` | `0` |
| 17 | `ja` | `ja` | `日本語` | `Japanese` | `ltr` | `false` | `0` |
| 18 | `ko` | `ko-KR` | `한국어` | `Korean` | `ltr` | `false` | `0` |
| 19 | `ms` | `ms-MY` | `Bahasa Melayu` | `Malay` | `ltr` | `false` | `0` |
| 20 | `nb` | `nb-NO` | `Norsk bokmål` | `Norwegian Bokmål` | `ltr` | `false` | `0` |
| 21 | `nl` | `nl-NL` | `Nederlands` | `Dutch` | `ltr` | `false` | `0` |
| 22 | `pl` | `pl-PL` | `Polski` | `Polish` | `ltr` | `false` | `0` |
| 23 | `pt-br` | `pt-BR` | `Português do Brasil` | `Portuguese (Brazil)` | `ltr` | `false` | `0` |
| 24 | `pt-pt` | `pt-PT` | `Português` | `Portuguese (Portugal)` | `ltr` | `false` | `0` |
| 25 | `ro` | `ro-RO` | `Română` | `Romanian` | `ltr` | `false` | `0` |
| 26 | `ru` | `ru-RU` | `Русский` | `Russian` | `ltr` | `false` | `0` |
| 27 | `sv` | `sv-SE` | `Svenska` | `Swedish` | `ltr` | `false` | `0` |
| 28 | `th` | `th` | `ไทย` | `Thai` | `ltr` | `false` | `0` |
| 29 | `tr` | `tr-TR` | `Türkçe` | `Turkish` | `ltr` | `false` | `0` |
| 30 | `uk` | `uk` | `Українська` | `Ukrainian` | `ltr` | `false` | `0` |
| 31 | `zh-hans` | `zh-CN` | `简体中文` | `Chinese (Simplified)` | `ltr` | `false` | `0` |
| 32 | `zh-hant` | `zh-TW` | `繁體中文` | `Chinese (Traditional)` | `ltr` | `false` | `0` |

- Helpers thuần trong `opendash_core::i18n`:
  - `pub fn find_cms_locale(raw: &str) -> Option<&'static CmsLocaleOption>`: chuẩn hóa `raw.trim().to_ascii_lowercase()`, khớp theo `opt.code` (cũng chấp nhận dạng `opt.bcp47.eq_ignore_ascii_case(trimmed)` nếu có, nhưng luôn lưu giá trị chuẩn `opt.code`).
  - `pub fn resolve_effective_ui_locale(configured: &str) -> &'static CmsLocaleOption`: chuẩn hóa `configured.trim().to_ascii_lowercase()`; nếu khớp `"vi"` trả về `&CMS_LANGUAGE_CATALOG[1]` (`vi`, `vi-VN`, `ltr`), **mọi giá trị khác** (kể cả ngôn ngữ `installed == false` như `ja`/`ar`/`he` hoặc chuỗi lạ trong DB) đều fallback về `&CMS_LANGUAGE_CATALOG[0]` (`en`, `en-US`, `ltr`). Do đó `effective_bcp47` và `effective_direction` luôn bám theo bản dịch thực tế được render, không bao giờ hiển thị chữ tiếng Anh trong layout `rtl`.

#### 2.2. Bootstrap công khai `GET /api/i18n/admin` & Settings API (`settings.rs`)
- **Contract `GET /api/i18n/admin`** (không yêu cầu session, `Cache-Control: no-store`, đăng ký trong [`app.rs`](../../cms/crates/opendash-server/src/app.rs)):
  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct AdminI18nBootstrap {
      pub configured_locale: String,
      pub effective_locale: &'static str,
      pub effective_bcp47: &'static str,
      pub effective_direction: &'static str,
      pub configured_installed: bool,
      pub catalog_version: &'static str,
      pub available_locales: &'static [opendash_core::i18n::CmsLocaleOption],
  }
  ```
- **Contract `GeneralSettings` & `GeneralSettingsInput`** ([`storage/settings.rs#L781-L821`](../../cms/crates/opendash-server/src/storage/settings/mod.rs)):
  - Thêm `pub cms_locale: String` vào `GeneralSettings` (trả trong cả `get_general_in_conn_for_locale` và `SaveOutcome::Saved`).
  - Derive `Default` trên `GeneralSettingsInput` và thêm `#[serde(default)] pub cms_locale: Option<String>` (cập nhật 2 vị trí khởi tạo struct trong Rust: [`storage/overrides.rs#L3155-L3159`](../../cms/crates/opendash-server/src/storage/overrides.rs#L3155-L3159) và unit test `settings.rs#L2667-L2671`).
  - Cập nhật `GeneralSettingsDraft` trong [`cms/admin/src/api/settings.ts`](../../cms/admin/src/api/settings.ts#L14) thêm `cms_locale?: string`.
  - Quy tắc xử lý trong `save_general_for_locale` (`BEGIN IMMEDIATE`):
    1. Đọc `global_tz, global_cms_locale` từ `SELECT timezone, cms_locale FROM site_general_settings WHERE id = 1`.
    2. Nếu `input.cms_locale` là `Some(raw)`: gọi `find_cms_locale(&raw)`. Nếu `None` -> trả `SaveOutcome::InvalidCmsLocale` (`400 Bad Request`, `settings.invalid_cms_locale`). Lấy `normalized_cms = opt.code.to_owned()`.
    3. Nếu `!is_default_locale` (đang lưu tab ngôn ngữ nội dung phụ): nếu `input.cms_locale.is_some()` và `normalized_cms != global_cms_locale` -> trả `SaveOutcome::SecondaryCmsLocaleLocked` (`400 Bad Request`, `settings.secondary_cms_locale_locked`, đối xứng với `SecondaryTimezoneLocked` ở `L930`).
    4. Nếu `is_default_locale`: tính `next_cms_locale = normalized_cms_if_some.unwrap_or(global_cms_locale)`. Lưu `UPDATE site_general_settings SET site_title = ?, tagline = ?, timezone = ?, cms_locale = ?, version = ? WHERE id = 1` và cập nhật `site_general_settings_locales.version` bình thường.
    5. Nếu `SaveOutcome::Saved(ref saved)` trong `routes::settings::general_put`, sau khi DB đã commit thành công, handler gọi `state.ui_locale.store(crate::i18n::encode_ui_locale(&saved.cms_locale), Ordering::Release)` ngay lập tức!

#### 2.3. Hệ thống Dịch Lỗi Backend (`src/i18n.rs`, `AppState.ui_locale` Cache & `localize_error_response`)
Thiết kế giải quyết triệt để hiệu năng `0`-query khi lỗi, an toàn HTTP `Content-Length`/`Transfer-Encoding`, phân tách chính xác Public vs Admin/Auth/Builder/Themes, và độ phủ 100% cả thông báo tiếng Việt lẫn các lỗi tiếng Anh có sẵn / Axum rejections:

1. **Giữ nguyên signature `ApiError` & Bổ sung `AppState.ui_locale` Cache:**
   - Giữ nguyên [`pub struct ApiError(pub StatusCode, pub Cow<'static, str>);`](../../cms/crates/opendash-server/src/error.rs#L9) cho mọi call site trong codebase.
   - Bổ sung `pub ui_locale: std::sync::Arc<std::sync::atomic::AtomicU8>` vào [`AppState`](../../cms/crates/opendash-server/src/app.rs#L78-L93) (`0 = UNSET`, `1 = EN`, `2 = VI`; cập nhật cả 3 chỗ khởi tạo `AppState`: `main.rs#L195`, `routes/plugin.rs#L766`, `routes/operations.rs#L1169`). Khi server boot (`main.rs`), nạp `site_general_settings.cms_locale` qua [`storage/settings.rs`](../../cms/crates/opendash-server/src/storage/settings/mod.rs) helper; mỗi lần `general_put` lưu thành công sẽ cập nhật nguyên tử vào `state.ui_locale`.
   - Khắc phục trước toàn bộ **`18` chuỗi mojibake `?`** trong [`cms/crates/opendash-server/src/storage/entries.rs`](../../cms/crates/opendash-server/src/storage/entries.rs) (`L1405, L1410, L1459, L1528, L1533, L1806, L1813, L1818, L1825, L1831, L1838, L1856, L1861, L1953, L1956, L2513, L2579, L2584`) về UTF-8 tiếng Việt sạch để bảng khớp chuẩn không bị rớt.

2. **Động cơ Ánh xạ & Dịch Hai Chiều (`cms/crates/opendash-server/src/i18n.rs`):**
   - `pub struct LocalizedError { pub code: &'static str, pub message: String }`
   - `pub fn translate_api_message(raw_msg: &str, effective_ui_locale: &str) -> LocalizedError` hỗ trợ **dịch hai chiều (`vi <-> en`)** từ cùng 1 bảng sự thật `EXACT_ERROR_ENTRIES` + `TEMPLATE_MATCHERS`:
     - **Lớp 1 — Khớp chính xác (`EXACT_ERROR_ENTRIES`):** Bộ ba `(code, vi_text, en_text)` bao phủ toàn bộ thông báo tĩnh từ `auth.rs`, `error.rs`, `operations.rs`, `seo.rs`, `contact.rs`, `document.rs`, `page.rs`, `public_context.rs`, `main.rs`, `hooks.rs`, `opendash-core/src/id.rs`, tất cả `routes/*` và `storage/*`, cộng thêm các lỗi tiếng Anh có sẵn (`routes/themes.rs#L199-207`, `routes/builder.rs#L432-434`, `routes/operations.rs#L516-522`, `storage/entries.rs#L156`), `From<sqlx::Error>`, `From<serde_json::Error>`, và `app.rs` (`Origin không hợp lệ.`, empty Host `400`).
     - **Lớp 2 — Khớp mẫu tham số (`TEMPLATE_MATCHERS`):** Dịch chính xác hai chiều mọi thông báo nội suy động trên toàn server (ví dụ `"Không tìm thấy loại nội dung '<k>'."`, `"schema_version_conflict: ..."`, `"Dữ liệu JSON liên quan (<ctx>) ..."`, `"Plugin kết thúc với mã lỗi <n>."`, `"unknown_field_key: Trường '<k>' không thuộc loại nội dung '<t>'."`, `"field_edit_forbidden: ..."`, `"Dữ liệu đầu vào không phải JSON hợp lệ: <err>"`, `"permalink_collision:path=<p> ..."`, plus các lỗi Axum built-in như `"length limit exceeded"` (`413`), `"Failed to deserialize ..."` (`400`/`422`), `405 Method Not Allowed`, `415 Unsupported Media Type`).
   - **Quy ước `X-OpenDash-Error-Code`:** Luôn dùng ASCII lowercase dạng `<domain>.<slug>` (`[a-z0-9._]`). Các mã chuẩn quan trọng gồm: `auth.unauthorized`, `auth.forbidden`, `auth.invalid_credentials`, `request.bad_host`, `request.bad_origin`, `request.if_match_required`, `request.invalid_if_match`, `request.precondition_failed`, `request.invalid_json`, `request.body_too_large`, `request.unsupported_media_type`, `request.method_not_allowed`, `settings.invalid_cms_locale`, `settings.secondary_cms_locale_locked`, `settings.secondary_timezone_locked`, `settings.invalid_timezone`, `multilingual.disabled`, `locale.not_found`, `locale.not_deleted`, `locale.cannot_delete_default`, `locale.orphaned_conflict`, `locale.purge_confirm_mismatch`, `locale.conflict`, `permalink.conflict`, `hook.before_save_veto`, `plugin.error`, `storage.busy`, `server.error`.
   - **Unit Test Độ Phủ Tĩnh trong `src/i18n.rs`:** Quét qua mọi chuỗi lỗi liệt kê từ toàn bộ server modules (cộng test các mẫu động) để bảo đảm `translate_api_message(msg, "en")` trả `en` sạch không còn ký tự tiếng Việt và `translate_api_message(msg, "vi")` trả `vi` sạch không bị rớt câu tiếng Anh hệ thống.

3. **Vị trí, Marker Host/Origin & Quy tắc Thực thi Middleware `localize_error_response` (`app.rs`):**
   - Trong [`app.rs#L743-L750`](../../cms/crates/opendash-server/src/app.rs#L743-L750), lớp kiểm tra Host/Origin gắn extension marker nội bộ `#[derive(Clone, Copy)] pub(crate) struct HostOriginRejected;` vào 2 response `!valid_host` (`400`) và `!origin_valid` (`403`) trước khi trả về.
   - Đặt `middleware::from_fn_with_state(state.clone(), crate::i18n::localize_error_response)` ngay **bên ngoài** lớp Host/Origin (`app.rs#L751`) và **trước** `.fallback(crate::routes::seo::public_fallback)` (`app.rs#L752`), trước `.with_state(state)` (`L768`). Nhờ vậy mọi phản hồi từ router chính lẫn Host/Origin đi qua `localize_error_response`, còn `public_fallback` (trang 404/lỗi công khai) hoàn toàn độc lập bên dưới!
   - **Luồng hoạt động từng bước trong `localize_error_response`:**
     1. *Fast-path cho luồng bình thường:* Lưu `method` & `path` -> `let mut resp = next.run(request).await;` -> nếu `!resp.status().is_client_error() && !resp.status().is_server_error()`, trả `resp` ngay (**0 query, 0 đọc body**).
     2. *Kiểm tra Phạm vi Nghiêm ngặt (Admin/Auth/Themes/Builder + HostOriginRejected):*
        - `let is_host_origin_rejected = resp.extensions().get::<HostOriginRejected>().is_some();`
        - `let is_admin_scope = path == "/admin" || path.starts_with("/admin/") || path == "/builder" || path.starts_with("/builder/") || path.starts_with("/auth/") || path.starts_with("/api/admin/") || path.starts_with("/api/themes/") || matches!(path.as_str(), "/registry" | "/page" | "/preview" | "/save");`
        - Nếu **`!is_host_origin_rejected && !is_admin_scope`**, HOẶC response có `Content-Type: text/html` -> bỏ qua, trả `resp` nguyên trạng! Nhờ ranh giới biên theo segment (`== "/admin" || starts_with("/admin/")`, chính xác như `is_reserved_system_root` ở [`public_context.rs#L14-45`](../../cms/crates/opendash-server/src/public_context.rs#L14-L45)):
          - Các trang public `/admin-guide`, `/builder-info`, `/theme/*`, `/api/public/contact/*` (gồm cả `routes/contact.rs#L254-259` `403 Origin không hợp lệ.` do chính handler contact sinh ra **không** mang `HostOriginRejected`), `/robots.txt`, `/sitemap*` **không bao giờ bị ảnh hưởng bởi `cms_locale`**.
          - Toàn bộ `7` route quản trị ngoài `/api/admin/` gồm `/api/themes/{theme_id}/options`, `/api/themes/{theme_id}/import/{dry-run,execute}` ([`app.rs#L473-486`](../../cms/crates/opendash-server/src/app.rs#L473-L486)) và `/registry`, `/page`, `/preview`, `/save` ([`app.rs#L661-664`](../../cms/crates/opendash-server/src/app.rs#L661-L664)) được dịch đầy đủ 100%.
     3. *Lấy `effective_ui_locale` không block DB:* Đọc `state.ui_locale.load(Ordering::Acquire)`. Chỉ khi `0` (`UNSET` — unit test không qua `main.rs`), gọi `state.db.try_acquire()`: nếu có connection rảnh, gọi helper trong storage **`crate::storage::settings::get_cms_locale_in_conn(&mut conn).await`** (tuyệt đối **không** đặt `sqlx::query*` inline trong `src/i18n.rs`, bảo đảm vượt qua [`check-standards.py#L466`](../../cms/scripts/check-standards.py#L466)) và cập nhật `state.ui_locale`; nếu DB bận hoặc lỗi -> lấy ngay `"en"` (**0 deadlock, 0 delay `503`**).
     4. *Bảo toàn `Method::HEAD` & Kiểm tra Bound Trước Khi Buffering:*
        - Nếu `method == Method::HEAD`: không đọc/ghi body (chỉ gắn `X-OpenDash-Error-Code`).
        - Kiểm tra `resp.body().size_hint().upper().is_some_and(|n| n > MAX_LOCALIZED_ERROR_BYTES as u64)` **trước khi** gọi `to_bytes(..., MAX_LOCALIZED_ERROR_BYTES = 1_048_576)`: nếu vượt `1 MiB` (hoặc `to_bytes` thất bại), giữ nguyên/trả phản hồi không nuốt dữ liệu một cách an toàn; không chặn hoặc treo stream.
     5. *Dịch Body & Chuẩn hóa Headers HTTP:*
        - **Body rỗng** (ví dụ `400` bad Host `app.rs#L744` hoặc `405` rỗng): sinh body `text/plain; charset=utf-8` tương ứng (`Invalid Host header.` / `Header Host không hợp lệ.`), gắn `X-OpenDash-Error-Code`.
        - **Content-Type `text/plain` (hoặc không rõ nhưng không HTML/JSON):** `translate_api_message` -> gắn `X-OpenDash-Error-Code`.
        - **Content-Type `application/json`:** parse `serde_json::Value`:
          - Nếu có `"error": String` (`PermalinkConflictError`, `{"error": "..."}`, [`PluginRunResponse`](../../cms/crates/opendash-server/src/plugin/manager/mod.rs)): dịch `"error"` khi khớp bảng hệ thống/runtime, giữ nguyên `"conflicts"`, `"code"`, `"correlation_id"`.
          - Nếu có `"message": String` ([`ContentBeforeSaveVeto`](../../cms/crates/opendash-server/src/hooks/mod.rs) `422` tại tất cả `5` điểm trả ở `routes/entries.rs#L547, L683, L1020, L1141, L1231`, và `PluginRunResponse.data` chứa veto lồng nhau): dịch `"message"` + chuỗi giá trị trong `"field_errors"` nếu khớp bảng hệ thống, giữ nguyên `"code"`.
          - Gắn `X-OpenDash-Error-Code`.
        - **Bắt buộc khi thay đổi bytes body:** gọi **`resp.headers_mut().remove(header::CONTENT_LENGTH)`**, **`resp.headers_mut().remove(header::TRANSFER_ENCODING)`**, rồi ghi lại exact `HeaderValue::from(new_bytes.len())` vào `header::CONTENT_LENGTH`!

4. **Frontend [`cms/admin/src/api/client.ts`](../../cms/admin/src/api/client.ts):**
   - Thêm trường `readonly errorCode?: string` vào `ApiClientError`.
   - Cả **`handleResponseText`** (`L60-66`) lẫn **`requestJson`** (`L110-152`) đọc `res.headers.get('X-OpenDash-Error-Code')`, đọc cả `errObj?.error` **và** `errObj?.message`, và dịch fallback `L150`/`L163` qua `tStatic(...)`.

### 3. Kiến trúc Quản lý Đa ngôn ngữ (`#/settings/multilingual`, Soft-Delete & Orphan Purge)

#### 3.1. Ma trận Hợp đồng REST API & Thứ tự Kiểm tra

Tất cả endpoint ghi dưới đây dùng `AdmittedMutation`, `require_admin_with_csrf`, `DefaultBodyLimit::max(4096)`, `parse_if_match(&headers)` đối chiếu `site_locales_state.version`, và trả `Cache-Control: no-store` + `ETag: "v<version>"`. Chuẩn hóa tham số đường dẫn `{code}` luôn chạy `raw_code.trim().to_ascii_lowercase()`. Trong `settings::normalize_locale_code` (`settings.rs#L97-108`), chặn thêm `"multilingual"` và `"orphans"` (trả `422`) để tránh xung đột với đường dẫn tĩnh trên `app.rs`.

| Endpoint | Quyền | Request Body / Guard | HTTP Status & Headers | Idempotency & Thứ tự Precedence |
| --- | --- | --- | --- | --- |
| `GET /api/i18n/admin` | Public | — | `200 OK`, `Cache-Control: no-store` (`AdminI18nBootstrap`) | Read-only. |
| `GET /api/admin/locales` | Session | `require_session` | `200 OK`, `ETag: "vN"`, `no-store` (`SiteLocalesSnapshot` gồm `multilingual_enabled: bool`, lọc `deleted_at IS NULL`) | Read-only. |
| `PUT /api/admin/locales/multilingual` | Admin | `SetMultilingualEnabledInput { enabled: bool }` (`deny_unknown_fields`) + CSRF + `If-Match` | `200 OK`, `ETag: "vN"`, `no-store` (`SiteLocalesSnapshot`); conflict -> `409 PermalinkConflictError` | **Precedence:** Auth/CSRF -> `If-Match` header -> JSON parse -> CAS `412`. Nếu `enabled == current_enabled`: trả `200 OK` snapshot hiện tại không bump `version`, không chạy `sync_claims`. Nếu đổi: bump `version + 1` và chạy `sync_claims_with_options(&mut conn, true)`. |
| `POST /api/admin/locales` | Admin | `CreateLocaleInput` + CSRF + `If-Match` | `201 Created`, `ETag: "vN"`, `no-store` (`SiteLocalesSnapshot`) | **Precedence:** Auth/CSRF -> `If-Match` header -> JSON & `normalize_locale_code` (`422`) + `validate_locale_*` -> CAS `412` -> **`multilingual_enabled == 0` -> `409 multilingual.disabled`** -> active duplicate (`deleted_at IS NULL`) -> `409 Conflict` -> **`active_count >= MAX_SITE_LOCALES (20)` (`WHERE deleted_at IS NULL`) -> `422`** (chặn cả Revive lẫn Insert vượt 20!) -> nếu tồn tại `deleted_at IS NOT NULL`: **Revive** (`is_default = 0, enabled = 1, deleted_at = NULL`, `position = COALESCE(MAX(position WHERE deleted_at IS NULL), -1) + 1`, bump `version + 1`, `sync_claims`), ngược lại **Insert** mới. |
| `PUT /api/admin/locales/{code}` | Admin | `UpdateLocaleInput` + CSRF + `If-Match` | `200 OK`, `ETag: "vN"`, `no-store` (`SiteLocalesSnapshot`) | **Precedence:** Auth/CSRF -> `If-Match` header -> JSON & `validate_locale_*` -> CAS `412` -> tồn tại `WHERE code = ? AND deleted_at IS NULL` (`404 locale.not_found`) -> nếu `is_default == 0 && multilingual_enabled == 0` -> **`409 multilingual.disabled`** -> nếu `is_default != 0 && !input.enabled` -> `422`. |
| `POST /api/admin/locales/{code}/set-default` | Admin | Body rỗng / `{}` + CSRF + `If-Match` | `200 OK`, `ETag: "vN"`, `no-store` (`SiteLocalesSnapshot`) | **Precedence:** Auth/CSRF -> `If-Match` header -> body validate -> CAS `412` -> `multilingual_enabled == 0` -> **`409 multilingual.disabled`** -> tồn tại `WHERE code = ? AND deleted_at IS NULL` (`404`) -> `enabled == 0` (`409`). |
| `DELETE /api/admin/locales/{code}` | Admin | Body rỗng + CSRF + `If-Match` | `200 OK`, `ETag: "vN"`, `no-store` (`DeleteLocaleResponse`) | Cho phép chạy cả khi `multilingual_enabled == 0`. **Precedence:** Auth/CSRF -> `If-Match` -> CAS `412` -> tồn tại `WHERE code = ? AND deleted_at IS NULL` (`404 locale.not_found`) -> `is_default != 0` (`422 locale.cannot_delete_default`). Xem luồng §3.2. |
| `GET /api/admin/locales/orphans` | Admin | `require_admin` | `200 OK`, `ETag: "vN"`, `no-store` (`OrphanedLocalesSnapshot`) | Read-only. Chỉ liệt kê các hàng `site_locales WHERE deleted_at IS NOT NULL AND is_default = 0` (được bảo đảm đầy đủ nhờ FK `ON DELETE RESTRICT` trên 13 bảng `0047`). |
| `POST /api/admin/locales/orphans/{code}/purge` | Admin | `PurgeOrphanLocaleInput { confirm_code }` + CSRF + `If-Match` | `200 OK`, `ETag: "vN"`, `no-store` (`PurgeOrphanLocaleResult`) | Cho phép chạy cả khi `multilingual_enabled == 0`. **Precedence:** Auth/CSRF -> `If-Match` -> JSON parse -> CAS `412` -> tra `site_locales WHERE code = ?`: không tồn tại (`404 locale.not_found`), `is_default != 0` (`422 locale.cannot_delete_default`), `deleted_at IS NULL` (`422 locale.not_deleted`) -> `confirm_code.trim().to_ascii_lowercase() != code` (`422 locale.purge_confirm_mismatch`). Xem luồng §3.3. |

#### 3.2. Helper Bảo toàn Pattern Dùng Chung & Xóa Ngôn ngữ Phụ (`DELETE /api/admin/locales/{code}`)

- **Helper dùng chung `reassign_locale_patterns_to_default_in_tx(conn, from_locale, default_locale)`:**
  Vì [`patterns::create`](../../cms/crates/opendash-server/src/storage/patterns.rs#L115-L120) ghi `locale = default_locale` nhưng `patterns::list`/`get`/`update` coi Pattern là tài nguyên toàn cục và [`0047#L111`](../../cms/migrations/0047_multilingual_content_and_theme_l10n.sql#L111) đặt `UNIQUE (content_type_key, locale, slug)`, nếu một `pattern` mang `locale = from_locale` khi xóa/purge thì bắt buộc chuyển sang `default_locale` an toàn trước khi xóa dòng `site_locales`:
  1. Nếu không có row `WHERE content_type_key = 'pattern' AND locale = ?` -> return `Ok(())` ngay.
  2. Bật `UPDATE site_locales_state SET rename_in_progress = 1 WHERE id = 1;` (vượt `content_entries_locale_guard` `0047#L472`; `content_entries_pattern_private_update` `0047#L425` không cản đổi `locale`/`slug`).
  3. Duyệt từng `(id, slug)` theo `ORDER BY id ASC`: nếu `( 'pattern', default_locale, slug )` chưa tồn tại thì giữ nguyên `slug`; nếu bị trùng, sinh slug khử trùng dài `<= 64` bytes theo công thức deterministic `<prefix>-<id>` (hoặc `<prefix>-<id>-<n>` nếu vẫn trùng), rồi `UPDATE content_entries SET locale = ?, slug = ? WHERE id = ?`. Bảo toàn nguyên vẹn `id` nên mọi `entry_pattern_refs` giữ nguyên 100%.
  4. Tắt `UPDATE site_locales_state SET rename_in_progress = 0 WHERE id = 1;`.

- **`DeleteLocaleResponse` & Thuật toán `DELETE /api/admin/locales/{code}` (`BEGIN IMMEDIATE`):**
  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct DeleteLocaleResponse {
      pub snapshot: SiteLocalesSnapshot,
      pub deleted_code: String,
      pub soft_deleted: bool,
      pub orphan_total: i64,
  }
  ```
  1. Đếm `counts: OrphanedLocaleCounts = count_locale_data_in_conn(&mut conn, &code).await?` (bucket `routes` chỉ đếm `entry_public_paths + entry_redirects + permalink_route_redirects`, loại trừ `permalink_route_claims` tự sinh và `content_type_key = 'pattern'`).
  2. Đánh dấu `UPDATE site_locales SET enabled = 0, deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE code = ?`.
  3. Gọi `permalinks::sync_claims_with_options(&mut conn, true).await?` (tự động rút toàn bộ `permalink_route_claims` cho `code`).
  4. Nếu `counts.total == 0` (**0 dữ liệu người dùng/redirect**):
     - Gọi **`reassign_locale_patterns_to_default_in_tx(&mut conn, &code, &default_code).await?`** (tránh `500 FK failed` nếu từng tạo `pattern` lúc `code` làm mặc định!).
     - Dọn phòng vệ mọi `permalink_route_claims WHERE locale = ?` và thực thi `DELETE FROM site_locales WHERE code = ?` -> **`soft_deleted = false`**.
  5. Ngược lại (`counts.total > 0`): giữ nguyên `deleted_at IS NOT NULL` (các pattern vẫn sống bình thường qua `patterns::list` cho đến khi Purge ở §3.3 bước 0 hoặc chuyển ngay ở Purge) -> **`soft_deleted = true`**.
  6. `UPDATE site_locales_state SET version = version + 1 WHERE id = 1` -> commit -> trả `200 OK` `DeleteLocaleResponse`.

#### 3.3. Thống kê (`OrphanedLocaleCounts`) & Purge Tô-pô (`PurgeOrphanLocaleResult`)
- **Structs đầy đủ:**
  ```rust
  #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
  pub struct OrphanedLocaleCounts {
      pub entries: i64,          // content_entries WHERE locale = ? AND content_type_key != 'pattern'
      pub terms: i64,            // taxonomy_terms WHERE locale = ?
      pub settings: i64,         // site_general_settings_locales + site_reading_settings + site_home_seo + theme_option_locale_{values,versions}
      pub menu_slots: i64,       // site_menu_slot_assignments + site_menu_slot_versions
      pub routes: i64,           // entry_public_paths + entry_redirects + permalink_route_redirects
      pub projections: i64,      // plugin_entry_projections WHERE locale = ?
      pub media_seo_refs: i64,   // seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id IN (non-pattern entries)
      pub total: i64,            // tổng 7 trường trên (0 trùng lặp cascade)
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct OrphanedLocaleSummary {
      pub code: String,
      pub bcp47_tag: String,
      pub label: String,
      pub deleted_at: String,
      pub counts: OrphanedLocaleCounts,
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct OrphanedLocalesSnapshot {
      pub version: i64,
      pub items: Vec<OrphanedLocaleSummary>,
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct PurgeOrphanLocaleResult {
      pub snapshot: OrphanedLocalesSnapshot,
      pub purged_code: String,
      pub deleted: OrphanedLocaleCounts,
  }
  ```
- **Ghi chú chính xác cho `media_seo_refs`:** [`0047#L303`](../../cms/migrations/0047_multilingual_content_and_theme_l10n.sql#L303) đã xóa `'home_seo'` khỏi `seo_public_media_refs` và lưu `og_image_id` trực tiếp trên `site_home_seo` ([`storage/media_refs.rs#L313-L332`](../../cms/crates/opendash-server/src/storage/media_refs.rs#L313-L332)), nên `media_seo_refs` chỉ đếm `source_kind = 'entry_seo'` gắn với non-pattern entries của `code`.
- **Thứ tự Purge nguyên tử trong `BEGIN IMMEDIATE` (tuân thủ 100% `0047` + `0050` + bảo toàn Patterns):**
  0. **Bảo toàn Reusable Patterns:** Gọi `reassign_locale_patterns_to_default_in_tx(&mut conn, &code, &default_code).await?` (khử trùng slug trước khi chuyển về `default_code`).
  1. **Gỡ liên kết Reading chéo ngôn ngữ & tăng CAS version:**
     - `UPDATE site_reading_settings SET homepage_page_id = CASE WHEN homepage_page_id IN (SELECT id FROM content_entries WHERE locale = ?) THEN NULL ELSE homepage_page_id END, posts_page_id = CASE WHEN posts_page_id IN (SELECT id FROM content_entries WHERE locale = ?) THEN NULL ELSE posts_page_id END, version = version + 1, updated_at = datetime('now') WHERE locale != ? AND (homepage_page_id IN (SELECT id FROM content_entries WHERE locale = ?) OR posts_page_id IN (SELECT id FROM content_entries WHERE locale = ?));`
  2. **Xóa `seo_public_media_refs`:**
     - `DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id IN (SELECT id FROM content_entries WHERE locale = ?);` -> cộng `rows_affected()` vào `deleted.media_seo_refs`.
  3. **Xóa `entry_terms` TRƯỚC `taxonomy_terms` & `content_entries`** (`0047#L140` `ON DELETE RESTRICT`):
     - `DELETE FROM entry_terms WHERE entry_id IN (SELECT id FROM content_entries WHERE locale = ?) OR term_id IN (SELECT id FROM taxonomy_terms WHERE locale = ?);`
  4. **Xóa routes & redirects** -> cộng `rows_affected()` từ `entry_public_paths`, `entry_redirects`, `permalink_route_redirects` vào `deleted.routes` (và xóa `permalink_route_claims WHERE locale = ?`):
     - `DELETE FROM entry_public_paths WHERE locale = ?;`
     - `DELETE FROM entry_redirects WHERE locale = ?;`
     - `DELETE FROM permalink_route_claims WHERE locale = ?;`
     - `DELETE FROM permalink_route_redirects WHERE locale = ?;`
  5. **Xóa settings, menu slots & `plugin_entry_projections`** (xóa `plugin_entry_projections` trước `content_entries` để cộng riêng `deleted.projections`, không đếm trùng qua cascade):
     - `DELETE FROM site_general_settings_locales WHERE locale = ?;`
     - `DELETE FROM site_reading_settings WHERE locale = ?;`
     - `DELETE FROM site_home_seo WHERE locale = ?;`
     - `DELETE FROM theme_option_locale_values WHERE locale = ?;`
     - `DELETE FROM theme_option_locale_versions WHERE locale = ?;`
     - `DELETE FROM site_menu_slot_assignments WHERE locale = ?;`
     - `DELETE FROM site_menu_slot_versions WHERE locale = ?;`
     - `DELETE FROM plugin_entry_projections WHERE locale = ?;`
  6. **Xóa `content_entries WHERE locale = ?`** (lúc này chỉ còn non-pattern entries; `ON DELETE CASCADE` dọn `entry_seo`, `entry_builder_documents`, `entry_builder_media_refs` gồm cả refs video/poster, và `entry_pattern_refs`) -> cộng `rows_affected()` vào `deleted.entries`.
  7. **Phá cây `taxonomy_terms.parent_id` (`0047#L70` `ON DELETE RESTRICT`, được phép bởi `0047#L446`) rồi xóa `taxonomy_terms`:**
     - `UPDATE taxonomy_terms SET parent_id = NULL WHERE locale = ? AND parent_id IS NOT NULL;`
     - `DELETE FROM taxonomy_terms WHERE locale = ?;` -> cộng `rows_affected()` vào `deleted.terms`.
  8. **Xóa dòng `site_locales WHERE code = ? AND deleted_at IS NOT NULL AND is_default = 0;`**
  9. **Tăng `site_locales_state.version = version + 1`, gọi `permalinks::sync_claims_with_options(&mut conn, true).await?`, tính `deleted.total`, trả `200 OK` + `no_store_headers(Some(snapshot.version))` + `PurgeOrphanLocaleResult`.**

#### 3.4. Danh mục Kiểm soát `site_locales` Đầy đủ (`12` Storage Modules + `plugin/runtime.rs`)

1. **[`storage/settings.rs`](../../cms/crates/opendash-server/src/storage/settings/mod.rs):**
   - `SiteLocalesSnapshot` (`L46-51`): thêm `pub multilingual_enabled: bool`.
   - `EnabledLocaleInfo` (`L200-205`) & `list_enabled_locales_in_conn` (`L207-227`): thêm **`pub label: String`** vào struct và select `label` cùng `code, bcp47_tag, direction, is_default`, lọc `enabled = 1 AND deleted_at IS NULL` và khi `multilingual_enabled == 0` chỉ trả `is_default = 1`.
   - `get_default_locale_code_in_conn` (`L135`), `get_locale_bcp47_and_dir_in_conn` (`L168`), `resolve_requested_locale_in_conn` (`L229`), `list_locales_in_conn` (`L255`), `update_locale` (`L408`), `set_default_locale` (`L471`): thêm `deleted_at IS NULL`.
   - `enabled_secondary_locale_for_segment` (`L150`): thêm `AND deleted_at IS NULL AND EXISTS (SELECT 1 FROM site_locales_state WHERE id = 1 AND multilingual_enabled = 1)`.
   - `save_public` (`L3815`): đếm `COUNT(*) FROM site_locales WHERE deleted_at IS NULL`. Nếu `multilingual_enabled == 1` hoặc count `> 1` -> `MultiLocaleConflict`. Nếu đơn ngữ (`<= 1`) đổi `content_language`, kiểm tra `SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ? AND deleted_at IS NOT NULL)` -> `409 Conflict` (`locale.orphaned_conflict`); và cập nhật thêm `UPDATE plugin_entry_projections SET locale = ? WHERE locale = ?`.
2. **[`storage/permalinks.rs`](../../cms/crates/opendash-server/src/storage/permalinks.rs):**
   - `load_secondary_locale_roots_in_conn` (`L145`), `sync_claims_with_options_and_swap` (`L1002-1017`), và toàn bộ `JOIN site_locales sl` (`L587, L607, L635, L660, L1198, L1316, L1354, L2013, L2025, L2053, L2112`): áp dụng `sl.deleted_at IS NULL AND sl.enabled = 1 AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)`.
   - **Tách `parked_redirects` trong `old_redirects` (`L1526-1562`) & write-back (`L1973-1996`) — Bảo toàn lossless + Không vỡ PK:**
     - Đọc tập `soft_deleted_codes` (`SELECT code FROM site_locales WHERE deleted_at IS NOT NULL`) trong transaction.
     - Mọi dòng trong `old_redirects` có `locale` thuộc `soft_deleted_codes` được xem là **`parked`**: **KHÔNG** đưa vào `previous_history` và **KHÔNG** đưa vào `historical`. Nhờ đó `L1973-1980` giữ nguyên dữ liệu redirect của ngôn ngữ đang chờ Purge (bảo toàn 100% nếu Admin `revive`), không đếm vào `MAX_REDIRECTS`, và không tạo lỗi collision cho site đang chạy.
     - Các ngôn ngữ còn tồn tại (`deleted_at IS NULL`, kể cả đang tắt `enabled = 0` hay tạm ngưng `multilingual_enabled = 0`) giữ nguyên logic `old_redirects` hiện hành để bảo toàn lịch sử redirect canonical.
     - Tại vòng lặp ghi (`L1982-1996`), dùng `INSERT INTO permalink_route_redirects(source_path,target_kind,target_id,locale,origin) VALUES (?,?,?,?,?) ON CONFLICT(source_path) DO UPDATE SET target_kind=excluded.target_kind, target_id=excluded.target_id, locale=excluded.locale, origin=excluded.origin, created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE permalink_route_redirects.locale IN (SELECT code FROM site_locales WHERE deleted_at IS NOT NULL)` — ghi đè an toàn nếu một tuyến active hợp lệ chiếm lại `source_path` đang parked mà không bao giờ dính `SQLITE_CONSTRAINT_PRIMARYKEY` (`500`).
3. **[`storage/seo.rs`](../../cms/crates/opendash-server/src/storage/seo.rs):**
   - `L74, L91, L153, L608, L656, L692, L1228, L1232, L1387, L1456, L1535`: áp dụng `sl.deleted_at IS NULL` (và gating `sl.enabled = 1` + `multilingual_enabled = 1` cho truy vấn public).
   - **`validate_route_claim_in_tx` (`L320-326`)** & `check_cpt_root_claim_in_tx` (`L345..L416`): lọc `JOIN site_locales sl ON sl.code = r.locale AND sl.deleted_at IS NULL` ở `L320`, `L383` (`entry_redirects r`), `L398` (`permalink_route_redirects r`), đặt alias `entry_public_paths p` tại `L357` với `JOIN site_locales sl ON sl.code = p.locale AND sl.deleted_at IS NULL`, và bổ sung `AND deleted_at IS NULL` tại `L345` để các đường dẫn `parked` của ngôn ngữ đã soft-delete không chặn author đặt custom URL hay tạo CPT mới.
   - `has_multilingual_alternates_prereq` (`L1106`, đổi `L1117` sang **`settings::list_enabled_locales_in_conn(conn)`** để short-circuit ngay khi `multilingual_enabled == 0`) & **`build_theme_locale_context_in_conn` (`L1655-L1770`)**: đổi `L1660` sang **`settings::list_enabled_locales_in_conn(conn)`** (nay đã có trường `label`), và trong `ThemeLocaleTarget::{Home, Entry, Archive, Term}` (`L1691-1767`) lọc chỉ các locale thuộc tập `enabled`.
4. **[`storage/sitemap.rs`](../../cms/crates/opendash-server/src/storage/sitemap.rs):**
   - `L40, L75, L106, L145, L223, L247, L288, L333, L366`: dùng chung điều kiện `sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)`.
5. **[`storage/public_content.rs`](../../cms/crates/opendash-server/src/storage/public_content.rs):**
   - Cập nhật toàn bộ **`6` hàm đọc public** (`get_by_slug_for_locale` `L93`, `get_published_by_id_in_tx` `L131`, `taxonomy_public_page_for_locale` `L556`, **`taxonomy_public_page_by_id` `L597`**, `query_related_entries` `L1091`, `query_recent_posts` `L1168`) và toàn bộ **`11` truy vấn trong `media_is_referenced`** (`L1200, L1225, L1280, L1290, L1305, L1313, L1318, L1324, L1333, L1356`, và **truy vấn thứ 2 Builder + Pattern ở `L1393`**): áp dụng `sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)`. Khi truyền trực tiếp một `Some(raw)` locale đã bị soft-delete, tắt (`enabled = 0`) hoặc là ngôn ngữ phụ trong lúc `multilingual_enabled == 0`, `get_by_slug_for_locale` (`L85`) và `taxonomy_public_page_for_locale` (`L549`) trả `Ok(None)` (nhất quán `404` công khai thay vì `400`).
6. **[`storage/entries.rs`](../../cms/crates/opendash-server/src/storage/entries.rs):**
   - `decoded_with_terms` (`L403`) & `batch_translations_by_group_in_tx` (`L477`): thêm `AND sl.deleted_at IS NULL`.
   - `FILTERED_ENTRIES_WHERE` (`L1015-1033`): khi `locale IS NULL`, lọc `EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = e.locale AND sl.deleted_at IS NULL)`.
   - `resolve_entry_write_locale_in_tx` (`L628`): lọc `deleted_at IS NULL`; nếu ghi ngôn ngữ không mặc định khi `multilingual_enabled == 0` -> trả `409 multilingual.disabled`.
   - `prepare_entry_translation` (`L2823`–`L2863`) & `commit_entry_translation` (`L3006`–`L3035`): chặn hoàn toàn mọi tạo bản dịch mới (`409 multilingual.disabled`) nếu `multilingual_enabled == 0` (kể cả dịch từ bài phụ còn lại sang ngôn ngữ mặc định!), đồng thời kiểm tra cả source lẫn target đều `deleted_at IS NULL`.
7. **[`storage/entry_table.rs`](../../cms/crates/opendash-server/src/storage/entry_table.rs):**
   - `SCOPE` (`L108-126`): lọc `EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = e.locale AND sl.deleted_at IS NULL)` khi `filter.locale` là `None`.
8. **[`storage/taxonomy.rs`](../../cms/crates/opendash-server/src/storage/taxonomy.rs):**
   - `batch_term_translations_in_conn` (`L101`): thêm `AND sl.deleted_at IS NULL`.
   - `list_terms_for_locale` (`L780-806`): thêm `EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = locale AND sl.deleted_at IS NULL)` vào **cả `SELECT COUNT(*)` (`L780-789`) lẫn `SELECT ... LIMIT ? OFFSET ?` (`L791-806`)** khi `resolved_locale` là `None`.
   - `create_term_with_locale` (`L948-949`): nếu `multilingual_enabled == 0` và (`target_locale != default_code` hoặc `source_term_id.is_some()`), trả `409 multilingual.disabled`.
9. **[`storage/dashboard.rs`](../../cms/crates/opendash-server/src/storage/dashboard.rs#L19-L35):**
   - `get_counts`: lọc `EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = content_entries.locale AND sl.deleted_at IS NULL)`.
10. **[`storage/builder_binding.rs`](../../cms/crates/opendash-server/src/storage/builder_binding.rs#L113) & [`storage/media_refs.rs`](../../cms/crates/opendash-server/src/storage/media_refs.rs#L319):**
    - `builder_binding.rs#L113`: lọc `sl.deleted_at IS NULL` và trên public (`authenticated_preview == 0`) lọc thêm `(sl.is_default = 1 OR multilingual_enabled = 1)`. `media_refs.rs#L319`: `AND deleted_at IS NULL`.
11. **[`storage/plugin.rs`](../../cms/crates/opendash-server/src/storage/plugin.rs#L1040-L1058) & [`plugin/runtime.rs`](../../cms/crates/opendash-server/src/plugin/runtime/mod.rs):**
    - `plugin::count_entries_for_host_list` (`storage/plugin.rs#L1040-L1058`) bổ sung tham số `enforce_public_locale: bool` (hoặc join `site_locales sl ON sl.code = content_entries.locale AND sl.deleted_at IS NULL` và khi public: `sl.enabled = 1 AND (sl.is_default = 1 OR multilingual_enabled = 1)`), đồng bộ hoàn toàn giữa `items` và `total`.
    - `entries::list_published_filtered` và `HostReadRequest::{ContentGet, ContentList, TaxonomyList}` (`plugin/runtime.rs#L965-L1123`) thực thi gating `sl.deleted_at IS NULL`, và khi `is_public_scope == true` chỉ cho phép các ngôn ngữ thực sự public (`sl.enabled = 1` + `multilingual_enabled = 1` cho phụ), ngăn plugin rò rỉ dữ liệu ngôn ngữ đã xóa/tạm ngưng qua public hook/filter.

### 4. Kiến trúc Frontend React Admin i18n (`cms/admin/src/i18n/`)

#### 4.1. Thư viện Thuần TypeScript & Cầu nối `tStatic()` (Zero `npm` Deps)
- Thư mục mới `cms/admin/src/i18n/`:
  - `locales/en.ts` (`export const en = { ... } as const; export type TranslationKey = keyof typeof en;`) và `locales/vi.ts` (`export const vi: Record<TranslationKey, string> = { ... } as const;`).
  - **Quy ước DSL:** Nội suy `{{paramName}}`; số nhiều dùng khóa đôi `<key>_one` / `<key>_other` qua `tCount(baseKey, count, params?)`.
  - `index.tsx`: Synchronous static store (`tStatic`, `tCountStatic`, `getEffectiveBcp47`, `getUiLocale`) đồng bộ tức thì với `<I18nProvider>` và `useI18n()`. Tự động cập nhật `document.documentElement.lang`, `dir`, và `document.title = t('app.document_title', { section })`.
  - `format.ts`: `formatDateTime` / `formatNumber` dùng `getEffectiveBcp47()`.
- **`cms/admin/src/api/locales.ts` (`SiteLocalesSnapshot` & `AdminLocaleState`):**
  - Bổ sung `multilingual_enabled: boolean` vào `SiteLocalesSnapshot`.
  - Bổ sung `multilingualEnabled: boolean` và `hasStoredSecondaryLocales: boolean` vào `AdminLocaleState` (default `false` trong `EMPTY_ADMIN_LOCALE_STATE`).
  - Cập nhật `deriveAdminLocaleState(snapshot)`:
    - `multilingualEnabled: snapshot.multilingual_enabled`
    - `hasStoredSecondaryLocales: allLocales.length > 1`
    - `hasMultipleLocales: snapshot.multilingual_enabled && allLocales.length > 1`
    - `hasMultiplePublicLocales: snapshot.multilingual_enabled && enabledLocales.length > 1`
  - Mọi `LocaleTabs` và UI đa ngôn ngữ tự động ẩn/hiện đồng bộ qua `hasMultipleLocales`.

#### 4.2. Khởi tạo `App.tsx`, `AdminShell.tsx` & `router.ts`
- **`App.tsx`:** Nạp `getAdminI18nBootstrap()` ngay lúc mount (tránh flash tiếng Việt trên `statusMessage`), truyền `onLocalesChanged` đồng bộ.
- **`AdminShell.tsx` + `router.ts`:** Thêm `#/settings/multilingual` vào nhóm `Settings` (`adminOnly: true`), bổ sung `'multilingual'` vào `subLabels`, dịch 100% nhãn navigation, nhóm sidebar, `'Quản trị'`, `'403 Không có quyền truy cập'`, và title.

#### 4.3. Hai màn hình Settings (`GeneralSettingsPanel.tsx` & `MultilingualSettingsPage.tsx`)
1. **`GeneralSettingsPanel.tsx` (`#/settings/general`):**
   - Bổ sung `<select id="od-cms-locale" data-testid="cms-locale-select">` (disabled trên tab nội dung phụ; hiển thị ghi chú fallback nếu `!installed`; gọi `useI18n().refreshBootstrap()` ngay sau khi lưu để đổi ngôn ngữ toàn UI không cần F5).
   - Gỡ `<ContentLocalesSection />`, thêm link ngắn tới `#/settings/multilingual`.
   - Khóa `#od-content-language` (`disabled` + notice ở `L370, L425, L431-L433`) theo **`localeState.multilingualEnabled || localeState.hasStoredSecondaryLocales`** (khớp 100% điều kiện `MultiLocaleConflict` của `save_public` backend!).
2. **`MultilingualSettingsPage.tsx` (`#/settings/multilingual`):**
   - **Khối 1 (`multilingual-toggle-section`):** Checkbox `multilingual-enabled-checkbox`; tick bật lưu ngay (`PUT /api/admin/locales/multilingual` `{ enabled: true }`); bỏ tick mở modal `disable-multilingual-modal` (`confirm-disable-multilingual` / `cancel-disable-multilingual`); rollback + render `locale-conflicts` nếu `409`; hiện notice stale nếu `412`.
   - **Khối 2 (`content-locales-section`):** Khi `!localeState.multilingualEnabled`, khóa/ẩn Add/Edit/Enable/Disable/Set Default kèm notice giải thích, giữ nút `delete-locale-<code>` cho ngôn ngữ phụ. Khi bật, đầy đủ Add, Inline Edit (`edit-locale-<code>`, `edit-locale-label-<code>`, `edit-locale-direction-<code>`, `save-locale-<code>`, `cancel-edit-locale-<code>`), Set Default, Toggle Enable, Delete (`delete-locale-<code>` + `delete-locale-modal`).
   - **Khối 3 (`orphaned-locales-section`):** Hoạt động kể cả khi tắt đa ngôn ngữ; tải `GET /api/admin/locales/orphans`, hiển thị bảng 7 cột breakdown + tổng, mở modal `purge-orphan-modal` yêu cầu nhập đúng `code` vào `purge-confirm-input` để mở nút `purge-confirm-submit`.
   - **Đồng bộ CAS chéo giữa Khối 1/2 và Khối 3:** Mọi thao tác mutate thành công ở Khối 1/2 (gồm Add/Edit/Toggle/SetDefault/Delete) **và** Khối 3 (`purgeOrphanedLocale`) đều gọi **cả** `await onLocalesChanged()` lẫn `await loadOrphans()` (hoặc reload tự động theo `localeState.version`), tránh lỗi `412` tự gây ra giữa các khối dùng chung `site_locales_state.version`!

#### 4.4. Bộ kiểm tra Tự động `cms/scripts/check-i18n.mjs`
- Node built-in `node:fs` / `node:path` only; parse deterministic flat object literals `export const <id> ... = { ... } as const;`.
- Hỗ trợ `--mode=s03` (parity `en.ts` ↔ `vi.ts` + quét file phạm vi S03) và `--strict` (quét 100% `cms/admin/src/**/*.{ts,tsx}` ngoài `src/i18n/locales/vi.ts`). Kiểm thử độ phủ catalog lỗi backend nằm trong `cargo test` (`src/i18n.rs`).

---

## Alternatives Considered

| Quyết định thiết kế | Phương án được chọn | Phương án bị loại & Lý do bác bỏ |
| --- | --- | --- |
| **Lưu & Cache `cms_locale` khi xử lý lỗi HTTP** | `site_general_settings.cms_locale` + `AppState.ui_locale: Arc<AtomicU8>` + non-blocking `try_acquire()` gọi `storage::settings::get_cms_locale_in_conn`. | **Query SQLite trực tiếp mỗi lần lỗi `4xx`/`5xx`**: nghẽn pool `max_connections = 5`, chờ `5s` hoặc deadlock khi DB `503`. **Theo `users.ui_locale`**: trái yêu cầu WordPress-style toàn site. |
| **Phạm vi & Layering `localize_error_response`** | Đặt ngoài `Host/Origin` (`app.rs#L751`) và trước `.fallback`, dùng marker `HostOriginRejected` + allowlist bounded (`/auth/`, `/api/admin/`, `/api/themes/`, `/admin`, `/builder`, `/registry`, `/page`, `/preview`, `/save`), dịch hai chiều `vi <-> en`, xử lý `HEAD`, bound `1 MiB` pre-check, cập nhật `Content-Length`/`Transfer-Encoding`. | **Không đánh dấu `HostOriginRejected`**: không phân biệt được `Origin không hợp lệ.` từ middleware với `routes/contact.rs#L257`. **Thiếu `/api/themes/` & Builder paths**: bỏ sót 7 Admin API endpoints. |
| **Redirect parked & Patterns khi Delete/Purge** | `parked` redirect của `deleted_at IS NOT NULL` đứng ngoài `previous_history`/`historical` và nhường chỗ bằng `ON CONFLICT DO UPDATE` nếu active route chiếm `source_path`; `reassign_locale_patterns_to_default_in_tx` kèm khử trùng slug chạy trong cả `DELETE` (`total == 0`) và Purge Step 0. | **Xóa thẳng ở `L1973` hoặc bỏ qua ở `L1526`**: phá lossless soft-delete hoặc gây `500 PK/UNIQUE/FK violation`. |
| **React Admin i18n** | Pure TS `I18nProvider` + `tStatic()` + `*_one`/`*_other` (0 npm deps) + `check-i18n.mjs`. | **Cài `i18next`/`react-intl`** (tăng dependency, thiếu kiểm parity static nhẹ). |

---

## Non-Functional Requirements

1. **Bảo mật & Phân quyền:** `GET /api/i18n/admin` chỉ đọc metadata công khai (`no-store`); mọi route mới ở `/api/admin/locales*` yêu cầu Admin + CSRF + `AdmittedMutation` + `If-Match`; 100% SQL nằm trong `storage/`, 100% HTTP frontend đi qua [`client.ts`](../../cms/admin/src/api/client.ts).
2. **Bảo toàn dữ liệu & Giới hạn:** `sqlite_schema` giữ nguyên `219 <= 256`; `pattern`/`media_assets`/`site_menus` không bị xóa nhầm; Purge atomicity 100%.
3. **Hiệu năng:** `0` DB query & `0` body allocation trên `2xx`/`3xx`, `0` blocking query trên `4xx`/`5xx`.

---

## Detailed Implementation Plan

Thực thi **tuần tự nghiêm ngặt (`S01 -> S02 -> S03 -> S04 -> S05`)**.

### Slice S01: Migration `0051`, PRD Sync, `CMS_LANGUAGE_CATALOG`, Bootstrap & Backend Error Localization

- **Dependencies:** `main@f7cc9520dea764214ad055a5a95a72c8515e4536` và sự phê duyệt của Owner (`Approved By: Owner` cho mở rộng phạm vi PRD).
- **File Ownership:**
  - `docs/product/PRODUCT_REQUIREMENTS.md` (cập nhật §1, §3, §6, §7 đồng bộ với sự phê duyệt của Owner)
  - `cms/migrations/0051_admin_i18n_and_multilingual_settings.sql`
  - `cms/migrations/README.md` (bổ sung mô tả migration `0051`)
  - `cms/crates/opendash-core/src/{i18n.rs,lib.rs}`
  - `cms/crates/opendash-server/src/{main.rs,app.rs,error.rs,i18n.rs}`
  - `cms/crates/opendash-server/src/routes/{operations.rs,plugin.rs,settings.rs}` (chỉ bổ sung `ui_locale` vào 2 unit-test `AppState` literals + `admin_i18n_bootstrap` & `general_put` cache sync)
  - `cms/crates/opendash-server/src/storage/{entries.rs,overrides.rs,settings.rs}` (`overrides.rs#L3155` cập nhật `GeneralSettingsInput`; `entries.rs` sửa đủ **18** chuỗi mojibake `?` về UTF-8 sạch; `settings.rs` thêm `cms_locale`, `get_cms_locale_in_conn`, `AdminI18nBootstrap`)
  - `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs` (cập nhật `assert_eq!(migration_version, 51)`)
  - `cms/crates/opendash-server/tests/admin_i18n_s01_test.rs` (test tích hợp mới: `0051` + upgrade backfill, `GET /api/i18n/admin`, `PUT /api/admin/settings/general`, `en`/`vi`/fallback `ja`/`ar`, middleware trên Admin/Auth/Themes/Builder + `HostOriginRejected` + `ContentBeforeSaveVeto.message` + `PluginRunResponse` + `Content-Length`/`HEAD`/bound `1 MiB` + exempt `/api/public/*` + `0` deadlock khi `503`)
- **Acceptance Criteria:**
  - [ ] `0051` chạy sạch trên DB trống và nâng cấp từ `0050`, giữ `sqlite_schema` ở `219 <= 256`; fresh DB (`cms_locale = 'en'`, `multilingual_enabled = 0`); nâng cấp `> 1` locale (`multilingual_enabled = 1`, `cms_locale = 'en'`).
  - [ ] `GET /api/i18n/admin` công khai trả 32 ngôn ngữ (`en`/`vi` `installed: true`), fallback `ja`/`ar` về `en`/`en-US`/`ltr`.
  - [ ] `PUT /api/admin/settings/general` lưu/chuẩn hóa `cms_locale`, cập nhật `AppState.ui_locale` sau commit, chặn sửa khác giá trị toàn cục ở tab phụ (`400 settings.secondary_cms_locale_locked`), và tương thích payload cũ thiếu `cms_locale`.
  - [ ] Middleware `localize_error_response` (đặt ngoài Host/Origin check + `HostOriginRejected`, trước `.fallback`) dùng `AtomicU8` + `storage::settings::get_cms_locale_in_conn`, dịch hai chiều (`vi <-> en`) cho phạm vi Admin/Auth/Themes/Builder + Host/Origin rejections, bảo toàn `/api/public/*` và `text/html`, chuẩn hóa `Content-Length`/`HEAD`/1 MiB bound.
  - [ ] Toàn bộ 18 chuỗi mojibake trong `storage/entries.rs` được sửa sạch; `cargo test --workspace --release --locked` và `check-standards.py` PASS.
- **Rollback:** Revert PR của S01.

### Slice S02: Toggle `multilingual_enabled`, Soft-Delete/Revive Ngôn ngữ Phụ & Công cụ Orphan Purge

- **Dependencies:** `Slice S01`.
- **File Ownership:**
  - `cms/crates/opendash-server/src/app.rs` (thêm routes `/api/admin/locales/multilingual`, `DELETE /api/admin/locales/{code}`, `/api/admin/locales/orphans`, `/api/admin/locales/orphans/{code}/purge`)
  - `cms/crates/opendash-server/src/routes/settings.rs` (`locales_multilingual_put`, `locales_delete`, `locales_orphans_get`, `locales_orphans_purge`)
  - `cms/crates/opendash-server/src/plugin/runtime.rs` (`HostReadRequest::{ContentGet, ContentList, TaxonomyList}` `deleted_at IS NULL` & `is_public_scope` gating)
  - **`12` module `cms/crates/opendash-server/src/storage/`**: `{builder_binding.rs,dashboard.rs,entries.rs,entry_table.rs,media_refs.rs,permalinks.rs,plugin.rs,public_content.rs,seo.rs,settings.rs,sitemap.rs,taxonomy.rs}` (cập nhật đầy đủ §3.1–§3.4 gồm `EnabledLocaleInfo.label`, `reassign_locale_patterns_to_default_in_tx`, `parked_redirects` + `ON CONFLICT DO UPDATE`, `seo.rs#L320, L1660`, `public_content.rs#L597, L1393`, `taxonomy.rs#L780-806`, `plugin.rs#L1040-1058`, `prepare/commit_entry_translation`)
  - `cms/crates/opendash-server/tests/m8_s01_t02_writer_barrier.rs` (bổ sung `locales_multilingual_put`, `locales_delete`, `locales_orphans_purge` vào `settings.rs` inventory)
  - `cms/crates/opendash-server/tests/{multilingual_test.rs,multilingual_s01_test.rs,multilingual_s02_test.rs,multilingual_s03_test.rs,multilingual_s04_test.rs}` (bổ sung bật `PUT /api/admin/locales/multilingual` `{"enabled": true}` trong setup)
  - `cms/crates/opendash-server/tests/admin_i18n_s02_multilingual_purge_test.rs` (test tích hợp mới phủ toggle bảo toàn DB & `409 multilingual.disabled`, `DELETE` mặc định `422`, `DELETE` rỗng có/không có `pattern` trùng slug, soft-delete + `parked_redirects` không văng `500` PK + revive, `save_public` `409 locale.orphaned_conflict`, `build_theme_locale_context`, `media_is_referenced#L1393`, `HostReadRequest`, `Purge` tô-pô & bump `site_reading_settings.version`)
- **Acceptance Criteria:**
  - [ ] `PUT /api/admin/locales/multilingual` bảo toàn DB khi tắt, ẩn toàn bộ route/`hreflang`/sitemap/language-switcher/Builder-Pattern media ACL phụ, và khôi phục hoàn chỉnh khi bật lại.
  - [ ] `DELETE /api/admin/locales/{code}` chuyển `pattern` (kể cả trùng slug) + xóa cứng khi `total == 0`, soft-delete khi `> 0`, cấm mặc định (`422`), và revive sạch (chặn vượt `MAX_SITE_LOCALES`).
  - [ ] `POST /api/admin/locales/orphans/{code}/purge` chuyển `pattern`, bump `site_reading_settings.version` chéo, xóa đủ 7 nhóm theo thứ tự tô-pô (`0` lỗi FK/PK/trigger), giữ `media_assets` & `site_menus`, giải phóng pins.
  - [ ] Toàn bộ 12 storage files + `plugin/runtime.rs` + `m8_s01_t02_writer_barrier.rs` cập nhật đủ; `cargo test --workspace --release --locked` và `check-standards.py` PASS.
- **Rollback:** Revert PR của S02.

### Slice S03: Hạ tầng React Admin i18n (`en` & `vi`), `General -> CMS Language` & Trang `Settings -> Multilingual`

- **Dependencies:** `Slice S02`.
- **File Ownership:**
  - `cms/admin/index.html` (`<html lang="en">`, `<title>OpenDash — CMS Admin</title>`)
  - `cms/admin/src/i18n/{index.tsx,format.ts,locales/en.ts,locales/vi.ts}`
  - `cms/admin/src/api/{client.ts,auth.ts,locales.ts,settings.ts}` (`errorCode`, `"message"` + `"error"`, `tStatic`, `AdminI18nBootstrap`, `SiteLocalesSnapshot.multilingual_enabled`, `AdminLocaleState.{multilingualEnabled,hasStoredSecondaryLocales}`, `setMultilingualEnabled`, `deleteLocale`, `listOrphanedLocales`, `purgeOrphanedLocale`)
  - `cms/admin/src/{App.tsx,types.ts}`
  - `cms/admin/src/shell/{AdminShell.tsx,AuthPanel.tsx,MediaRoute.tsx,router.ts}`
  - `cms/admin/src/components/{AdminPrimitives.tsx,FormPrimitives.tsx,LocaleTabs.tsx}`
  - `cms/admin/src/features/system/{GeneralSettingsPanel.tsx,SystemStatus.tsx}`
  - `cms/admin/src/features/settings/{MultilingualSettingsPage.tsx,ReadingSettingsPanel.tsx,PermalinkSettingsPage.tsx}`
  - `cms/scripts/check-i18n.mjs` (`--mode=s03` & `--strict`)
- **Acceptance Criteria:**
  - [ ] `App.tsx` nạp `getAdminI18nBootstrap()` ngay lúc mount (`0` nháy chữ Việt); `AuthPanel` & `AdminShell` chạy sạch 100% ở `en` và `vi`.
  - [ ] `GeneralSettingsPanel.tsx` có `<select data-testid="cms-locale-select">` đổi ngôn ngữ tức thì không cần F5, gỡ `<ContentLocalesSection />`, và khóa `#od-content-language` trên `localeState.multilingualEnabled || localeState.hasStoredSecondaryLocales`.
  - [ ] `MultilingualSettingsPage.tsx` (`#/settings/multilingual`) đủ checkbox + confirm tắt, bảng ngôn ngữ (Add / Inline Edit / Toggle / Set Default / Delete + modal), bảng `locale-conflicts`, Orphan Purge + modal xác nhận `confirm_code`, và tự đồng bộ `onLocalesChanged()` + `loadOrphans()` chéo giữa 3 khối sau mỗi mutation.
  - [ ] `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `node cms/scripts/check-i18n.mjs --mode=s03`, `cargo build --manifest-path cms/Cargo.toml --locked` và `check-standards.py` PASS.
- **Rollback:** Revert PR của S03.

### Slice S04: Migration 100% Toàn bộ `api/`, `features/`, `studio/` & `builder/` sang `i18n` + Bật `--strict`

- **Dependencies:** `Slice S03`.
- **File Ownership:** (danh sách đầy đủ 100% khớp thực tế mã nguồn)
  - `cms/admin/src/i18n/locales/{en.ts,vi.ts}` (trọn bộ keys cho toàn Admin)
  - **`18` API modules còn lại** (`cms/admin/src/api/`): `builder.ts`, `contact.ts`, `content.ts`, `dashboard.ts`, `index.ts`, `media.ts`, `menus.ts`, `operations.ts`, `patterns.ts`, `plugins.ts`, `postPage.ts`, `sections.ts`, `seo.ts`, `smtp.ts`, `studio.ts`, `taxonomy.ts`, `themes.ts`, `users.ts`
  - **`39` Feature modules còn lại** (`cms/admin/src/features/`): `contact/{ContactPanel,SmtpSettingsPanel}.tsx`, `content/{ContentTypeForm,ContentTypeSchemaSettings,ContentWorkspace,EditorLayoutConfigurator,EntryListPage,FieldGroupsManager,FieldValueInput}.tsx` (chỉ reset `?locale=` trên `EntryListPage.tsx` khi `localeState.allLocales.length > 0` và `locale` không còn hợp lệ để tránh xóa deep-link `#/posts?locale=en` lúc mount ban đầu), `content/types.ts`, `dashboard/DashboardPage.tsx`, `editor/{EditorDraftStore.ts,PanelRegionRenderer.tsx,SingleEditWorkspace.tsx}`, `extensions/AdminContributionRenderer.tsx`, `library/TemplateLibraryPage.tsx`, `media/{MediaLibraryPage,MediaPicker}.tsx`, `media/library/{fileValidation,format}.ts`, `menus/MenusPanel.tsx`, `operations/{BackupsPanel,ReconcilePanel,RestorePanel}.tsx`, `patterns/PatternsPage.tsx`, `plugins/{DeclarativePluginUi,PluginsPanel}.tsx`, `plugins/declarativeSchema.ts`, `posts/PostPageWorkspace.tsx`, `seo/SeoPanel.tsx`, `shared/OwnerOptionsForm.tsx`, `taxonomy/{TaxonomyDefinitionsPage,TaxonomyPage,TermEditPage,TermPicker}.tsx`, `themes/{ThemeOptionsForm,ThemesPanel}.tsx`, `users/{UserProfilePanel,UsersPanel}.tsx`
  - **`7` Studio modules** (`cms/admin/src/studio/`): `StudioBlockCatalog.tsx`, `StudioBlockTree.tsx`, `StudioCustomizerNav.tsx`, `StudioMenuManager.tsx`, `StudioSidebarFooter.tsx`, `StudioSidebarHeader.tsx`, `StudioWorkspace.tsx` (kèm tự reset `previewLocale` nếu locale bị tắt/xóa)
  - **`13` Builder modules** (`cms/admin/src/builder/`): `BlockSchemaForm.tsx`, `BuilderBindingFields.tsx`, `BuilderCanvas.tsx`, `BuilderContactFormFields.tsx`, `BuilderInspector.tsx`, `BuilderListFields.tsx`, `BuilderPreviewFrame.tsx`, `BuilderTree.tsx`, `BuilderWorkspace.tsx`, `PatternLibraryModal.tsx`, `StudioFieldControls.tsx`, `StudioRichTextLightbox.tsx`, `document.ts`
  - `cms/scripts/check-i18n.mjs` (bật mặc định `--strict`)
- **Acceptance Criteria:**
  - [ ] `node cms/scripts/check-i18n.mjs --strict` PASS (`0` chuỗi tiếng Việt hardcode lọt ngoài `src/i18n/locales/vi.ts`, 100% key `en.ts` ↔ `vi.ts` khớp nhau và khớp placeholder `{{...}}`).
  - [ ] Toàn bộ 22 `api/*.ts`, 43 `features/**/*`, 7 `studio/*`, 13 `builder/*`, `formatDateTime`/`formatNumber` hoạt động chuẩn ở cả `en` và `vi`.
  - [ ] `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `cargo fmt --manifest-path cms/Cargo.toml --all --check`, `cargo build --manifest-path cms/Cargo.toml --locked`, `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` và `check-standards.py` PASS.
- **Rollback:** Revert PR của S04.

### Slice S05: Đồng bộ Tài liệu Contracts/Nav, Browser E2E Suite & Alignment Các Script Playwright Cũ

- **Dependencies:** `Slice S04`.
- **File Ownership:**
  - `docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md` (cập nhật nhóm Cài đặt: bổ sung `#/settings/reading` và `#/settings/multilingual`)
  - `docs/contracts/M8_WRITER_INVENTORY.md` (bổ sung `locales_multilingual_put`, `locales_delete`, `locales_orphans_purge` vào hàng *Settings & Permalinks*)
  - `docs/designs/SITEMAP.md` (cập nhật trạng thái `Implemented` / `Verified` khi đóng spec kèm link biên bản verification)
  - `cms/scripts/admin_i18n_multilingual_e2e.py` (tạo mới: Playwright Live Browser E2E kiểm chứng đủ CUJ-1 → CUJ-4 trên release binary)
  - **`52` script Playwright/smoke hiện hành bị ảnh hưởng bởi `cms_locale = 'en'` mặc định** (`cms/scripts/`): `admin_nav_single_edit_e2e.py`, `builder_binding_inspector_browser.py`, `builder_binding_inspector_release_g5.py`, `builder_editor_browser.py`, `builder_editor_integration_browser.py`, `builder_list_browser.py`, `builder_list_release_g5.py`, `builder_list_smoke.py`, `builder_responsive_browser.py`, `content_entries_smoke.py`, `content_foundation_m3_s31_t01_e2e.py`, `content_foundation_m4_s12_t02_e2e.py`, `content_foundation_m6_s11_t01_e2e.py`, `content_foundation_m7_s17_t02_e2e.py`, `dashboard_browser.py`, `demo-website-runner.py`, `egm_cpt_builder_s01_e2e.py`, `egm_pattern_library_s03_e2e.py`, `entry_terms_browser.py`, `general_settings_browser.py`, `general_settings_smoke.py`, `m4_release_macos.py`, `m4_release_windows.py`, `m4_s11_t01_page_builder_e2e.py`, `m4_s11_t02_sections_e2e.py`, `m5_beta_theme_v2_e2e.py`, `m5_s12_t01_library_e2e.py`, `m6_s10_t01_admin_extensions_e2e.py`, `m7_beta_draft_navigation_e2e.py`, `m7_beta_media_picker_e2e.py`, `m7_beta_menu_picker_e2e.py`, `m7_browser_e2e.py`, `m7_s16_t01_media_upload_e2e.py`, `m7_s16_t02_navigation_e2e.py`, `m7_s16_t03_editor_e2e.py`, `m7_s16_t04_taxonomy_e2e.py`, `m7_s16_t06_entry_table_e2e.py`, `m8_browser_e2e.py`, `public_content_smoke.py`, `taxonomy_terms_browser.py`, `taxonomy_terms_smoke.py`, `test_dev_launcher.py`, `themes_browser.py`, `themes_smoke.py`, `windows_admin_browser.py`, `windows_auth_smoke.py`, `windows_distribution_smoke.py`, `windows_m3_release_e2e.py`, `windows_plugin_smoke.py`, `windows_plugins_browser.py`, `windows_smoke.py`, `windows_users_browser.py`
- **Acceptance Criteria:**
  - [ ] Tài liệu `ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`, `M8_WRITER_INVENTORY.md` đồng bộ 100% code; `check-plans.py`, `check-standards.py`, `check-agent-artifacts.py`, và `node cms/scripts/codegraph-index-docs.mjs` PASS.
  - [ ] `python3 cms/scripts/admin_i18n_multilingual_e2e.py` PASS end-to-end (CUJ-1 `en` default -> đổi `vi` tức thì -> CUJ-2 `ja`/`ar` fallback `en` LTR -> CUJ-3 tắt/bật Multilingual bảo toàn DB -> CUJ-4 xóa ngôn ngữ rỗng + soft-delete + Orphan Purge giữ media/patterns).
  - [ ] Toàn bộ nhóm script đại diện (`general_settings_browser.py`, `m7_s16_t02_navigation_e2e.py`, `windows_users_browser.py`, `windows_auth_smoke.py`, `m7_s16_t06_entry_table_e2e.py`) PASS trên binary `0051`.
- **Rollback:** Revert PR của S05.

---

### Target Files to Create/Modify

| File Path | Slice | Action | Scope / Key API / Reason |
| --- | --- | --- | --- |
| `docs/product/PRODUCT_REQUIREMENTS.md` | S01 | Modify | Mở rộng V1 đưa Admin UI & API L10n + Settings Multilingual vào phạm vi chuẩn (theo duyệt Owner). |
| `cms/migrations/0051_admin_i18n_and_multilingual_settings.sql` | S01 | Create | Thêm `cms_locale`, `multilingual_enabled` (kèm backfill `>1` locale), `deleted_at`; `+0` `sqlite_schema` objects. |
| `cms/migrations/README.md` | S01 | Modify | Đăng ký migration `0051`. |
| `cms/crates/opendash-core/src/{i18n.rs,lib.rs}` | S01 | Create / Modify | `CMS_CATALOG_VERSION`, `CmsLocaleOption`, `CMS_LANGUAGE_CATALOG` (32 mục), `find_cms_locale`, `resolve_effective_ui_locale`. |
| `cms/crates/opendash-server/src/{main.rs,app.rs,error.rs,i18n.rs}` | S01 | Create / Modify | `AppState.ui_locale`, `HostOriginRejected`, `EXACT_ERROR_ENTRIES`/`TEMPLATE_MATCHERS`, middleware `localize_error_response`, `/api/i18n/admin`. |
| `cms/crates/opendash-server/src/routes/{operations,plugin,settings}.rs` | S01 | Modify | Cập nhật 2 `AppState` test literals; `admin_i18n_bootstrap`, `general_put` cache sync. |
| `cms/crates/opendash-server/src/storage/{entries,overrides,settings}.rs` | S01 | Modify | `GeneralSettingsInput` `cms_locale` (`overrides.rs#L3155`, `settings.rs#L2667`), sửa **18** mojibake `entries.rs`, thêm `get_cms_locale_in_conn`. |
| `cms/crates/opendash-server/tests/{admin_i18n_s01_test,content_foundation_m6_s11_t01}.rs` | S01 | Create / Modify | Test S01 + cập nhật `migration_version == 51`. |
| `cms/crates/opendash-server/src/{app.rs,routes/settings.rs,plugin/runtime.rs}` | S02 | Modify | Thêm routes & handlers `multilingual`, `DELETE`, `orphans`, `orphans/{code}/purge`; `HostReadRequest` gating. |
| `cms/crates/opendash-server/src/storage/{builder_binding,dashboard,entries,entry_table,media_refs,permalinks,plugin,public_content,seo,settings,sitemap,taxonomy}.rs` | S02 | Modify | `12` storage modules: `multilingual_enabled` gating, `deleted_at IS NULL`, `EnabledLocaleInfo.label`, `reassign_locale_patterns_to_default_in_tx`, `parked_redirects`, `delete_locale`, revive, `Purge`. |
| `cms/crates/opendash-server/tests/{admin_i18n_s02_multilingual_purge_test,m8_s01_t02_writer_barrier,multilingual*}.rs` | S02 | Create / Modify | Test S02 + cập nhật writer inventory + bật `multilingual_enabled: true` ở 5 suite cũ. |
| `cms/admin/index.html` & `cms/admin/src/{App.tsx,types.ts,i18n/*,api/{client,auth,locales,settings}.ts,shell/*,components/*,features/{system/*,settings/*}}` | S03 | Create / Modify | `I18nProvider`, `tStatic`, `en`/`vi` shell + settings, `GeneralSettingsPanel`, `MultilingualSettingsPage`. |
| `cms/scripts/check-i18n.mjs` | S03 | Create | Parity `en.ts` ↔ `vi.ts`, `--mode=s03` & `--strict`. |
| `cms/admin/src/{i18n/locales/*,api/*,features/*,studio/*,builder/*}` | S04 | Modify | Migrate 100% `18` `api/`, `39` `features/`, `7` `studio/`, `13` `builder/`, kích hoạt `--strict`. |
| `docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`, `docs/contracts/M8_WRITER_INVENTORY.md`, `docs/designs/SITEMAP.md` | S05 | Modify | Cập nhật `#/settings/reading` + `#/settings/multilingual`, Writer Inventory và đóng Spec trong SITEMAP. |
| `cms/scripts/admin_i18n_multilingual_e2e.py` & 52 `cms/scripts/*.py` | S05 | Create / Modify | Live Browser E2E cho Spec + đồng bộ 52 browser/smoke scripts cũ theo `en` mặc định. |

---

### Verification Plan

Khối lệnh chuẩn dưới đây được kiểm tra cho **tất cả các Slice (`S01`–`S05`)** (theo chuẩn trích xuất khối lệnh đầu tiên của `.agents/scripts/prepare-github-issue.py`):

```bash
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
git diff --check
```

#### Slice-Specific & Closure Gates (Bổ sung theo từng mốc)

- **Sau Slice S03:** `node cms/scripts/check-i18n.mjs --mode=s03`
- **Sau Slice S04:** `node cms/scripts/check-i18n.mjs --strict`
- **Sau Slice S05 & Đóng Spec (`Verified`):**
  - `node cms/scripts/check-i18n.mjs --strict`
  - `python3 cms/scripts/admin_i18n_multilingual_e2e.py`
  - `node cms/scripts/codegraph-index-docs.mjs`
  - Lưu biên bản tại `docs/verification/` và cập nhật `docs/designs/SITEMAP.md` kèm chạy `/egm-drift-report`.

---

## Appendix A — Ghi chú Tham chiếu Quyết định Đã Khóa (Owner Decisions A1–A4)

- **A1 (`cms_locale` Selector):** 1 `<select>` toàn site tại `Settings -> General` lưu `site_general_settings.cms_locale`, mặc định `en`, 32 ngôn ngữ chuẩn WordPress (`en` & `vi` đủ 100%, ngôn ngữ khác fallback sạch về `en`/`en-US`/`ltr`).
- **A2 (Phạm vi Dịch):** Toàn bộ React Admin SPA + toàn bộ HTTP error responses Admin/Auth/Themes/Builder (`text/plain` + JSON `"error"`/`"message"`/`"field_errors"` + `X-OpenDash-Error-Code`). Không rò rỉ sang trang/lỗi công khai.
- **A3 (Xóa Ngôn ngữ & Orphan Purge):** Chuyển `pattern` sang ngôn ngữ mặc định (khử trùng slug) + xóa cứng ngay nếu `0` dữ liệu người dùng/redirect; soft-delete (`deleted_at`) + hồi sinh nếu còn dữ liệu; Purge tô-pô nguyên tử bảo toàn `pattern` CPT, `media_assets` và `site_menus`, yêu cầu gõ lại `confirm_code`.
- **A4 (Tắt Đa ngôn ngữ & Upgrade `0051`):** Tắt giữ nguyên 100% DB, chỉ ẩn UI Admin và rút tuyến phụ khỏi public/SEO/sitemap/theme switcher/Builder media ACL; site upgrade `> 1` locale tự bật `multilingual_enabled = 1`; cả site mới lẫn upgrade mặc định `cms_locale = 'en'`.
