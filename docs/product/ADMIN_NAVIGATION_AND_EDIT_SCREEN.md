# Admin Navigation và Bố cục Màn hình Chỉnh sửa

> Tài liệu này tổng hợp thiết kế sản phẩm đã triển khai trong [`AdminShell.tsx`](../../cms/admin/src/shell/AdminShell.tsx), [`router.ts`](../../cms/admin/src/shell/router.ts) và [`SingleEditWorkspace.tsx`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx). Các chi tiết hợp đồng API/schema xem tại [`ADMIN_NAVIGATION_AND_SINGLE_EDIT.md`](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) và [`ADMIN_REVIEW_2026_11.md`](../contracts/ADMIN_REVIEW_2026_11.md).

## 1. Mục tiêu

Người quen WordPress có thể tìm đúng công cụ trong Admin, quản lý nội dung và chỉnh từng trang theo bố cục nhất quán. Các tính năng mở rộng đăng ký panel/column vào vùng có sẵn theo cấu hình khai báo, không nối thêm panel tùy ý vào màn hình.

## 2. Nhóm Menu Chính, Submenu và Hash Routes

Thanh điều hướng Admin (desktop sidebar thu gọn được + mobile drawer) tổ chức thành `Tổng quan` và 3 nhóm có nhãn rõ ràng. Bảng dùng nhãn Tiếng Việt; giao diện mặc định là English (`en`), và nhãn thay đổi theo CMS Language.

| Nhóm | Menu chính | Submenu & Hash Route chính | Quyền |
| --- | --- | --- | --- |
| — | **Tổng quan** | Dashboard (`#/dashboard`) | Admin, Editor |
| **Nội dung** | **Bài viết** | Tất cả bài viết (`#/posts`) · Thêm mới (`#/posts/new`) · Chuyên mục (`#/posts/categories`) · Thẻ (`#/posts/tags`) | Admin, Editor |
| **Nội dung** | **Trang** | Tất cả trang (`#/pages`) · Thêm mới (`#/pages/new`) | Admin, Editor |
| **Nội dung** | **CPT tùy chỉnh** | Danh sách (`#/cpt/:typeKey`) · Thêm mới (`#/cpt/:typeKey/new`) · Taxonomy gắn với CPT | Admin, Editor |
| **Nội dung** | **Thư viện media** | Media (`#/media`) | Admin, Editor |
| **Nội dung** | **Liên hệ** | Cấu hình và quản lý liên hệ (`#/settings/contact`) | Admin |
| **Giao diện** | **Tùy chỉnh giao diện** | OpenDash Studio (`#/studio`) | Admin |
| **Giao diện** | **Theme** | Themes đã cài (`#/appearance/themes`) · Cài theme (`#/appearance/library`) | Admin |
| **Giao diện** | **Patterns** | Tất cả Pattern (`#/patterns`) · Thêm Pattern (`#/patterns/new`) | Admin |
| **Hệ thống** | **Cấu trúc nội dung** | Loại nội dung & Trường (`#/appearance/content-types`) · Taxonomy tùy chỉnh (`#/taxonomy/definitions`) | Admin |
| **Hệ thống** | **Plugins** | Plugins đã cài (`#/plugins`) · Cài plugin (`#/plugins/install`) | Admin |
| **Hệ thống** | **Người dùng** | Tất cả người dùng (`#/users`) · Thêm người dùng (`#/users/new`) · Hồ sơ của tôi (`#/users/profile`) | Admin; Editor chỉ thấy Hồ sơ của tôi |
| **Hệ thống** | **Công cụ** | Sao lưu & Phục hồi (`#/tools/backup`) · Đối soát Media (`#/tools/media-reconcile`) · Tình trạng hệ thống (`#/tools/health`) | Admin |
| **Hệ thống** | **Cài đặt** | Tổng quát (`#/settings/general`) · Đa ngôn ngữ (`#/settings/multilingual`) · Trang chủ & Bài viết (`#/settings/reading`) · Đường dẫn tĩnh (`#/settings/permalinks`) · SEO (`#/settings/seo`) · Thông báo Email (SMTP) (`#/settings/smtp`) | Admin |

- Mỗi màn hình có URL riêng hỗ trợ reload, back/forward; ẩn menu không thay thế kiểm quyền server.
- Tổng quát lưu CMS Language (`cms_locale`) toàn site; màn hình Đa ngôn ngữ riêng quản lý ngôn ngữ nội dung, công tắc `multilingual_enabled`, xóa và dọn dữ liệu mồ côi. Tắt đa ngôn ngữ ẩn các công cụ ngôn ngữ phụ nhưng giữ dữ liệu và vẫn cho phép mở màn hình cấu hình này.
- Khu vực `Loại nội dung & Trường` chỉ dùng để cấu hình schema/field groups/editor layout, tuyệt đối không chứa danh sách hay form nhập liệu entry.
- Khi phiên hết hạn (`401`), `EditorDraftStore` quarantine bản nháp và kiểm tra lại quyền hiệu lực (`effective-editor-config`) sau khi đăng nhập lại; logout hoặc đổi tài khoản sẽ xóa sạch bản nháp.

## 3. Các Vùng của Màn hình Single Edit

Áp dụng cho `post`, `page` và các CPT trên desktop (`>= 1024px`):

```text
Thanh màn hình (header_bar): breadcrumb / tiêu đề / Xem trước / Đăng bài hoặc Lưu nháp / Cập nhật
┌──────────────────────────────────┬─────────────────────────┐
│ Tiêu đề nội dung, đường dẫn       │ Xuất bản / trạng thái   │
├──────────────────────────────────┼─────────────────────────┤
│ main_top (panel trên nội dung)    │ sidebar:                │
│ main_primary (nội dung chính /    │ - Thuộc tính / template │
│   mở Page-scoped Builder)         │ - Ảnh đại diện          │
│ main_bottom (panel dưới nội dung: │ - Taxonomy              │
│   nhóm custom fields / SEO)       │ - Nhóm field phụ /      │
│                                   │   Panel mở rộng         │
└──────────────────────────────────┴─────────────────────────┘
```

- Mobile (`< 1024px`) xếp thành 1 cột theo thứ tự `header_bar → main_top → main_primary → main_bottom → sidebar`.
- Các field thuộc nội dung dùng chung trạng thái chưa lưu (dirty state), validation và nút Lưu nguyên tử (`CompositeEntrySaveInput`); các panel hành động độc lập (`PanelKind::Action`) dùng nút và phản hồi riêng.
- Mở Visual Builder cho entry (`page`, CPT có bật Builder) đi vào OpenDash Studio Mode 2 tại `#/studio/entry/:typeKey/:id`, và chỉnh Pattern đi vào `#/studio/entry/pattern/:id` (chỉ Admin; `BuilderWorkspace` tại `#/builder` đã ngừng sử dụng, chỉ giữ HTTP redirect `GET /builder` → `/admin/#/studio` và chuyển hướng hash tương thích cho Pattern sang `#/studio/entry/pattern/:id`).
