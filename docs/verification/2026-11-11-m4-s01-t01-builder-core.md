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

**Coverage của các fixture Builder:** JSON round-trip và reorder không thay ID, typed props cho bốn loại core, block legacy từ field map M3, schema version không hỗ trợ, sai quan hệ cha-con, duplicate/invalid ID, vượt node/byte limits, URL có scheme trái phép hoặc credentials, unknown/malformed props, descriptor inactive, unknown node giữ nguyên payload và vị trí, chặn thay đổi payload/ID/di chuyển unresolved.

## Giới hạn và chuyển giao

- Đây là **core-only** (G4 implementation/test tại branch); HTTP, SQLite, homepage M3 projection, Preview/Save, renderer, media existence/ACL và chẩn đoán Admin thuộc **M4-S02-T01 / Issue #78**. Không tuyên bố G5 hoặc Project Done cho #77 khi PR chưa được tích hợp và hậu kiểm.
- Resolver / registry được inject bởi tầng server; core chỉ kiểm cú pháp media ID, còn tính tồn tại và quyền public media phải được kiểm ở storage/save khi triển khai P02. Khi provider thiếu, downstream renderer phải bỏ qua node và cung cấp diagnostic theo node ID theo ADR-0007.
- Rollback là code-only; không có migration hay thay đổi dữ liệu site. Kiểm thử chạy trên source/worktree; chưa chạy browser hoặc server smoke cho P02.
