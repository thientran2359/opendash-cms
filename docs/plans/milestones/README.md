# Kế hoạch từng milestone

Mỗi milestone có kế hoạch ghi phạm vi, module/capability liên quan, dependency, các slice và điều kiện đóng mốc. Khi một milestone có nhiều slice, đặt chúng dưới `<milestone>/slices/` và dùng `<milestone>/README.md` để điều hướng. **Không chuyển plan vào archive chỉ vì source đã triển khai**; task tiến hành theo [GitHub Project #3](https://github.com/users/thientran2359/projects/3), trạng thái milestone theo [Roadmap V1](../ROADMAP_V1.md), bằng chứng theo [verification](../../verification/README.md).

| Mốc | Kế hoạch hoặc tài liệu đầu vào |
| --- | --- |
| M2.5 — Core Foundation | [M2.5-core-foundation.md](M2.5-core-foundation.md); [bằng chứng 07/11/2026](../../verification/2026-11-07-m2-5-core-kernel.md) |
| M3 — Content & Media (**PARTIAL**) | [M3 — tổng quan và sáu slice M3.1–M3.6](M3/README.md); [kế hoạch nhóm task/gates kế tiếp](M3/TASK_PLAN.md); [trạng thái/bằng chứng theo slice](../M3_STATUS_2026-11-09.md). **Không phải task Ready/Done**; tra Issue/Project để xem trạng thái thực |

Khi mở milestone, dùng [Planning Workflow](../PLANNING_WORKFLOW.md), [Module Brief](../templates/MODULE_BRIEF.md) và [Slice Task](../templates/SLICE_TASK.md). [Modules](../modules/README.md) mô tả capability/contract có thể dùng lại qua nhiều milestone. Hồ sơ chỉ mang tính lịch sử lưu tại [archive](../../archive/README.md).

**Quy ước plan task:** `<Milestone>/README.md` điều hướng phạm vi/slices, `<Milestone>/TASK_PLAN.md` (chỉ khi cần) mô tả **candidate, dependency, gate và acceptance** đang chuẩn bị; kế hoạch slice giữ trong `<Milestone>/slices/`. Chỉ tạo **GitHub Issue thực** sau khi task G3 được review/merge và đủ điều kiện. Không tạo file .md riêng cho mỗi task hoặc thư mục `backlog/ready/doing/done/` để sao chép GitHub Project Status; M4–M8 chỉ chi tiết hóa task khi gate nghiên cứu tương ứng đã mở.
