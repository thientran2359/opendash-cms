# Kiểm chứng M3.4 — Admin Content UI

- **Ngày:** 2026-11-08
- **Môi trường:** macOS Intel (Darwin), Rust/Cargo 1.99.1, Node.js 24.22.2, React 18.3.1, TypeScript 5.6.3, Vite 6.0.7.
- **Code:** `/local-sites/opendash-cms`, sau commit backend `7defd2d`.
- **Phạm vi:** [kế hoạch M3.4](../plans/milestones/M3/slices/2026-11-08-m3-4-admin-content-ui.md), giao diện React Admin và tích hợp backend M3.2/3.

## Mã và luồng đã thêm

- `cms/admin/src/features/content/ContentWorkspace.tsx`: màn hình CPT + entry, form state, pagination, lưu/xóa, cảnh báo và bảo toàn draft.
- `cms/admin/src/features/content/ContentTypeForm.tsx`: tạo CPT và custom fields/schema (Admin only).
- `cms/admin/src/features/content/FieldValueInput.tsx`: 9 kiểu field có form nhập đúng kiểu.
- `cms/admin/src/features/content/types.ts`, `cms/admin/src/api/content.ts`: kiểu wire JSON và client gọi API.
- `cms/admin/src/api/client.ts`: bổ sung `ifMatchVersion` cho các yêu cầu PUT/DELETE, tiếp tục dùng common headers Auth/CSRF và `ApiClientError`.
- `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/App.tsx`, `cms/admin/src/styles.css`, `cms/admin/src/components/FormPrimitives.tsx`: tab Nội dung, giữ Builder hoạt động độc lập, form primitives mở rộng và giao diện responsive.

## Kiểm chứng thực tế trên máy Mac

| Lệnh/kiểm tra | Kết quả |
| --- | --- |
| `npm run typecheck` trong `cms/admin/` (Node v24.22.2) | PASS |
| `npm run build` trong `cms/admin/` | PASS — TypeScript và Vite production build |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `cargo build --manifest-path cms/Cargo.toml --locked` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — 17 Core + 34 Server unit tests |
| Browser qua CoS Playwright tại `http://127.0.0.1:<cổng tạm>/admin/` với SQLite trong `/tmp` | PASS — thao tác được Admin tạo CPT 9 field types, entry có số 0, boolean false, ngày nhuận, select, image/gallery ID; lưu, mở lại và form hiển thị dữ liệu |
| Browser xung đột `If-Match` sau cập nhật bằng client thứ hai | PASS — 412; bản nháp còn và tab Builder/Nội dung không xóa dữ liệu; nút tải bản mới có xác nhận |
| Browser hết hạn phiên 401 khi lưu, đăng nhập lại | PASS — draft title còn, lưu tiếp thành công (version từ 2 lên 3) |
| Browser login Editor | PASS — không có nút tạo CPT, mở/sửa/lưu entry (version lên 4) |
| Browser rich-text test với `<script>` và `<img onerror>` | PASS — chỉ tồn tại như chuỗi trong textarea, không thực thi trong trình duyệt kiểm thử |
| Browser viewport 390px | PASS — layout một cột, `documentElement.scrollWidth <= innerWidth` |

Các bước browser dùng server/backend với `OPENDASH_SITE_ROOT` và `OPENDASH_DB_PATH` riêng cho website thử nghiệm; không tác động cơ sở dữ liệu website đang hoạt động. Bản build Admin được nhúng lại vào binary trước khi kiểm thử. Console có lỗi HTTP 401 theo test chủ động làm hết hạn phiên và 404 favicon; không có JS exception gây dừng UI.

## Giới hạn/chưa kiểm chứng

- Không có Media Library/picker và kiểm tra ID media tồn tại; không render raw HTML từ rich text.
- Lưu entry chỉ cập nhật SQLite, chưa xuất bản thành trang công khai do template/renderer CPT chưa nối.
- Chưa có thao tác sửa/xóa schema CPT hay taxonomy, bản nháp/phát hành riêng và quản lý revision.
- Browser test dùng plugin Playwright trên macOS, chưa chạy full Windows browser regression, kiểm thử Chromium trên Linux hay kiểm thử mọi tổ hợp 64 fields/512 KiB.
