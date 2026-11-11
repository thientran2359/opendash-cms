# M3.4 — Giao diện quản trị CPT và Content Entries

- **Verification theo slice (không phải task `Done`):** `VERIFIED` trong phạm vi frontend/backend tích hợp trên macOS; xem [báo cáo M3.4](../../../../verification/2026-11-08-m3-4-admin-content-ui.md). Chưa kiểm thử Windows release/browser; trạng thái task phải tra [GitHub Project #3](https://github.com/users/thientran2359/projects/3).
- **Hợp đồng:** [PRODUCT_REQUIREMENTS](../../../../product/PRODUCT_REQUIREMENTS.md), [CORE_FOUNDATION](../../../../contracts/CORE_FOUNDATION.md), [M3.1](2026-11-08-m3-content-types-and-fields.md), [M3.2](2026-11-08-m3-2-content-types-backend.md), [M3.3](2026-11-08-m3-3-content-entry-crud.md).
- **Mục đích:** chủ website và biên tập viên có thể quản lý CPT/entry bằng trình duyệt, dùng lại React Admin/Builder và API có sẵn, không phải nhập JSON trực tiếp.

## Hành vi và phạm vi

1. Bổ sung tab **Nội dung** bên cạnh Builder trong `AdminShell`. Tab Builder và các selector cũ giữ nguyên; tab Nội dung dùng layout riêng và responsive.
2. Admin tạo CPT với key, tên hiển thị và schema custom fields gồm 9 kiểu V1. Field `select` nhập danh sách `value|label` nhiều dòng. Admin và Editor cùng xem danh sách CPT; chỉ Admin được đăng ký schema mới.
3. Admin/Editor xem danh sách entry theo CPT, phân trang 50 entry/trang, tạo và sửa title/slug cùng fields, xóa có xác nhận. Các trường dùng form điều khiển theo schema từ backend.
4. Client gửi `X-CSRF-Token` qua `requestJson`/`requestText` hiện có; gửi `If-Match: "vN"` cho PUT/DELETE theo `entry.version`. Mỗi lần lưu thành công thay bản nháp bằng dữ liệu/phiên bản mới trả từ backend.
5. Khi server báo `401`, thông tin phiên được yêu cầu đăng nhập lại nhưng form vẫn được giữ trong component (không tự gửi lại POST/PUT). Sau đăng nhập cùng tài khoản, nội dung đang sửa vẫn còn. Nếu đăng nhập tài khoản khác, bản nháp cũ được bỏ để không lộ dữ liệu. `409` và `412` không xóa form; lỗi `412` cho lựa chọn tải bản mới (cần xác nhận bỏ bản nháp).
6. Cảnh báo trước khi rời trang khi có thay đổi chưa lưu, trước khi đổi entry/CPT khi có entry draft, trước khi đăng xuất và khi xóa nội dung. Đổi tab Builder/Nội dung không làm mất form.
7. Hiển thị trạng thái `provider_inactive` để người dùng biết nguồn CPT đã tắt nhưng vẫn có thể sửa entry.

## Hợp đồng field và an toàn

- `short_text` là input ngắn; `long_text`, `rich_text` là textarea; `number` gửi số JSON hữu hạn, `boolean` gửi true/false (không nhầm thành chuỗi), `date` gửi YYYY-MM-DD, `select` chỉ chọn option, `image` gửi một mã media và `gallery` gửi mảng mã media.
- Rich text chỉ hiển thị **dưới dạng văn bản trong textarea**; không dùng `dangerouslySetInnerHTML` hay chạy script nhập vào. Việc render/sanitize HTML đúng ngữ cảnh thuộc slice renderer sau.
- Image/gallery hiện **chỉ nhập ID**; Media Library, chọn tệp và kiểm tra tệp tồn tại chưa có ở M3.4. Mọi dữ liệu vẫn được server kiểm tra theo M3.3, form không thay cho backend guard.
- API tạo/sửa schema sau khi đăng ký, taxonomy, renderer CPT công khai, revision, publishing workflow và media picker **ngoài phạm vi**.

## Tiêu chí nghiệm thu

- `npm run typecheck`, `npm run build` trong `cms/admin/`; `cargo fmt --check`, `cargo build --locked`, `cargo test --workspace --locked`.
- Browser test tại một site/database tạm: Admin tạo CPT chín kiểu field, lưu entry (số 0, boolean false, ngày nhuận, select, media IDs); raw rich text không thực thi. Editor không có nút tạo CPT nhưng sửa entry được.
- Test hai phiên bản cùng một entry: bản cũ PUT bị 412, bản nháp không mất; xem ở hai tab không xóa form. Hết hạn phiên 401, đăng nhập lại cùng tài khoản và lưu bản nháp. Kiểm tra responsive 390px không tràn ngang.
- Chạy lại smoke tests M3.2/M3.3 và regressions Auth/Builder; ghi rõ các giới hạn trên ở báo cáo.
