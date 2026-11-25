# Feature modules (`cms/admin/src/features/`)

Các phân hệ giao diện của React Admin sử dụng chung HTTP client [`src/api/client.ts`](../api/client.ts), [`AdminPrimitives.tsx`](../components/AdminPrimitives.tsx) và [`FormPrimitives.tsx`](../components/FormPrimitives.tsx):

- `dashboard/`: `DashboardPage.tsx` — Tổng quan và lối tắt quản trị.
- `content/`: `EntryListPage.tsx`, `ContentTypeForm.tsx`, `ContentTypeSchemaSettings.tsx`, `FieldGroupsManager.tsx`, `EditorLayoutConfigurator.tsx`, `FieldValueInput.tsx` — Bảng danh sách entry, cấu hình CPT, nhóm custom fields và bố cục Single Edit.
- `editor/`: `SingleEditWorkspace.tsx`, `EditorDraftStore.ts`, `PanelRegionRenderer.tsx` — Màn hình chỉnh sửa đơn nhất theo vùng (`header_bar`, `main_top`, `main_primary`, `main_bottom`, `sidebar`) cho `post`, `page`, CPT và bảo toàn draft khi `401`/`412`.
- `patterns/`: `PatternsPage.tsx` — Quản lý private Pattern CPT (đồng bộ qua `core/pattern_ref`).
- `media/`: `MediaLibraryPage.tsx`, `MediaPicker.tsx` — Thư viện media (upload ảnh JPEG/PNG/WebP và video MP4 độc lập, picker single/gallery).
- `taxonomy/`: `TaxonomyDefinitionsPage.tsx`, `TaxonomyPage.tsx`, `TermEditPage.tsx`, `TermPicker.tsx` — Quản lý custom taxonomy definitions, chuyên mục, thẻ và term picker.
- `themes/`: `ThemesPanel.tsx`, `ThemeOptionsForm.tsx` — Cài đặt Theme v3 ZIP, kích hoạt revision, Theme Options (hiển thị trong OpenDash Studio) và selective sample import.
- `library/`: `TemplateLibraryPage.tsx` — Thư viện mẫu cục bộ, Patterns và quản lý site template overrides.
- `plugins/`: `PluginsPanel.tsx`, `DeclarativePluginUi.tsx` — Cài plugin ZIP, bật/tắt, cài đặt plugin, quyền và UI khai báo.
- `extensions/`: `AdminContributionRenderer.tsx` — Hiển thị columns, row actions và term panels do theme/plugin đăng ký.
- `users/`: `UsersPanel.tsx`, `UserProfilePanel.tsx` — Quản lý tài khoản Admin/Editor và hồ sơ cá nhân.
- `contact/`: `ContactPanel.tsx`, `SmtpSettingsPanel.tsx` — Form liên hệ, submissions và cấu hình SMTP.
- `seo/`: `SeoPanel.tsx` — Cấu hình SEO website (`robots`, OG, canonical).
- `settings/`: `PermalinkSettingsPage.tsx`, `ReadingSettingsPanel.tsx`, `MultilingualSettingsPage.tsx` — Cấu hình đường dẫn tĩnh, Reading (Homepage/Posts Page) và Đa ngôn ngữ.
- `operations/`: `BackupToolsPanel.tsx`, `BackupsPanel.tsx`, `RestorePanel.tsx`, `ReconcilePanel.tsx` — Sao lưu mã hóa, phục hồi sang site root mới và đối soát/xóa media.
- `system/`: `GeneralSettingsPanel.tsx`, `SystemStatus.tsx` — Cài đặt tổng quát (bao gồm `cms_locale`) và tình trạng hệ thống.

(OpenDash Studio nằm tại `src/studio/`, các thành phần dựng khối dùng chung nằm tại `src/builder/`, khung điều hướng và định tuyến nằm tại `src/shell/`).
