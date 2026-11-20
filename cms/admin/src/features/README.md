# Feature modules (`cms/admin/src/features/`)

Các phân hệ giao diện của React Admin sử dụng chung HTTP client [`src/api/client.ts`](../api/client.ts) và [`FormPrimitives.tsx`](../components/FormPrimitives.tsx):

- `dashboard/`: `DashboardPage.tsx` — Tổng quan và lối tắt quản trị.
- `posts/`: `PostPageWorkspace.tsx` — Quản lý Bài viết (`post`) và Trang (`page`).
- `content/`: `ContentWorkspace.tsx`, `EntryListPage.tsx`, `ContentTypeForm.tsx`, `ContentTypeSchemaSettings.tsx`, `FieldGroupsManager.tsx`, `EditorLayoutConfigurator.tsx`, `FieldValueInput.tsx` — Bảng danh sách entry, cấu hình CPT, nhóm custom fields và bố cục Single Edit.
- `editor/`: `SingleEditWorkspace.tsx`, `EditorDraftStore.ts`, `PanelRegionRenderer.tsx` — Màn hình chỉnh sửa đơn nhất theo vùng (`header_bar`, `main_top`, `main_primary`, `main_bottom`, `sidebar`) và bảo toàn draft khi `401`/`412`.
- `sections/`: `SectionsPage.tsx` — Quản lý private Section CPT.
- `media/`: `MediaLibraryPage.tsx`, `MediaPicker.tsx` — Thư viện media (upload ảnh JPEG/PNG/WebP và video MP4 độc lập, picker single/gallery).
- `taxonomy/`: `TaxonomyDefinitionsPage.tsx`, `TaxonomyPage.tsx`, `TermEditPage.tsx`, `TermPicker.tsx` — Quản lý custom taxonomy definitions, chuyên mục, thẻ và term picker.
- `themes/`: `ThemesPanel.tsx`, `ThemeOptionsForm.tsx` — Cài đặt Theme ZIP v2, kích hoạt, Theme Options và selective sample import.
- `menus/`: `MenusPanel.tsx` — Quản lý menu đa cấp và gán menu slots theo theme.
- `library/`: `TemplateLibraryPage.tsx` — Thư viện mẫu cục bộ và chèn Section vào thiết kế.
- `plugins/`: `PluginsPanel.tsx`, `DeclarativePluginUi.tsx` — Cài plugin ZIP, bật/tắt, quyền và UI khai báo.
- `extensions/`: `AdminContributionRenderer.tsx` — Hiển thị columns, row actions và term panels do theme/plugin đăng ký.
- `users/`: `UsersPanel.tsx`, `UserProfilePanel.tsx` — Quản lý tài khoản Admin/Editor và hồ sơ cá nhân.
- `contact/`: `ContactPanel.tsx`, `SmtpSettingsPanel.tsx` — Form liên hệ, submissions và cấu hình SMTP.
- `seo/`: `SeoPanel.tsx` — Cấu hình SEO website (`robots`, OG, canonical).
- `settings/`: `PermalinkSettingsPage.tsx` — Cấu hình cấu trúc đường dẫn tĩnh (permalinks) và rewrite base.
- `operations/`: `BackupsPanel.tsx`, `RestorePanel.tsx`, `ReconcilePanel.tsx` — Sao lưu mã hóa, phục hồi sang site root mới và đối soát/xóa media.
- `system/`: `GeneralSettingsPanel.tsx`, `SystemStatus.tsx` — Cài đặt tổng quát và tình trạng hệ thống.

(Không gian làm việc Visual Builder nằm tại `src/builder/`, khung điều hướng và định tuyến nằm tại `src/shell/`).
