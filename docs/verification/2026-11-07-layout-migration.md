# Chuyển cấu trúc `cms/` và đổi `data/` thành `database/` — 07/11/2026

**Phạm vi:** Di chuyển thư mục thực tế trong `/opendash-cms`, cập nhật đường dẫn Rust, scripts, docs, AI workflow và bảo toàn hai tệp SQLite hiện có. Chức năng CMS V1 chưa được triển khai thêm.

## Các thay đổi

- Chuyển `admin/`, `crates/`, `docs/`, `examples/`, `fixtures/`, `migrations/`, `scripts/`, `sdk/`, `spikes/`, `target/`, `Cargo.toml` và `Cargo.lock` vào `cms/`.
- Đổi thư mục cấp gốc `data/` thành `database/`; giữ `themes/`, `plugins/`, `uploads/` và `.agents/` ở gốc.
- Đổi vị trí mặc định database thành `database/opendash.sqlite` trong site root. Server nhận diện trường hợp chạy từ `opendash-cms/` hoặc `cms/`; cấu hình `OPENDASH_DB_PATH` và `OPENDASH_SITE_ROOT` vẫn có thể ghi đè đường dẫn.
- Cập nhật `.gitignore` để bỏ qua `cms/target/`, `cms/admin/node_modules/`, `cms/admin/dist/` và file database, vẫn giữ `database/README.md`.
- Chuyển và sửa các liên kết Markdown nội bộ, cập nhật README, ADR, architecture, script và hướng dẫn phối hợp Codex–Antigravity.

## Kiểm chứng thực hiện

| Kiểm tra | Kết quả |
| --- | --- |
| `PRAGMA quick_check` trước/sau chuyển trên `opendash.sqlite` và `scaffold-smoke.sqlite` | PASS |
| SHA-256 từng file SQLite trước/sau đổi thư mục | MATCH — dữ liệu giữ nguyên tại thời điểm chuyển |
| `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1` | PASS — `npm ci`, TypeScript, Vite, `cargo build --locked` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — 4 test Rust |
| `cargo metadata --manifest-path cms/Cargo.toml --locked --no-deps --format-version 1` | PASS |
| HTTP smoke từ root và từ `cms/` | PASS — `/api/health`, `/`, `/admin/`, Admin JS embedded |
| Không tạo database dưới `cms/` khi chạy ở hai working directory | PASS |
| Liên kết Markdown nội bộ sau khi dời và cập nhật tài liệu | PASS — 143 link, 0 link hỏng |

**Giới hạn:** Repository này chưa có `.git`, do đó `git diff --check` không áp dụng. Chưa kiểm chứng bản Linux, ZIP installer, plugin Wasmi, backup/restore hoặc luồng CMS M1/M2 đã triển khai trong repository nguồn. Báo cáo ngày 06/11/2026 vẫn là lịch sử trước khi đổi cấu trúc.
