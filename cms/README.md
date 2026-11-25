# OpenDash CMS — Khu vực phát triển

Thư mục `cms/` tập trung mã nguồn, migrations (`0001`–`0041`), SDK, scripts và công cụ build của OpenDash. Tài liệu thiết kế nằm tại [`docs/`](../docs/README.md) ở gốc.

| Thành phần | Vị trí |
| --- | --- |
| Rust workspace, lockfile | [`Cargo.toml`](Cargo.toml), `Cargo.lock` |
| Domain models & validators thuần Rust | [`crates/opendash-core`](crates/opendash-core/README.md) |
| Axum HTTP + SQLite + MiniJinja + Wasmi + Backup | [`crates/opendash-server`](crates/opendash-server/README.md) |
| React Admin / Visual Builder | [`admin`](admin/package.json) ([features](admin/src/features/README.md)) |
| Plugin SDK (AssemblyScript) | [`sdk`](sdk/README.md) |
| Theme và plugin mẫu | [`examples/plugins/sample-plugin`](examples/plugins/sample-plugin/README.md), [`examples/themes/sample-theme-v3`](examples/themes/sample-theme-v3/README.md) |
| SQLite migrations và fixture nguồn | [`migrations`](migrations/README.md), [`fixtures`](fixtures/README.md) |
| Linux deploy configs (systemd/nginx) | [`deploy/`](deploy/install-linux.sh), [`opendash.service`](deploy/opendash.service) |
| Build/health/check/E2E scripts | [`scripts`](scripts/README.md) |
| Tài liệu, kiến trúc, hợp đồng, thiết kế EGM | [`docs`](../docs/README.md) |

Từ thư mục gốc repository:

```powershell
powershell -ExecutionPolicy Bypass -File cms/scripts/build.ps1
cargo run --manifest-path cms/Cargo.toml --locked -p opendash-server
```

Theme/plugin packages (`themes/revisions/`, `plugins/revisions/`), media (`uploads/`), backups (`backups/`), secrets (`database/secrets/`) và cấu hình (`config/site.json`) được quản lý qua `SitePaths` tại site root. Xem [README cấp gốc](../README.md).
