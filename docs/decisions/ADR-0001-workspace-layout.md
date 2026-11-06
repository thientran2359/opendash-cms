# ADR-0001 — Cấu trúc Rust workspace và giao diện Admin

- **Ngày:** 2026-11-06
- **Trạng thái:** AGREED — đã tạo scaffold 2 crate dưới `cms/` và đặt dữ liệu website ở gốc; chưa triển khai đầy đủ CMS V1.
- **Phạm vi:** Tổ chức repository và ranh giới phụ thuộc; không thay đổi tính năng V1.

## Bối cảnh

OpenDash là CMS Rust + SQLite, đóng gói một executable cho mỗi website, hỗ trợ React Admin/Builder, theme ZIP và plugin Wasmi từ V1. Repository cần cấu trúc đủ rõ để phát triển song song và tránh phụ thuộc vòng khi thêm CPT, fields, media và builder.

## Quyết định

- Dùng **Rust workspace gồm hai crate**: `crates/opendash-core` (domain/schema/validation thuần Rust) và `crates/opendash-server` (Axum, SQLite, MiniJinja, Wasmi, API và binary `opendash`). Chỉ tách thêm crate khi có ranh giới và nhu cầu thực tế.
- Giao diện React + TypeScript + Vite ở `admin/`: `src/api/` cho HTTP client chung, `src/shell/` cho khung Admin, `src/builder/` cho visual builder, `src/features/` cho các chức năng quản trị. Tạo module khi bắt đầu triển khai.
- Gom toàn bộ mã nguồn và công cụ phát triển vào `cms/`: `crates/`, `admin/`, `sdk/`, `examples/`, `migrations/`, `fixtures/`, `scripts/`, `spikes/`, `Cargo.toml` và `Cargo.lock`; vẫn giữ các đường dẫn tương đối nội bộ để dùng lại fixtures và scripts.
- Đặt `docs/` trực tiếp tại gốc dự án để tài liệu sản phẩm, kiến trúc và hướng dẫn AI dễ truy cập, độc lập với source code trong `cms/`.
- Đặt `themes/`, `plugins/`, `uploads/` và `database/` **ngay ở cấp gốc website** để dễ tìm; `database/` là tên mới của `data/`. Từng theme/plugin được cài vào thư mục con riêng khi tính năng ZIP được triển khai. Trong repo scaffold, nội dung website bị Git bỏ qua.
- Dữ liệu runtime của website tách khỏi mã nguồn; `.agents/` giữ workflow Codex ↔ Antigravity, `.agents/sync/` chỉ chứa trạng thái tạm.
- Phân loại tài liệu theo `product/`, `architecture/`, `contracts/`, `decisions/`, `plans/`, `verification/`, `discussions/` và ghi rõ `AGREED` / `implemented` / `verified`.

## Hệ quả và ràng buộc

- `opendash-core` không phụ thuộc Axum, SQLx, Wasmi hay I/O hệ thống.
- Preview chưa lưu và trang công khai dùng cùng nguyên tắc render; bấm Lưu cập nhật nội dung công khai. CPT/fields và bố cục do website sở hữu, không bị mất khi đổi theme hoặc tắt plugin.
- Giữ nguyên tên binary `opendash`, chuỗi migrations và plugin ABI v1 khi chuyển source; kiểm tra lại các đường dẫn `build.rs`, `rust-embed`, `sqlx::migrate!`, fixtures và smoke scripts.
- Các lệnh Cargo từ root dùng `--manifest-path cms/Cargo.toml`; `.vscode/settings.json` khai báo `rust-analyzer.linkedProjects` trỏ vào `cms/Cargo.toml` để IDE nhận diện workspace ngay từ gốc; scripts build tại `cms/scripts/`. Mặc định server ghi `database/opendash.sqlite` trong thư mục website cả khi chạy từ `cms/`. Khi nhập M1/M2 và triển khai V1, toàn bộ đường dẫn runtime và thư mục staging giải nén cùng ổ đĩa (`runtime/staging/`) được quản lý tập trung qua `SitePaths` theo [CMS_ARCHITECTURE.md](../architecture/CMS_ARCHITECTURE.md).
- Scaffold chỉ minh họa kết nối Rust/React/SQLite/MiniJinja, không chứng minh đã nhập hoặc kiểm chứng hệ CMS M1/M2. Xem [kiểm kê trước scaffold](../verification/2026-11-06-repository-inventory.md), [báo cáo kiểm chứng](../verification/README.md) và [kế hoạch nhập mã](../plans/README.md).

## Tham chiếu

- [Yêu cầu V1](../product/PRODUCT_REQUIREMENTS.md)
- [Cấu trúc và kế hoạch chuyển đổi chi tiết](../architecture/CMS_ARCHITECTURE.md)
- [Tech stack đã duyệt](../architecture/TECH_STACK.md)
