# M7 — G3: Admin Navigation, CPT Schema Builder, Panel Registry & Composite Single Edit (8 task / 5 slice)

**Coordinator / Final Decision:** Atlas · **G3 Author:** Nova · **Date:** 2026-11-16
**Baseline:** `main@2808e3ecd180acc20379065d30f7711502e2b158` (migration cao nhất hiện tại: `0033_media_lifecycle.sql`).
**G2:** `AGREED` ([PR #270 receipt](https://github.com/thientran2359/opendash-cms/pull/270#issuecomment-6903576673) · [Issue #268](https://github.com/thientran2359/opendash-cms/issues/268)) · **G3:** `READY` (có hiệu lực sau khi PR G3 này được review và merge vào `main`).
**Implementation:** `NOT_STARTED` · **Verification:** `NOT_RUN` · **Project:** Chưa phát hành GitHub Issue hay cấp `Project Ready` từ tài liệu này.
**Nguồn:** [Product Requirements](../../../../../product/PRODUCT_REQUIREMENTS.md) · [Admin Navigation & Edit Screen](../../../../../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md) · [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md) · [Contract Admin Navigation & Single Edit](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) · [Core Foundation](../../../../../contracts/CORE_FOUNDATION.md) · [Plugin ABI](../../../../../contracts/PLUGIN_ABI.md) · [Planning Workflow](../../../PLANNING_WORKFLOW.md) · [Slice Task Template](../../../templates/SLICE_TASK.md).

---

## 1. Inventory Màn hình / Route Hiện tại (`main@2808e3e`) và Phân loại Tái tổ chức UI vs Chức năng Mới

Bảng dưới đây đối chiếu toàn bộ 10 menu chính và các submenu mục tiêu trong [Admin Navigation & Edit Screen](../../../../../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md#2-menu-chính-và-submenu) với mã nguồn thực tế tại `main@2808e3ecd180acc20379065d30f7711502e2b158`, phân biệt rõ phần **Tái tổ chức UI (`UI_REORG`)** trên API sẵn có với phần **Chức năng / API / Schema mới (`NEW_BACKEND_AND_UI`)**:

| Menu chính & Submenu mục tiêu | Hash Route chuẩn | Quyền tối thiểu | Màn hình / Component hiện có (`cms/admin/src/`) | API / Storage hiện có (`cms/crates/opendash-server/src/`) | Phân loại & Khoảng thiếu cần triển khai |
| --- | --- | --- | --- | --- | --- |
| **1. Tổng quan** → Dashboard | `#/dashboard` | `Editor` | `features/dashboard/DashboardWorkspace.tsx` (`#/dashboard`) | `GET /api/admin/dashboard` (`routes/dashboard.rs`) | **`UI_REORG`:** Đã có màn hình và API; cập nhật lối tắt nhanh dẫn tới các route chuẩn mới (`#/posts/new`, `#/pages/new`, `#/settings/content-types` cho Admin). |
| **2. Bài viết** → Tất cả bài viết · Thêm mới | `#/posts` · `#/posts/new` · `#/posts/:id/edit` | `Editor` | `features/posts/PostPageWorkspace.tsx` (hiện gộp danh sách và form sửa trên cùng màn hình `#/posts`) | `GET/POST /api/admin/content-types/post/entries`, `GET/PUT/DELETE .../entries/{id}`, `POST .../publication`, `GET/PUT .../route`, `GET/PUT .../seo` | **`NEW_BACKEND_AND_UI`:** Tách màn hình danh sách (`#/posts`) khỏi màn hình Single Edit (`#/posts/new`, `#/posts/:id/edit`); dựng Single Edit 5 vùng theo `effective-editor-config` và lưu nguyên tử qua `CompositeEntrySaveInput`. |
| **2. Bài viết** → Chuyên mục · Thẻ | `#/posts/categories` · `#/posts/tags` | `Editor` | `features/taxonomy/TaxonomyWorkspace.tsx` (`#/taxonomy`) | `GET/POST/PUT/DELETE /api/admin/taxonomies/{kind}/terms` (`routes/taxonomy.rs`) | **`UI_REORG`:** Gắn submenu `Chuyên mục` (`kind=category`) và `Thẻ` (`kind=tag`) dưới menu `Bài viết` (và hiển thị trên các CPT có đăng ký taxonomy dùng chung). |
| **3. Trang** → Tất cả trang · Thêm mới | `#/pages` · `#/pages/new` · `#/pages/:id/edit` | `Editor` | `features/posts/PostPageWorkspace.tsx` (`#/pages`) | `/api/admin/content-types/page/entries...` | **`NEW_BACKEND_AND_UI`:** Tách danh sách (`#/pages`) và Single Edit (`#/pages/new`, `#/pages/:id/edit`) có nút mở Builder kèm cảnh báo phạm vi và bảo toàn nháp. |
| **Menu động cho từng CPT** (`Dự án`, `Dịch vụ`, v.v.) → Tất cả · Thêm mới · Taxonomy gắn kèm | `#/cpt/:typeKey` · `#/cpt/:typeKey/new` · `#/cpt/:typeKey/:id/edit` | `Editor` | Hiện đang nằm chung trong `features/content/ContentWorkspace.tsx` (`#/content`) | `/api/admin/content-types/{type_key}/entries...` | **`NEW_BACKEND_AND_UI`:** Mỗi CPT có một mục menu riêng trên Sidebar cho `Editor` + `Admin`; tách hẳn CRUD entry của CPT ra khỏi màn hình cấu hình schema (`F1`). Vẫn quản lý được khi provider `provider_inactive` hoặc `withdrawn` (`R1`). |
| **4. Thư viện** → Tất cả media · Tải lên | `#/media` · `#/media/upload` | `Editor` | `features/media/MediaWorkspace.tsx` (`#/media`) | `GET/POST /api/admin/media`, `POST /api/admin/media/video`, `DELETE /api/admin/media/{id}` | **`UI_REORG`:** Bổ sung hash route `#/media/upload` mở thẳng khu vực tải lên và giữ nguyên `MediaWorkspace`. |
| **5. Giao diện** → Themes · Tùy chỉnh theme · Menus · Templates · Builder | `#/appearance/themes` · `#/appearance/customize` · `#/appearance/menus` · `#/appearance/templates` · `#/builder` | `Admin` (riêng `#/builder` có `entryId` cụ thể cho phép `Editor`) | `ThemesWorkspace.tsx`, `ThemeOptionsWorkspace.tsx`, `MenuWorkspace.tsx`, `TemplatesWorkspace.tsx`, `BuilderWorkspace.tsx` | `routes/themes.rs`, `routes/theme_options.rs`, `routes/menu.rs`, `routes/overrides.rs`, `routes/builder.rs` | **`UI_REORG` + Guard RBAC (`F7`):** Gom vào menu cha `Giao diện`; khóa chặt RBAC cả client và server để template dùng chung toàn site (`header`, `footer`, `archive`, `taxonomy_archive`, `single`/`page` không có `entryId`) chỉ cho phép `Admin`, còn `Editor` chỉ được sửa/reset override gắn với `entryId` cụ thể. |
| **6. Liên hệ** → Form liên hệ · Tin nhắn nhận được · Cấu hình email | `#/contact/forms` · `#/contact/submissions` · `#/contact/smtp` | `Editor` xem `submissions`; `Admin` cấu hình `forms` & `smtp` | `features/contact/ContactWorkspace.tsx` (`#/contact`) | `routes/contact.rs`, `routes/smtp.rs` | **`UI_REORG`:** Tách 3 submenu với hash URL riêng; ẩn `Form liên hệ` và `Cấu hình email` đối với `Editor` và chặn `403` khi truy cập URL trực tiếp. |
| **7. Plugins** → Plugins đã cài · Cài từ ZIP | `#/plugins` · `#/plugins/install` | `Admin` | `features/plugins/PluginsWorkspace.tsx` (`#/plugins`) | `routes/plugin.rs` | **`UI_REORG` + Mở rộng Manifest (`270-R3`, `270-R9`):** Tách submenu `#/plugins/install`; mở rộng bộ đọc manifest hỗ trợ `field_groups`, `editor_panels` (`ManifestActionPanelInput`) và `ScopedFieldGrant` (`content_read_private` / `content_write_private`). |
| **8. Người dùng** → Tất cả người dùng · Thêm mới · Hồ sơ của tôi | `#/users` · `#/users/new` · `#/users/profile` | `#/users` & `#/users/new`: `Admin`; `#/users/profile`: `Editor` + `Admin` | `features/users/UsersWorkspace.tsx` (`#/users`) | `routes/users.rs` (`GET/POST /api/admin/users`, `PATCH/DELETE /api/admin/users/{id}`, `POST .../password`) | **`NEW_BACKEND_AND_UI` (`F7`):** Hiện `routes/users.rs` khóa `require_admin` toàn bộ; cần bổ sung `GET/PUT /api/admin/profile` (cho `Editor` + `Admin` tự sửa `display_name` và đổi mật khẩu của chính mình khi gửi đúng `current_password`) và màn hình `#/users/profile`. |
| **9. Công cụ** → Sao lưu · Phục hồi · Đối soát media · Tình trạng hệ thống | `#/tools/backup` · `#/tools/restore` · `#/tools/media-reconcile` · `#/tools/health` | `Admin` | `features/operations/OperationsWorkspace.tsx` (`#/operations`) | `routes/operations.rs`, `routes/media_lifecycle.rs`, `/api/health` | **`UI_REORG`:** Ánh xạ các tab trong `OperationsWorkspace` sang 4 submenu dưới `Công cụ` với hash URL riêng (`Admin`-only). |
| **10. Cài đặt** → Tổng quát · Đường dẫn · SEO website · **Loại nội dung & Trường** | `#/settings/general` · `#/settings/permalinks` · `#/settings/seo` · **`#/settings/content-types`** · **`#/settings/content-types/:typeKey`** | `Admin` | `GeneralSettingsPanel.tsx`, `SeoPanel.tsx`, `ContentTypeForm.tsx` | `routes/settings.rs`, `routes/seo.rs`, `routes/content.rs` (`GET/POST /api/admin/content-types`) | **`NEW_BACKEND_AND_UI` (`F1`, `F2`, `F3`, `270-R1`..`R9`):** Đặt cố định `Loại nội dung & Trường` dưới `Cài đặt` (`Admin`-only, luôn hiển thị kể cả khi có `0` CPT, **tuyệt đối không có bảng/form nhập entry**). Bổ sung migration `0034`, `PUT /api/admin/content-types/{type_key}`, CRUD `/api/admin/content-field-groups`, và `GET/PUT .../editor-layout`. |

---

## 2. Thiết kế UI / Wireframe Mockups (Desktop & Mobile 390px) và Ma trận Trạng thái

### 2.1. Màn hình Danh sách Nội dung (`#/posts`, `#/pages`, `#/cpt/:typeKey`)

```text
┌────────────────────────┬──────────────────────────────────────────────────────────────────────┐
│ OPENDASH ADMIN    [≡]  │ Bài viết / Tất cả bài viết                        [+ Thêm bài viết] │
├────────────────────────┼──────────────────────────────────────────────────────────────────────┤
│ ▸ Tổng quan            │ Bộ lọc: [Tất cả trạng thái ▾] [Chuyên mục ▾] [Thẻ ▾]     [Lọc]      │
│ ▾ Bài viết             │ ┌────────────────────────────┬────────────┬───────────┬────────────┐ │
│   • Tất cả bài viết ●  │ │ Tiêu đề / Đường dẫn        │ Phân loại  │ Trạng thái│ Hành động  │ │
│   • Thêm mới           │ ├────────────────────────────┼────────────┼───────────┼────────────┤ │
│   • Chuyên mục         │ │ Thiết kế nội thất gỗ óc chó│ Kiến trúc  │ Đã xuất   │ [Chỉnh sửa]│ │
│   • Thẻ                │ │ /content/post/thiet-ke-go  │            │ bản (v3)  │ [Xem] [Xóa]│ │
│ ▸ Trang                │ ├────────────────────────────┼────────────┼───────────┼────────────┤ │
│ ▾ Dự án (CPT)          │ │ Xu hướng tối giản 2026     │ Nội thất   │ Bản nháp  │ [Chỉnh sửa]│ │
│   • Tất cả dự án       │ │ /content/post/xu-huong     │            │ (v1)      │ [Xem trước]│ │
│   • Thêm dự án mới     │ └────────────────────────────┴────────────┴───────────┴────────────┘ │
│ ▸ Thư viện             │ Trang 1 / 3                                       [‹ Trước] [Sau ›]  │
│ ▸ Giao diện            │                                                                      │
│ ▸ Liên hệ              │                                                                      │
│ ▸ Plugins              │                                                                      │
│ ▸ Người dùng           │                                                                      │
│ ▸ Công cụ              │                                                                      │
│ ▸ Cài đặt              │                                                                      │
└────────────────────────┴──────────────────────────────────────────────────────────────────────┘
```

### 2.2. Màn hình Single Edit cho Post / Page / CPT (`#/posts/:id/edit`, `#/pages/:id/edit`, `#/cpt/:typeKey/:id/edit`)

- **Desktop (`>= 1024px` — bố cục 2 cột với Sticky `header_bar`):**

```text
┌───────────────────────────────────────────────────────────────────────────────────────────────┐
│ [header_bar] Dự án › Chỉnh sửa: Biệt thự Thảo Điền   [● Chưa lưu]  [Xem trước]  [Lưu (v4)]    │
├──────────────────────────────────────────────────────┬────────────────────────────────────────┤
│ [main_top]                                           │ [sidebar]                              │
│ ┌─ Thông báo / Nhóm trường ưu tiên (Collapsible) ──┐ │ ┌─ Xuất bản & Trạng thái (core) ─────┐ │
│ │  • Mã hồ sơ: [BT-2026-01            ]            │ │ │ Trạng thái: Đã xuất bản            │ │
│ └──────────────────────────────────────────────────┘ │ │ Phiên bản: v4 · Schema: sha256:8f… │ │
│ [main_primary]                                       │ │ [Lưu thay đổi]                     │ │
│ ┌─ Tiêu đề & Đường dẫn (core:title_slug) ──────────┐ │ └────────────────────────────────────┘ │
│ │  Tiêu đề *: [Biệt thự Thảo Điền                ] │ │ ┌─ Thuộc tính / Template ────────────┐ │
│ │  Slug *:    [biet-thu-thao-dien                ] │ │ │ Template đang áp dụng: single      │ │
│ │  URL riêng: [/du-an/biet-thu-thao-dien         ] │ │ │ [Mở Builder cho bản ghi này]       │ │
│ └──────────────────────────────────────────────────┘ │ └────────────────────────────────────┘ │
│ ┌─ Nội dung chính / Mở Builder (core:body_editor) ─┐ │ ┌─ Ảnh đại diện & Phân loại ─────────┐ │
│ │  [Soạn thảo RichText hoặc mở Visual Builder]     │ │ │ [Chọn ảnh đại diện]                │ │
│ └──────────────────────────────────────────────────┘ │ │ Chuyên mục: [x] Biệt thự           │ │
│ [main_bottom]                                        │ └────────────────────────────────────┘ │
│ ┌─ Thông tin dự án (layout:project_info) ────── [▾]┐ │ ┌─ Trạng thái dự án (layout:status) ─┐ │
│ │  ─ Subgroup: Thông số kỹ thuật ─                 │ │ │ Tiến độ *: [Đang thi công ▾]       │ │
│ │  Diện tích (m2): [450]   Khách hàng: [Anh Minh]  │ │ │ Năm hoàn thành: [2026]             │ │
│ └──────────────────────────────────────────────────┘ │ └────────────────────────────────────┘ │
│ ┌─ Cấu hình SEO (core:seo) ──────────────────── [▾]┐ │ ┌─ Đồng bộ CRM (plugin action) ── [▾]┐ │
│ │  SEO Title, Meta Description, OG Image, Noindex  │ │ │ Hành động độc lập (không tự chạy   │ │
│ └──────────────────────────────────────────────────┘ │ │ khi bấm Lưu): [Gửi thử sang CRM]   │ │
│                                                      │ └────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┴────────────────────────────────────────┘
```

- **Mobile (`< 1024px` — bố cục 1 cột):**
  - Sidebar điều hướng trái chuyển thành ngăn kéo (drawer) đóng/mở bằng nút `[≡]` có `aria-expanded` và bẫy tiêu điểm (focus trap) khi mở.
  - Thứ tự DOM và thứ tự đọc từ trên xuống dưới thống nhất tuyệt đối:
    **`header_bar` (sticky) → `main_top` → `main_primary` → `main_bottom` → `sidebar`**.

### 2.3. Màn hình `Cài đặt → Loại nội dung & Trường` (`#/settings/content-types` và `#/settings/content-types/:typeKey`)

- **Ràng buộc sản phẩm (`F1`):** Chỉ `Admin` mới thấy và truy cập được; luôn hiển thị dưới menu `Cài đặt` kể cả khi hệ thống có `0` CPT; **tuyệt đối không có bảng danh sách entry hay form nhập nội dung entry**.

```text
┌───────────────────────────────────────────────────────────────────────────────────────────────┐
│ Cài đặt › Loại nội dung & Trường › Cấu hình: Dự án (portfolio · v2)    [Lưu cấu hình bố cục] │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ Tab: [1. Định nghĩa CPT & Base Fields]  [2. Nhóm Custom Fields mở rộng]  [3. Bố cục Panel ●] │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ A. Chia nhóm Base Fields (base_field_panels — không đổi field key hay dữ liệu đã lưu):       │
│   • layout:project_info ("Thông tin dự án") → Vùng: [main_bottom ▾] Thứ tự: [10]             │
│       Trường: [area_m2, client_name] · Subgroup: "Thông số kỹ thuật"                          │
│   • layout:project_status ("Trạng thái dự án") → Vùng: [sidebar ▾] Thứ tự: [20]              │
│       Trường: [progress_stage *, completion_year] (Chứa trường bắt buộc -> Không thể ẩn)     │
│                                                                                               │
│ B. Sắp xếp các Panel theo 4 vùng (main_top / main_primary / main_bottom / sidebar):          │
│   • Vùng main_top:    [x] user:project_banner (Thứ tự: 10, Thu gọn mặc định: Không)          │
│   • Vùng main_primary:[x] core:title_slug (Khóa Core) · [x] core:base_fields (Fallback)      │
│   • Vùng main_bottom: [x] layout:project_info (10) · [x] core:seo (90)                       │
│   • Vùng sidebar:     [x] core:publish_box (Khóa Core) · [x] layout:project_status (20)      │
│                       [x] plugin:crm-sync:sync_crm_panel (Action · 80)                       │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.4. Ma trận Trạng thái UI (Dirty, Validation, `401`, `403`, `409`, `412`, `provider_inactive` / `withdrawn`)

| Trạng thái | Hành vi bắt buộc trên UI & Client State |
| --- | --- |
| **Dirty State (`EditorDraftStore`)** | Khi người dùng sửa bất kỳ trường nào trong core entry, custom fields, taxonomy, `custom_path`, hoặc SEO: hiện badge `Chưa lưu thay đổi` trên `header_bar`; chuyển hash route trong ứng dụng vẫn giữ bản nháp trong RAM (`EditorDraftStore`); đóng tab trình duyệt kích hoạt `beforeunload`. |
| **Lỗi Validation trong Panel đang thu gọn** | Khi bấm Lưu mà một trường (ví dụ trong panel ở `main_bottom` hoặc `sidebar` đang `collapsed`) bị lỗi validation (hoặc bị veto từ `content/before_save` kèm `field_errors`): tự động mở rộng (`collapsed = false`) panel chứa trường lỗi đầu tiên, hiển thị badge số lỗi trên tiêu đề panel, và cuộn/đặt focus tới đúng ô input bị lỗi. |
| **Mất phiên `401 Unauthorized` (`R5`)** | Tăng `session_epoch` (hủy mọi response trễ từ phiên cũ), chuyển toàn bộ bản nháp trong `EditorDraftStore` sang `status = 'quarantined'`, và **unmount/ẩn cây DOM của form chỉnh sửa đằng sau màn hình yêu cầu đăng nhập lại**. Sau khi đăng nhập lại, kiểm tra `(site_instance_id, user.id)` và tải mới `GET .../effective-editor-config` để xóa mọi trường mất quyền xem (`caller.role < min_view_role`) và đặt lại trường mất quyền sửa trước khi hiển thị lại nháp (không tự động bấm Lưu). |
| **Trường chỉ đọc (`min_view_role <= caller.role < min_edit_role`)** | Trong `EffectiveEditorConfigProjection`, trường có `editable: false` được render ở chế độ disabled/read-only và **client không gửi khóa đó trong `CompositeEntrySaveInput.values`** (`Omitted = Preserve`). |
| **Xung đột `409 schema_version_conflict` hoặc `412 Precondition Failed`** | Giữ nguyên bản nháp người dùng đang nhập trong `EditorDraftStore`; hiển thị thông báo rõ ràng kèm nút tải lại `effective-editor-config` mới (với `409`) hoặc đối chiếu phiên bản nội dung mới (với `412`). |
| **Provider `provider_inactive` hoặc Khai báo `withdrawn` (`R1`, `270-R1`)** | Với trường dữ liệu (`BaseFieldsSlice`, `core:base_fields`, `ContentFields`): **vẫn dựng đầy đủ các ô nhập liệu qua Core Standard Field Fallback Form** để `Editor`/`Admin` tiếp tục xem và sửa dữ liệu, kèm nhãn thông tin `(Provider đang tắt)` hoặc `(Đã rút khai báo)`. Với `PanelKind::Action` của plugin đang tắt: nút bấm bị vô hiệu hóa (`disabled`) kèm nhãn `(Plugin đang tắt)`. |

---

## 3. Phân bổ Ma trận Nghiệm thu (`AC-01`–`AC-11`), Chuỗi Phụ thuộc và Quyền Sở hữu File

### 3.1. Bảng ánh xạ `AC-01`–`AC-11` sang 5 Slice / 8 Task G3

| Slice | Mã Task G3 | Module / Capability | Tiêu chí Nghiệm thu từ Contract (`AC-01`–`AC-11`) | Phụ thuộc chặn trước khi nhận Task | Owner dự kiến |
| --- | --- | --- | --- | --- | --- |
| **`M7-S09` (`S-SCHEMA`)** | **`M7-S09-T01`** | `CNT-01` / `CNT-01.3` | `AC-01`, `AC-02`, `AC-03(a,b,d,e,f)`, `AC-05(a)` | Contract #270 đã merge (`2808e3e`) | Nova |
| **`M7-S09` (`S-SCHEMA`)** | **`M7-S09-T02`** | `CNT-02` / `CNT-02.3` | `AC-05(a,b,c)`, `AC-06(a,b,c)` | `M7-S09-T01` | Nova |
| **`M7-S10` (`S-LAYOUT`)** | **`M7-S10-T01`** | `CNT-02` / `CNT-02.4` | `AC-03(c)`, `AC-04(a,b,c)`, `AC-09(b)` | `M7-S09-T01`, `M7-S09-T02` | Nova |
| **`M7-S11` (`S-SAVE`)** | **`M7-S11-T01`** | `CNT-04` / `CNT-04.3` | `AC-06(d)`, `AC-07(a,b,c)`, `AC-08(a,b)`, `AC-09(c)` | `M7-S10-T01` | Nova |
| **`M7-S12` (`S-NAV-UI` & `S-RECOVERY`)** | **`M7-S12-T01`** | `FND-05` / `FND-05.3` | `AC-04(a,b)`, `AC-09(a,b,c)` | `M7-S11-T01` | Nova |
| **`M7-S12` (`S-NAV-UI` & `S-RECOVERY`)** | **`M7-S12-T02`** | `FND-05` / `FND-05.4` | `AC-01(a,b)`, `AC-04(d)`, `AC-07`, `AC-10`, `AC-11` | `M7-S12-T01` | Nova |
| **`M7-S13` (E2E & Demo)** | **`M7-S13-T01`** | `FND-05` / `FND-05.5` | Toàn bộ `AC-01`–`AC-11` trên Windows + bộ ảnh demo #258 | `M7-S12-T02` | Nova |
| **`M7-S13` (macOS Parity)** | **`M7-S13-T02`** | `FND-05` / `FND-05.5` | Regression đối xứng `AC-01`–`AC-11` trên macOS | `M7-S13-T01` (hoãn theo ADR-0014) | Harbor |

### 3.2. Thứ tự thực hiện an toàn, Migration và Quy tắc Shared Wiring

1. **Chuỗi thực hiện tuần tự không đụng độ file:**
   `M7-S09-T01 → M7-S09-T02 → M7-S10-T01 → M7-S11-T01 → M7-S12-T01 → M7-S12-T02 → M7-S13-T01 (Windows) → M7-S13-T02 (macOS)`.
2. **Migration SQLite:**
   - `M7-S09-T01` sở hữu migration dự kiến `cms/migrations/0034_field_groups_and_editor_layout.sql`. Trước khi tạo file migration, người thực thi phải kiểm tra số thứ tự cao nhất trong `cms/migrations/` trên `main` (tại baseline `2808e3e` là `0033_media_lifecycle.sql`); tuyệt đối không sửa các file `0001`–`0033` đã phát hành.
3. **Shared Wiring Seam:**
   - Các file điều phối dùng chung gồm `cms/crates/opendash-core/src/lib.rs`, `cms/crates/opendash-server/src/app.rs`, `cms/crates/opendash-server/src/routes/mod.rs`, `cms/crates/opendash-server/src/storage/mod.rs`, `cms/admin/src/App.tsx`, `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/api/index.ts`.
   - Chỉ task đang active trong chuỗi tuần tự ở trên mới được phép sửa shared wiring phục vụ đúng slice của mình; không cho phép hai nhánh cùng sửa một file tại cùng thời điểm.
4. **Tái sử dụng Base APIs (`CORE_FOUNDATION.md`):**
   - Bắt buộc dùng chung `opendash_core::id`, `opendash_core::registry`, `HookDispatcher`, `opendash_server::package`, `opendash_server::auth` (`require_session`, `require_session_with_csrf`, `require_admin_with_csrf`), `ApiError`, `AdmittedMutation`, và đặt 100% câu lệnh SQL trong `cms/crates/opendash-server/src/storage/`. Không viết SQL trực tiếp trong `routes/`.

---

## 4. Đặc tả Chi tiết Từng Task G3 (`M7-S09-T01` đến `M7-S13-T02`)

### M7-S09-T01 — Migration 0034, ScopedResourceId, chuẩn hóa FieldDefinitionInput và API quản trị CPT cùng Field Groups

- **Thuộc:** V1 · M7 · M7-S09 · Admin Navigation & Single Edit
- **Module:** CNT-01
- **Capability:** CNT-01.3
- **Primary Acceptance Slice của capability:** M7-S09; M7-S13 là regression E2E toàn phạm vi.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh, chưa phát hành Issue).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 1 và Mục 2](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md).

#### Tổng quan và giá trị

Hiện tại quản trị viên chỉ có thể tạo mới CPT một lần mà chưa thể cập nhật schema sau khi tạo, chưa thể gắn nhóm custom fields mở rộng vào `post`, `page` hay CPT, và khi gỡ một `target_type` hoặc một trường khỏi nhóm thì chưa có cơ chế bảo toàn định nghĩa để giữ kiểm kê media và hiển thị fallback form. Task này xây dựng tầng lưu trữ, định danh có cấu trúc và bộ API quản trị schema/field groups an toàn dữ liệu.

#### Mục tiêu và hành vi quan sát được

Sau khi hoàn thành, Admin có thể cập nhật CPT thuộc `ResourceOwner::User` qua `PUT /api/admin/content-types/{type_key}` có kiểm tra `If-Match: "v{version}"`, tạo/sửa/rút các nhóm custom fields qua `/api/admin/content-field-groups`, đồng thời Theme/Plugin có thể khai báo `field_groups` trong manifest. Mọi thao tác chuẩn hóa `public_exposure` đúng ngữ cảnh (`BaseCptField` mặc định `Public`, `ExtensionGroupField` mặc định `Private`, khai báo tường minh `Public` giữ nguyên `Public`), bảo toàn tombstone ở cả 3 cấp (`group`, `target_type`, `field`) và đảm bảo `Editor` luôn tạo được entry mới.

#### Phạm vi công việc

- Bổ sung migration forward `cms/migrations/0034_field_groups_and_editor_layout.sql` (kiểm tra lại số thứ tự kế tiếp trên `main` khi nhận task): thêm cột `version` vào `content_types`, tạo bảng `content_field_groups`, `content_field_group_targets` và `content_editor_layouts`.
- Cài đặt `ScopedResourceId`, `PublicExposure`, `FieldScopeKind`, `FieldDefinitionInput` và hàm `into_canonical(scope)` trong `opendash_core`, đảm bảo `preflight_builtin_key_collisions` vẫn đạt 100% với `post` và `page`.
- Cài đặt hàm kiểm tra tương thích đối xứng `verify_effective_schema_transition` (`Rule COMPAT-1` đến `Rule COMPAT-4`, bao gồm kiểm tra vô điều kiện khả năng tạo mới của `Editor` độc lập với `has_existing_entries`) và cơ chế tombstone 3 cấp (`status = 'withdrawn'`, `association_state = 'withdrawn'`, `declaration_state = 'withdrawn'`).
- Bổ sung các endpoint `PUT /api/admin/content-types/{type_key}`, `GET /api/admin/content-field-groups`, `POST /api/admin/content-field-groups`, `PUT /api/admin/content-field-groups/{group_id}`, `DELETE /api/admin/content-field-groups/{group_id}` và tích hợp đồng bộ `field_groups` khi kích hoạt Theme/Plugin.

#### Ngoài phạm vi

- Không xây dựng giao diện React trong task này (thuộc `M7-S12-T01`).
- Không thực hiện xóa vĩnh viễn dữ liệu (destructive purge) các trường đã rút khai báo.

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc kỹ thuật: Contract [ADMIN_NAVIGATION_AND_SINGLE_EDIT.md](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) đã merge tại `main@2808e3ecd180acc20379065d30f7711502e2b158`.
- Điều kiện trước khi nhận task: đặc tả G3 này đã merge vào `main`, có GitHub Issue và trạng thái Project Ready được xác nhận, nhánh của agent sạch và không có task đang chạy song song.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/migrations/0034_field_groups_and_editor_layout.sql`
  - `cms/crates/opendash-core/src/id.rs`
  - `cms/crates/opendash-core/src/content.rs`
  - `cms/crates/opendash-core/src/plugin.rs`
  - `cms/crates/opendash-server/src/theme.rs`
  - `cms/crates/opendash-server/src/storage/content.rs`
  - `cms/crates/opendash-server/src/storage/media.rs`
  - `cms/crates/opendash-server/src/routes/content.rs`
  - `cms/crates/opendash-server/tests/m7_s09_t01_schema_field_groups.rs`
  - `docs/verification/2026-11-16-m7-s09-t01-g5.md`
- Shared wiring được phép chạm: `cms/crates/opendash-core/src/lib.rs`, `cms/crates/opendash-server/src/app.rs`, `cms/crates/opendash-server/src/routes/mod.rs`, `cms/crates/opendash-server/src/storage/mod.rs`.

#### Cách triển khai theo contract

Tái sử dụng `require_admin_with_csrf`, `require_session`, `AdmittedMutation` và `ApiError`. Toàn bộ truy vấn đọc/ghi `content_types`, `content_field_groups` và `content_field_group_targets` đặt trong `storage::content` bên trong giao dịch `BEGIN IMMEDIATE`. Khi ghi `ContentFieldDefinition` xuống JSON trong SQLite, luôn serialize tường minh `public_exposure`, `min_view_role`, `min_edit_role` và `declaration_state` sau khi chạy `into_canonical(scope)`. Cập nhật `M8_WRITER_INVENTORY.md` nếu bổ sung đường ghi mới có `AdmittedMutation`.

#### Nghiệm thu

- [ ] Migration `0034` chạy thành công trên cả DB mới (fresh install) và bản sao DB nâng cấp từ `0033`, giữ nguyên toàn bộ `content_types`, `content_entries` và vượt qua `preflight_builtin_key_collisions` cho `post`/`page`.
- [ ] Chuẩn hóa `FieldDefinitionInput` hoạt động đúng cho cả 3 trường hợp qua User API và Manifest (base field thiếu `public_exposure` lưu thành `public`; extension group field thiếu `public_exposure` lưu thành `private`; extension group field khai báo tường minh `public` lưu thành `public`) và giữ nguyên sau khi khởi động lại server (`AC-05a`).
- [ ] Bảo toàn tombstone 3 cấp (`AC-01b`): xóa group (`DELETE`), bỏ một `type_key` khỏi `target_types` khi đã có dữ liệu, hoặc rút một field khỏi package manifest đều chuyển sang trạng thái `withdrawn` thay vì mất định nghĩa; User `PUT` thử xóa hẳn một `field.key` khi đã có entry bị chặn `409 cannot_remove_field_with_data`; đối soát media (`collect_pins_in_tx`) không báo orphan cho ảnh thuộc trường/target `withdrawn`.
- [ ] Kiểm tra tương thích đối xứng trên cả 3 hướng (`AC-02`) và bất biến `Rule COMPAT-4` ở mọi transition (`AC-03a,b,d,e,f`): chặn `409 field_key_collision` khi trùng khóa; chặn `409 incompatible_schema_change` khi biến trường optional thành `required: true` lúc đã có entry; chặn `422 admin_required_field_missing_default` khi đổi `min_edit_role` từ `Editor` sang `Admin` trên trường `required: true` hiện hữu mà không kèm `default_value` hoặc khi xóa `default_value` sau khi đã có entry.
- [ ] Có báo cáo kiểm chứng thực chạy ghi rõ SHA, môi trường, lệnh và kết quả.

#### Kiểm thử, rủi ro và rollback

- Chạy `cargo test --manifest-path cms/Cargo.toml --test m7_s09_t01_schema_field_groups --locked` và bộ regression `content_types_smoke.py`, `content_entries_smoke.py` cùng các script governance (`check-plans.py`, `check-standards.py`, `check-agent-artifacts.py`, `git diff --check`).
- Kiểm tra nâng cấp trên bản sao DB `0033` và xác minh rollback bằng cách khôi phục bản sao DB + thư mục site cũ với binary trước migration.

#### Bàn giao và review

- Ghi nhận diff, lệnh kiểm thử, kết quả và báo cáo tại `docs/verification/2026-11-16-m7-s09-t01-g5.md`.

---

### M7-S09-T02 — Quyền public_exposure trên Public, Preview, Media và ScopedFieldGrant cho Plugin

- **Thuộc:** V1 · M7 · M7-S09 · Admin Navigation & Single Edit
- **Module:** CNT-02
- **Capability:** CNT-02.3
- **Primary Acceptance Slice của capability:** M7-S09; M7-S13 là regression E2E toàn phạm vi.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S09-T01`).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 1.2 và Mục 4.4](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md).

#### Tổng quan và giá trị

Sau khi có các trường mở rộng với `public_exposure = Private` và `min_view_role` / `min_edit_role`, hệ thống cần đảm bảo các trường riêng tư hoặc ảnh đính kèm chỉ dùng nội bộ không bị lộ ra trang công khai, template binding, `/media/{id}/file` hay rò rỉ sang plugin thứ ba qua Wasm hooks/actions, đồng thời vẫn giữ nguyên toàn bộ 7 nguồn media công khai hợp lệ hiện có (như logo theme, ảnh OG trang chủ, Builder homepage và shared template overrides).

#### Mục tiêu và hành vi quan sát được

Các trang public, MiniJinja view model, Builder `FixedEntry`/`RouteContext` binding và chế độ public preview chỉ hiển thị các trường có `public_exposure == Public`. Hàm `public_content::media_is_referenced` giữ nguyên phép hợp (`OR`) của cả 7 nhánh nguồn công khai hiện hữu và bổ sung kiểm tra `public_exposure = 'public'` cho nhánh custom fields của entry. Plugin `manifest_schema: 2` có thể khai báo `ScopedFieldGrant` (`content_read_private`, `content_write_private`) được kiểm tra chặt chẽ trên cả 4 đường giao tiếp Host và Plugin mà không làm hạ quyền của `Admin` khi gọi action có `required_role = "editor"`.

#### Phạm vi công việc

- Cập nhật `storage::public_content::media_is_referenced` để mở rộng Nhánh 1 sang cả `content_types.definition_json` (`COALESCE(public_exposure, 'public') = 'public'`) và `content_field_groups.fields_json` (`COALESCE(public_exposure, 'private') = 'public'`), giữ nguyên 100% các Nhánh 2 đến Nhánh 7 (`builder_homepage_media`, `theme_option_media_refs`, `seo_public_media_refs` cho `entry_seo` và `home_seo`, `builder_homepage_video_refs`, `template_override_media_refs`) và giữ nguyên tính độc lập của `media_refs::collect_pins_in_tx`.
- Lọc bỏ các trường `public_exposure == Private` khỏi view model trên public routes, MiniJinja context, Builder `FixedEntry`/`RouteContext` field binding và public preview.
- Bổ sung `ScopedFieldGrant` (`content_read_private`, `content_write_private`) vào `PluginPermissions` (chỉ hợp lệ khi `manifest_schema == 2` và có `min_host_version`, default deny, cấm wildcard `"*"`) và mở rộng allowlist `PluginActionDeclaration.capabilities`.
- Cài đặt hai hàm `plugin_can_read_field` và `plugin_can_write_field` (tách biệt ngưỡng admission `action.required_role` với quyền thực tế `actor.current_role`) và áp dụng trên `dispatch_content_before_save`, `dispatch_content_after_save`, action context và `HostOperationRequest::ContentSave`.

#### Ngoài phạm vi

- Không thay đổi ABI Wasm cơ bản (`opendash_plugin_v1`) hay cho phép plugin tự sửa đổi `candidate_values` trong `content/before_save`.

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S09-T01` đã merge và có bằng chứng kiểm chứng đạt.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/crates/opendash-core/src/plugin.rs`
  - `cms/crates/opendash-server/src/storage/public_content.rs`
  - `cms/crates/opendash-server/src/storage/builder_binding.rs`
  - `cms/crates/opendash-server/src/content_render.rs`
  - `cms/crates/opendash-server/src/hooks.rs`
  - `cms/crates/opendash-server/src/plugin/manager.rs`
  - `cms/crates/opendash-server/src/plugin/host_ops.rs`
  - `cms/crates/opendash-server/tests/m7_s09_t02_exposure_and_plugin_grants.rs`
  - `docs/verification/2026-11-16-m7-s09-t02-g5.md`

#### Cách triển khai theo contract

Trong `hooks.rs`, tính toán `filtered_candidate_values` riêng cho từng `plugin_id` đăng ký `content/before_save` hoặc `content/after_save` dựa trên `plugin_can_read_field`. Trong `plugin/manager.rs`, kiểm tra `plugin_can_write_field` cho mọi khóa gửi tường minh trong `cmd.values` cả ở bước preflight lẫn bên trong giao dịch `BEGIN IMMEDIATE` sau `verify_plugin_action_fence`; nếu vi phạm trả ngay `403 field_edit_forbidden` mà không đọc hay so sánh giá trị trong DB. Không ghi `values` hay `candidate_values` vào log lỗi.

#### Nghiệm thu

- [ ] Ảnh chỉ gắn với trường `Private` (hoặc bài `draft`) trả `404` trên `GET /media/{id}/file` và không hiển thị trên trang public hay Builder binding, nhưng vẫn được giữ pin (`total_pins` lớn hơn `0`) trong `collect_pins_in_tx` (`AC-05a`).
- [ ] Ảnh dùng đồng thời ở trường `Private` và nguồn `Public`, cũng như ảnh trong `theme_option_media_refs` (`public = 1`), `seo_public_media_refs` (`home_seo` và `entry_seo` published), `builder_homepage_media` và `template_override_media_refs` đều trả `200 OK` trên `GET /media/{id}/file` (`AC-05b,c`).
- [ ] Plugin `manifest_schema: 1` khai báo `content_read_private` hoặc dùng wildcard `"*"` trong `ScopedFieldGrant` bị từ chối khi cài/bật; plugin chỉ có `content_read` cơ bản không nhận được trường `Private` của chủ sở hữu khác trong `content/before_save` và `content/after_save`, còn plugin có `ScopedFieldGrant` hợp lệ nhận được đúng trường được cấp khi vai trò của `actor` đạt từ `min_view_role` trở lên (`AC-06a,b`).
- [ ] Action có `required_role = "editor"` khi được `Admin` gọi vẫn giữ quyền `Admin` trên các trường `min_edit_role = Admin`, còn khi được `Editor` gọi thì bị chặn `403 field_edit_forbidden` (`AC-06c`).
- [ ] Có báo cáo kiểm chứng thực chạy ghi rõ SHA, môi trường, lệnh và kết quả.

#### Kiểm thử, rủi ro và rollback

- Chạy `cargo test --manifest-path cms/Cargo.toml --test m7_s09_t02_exposure_and_plugin_grants --locked`, toàn bộ plugin tests và `public_content_smoke.py`.
- Rollback bằng commit/binary trước đó nếu phát hiện regression trên đường đọc public hoặc sandbox plugin.

#### Bàn giao và review

- Ghi nhận diff, lệnh kiểm thử, kết quả và báo cáo tại `docs/verification/2026-11-16-m7-s09-t02-g5.md`.

---

### M7-S10-T01 — Panel Registry, phân hoạch base_field_panels, API editor-layout và effective-editor-config cho Editor

- **Thuộc:** V1 · M7 · M7-S10 · Admin Navigation & Single Edit
- **Module:** CNT-02
- **Capability:** CNT-02.4
- **Primary Acceptance Slice của capability:** M7-S10; M7-S13 là regression E2E toàn phạm vi.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S09-T02`).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 3 và Mục 4.1–4.2](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md).

#### Tổng quan và giá trị

Để màn hình Single Edit hiển thị nhất quán cho `post`, `page` và CPT đồng thời cho phép Admin sắp xếp các nhóm trường (bao gồm cả việc chia các base fields có sẵn của CPT thành nhiều nhóm đặt ở vùng chính và sidebar mà không đổi khóa dữ liệu), hệ thống cần một Panel Registry chuẩn hóa, API lưu `EditorLayoutConfigV1` cho Admin và endpoint đọc cấu hình hiệu lực đã lọc theo quyền cho `Editor`.

#### Mục tiêu và hành vi quan sát được

Admin có thể đọc/lưu cấu hình bố cục qua `GET/PUT /api/admin/content-types/{type_key}/editor-layout` (có CAS `If-Match: "v{layout_version}"`), chia các base fields của CPT thành nhiều nhóm `base_field_panels` (`layout:{slice_key}`) đặt ở các vùng khác nhau, trong khi các base fields chưa gán tự động rơi vào `core:base_fields`. Khi nâng cấp Theme/Plugin hoặc thêm Field Group mới, panel mới tự động được nối (auto-append) vào `default_region` mà không mất bố cục cũ. Cả `Editor` và `Admin` đều gọi được `GET /api/admin/content-types/{type_key}/effective-editor-config` để nhận cấu hình đã lọc theo `caller.role` kèm `schema_token` đầy đủ (`sha256:{64-hex}`).

#### Phạm vi công việc

- Cài đặt `EditorRegion`, `PanelKind`, `PanelDefinition`, `BaseFieldSlicePanelConfig`, `PanelPlacementConfig`, `LayoutFieldSubgroup`, `EditorLayoutConfigV1`, `ManifestActionPanelInput` và bộ kiểm tra `validate_editor_layout_config` (`Rule LAY-1` đến `Rule LAY-6`) trong `opendash_core` và `opendash_server`.
- Cài đặt hàm tính `schema_token` (`sha256:{64-hex}`) bằng chuỗi JSON chuẩn hóa (canonical `BTreeMap` theo RFC 8785 JCS) trên toàn bộ base CPT + tất cả attached groups (`declared` và `withdrawn`) + thế hệ revision gói.
- Mở rộng manifest của Plugin (`manifest_schema == 2`) để đăng ký `editor_panels` (`ManifestActionPanelInput`), thực thi ma trận allowlist `Owner × PanelKind × Namespace × AllowedRegions` (cấm gói chiếm `core:`/`layout:`, cấm Theme đăng ký `Action`, cấm đặt `Action` vào `HeaderBar` hoặc `MainPrimary`).
- Bổ sung các endpoint `GET /api/admin/content-types/{type_key}/editor-layout` (`Admin`-only), `PUT /api/admin/content-types/{type_key}/editor-layout` (`Admin`-only, kiểm tra `If-Match` và `Rule COMPAT-4`), và `GET /api/admin/content-types/{type_key}/effective-editor-config` (`Editor` + `Admin`).

#### Ngoài phạm vi

- Không thực hiện lưu dữ liệu nội dung entry trong task này (thuộc `M7-S11-T01`).

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S09-T01` và `M7-S09-T02` đã merge và có bằng chứng kiểm chứng đạt.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/crates/opendash-core/src/content.rs`
  - `cms/crates/opendash-core/src/plugin.rs`
  - `cms/crates/opendash-server/src/storage/content.rs`
  - `cms/crates/opendash-server/src/routes/content.rs`
  - `cms/crates/opendash-server/tests/m7_s10_t01_panel_registry_and_layout.rs`
  - `docs/verification/2026-11-16-m7-s10-t01-g5.md`
- Shared wiring được phép chạm: `cms/crates/opendash-core/src/lib.rs`, `cms/crates/opendash-server/src/app.rs`.

#### Cách triển khai theo contract

Khi dựng `EffectiveEditorConfigProjection` cho `caller.role`, loại bỏ hoàn toàn các trường mà `caller.role` thấp hơn `field.min_view_role` và các panel mà `caller.role` thấp hơn `panel.min_view_role`; với các trường mà `caller.role` đạt `min_view_role` nhưng thấp hơn `min_edit_role`, trả cờ `editable: false`. Đảm bảo việc cập nhật `content_editor_layouts` chỉ tăng `layout_version` (`layout_token`) và không làm thay đổi `schema_token`.

#### Nghiệm thu

- [ ] Một CPT có sẵn các base fields trong `content_types.definition_json.fields` có thể chia thành ít nhất 2 nhóm `base_field_panels` (`layout:project_info` ở `main_bottom` và `layout:project_status` ở `sidebar`) mà không làm thay đổi `definition_json` hay khóa dữ liệu; các base fields chưa gán tự động hiển thị trong `core:base_fields` (`AC-04a`).
- [ ] Khi `content_editor_layouts` đã có cấu hình lưu cho CPT, nâng cấp Theme/Plugin hoặc thêm User Field Group mới sẽ tự động nối (auto-append) panel mới vào `default_region` mà không làm mất vị trí các panel cũ (`AC-04b`).
- [ ] Manifest của Theme/Plugin thử đăng ký `CoreBuiltIn`, `BaseFieldsSlice`, tiền tố `core:`/`layout:`, Theme thử đăng ký `Action`, hoặc đặt panel vào vùng ngoài `allowed_regions` đều bị từ chối `422`; thử ẩn panel chứa trường `effective_required == true, min_edit_role == Editor` bị từ chối `422 required_field_hidden_or_inaccessible` (`AC-03c`, `AC-04c`).
- [ ] `Editor` gọi `GET /api/admin/content-types/{type_key}/effective-editor-config` nhận được `200 OK` (đã ẩn toàn bộ trường/panel `min_view_role == Admin` và gắn `editable: false` cho trường `min_edit_role == Admin`), nhưng khi `Editor` gọi `GET` hoặc `PUT /api/admin/content-types/{type_key}/editor-layout` thì bị chặn `403 Forbidden` (`AC-09b`).
- [ ] Có báo cáo kiểm chứng thực chạy ghi rõ SHA, môi trường, lệnh và kết quả.

#### Kiểm thử, rủi ro và rollback

- Chạy `cargo test --manifest-path cms/Cargo.toml --test m7_s10_t01_panel_registry_and_layout --locked` và các script governance.
- Cấu hình bố cục nằm riêng trong bảng `content_editor_layouts`, không sửa đổi dữ liệu trong `content_entries`.

#### Bàn giao và review

- Ghi nhận diff, lệnh kiểm thử, kết quả và báo cáo tại `docs/verification/2026-11-16-m7-s10-t01-g5.md`.

---

### M7-S11-T01 — Giao dịch Lưu nguyên tử CompositeEntrySaveInput, PatchField 3 trạng thái, Hàng rào Pha 3 và API Hồ sơ cá nhân

- **Thuộc:** V1 · M7 · M7-S11 · Admin Navigation & Single Edit
- **Module:** CNT-04
- **Capability:** CNT-04.3
- **Primary Acceptance Slice của capability:** M7-S11; M7-S13 là regression E2E toàn phạm vi.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S10-T01`).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 4.3, 4.5 và Mục 5.1](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md).

#### Tổng quan và giá trị

Hiện tại trên màn hình chỉnh sửa bài viết/trang/CPT, việc lưu nội dung chính, lưu đường dẫn tùy chỉnh (`custom_path`) và lưu cấu hình SEO đang tách thành 3 lệnh gọi API rời rạc, mỗi lệnh tự tăng `version` khiến phát sinh lỗi xung đột `412` giả tạo hoặc lưu dở dang (partial commit) khi một phần bị lỗi. Ngoài ra `Editor` chưa có API tự cập nhật hồ sơ/mật khẩu cá nhân vì `/api/admin/users/{id}` đang khóa `Admin`-only. Task này giải quyết triệt để bằng giao dịch composite nguyên tử 4 pha và endpoint hồ sơ cá nhân an toàn.

#### Mục tiêu và hành vi quan sát được

Một lần bấm Lưu gửi `CompositeEntrySaveInput` tới `POST /api/admin/content-types/{type_key}/entries/composite` hoặc `PUT /api/admin/content-types/{type_key}/entries/{id}/composite` sẽ kiểm tra `expected_schema_token` bắt buộc (`428` nếu thiếu, `409` nếu lệch), từ chối `403 field_edit_forbidden` cho mọi trường vượt quyền sửa trước khi đọc/so sánh DB, áp dụng ngữ nghĩa 3 trạng thái `PatchField` (`Omitted = Preserve`, `Clear = xóa khóa`, `Set = cập nhật`), chạy `content/before_save` ngoài transaction, rồi ghi toàn bộ `content_entries` + `entry_terms` + `entry_public_paths` + `entry_seo` trong một giao dịch `BEGIN IMMEDIATE` duy nhất với đúng một lần tăng `version + 1` (sau khi kiểm lại snapshot Pha 1). Đồng thời cả `Editor` và `Admin` đều dùng được `GET/PUT /api/admin/profile` (bắt buộc `current_password` khi đổi mật khẩu).

#### Phạm vi công việc

- Cài đặt `PatchField` (`Omitted`, `Clear`, `Set`) kèm hàm `deserialize_patch_field` và DTO `CompositeEntrySaveInput` (`#[serde(deny_unknown_fields)]`, không có `body`/`cover_image` trùng lặp ở cấp ngoài) trong `opendash_core` / `opendash_server`.
- Cài đặt quy trình thực thi 4 pha cho `POST /api/admin/content-types/{type_key}/entries/composite` và `PUT /api/admin/content-types/{type_key}/entries/{id}/composite`, đồng thời bổ sung hàng rào kiểm tra lại snapshot Pha 1 (`current_schema_token == preflight_schema_token` và `current_actor_role == preflight_actor_role`) vào bên trong giao dịch `BEGIN IMMEDIATE` cho cả composite endpoint lẫn các endpoint ghi nội dung hiện có (`entries.rs`).
- Lọc bỏ các trường mà `caller.role` thấp hơn `field.min_view_role` khỏi kết quả trả về của các API đọc entry trong Admin (`GET /api/admin/content-types/{type_key}/entries` và `GET /api/admin/content-types/{type_key}/entries/{id}`).
- Bổ sung `GET /api/admin/profile` và `PUT /api/admin/profile` trong `routes/users.rs` và `storage/users.rs` cho phép người dùng đang đăng nhập (`Editor` hoặc `Admin`) sửa `display_name` hoặc đổi mật khẩu của chính mình khi gửi kèm đúng `current_password` (sai trả `403 invalid_current_password`; không cho phép đổi `role`).

#### Ngoài phạm vi

- Không thay đổi giao diện React trong task này (thuộc `M7-S12-T01` và `M7-S12-T02`).

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S10-T01` đã merge và có bằng chứng kiểm chứng đạt.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/crates/opendash-core/src/content.rs`
  - `cms/crates/opendash-server/src/storage/entries.rs`
  - `cms/crates/opendash-server/src/storage/seo.rs`
  - `cms/crates/opendash-server/src/storage/users.rs`
  - `cms/crates/opendash-server/src/routes/entries.rs`
  - `cms/crates/opendash-server/src/routes/users.rs`
  - `cms/crates/opendash-server/tests/m7_s11_t01_composite_save_and_profile.rs`
  - `docs/verification/2026-11-16-m7-s11-t01-g5.md`
- Shared wiring được phép chạm: `cms/crates/opendash-server/src/app.rs`.

#### Cách triển khai theo contract

Tái sử dụng các hàm kiểm tra trong transaction đã có của `storage::seo` (`validate_route_claim_in_tx`, `check_default_slug_claim_in_tx`, `validate_seo_input`, `verify_og_media_exists_in_tx`, `sync_entry_seo_media_ref_in_tx`) và `storage::media::require_existing_references_in_tx` bên trong cùng một giao dịch `BEGIN IMMEDIATE` của `storage::entries`. Đảm bảo khi `update`, mọi trường không gửi trong `input.values` (`Omitted`) luôn giữ nguyên giá trị trong `existing_entry.values` và không bao giờ tự động chèn lại `default_value`.

#### Nghiệm thu

- [ ] Một lệnh composite `POST` hoặc `PUT` cập nhật nguyên tử `content_entries` (`values_json` chứa `body`/`featured_image`) + `entry_terms` + `entry_public_paths` + `entry_seo` trong một giao dịch `BEGIN IMMEDIATE` và chỉ tăng `version` thêm `+1`; nếu `custom_path` bị trùng hoặc `og_image_id` không tồn tại thì toàn bộ `content_entries` và `entry_terms` được rollback 100% (`AC-07a`).
- [ ] Gửi `"custom_path": null` hoặc `"seo": null` xóa dòng tương ứng trong `entry_public_paths` / `entry_seo`; không gửi khóa `custom_path` hoặc `seo` (`Omitted`) giữ nguyên dữ liệu cũ; khi `Admin` đã xóa (`"field": null`) một trường tùy chọn có `default_value`, lần sau `Editor` sửa `title` và không gửi trường đó (`Omitted`) thì trường đó vẫn vắng mặt trong `values_json` (`AC-07b,c`).
- [ ] Khi `Editor` gửi tường minh một `field_key` có `min_edit_role == Admin` trong `values` (dù gửi giá trị trùng khớp với DB, khác DB, hay `null`), server luôn trả đúng mã `403 field_edit_forbidden` trước khi đọc/so sánh DB (`AC-06d`).
- [ ] Không gửi hoặc gửi rỗng `expected_schema_token` trả đúng `428 missing_schema_token`; gửi token cũ trả `409 schema_version_conflict`; nếu schema hoặc quyền của `actor` thay đổi trong lúc Pha 2 (`content/before_save`) đang chạy thì Pha 3 (`BEGIN IMMEDIATE`) chặn ghi với `409` / `403` (`AC-08a,b`).
- [ ] `Editor` và `Admin` đổi mật khẩu tại `PUT /api/admin/profile` bắt buộc gửi đúng `current_password` (thiếu hoặc sai trả `403`) và không thể nâng `role` (`AC-09c`).
- [ ] Có báo cáo kiểm chứng thực chạy ghi rõ SHA, môi trường, lệnh và kết quả.

#### Kiểm thử, rủi ro và rollback

- Chạy `cargo test --manifest-path cms/Cargo.toml --test m7_s11_t01_composite_save_and_profile --locked` và toàn bộ workspace tests.
- Giao dịch `BEGIN IMMEDIATE` đảm bảo không có trạng thái ghi dở dang; các endpoint cũ vẫn giữ nguyên tương thích ngược.

#### Bàn giao và review

- Ghi nhận diff, lệnh kiểm thử, kết quả và báo cáo tại `docs/verification/2026-11-16-m7-s11-t01-g5.md`.

---

### M7-S12-T01 — Menu điều hướng 2 cấp, Hash Router chuẩn, Màn hình Hồ sơ cá nhân và Khu vực Cài đặt Loại nội dung & Trường

- **Thuộc:** V1 · M7 · M7-S12 · Admin Navigation & Single Edit
- **Module:** FND-05
- **Capability:** FND-05.3
- **Primary Acceptance Slice của capability:** M7-S12; M7-S13 là regression E2E toàn phạm vi.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S11-T01`).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 3 và Mục 5.1](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md), [Product Doc Mục 2 và Mục 4](../../../../../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

#### Tổng quan và giá trị

Hiện tại thanh điều hướng Admin đang hiển thị phẳng và màn hình `#/content` đang trộn lẫn việc tạo CPT mới với việc nhập liệu entry, trái với chỉ đạo của chủ dự án. Task này tổ chức lại toàn bộ hệ thống menu 2 cấp giống WordPress, bổ sung bảng định tuyến hash URL chuẩn hỗ trợ mở trực tiếp/reload/back/forward, khóa chặt RBAC cho các deep-link (đặc biệt là Builder template toàn site), xây dựng màn hình `Hồ sơ của tôi` và xây dựng khu vực `Cài đặt → Loại nội dung & Trường` chỉ dành riêng cho cấu hình schema, nhóm trường và bố cục panel (không có bất kỳ bảng/form nhập entry nào).

#### Mục tiêu và hành vi quan sát được

Người dùng nhìn thấy cây menu 10 mục chính kèm các menu động cho từng CPT theo đúng quyền `Admin` / `Editor`; menu cha tự mở theo URL hiện tại, có breadcrumb nhất quán và ngăn kéo (drawer) hỗ trợ bàn phím trên mobile. Khi website có `0` CPT, `Admin` vẫn thấy `Cài đặt → Loại nội dung & Trường` (`#/settings/content-types`) để tạo/sửa CPT, quản lý `content_field_groups` và cấu hình `EditorLayoutConfigV1` (bao gồm chia nhóm `base_field_panels`), hoàn toàn không có giao diện nhập liệu entry tại đây. Mỗi CPT có menu riêng (`#/cpt/:typeKey`) trên Sidebar để quản lý entry.

#### Phạm vi công việc

- Nâng cấp `AdminShell.tsx` và bộ định tuyến trong `App.tsx` sang cây điều hướng 2 cấp theo đúng bảng Hash Route chuẩn tại Mục `5.1` của Contract (hỗ trợ mở URL trực tiếp, reload, back/forward, tự động redirect `#/content` sang `#/settings/content-types` cho `Admin` và `#/posts` cho `Editor`).
- Bổ sung kiểm tra RBAC cho các deep-link: ẩn mục menu không thuộc quyền của `Editor`; khi `Editor` mở trực tiếp URL quản trị (`#/settings/*`, `#/plugins/*`, `#/tools/*`, `#/appearance/themes|customize|menus|templates`, hoặc `#/builder` cho template toàn site không gắn `entryId`), hiển thị màn hình `403 Không có quyền truy cập` và đồng bộ kiểm tra quyền tương ứng ở `routes/overrides.rs` nếu còn đường ghi template chung chưa khóa `Admin`.
- Xây dựng màn hình `Hồ sơ của tôi` (`#/users/profile`) gọi `GET/PUT /api/admin/profile` cho cả `Editor` và `Admin`.
- Xây dựng khu vực `Cài đặt → Loại nội dung & Trường` (`#/settings/content-types` và `#/settings/content-types/:typeKey`) dành riêng cho `Admin` với 3 chức năng: (1) Tạo và sửa CPT (`POST/PUT /api/admin/content-types`), (2) Tạo, sửa, rút nhóm trường mở rộng (`/api/admin/content-field-groups`), và (3) Cấu hình bố cục màn hình chỉnh sửa (`GET/PUT /api/admin/content-types/{type_key}/editor-layout`, bao gồm chia nhóm `base_field_panels` và `subgroups`), tuyệt đối không chứa danh sách hay form nhập entry.

#### Ngoài phạm vi

- Màn hình Single Edit 5 vùng và `EditorDraftStore` sẽ được hoàn thiện trong `M7-S12-T02` ngay kế tiếp.

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S11-T01` đã merge và có bằng chứng kiểm chứng đạt.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/crates/opendash-server/src/routes/overrides.rs`
  - `cms/crates/opendash-server/src/storage/overrides.rs`
  - `cms/admin/src/App.tsx`
  - `cms/admin/src/shell/AdminShell.tsx`
  - `cms/admin/src/api/content.ts`
  - `cms/admin/src/api/users.ts`
  - `cms/admin/src/features/content/ContentTypeSchemaSettings.tsx`
  - `cms/admin/src/features/content/FieldGroupsManager.tsx`
  - `cms/admin/src/features/content/EditorLayoutConfigurator.tsx`
  - `cms/admin/src/features/users/UserProfilePanel.tsx`
  - `cms/crates/opendash-server/tests/m7_s12_t01_navigation_and_schema_ui.rs`
  - `docs/verification/2026-11-16-m7-s12-t01-g5.md`
- Shared wiring được phép chạm: `cms/admin/src/api/index.ts`, `cms/admin/src/features/content/ContentWorkspace.tsx`, `cms/admin/src/features/content/ContentTypeForm.tsx`.

#### Cách triển khai theo contract

Không dùng thư viện ngoài chưa được pin trong `cms/admin/package.json`; cài đặt bộ phân tích và đồng bộ `window.location.hash` thuần TypeScript/React hỗ trợ `hashchange` và `popstate`. На màn hình `Cài đặt → Loại nội dung & Trường`, loại bỏ hoàn toàn mọi bảng danh sách entry và form tạo/sửa entry khỏi khu vực cấu hình này.

#### Nghiệm thu

- [ ] Khi website có `0` CPT, `Admin` vẫn thấy và mở được `Cài đặt → Loại nội dung & Trường` (`#/settings/content-types`); khu vực này chỉ có cấu hình CPT, Field Groups và Editor Layout, không có danh sách hay form nhập entry; khi tạo một CPT mới (ví dụ `portfolio`), CPT đó xuất hiện thành một mục menu riêng (`#/cpt/portfolio`) trên Sidebar (`AC-09a`).
- [ ] Mở trực tiếp từng hash URL chuẩn, bấm reload, hoặc bấm Back/Forward trên trình duyệt đều đến đúng màn hình, tự mở đúng menu cha và đánh dấu đúng submenu đang hoạt động; trên mobile (`390px`), sidebar hoạt động dạng ngăn kéo (drawer) hỗ trợ bàn phím (`Escape` để đóng, có `aria-expanded`).
- [ ] `Editor` không thấy các menu `Admin`-only và khi truy cập trực tiếp `#/settings/content-types` hoặc `#/builder?template=header` (template dùng chung toàn site) thì bị chặn `403` cả trên UI lẫn API (`AC-09b`).
- [ ] `Editor` và `Admin` mở được `#/users/profile` để đổi tên hiển thị và đổi mật khẩu (bắt buộc nhập mật khẩu hiện tại) (`AC-09c`).
- [ ] `Admin` cấu hình chia base fields của CPT thành 2 nhóm `base_field_panels` đặt ở `main_bottom` và `sidebar` trên giao diện `#/settings/content-types/:typeKey` và lưu thành công qua `PUT /api/admin/content-types/{type_key}/editor-layout` (`AC-04a`).
- [ ] Có báo cáo kiểm chứng thực chạy bằng trình duyệt thật với binary thật.

#### Kiểm thử, rủi ro và rollback

- Chạy `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `cargo test --manifest-path cms/Cargo.toml --test m7_s12_t01_navigation_and_schema_ui --locked` và kiểm chứng trên trình duyệt Chrome thật ở cả desktop (`1440x900`) và mobile (`390x844`).

#### Bàn giao và review

- Ghi nhận diff, lệnh kiểm thử, kết quả và báo cáo tại `docs/verification/2026-11-16-m7-s12-t01-g5.md`.

---

### M7-S12-T02 — Màn hình Single Edit thống nhất 5 vùng, Core Standard Field Fallback Form, Action Panel và Quarantine EditorDraftStore

- **Thuộc:** V1 · M7 · M7-S12 · Admin Navigation & Single Edit
- **Module:** FND-05
- **Capability:** FND-05.4
- **Primary Acceptance Slice của capability:** M7-S12; M7-S13 là regression E2E toàn phạm vi.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S12-T01`).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 1.3, Mục 3, Mục 4 và Mục 5.2](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md), [Product Doc Mục 3, 4 và 5](../../../../../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

#### Tổng quan và giá trị

Sau khi đã có API composite save, `effective-editor-config` và hệ thống điều hướng mới, người biên tập và quản trị viên cần màn hình Single Edit thống nhất cho `post`, `page` và mọi CPT. Màn hình này phải dựng đúng 5 vùng (`header_bar`, `main_top`, `main_primary`, `main_bottom`, `sidebar`) theo cấu hình hiệu lực của từng vai trò, lưu nguyên tử trong một lần bấm Lưu, vẫn cho phép xem/sửa dữ liệu qua Core Standard Field Fallback Form khi theme/plugin bị tắt, tự động mở rộng panel thu gọn khi có lỗi validation, tách biệt nút Action của plugin khỏi nút Lưu, và bảo vệ bản nháp qua cơ chế quarantine + kiểm quyền lại khi hết hạn phiên (`401`).

#### Mục tiêu và hành vi quan sát được

Khi `Editor` hoặc `Admin` mở `#/posts/new`, `#/posts/:id/edit`, `#/pages/new`, `#/pages/:id/edit`, `#/cpt/:typeKey/new` hoặc `#/cpt/:typeKey/:id/edit`, giao diện tải `GET /api/admin/content-types/{type_key}/effective-editor-config` để dựng 5 vùng panel đúng thứ tự (trên mobile `< 1024px` theo đúng thứ tự DOM `header_bar → main_top → main_primary → main_bottom → sidebar`). Một lần bấm Lưu gửi `CompositeEntrySaveInput` cập nhật toàn bộ dữ liệu của bản ghi. Khi gặp `401`, bản nháp được quarantine (ẩn khỏi DOM) và sau khi đăng nhập lại sẽ được lọc qua `scrubQuarantinedDraft` dựa trên `effective-editor-config` mới trước khi khôi phục.

#### Phạm vi công việc

- Xây dựng màn hình Single Edit dùng chung (`SingleEditWorkspace.tsx`) cho `post`, `page` và `:typeKey`, kết nối với danh sách bài viết (`#/posts`), danh sách trang (`#/pages`) và danh sách entry của từng CPT (`#/cpt/:typeKey`).
- Dựng đầy đủ 5 vùng (`header_bar`, `main_top`, `main_primary`, `main_bottom`, `sidebar`) với thứ tự DOM thống nhất `header_bar → main_top → main_primary → main_bottom → sidebar`, hỗ trợ các panel `CoreBuiltIn`, `BaseFieldsSlice` (`layout:{slice_key}`), `ContentFields` (cùng `subgroups` và Core Standard Field Fallback Form khi `provider_inactive` hoặc `withdrawn`) và `Action` (`PanelKind::Action` gọi `plugin_action` độc lập, không chạy khi bấm Lưu).
- Cài đặt cơ chế tự động mở rộng (`collapsed = false`) và cuộn/focus tới trường đầu tiên bị lỗi validation (hoặc lỗi `field_errors` trả về từ `content/before_save`) khi panel chứa trường đó đang thu gọn.
- Cài đặt quy tắc chuyển sang Builder an toàn từ Single Edit: trên màn hình tạo mới CPT (`id === null`), nút mở Builder yêu cầu người dùng Lưu/Xuất bản trước chứ không tự động xuất bản ngầm; chỉ hiển thị cảnh báo ghi đè `body` khi template đang áp dụng không bind trường `body`.
- Cài đặt `EditorDraftStore` trong RAM gắn `(site_instance_id, user_id, auth_generation, draft_key, status, schema_token_at_edit)` với `draft_uuid` riêng cho từng tab tạo mới, biến đếm `session_epoch` hủy response trễ từ phiên cũ, quarantine ẩn khỏi DOM khi gặp `401`, xóa sạch khi Logout/đổi tài khoản/đổi site, và hàm `scrubQuarantinedDraft` kiểm quyền lại với `effective-editor-config` mới sau khi đăng nhập lại.

#### Ngoài phạm vi

- Không tự mở rộng sang trình soạn thảo Gutenberg, lịch sử phiên bản (revisions), tự động lưu ngầm lên server (autosave) hay luồng duyệt bài.

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S12-T01` đã merge và có bằng chứng kiểm chứng đạt.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/crates/opendash-server/src/routes/auth.rs` (đảm bảo `/api/admin/auth/me` trả kèm `site_instance_id` ổn định của site để `EditorDraftStore` đối chiếu provenance)
  - `cms/admin/src/App.tsx`
  - `cms/admin/src/shell/AdminShell.tsx`
  - `cms/admin/src/api/content.ts`
  - `cms/admin/src/api/auth.ts`
  - `cms/admin/src/features/editor/EditorDraftStore.ts`
  - `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
  - `cms/admin/src/features/editor/PanelRegionRenderer.tsx`
  - `cms/admin/src/features/posts/PostPageWorkspace.tsx`
  - `cms/admin/src/features/content/ContentWorkspace.tsx`
  - `cms/admin/src/features/seo/SeoPanel.tsx`
  - `cms/crates/opendash-server/tests/m7_s12_t02_single_edit_and_draft_recovery.rs`
  - `docs/verification/2026-11-16-m7-s12-t02-g5.md`

#### Cách triển khai theo contract

Trong `SingleEditWorkspace.tsx`, chỉ gửi trong `CompositeEntrySaveInput.values` các trường có `editable == true` mà người dùng có quyền sửa (với các trường `editable == false`, không đưa khóa vào `values` để server giữ nguyên giá trị cũ theo quy tắc `Omitted = Preserve`). Khi bấm xóa một trường tùy chọn, gửi giá trị `null` (`FieldPatch::Clear`).

#### Nghiệm thu

- [ ] Trên cả Post, Page và CPT, màn hình Single Edit dựng đúng 5 vùng theo `effective-editor-config`; trên mobile (`390px`) thứ tự DOM thực tế là `header_bar → main_top → main_primary → main_bottom → sidebar`; một lần bấm Lưu cập nhật nguyên tử core fields + custom fields + taxonomy + `custom_path` + SEO (`AC-04d`, `AC-07`).
- [ ] Khi tắt plugin hoặc đổi theme (`provider_inactive`) hoặc khi group/target/field ở trạng thái `withdrawn`, người dùng có quyền vẫn xem và sửa được giá trị các trường (bao gồm cả trường `Image`) qua Core Standard Field Fallback Form trên Single Edit (`AC-01a,b`).
- [ ] Khi một trường bắt buộc nằm trong panel đang thu gọn bị bỏ trống hoặc bị lỗi validation, bấm Lưu sẽ tự động mở rộng panel đó và đưa focus tới ô input lỗi; bấm Lưu không kích hoạt `plugin_action` trong Action Panel và bấm nút trên Action Panel không tự lưu dữ liệu form chưa lưu; màn hình tạo mới CPT (`id === null`) không tự động xuất bản ngầm khi bấm mở Builder (`AC-10`).
- [ ] Kiểm chứng `EditorDraftStore` và negative case hạ quyền `Admin → Editor` cùng `user_id` (`AC-11`): User đang là `Admin` sửa nháp chứa cả trường `Editor` và trường `Admin`-only → gặp `401` khiến nháp chuyển sang `quarantined` và ẩn khỏi DOM → tài khoản bị hạ quyền xuống `Editor` (hoặc trường đó đổi sang `Admin`-only) → cùng user đăng nhập lại → hệ thống tải `effective-editor-config` mới, xóa vĩnh viễn trường `Admin`-only khỏi bản nháp trước khi khôi phục các trường `Editor` hợp lệ; khi Logout hoặc đổi sang tài khoản/site khác thì toàn bộ nháp bị xóa sạch.
- [ ] Có báo cáo kiểm chứng thực chạy bằng trình duyệt Chrome thật với binary thật.

#### Kiểm thử, rủi ro và rollback

- Chạy `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `cargo test --manifest-path cms/Cargo.toml --test m7_s12_t02_single_edit_and_draft_recovery --locked` và kiểm chứng trình duyệt Chrome thật.

#### Bàn giao và review

- Ghi nhận diff, lệnh kiểm thử, kết quả và báo cáo tại `docs/verification/2026-11-16-m7-s12-t02-g5.md`.

---

### M7-S13-T01 — Nghiệm thu E2E Admin Navigation và Single Edit cùng bộ ảnh Mộc Studio trên Windows

- **Thuộc:** V1 · M7 · M7-S13 · Admin Navigation & Single Edit
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13 (Windows primary acceptance; `M7-S13-T02` là parity trên macOS).
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S12-T02`).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 6 (`AC-01`–`AC-11`)](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md), [Product Doc Mục 7](../../../../../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

#### Tổng quan và giá trị

Sau khi hoàn thành toàn bộ các slice từ `M7-S09` đến `M7-S12`, cần một bộ kiểm thử tích hợp đầu-cuối (end-to-end) tự động chạy trên binary release thật và trình duyệt Chrome thật trên Windows để chứng minh toàn bộ 11 tiêu chí nghiệm thu (`AC-01` đến `AC-11`) cùng khả năng nâng cấp/rollback DB hoạt động trơn tru, đồng thời cập nhật bộ kịch bản và ảnh chụp màn hình trên website mẫu Mộc Studio (#258) theo đúng yêu cầu sản phẩm.

#### Mục tiêu và hành vi quan sát được

Script E2E tự động khởi tạo và nâng cấp website, kiểm chứng đầy đủ `AC-01` đến `AC-11` qua HTTP API và trình duyệt Chrome thật (cả desktop `1440x900` và mobile `390x844`), kiểm chứng nâng cấp từ snapshot trước `0034` và phục hồi bằng binary cũ trên site root riêng, đồng thời chụp lại bộ ảnh màn hình thực tế trên site demo Mộc Studio (#258) phản ánh cây điều hướng mới, màn hình `Cài đặt → Loại nội dung & Trường`, menu CPT riêng và màn hình Single Edit 5 vùng.

#### Phạm vi công việc

- Xây dựng script kiểm thử E2E tổng hợp `cms/scripts/admin_nav_single_edit_e2e.py` chạy với binary release thật và Chrome headless/browser thật trên Windows, bao phủ toàn bộ 11 case `AC-01` đến `AC-11` và kiểm tra nâng cấp/rollback pre-`0034`.
- Cập nhật runner demo Mộc Studio (`cms/scripts/moc_studio_demo_runner.py` từ #258) để cấu hình nhóm custom fields, chia nhóm `base_field_panels` trên CPT của Mộc Studio và chụp bổ sung các màn hình mới (Desktop + Mobile `390px`) theo đúng source SHA đã build.
- Chạy toàn bộ Rust workspace tests và các bộ smoke/regression của M3–M8 liên quan để xác nhận không có hồi quy.

#### Ngoài phạm vi

- Không tự ý tuyên bố visual approval của chủ dự án chỉ từ việc chụp ảnh thành công; không thay thế nghiệm thu Linux cuối V1 của M8.

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S09-T01`, `M7-S09-T02`, `M7-S10-T01`, `M7-S11-T01`, `M7-S12-T01` và `M7-S12-T02` đều đã merge và đạt kiểm chứng.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `cms/scripts/admin_nav_single_edit_e2e.py`
  - `cms/scripts/moc_studio_demo_runner.py`
  - `docs/verification/2026-11-16-m7-s13-t01-admin-nav-single-edit-windows.md`
  - Các ảnh chụp màn hình cập nhật trong thư mục verification/demo được chỉ định trong báo cáo.

#### Cách triển khai theo contract

Chạy kiểm thử trên thư mục site tạm cô lập; ghi nhận chính xác mã SHA của source, đường dẫn và mã băm SHA-256 của file thực thi `opendash.exe`, phiên bản Chrome, toàn bộ lệnh thực chạy và kết quả từng tiêu chí `AC-01` đến `AC-11`.

#### Nghiệm thu

- [ ] Script `admin_nav_single_edit_e2e.py` chạy trên Windows release binary đạt PASS 100% cho toàn bộ 11 tiêu chí `AC-01` đến `AC-11` (bao gồm cả browser flows trên desktop và mobile `390px`, tắt/bật provider, gỡ target/field tombstone, kiểm tra 7 nhánh media public, lọc hook/action của plugin, composite save `PatchField`, và `EditorDraftStore` quarantine + hạ quyền `Admin → Editor`).
- [ ] Kiểm thử nâng cấp DB từ snapshot trước `0034` lên schema mới và kiểm thử rollback bằng binary cũ trên site root riêng đều đạt PASS, toàn vẹn dữ liệu (`PRAGMA integrity_check` và `PRAGMA foreign_key_check`).
- [ ] Bộ ảnh chụp màn hình trên site demo Mộc Studio (#258) được cập nhật đầy đủ từ đúng source SHA đã kiểm chứng.
- [ ] Toàn bộ `cargo test --manifest-path cms/Cargo.toml --workspace --locked` và các script governance (`check-plans.py`, `check-standards.py`, `check-agent-artifacts.py`, `git diff --check`) đều PASS.

#### Kiểm thử, rủi ro và rollback

- Chạy `cargo build --manifest-path cms/Cargo.toml --release --locked`, `cargo test --manifest-path cms/Cargo.toml --workspace --locked` và `python cms/scripts/admin_nav_single_edit_e2e.py`.

#### Bàn giao và review

- Ghi nhận báo cáo nghiệm thu đầy đủ tại `docs/verification/2026-11-16-m7-s13-t01-admin-nav-single-edit-windows.md`.

---

### M7-S13-T02 — Nghiệm thu đối xứng Admin Navigation và Single Edit trên macOS

- **Thuộc:** V1 · M7 · M7-S13 · Admin Navigation & Single Edit
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** Regression đa hệ điều hành cho capability `FND-05.5` (không đếm trùng capability mới).
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (chờ `M7-S13-T01`; hoãn thực thi theo chính sách macOS deferral của ADR-0014 khi chưa mở đợt chạy macOS).
- **Người triển khai / review:** Harbor dự kiến (trên macOS); coordinator Atlas, self-review hoặc peer review theo ADR-0010, không phải active assignment.
- **Nguồn quyết định:** [ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md), [Contract Mục 6 (`AC-01`–`AC-11`)](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md), [ADR-0014 macOS deferral](../../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Đảm bảo toàn bộ hành vi của Admin Navigation, CPT Schema Builder, Panel Registry và Composite Single Edit (`AC-01` đến `AC-11`) hoạt động đồng nhất trên binary Mach-O và trình duyệt Chrome bản địa trên macOS từ cùng mã nguồn đã qua nghiệm thu Windows.

#### Mục tiêu và hành vi quan sát được

Chạy bộ kiểm thử Rust workspace và `admin_nav_single_edit_e2e.py` trên macOS với binary release thật và Chrome bản địa, xác nhận đạt toàn bộ `AC-01` đến `AC-11` cùng kiểm thử nâng cấp/rollback DB.

#### Phạm vi công việc

- Build binary release `opendash` trên macOS từ commit trên `main` sau khi `M7-S13-T01` đã hoàn tất.
- Chạy `cargo test --manifest-path cms/Cargo.toml --workspace --locked` và `python3 cms/scripts/admin_nav_single_edit_e2e.py` trên macOS.
- Ghi nhận biên bản nghiệm thu bản địa macOS tại `docs/verification/2026-11-16-m7-s13-t02-admin-nav-single-edit-macos.md`.

#### Ngoài phạm vi

- Không sửa đổi contract hay schema trong task kiểm chứng đối xứng này (nếu phát hiện lỗi đặc thù hệ điều hành, báo blocker và mở PR sửa lỗi có kiểm chứng trên cả hai OS).

#### Phụ thuộc và điều kiện chặn

- Phụ thuộc trực tiếp: `M7-S13-T01` đã hoàn tất trên `main` và có runner macOS khả dụng. Theo ADR-0014, việc task macOS này ở trạng thái `NOT_RUN` khi chưa chạy đợt macOS không chặn quá trình phát triển trên Windows, nhưng vẫn là điều kiện cần cho đợt đóng nghiệm thu đa hệ điều hành.

#### Kỹ thuật và file ownership

- File được phép sửa trong lượt này:
  - `docs/verification/2026-11-16-m7-s13-t02-admin-nav-single-edit-macos.md`

#### Cách triển khai theo contract

Thực thi bộ kiểm thử trên thư mục site tạm cô lập trên macOS, ghi nhận đầy đủ thông tin máy, phiên bản macOS, SHA-256 của binary Mach-O, phiên bản Chrome và kết quả từng tiêu chí `AC-01` đến `AC-11`.

#### Nghiệm thu

- [ ] Binary release trên macOS vượt qua 100% `cargo test --manifest-path cms/Cargo.toml --workspace --locked` và `admin_nav_single_edit_e2e.py` (`AC-01` đến `AC-11`).
- [ ] Kiểm thử nâng cấp DB từ snapshot trước `0034` và phục hồi bằng binary cũ trên site root riêng đạt PASS trên macOS.
- [ ] Có báo cáo kiểm chứng thực chạy ghi rõ SHA, binary digest, môi trường macOS, lệnh và kết quả.

#### Kiểm thử, rủi ro và rollback

- Chạy `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, `python3 cms/scripts/admin_nav_single_edit_e2e.py`, và các script governance (`check-plans.py`, `check-standards.py`, `check-agent-artifacts.py`, `git diff --check`).

#### Bàn giao và review

- Ghi nhận báo cáo nghiệm thu tại `docs/verification/2026-11-16-m7-s13-t02-admin-nav-single-edit-macos.md`.
