# Báo cáo kiểm chứng Demo Website V1 trên Windows — Mộc Studio (`#258`)

- **Issue:** `#258` — `[Demo V1] Nova dựng website mẫu đầy đủ và chụp toàn bộ màn hình vào screenshots/`
- **Agent thực hiện:** Nova (Windows)
- **Commit nền (`main`):** `3eca45c`
- **Binary kiểm chứng:** `cms/target/debug/opendash.exe` (build qua `cargo build --manifest-path cms/Cargo.toml --locked`)
- **Trạng thái:** `VERIFIED` (với 1 ghi nhận trung thực về `uploads/media/.staging` ở mục Giới hạn)

## 1. Phạm vi & tài nguyên đã dựng

1. **Theme V2 `moc-studio`:** [cms/fixtures/demo-website/theme-moc-studio/](../../cms/fixtures/demo-website/theme-moc-studio/theme.json) (`manifest_version: 2`, đủ 7 role template: `header`, `footer`, `home`, `page`, `archive`, `single`, `taxonomy_archive`, kèm `options` và `menus: ["primary", "footer"]`).
2. **Danh mục màn hình & hướng dẫn:** [screen-inventory.md](../../cms/fixtures/demo-website/screen-inventory.md) và [README.md](../../cms/fixtures/demo-website/README.md).
3. **Script tự động hóa cô lập:** [demo-website-runner.py](../../cms/scripts/demo-website-runner.py) hỗ trợ `seed`, `smoke`, `capture`, `all`, `serve`, `reset` trên `runtime/demo-website/<run-name>/` (không đụng `database/`, `themes/`, `plugins/`, `uploads/` ở root).
4. **Dữ liệu mẫu “Mộc Studio — Thiết kế không gian sống”:**
   - 12 ảnh kiến trúc/nội thất `1200x800` PNG sinh cục bộ + 1 video MP4 (`cms/fixtures/m7/video-avc-aac.mp4`).
   - Theme V2 `moc-studio` cài từ ZIP, kích hoạt và cấu hình Theme Options; `sample-plugin.zip` cài và chạy action `ping`.
   - Cài đặt chung (`Asia/Ho_Chi_Minh`), cài đặt công khai (`vi`, indexing bật), Home SEO, Menu `primary`/`footer` gán vào slot.
   - 4 category phân cấp (`kien-truc` -> `nha-pho`, `noi-that` -> `can-ho`), 4 tag (`go-tu-nhien`, `toi-gian`, `anh-sang`, `ben-vung`).
   - CPT `dich_vu` (6 bản ghi) và `du_an` (8 bản ghi, phủ đủ 9 nhóm field V1: `short_text`, `long_text`, `rich_text`, `number`, `boolean`, `date`, `select`, `image`, `gallery`).
   - 12 bài viết `published` + 2 bài `draft`, 2 trang (`gioi-thieu`, `lien-he`), 1 Contact Form (`tu-van-thiet-ke`) và Builder homepage dùng `core/heading`, `core/text`, `core/button`, `core/image`, `core/list`, `core/video`, `core/contact_form`.
   - Tài khoản `admin_moc` (`admin`) và `editor_moc` (`editor`).
5. **Bộ ảnh chụp & manifest:** [screenshots/demo-website/v1-moc-studio-win/](../../screenshots/demo-website/v1-moc-studio-win/README.md) gồm **29 ảnh PNG** (`pub-01..pub-14`, `err-01..err-02`, `adm-01..adm-13`) và `manifest.json`.

## 2. Lệnh thực chạy & kết quả

```powershell
python -u cms/scripts/demo-website-runner.py all --run-name v1-moc-studio-win
```

- **Public & Responsive (`1440x900`, `768x1024`, `390x844`):** `PUB-01`..`PUB-14` đều trả HTTP `200`, `0` ảnh vỡ (`naturalWidth > 0`), `0` tràn ngang (`scrollWidth <= innerWidth + 1`), `0` `pageerror`/`console.error` ngoài `/favicon.ico`.
- **Phân trang & Draft isolation:** `/content/post?page=2` (`PUB-09`) hoạt động thật; bài nháp không lộ trên archive và trả HTTP `404` (`ERR-02`); URL không tồn tại trả `404` (`ERR-01`).
- **Video Range 206:** `GET /media/{video_id}/file` với `Range: bytes=0-1023` trả `206 Partial Content`.
- **Contact Form E2E:** Điền và gửi form trực tiếp trên `/` qua Playwright chuyển hướng `/api/public/contact/received` và xác minh bản ghi có trong `/api/admin/contact/submissions/{id}`.
- **Admin UI, Unsaved Preview, RBAC & Referenced Delete:**
  - `ADM-01`..`ADM-12` chụp đầy đủ trạng thái login và 11 view Admin (`dashboard`, `posts`, `pages`, `content`, `media`, `taxonomy`, `themes`, `builder`, `plugins`, `users`, `settings`).
  - `POST /api/admin/builder/homepage/preview` không làm thay đổi trang công khai `/`.
  - `editor_moc` vào được `/admin#/posts` (`ADM-13`) và bị chặn `403` trên `/api/admin/themes`, `/api/admin/users`.
  - Xóa term đang được dùng (`kien-truc`) trả `409`.
- **Tính lặp lại 2 lần sạch:** Đã chạy 2 lượt độc lập (`v1-moc-studio-win` và `v1-moc-studio-win-r2`), cả hai đều seed sạch và xuất đủ 29 ảnh khớp danh mục.

## 3. Giới hạn & phát hiện thực tế

- **Backup & Diagnostics khi có media upload:** Sau khi upload ảnh/video qua `/api/admin/media`, server để lại thư mục rỗng `uploads/media/.staging`. Khi `backup_worker.rs` hoặc `/api/admin/operations/diagnostics` quét `uploads/media/`, cả hai trả lỗi `409` (`Phát hiện đối tượng lạ trong uploads/media.`), khiến bước backup trong `demo-website-runner.py` ghi nhận trạng thái `failed` (`snapshot_failed`) và `ADM-12` ghi nhận `409` trên `/api/admin/operations/diagnostics`. Đã ghi nhận trung thực trong `manifest.json` và không sửa core trong phạm vi `#258`.
