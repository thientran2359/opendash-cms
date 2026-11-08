# M3.6 — Public CPT Content Rendering

- **Trạng thái:** `VERIFIED` trong phạm vi SSR/API và kiểm thử trên macOS; xem [báo cáo M3.6](../verification/2026-11-09-m3-6-public-content-rendering.md).
- **Hợp đồng:** [Yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md), [Core Foundation](../contracts/CORE_FOUNDATION.md), [kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) và các mốc [M3.3](2026-11-08-m3-3-content-entry-crud.md)–[M3.5](2026-11-09-m3-5-media-library.md).
- **Mục tiêu:** bài/entry lưu bằng Admin được hiển thị công khai theo CPT và slug, dùng template có escape mặc định, có hình ảnh đã được tham chiếu; không ảnh hưởng giao diện Builder hay homepage `/`.

## HTTP và URL

| Route | Hành vi |
| --- | --- |
| `GET /content/{type_key}` | Trang danh sách (archive) của CPT, 20 entry/trang, `?page=N` từ 1 đến 10000; HTML hoặc 404 nếu CPT không có |
| `GET /content/{type_key}/{slug}` | Nội dung chi tiết (single), định danh CPT snake_case + slug kebab-case, trả HTML hoặc 404 |
| `GET /media/{id}/file` | Ảnh gốc chuẩn hóa chỉ khi có entry **đang lưu** tham chiếu từ field schema kiểu `image`/`gallery` |
| `GET /media/{id}/thumbnail` | Thumbnail PNG với cùng ACL tham chiếu |
| `GET /` | **Giữ nguyên** homepage hiện tại của Builder |

Không cần đăng nhập để xem nội dung công khai. Admin/Editor tiếp tục quản lý và upload qua `/api/admin/...` (yêu cầu phiên/CSRF). Entry hiện **không có trạng thái nháp/xuất bản riêng**: Save thành công cập nhật ngay nội dung website.

## Template & dữ liệu

- Renderer MiniJinja dùng **HTML auto-escape**, Strict undefined và fuel budget của `theme.rs`; tránh thực thi template delimiters nằm trong dữ liệu user. Custom field `rich_text` được render như **văn bản HTML-escaped**, giữ newline qua `white-space: pre-wrap`; chưa cho phép HTML rich formatting.
- Archive/single dùng template mặc định có sẵn khi chưa có template theme tương ứng.
- Theme ZIP trước đây có `block.json` và `block.html` vẫn cài như cũ. Theme mới có thể kèm thêm `content-list.html` và/hoặc `content-single.html` (tối đa 4 file, tổng kích thước vẫn theo ZIP M1); template được validate cú pháp khi cài.
- Migration bổ sung **`0007_content_templates.sql`** thêm hai cột nullable vào `theme_blocks` và không sửa migrations 0001–0006. Các template của block hiện được chọn từ `home_page.block_id` khi Builder đã lưu trang chủ. Đây là quy ước tạm thời, **chưa phải cơ chế active theme riêng**; nếu thay block trang chủ, template CPT thay theo, trong khi dữ liệu CPT/entry vẫn giữ.
- Context cho `content-list.html`: `content_type`, `entries` (mỗi entry có `id/title/slug/url/updated_at/values/fields/cover_url`), `previous_url`, `next_url`. Context cho `content-single.html`: `content_type`, `entry` và `archive_url`. `entry.fields` chứa `key/label/kind/text/image_url/gallery_urls`; URL media được server tạo từ ID chuẩn, không đọc URL user nhập. Theme không nên dùng filter `safe` cho dữ liệu người dùng.

## An toàn và tương thích

- SQL route public chỉ đặt trong `storage/public_content.rs` và các `storage/` có sẵn; `GET /media/{id}` yêu cầu asset tồn tại **và** được tham chiếu bởi field `image`/`gallery` đã khai báo, không tính chuỗi ID nằm ở field text hoặc ảnh đã upload nhưng chưa được chọn.
- Public HTML CSP chặn script/plugin/form/iframe, chỉ cho phép ảnh same-origin và CSS inline của template mặc định; `nosniff`, `no-referrer`, `no-store`. Public media `no-store` để ảnh ngừng public ngay sau khi bỏ tham chiếu.
- CPT `provider_inactive` vẫn render được bằng fallback khi mất template; việc đổi/tắt theme/plugin không xóa nội dung site.
- Template ZIP nhập chỉ bởi Admin, người dùng không được biên dịch nội dung thành mã template. HTML output M3.6 là SSR, không thay đổi `/preview` và `/save` Builder.

## Nghiệm thu

1. Cargo fmt/build/workspace unit tests và typecheck/Vite build đạt. Chạy `python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash` trên site tạm.
2. Public 200/404, archive phân trang, XSS/rich-text template injection escaped, chỉnh sửa phản ánh tức thì, thông tin CPT không mất qua restart hoặc `provider_inactive`.
3. Public media: upload chưa gắn entry → 404; field text không công bố ảnh; gắn image/gallery → 200; bỏ một ref vẫn 200 nếu có ref khác; bỏ ref cuối → 404.
4. Old 2-file theme ZIP + new 4-file ZIP, fallback nếu theme chưa kích hoạt hoặc thiếu template, không phá Builder homepage/preview; các smoke M3.2–M3.5 tiếp tục đạt.

## Ngoài phạm vi

- Active theme registry riêng, Builder cây archive/single, preview nội dung chưa lưu và sửa template trực tiếp trong UI.
- HTML rich-text sanitization/formatting, SEO canonical/sitemap, taxonomy, tìm kiếm, revision, SEO image caption/alt metadata và dashboard analytics.
- Tối ưu chỉ mục tham chiếu media (hiện kiểm tra JSON field schema khi GET), cache HTML và tổng hợp render artifacts thuộc lát cắt sau.
