# Kiểm chứng M3.3 — Content Entry CRUD

- **Ngày:** 2026-11-08
- **Môi trường:** macOS Intel (Darwin x86_64), Rust/Cargo 1.99.1, Python 3, Admin dist đã build từ M3.1/M3.2.
- **Mã nguồn:** `/local-sites/opendash-cms`.
- **Phạm vi:** [kế hoạch M3.3](../archive/plans/m3/2026-11-08-m3-3-content-entry-crud.md), chỉ backend.

## Thay đổi đã kiểm chứng

- `cms/migrations/0005_content_entries.sql`: bảng entry theo CPT và slug, JSON values, version/timestamps, FK tới schema không cascade.
- `cms/crates/opendash-core/src/content.rs`: `ContentEntryInput::validate_against` kiểm tra kiểu và giới hạn chín loại field.
- `cms/crates/opendash-server/src/storage/entries.rs`: INSERT/SELECT/LIST/UPDATE/DELETE, unique slug per type, atomic version compare.
- `cms/crates/opendash-server/src/routes/entries.rs`: GET/POST/PUT/DELETE, guard Auth/CSRF, `ETag` và `If-Match`, hook before/after save qua dispatcher.
- `cms/scripts/content_entries_smoke.py`: kiểm chứng server HTTP thực tế với SQLite tạm và bản sao fixture lịch sử.
- `cms/scripts/windows_auth_smoke.py`: chỉnh bước clone fixture để mở nguồn ở chế độ SQLite `mode=ro&immutable=1`; tránh tạo file `-wal/-shm` cạnh fixture đã được Git theo dõi.

## Các kiểm chứng đã chạy

| Lệnh | Kết quả |
| --- | --- |
| `~/.cargo/bin/cargo fmt --manifest-path cms/Cargo.toml --all` | PASS |
| `~/.cargo/bin/cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — 17 Core + 34 Server, tổng 51 unit tests |
| `~/.cargo/bin/cargo build --manifest-path cms/Cargo.toml --locked` | PASS |
| `python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash` | PASS — toàn bộ CRUD, auth, CSRF, field validation, slug conflicts, CAS/If-Match concurrency, provider inactive, restart, baseline upgrade |
| `python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash` | PASS — M3.2 regression |
| `python3 scripts/windows_auth_smoke.py --binary target/debug/opendash` (từ `cms/`, chạy trên Mac) | PASS — auth, role, CSRF, Origin, expiry, upgrade |
| `python3 scripts/windows_smoke.py --binary target/debug/opendash` (từ `cms/`, chạy trên Mac) | PASS — Builder/theme save/preview regression |
| `git diff --check` | PASS |

Test dùng `OPENDASH_SITE_ROOT` và `OPENDASH_DB_PATH` riêng trong thư mục tạm; fixture SQLite nguồn được mở `immutable=1` khi clone, không sửa dữ liệu site đang sử dụng.
Lệnh auth smoke đã được chạy lại sau khi dọn hai sidecar SQLite cũ; kết quả PASS và không phát sinh lại file phụ cạnh fixture.

## Giới hạn

- Chưa có giao diện Admin CPT/Entry, template render nội dung công khai, media library, taxonomy, hoặc revision/restore.
- `ContentBeforeSave` mang nghĩa sự kiện trước nỗ lực ghi đã được validate; bản ghi có thể thất bại vì slug trùng/ETag stale. `ContentAfterSave` chỉ phát sau khi ghi thành công. Dispatcher chưa thực thi plugin hook listeners tự động.
- Chưa kiểm chứng trên Windows release hoặc full browser/UI. Giá trị `rich_text` chưa được render/sanitize; tham chiếu `image` và `gallery` chưa xác minh file tồn tại.
