# Chuyển docs ra thư mục gốc — 07/11/2026

## Phạm vi

Theo yêu cầu của chủ dự án, di chuyển nguyên thư mục `cms/docs/` thành `docs/` ngay tại root `opendash-cms/`. Mã nguồn và công cụ build tiếp tục ở `cms/`; `database/`, `themes/`, `plugins/`, `uploads/` và `.agents/` không thay đổi.

## Thực hiện

- Kiểm tra `docs/` chưa tồn tại tại root, bảo toàn 13 file gốc và xác minh SHA-256 của chúng trước khi sửa liên kết.
- Di chuyển `cms/docs/` → `docs/`; cập nhật các Markdown links tương đối và những quy ước cấu trúc trong root `README.md`, `AGENTS.md`, `cms/README.md`, `.agents/README.md`, ADR, kiến trúc và giao thức thảo luận.
- Không thay đổi logic Rust, React, SQLite, scripts build hay dữ liệu của website.

## Kiểm chứng

| Kiểm tra | Kết quả |
| --- | --- |
| `docs/README.md` và `docs/architecture/CMS_ARCHITECTURE.md` tồn tại | PASS |
| Không còn thư mục `cms/docs/` | PASS |
| 13 file tài liệu được giữ nguyên byte trước khi cập nhật links | PASS |
| 39 file Markdown được rà, 146 liên kết nội bộ được kiểm tra | PASS — không có link hỏng |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS — 4/4 unit tests |

Git chưa được khởi tạo tại workspace, nên chưa thể dùng `git diff --check`. Thay đổi chỉ liên quan cấu trúc tài liệu và đường dẫn tham chiếu.
