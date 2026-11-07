# Quy trình quản lý roadmap, module và task của OpenDash

**Trạng thái tài liệu:** Quy trình planning V1 được chủ dự án cho áp dụng sau [5 lượt thảo luận Codex ↔ Antigravity](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) (`CONSENSUS_REACHED` **cho quy trình**, không phải cho thiết kế M3). Quy ước ID, gates và status không thay thế [yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md), [hợp đồng kỹ thuật](../contracts/CORE_FOUNDATION.md) hoặc quyết định có ADR.

Tài liệu này dành cho người quản lý dự án và các coding agent. Mục tiêu là nhìn được toàn bộ chức năng cần có, tiến độ từng phiên bản, lý do của quyết định kỹ thuật và bằng chứng nghiệm thu; cùng một module có thể được triển khai trong nhiều milestone.

## Bắt đầu một lượt làm việc

1. Đọc [roadmap V1](ROADMAP_V1.md), xác định milestone đang mở và điều kiện chuyển bước.
2. Tìm module theo ID trong [danh mục module](MODULE_CATALOG_V1.md); kiểm tra phạm vi đã có, dependency và các tài liệu liên quan.
3. Với module chưa đủ thiết kế, duy trì **một Living Module Brief** theo [mẫu](templates/MODULE_BRIEF.md): chứa G0 scope, G1 research tóm tắt, G2 decisions và G3 breakdown; chỉ tách `docs/research/` khi cần khảo sát lớn, benchmark hoặc bảo toàn dữ liệu đối chiếu.
4. Phân loại **Full-Track/Fast-Track** trước khi giao việc. Với Full-Track, phải qua G1 và chốt G2 trước khi chia task G3. ADR/contract chỉ cập nhật nếu có quyết định liên module hoặc thay đổi hợp đồng hiện hữu. Task ở dạng bảng trong module brief; [mẫu task](templates/SLICE_TASK.md) dùng để handoff, không mặc định tạo file task riêng.
5. Chỉ bắt đầu thay đổi source sau khi nêu được contract, dependency, cách dùng lại API lõi, cách kiểm thử, dữ liệu cần bảo toàn và phần ngoài phạm vi. Ghi bằng chứng chạy thực tế tại `docs/verification/`.

## Hai trục kế hoạch độc lập

| Trục | Cấp | Trả lời câu hỏi |
| --- | --- | --- |
| **Danh mục chức năng** | Product → Domain → Module → Capability | CMS có những phần nào, ai sở hữu, module phụ thuộc phần nào? |
| **Lộ trình thực hiện** | Release → Milestone → Slice → Task | Trong lần triển khai này sẽ thực hiện lát cắt nào và khi nào mới được nghiệm thu? |

- **Product**: V1 cho blog và website giới thiệu, được ràng buộc bởi PRD.
- **Domain**: nhóm tính năng có trách nhiệm thống nhất, ví dụ Content, Builder hoặc Plugin.
- **Module**: thành phần có ranh giới nghiệp vụ ổn định, ID như `CNT-01`. Module có thể kéo dài M3, M4 và M6.
- **Capability**: hành vi đầu ra có thể nghiệm thu, ví dụ “định nghĩa CPT bởi plugin sống qua restart”. Không cần có một file Rust riêng cho mỗi capability.
- **Release**: bộ sản phẩm có thể phân phối. **Milestone**: nhóm kết quả có điều kiện đóng mốc. **Slice**: đường chức năng xuyên các tầng có thể chạy và kiểm chứng. **Task**: công việc đủ hẹp để một agent triển khai/review độc lập.

Ví dụ truy vết: `V1 → M3 → M3-S01 → M3-S01-T01` dự kiến triển khai một phần `CNT-01.1`, phụ thuộc `FND-01`; phải qua G2 mới đánh dấu task `READY`. Khi M6 cần nối plugin registration, tiếp tục tham chiếu `CNT-01` nhưng ghi capability khác thuộc `PLG-03` cho phần bridge, không đếm lại năng lực CPT của `CNT-01`.

## Quy tắc ID và liên kết

| Đối tượng | Cách đặt | Ví dụ |
| --- | --- | --- |
| Module | Mã Domain + số tăng dần, không đổi khi dời milestone | `CNT-01` |
| Capability | Module ID + `.` + số | `CNT-01.1` |
| Milestone | `M` + số, mốc trung gian có dạng `M2.5` | `M3` |
| Slice | Milestone + `-S` + số | `M3-S01` |
| Task | Slice + `-T` + số | `M3-S01-T02` |
| Tài liệu nghiên cứu | `docs/research/<module>-<topic>.md` | `docs/research/cnt-01-cpt-data-model.md` |
| Kế hoạch milestone | `docs/plans/milestones/<Mốc>-<chủ-đề>.md` | `milestones/M2.5-core-foundation.md` |

Mọi slice ghi rõ module/capability liên quan, các dependency chặn triển khai, đầu ra và link đến PRD, contract, quyết định, kiểm chứng. Các ID đã sử dụng không đổi hoặc tái cấp phát; nếu tách module thì ghi lịch sử kế thừa.

**Primary Acceptance Slice:** mỗi capability ID có đúng một slice nghiệm thu chính tại phạm vi/release cụ thể. Một module có thể xuất hiện trong nhiều milestone, nhưng không nhân đôi tiến độ capability. Các kiểm thử hồi quy sau đó bổ sung evidence mà không tạo capability giả.

## Trạng thái cần lưu riêng

Không dùng một checkbox duy nhất để đánh dấu “xong module”. Module có thể có một capability đã kiểm chứng và các capability khác còn nằm ở bước nghiên cứu.

| Thuộc tính | Giá trị hợp lệ | Ý nghĩa |
| --- | --- | --- |
| `scope` | `PROPOSED`, `AGREED` | Danh mục/ưu tiên do AI đề xuất hay đã được chủ dự án chốt |
| `research` | `NOT_STARTED`, `IN_PROGRESS`, `REVIEWED` | Đã khảo sát phương án, source và dependency hay chưa |
| `design` | `NOT_STARTED`, `PROPOSED`, `AGREED` | Giao diện, contract, migration và luồng dữ liệu đã chốt chưa |
| `implementation` | `NOT_STARTED`, `IN_PROGRESS`, `IMPLEMENTED` | Có code dùng được trong phạm vi được chỉ rõ hay chưa |
| `verification` | `NOT_RUN`, `PARTIAL`, `VERIFIED` | Có bằng chứng thực thi đúng phạm vi hay chưa |
| `milestone` | `PLANNED`, `ACTIVE`, `CLOSED` | Trạng thái điều phối mốc, độc lập với trạng thái module |

“`AGREED`” đối với phạm vi PRD không đồng nghĩa đã chốt kiến trúc của module. “`IMPLEMENTED`” không tự nâng thành “`VERIFIED`”. “`VERIFIED`” phải kèm đường dẫn kiểm chứng, mã nguồn/commit hoặc branch được chạy, môi trường, lệnh và phạm vi; không phải nhãn bất biến: khi phát hiện regression, cập nhật trạng thái hiện hành thành `PARTIAL` hoặc nêu blocker và **giữ nguyên bằng chứng lịch sử**. Với tính năng mới, trạng thái ban đầu là `NOT_STARTED` trừ khi có chứng cứ.

## Cổng chuyển bước (gates)

| Gate | Đầu vào | Điều kiện qua gate |
| --- | --- | --- |
| G0 — Scope | PRD và danh mục module | Rõ hành vi V1, người dùng, giới hạn, module owner và dependency; chốt scope khi có thay đổi yêu cầu |
| G1 — Research | Source/CodeGraph, contracts, phương án | Đã khảo sát hiện trạng, caller, đường dữ liệu, các lựa chọn và rủi ro; nêu phương án đề xuất cùng lý do |
| G2 — Design | Kết quả research | API/schema/ownership/migration/permission, luồng lỗi và hành vi bảo toàn được chốt; ADR nếu có quyết định kiến trúc |
| G3 — Breakdown | Thiết kế đã chốt | Có slice chạy được, task nhỏ, dependency, file liên quan, tiêu chí nghiệm thu và test thực tế |
| G4 — Implementation | Task đã sẵn sàng | Code đúng contract, tự kiểm tra phù hợp; không thay đổi dữ liệu/ABI ngoài phạm vi |
| G5 — Verification | Code và fixture/binary phù hợp | Bằng chứng theo hành vi, kiểm thử hồi quy liên quan, giới hạn đã ghi; đóng milestone nếu tất cả capability bắt buộc đạt |

Research có thể chạy trước ở 1–2 module kế tiếp trong khi milestone hiện tại đang code. Implementation module phụ thuộc chỉ mở sau khi gate tương ứng đã thỏa; không yêu cầu toàn bộ V1 phải thiết kế chi tiết trước.

## Chọn Full-Track hoặc Fast-Track

| Chế độ | Khi nào áp dụng | Điều kiện |
| --- | --- | --- |
| **Full-Track** | Thêm/sửa schema hoặc migration SQLite; cấu trúc dữ liệu `SitePaths/uploads/backups`; API/kiểu dữ liệu công khai và validation trong core; ABI; **HTTP route**, auth/RBAC/CSRF/Origin; ZIP/plugin permissions/sandbox; upload hoặc public render/CSP | G0–G2 phải có kết quả review/chốt trước G3; G4/G5 có test cả hành vi lỗi, quyền, bảo toàn dữ liệu |
| **Fast-Track** | UI Admin gọi các endpoint đã chốt, sửa hành vi không đổi API/DB/security boundary, tăng regression tests hoặc bảo trì tài liệu | G0–G3 có thể gộp trong một lượt; G4 và G5 vẫn bắt buộc với thay đổi code; không được miễn verify |

Một task mới thêm CRUD route **không** mặc nhiên thuộc Fast-Track: phải phân tích quyền Editor/Admin, CSRF, ID validation, public exposure, consistency và API contract. Nếu chưa chắc phạm vi ảnh hưởng, mặc định Full-Track. Gates là **bộ tiêu chí**, không phải sáu file hay sáu lần xin phép.

## Nguồn sự thật và kiểm tra nhất quán

| Loại thông tin | Nơi có thẩm quyền |
| --- | --- |
| Phạm vi sản phẩm đã xác nhận | [PRODUCT_REQUIREMENTS](../product/PRODUCT_REQUIREMENTS.md) |
| Milestone/slice, dependency và tiến độ điều phối | [ROADMAP_V1](ROADMAP_V1.md) |
| Domain/module ID và coverage V1 | [MODULE_CATALOG_V1](MODULE_CATALOG_V1.md) |
| Chi tiết capability, research, design, bảng task và primary slice khi mở module | `docs/plans/modules/<module>.md` |
| Contract và kiến trúc đã quyết định | [CORE_FOUNDATION](../contracts/CORE_FOUNDATION.md), các ADR liên quan |
| Test đã thực hiện theo commit/môi trường | [Verification](../verification/README.md) |

README chỉ làm mục lục. Không dựng thêm JSON/YAML manifest giữ bản sao trạng thái. Dùng `python cms/scripts/check-plans.py` (và `--self-test`) kiểm link Markdown, ID module, tham chiếu lạ/trùng và bằng chứng cho hàng ghi verified. Không hardcode tổng module = 27: tổng hiện tại được tính từ catalog; nếu PRD sau này thay đổi phải review và cập nhật module ID có lịch sử.

## Cách chia và giao task

- Mỗi slice nên tạo được một thay đổi chạy xuyên qua giao diện/API/storage khi có thể. Không chia thuần theo tên file nếu không có hành vi kiểm chứng được.
- Một task cần có **mục tiêu**, **module/capability**, **chế độ gate**, **đầu vào**, **file được sửa trong lượt**, **phần ngoài phạm vi**, **dependency**, **tiêu chí nghiệm thu**, **cách kiểm tra**, **rủi ro/rollback** và **bằng chứng bàn giao**. G1 và G2 là hoạt động trước G3, không gắn số `-Txx` như task code.
- Nếu hai task cùng sửa contract/schema/API chung, xác định thứ tự sở hữu trước khi giao song song; không cho hai agent ghi cùng file. **File ownership là theo lượt/task**, không khóa vĩnh viễn cả thư mục cho một AI.
- Phù hợp cách phối hợp hiện tại: Codex có thể chuẩn bị research/plan và review, Antigravity triển khai slice. Luồng phối hợp tuân theo [PROTOCOL](../discussions/PROTOCOL.md); danh sách nhiệm vụ ở đây không tự cấp quyền cho agent triển khai/deploy.
- Trước khi code, tra API và quan hệ caller/callee bằng CodeGraph nếu sẵn có; xác minh lại source thực. Giữ đúng các Base APIs của [CORE_FOUNDATION](../contracts/CORE_FOUNDATION.md).
- **Stop-and-Propose:** nếu chỉ là tinh chỉnh nội bộ không phá contract, báo `Design Delta` cùng test để Codex review. Nếu đụng PRD, migration/schema đã sử dụng, security, API/ABI hoặc quyết định G2 thì dừng ở ranh giới đó, báo hai phương án và chỉ làm tiếp sau khi chốt lại; không tự thay đổi contract.
- **An toàn migration:** test trên bản sao DB / site root riêng, nâng cấp từ fixture cũ; snapshot/backup nhất quán trước nâng cấp dữ liệu thật. Git revert không rollback DB đã migrate; migration đã phát hành không được chỉnh/xóa, thay bằng forward migration khi cần.

## Thay đổi kế hoạch và kiểm soát chất lượng

1. Thay đổi **phạm vi sản phẩm**: đối chiếu PRD và xin quyết định chủ dự án; cập nhật catalog và roadmap cùng lúc sau khi chốt.
2. Thay đổi **kiến trúc/contract**: ghi phương án và hậu quả, dùng ADR/contract theo quy ước; chỉ cập nhật task liên quan sau khi quyết định.
3. Thay đổi **thứ tự milestone**: giữ nguyên ID module; cập nhật dependency, lý do và gate, không sửa lịch sử bằng chứng.
4. Thay đổi **code**: chọn kiểm chứng theo tác động (preview, lưu, theme/plugin lifecycle, data preservation, backup). Không dùng kế hoạch thay cho test thực chạy.
5. Khi mở milestone, rà phần việc không thuộc V1: không tự đưa draft/publish riêng, revision history, repeater, relation, scheduled jobs, plugin UI tùy ý hoặc đa ngôn ngữ vào backlog V1.

**Nguyên tắc vận hành:** chỉ chuyển nghiên cứu thành thiết kế chốt khi có quyết định; chỉ chuyển implementation thành verified khi có bằng chứng; luôn chỉ ra phần còn thiếu. Đồng thuận AI về quy trình không đồng nghĩa chốt schema M3; `post/page` mặc định, plugin CPT khai báo trong manifest và opaque `media_id` hiện là **ứng viên G1**, chưa phải thiết kế `AGREED` ở G2.
