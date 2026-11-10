# OpenDash — Mục lục tài liệu

Tài liệu này nằm tại `docs/` ở gốc workspace `/opendash-cms`, độc lập với mã nguồn trong `cms/`. Đọc [README gốc](../README.md) và [hướng dẫn phát triển CMS](../cms/README.md) để biết cách chạy OpenDash và phạm vi chưa triển khai.

## Nguồn quyết định và thứ tự ưu tiên

1. Chỉ đạo mới nhất của chủ dự án.
2. [Yêu cầu sản phẩm V1](product/PRODUCT_REQUIREMENTS.md) — phạm vi sản phẩm đã duyệt.
3. [Tech stack](architecture/TECH_STACK.md) — các lựa chọn kỹ thuật đã duyệt.
4. [Kiến trúc CMS](architecture/CMS_ARCHITECTURE.md), [ADR-0001](decisions/ADR-0001-workspace-layout.md), [ADR-0002](decisions/ADR-0002-core-extension-contract.md) và [ADR-0003](decisions/ADR-0003-agent-consensus-governance.md) — ranh giới module, cấu trúc đích, core extension và agent technical decision authority.
5. [Bộ khung lõi & Quy chuẩn code](contracts/CORE_FOUNDATION.md) và [Plugin ABI v1](contracts/PLUGIN_ABI.md) — contract core kernel, quy chuẩn phân tầng và runtime/SDK.

## Tra cứu theo mục đích

| Mục đích | Vị trí | Quy ước |
| --- | --- | --- |
| Phạm vi và hành vi sản phẩm | [`product/`](product/PRODUCT_REQUIREMENTS.md) | Đã xác nhận, thay đổi cần chỉ đạo của chủ dự án |
| Kiến trúc và lựa chọn stack | [`architecture/`](architecture/CMS_ARCHITECTURE.md) | Ghi rõ đề xuất và quyết định; [CMS_PROPOSAL](architecture/CMS_PROPOSAL.md) chỉ tham khảo có chọn lọc |
| Nghiên cứu, thảo luận chưa duyệt | [`.agents/research/`](../.agents/research/README.md) và [`.agents/discussions/`](../.agents/discussions/README.md) | Lưu bản nháp ngoài `docs/`; tài liệu lịch sử còn giữ để truy vết nguồn |
| Giao diện kỹ thuật giữa các phần | [`contracts/`](contracts/CORE_FOUNDATION.md) — [Core Foundation](contracts/CORE_FOUNDATION.md) · [Plugin ABI](contracts/PLUGIN_ABI.md) | Contract có version, trạng thái `AGREED` khác `verified` |
| Quyết định kiến trúc & điều phối | [`decisions/`](decisions/ADR-0001-workspace-layout.md) — [ADR-0001](decisions/ADR-0001-workspace-layout.md) · [ADR-0002](decisions/ADR-0002-core-extension-contract.md) · [ADR-0003](decisions/ADR-0003-agent-consensus-governance.md) | ADR có bối cảnh, quyết định, hệ quả, reviewer nếu cần và trạng thái |
| Roadmap và danh mục module | [Roadmap V1](plans/ROADMAP_V1.md) · [Module Catalog](plans/MODULE_CATALOG_V1.md) · [Quy trình](plans/PLANNING_WORKFLOW.md) | Hai trục chức năng và tiến độ, ID module, research/design gates, mẫu task |
| Kế hoạch triển khai | [`plans/`](plans/README.md) · [Milestones](plans/milestones/README.md) | Mỗi kế hoạch nêu phạm vi, mốc và tiêu chí nghiệm thu; có mẫu module và task |
| Hồ sơ lịch sử đã đóng | [`archive/`](archive/README.md) | Lưu hồ sơ chuyển đổi/migration đã hoàn tất, giữ liên kết đến verification |
| Bằng chứng thực thi | [`verification/`](verification/README.md) | Chỉ ghi `verified` khi có lệnh, môi trường và kết quả thực tế |
| Điều phối agent | [Giao thức hiện hành](discussions/PROTOCOL.md) · [Quy trình GitHub](../.agents/GITHUB_WORKFLOW.md) · [Handoff cũ](discussions/AGENT_COORDINATION.md) | Handoff trước đây chỉ để tham khảo, không còn là cơ chế hoạt động |

Không tạo thư mục/module rỗng chỉ để khớp sơ đồ. Khi nguồn mã được đưa vào, đối chiếu cây thư mục và kiểm thử trước khi sửa tài liệu trạng thái.

Xem [báo cáo chuyển thư mục ngày 07/11/2026](verification/2026-11-07-layout-migration.md) cho thay đổi `cms/`, `database/` và các kiểm chứng sau di chuyển.
