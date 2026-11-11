# Kiểm chứng M3.5 — Media Library

- **Ngày:** 2026-11-09 (Asia/Bangkok).
- **Môi trường:** macOS Intel x86_64, Rust/Cargo 1.99.1, Python 3, Node.js 24.22.2, React 18, TypeScript 5.6.3, Vite 6.0.7.
- **Workspace:** `/local-sites/opendash-cms`; khởi đầu từ commit `9784b1b` của M3.4.
- **Phạm vi:** backend+frontend theo [kế hoạch M3.5](../plans/milestones/M3/slices/2026-11-09-m3-5-media-library.md), SQLite/site test nằm trong thư mục tạm.

## Các thay đổi

- Thêm `0006_media_assets.sql`, `storage/media.rs`, `media.rs` và `routes/media.rs`, wire vào `AppState`, routes và init. SQL được giữ trong `storage/`.
- Pin thư viện Rust `image = 0.25.10` (JPEG, PNG, WebP), cập nhật `cms/Cargo.lock`; giới hạn giải mã và chuẩn hóa trước khi ghi file, thumbnail PNG.
- M3.3 entry POST/PUT kiểm tra tồn tại các media IDs từ `image`/`gallery`; cập nhật regression smoke để upload ảnh thật thay cho mã giả.
- Thêm `admin/src/api/media.ts`, `features/media/MediaPicker.tsx` và các điều khiển chọn/thay/bỏ/sắp xếp ảnh trong field. Hình xem trước dùng URL nội bộ được server xây dựng; React không dùng raw HTML.

## Kết quả thực thi

| Kiểm tra | Kết quả |
| --- | --- |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `cargo build --manifest-path cms/Cargo.toml --locked` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — 17 Core + 38 Server tests, 55/55 |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server media::tests::accepts_and_reencodes_three_raster_formats --locked` | PASS — JPEG, PNG, WebP đều được re-encode và thumbnail PNG đúng định dạng |
| `python3 cms/scripts/media_smoke.py --binary cms/target/debug/opendash` | PASS — Auth, CSRF, Origin, định dạng/MIME, size/dimensions, upload, list, thumbnails, headers, duplicate, refs tồn tại, restart, migration 0006 |
| `python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash` | PASS — M3.3 regression đã upload real media refs |
| `python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash` | PASS — M3.2 regression |
| `python3 scripts/windows_auth_smoke.py --binary target/debug/opendash` (chạy ở `cms/` trên Mac) | PASS — Auth/CSRF/Origin và upgrade |
| `python3 scripts/windows_smoke.py --binary target/debug/opendash` (chạy ở `cms/` trên Mac) | PASS — Builder regression |
| `npm run typecheck`, `npm run build` trong `cms/admin/` | PASS — React/TypeScript/Vite build |
| Browser CoS Playwright tại website/SQLite tạm | PASS — Admin upload PNG (64×48), thumbnail thực tải được (naturalWidth=64), tạo CPT image+gallery, chọn cùng ảnh từ picker, lưu entry và kiểm tra phiên bản v1 |

**Đọc lại sau restart:** API smoke đã chứng minh media metadata, file thumbnail và refs tồn tại sau restart. Browser thử trên site mới hoàn toàn, không sử dụng database thực.

## Chưa kiểm chứng / giới hạn

- Không có Windows release/browser smoke, cũng chưa kiểm chứng backup/restore uploads hoặc khả năng xóa media đang được tham chiếu. Không hỗ trợ direct public URLs trong slice này.
- Browser test tập trung PNG và form image/gallery; JPEG/WebP được kiểm tra bằng Rust codec test, chưa thực hiện upload hai loại này qua trình duyệt.
- Core chỉ kiểm cú pháp opaque ID; authoritative existence validation nằm ở server cho write. Những entry cũ dùng ID giả cần chọn lại media hợp lệ khi sửa.
- Một upload đã thành công nhưng sau đó người dùng bỏ bản nháp vẫn giữ file trong Media Library; không tự động GC orphan metadata.
