# OpenDash — Mục lục tài liệu

Tài liệu này nằm tại `docs/` ở gốc workspace `/opendash-cms`, độc lập với mã nguồn trong `cms/`. Đọc [README gốc](../README.md) và [hướng dẫn phát triển CMS](../cms/README.md) để biết cách chạy OpenDash và phạm vi chưa triển khai.

## Nguồn quyết định và thứ tự ưu tiên

1. Chỉ đạo mới nhất của chủ dự án.
2. [Yêu cầu sản phẩm V1](product/PRODUCT_REQUIREMENTS.md) — phạm vi sản phẩm đã duyệt.
3. [Tech stack](architecture/TECH_STACK.md) — các lựa chọn kỹ thuật đã duyệt.
4. [Kiến trúc CMS](architecture/CMS_ARCHITECTURE.md) và [ADR](decisions/ADR-0001-workspace-layout.md) — ranh giới module và cấu trúc đích.
5. [Bộ khung lõi & Quy chuẩn code](contracts/CORE_FOUNDATION.md) và [Plugin ABI v1](contracts/PLUGIN_ABI.md) — contract core kernel, quy chuẩn phân tầng và runtime/SDK.

## Tra cứu theo mục đích

| Mục đích | Vị trí | Quy ước |
| --- | --- | --- |
| Phạm vi và hành vi sản phẩm | [`product/`](product/PRODUCT_REQUIREMENTS.md) | Đã xác nhận, thay đổi cần chỉ đạo của chủ dự án |
| Kiến trúc và lựa chọn stack | [`architecture/`](architecture/CMS_ARCHITECTURE.md) | Ghi rõ đề xuất và quyết định; [CMS_PROPOSAL](architecture/CMS_PROPOSAL.md) chỉ tham khảo có chọn lọc |
| Nghiên cứu chưa chốt | [`research/`](research/DAYPRESS_EMDASH_RESEARCH.md) — Daypress & EmDash | Bóc tách source, khả năng tái sử dụng, rủi ro, nguồn pin commit; trạng thái `RESEARCH / PROPOSED`, chưa thành quyết định |
| Giao diện kỹ thuật giữa các phần | [`contracts/`](contracts/CORE_FOUNDATION.md) — [Core Foundation](contracts/CORE_FOUNDATION.md) · [Plugin ABI](contracts/PLUGIN_ABI.md) | Contract có version, trạng thái `AGREED` khác `verified` |
| Quyết định kiến trúc | [`decisions/`](decisions/ADR-0001-workspace-layout.md) | ADR có bối cảnh, quyết định, hệ quả và trạng thái |
| Kế hoạch triển khai | [`plans/`](plans/README.md) | Mỗi kế hoạch nêu phạm vi, mốc và tiêu chí nghiệm thu |
| Bằng chứng thực thi | [`verification/`](verification/README.md) | Chỉ ghi `verified` khi có lệnh, môi trường và kết quả thực tế |
| Thảo luận giữa AI | [Giao thức](discussions/PROTOCOL.md) · [Điều phối agent](discussions/AGENT_COORDINATION.md) | Giữ lịch sử theo lượt, phân công qua Prime, không tự coi đồng thuận AI là chỉ đạo chủ dự án |

Không tạo thư mục/module rỗng chỉ để khớp sơ đồ. Khi nguồn mã được đưa vào, đối chiếu cây thư mục và kiểm thử trước khi sửa tài liệu trạng thái.

Xem [báo cáo chuyển thư mục ngày 07/11/2026](verification/2026-11-07-layout-migration.md) cho thay đổi `cms/`, `database/` và các kiểm chứng sau di chuyển.
