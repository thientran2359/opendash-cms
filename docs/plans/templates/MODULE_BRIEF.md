# <MODULE-ID> — <Tên module> · Living Module Brief (G0–G3)

**Domain:** ... · **Milestone dự kiến:** ... · **PRD:** ... · **Owner nghiên cứu:** ... · **Track:** Full-Track/Fast-Track (lý do)

**Trạng thái:** scope = PROPOSED/AGREED; research = NOT_STARTED/IN_PROGRESS/REVIEWED; design = NOT_STARTED/PROPOSED/AGREED; implementation = NOT_STARTED/IN_PROGRESS/IMPLEMENTED; verification = NOT_RUN/PARTIAL/VERIFIED.

**Quy tắc:** một brief duy nhất cho G0–G3; không tạo file research/task riêng mặc định. Tách research nếu có benchmark/khảo sát dài; dùng ADR khi có quyết định kiến trúc liên module. [Planning Workflow](../PLANNING_WORKFLOW.md) là nguồn quy tắc gate. Không nhầm `AGREED` scope PRD với `AGREED` design G2.

## G0 — Vì sao cần module này

Mục tiêu người dùng, hành vi V1 và giới hạn. Dẫn link PRD. Ghi ít nhất một tình huống thực tế.

## G0 — Capability và điều kiện nghiệm thu

| Capability ID | Hành vi / use case | Primary Acceptance Slice dự kiến | Điều kiện nghiệm thu | Bằng chứng |
| --- | --- | --- | --- | --- |
| <MODULE-ID>.1 | ... | Mx-Sxx (chưa READY nếu G2 chưa chốt) | ... | — |

## G0 — Ranh giới và phụ thuộc

- Module chủ sở hữu dữ liệu/quy tắc: ...
- Cần từ module khác: ...
- Module phụ thuộc vào phần này: ...
- Phần ngoài phạm vi: ...
- Dữ liệu cần bảo toàn, quyền và tình huống thất bại: ...

## G1 — Research (bắt đầu khi đã kiểm tra source)

- Source/CodeGraph: API, caller, file và luồng thực tế; điểm chưa có.
- Phương án A/B, lợi/hại, điều kiện chọn, mẫu thử/measurement nếu cần.
- Questions chưa trả lời và nguồn đáng tin.
- Kết quả đề xuất (PROPOSED) và rủi ro; chỉ link `docs/research/` khi có tài liệu lớn phải tách.

## G2 — Design (chưa tự chốt nếu đang research)

- Data model, migrations, owner/lifecycle, HTTP API, core validation, storage boundaries.
- UI/preview/public, quyền/CSRF, backward compatibility, failure/rollback.
- Các contract/ADR cần chốt và người quyết định. Không đánh dấu AGREED chỉ vì hai AI đồng ý hướng G1.
- Chú ý rollback migration đã chạy: Git revert không rollback SQLite; plan test DB copy, backup và forward migration.

## G3 — Breakdown (chỉ sau G2 hoặc Fast-Track đã có contract)

| Slice ID | Task ID | Đầu ra kiểm chứng được | Dependency/Gate | Trạng thái |
| --- | --- | --- | --- | --- |
| ... | ... | ... | G2 + ... | DRAFT |

G1/G2 là **cổng điều kiện**, không đánh số `Mx-Sxx-Txx` như task implementation. Chỉ chốt ID task và chuyển `READY` khi đủ scope, file ownership, acceptance và kiểm thử. Giữ lịch sử/ánh xạ ID nếu bản nháp cũ đã dùng các số T cho research/design.

## G5 — Verification và bằng chứng

- Phạm vi test theo hành vi và trường hợp negative.
- Lệnh thực chạy, môi trường/commit, kết quả, báo cáo trong `docs/verification/`.
- Giới hạn và công việc kế tiếp.
- Mọi `VERIFIED` gắn commit + môi trường + phạm vi; có thể hạ trạng thái hiện hành khi xuất hiện regression, không xóa evidence cũ.
