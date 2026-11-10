# OpenDash

CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress. Rust, SQLite, một file thực thi cho mỗi bản cài và hệ theme/plugin riêng.

**Hiện trạng (09/11/2026).** M1/M2 đã được nhập từ cây nguồn đã kiểm chứng vào workspace. Xem thêm [kiểm chứng bộ khung lõi M2.5](docs/verification/2026-11-07-m2-5-core-kernel.md). Bản Windows có thiết lập Admin, đăng nhập, vai trò Editor, nhập ZIP theme một block, Preview/Lưu và plugin Wasmi chạy trong worker riêng. Xem [kiểm chứng M1/M2](docs/verification/2026-11-07-m1-m2-import.md). M3.1–M3.6 đã có CPT/custom fields, SQLite/API entry CRUD, giao diện Admin nội dung, [Media Library](docs/verification/2026-11-09-m3-5-media-library.md) và [trang CPT công khai](docs/verification/2026-11-09-m3-6-public-content-rendering.md) kiểm chứng trên macOS. Chưa có taxonomy, Builder nhiều tầng, active theme riêng, xóa media hoặc backup/restore.


Five agents can use five independent local working directories and agent-specific
branches of the same GitHub opendash-cms repository. Repository .agents/ holds
unapproved technical research/discussions/task handoffs; docs/ holds official
approved records and real verification. Completed tasks are committed and pushed
to the agent's own branch for Issue/PR review, never auto-merged into main.
Muse.ai dispatch to CoS/Antigravity remains a proposed integration requiring
verification of real adapters and permissions.

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
| `.agents/` | Research, draft discussions, task handoffs and reference archives; retired skills are in `.agents/archive/skills/` |

M1 lưu template và dữ liệu trang trong SQLite. M2 lưu manifest, Wasm, KV và log của plugin trong cùng database. Các thư mục website ở gốc dành cho bố cục triển khai V1.

## Bắt đầu ở đâu

1. Đọc [yêu cầu sản phẩm](docs/product/PRODUCT_REQUIREMENTS.md) và [đánh giá tech stack](docs/architecture/TECH_STACK.md) để biết phạm vi sản phẩm và stack đã được chủ dự án xác nhận.
2. Đọc [kiến trúc CMS và cấu trúc thư mục](docs/architecture/CMS_ARCHITECTURE.md) cùng [đề xuất CMS-first](docs/architecture/CMS_PROPOSAL.md) để nắm ranh giới module trước khi triển khai.
3. Dùng [mục lục docs](docs/README.md) để phân biệt thiết kế đã duyệt, kế hoạch và bằng chứng kiểm chứng. Khi làm việc bằng AI, đọc [AGENTS.md](AGENTS.md) và [giao thức thảo luận](docs/discussions/PROTOCOL.md).
4. Tra lại phạm vi và nguồn M1/M2 theo [kế hoạch nhập mã](docs/plans/README.md) và ghi bằng chứng mới vào [verification](docs/verification/README.md). Không thay thế migrations hay plugin runtime đã kiểm chứng bằng mã mẫu.
5. Khi lập kế hoạch tính năng V1, đọc [Roadmap](docs/plans/ROADMAP_V1.md), [Module Catalog](docs/plans/MODULE_CATALOG_V1.md) và [quy trình chia task](docs/plans/PLANNING_WORKFLOW.md). M3.1–M3.6 đã có [bằng chứng theo slice](docs/plans/M3_STATUS_2026-11-09.md); toàn M3 vẫn PARTIAL và các capability còn thiếu phải qua gate phù hợp.

## Bộ tài liệu

| Nhóm vai trò | Tài liệu | Nội dung và trạng thái |
|---|---|---|
| **Yêu cầu & Stack đã chốt** | [Yêu cầu sản phẩm (PRODUCT_REQUIREMENTS)](docs/product/PRODUCT_REQUIREMENTS.md) | Phạm vi tính năng V1 đã được chủ dự án xác nhận; có ưu tiên cao nhất |
| **Yêu cầu & Stack đã chốt** | [Đánh giá tech stack (TECH_STACK)](docs/architecture/TECH_STACK.md) | Stack đã duyệt (Axum, SQLx/SQLite, React/Vite, MiniJinja, Wasmi) và kết quả bản thử |
| **Kiến trúc & Đề xuất đang dùng** | [Kiến trúc CMS và cấu trúc thư mục (CMS_ARCHITECTURE)](docs/architecture/CMS_ARCHITECTURE.md) | Cấu trúc Workspace 2 crate (`opendash-core` / `opendash-server` / `admin`), luồng Preview/Lưu và trình tự chuyển đổi |
| **Kiến trúc & Đề xuất đang dùng** | [Đề xuất CMS-first (CMS_PROPOSAL)](docs/architecture/CMS_PROPOSAL.md) | **Đề xuất nền đang dùng có chọn lọc:** Dùng lại ranh giới module (Mục 2) và nguyên tắc tách dữ liệu khỏi theme; các phần khác đối chiếu theo `PRODUCT_REQUIREMENTS`, `TECH_STACK` và `CMS_ARCHITECTURE` |
| **Hợp đồng kỹ thuật (Contract)** | [Bộ khung lõi & Quy chuẩn code (CORE_FOUNDATION)](docs/contracts/CORE_FOUNDATION.md) · [Hợp đồng ABI Plugin (PLUGIN_ABI)](docs/contracts/PLUGIN_ABI.md) | Danh mục Base APIs bắt buộc tái sử dụng, quy chuẩn phân tầng/SQL và giao thức giữa host Wasmi với SDK plugin |
| **Quy trình đang áp dụng** | [AGENTS.md](AGENTS.md) · [Giao thức thảo luận (PROTOCOL)](docs/discussions/PROTOCOL.md) | Quy tắc làm việc bắt buộc trong repo và giao thức phối hợp Antigravity ↔ Codex |
| **Quyết định kiến trúc** | [ADR-0001](docs/decisions/ADR-0001-workspace-layout.md) · [ADR-0002](docs/decisions/ADR-0002-core-extension-contract.md) | Ranh giới 2 crate + React Admin và cơ chế mở rộng lõi M2.5 (`RegistryCatalog`, `HookDispatcher`, `storage/plugin_kv`) |
| **Trạng thái và kế hoạch** | [Kiểm kê](docs/verification/2026-11-06-repository-inventory.md) · [Kế hoạch](docs/plans/README.md) | Những gì có trong workspace mới, điều kiện để import source và kiểm chứng |

Khi có khác biệt, ưu tiên chỉ đạo mới nhất của chủ dự án và yêu cầu đã xác nhận trong [PRODUCT_REQUIREMENTS.md](docs/product/PRODUCT_REQUIREMENTS.md).

## Chạy OpenDash trên Windows

Từ gốc repo, dùng Rust, Node.js 22+, npm và PowerShell để build Admin, plugin mẫu rồi nhúng Admin vào binary release.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release
.\cms\target\release\opendash.exe
```

Mở [Admin](http://127.0.0.1:18766/admin/) hoặc [Builder](http://127.0.0.1:18766/builder). Trên site mới, nhập tên và mật khẩu, bấm **Thiết lập Admin**, rồi **Đăng nhập**. Admin có thể nhập ZIP theme, tạo Editor và cài [plugin mẫu](cms/examples/plugins/sample-plugin/README.md). Xem trước dùng dữ liệu đang sửa. Bấm **Lưu** cập nhật [trang công khai](http://127.0.0.1:18766/).

Binary bind loopback, mặc định cổng 18766. Binary phân phối lưu SQLite ở `database/opendash.sqlite` cạnh executable; bản trong `cms/target/` dùng gốc dự án. Đặt `OPENDASH_SITE_ROOT` để chọn site, `OPENDASH_DB_PATH` để chọn riêng SQLite và `OPENDASH_PORT` để chọn cổng. Lần mở đầu chạy migration 0001–0007. Để dùng thử với dữ liệu riêng, đặt `$env:OPENDASH_SITE_ROOT` tới một thư mục trống trước khi chạy. Khi server đang chạy, dùng `powershell -File cms/scripts/check-health.ps1` để kiểm tra SQLite.

## Kiểm chứng tại workspace

Sau khi build Admin và plugin mẫu bằng script trên, chạy từ gốc:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
cd cms
python scripts/windows_smoke.py --binary target/release/opendash.exe
python scripts/windows_auth_smoke.py --binary target/release/opendash.exe
python scripts/windows_distribution_smoke.py --binary target/release/opendash.exe
python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe
python scripts/windows_kernel_smoke.py --binary target/release/opendash.exe
python scripts/content_types_smoke.py --binary target/release/opendash.exe
python scripts/content_entries_smoke.py --binary target/release/opendash.exe
python scripts/media_smoke.py --binary target/release/opendash.exe
python scripts/public_content_smoke.py --binary target/release/opendash.exe
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

**Lưu ý:** M3.3 là lớp lưu trữ; từ M3.6, nội dung đã lưu được hiển thị công khai tại `/content/{type}/{slug}` (không có cơ chế draft/publish riêng). `rich_text` chỉ hiển thị dưới dạng văn bản đã escape; từ M3.5, ảnh tham chiếu trong entry mới/sửa phải có trong Media Library. Xem [kế hoạch](docs/archive/plans/m3/2026-11-08-m3-3-content-entry-crud.md) và [kiểm chứng M3.3](docs/verification/2026-11-08-m3-3-content-entry-crud.md).

## M3.4 — Admin Content UI

Sau khi đăng nhập, chọn tab **Nội dung** bên cạnh **Builder**. Admin có thể tạo CPT/schema với các trường theo 9 kiểu V1; Admin và Editor đều có thể mở CPT, liệt kê, tạo, sửa và xóa entry.

Form gửi JSON theo đúng kiểu field và dùng ETag/If-Match khi sửa/xóa. Khi phiên đăng nhập hết hạn (401) hoặc dữ liệu có phiên bản mới (412), bản nháp vẫn được giữ để người dùng đăng nhập lại hoặc chủ động tải bản mới. Tab Builder không bị thay thế.

HTML rich text được lưu như dữ liệu và không được thực thi khi nhập; từ M3.6, lưu entry sẽ cập nhật trang công khai. Từ M3.5, field ảnh/gallery đã có thư viện chọn ảnh. Xem [kế hoạch M3.4](docs/archive/plans/m3/2026-11-08-m3-4-admin-content-ui.md) và [bằng chứng UI](docs/verification/2026-11-08-m3-4-admin-content-ui.md).

## M3.5 — Media Library

Trong **Admin → Nội dung**, mở **Media Library** để upload JPEG/PNG/WebP, xem thumbnail và chọn ảnh trực tiếp cho field `image` hoặc nhiều ảnh cho `gallery`. Có thể đổi thứ tự và bỏ ảnh khỏi entry; file upload được giữ lại trong thư viện. Admin và Editor đều có thể upload/chọn ảnh.

- `POST /api/admin/media`: gửi bytes ảnh trực tiếp (Content-Type JPEG/PNG/WebP, `X-CSRF-Token`), giới hạn 8 MiB/6000px/20 megapixel. Trả metadata chứa ID SHA-256.
- `GET /api/admin/media?limit=48&offset=0`: liệt kê metadata. `GET /api/admin/media/{id}/thumbnail` hoặc `/file` phục vụ ảnh cho tài khoản đã đăng nhập, không phải URL ảnh công khai.
- Ảnh lưu trong site-root `uploads/media/`; SQLite migration `0006_media_assets.sql` ghi metadata. File được chuẩn hóa để bỏ EXIF và thumbnail PNG giới hạn 320px.
- Khi tạo/sửa entry, backend xác minh toàn bộ media IDs thực sự tồn tại, tránh lưu các ID tùy tiện. Entry cũ với ID giả cần thay ảnh hợp lệ khi sửa.

```sh
cargo build --manifest-path cms/Cargo.toml --locked
python3 cms/scripts/media_smoke.py --binary cms/target/debug/opendash
```

Chưa có xóa media khỏi thư viện hay backup/restore uploads. Từ M3.6, ảnh đã được gắn trong entry có URL công khai `/media/{id}/file`, ảnh chưa được dùng vẫn chỉ đọc được qua API Admin. Xem [kế hoạch M3.5](docs/archive/plans/m3/2026-11-09-m3-5-media-library.md) và [báo cáo kiểm chứng](docs/verification/2026-11-09-m3-5-media-library.md).

## M3.6 — Hiển thị nội dung CPT công khai

Trong **Admin → Nội dung**, các liên kết **Xem danh sách công khai** và **Xem trang công khai (bản đã lưu)** mở URL thực theo CPT/slug:

- `GET /content/{type_key}`: archive 20 entry/trang (phân trang bằng `?page=N`).
- `GET /content/{type_key}/{slug}`: single của nội dung đã lưu; khi Admin/Editor bấm Lưu, website cập nhật ngay, không có bước publish riêng.
- `GET /media/{id}/file` và `/thumbnail`: ảnh công khai chỉ khi đang được tham chiếu bởi entry trong field khai báo `image` hoặc `gallery`. Ảnh chưa dùng được giữ riêng tư.

Renderer mặc định có HTML auto-escape, CSP chặn script và chỉ dùng ảnh nội bộ. `rich_text` được escape thành văn bản, **chưa hỗ trợ render HTML có định dạng**. Theme ZIP gồm `block.json` và `block.html` trước đây vẫn hoạt động; theme mới có thể thêm `content-list.html` và `content-single.html`. M3.6 lấy template từ block trang chủ đang lưu, nếu không có sẽ dùng mẫu mặc định. Dữ liệu CPT/entry không phụ thuộc vòng đời theme.

```sh
cargo build --manifest-path cms/Cargo.toml --locked
python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash
```

Chưa có active theme registry riêng, taxonomy, chỉnh archive/single bằng Builder hay preview entry chưa lưu. Xem [kế hoạch M3.6](docs/archive/plans/m3/2026-11-09-m3-6-public-content-rendering.md) và [báo cáo kiểm chứng](docs/verification/2026-11-09-m3-6-public-content-rendering.md).

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
