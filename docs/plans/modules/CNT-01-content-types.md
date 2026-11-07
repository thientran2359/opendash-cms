# CNT-01 — Content Types (CPT) · Living Module Brief

**Scope:** PRD V1 `AGREED` / ranh giới module đã thống nhất · **Milestone dự kiến:** M3 (`M3-S01`, `M3-S05`) · **PRD:** [Content & ownership](../../product/PRODUCT_REQUIREMENTS.md) · **Track:** `Full-Track` vì cần core contract, migration SQLite và API.

**Trạng thái hiện tại:** Research `NOT_STARTED` · Design `NOT_STARTED` · Implementation `NOT_STARTED` · Verification `NOT_RUN`. Các capability bên dưới là mục tiêu G0, **chưa có G2/Task READY**.

Đây là **Living Module Brief** chứa G0–G3 tại một nơi. Hiện mới ghi scope và câu hỏi G1; nghiên cứu theo source thực, rồi đưa kết luận G2 và bảng task G3 vào chính file này. Chỉ tách `docs/research/` cho đánh giá lớn/benchmark, không tự tạo file task rời. Kết luận [planning consensus](../../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) không tự chốt schema M3.

## G0 — Năng lực cần có

| ID | Capability | Primary acceptance slice dự kiến | Nghiệm thu dự kiến |
| --- | --- | --- | --- |
| `CNT-01.1` | Quản trị viên tạo và quản lý định nghĩa CPT qua Admin | `M3-S01` | Định nghĩa hợp lệ được lưu, khởi động lại vẫn xuất hiện; Editor không sửa schema khi thiếu quyền |
| `CNT-01.2` | Core, User, Theme, Plugin đăng ký định nghĩa theo ownership | `M3-S01` | Xử lý owner conflict, cập nhật cùng owner và phục hồi trạng thái qua restart |
| `CNT-01.3` | Tắt provider vẫn giữ schema và nội dung người dùng | `M3-S05` (plugin E2E) | Không xóa schema/entries khi plugin tắt/restart; theme switch E2E thuộc THM-03 ở M5 |
| `CNT-01.4` | Tích hợp với custom fields/entries | `M3-S02` | CNT-02/CNT-04 dùng chung API CPT, không xây đường lưu bản sao |

**Định hướng cần khảo sát, chưa chốt:** built-in `post`/`page` dưới `ResourceOwner::Core` và cách giữ tương thích bảng `home_page`/route `/` của M1. Seed ở migration hay bootstrap, khả năng sửa/xóa core types đều cần G2.

## G0 — Ranh giới và dependency

- **Cần trước:** FND-01 registry/owner và hook thực đã kiểm chứng trong M2.5 (`da79304`), FND-03 storage/migration, FND-02 quyền; research riêng persistence của CPT trong M3.
- **Cung cấp cho:** CNT-02 field schema, CNT-03 taxonomy, CNT-04 entries và public request context, BLD-05 binding, PLG-03 plugin registration bridge. Domain CNT-01 sở hữu validation/schema/SQLite, không chuyển persistence sang PLG-03.
- **Ngoài phạm vi CNT-01:** hiện thực toàn bộ custom field types, biểu mẫu soạn entry, UI builder nhiều tầng hoặc quản lý media. Không đổi migrations 0001–0003 hay plugin ABI v1 chỉ để mở CPT.

## G1 — Research cần thực hiện (chưa bắt đầu)

1. **Hiện trạng code:** tra `cms/crates/opendash-core/src/{content,registry}.rs`, `cms/crates/opendash-server/src/{storage,hooks,plugin}` và các caller bằng CodeGraph/source. API công khai và các nhánh dynamic registration hiện có ra sao?
2. **Ownership:** registry có phân biệt definition thuộc provider với dữ liệu nội dung thuộc website chưa? Có thể phát hiện conflict, deactivate/reactivate và re-register an toàn không?
3. **Persistence:** biểu diễn CPT schema trong SQLite hay registry runtime, kết hợp thế nào để load sau restart và nâng cấp từ DB cũ? Migration mới cần điều kiện gì?
4. **Versioning:** theme/plugin cập nhật định nghĩa CPT/field thì áp dụng thay đổi schema ra sao, nhất là đổi tên, xóa, đổi kiểu? Ai được thực hiện tác vụ xóa dữ liệu?
5. **API và quyền:** Admin tạo/sửa CPT theo API nào, Editor được xem/sử dụng phần nào, validation key có trùng ID của provider không?
6. **Tích hợp hooks:** đăng ký và lưu schema trước/sau commit thế nào để không có event báo thành công khi transaction thất bại?
7. **An toàn và tương thích:** các URL/slug, reserved key, rollback và thứ tự load provider được kiểm thử thế nào?
8. **M3 vs M5/M6:** cách nào chứng minh lifecycle `ResourceOwner::Theme` trên catalog mà không giả vờ đã có theme switch E2E? Plugin manifest khai báo `content_types` tùy chọn và bridge PLG-03 có tương thích package cũ, install/disable/restart không?
9. **Route namespace:** phân biệt kiểm tra cú pháp `SnakeKey` với URL prefix bảo lưu (`/api`, `/admin`, `/assets`); quyết định ở tầng router, tránh cấm nhầm CPT hợp lệ.

**Đầu ra G1:** bảng API/callers theo source + CodeGraph, so sánh ít nhất hai phương án owner/schema/init khi có tradeoff, test tối thiểu cho điểm chưa chắc chắn, lựa chọn đề xuất và rủi ro. Ghi tóm tắt tại đây, chỉ tách `docs/research/` nếu cần.

## G2 — Quyết định cần chốt (hiện `NOT_STARTED`)

- ID CPT, owner và override policy, cách biểu diễn trạng thái provider inactive; bảo toàn owner/status qua restart. `RegistryCatalog::register_with_status` chỉ là một ứng viên; không tự kích hoạt provider đang tắt.
- Schema SQLite, migration versioning và cơ chế khôi phục dữ liệu/definition khi provider vắng mặt.
- Khảo sát cách seed `post`/`page` idempotent và bảo toàn `home_page` M1; phân biệt reserved URL namespace với validator `SnakeKey`.
- HTTP/use case/core/storage interfaces, transaction và event order; quản lý quyền/CSRF và public visibility. Entry public route nếu có cần contract riêng ở CNT-04.
- Cầu nối plugin manifest tối thiểu nếu dùng ở M3: schema/compatibility, install/disable/restart, rollback an toàn; không sửa Wasm ABI v1 để né gate.
- Cách CNT-02/CNT-03/CNT-04 truy cập CPT mà không xây API khác.
- Test matrix (owner conflict, reactivation, restart, migration từ DB copy, rollback thực tế, giữ content, lỗi handler). **Git revert không rollback migration đã chạy**; migration phát hành cần forward-fix.

Nếu một quyết định thay đổi contract chung, tạo ADR hoặc cập nhật contract trong `docs/contracts/` trước khi sửa code.

## G3 — Breakdown task dự kiến (chưa sẵn sàng giao code)

| Task ID giữ từ bản nháp | Đầu ra khi qua G3 | Phụ thuộc và điều kiện |
| --- | --- | --- |
| `M3-S01-T03` | Core CPT/registry và storage theo contract G2 | Full-Track G2 đã duyệt; M2.5 verified |
| `M3-S01-T04` | HTTP Admin + form quản lý CPT | T03, FND-05; RBAC/CSRF |
| `M3-S01-T05` | Plugin adapter/lifecycle tối thiểu, chỉ theo manifest đã chốt | T03/T04, PLG-03; theme switch E2E vẫn thuộc M5 |
| `M3-S01-T06` | Tests HTTP/DB/restart/provider, báo cáo verification | T03–T05, negative tests trên DB copy |

**Ánh xạ lịch sử ID:** `M3-S01-T01` và `M3-S01-T02` trong discovery draft cũ từng chỉ G1/G2; nay chuyển về các mục G1/G2 ở trên và **giữ hai số ID đó ở trạng thái retired, không tái cấp phát**. `T03..T06` là *task candidates*, chưa `READY`; bảng này chỉ được chốt ở G3.

Khi qua G2, chuyển từng task sang [mẫu giao việc](../templates/SLICE_TASK.md) trong `.agents/sync/task-for-ag.md`, ghi file ownership theo lượt, test và rollback. Không tạo hàng loạt file task mới; chưa được phép triển khai M3 từ nội dung brief này.
