# Kiểm chứng M3.2 — CPT storage + API backend

- **Ngày:** 2026-11-08
- **Môi trường:** macOS Darwin Intel x86_64; Rust/Cargo 1.99.1, Python 3 và Admin assets đã build từ Node.js 24.
- **Phạm vi:** [M3.2](../plans/milestones/M3/slices/2026-11-08-m3-2-content-types-backend.md), chạy trên mã `/local-sites/opendash-cms` hiện tại.

## Thay đổi

- SQLx migration `0004_content_types.sql` lưu CPT, field schema, nguồn cung cấp và trạng thái. `0001`–`0003` cùng fixture gốc không sửa.
- `storage/content.rs` chứa truy vấn parameterized, ánh xạ dữ liệu JSON và `ResourceOwner`/`ProviderStatus`, trả 409 cho khóa đã tồn tại.
- `routes/content.rs` cùng `app.rs` cung cấp `GET` (Admin/Editor) và `POST` (Admin + CSRF) ở `/api/admin/content-types`; giới hạn 512 KiB trên JSON body.
- Thêm smoke test riêng dùng site SQLite tạm và fixture nâng cấp; các thay đổi ngoài slice M3.2 hiện có trong working tree được giữ nguyên.

## Lệnh đã thực thi và kết quả

| Lệnh (tại root nếu không chỉ rõ khác) | Kết quả |
| --- | --- |
| `~/.cargo/bin/cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `~/.cargo/bin/cargo build --manifest-path cms/Cargo.toml --locked` | PASS |
| `~/.cargo/bin/cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — 14 tests core + 33 tests server, 0 failures |
| `python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash` | PASS — fresh site, auth, role, CSRF, Origin, bad schema, duplicate/race, giới hạn body, JSON, restart, provider inactive, upgrade legacy |
| `python3 scripts/windows_auth_smoke.py --binary target/debug/opendash` (từ `cms/`, chạy trên Mac) | PASS — setup race, auth/RBAC, CSRF, Origin, expiry và upgrade từ fixture |
| `git diff --check` | PASS |

## Giới hạn

- API thực tế được kiểm chứng trên macOS với binary debug; chưa chạy Windows release, bộ smoke test Windows còn lại, hoặc browser UI.
- Chưa triển khai entry CRUD, cập nhật/xóa schema, tự động đăng ký provider qua theme/plugin, ảnh/media, taxonomy hay giao diện Admin CPT.
- Test sử dụng các site và SQLite riêng trong thư mục tạm. Dữ liệu website chính không được dùng để kiểm thử.
