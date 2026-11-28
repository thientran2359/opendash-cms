# Nâng cấp Toàn diện UI/UX Admin CMS (CSS Rescue, Modals/A11y, Studio/Mobile, Draft Safety & i18n)

* **Status:** Approved
* **Approved By:** Owner
* **Approval Evidence:** Owner approved artifact `admin_ui_ux_audit_and_plan.md` on 2026-11-28 (Proceed)
* **Date:** 2026-11-28
* **Baseline:** `main@018959e`
* **Migrations:** Không có (0 SQL migration; 100% phạm vi `cms/admin/` + `cms/l10n/` + `docs/`)
* **Worktree / Branch:** `feat/admin-ui-ux-polish`
* **Goldfish Review:** Chủ dự án chọn làm nhanh theo quy trình EGM Tinh gọn cho UI cô lập (`Owner Directive 2026-11-27`) sau đợt kiểm thử trực tiếp Desktop (`1440×900`) & Mobile (`390×844`) + quét tĩnh mã nguồn Admin; kiểm tra drift sẽ chạy trước khi mở PR.

---

## Overview

Đặc tả này khắc phục toàn bộ các lỗi UI đã xác minh trực tiếp trên bản chạy `main@018959e` (`http://127.0.0.1:18766/admin/`) và nâng cấp các điểm ma sát UX / an toàn dữ liệu trên toàn bộ 16 nhóm màn hình Admin mà **không đụng backend Rust, không thêm SQL migration và không phá vỡ 7 bộ Playwright E2E hiện hữu**.

---

## The Problem

Qua kiểm thử thực tế trên trình duyệt (`1440×900` và `390×844`) kết hợp quét tĩnh `cms/admin/src/`, đã xác nhận các lỗi cụ thể:

1. **8 raw i18n dot-keys hiển thị thô trên UI:** `DashboardPage.tsx#L13-L16` (`feature.dashboard.dashboardpage.*`) và `PermalinkSettingsPage.tsx#L29-L32` (`permalink.preset_*`).
2. **Mất toàn bộ style nút & input ngoài `.od-legacy-scope`:** `MediaPicker.tsx` (khi mở ở Categories/Tags, Studio, Options Pages), `PatternLibraryModal.tsx`, `BackupsPanel.tsx`, và `DeclarativePluginUi.tsx` dùng class `.btn-*`/`.table`/`.content-*` vốn chỉ hoạt động trong `.od-legacy-scope`, khiến nút bị reset về transparent 0px padding (`styles.css#L38-L45`).
3. **Thiếu reset UA cho `<dd>` và `<ul>`:** `SearchIndexToolsPanel.tsx` bị thụt lề trái 40px ở `<dd>`; `ThemesPanel.tsx` và `EditorLayoutConfigurator.tsx` hiện bullet `•` thô trên `<ul>`.
4. **Lỗi trạng thái & logic form rõ rệt:**
   - `FieldGroupEditorPage.tsx#L77-L94`: `fields` khởi tạo 1 field trắng nhưng `initialSerialized` so với `[]` → vừa mở trang mới đã báo `● Unsaved changes`.
   - `ContactPanel.tsx#L40-L68`: khởi tạo `selectedId = null` với `slug = "contact"` trùng sẵn form `#1` nhưng nút submit lại là `Create form`; thiếu header cột `Required`.
   - `ContentTypeSchemaSettings.tsx#L1046`: hiển thị dấu chấm lặp `· · Owner:`.
   - Topbar `#status` (`App.tsx` / `AdminShell.tsx`): lỗi từ màn hình cũ (ví dụ `409` ở `#/tools/health`) dính vĩnh viễn khi chuyển route, thiếu nút đóng và CTA.
5. **6 hành động phá hủy dữ liệu hoàn toàn thiếu xác nhận** (`MediaDetailsPanel`, `ContactPanel` form & submission, `BackupsPanel`, `LanguagePacksPanel`, `FieldGroupsListPage` withdraw) và **Modals/Dropdowns thiếu a11y** (`RedirectFormModal` & `CsvImportModal` không đóng khi nhấn `Escape`/click backdrop + thiếu `htmlFor`/`id`; dropdown `Clear log` ở `NotFoundLogPanel` không đóng khi `Escape`/click ngoài; 2 `<main>` lồng nhau; `<h1>` rỗng ở `ThemesPanel`; `<h1>` trong 5 `<dialog>`).
6. **Studio & Mobile (`390×844`) vỡ/chật cục bộ:** Color hex input & swatch bị cắt trong sidebar `320px` ở `Appearance options`, `#/studio` root không tải sẵn preview homepage, gear icon đè text preview, alert `truncate` 1 dòng, mobile `Studio` sidebar bị ép `max-h-[50vh]` + preview để mặc định `Desktop`, `ContactPanel` field builder 5 cột bị cắt chữ, `BackupsPanel` / `MultilingualSettingsPage` thiếu `overflow-x-auto`.
7. **An toàn bản nháp & UX flow:** `GeneralSettingsPanel` / `SeoPanel` chuyển locale hoặc rời trang mất draft không cảnh báo; `ReadingSettingsPanel` chỉ kiểm tra dirty của locale hiện tại; `RestorePanel` nuốt lỗi polling; `ContactPanel`/`SmtpSettingsPanel`/`BackupsPanel`/`ReconcilePanel` thiếu `<AdminAlert tone="error">` inline; `SeoPanel` & SEO box thiếu `MediaPicker`; `TemplateLibraryPage` thiếu nút `Customize in Studio`; `CustomOptionsPageView` thiếu CTA tới `Field groups`; `AdminShellMenu` đẩy `parent_menu: 'top_level'` vào nhóm `Content`; `FieldGroups` list (`Withdrawn`) vs detail (`Active`) gây nhầm; number inputs ép `|| 0` khi xóa trắng; `PluginsPanel` install mode vẫn hiện nút `Reload list`; `UsersPanel` fetch dư ở `mode === 'new'`; `PatternsPage` nút Delete dùng `btn-secondary`; ~20 chỗ nối chuỗi `__(...)` thủ công và native `<input type="file">` bị cắt/lệch ngôn ngữ trình duyệt.

---

## Goals

- Khắc phục 100% lỗi UI P1/P2 đã xác minh ở Desktop (`1440×900`) và Mobile (`390×844`).
- Chuẩn hóa CSS Tailwind cho 4 component ngoài `.od-legacy-scope`, bổ sung `ConfirmDialog` chuẩn cho 6 hành động phá hủy đang thiếu confirm, và sửa trọn vẹn keyboard/backdrop/landmark a11y cho modals & dropdowns.
- Đảm bảo an toàn bản nháp (`GeneralSettings`, `Seo`, `Reading`), hiển thị lỗi inline rõ ràng, cải thiện các luồng thao tác chính (`Studio`, `SEO OG MediaPicker`, `Template Library → Studio`, `Options Pages`, `Field Groups`, custom file upload button) và dọn sạch 100% chuỗi ghép i18n + `.pot`/`.po`.
- Giữ nguyên 100% tương thích ngược với 7 bộ Playwright E2E hiện có (`dialog` handlers & `data-testid`) và tuân thủ nghiêm ngặt giới hạn LOC (`<= 600` dòng/file mới, không tăng vượt `loc-baseline.json`).

---

## Non-Goals

- Không thêm SQL migration và không sửa code Rust backend (`cms/crates/**`).
- Không thay thế các `window.confirm` hiện đang được 7 script E2E Playwright (`entry_lifecycle_*`, `revisions_autosave_e2e`, `manual_redirects_404_e2e`, `admin_sidebar_ia_e2e`, `acf_content_structure_e2e`) bắt qua `page.on("dialog")`.
- Không thêm dependency npm/Cargo mới.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | LOC / Cap | Summary |
| --- | --- | --- | --- |
| `cms/admin/src/components/ConfirmDialog.tsx` | Create | `<= 140 / 600` | Component `<dialog>` xác nhận chuẩn (`Escape`, backdrop click, destructive/default tone, focus). |
| `cms/admin/src/components/UploadFileButton.tsx` | Create | `<= 100 / 600` | Custom file picker tuân thủ ngôn ngữ CMS thay thế native browser file input text. |
| `cms/admin/src/features/dashboard/DashboardPage.tsx` | Modify | `<= 180 / 600` | Thay 4 raw dot-key `feature.dashboard.dashboardpage.*` bằng English source strings. |
| `cms/admin/src/features/settings/PermalinkSettingsPage.tsx` | Modify | `<= 520 / 600` | Thay 4 raw dot-key `permalink.preset_*` bằng English source strings. |
| `cms/admin/src/features/media/MediaPicker.tsx` | Modify | `<= 320 / 600` | Chuyển toàn bộ nút, header, input search, phân trang sang Tailwind/AdminPrimitives độc lập với `.od-legacy-scope`. |
| `cms/admin/src/builder/PatternLibraryModal.tsx` | Modify | `<= 220 / 600` | Chuyển nút Close, input search, nút Insert sang Tailwind/AdminPrimitives chuẩn. |
| `cms/admin/src/features/operations/BackupsPanel.tsx` | Modify | `<= 290 / 600` | Chuyển bảng & nút sang Tailwind, thêm `overflow-x-auto`, inline `<AdminAlert>`, `ConfirmDialog` xóa backup, dọn chuỗi i18n. |
| `cms/admin/src/features/plugins/DeclarativePluginUi.tsx` | Modify | `<= 350 / 600` | Chuyển tabs, notices, buttons sang Tailwind/AdminPrimitives chuẩn, dọn nối chuỗi i18n. |
| `cms/admin/src/styles.css` | Modify | `<= 520 / 600` | Thêm reset `@layer base` cho `dd { margin: 0 }` và chống đè canvas gear/layout nhỏ. |
| `cms/admin/src/features/operations/SearchIndexToolsPanel.tsx` | Modify | `<= 200 / 600` | Bổ sung `ml-0` cho `<dd>` đảm bảo căn thẳng hàng trên mọi trình duyệt. |
| `cms/admin/src/features/themes/ThemesPanel.tsx` | Modify | `<= 550 / 600` | Thêm `list-none pl-0`, bỏ `<h1>` rỗng thừa nếu có, dọn nối chuỗi i18n, dùng `UploadFileButton`. |
| `cms/admin/src/features/content/EditorLayoutConfigurator.tsx` | Modify | `<= 500 / 600` | Thêm `list-none pl-0` cho danh sách panel/field. |
| `cms/admin/src/features/field-groups/FieldGroupEditorPage.tsx` | Modify | `<= 420 / 600` | Khớp `initialSerialized.fields` với blank field khởi tạo; tách rõ badge trạng thái `Active` vs `Withdrawn`; sửa number input. |
| `cms/admin/src/features/field-groups/FieldGroupsListPage.tsx` | Modify | `<= 340 / 600` | Thêm `ConfirmDialog` cho Withdraw, làm rõ nhãn trạng thái. |
| `cms/admin/src/features/field-groups/FieldSpecEditorCard.tsx` | Modify | `<= 420 / 600` | Xử lý lỗi thay vì `catch {}` rỗng. |
| `cms/admin/src/features/contact/ContactPanel.tsx` | Modify | `<= 560 / 600` | Tự chọn form đầu tiên khi load, header cột `Required`, `ConfirmDialog` xóa form & submission, inline `<AdminAlert>`, `aria-label` nút `×`, responsive mobile field builder, dọn i18n. |
| `cms/admin/src/features/content/ContentTypeSchemaSettings.tsx` | Modify | `<= 1299 / 1299` | Sửa `· · Owner:` và dọn nối chuỗi i18n (giữ `<= 1299` LOC). |
| `cms/admin/src/shell/AdminShell.tsx` & `cms/admin/src/App.tsx` | Modify | `<= 560` & `<= 879` | Clear `#status` khi đổi route, thêm nút dismiss + link `#/tools/media-reconcile` cho lỗi `media.*`, đảm bảo đúng 1 `<main>` / 1 `<h1>`, nối `dirty` cho `GeneralSettings`/`Seo`, fix `ReadingSettings` multi-locale dirty guard. |
| `cms/admin/src/shell/AdminShellMenu.tsx` | Modify | `<= 270 / 600` | Hỗ trợ đúng `parent_menu: 'top_level'` cho Custom Options Pages. |
| `cms/admin/src/features/operations/RedirectFormModal.tsx` & `CsvImportModal.tsx` | Modify | `<= 270` & `<= 310` | Bổ sung `Escape`, backdrop click để đóng, `htmlFor`/`id` đầy đủ cho mọi `<label>` (giữ nguyên `data-testid`). |
| `cms/admin/src/features/operations/NotFoundLogPanel.tsx` | Modify | `<= 420 / 600` | Đóng dropdown `Clear log` khi `Escape`, click ngoài, mở modal; thêm `aria-expanded`, `aria-haspopup`, `aria-label` checkboxes. |
| `cms/admin/src/features/media/MediaDetailsPanel.tsx` | Modify | `<= 330 / 600` | Bổ sung `ConfirmDialog` trước khi xóa media vào quarantine; i18n cho `WebP srcset` / `Fallback srcset`. |
| `cms/admin/src/features/settings/LanguagePacksPanel.tsx` | Modify | `<= 280 / 600` | Bổ sung `ConfirmDialog` xóa override, dùng `UploadFileButton`. |
| `cms/admin/src/features/editor/SingleEditBuilderPromptDialog.tsx`, `SingleEditTranslationModal.tsx`, `TermInlineCreateModal.tsx`, `OptionsPagesListPage.tsx`, `RevisionHistoryModal.tsx` | Modify | `<= 600` each | Đổi `<h1>` trong `<dialog>` sang `<h2>`, thêm `m-auto` còn thiếu; sửa number input & dọn i18n. |
| `cms/admin/src/studio/StudioWorkspace.tsx`, `StudioCustomizerNav.tsx`, `fields/controls/VisualControls.tsx`, `builder/BuilderPreviewFrame.tsx` | Modify | `<= baseline / 600` | Auto-load preview trang chủ ở `#/studio` root, fix color hex input/swatch trong sidebar 320px, bỏ `truncate` trên studio alert, chống gear icon đè text, mobile switcher `Controls \| Preview` + mặc định viewport `mobile`. |
| `cms/admin/src/features/system/GeneralSettingsPanel.tsx`, `features/seo/SeoPanel.tsx`, `features/settings/ReadingSettingsPanel.tsx`, `features/settings/MultilingualSettingsPage.tsx`, `features/contact/SmtpSettingsPanel.tsx`, `features/operations/RestorePanel.tsx`, `features/operations/ReconcilePanel.tsx` | Modify | `<= 600` each | Draft safety + inline `<AdminAlert>`, tích hợp `MediaPicker` cho `SeoPanel`, `overflow-x-auto` + tabs cuộn ngang mobile, fix polling error ở `RestorePanel`, fix number input SMTP. |
| `cms/admin/src/features/library/TemplateLibraryPage.tsx`, `features/options-pages/CustomOptionsPageView.tsx`, `features/patterns/PatternsPage.tsx`, `features/plugins/PluginsPanel.tsx`, `features/users/UsersPanel.tsx`, `features/content/EntryListPage.tsx`, `features/media/MediaLibraryPage.tsx`, `features/taxonomy/{TaxonomyDefinitionsPage,TermEditPage}.tsx`, `features/editor/SingleEditTranslationsPanel.tsx` | Modify | `<= baseline / 600` | Thêm nút `Customize in Studio` ở `TemplateLibraryPage`, CTA tới `Field groups` ở `CustomOptionsPageView`, bỏ `Reload list` ở `#/plugins/install`, tránh `listUsers()` ở `#/users/new`, nút Delete màu danger ở `PatternsPage`, đồng nhất format ngày ở `EntryListPage`, dọn chuỗi ghép i18n. |
| `cms/l10n/opendash.pot` & `cms/l10n/vi.po` | Modify | — | Cập nhật & đối soát đủ 100% khóa gettext Anh–Việt mới/sửa. |

---

### Slice S01: Vá lỗi UI P1 & Cứu CSS ngoài `.od-legacy-scope`

- **Dependencies:** Baseline `main@018959e` + Owner approval.
- **File Ownership:** `DashboardPage.tsx`, `PermalinkSettingsPage.tsx`, `MediaPicker.tsx`, `PatternLibraryModal.tsx`, `BackupsPanel.tsx`, `DeclarativePluginUi.tsx`, `styles.css`, `SearchIndexToolsPanel.tsx`, `ThemesPanel.tsx`, `EditorLayoutConfigurator.tsx`, `FieldGroupEditorPage.tsx`, `ContactPanel.tsx`, `ContentTypeSchemaSettings.tsx`, `AdminShell.tsx`, `App.tsx`.
- **Acceptance Criteria:**
  - [ ] Hết toàn bộ 8 raw dot-key trên Dashboard & Permalinks.
  - [ ] `MediaPicker`, `PatternLibraryModal`, `BackupsPanel`, `DeclarativePluginUi` hiển thị đầy đủ nền/viền/padding khi render ngoài `.od-legacy-scope`.
  - [ ] `<dd>` ở Search Index căn thẳng hàng, `<ul>` card ở Themes & Editor Layout không còn bullet thô.
  - [ ] `#/content-structure/field-groups/new` mở ra với `dirty === false`; `#/contact` tự chọn form đầu tiên khi có form & có header cột `Required`; hết `· · Owner:`.
  - [ ] Topbar `#status` tự xóa khi đổi route, có nút đóng `×` và CTA `#/tools/media-reconcile` khi gặp lỗi `media.*`.
- **Rollback:** `git revert` commit `S01` trên `feat/admin-ui-ux-polish`.

### Slice S02: Chuẩn hóa ConfirmDialog, Modal/Dropdown Keyboard & Landmark A11y

- **Dependencies:** `S01`.
- **File Ownership:** `components/ConfirmDialog.tsx`, `MediaDetailsPanel.tsx`, `ContactPanel.tsx`, `BackupsPanel.tsx`, `LanguagePacksPanel.tsx`, `FieldGroupsListPage.tsx`, `RedirectFormModal.tsx`, `CsvImportModal.tsx`, `NotFoundLogPanel.tsx`, `AdminShell.tsx`, `SingleEditBuilderPromptDialog.tsx`, `SingleEditTranslationModal.tsx`, `TermInlineCreateModal.tsx`, `OptionsPagesListPage.tsx`, `RevisionHistoryModal.tsx`.
- **Acceptance Criteria:**
  - [ ] 6 hành động phá hủy đang thiếu confirm đều đi qua `ConfirmDialog` chuẩn (`Escape`, backdrop click, nút destructive đỏ).
  - [ ] `RedirectFormModal` & `CsvImportModal` đóng được bằng `Escape` & click backdrop, đủ `htmlFor`/`id`, giữ nguyên `data-testid`.
  - [ ] Dropdown `Clear log` ở `NotFoundLogPanel` có `aria-expanded`/`aria-haspopup`, đóng khi `Escape`, click ngoài, hoặc khi mở modal `Create Redirect`; checkboxes & nút `×` ở `ContactPanel` có `aria-label`.
  - [ ] Đúng 1 `<main>` và 1 `<h1>` có nghĩa trên mọi route; không còn `<h1>` trong `<dialog>`.
- **Rollback:** `git revert` commit `S02` trên `feat/admin-ui-ux-polish`.

### Slice S03: Studio UI & Responsive Mobile (`390×844`)

- **Dependencies:** `S02`.
- **File Ownership:** `StudioWorkspace.tsx`, `StudioCustomizerNav.tsx`, `fields/controls/VisualControls.tsx`, `builder/BuilderPreviewFrame.tsx`, `ContactPanel.tsx`, `BackupsPanel.tsx`, `MultilingualSettingsPage.tsx`.
- **Acceptance Criteria:**
  - [ ] Mở `#/studio` (root) tự tải sẵn preview trang chủ; color hex input & swatch ở `Appearance options` hiển thị đủ không bị cắt trong sidebar 320px; gear icon không đè nội dung block; alert Studio hiển thị đủ nhiều dòng.
  - [ ] Trên mobile (`390×844`): Studio mặc định viewport preview `mobile` và chuyển đổi gọn `Controls | Preview`; `ContactPanel` không cắt chữ key field; bảng `BackupsPanel` và bảng conflict `Multilingual` có `overflow-x-auto`; tab bar `Multilingual` nằm gọn 1 hàng cuộn ngang.
- **Rollback:** `git revert` commit `S03` trên `feat/admin-ui-ux-polish`.

### Slice S04: Nâng cấp UX Flow, An toàn Bản nháp & Phản hồi Lỗi

- **Dependencies:** `S03`.
- **File Ownership:** `GeneralSettingsPanel.tsx`, `SeoPanel.tsx`, `ReadingSettingsPanel.tsx`, `RestorePanel.tsx`, `ReconcilePanel.tsx`, `SmtpSettingsPanel.tsx`, `TemplateLibraryPage.tsx`, `CustomOptionsPageView.tsx`, `AdminShellMenu.tsx`, `FieldGroupsListPage.tsx`, `FieldGroupEditorPage.tsx`, `FieldSpecEditorCard.tsx`, `OptionsPagesListPage.tsx`, `TermEditPage.tsx`, `PluginsPanel.tsx`, `UsersPanel.tsx`, `PatternsPage.tsx`, `EntryListPage.tsx`, `App.tsx`.
- **Acceptance Criteria:**
  - [ ] `GeneralSettingsPanel` & `SeoPanel` nối dirty guard khi đổi locale/rời trang; `ReadingSettingsPanel` kiểm tra dirty trên mọi locale trong `pendingDrafts`.
  - [ ] `RestorePanel` báo lỗi inline khi poll thất bại; `ContactPanel`/`SmtpSettingsPanel`/`BackupsPanel`/`ReconcilePanel` có `<AdminAlert tone="error">` inline.
  - [ ] `SeoPanel` tích hợp `MediaPicker` cho OG image; `TemplateLibraryPage` có nút `Customize in Studio`; `CustomOptionsPageView` có CTA tới `Field groups`; `parent_menu: 'top_level'` hiển thị ở top-level; `FieldGroups` tách rõ trạng thái `Active` vs `Withdrawn`; number inputs cho phép xóa trắng khi đang gõ; bỏ `Reload list` thừa ở `#/plugins/install`, tránh `listUsers()` ở `#/users/new`, đồng nhất format ngày ở `EntryListPage`, `PatternsPage` nút Delete dùng `btn-danger`.
- **Rollback:** `git revert` commit `S04` trên `feat/admin-ui-ux-polish`.

### Slice S05: Dọn i18n, Custom File Upload, Kiểm chứng Tầng 1 & Drift Audit

- **Dependencies:** `S04`.
- **File Ownership:** `components/UploadFileButton.tsx`, `LanguagePacksPanel.tsx`, `MediaPicker.tsx`, `ThemesPanel.tsx`, `PluginsPanel.tsx`, `MediaDetailsPanel.tsx`, `SingleEditTranslationModal.tsx`, `SingleEditTranslationsPanel.tsx`, `MediaLibraryPage.tsx`, `TaxonomyDefinitionsPage.tsx`, `TermEditPage.tsx`, `EntryListPage.tsx`, `PatternsPage.tsx`, `StudioCustomizerNav.tsx`, `cms/l10n/{opendash.pot,vi.po}`, `docs/designs/SITEMAP.md`.
- **Acceptance Criteria:**
  - [ ] ~20 chỗ ghép chuỗi `__(...)` thủ công được chuyển sang template biến `{{value1}}`; mọi input file upload chính dùng `UploadFileButton` theo ngôn ngữ CMS.
  - [ ] Cập nhật đầy đủ `cms/l10n/opendash.pot` & `cms/l10n/vi.po`; `node cms/scripts/check-i18n.mjs --strict` và `run-tests.py --group i18n` PASS.
  - [ ] Toàn bộ kiểm chứng Tầng 1 & kiểm tra drift trước khi mở PR đều PASS.
- **Rollback:** `git revert` commit `S05` trên `feat/admin-ui-ux-polish`.

---

### Verification Plan

- **Tầng 0 (giữa `S01`–`S04`):**
  ```bash
  npm --prefix cms/admin run typecheck
  ```
- **Tầng 1 (1 lần duy nhất sau `S05`, trước Bước 6):**
  ```bash
  python3 cms/scripts/check-standards.py && python3 cms/scripts/check-standards.py --self-test
  python3 cms/scripts/check-plans.py && python3 cms/scripts/check-plans.py --self-test
  python3 .agents/scripts/check-agent-artifacts.py
  cargo fmt --manifest-path cms/Cargo.toml --all --check
  npm --prefix cms/admin run build
  node cms/scripts/check-i18n.mjs --strict
  npm --prefix cms/admin run test:nav
  npm --prefix cms/admin run test:fields
  npm --prefix cms/admin run test:media-preview
  node cms/admin/scripts/test-studio-ux.mjs
  python3 cms/scripts/run-tests.py --changed
  git diff --check
  ```
- **Kiểm tra Drift & Xác minh Live:** Rà soát đối chiếu từng mục `S01..S05` với diff thực tế, sau đó mở 1 Feature PR duy nhất (`feat/admin-ui-ux-polish`).
