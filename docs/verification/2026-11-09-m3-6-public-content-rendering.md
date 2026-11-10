# Kiểm chứng M3.6 — Public CPT Rendering

- **Ngày:** 2026-11-09.
- **Môi trường:** Mac Intel x86_64, Cargo/Rust 1.99.1, Python 3, Node.js 24.22.2, React 18/TypeScript 5.6.3/Vite 6.0.7.
- **Git baseline:** `4213bba feat(cms): add secure media library and image gallery picker`; commit M3.5 đã được tạo, M3.6 được thực hiện sau đó.
- **Kế hoạch:** [M3.6](../archive/plans/m3/2026-11-09-m3-6-public-content-rendering.md).

## Thay đổi

- `0007_content_templates.sql`: thêm cột nullable template CPT trong `theme_blocks`, giữ dữ liệu M1–M3.5. `theme.rs` mở rộng ZIP cũ thành tùy chọn 2 template và dùng chung MiniJinja HTML escape.
- `storage/public_content.rs`: lookup entry theo slug và kiểm tra media được tham chiếu qua schema-declared image/gallery. `content_render.rs` xây view model và template mặc định.
- `routes/public_content.rs`: HTTP archive, single và public image/thumbnail với CSP, no-store, nosniff; `app.rs` chỉ bổ sung route, không thay route `/`.
- `cms/admin/src/features/content/ContentWorkspace.tsx`: thêm link archive và trang chi tiết công khai, thông báo Save được cập nhật công khai.
- `cms/scripts/public_content_smoke.py`: smoke end-to-end trên site/database tạm riêng.

## Kết quả đã chạy

| Kiểm thử | Kết quả |
| --- | --- |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `cargo build --manifest-path cms/Cargo.toml --locked` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — **17 Core + 39 Server = 56 tests** |
| `python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash` | PASS — URL public, 404, archive pagination, theme ZIP 2/4 file, fallback, escape HTML, media reference ACL, live update, provider inactive, restart |
| `python3 cms/scripts/media_smoke.py --binary cms/target/debug/opendash` | PASS — M3.5 |
| `python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash` | PASS — M3.3 |
| `python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash` | PASS — M3.2 |
| `npm run typecheck` và `npm run build` trong `cms/admin/` | PASS — Vite production build |
| `python3 scripts/windows_auth_smoke.py --binary target/debug/opendash` (ở `cms/` trên Mac) | PASS — Auth/RBAC/Origin/upgrade fixture |
| `python3 scripts/windows_smoke.py --binary target/debug/opendash` (ở `cms/` trên Mac) | PASS — homepage/Builder M1 regression |
| CoS Playwright trên site + SQLite `/tmp/opendash-m36-ui.*` | PASS — trang single/archvie không cần cookie, HTML/script không thực thi, ảnh public tải `naturalWidth=2`, link Admin đến đúng slug và archive |
| Browser viewport 390px sau CSS update | PASS — archive không tràn ngang (`scrollWidth=390`) |

**Bằng chứng smoke chính:** không login vẫn GET chi tiết 200; không tồn tại CPT/slug 404; field rich text chứa `<script>`, `<img onerror>`, `{{7*7}}` được escape; Content-Type và CSP có `nosniff`, `script-src 'none'`. Public asset upload chưa tham chiếu 404, gallery/image có ref 200, text field chứa SHA không làm ảnh public, bỏ ref cuối thì 404. Theme ZIP chứa template CPT chỉ có hiệu lực khi Builder lưu block trang chủ tương ứng; đổi sang theme ZIP cũ sẽ fallback mà không làm mất entry. Dữ liệu vẫn hiện khi provider CPT chuyển thành inactive và sau restart.

**Trình duyệt thực tế:** site thử riêng sử dụng CPT `projects` và entry `first-project` với tiêu đề `<script>window.__xss=1</script>` và body chứa `<img onerror>`. Trên trang single, `document.scripts.length = 0`, `window.__xss` không được thiết lập, ảnh `/media/<id>/file` có `complete=true` và `naturalWidth=2`. Archive tải ảnh và link chính xác. Admin hiện nút `Xem danh sách công khai` và `Xem trang công khai (bản đã lưu)`. Sau khi thay CSS mặc định, archive 390px có `documentElement.scrollWidth=390` (không tràn).

## Giới hạn

- Chạy trên macOS, chưa kiểm thử bản phát hành Windows/Linux thực tế.
- Rich text chưa sanitization để render định dạng HTML; M3.6 hiển thị dưới dạng văn bản escape để đảm bảo an toàn.
- Public media ACL kiểm tra reference trực tiếp trong SQLite JSON; phù hợp slice ban đầu nhưng cần chỉ mục nếu nội dung lớn.
- Không có active-theme riêng ngoài `home_page.block_id`, không hỗ trợ preview các thay đổi entry chưa lưu.
