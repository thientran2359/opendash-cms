# OpenDash Server (`opendash-server`)

Binary thực thi duy nhất của OpenDash (`[[bin]] name = "opendash"`), phụ thuộc vào [`opendash-core`](../opendash-core/README.md) và nhúng bản build tĩnh của React Admin (`cms/admin/dist/`) thông qua `rust-embed`.

## Cấu trúc Thư mục con

- [`src/routes/`](src/routes/README.md): 22 module Axum HTTP handlers (auth/RBAC, CSRF, `AdmittedMutation`, không chứa SQL).
- [`src/storage/`](src/storage/README.md): 31 module SQLx/SQLite (sở hữu 100% truy vấn SQL và migrations `0001`–`0041`).
- [`src/plugin/`](src/plugin/README.md): Host runtime Wasmi (`=2.0.0`), worker cô lập `--plugin-worker`, `host_ops` và DNS-pinned HTTP egress.
- `tests/`: Bộ integration test cho toàn bộ các mốc M3–M8, Admin Navigation, Owner Admin Review và Content Foundation.

## Các Module Cấp cao chính (`src/*.rs`)

- `main.rs`, `app.rs`, `error.rs`, `config.rs`, `paths.rs`: Entrypoint (`--plugin-worker`, `--operations-worker`, HTTP server), `AppState`, router/middleware, `ApiError`, `RuntimeConfigV1` và `SitePaths`.
- `auth.rs`, `hooks.rs`, `package.rs`, `secrets.rs`: Session/Argon2/CSRF/Origin, `HookDispatcher`, trình đọc ZIP an toàn (Theme v1/v2, Plugin, Backup) và `SecretStore`.
- `page.rs`, `theme.rs`, `template_sec.rs`, `public_context.rs`, `content_render.rs`, `document.rs`, `seo.rs`: MiniJinja `C-SEC` safe subset, `PublicRequestContext`, render public/preview và `normalize_public_document`.
- `media.rs`, `media_video.rs`, `media_lifecycle.rs`: Xử lý ảnh raster/thumbnail, parser MP4 thuần Rust và vòng đời xóa/quarantine media.
- `contact.rs`, `smtp.rs`: Contact form validation/dedup/rate-limit và gửi thông báo SMTP qua `lettre`.
- `operations.rs`, `operations_worker_process.rs`, `backup.rs`, `backup_archive.rs`, `backup_worker.rs`, `restore.rs`: `SiteCoordinator`, khóa bảo trì, snapshot `VACUUM INTO`, mã hóa `age` và phục hồi sang site root mới (Safe Mode).
