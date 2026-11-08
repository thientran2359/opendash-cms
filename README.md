# OpenDash

CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress. Rust, SQLite, một file thực thi cho mỗi bản cài và hệ theme/plugin riêng.

**Hiện trạng (08/11/2026).** M1/M2 đã được nhập từ cây nguồn đã kiểm chứng vào workspace. Bản Windows có thiết lập Admin, đăng nhập, vai trò Editor, nhập ZIP theme một block, Preview/Lưu và plugin Wasmi chạy trong worker riêng. Xem [kiểm chứng M1/M2](docs/verification/2026-11-07-m1-m2-import.md). M3.1–M3.3 đã có mô hình CPT/custom fields, API tạo/liệt kê CPT và backend CRUD entry, [kiểm chứng trên macOS](docs/verification/2026-11-08-m3-3-content-entry-crud.md). Chưa có màn hình Admin cho CPT/Entry, template render CPT ra trang công khai, Media, builder nhiều tầng hay backup/restore.

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

M1 lưu template và dữ liệu trang trong SQLite. M2 lưu manifest, Wasm, KV và log của plugin trong cùng database. Các thư mục website ở gốc dành cho bố cục triển khai V1.

## Bắt đầu ở đâu

1. Đọc [yêu cầu sản phẩm](docs/product/PRODUCT_REQUIREMENTS.md) và [đánh giá tech stack](docs/architecture/TECH_STACK.md) để biết phạm vi sản phẩm và stack đã được chủ dự án xác nhận.
2. Đọc [kiến trúc CMS và cấu trúc thư mục](docs/architecture/CMS_ARCHITECTURE.md) cùng [đề xuất CMS-first](docs/architecture/CMS_PROPOSAL.md) để nắm ranh giới module trước khi triển khai.
3. Dùng [mục lục docs](docs/README.md) để phân biệt thiết kế đã duyệt, kế hoạch và bằng chứng kiểm chứng. Khi làm việc bằng AI, đọc [AGENTS.md](AGENTS.md) và [giao thức thảo luận](docs/discussions/PROTOCOL.md).
4. Tra lại phạm vi và nguồn M1/M2 theo [kế hoạch nhập mã](docs/plans/README.md) và ghi bằng chứng mới vào [verification](docs/verification/README.md). Không thay thế migrations hay plugin runtime đã kiểm chứng bằng mã mẫu.

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

## Chạy OpenDash trên Windows

Từ gốc repo, dùng Rust, Node.js 22+, npm và PowerShell để build Admin, plugin mẫu rồi nhúng Admin vào binary release.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release
.\cms\target\release\opendash.exe
```

Mở [Admin](http://127.0.0.1:18766/admin/) hoặc [Builder](http://127.0.0.1:18766/builder). Trên site mới, nhập tên và mật khẩu, bấm **Thiết lập Admin**, rồi **Đăng nhập**. Admin có thể nhập ZIP theme, tạo Editor và cài [plugin mẫu](cms/examples/plugins/sample-plugin/README.md). Xem trước dùng dữ liệu đang sửa. Bấm **Lưu** cập nhật [trang công khai](http://127.0.0.1:18766/).

Binary bind loopback, mặc định cổng 18766. Binary phân phối lưu SQLite ở `database/opendash.sqlite` cạnh executable; bản trong `cms/target/` dùng gốc dự án. Đặt `OPENDASH_SITE_ROOT` để chọn site, `OPENDASH_DB_PATH` để chọn riêng SQLite và `OPENDASH_PORT` để chọn cổng. Lần mở đầu chạy migration 0001–0005. Để dùng thử với dữ liệu riêng, đặt `$env:OPENDASH_SITE_ROOT` tới một thư mục trống trước khi chạy. Khi server đang chạy, dùng `powershell -File cms/scripts/check-health.ps1` để kiểm tra SQLite.

## Kiểm chứng tại workspace

Sau khi build Admin và plugin mẫu bằng script trên, chạy từ gốc:

```powershell
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
cd cms
python scripts/windows_smoke.py --binary target/release/opendash.exe
python scripts/windows_auth_smoke.py --binary target/release/opendash.exe
python scripts/windows_distribution_smoke.py --binary target/release/opendash.exe
python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe
python scripts/content_types_smoke.py --binary target/release/opendash.exe
python scripts/content_entries_smoke.py --binary target/release/opendash.exe
# Bổ sung khi đã có Python Playwright và Chromium
python scripts/windows_admin_browser.py
cd ..
```

Script trình duyệt nhận `--browser` để chọn executable Chromium đã cài. Các smoke test tạo site riêng trong thư mục tạm. Để tạo ZIP theme thử từ gốc repo, chạy `python cms/scripts/windows_smoke.py --theme "$env:TEMP/opendash-banner.zip"` rồi chọn tệp đó trong Builder.

## M3.2 — CPT backend API

- `GET /api/admin/content-types`: yêu cầu đăng nhập, cho phép Admin và Editor xem các định nghĩa CPT cùng owner/status.
- `POST /api/admin/content-types`: Admin và header `X-CSRF-Token` hợp lệ, nhận JSON `ContentTypeDefinition`, trả `201` khi tạo, `400` nếu schema sai, `409` nếu khóa CPT trùng. Giới hạn body 512 KiB.
- Migration `0004_content_types.sql` lưu CPT và custom field schema trên SQLite, giữ lại thông tin provider không hoạt động. API không có endpoint sửa/xóa CPT trong slice này.

Từ root trên macOS, sau khi build Admin (`cd cms/admin && npm ci && npm run build && cd ../..`), kiểm chứng binary debug và API bằng:

```sh
cargo build --manifest-path cms/Cargo.toml --locked
python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash
```

Test dùng site và SQLite tạm, bao gồm nâng cấp từ fixture cũ; xem [báo cáo M3.2](docs/verification/2026-11-08-m3-2-content-types-backend.md).

## M3.3 — Content Entry CRUD API

Admin và Editor có quyền tạo/sửa/xóa entry của CPT đã khai báo. Dữ liệu entry được lưu trong migration `0005_content_entries.sql` và sống độc lập với vòng đời theme/plugin. Mọi route dưới đây đều yêu cầu đăng nhập:

- `GET/POST /api/admin/content-types/{type_key}/entries`: liệt kê có phân trang hoặc tạo entry. POST cần `X-CSRF-Token`, thành công trả `201` và `ETag: "v1"`.
- `GET/PUT/DELETE /api/admin/content-types/{type_key}/entries/{id}`: đọc, thay thế đầy đủ hoặc xóa entry. PUT/DELETE cần CSRF và `If-Match: "vN"`; bản đã cũ trả `412`, thiếu If-Match trả `428`.
- Giá trị `values` được kiểm tra theo 9 loại custom fields; body tối đa 512 KiB. Trùng slug trong cùng CPT trả `409`, không tồn tại trả `404`.

```sh
cargo build --manifest-path cms/Cargo.toml --locked
python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash
```

**Lưu ý:** Slice này lưu nội dung vào SQLite, chưa tích hợp renderer CPT để hiển thị ra trang công khai. `rich_text` vẫn là dữ liệu chưa được sanitize, tham chiếu media chưa được xác minh. Xem [kế hoạch](docs/plans/2026-11-08-m3-3-content-entry-crud.md) và [kiểm chứng M3.3](docs/verification/2026-11-08-m3-3-content-entry-crud.md).

## CodeGraph cho Codex, Antigravity và Chat On Steroids

CodeGraph 0.21.0 lập chỉ mục cú pháp và quan hệ gọi hàm trên mã Rust/TypeScript của OpenDash để các AI agent có thể tra cứu `symbol_search`, `get_call_graph`, `analyze_impact` và `get_edit_context` trước khi sửa code. Chỉ mục riêng cho từng workspace lưu ngoài Git tại `~/.codegraph/`; các tài liệu sản phẩm, kiến trúc và contract được nạp thêm để tra cứu thiết kế. Xem [bằng chứng, cách kết nối MCP và giới hạn](docs/verification/2026-11-07-codegraph-setup.md).

Sau khi thay đổi tài liệu nguồn trong các mục đã định nghĩa, chạy lại từ gốc repo:

```powershell
node cms/scripts/codegraph-index-docs.mjs
```

Chat On Steroids Core (terminal) cũng có thể truy vấn trực tiếp khi chưa kết nối plugin MCP:

```powershell
node cms/scripts/codegraph-query.mjs symbol HookDispatcher
node cms/scripts/codegraph-query.mjs callgraph cms/crates/opendash-server/src/plugin/host_ops.rs 94
node cms/scripts/codegraph-query.mjs docs "plugin ABI"
```

Chỉ mục code được cập nhật khi CodeGraph khởi động hoặc phát hiện file thay đổi. CodeGraph hỗ trợ việc định hướng; tài liệu contract và source code vẫn là nguồn để xác minh hành vi.
