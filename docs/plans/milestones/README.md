# Kế hoạch từng milestone

Mỗi milestone có kế hoạch ghi phạm vi, module/capability liên quan, dependency, các slice và điều kiện đóng mốc. Khi một milestone có nhiều slice, đặt chúng dưới `<milestone>/slices/` và dùng `<milestone>/README.md` để điều hướng. **Không chuyển plan vào archive chỉ vì source đã triển khai**; task tiến hành theo [GitHub Project #3](https://github.com/users/thientran2359/projects/3), trạng thái milestone theo [Roadmap V1](../ROADMAP_V1.md), bằng chứng theo [verification](../../verification/README.md).

**Bắt đầu ở [Coverage Matrix — 27 module → 9 milestone → task](COVERAGE_MATRIX.md).** Đây là kế hoạch V1 đầy đủ mà chủ dự án yêu cầu; ưu tiên A0/A1/B/C/D/E trong [Issue #15](https://github.com/thientran2359/opendash-cms/issues/15) chỉ là một phần của M3/M4, **không phải toàn bộ kế hoạch sản phẩm**. Cả **9 GitHub Milestones đã tồn tại** (M1=1, M2=2, M2.5=3, M3=4, M4=5, M5=6, M6=7, M7=8, M8=9); **không tạo lại** hoặc suy Issue đã có vì milestone đang có.

| Mốc | Scope và kế hoạch | Task candidates | Hiện trạng evidence |
| --- | --- | --- | --- |
| M1 — CMS Baseline | [M1](M1/README.md) | [M1-P01..P06](M1/TASK_PLAN.md) | Windows baseline từ M1/M2 report |
| M2 — Plugin Sandbox | [M2](M2/README.md) | [M2-P01..P05](M2/TASK_PLAN.md) | Windows baseline từ M1/M2 report |
| M2.5 — Core Foundation | [M2.5](M2.5/README.md), [plan gốc](M2.5-core-foundation.md) | [M2.5-P01..P08](M2.5/TASK_PLAN.md) | Roadmap CLOSED/VERIFIED đúng scope M2.5 |
| M3 — Content & Media | [M3](M3/README.md), [sáu slice có code](M3/README.md#các-slice-và-nguồn-kiểm-chứng) | [M3-P01..P15](M3/TASK_PLAN.md) | PARTIAL; còn taxonomy/bridge/media/parity |
| M4 — Visual Builder | [M4](M4/README.md) | [M4-P01..P08](M4/TASK_PLAN.md) | PLANNED |
| M5 — Theme & Templates | [M5](M5/README.md) | [M5-P01..P08](M5/TASK_PLAN.md) | PLANNED |
| M6 — Plugin Extensions | [M6](M6/README.md) | [M6-P01..P07](M6/TASK_PLAN.md) | PLANNED |
| M7 — Site Features | [M7](M7/README.md) | [M7-P01..P07](M7/TASK_PLAN.md) | PLANNED |
| M8 — Operations & Release | [M8](M8/README.md) | [M8-P01..P07](M8/TASK_PLAN.md) | PLANNED |

Khi mở milestone, dùng [Planning Workflow](../PLANNING_WORKFLOW.md), [Module Brief](../templates/MODULE_BRIEF.md) và [Slice Task](../templates/SLICE_TASK.md). [Modules](../modules/README.md) mô tả capability/contract có thể dùng lại qua nhiều milestone. Hồ sơ chỉ mang tính lịch sử lưu tại [archive](../../archive/README.md).

**Quy ước plan task:** `<Milestone>/README.md` điều hướng mục tiêu, module và slices; `<Milestone>/TASK_PLAN.md` mô tả các **candidate có mã `Mx-Pxx`**, dependency, gate, acceptance. **`Mx-Pxx` không phải ID task G3**; chỉ sau khi cụ thể hóa G2/G3 theo [SLICE_TASK](../templates/SLICE_TASK.md), review/merge và cấp `Mx-Sxx-Txx` riêng hợp lệ mới dùng CLI xuất Issue. Kế hoạch slice triển khai giữ trong `<Milestone>/slices/` khi thực sự có slice, không tạo file hoặc thư mục rỗng chỉ để đủ hình thức. Không tạo thư mục `backlog/ready/doing/done`: GitHub Project là nguồn trạng thái live, không nhân bản ở Markdown.
