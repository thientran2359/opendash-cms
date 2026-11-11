# M4-S01-T01 — Builder document v1 và kiểm tra cấu trúc

**Issue:** [#77](https://github.com/thientran2359/opendash-cms/issues/77) · **Agent:** Forge (Windows) · **Module:** BLD-02.1 · **Contract:** [ADR-0007 § P01](../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) và [G3 M4-S01-T01](../plans/milestones/M4/slices/2026-11-11-m4-foundation-g3.md).

**Approved source/baseline:** `0e3d9886afa6115f84d97aa8905bc29f18fa56cb` · **Implementation base:** `origin/main@5b8b79f29dc467a54523a4bcf795b0399122a2ca` · **Branch:** `task/forge-m4-s01-t01`.

## Hành vi đã triển khai

- `cms/crates/opendash-core/src/builder.rs` cung cấp `BuilderDocument` (`schema_version: 1`, `sections`), `SectionNode → RowNode → ColumnNode → ElementNode`. Các quan hệ cha-con được kiểm tra bởi kiểu Rust/Serde và cấu trúc JSON; thứ tự của `children` là thứ tự render, node ID độc lập với vị trí.
- Các loại `core/heading`, `core/text`, `core/image`, `core/button` có Rust props kiểu riêng và validator: Heading level 1–6; Image tham chiếu media ID theo cú pháp M3; Button URL chỉ cho path nội bộ hoặc HTTP(S), không cho credentials và chuỗi không an toàn.
- `legacy/block` giữ nguyên `block_id` và `values` dạng `Fields` M3; đối chiếu khóa field/value với descriptor trong `RegistryCatalog` hiện hành. `BuilderDocument::validate_for_save(catalog, previous)` chỉ cho phép node legacy thiếu hoặc inactive descriptor và payload unrecognized/malformed tồn tại **nguyên node và vị trí dưới cùng ancestor IDs** so với snapshot lưu trước đó. `previous` bắt buộc là snapshot đáng tin từ storage; không nhận từ payload request.
- Giới hạn 512 node trên toàn cây, 256 KiB JSON; kiểm tra kích thước bytes trước decode, kiểm `schema_version`, ID theo bộ kiểm định danh core, ID trùng và unknown fields; không có khả năng thêm/sửa unresolved element mới bằng validation. `to_json` kiểm cả giới hạn khi code Rust tạo document thủ công.
- Khai báo module trong `cms/crates/opendash-core/src/lib.rs` để task [#78](https://github.com/thientran2359/opendash-cms/issues/78) có thể dùng. Không thêm dependency, migration, router hay SQL.

## Kiểm chứng trên Windows

Môi trường: Windows, native Rust workspace tại worktree `opendash-cms-forge-m4-s01-t01`, locked dependencies trong `cms/Cargo.lock`. Các fixture unit test nằm cạnh implementation và dùng schema M3 hiện hành `theme::{Block, BlockDescriptor, Fields}`, `id` và `RegistryCatalog`.

| Lệnh từ repository root | Kết quả |
| --- | --- |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-core --locked` | **PASS 25/25** (6 Builder + 19 core khác) |
| `cd cms/admin; npm ci --no-audit --no-fund; npm run build` | **PASS**, Vite 60 modules, React Admin dist |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS**, full workspace after Admin asset build |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | **PASS 73/73** (25 core + 48 server, 0 doc-tests) |
| `python cms/scripts/check-plans.py` | **PASS** |
| `python cms/scripts/check-plans.py --self-test` | **PASS 12 fixtures** |
| `git diff --check` | **PASS** |

Lượt build full workspace đầu tiên **FAIL vì worktree mới chưa có `cms/admin/dist`** (RustEmbed yêu cầu thư mục này), kéo theo lỗi `AdminAssets::get` ở server. Đã giải quyết bằng `npm ci` + `npm run build` đúng lockfile của Admin, sau đó chạy lại nguyên lệnh Rust và nhận PASS, không cần sửa source ngoài phạm vi.

### Revalidation sau khi cập nhật main (2026-11-11, Windows)

- Trước cập nhật: PR implementation `1632ea8eeb974041d2630c34198ab9a71e430a4a`.
- Source `origin/main` mới: `c98ec430d7a4780117b6d03acc8f513e4117b138` (bao gồm ADR-0008 và Harbor M4-S03-T01). Merge task branch không conflict tại `e57d09cc8629cb3f7c575ad4a532faf95312e945`. Toàn bộ file thay đổi riêng của PR so với main vẫn là `builder.rs`, `lib.rs` và báo cáo này.
- `cd cms/admin; npm ci --no-audit --no-fund; npm run build`: **PASS** (Vite 62 modules).
- `cargo fmt --manifest-path cms/Cargo.toml --all --check`: **PASS**.
- `cargo build --manifest-path cms/Cargo.toml --locked`: **PASS**.
- `cargo test --manifest-path cms/Cargo.toml --workspace --locked`: **PASS 78/78** (25 core + 53 server, 0 doctest).
- `python cms/scripts/check-standards.py`, `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test` (12 fixtures), `git diff --check origin/main...HEAD`: **PASS**.
- `PYTHONIOENCODING=utf-8 python cms/scripts/themes_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite`: **PASS** (legacy A→B, A→C, C→A, saved fields, restart, migration and historical SQLite upgrade). Script dùng site test tạm; không dùng dữ liệu sản xuất.
- GitHub CI trên PR head cũ `1632ea8`: **PASS** Ubuntu/Windows (governance run `43013450152`). CI trên head được cập nhật **cần xác nhận lại trước merge**.

**Self-review scope:** không tìm thấy blocker khi đối chiếu G3/ADR-0007 với boundary code (không ghi SQLite, không render HTML, chỉ cho phép payload unresolved round-trip từ snapshot lưu trước, không cho thêm/sửa payload thiếu descriptor). Review này do Forge tự thực hiện theo ADR-0008, **không phải independent peer review**. Merge và G5 chỉ được ký sau khi kiểm gates trên head cuối và post-merge source.

**Coverage của các fixture Builder:** JSON round-trip và reorder không thay ID, typed props cho bốn loại core, block legacy từ field map M3, schema version không hỗ trợ, sai quan hệ cha-con, duplicate/invalid ID, vượt node/byte limits, URL có scheme trái phép hoặc credentials, unknown/malformed props, descriptor inactive, unknown node giữ nguyên payload và vị trí, chặn thay đổi payload/ID/di chuyển unresolved.

## Giới hạn và chuyển giao

- Đây là **core-only** (G4 implementation/test tại branch); HTTP, SQLite, homepage M3 projection, Preview/Save, renderer, media existence/ACL và chẩn đoán Admin thuộc **M4-S02-T01 / Issue #78**. Không tuyên bố G5 hoặc Project Done cho #77 khi PR chưa được tích hợp và hậu kiểm.
- Resolver / registry được inject bởi tầng server; core chỉ kiểm cú pháp media ID, còn tính tồn tại và quyền public media phải được kiểm ở storage/save khi triển khai P02. Khi provider thiếu, downstream renderer phải bỏ qua node và cung cấp diagnostic theo node ID theo ADR-0007.
- Rollback là code-only; không có migration hay thay đổi dữ liệu site. Kiểm thử chạy trên source/worktree; chưa chạy browser hoặc server smoke cho P02.
