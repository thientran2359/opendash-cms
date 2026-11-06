# OpenDash scaffold — Kiểm chứng ngày 2026-11-06

**Phạm vi:** Scaffold trong `/opendash-cms` gồm 2 crate Rust, React Admin, SQLite và MiniJinja. Không đánh giá tính năng CMS V1 hoặc các module M1/M2 chưa nhập từ repository nguồn.

**Môi trường:** Windows, Rust/Cargo 1.99.1, Node.js 22.16.0, npm 10.9.2.

| Lệnh / kiểm tra | Kết quả | Ghi chú |
| --- | --- | --- |
| `cargo generate-lockfile` | PASS | Sinh `Cargo.lock` với 152 packages |
| `npm install --package-lock-only --ignore-scripts` | PASS | Sinh `admin/package-lock.json` |
| `npm ci` | PASS | Frontend dependency install |
| `npm run build` (trong `admin/`) | PASS | TypeScript check và Vite 8.3.6 build `admin/dist/` |
| `cargo fmt --all` rồi `cargo fmt --check` | PASS | Format toàn Rust workspace |
| `cargo test --workspace --locked` | PASS | 4 Rust unit tests trong Core, không có test failed |
| `cargo build --locked -p opendash-server` | PASS | Tạo `target/debug/opendash.exe` và nhúng Admin assets |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1` | PASS | Chạy npm ci + build Admin + Cargo build theo thứ tự |
| Smoke test chạy binary tại `127.0.0.1:18791` | PASS | Health JSON + SQLite SELECT 1, MiniJinja public page, Admin index, JS asset, builder placeholder đều HTTP 200 |

**Giới hạn:** `/api/health` là endpoint API duy nhất; Builder là mẫu giao diện, không có Save/Preview. Auth, RBAC, migration schema CMS, theme ZIP và plugin host Wasmi chưa được triển khai. File SQLite `data/scaffold-smoke.sqlite` sinh bởi smoke test được loại trừ theo `.gitignore`. Repository chưa có `.git`, vì vậy không chạy được `git diff --check` hay kiểm tra trạng thái commit. Chưa thử release Windows, binary Linux, backup/restore, nâng cấp DB từ fixtures hoặc kiểm thử plugin.
