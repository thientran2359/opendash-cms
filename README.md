# OpenDash

CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress. Rust, SQLite, một file thực thi cho mỗi bản cài và hệ theme/plugin riêng.

**Hiện trạng:** Hệ thống đã triển khai qua các cột mốc nền tảng (`0001`–`0041` SQLite migrations): Auth/RBAC (`Admin`/`Editor`), Posts/Pages (`draft`/`published`), CPT & Field Groups có cấu hình `publication` (`immediate` | `draft_publish`), Custom Taxonomy, Media Library (ảnh + MP4 video + lifecycle/reconcile), Visual Builder 4 tầng (`Section → Row → Column → Element`, Homepage, Template Overrides, Page-scoped Builder, private Section CPT & Local Template Library), Theme ZIP v2 (Theme Options, selective sample import), Plugin ZIP (Wasmi worker cô lập, hooks, actions, declarative Admin UI, DNS-pinned egress), Menus, SEO/Permalinks/Sitemap, Contact Form + SMTP, và Encrypted Backup/Restore.

Quá trình thiết kế và kiểm chứng tuân theo quy trình **EGM (Elephant-Goldfish Model)** trong [`AGENTS.md`](AGENTS.md) và [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md).

## Cấu trúc thư mục

| Thư mục | Nội dung |
|---|---|
| [`cms/`](cms/README.md) | Mã nguồn và công cụ phát triển: `crates/` (`opendash-core`, `opendash-server`), `admin/`, `sdk/`, `examples/`, `migrations/` (`0001`–`0041`), `fixtures/`, `deploy/`, `scripts/`, `spikes/`; chứa `Cargo.toml` và `Cargo.lock` |
| [`docs/`](docs/README.md) | Tài liệu sản phẩm, kiến trúc, contracts, thiết kế EGM (`designs/`), quyết định (`decisions/`), bằng chứng (`verification/`) và lưu trữ lịch sử (`archive/`) |
| [`themes/`](themes/README.md) | Theme đã cài của website |
| [`plugins/`](plugins/README.md) | Plugin đã cài của website |
| [`uploads/`](uploads/README.md) | Ảnh/video và media người dùng tải lên (không commit vào Git) |
| [`database/`](database/README.md) | SQLite `opendash.sqlite`, WAL/SHM và `secrets/` (không commit vào Git) |
| [`.agents/`](.agents/README.md) | Skill ngữ cảnh `opendash-elephant` cho quy trình EGM và hồ sơ lịch sử |

## Bắt đầu ở đâu

1. Đọc [quy trình làm việc EGM](AGENTS.md) và [mục lục tài liệu](docs/README.md).
2. Đọc [yêu cầu sản phẩm](docs/product/PRODUCT_REQUIREMENTS.md), [Tech Stack](docs/architecture/TECH_STACK.md) và [Kiến trúc CMS](docs/architecture/CMS_ARCHITECTURE.md).
3. Tra cứu hợp đồng lõi tại [`CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md), [`PLUGIN_ABI.md`](docs/contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md) và danh mục thiết kế tại [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md).

## Chạy OpenDash Dev (Windows và macOS)

Tại thư mục gốc repository, chạy:

```sh
python dev.py
```

Yêu cầu Python 3.10+ có Tkinter (hoặc `--headless`), Node.js 22+ và Rust/Cargo trong `PATH`.
- Mặc định site data demo (**Mộc Studio**, tài khoản `admin_moc` / `MocStudioDemo#2026!`) được tạo tại `runtime/demo-website/dev-local/` (bỏ qua bởi `.gitignore`).
- Dùng `python dev.py --no-demo` để chạy với dữ liệu tại root; `python dev.py --site-root /duong/dan` để chọn site riêng; `--port 18766` để đổi cổng; `--no-watch` để tắt auto-rebuild.

## Build Release trên Windows & Linux

**Windows:**
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release
.\cms\target\release\opendash.exe
```

**Linux (`x86_64-unknown-linux-musl`):**
```sh
bash cms/scripts/build-release-linux.sh
```

Binary bind loopback mặc định tại cổng `18766`, tự chạy migrations `0001`–`0041` khi khởi động. Đặt `OPENDASH_SITE_ROOT`, `OPENDASH_DB_PATH`, `OPENDASH_PORT` để tùy biến site và cổng.

## Kiểm chứng tại workspace

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
git diff --check
```

## CodeGraph (Tra cứu mã nguồn & tài liệu)

Chỉ mục CodeGraph hỗ trợ `codegraph_symbol_search`, `codegraph_get_call_graph`, `codegraph_analyze_impact`, `codegraph_get_edit_context`, `codegraph_search_docs`. Sau khi cập nhật tài liệu chính, chạy:

```powershell
node cms/scripts/codegraph-index-docs.mjs
```

Hoặc truy vấn CLI:
```powershell
node cms/scripts/codegraph-query.mjs symbol HookDispatcher
node cms/scripts/codegraph-query.mjs docs "plugin ABI"
```
