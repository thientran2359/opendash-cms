# M2.5 — Ánh xạ task Core Foundation

**Nguồn thẩm quyền:** [kế hoạch M2.5 mục 4.1–4.8](../M2.5-core-foundation.md), [verification](../../../verification/2026-11-07-m2-5-core-kernel.md), [ADR-0002](../../../decisions/ADR-0002-core-extension-contract.md). Roadmap ghi **M2.5 closed trong scope này**; các mã `M2.5-Pxx` là nhãn tra cứu, không tạo task thực thi mới.

| Candidate | Phần của milestone plan gốc | Module | Dependency | Acceptance cần đối chiếu |
| --- | --- | --- | --- | --- |
| M2.5-P01 | §4.1 Contract/Base APIs thật | FND-01 | M1/M2 baseline | Hợp đồng API lõi và ví dụ dùng thực tế |
| M2.5-P02 | §4.2 Quy chuẩn tổ chức code | FND-01/03 | P01 | Storage/route/validation có ranh giới có thể kiểm chứng |
| M2.5-P03 | §4.3 Registry ownership | FND-01 | P01/P02 | ResourceOwner, provider lifecycle không mất dữ liệu |
| M2.5-P04 | §4.4 Hook/Action thực tế | PLG-02, FND-01 | P03, PLG-01 | Dispatch đúng lúc/sau commit, timeout/cancellation có hồi quy |
| M2.5-P05 | §4.5 Đưa luồng có sẵn qua core | FND-01/05 | P02..P04 | Builder/Plugin/Auth giữ hành vi cũ qua shared helpers |
| M2.5-P06 | §4.6 Khép bất đồng Base APIs | FND-01/03 | P01..P05 | Guard/Error/ZIP/SQL boundary không phá migration/ABI |
| M2.5-P07 | §4.7 Mẫu mở rộng + checker | FND-01/05 | P05/P06 | `check-standards.py` + negative self-test cho vi phạm |
| M2.5-P08 | §4.8 Full regression + G5 | FND-01/05, PLG-02 | P01..P07 | Windows smoke/CI/tests/evidence trên SHA báo cáo |

**Giới hạn:** plugin content hooks đầy đủ thuộc M3/M6, Admin màn V1 mở rộng thuộc M3–M7; các task này không tự cấp G3 Ready/Done trên GitHub Project.
