# Kế hoạch triển khai

Thư mục này dùng cho kế hoạch cụ thể theo giai đoạn, đặt tên `YYYY-MM-DD-<chu-de>.md`. Một kế hoạch cần có contract liên quan, phạm vi, các bước, tiêu chí nghiệm thu và ghi rõ phần ngoài phạm vi.

## Nhập mã nguồn M1/M2

Đã triển khai trên workspace đích ngày 07/11/2026. Xem [kết quả và giới hạn nghiệm thu](../verification/2026-11-07-m1-m2-import.md). Các bước dưới đây ghi lại trình tự nhập mã.

[Bản đối chiếu từng module với scaffold `0ec91ec`](2026-11-07-m1-m2-module-map.md) ghi các quyết định đề xuất và tiêu chí kiểm tra khi nhập. Phiên bản nguồn được định danh trong [báo cáo kiểm chứng M1/M2](../verification/2026-11-07-m1-m2-source-baseline.md).

1. Xác định repository M1/M2 nguồn và trạng thái Git thực tế; giữ lịch sử nguồn và mọi thay đổi chưa commit. Scaffold đang có là mã mẫu, không thay thế logic đã được kiểm chứng từ repository trước.
2. Chạy baseline ở **repository M1/M2 nguồn** và ghi lại kết quả; dùng [README hiện tại](../../README.md) để kiểm chứng scaffold mới riêng.
3. Đối chiếu `crates/opendash-core`, `crates/opendash-server`, `admin/`, `migrations/`, `fixtures/`, `sdk/`, `examples/`, `scripts/` với [kiến trúc](../architecture/CMS_ARCHITECTURE.md). Chọn lọc đưa module thực sang và bỏ/điều chỉnh mã demo khi đã có implementation; giữ hợp đồng API và chuỗi migrations.
4. Kiểm tra `Cargo.lock`, vị trí `build.rs`, đường dẫn `admin/dist`, macro migrations, đường dẫn fixture và scripts; giữ binary `opendash` và ABI plugin v1.
5. Kiểm tra `cargo fmt --check`, `cargo build --locked`, `cargo test --workspace --locked`, đóng gói và smoke tests tương ứng sau mỗi bước chuyển cấu trúc. Ghi lệnh chạy, kết quả và hạn chế; không đánh dấu `verified` trước khi có bằng chứng.

M3 (Content/Media) chỉ bắt đầu sau khi mã M1/M2 thực và bộ kiểm chứng nền đã được xác nhận. Các module chưa triển khai không tạo file rỗng chỉ để biểu thị tiến độ.

## M2.5 — Bộ khung Lõi (Core Kernel) & Quy chuẩn Code

Xem [kế hoạch chi tiết M2.5](2026-11-07-m2-5-core-kernel-and-standards.md) và hợp đồng [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md). Mốc này chuẩn hóa các hàm cơ bản (`id`, `registry`, `hooks`), tập trung SQL về `storage/`, hợp nhất kiểm tra gói ZIP và thiết lập UI/API primitives trước khi mở M3.
