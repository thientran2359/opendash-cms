# Kiểm kê workspace `/opendash-cms` — 2026-11-06

**Mục đích:** Ghi lại hiện trạng trước khi đưa mã nguồn CMS vào workspace mới.

| Hạng mục | Kết quả kiểm tra |
| --- | --- |
| Root | Có `README.md`, `AGENTS.md`, `docs/`, `.agents/` |
| Tài liệu lõi | Có `PRODUCT_REQUIREMENTS`, `TECH_STACK`, `CMS_ARCHITECTURE`, `CMS_PROPOSAL`, `PLUGIN_ABI` và `PROTOCOL` |
| Source Rust / React | Chưa có `Cargo.toml`, `crates/` và `admin/` |
| Build/test scripts | Chưa có `scripts/`, fixtures và migrations |
| Git metadata | Lệnh `git status --short` trả lỗi `not a git repository` trước khi chuẩn hóa |
| Code build, test, smoke | NOT RUN — mã nguồn và script chưa có trong workspace này |

Phạm vi kiểm kê là thư mục `/opendash-cms` hiện tại; tài liệu mô tả các kiểm chứng trước đó ở repository nguồn không được xem là bằng chứng đã chạy tại đây. Khi import source, bổ sung báo cáo kiểm thử mới và cập nhật trạng thái trong README.
