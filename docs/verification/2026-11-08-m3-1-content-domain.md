# Kiểm chứng M3.1 — CPT và Custom Fields (domain)

- **Ngày:** 2026-11-08
- **Môi trường:** macOS Darwin Intel x86_64, Rust/Cargo 1.99.1 (`stable-x86_64-apple-darwin`).
- **Phạm vi:** [M3.1 Content Types & Fields](../plans/2026-11-08-m3-content-types-and-fields.md), chỉ module `opendash-core`.

## Thay đổi đã thực hiện

- `cms/crates/opendash-core/src/content.rs`: thêm định nghĩa CPT và custom field, 9 field type V1, trường lựa chọn và `validate()`; dùng bộ kiểm tra ID gốc `id::valid_snake_key`.
- Thêm unit test serialize/deserialize JSON, chín loại field và trường hợp schema/options không hợp lệ.
- Chưa thay migration, API, Admin UI hay fixture SQLite cũ; chưa coi M3 thành chức năng có thể dùng từ giao diện.

## Kiểm chứng trên mã thực

| Lệnh tại root repo | Kết quả |
| --- | --- |
| `~/.cargo/bin/cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `~/.cargo/bin/cargo test --manifest-path cms/Cargo.toml -p opendash-core --locked` | PASS: 14 unit tests, 0 failed; 0 doc tests |
| `npm ci --no-audit --no-fund` trong `cms/admin/` (Node v24.22.2) | PASS: cài 67 packages theo lockfile |
| `npm run build` trong `cms/admin/` | PASS: TypeScript/Vite build, tạo `cms/admin/dist` cho `RustEmbed` |
| `~/.cargo/bin/cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS: 14 unit tests core + 33 unit tests server, 0 failed |
| `git diff --check` | PASS |

**Giới hạn:** Core domain và regression unit tests đã chạy trên Mac; chưa chạy smoke test Windows, kiểm thử trình duyệt Admin hoặc thực hiện một vòng CRUD CPT qua HTTP. `cargo test --workspace` ban đầu không build được vì thiếu `cms/admin/dist`, sau đó đã build Admin và chạy lại thành công. Nội dung SQLite/HTTP/UI thuộc các slice tiếp theo.
