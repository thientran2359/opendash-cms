# Demo Website Fixture — Mộc Studio

Bộ fixture và script tự động dựng website mẫu **“Mộc Studio — Thiết kế không gian sống”** trên một site root cô lập trong `runtime/demo-website/` (được `.gitignore` bỏ qua, không đụng `database/`, `themes/`, `plugins/`, `uploads/` ở root).

## Nguồn gốc & bản quyền tài nguyên

- **12 ảnh minh họa kiến trúc/nội thất (`1200x800` PNG):** Được sinh cục bộ bằng hàm Python thuần (`zlib` + `struct`) trong `cms/scripts/demo-website-runner.py` với bảng màu, hình khối kiến trúc và ánh sáng khác nhau cho từng dự án/dịch vụ. Không tải từ bên ngoài, không dính bản quyền bên thứ ba.
- **Video MP4 (`video-avc-aac.mp4`):** Dùng lại fixture nội bộ `cms/fixtures/m7/video-avc-aac.mp4` của repo.
- **Plugin mẫu (`sample-plugin.zip`):** Dùng lại `cms/examples/plugins/sample-plugin/sample-plugin.zip` của repo.
- **Theme V2 (`theme-moc-studio/`):** Tự soạn mới cho demo này, đóng gói thành ZIP khi seed và cài qua `POST /api/admin/themes`.

## Cách chạy nhanh (Windows PowerShell)

1. Build binary nếu chưa có:
   ```powershell
   cargo build --manifest-path cms/Cargo.toml --locked
   ```
2. Chạy toàn bộ luồng `seed` -> `smoke` -> `capture` vào một run cô lập:
   ```powershell
   python cms/scripts/demo-website-runner.py all --run-name v1-moc-studio-win
   ```
   Lệnh trên tạo site tại `runtime/demo-website/v1-moc-studio-win/`, seed đầy đủ dữ liệu, chạy toàn bộ kiểm chứng HTTP + Playwright và xuất ảnh vào `screenshots/demo-website/v1-moc-studio-win/`.
3. Chạy từng bước riêng lẻ:
   - `python cms/scripts/demo-website-runner.py seed --run-name demo-local`
   - `python cms/scripts/demo-website-runner.py smoke --run-name demo-local`
   - `python cms/scripts/demo-website-runner.py capture --run-name demo-local --out-dir screenshots/demo-website/demo-local`
   - `python cms/scripts/demo-website-runner.py reset --run-name demo-local`
4. Giữ server demo chạy để mở trình duyệt tự kiểm tra:
   ```powershell
   python cms/scripts/demo-website-runner.py serve --run-name demo-local --port 8787
   ```
   Sau đó mở `http://127.0.0.1:8787/` (public) hoặc `http://127.0.0.1:8787/admin` (admin).
5. `python dev.py` tại root repository tự dùng demo `runtime/demo-website/dev-local/`
   (lần đầu seed, lần sau giữ nguyên dữ liệu). Admin demo:
   `admin_moc` / `MocStudioDemo#2026!`. Để trở lại site data tại root, chạy
   `python dev.py --no-demo`. Nếu muốn đổi dữ liệu demo về bản gốc, **hãy sao lưu
   thay đổi trước**, đóng launcher rồi chạy
   `python cms/scripts/demo-website-runner.py reset --run-name dev-local`;
   lần tiếp theo `python dev.py` sẽ tự tạo lại site.

## Quy tắc an toàn

- Không commit `runtime/demo-website/`, file `.sqlite`, gói `.odbackup.age`, mật khẩu hay cookie phiên.
- Khi thay đổi dữ liệu mẫu hoặc thêm màn hình mới, cập nhật `cms/fixtures/demo-website/screen-inventory.md` và chạy lại 2 lần trên 2 `--run-name` sạch khác nhau để đối chiếu tính lặp lại.
