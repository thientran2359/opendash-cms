# Sample Theme — draft

`templates/hero.html` minh họa cú pháp MiniJinja. Các biến `title` và `description` là dữ liệu demo cần được escape theo ngữ cảnh khi render.

Để thử luồng cài theme M1, dùng ZIP có `block.json` và `block.html` được tạo bằng [script smoke](../../../scripts/windows_smoke.py).

## Theme v2 — ví dụ khai báo C4

Đây là ví dụ **cho một gói theme v2 riêng**, không phải `theme.json`/ZIP đã có trong thư mục draft này. Theme v2 dùng `manifest_version: 2` (khác `manifest_schema: 2` của plugin). Khi đóng gói cần có `theme.json` cùng file `templates/home.html` mà manifest tham chiếu.

```json
{
  "manifest_version": 2,
  "theme_id": "sample-c4-theme",
  "name": "Sample C4 Theme",
  "version": "1.0.0",
  "templates": {
    "home": "templates/home.html"
  },
  "declarations": {
    "taxonomy_definitions": [
      {
        "key": "art_period",
        "label": "Giai đoạn",
        "hierarchical": false,
        "public": true,
        "rewrite_base": "art-periods"
      }
    ],
    "content_types": [
      {
        "key": "gallery_item",
        "label": "Tác phẩm",
        "fields": [],
        "taxonomies": ["art_period", "category"],
        "settings": {
          "public": true,
          "has_archive": true,
          "publication": "draft_publish",
          "supports": ["editor", "featured_image"]
        },
        "rewrite_base": "gallery/works"
      }
    ],
    "taxonomy_eligibility": [
      { "content_type": "gallery_item", "taxonomies": ["tag"] }
    ]
  }
}
```

`taxonomy_definitions`, CPT `settings`, `taxonomies`, `taxonomy_eligibility` và `rewrite_base` đều có thể bỏ qua để giữ hành vi của gói cũ. Với CPT thông thường thiếu `settings`, core dùng public/archive, `immediate` và hai supports theo default legacy. Taxonomy thiếu `hierarchical` mặc định `false`, thiếu `public` mặc định `true`.

Theme chỉ khai báo cấu trúc và giá trị mặc định; **không có Wasmi hook hoặc action**. Core/registry bảo vệ ownership của taxonomy/CPT khi đổi theme hoặc cập nhật package; site override về taxonomy eligibility và permalink được ưu tiên. C3 kiểm tra base 1–3 đoạn kebab-case, root hệ thống, xung đột với URL khác và redirects theo transaction; không có filter URL tùy ý. Các entry/terms/fields đã nhập tiếp tục được lưu theo chính sách provider, kể cả khi theme không còn active.

Với `publication: draft_publish`, tác phẩm nháp chỉ hiển thị qua preview có quyền và không xuất hiện công khai trước khi xuất bản. Một theme không được dùng `rewrite_base` riêng cho `post`/`page`/`section`. Xem [bảng hỗ trợ C4 trong SDK](../../../sdk/README.md), [plugin Wasmi mẫu](../../plugins/sample-plugin/README.md) và [contract C4](../../../../docs/contracts/CONTENT_FOUNDATION_2026_11.md).
