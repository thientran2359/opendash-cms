# Kế hoạch triển khai

Kế hoạch tách biệt với kết quả thực chạy. Khi một bước có bằng chứng trên mã thực, ghi sang [verification](../verification/README.md).

## Thứ tự nhập mã nguồn M1/M2

Trước khi đưa mã nguồn M1/M2 sang `/opendash-cms`:

1. Chạy lại bộ kiểm chứng tại nguồn (`C:\Users\Thientran\Local Sites\opendash`) và ghi báo cáo vào [verification](../verification/README.md). Đã thực hiện tại [2026-11-07-m1-m2-source-baseline.md](../verification/2026-11-07-m1-m2-source-baseline.md).
2. Lập bảng ánh xạ từng file từ `src/` sang `crates/opendash-core` và `crates/opendash-server` theo [CMS_ARCHITECTURE](../architecture/CMS_ARCHITECTURE.md). Đã lập tại [2026-11-07-m1-m2-module-map.md](2026-11-07-m1-m2-module-map.md).
3. Chuyển migrations `0001`–`0003` và fixture `baseline_0001.sqlite` nguyên trạng, không tạo lại.
4. Tách `admin/src/` từ bundle một file thành `api/`, `shell/`, `builder/`, `features/` mà giữ nguyên các DOM selector đang được smoke test dùng.
5. Chuyển `sdk/as`, `examples/plugins/sample-plugin` và các script kiểm thử Windows; sửa đường dẫn theo cấu trúc workspace mới.
6. Chạy đầy đủ `cargo test`, `build.ps1 -Release` và 5 bộ smoke test trên Windows, ghi kết quả mới tại đích.

M3 (Content/Media) chỉ bắt đầu sau khi mã M1/M2 thực và bộ kiểm chứng hồi quy chạy xanh trên workspace mới. Đã hoàn tất bước 3–6 tại [2026-11-07-m1-m2-import.md](../verification/2026-11-07-m1-m2-import.md).

## M2.5 — Bộ khung Lõi (Core Kernel) & Quy chuẩn Code

Xem [kế hoạch chi tiết M2.5](2026-11-07-m2-5-core-kernel-and-standards.md), quyết định [ADR-0002](../decisions/ADR-0002-core-extension-contract.md), hợp đồng [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md) và [báo cáo kiểm chứng M2.5](../verification/2026-11-07-m2-5-core-kernel.md). Mốc này đã hoàn tất và kiểm chứng đầy đủ cả 8 mục hoàn thiện: đồng bộ Base APIs thật, quy chuẩn phân tầng (`routes` → `page` → `core`/`storage` → `hooks`), `RegistryCatalog` gắn quyền sở hữu cho block M1, `HookDispatcher` điều phối sự kiện sau commit tới core/plugin handler, đưa toàn bộ SQL (gồm cả `plugin_kv`) về `storage/`, cùng bộ kiểm tra ranh giới tự động `check-standards.py` và smoke test `windows_kernel_smoke.py`.
