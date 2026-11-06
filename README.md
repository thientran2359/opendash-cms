# OpenDash

CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress. Rust, SQLite, một file thực thi cho mỗi bản cài và hệ theme/plugin riêng.

**Hiện trạng (07/11/2026):** Khung Rust + React Admin đã được tổ chức dưới [`cms/`](cms/README.md). Các thư mục website `themes/`, `plugins/`, `uploads/`, `database/` nằm trực tiếp tại gốc. [Báo cáo build ban đầu](docs/verification/2026-11-06-scaffold-build.md) áp dụng cho scaffold mẫu; các tính năng CMS V1 như CPT, Builder thực, phân quyền, theme ZIP, plugin Wasmi và backup chưa được triển khai. Source M1/M2 từ repository nguồn chưa nhập vào đây. Commit đầu tiên trên nhánh `main` lưu scaffold làm mốc đối chiếu trước khi nhập M1/M2.

## Cấu trúc thư mục

**Phân chia:** mã nguồn và công cụ phát triển nằm trong [`cms/`](cms/README.md), tài liệu nằm ở [`docs/`](docs/README.md), còn dữ liệu website được đặt trực tiếp ở root. `.agents/`, `AGENTS.md` và `.gitignore` cũng ở root để các AI và Git dùng chung. [Kiểm kê trước khi tạo scaffold](docs/verification/2026-11-06-repository-inventory.md).

**Cấu trúc đã tạo và hướng mở rộng** (xem [ADR-0001](docs/decisions/ADR-0001-workspace-layout.md)):

| Thư mục | Nội dung |
|---|---|
| [`cms/`](cms/README.md) | Mã nguồn và công cụ phát triển: `crates/`, `admin/`, `sdk/`, `examples/`, `migrations/`, `fixtures/`, `scripts/`, `spikes/`; chứa `Cargo.toml` và `Cargo.lock` |
| [`docs/`](docs/README.md) | Tài liệu sản phẩm, kiến trúc, contracts, quyết định, kế hoạch và kiểm chứng |
| [`themes/`](themes/README.md) | Theme đã cài của website; từng theme nằm trong `themes/<theme-id>/` khi CMS hỗ trợ cài ZIP |
| [`plugins/`](plugins/README.md) | Plugin đã cài của website; từng plugin nằm trong `plugins/<plugin-id>/` khi CMS hỗ trợ cài ZIP |
| [`uploads/`](uploads/README.md) | Ảnh/video và media người dùng tải lên; nội dung không commit vào Git |
| [`database/`](database/README.md) | SQLite `opendash.sqlite`, file WAL/SHM; dữ liệu của website, được giữ nguyên khi đổi từ `data/` |
| `.agents/` | Workflow Codex ↔ Antigravity; `.agents/sync/` là dữ liệu tạm và được ignore |

Đã tạo code mẫu trong `cms/crates/opendash-core/src/`, `cms/crates/opendash-server/src/` và `cms/admin/src/`. Các module Wasmi và CPT/Builder thực sẽ được triển khai theo các mốc tương ứng.

Các thư mục `themes/`, `plugins/`, `uploads/` và `database/` nằm ngay dưới `opendash-cms/`, cách biệt với `cms/` để dễ tìm. Khi phát hành, chúng dự kiến nằm cùng cấp file thực thi `opendash`. Scaffold hiện chưa tự động quét/cài theme, plugin hoặc nhận upload. Git bỏ qua dữ liệu website và giữ README hướng dẫn.

## Bắt đầu ở đâu

1. Đọc [yêu cầu sản phẩm](docs/product/PRODUCT_REQUIREMENTS.md) và [đánh giá tech stack](docs/architecture/TECH_STACK.md) để biết phạm vi sản phẩm và stack đã được chủ dự án xác nhận.
2. Đọc [kiến trúc CMS và cấu trúc thư mục](docs/architecture/CMS_ARCHITECTURE.md) cùng [đề xuất CMS-first](docs/architecture/CMS_PROPOSAL.md) để nắm ranh giới module trước khi triển khai.
3. Dùng [mục lục docs](docs/README.md) để phân biệt thiết kế đã duyệt, kế hoạch và bằng chứng kiểm chứng. Khi làm việc bằng AI, đọc [AGENTS.md](AGENTS.md) và [giao thức thảo luận](docs/discussions/PROTOCOL.md).
4. Khi nhập mã nguồn CMS M1/M2 đã có, theo [kế hoạch nhập mã](docs/plans/README.md) và ghi bằng chứng mới vào [verification](docs/verification/README.md). Không thay thế migrations hay plugin runtime đã kiểm chứng bằng mã mẫu.

## Bộ tài liệu

| Nhóm vai trò | Tài liệu | Nội dung và trạng thái |
|---|---|---|
| **Yêu cầu & Stack đã chốt** | [Yêu cầu sản phẩm (PRODUCT_REQUIREMENTS)](docs/product/PRODUCT_REQUIREMENTS.md) | Phạm vi tính năng V1 đã được chủ dự án xác nhận; có ưu tiên cao nhất |
| **Yêu cầu & Stack đã chốt** | [Đánh giá tech stack (TECH_STACK)](docs/architecture/TECH_STACK.md) | Stack đã duyệt (Axum, SQLx/SQLite, React/Vite, MiniJinja, Wasmi) và kết quả bản thử |
| **Kiến trúc & Đề xuất đang dùng** | [Kiến trúc CMS và cấu trúc thư mục (CMS_ARCHITECTURE)](docs/architecture/CMS_ARCHITECTURE.md) | Cấu trúc Workspace 2 crate (`opendash-core` / `opendash-server` / `admin`), luồng Preview/Lưu và trình tự chuyển đổi |
| **Kiến trúc & Đề xuất đang dùng** | [Đề xuất CMS-first (CMS_PROPOSAL)](docs/architecture/CMS_PROPOSAL.md) | **Đề xuất nền đang dùng có chọn lọc:** Dùng lại ranh giới module (Mục 2) và nguyên tắc tách dữ liệu khỏi theme; các phần khác đối chiếu theo `PRODUCT_REQUIREMENTS`, `TECH_STACK` và `CMS_ARCHITECTURE` |
| **Hợp đồng kỹ thuật (Contract)** | [Hợp đồng ABI Plugin (PLUGIN_ABI)](docs/contracts/PLUGIN_ABI.md) | Quy ước bộ nhớ, hàm host, quyền, giới hạn tài nguyên và mã lỗi giữa host Wasmi và SDK plugin |
| **Quy trình đang áp dụng** | [AGENTS.md](AGENTS.md) · [Giao thức thảo luận (PROTOCOL)](docs/discussions/PROTOCOL.md) | Quy tắc làm việc bắt buộc trong repo và giao thức phối hợp Antigravity ↔ Codex |
| **Quyết định tổ chức repository** | [ADR-0001](docs/decisions/ADR-0001-workspace-layout.md) | Ranh giới 2 crate + React Admin; trạng thái thiết kế `AGREED` |
| **Trạng thái và kế hoạch** | [Kiểm kê](docs/verification/2026-11-06-repository-inventory.md) · [Kế hoạch](docs/plans/README.md) | Những gì có trong workspace mới, điều kiện để import source và kiểm chứng |

Khi có khác biệt, ưu tiên chỉ đạo mới nhất của chủ dự án và yêu cầu đã xác nhận trong [PRODUCT_REQUIREMENTS.md](docs/product/PRODUCT_REQUIREMENTS.md).

## Chạy scaffold trên Windows

Yêu cầu: Rust toolchain (edition 2024), Node.js 22+, npm và PowerShell. Build frontend trước để Rust nhúng `cms/admin/dist/` vào binary bằng `rust-embed`. Bản hiện tại chỉ phục vụ local qua `127.0.0.1` vì chưa có xác thực và phân quyền.

Từ thư mục gốc `/opendash-cms`:

```powershell
# Cài và build React Admin
cd cms/admin
npm ci
npm run build
cd ../..

# Kiểm chứng Rust workspace và biên dịch binary
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo test --manifest-path cms/Cargo.toml --workspace --locked
cargo build --manifest-path cms/Cargo.toml --locked -p opendash-server

# Chạy server demo (dùng database/opendash.sqlite tại gốc website)
cargo run --manifest-path cms/Cargo.toml --locked -p opendash-server
```

Hoặc dùng script build toàn bộ (sau khi đã có lockfiles):

```powershell
powershell -ExecutionPolicy Bypass -File cms/scripts/build.ps1
powershell -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release
```

Khi server đang chạy, mở:

- [Trang công khai mẫu](http://127.0.0.1:18766/) — render HTML với MiniJinja.
- [React Admin mẫu](http://127.0.0.1:18766/admin/) — Tổng quan và Builder minh họa.
- [Health API](http://127.0.0.1:18766/api/health) — `SELECT 1` trên SQLite. Có thể kiểm tra qua `powershell -File cms/scripts/check-health.ps1`.

Mặc định server dùng port `18766`. Ở lần chạy đầu, binary phân phối tạo `database/opendash.sqlite` bên cạnh file thực thi, kể cả khi khởi động từ thư mục khác. Binary phát triển dưới `cms/target/` dùng gốc dự án làm gốc website. Đặt `OPENDASH_SITE_ROOT` để chọn gốc website khác, kể cả khi dùng thư mục build Cargo tùy chỉnh. `OPENDASH_DB_PATH` chọn riêng tệp SQLite, còn `OPENDASH_PORT` chọn cổng. Xem [kiểm chứng SitePaths trên Windows](docs/verification/2026-11-07-site-paths.md).

**Giới hạn hiện tại:** API chỉ có `/api/health`; `/builder` mở cùng giao diện demo và không lưu dữ liệu. Không có đăng nhập, CSRF, quyền Admin/Editor, migrations CMS, content API, theme ZIP, Plugin ABI host hoặc các smoke tests M1/M2. Thư mục `cms/migrations/` và `cms/fixtures/` chỉ có hướng dẫn để bảo toàn dữ liệu lịch sử khi nhập mã nguồn chính thức. Không triển khai lên Internet từ scaffold này.

Để tiếp tục V1, xem [kế hoạch nhập mã và các kiểm chứng](docs/plans/README.md).
