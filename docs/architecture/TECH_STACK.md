# OpenDash — Tech Stack (Hiện hành)

Stack kỹ thuật của OpenDash được chốt và pin cứng phiên bản trong [`cms/Cargo.toml`](../../cms/Cargo.toml) và [`cms/admin/package.json`](../../cms/admin/package.json), nhằm đóng gói toàn bộ CMS thành **một file thực thi duy nhất** (`opendash` / `opendash.exe`).

## Stack đã triển khai

| Thành phần | Công nghệ & Phiên bản đã pin | Vai trò trong `cms/` |
|---|---|---|
| **HTTP server** | **Axum `=0.8.9` + Tokio `=1.54.1` + Tower** | Router, middleware xác thực (`AuthSession`, RBAC `Admin`/`Editor`), `Origin` / `X-CSRF-Token`, `SiteCoordinator` admission lease (`AdmittedMutation`). |
| **SQLite & Migrations** | **SQLx `=0.9.0`** (`sqlite`, `migrate`, `macros`) | Kết nối WAL + `foreign_keys = ON`, migrations nhúng (`0001`–`0053`, bỏ qua `0016`; nâng cấp thuận chiều và từ chối binary cũ trên schema mới), transaction `BEGIN IMMEDIATE`, `VACUUM INTO` snapshot. |
| **Admin & Visual Builder** | **React `=18.3.1` + TypeScript `=5.6.3` + Vite `=6.4.6` + Tailwind CSS `4.1.17`** | Giao diện quản trị 3 nhóm (`Nội dung`, `Giao diện`, `Tùy chỉnh`), Single Edit (`EditorDraftStore`), và Visual Builder 4 tầng (`Section → Row → Column → Element`) tự dựng (không dùng `dnd-kit` hay `Puck`). |
| **Theme & Public Render** | **MiniJinja `=3.0.5`** (`fuel`, `unstable_machinery`) | Template engine runtime với safe subset (`C-SEC`), `AutoEscape::Html`, fuel/recursion limit và trình chuẩn hóa HTML `document::normalize_public_document`. |
| **Plugin Runtime (Wasm)** | **Wasmi `=2.0.0`** (`wat = "=1.263.0"` cho test) + **Reqwest `=0.12.12`** (`rustls-tls`) | Thực thi plugin WebAssembly trong tiến trình worker cô lập (`--plugin-worker`), fuel/memory limits, `StagedCommitGuard`, và HTTP egress với DNS pinning chống SSRF. |
| **Plugin SDK** | **AssemblyScript (`cms/sdk/as/opendash.ts`)** | SDK biên dịch `.wasm` theo giao thức `opendash_v1` (`kv_get`, `kv_set`, `http_request`, `content_save`, `content/before_save`, `content/after_save`). |
| **Media & Đóng gói** | **Image `=0.25.10`** (`jpeg`, `png`, `webp`) + **Zip `=9.3.0`** + **RustEmbed `=8.6.0`** | Xử lý ảnh raster/thumbnail (strip EXIF), parser MP4 thuần Rust (`media_video.rs`), đọc ZIP an toàn (`package.rs`) và nhúng `cms/admin/dist/` vào binary. |
| **Bảo mật, Backup & Email** | **Argon2 `=0.5.3` + Age `=0.11.2` + Lettre `=0.11.23` + Subtle `=2.6.1` + Sha2 `=0.10.9`** | Hash mật khẩu Argon2id, mã hóa bản sao lưu bằng passphrase `age` (scrypt), gửi thông báo SMTP TLS/STARTTLS qua `lettre`. |

## Ghi chú lịch sử đánh giá

- Bản thử nghiệm Python ban đầu (`cms/spikes/stack-v1`, xem Git history) sử dụng Wasmtime 50.0.1 qua binding Python để chứng minh luồng fuel/memory trước khi chốt **Wasmi (`=2.0.0`)** thuần Rust.
- Các quy trình phản biện và kiểm chứng bản nhập M1/M2 ban đầu (`2026-11-07-m1-m2-import.md` và `2026-11-07-m2-5-core-kernel.md`) được lưu giữ trong Git history.
