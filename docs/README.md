# OpenDash — Mục lục tài liệu

**Checkpoint ưu tiên 16/11/2026:** [ADR-0017 — Usable Beta staging](decisions/ADR-0017-usable-beta-staging.md)
và [G3 M7-S14](plans/milestones/M7/slices/2026-11-16-usable-beta-g3.md).
Dev demo, internal Beta và V1 public là ba phạm vi khác nhau; thiết kế không chứng minh
runtime/human usability. V1/native/security/license gates giữ nguyên.

Tài liệu này nằm tại `docs/` ở gốc workspace `/opendash-cms`, độc lập với mã nguồn trong `cms/`. Đọc [README gốc](../README.md) và [hướng dẫn phát triển CMS](../cms/README.md) để biết cách chạy OpenDash và phạm vi chưa triển khai.

## Nguồn quyết định và thứ tự ưu tiên

1. Chỉ đạo mới nhất của chủ dự án.
2. [Yêu cầu sản phẩm V1](product/PRODUCT_REQUIREMENTS.md) — phạm vi sản phẩm đã duyệt.
3. [Tech stack](architecture/TECH_STACK.md) — các lựa chọn kỹ thuật đã duyệt.
4. [Kiến trúc CMS](architecture/CMS_ARCHITECTURE.md), [ADR-0001](decisions/ADR-0001-workspace-layout.md), [ADR-0002](decisions/ADR-0002-core-extension-contract.md), [ADR-0003](decisions/ADR-0003-agent-consensus-governance.md), [ADR-0005](decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md) và [ADR-0010](decisions/ADR-0010-agent-project-and-merge-authority.md) — ranh giới module, core extension và quyền điều phối, triển khai, merge hiện hành.
5. [Bộ khung lõi & Quy chuẩn code](contracts/CORE_FOUNDATION.md) và [Plugin ABI v1](contracts/PLUGIN_ABI.md) — contract core kernel, quy chuẩn phân tầng và runtime/SDK.

## Tra cứu theo mục đích

Bổ sung ngày 19/11/2026: [Builder template và content](product/BUILDER_TEMPLATE_AND_CONTENT.md)
phân biệt template/thành phần dùng lại với content từng bản ghi có đăng ký Builder;
hierarchy tham chiếu WordPress, nguồn theme/database và home/archive tách biệt; bỏ yêu cầu chuyển homepage legacy thành Page. Yêu cầu đã xác nhận, chưa là API/schema G2, G3 hay runtime evidence.

Bổ sung sản phẩm đã xác nhận ngày 15/11/2026: [Admin Navigation và bố cục màn hình chỉnh sửa](product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md) — menu/submenu, vùng đăng ký panel và vị trí nhóm custom fields; [ADR-0016](decisions/ADR-0016-admin-navigation-and-single-edit.md) và [Contract Admin Navigation & Single Edit](contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) đã đạt `G2 AGREED` và merge tại `main@2808e3e` ([receipt #270](https://github.com/thientran2359/opendash-cms/pull/270#issuecomment-6903576673) · [#268](https://github.com/thientran2359/opendash-cms/issues/268)); đặc tả [G3 (`M7-S09-T01`–`M7-S13-T02`)](plans/milestones/M7/slices/2026-11-16-admin-navigation-and-single-edit-g3.md) đã soạn xong (`Implementation NOT_STARTED / Verification NOT_RUN`, chưa cấp `Project Ready`).

| Mục đích | Vị trí | Quy ước |
| --- | --- | --- |
| Phạm vi và hành vi sản phẩm | [`product/`](product/PRODUCT_REQUIREMENTS.md) | Đã xác nhận, thay đổi cần chỉ đạo của chủ dự án |
| Kiến trúc và lựa chọn stack | [`architecture/`](architecture/CMS_ARCHITECTURE.md) | Ghi rõ đề xuất và quyết định; [CMS_PROPOSAL](architecture/CMS_PROPOSAL.md) chỉ tham khảo có chọn lọc |
| Nghiên cứu, thảo luận chưa duyệt | [`.agents/research/`](../.agents/research/README.md) và [`.agents/discussions/`](../.agents/discussions/README.md) | Lưu bản nháp ngoài `docs/`; tài liệu lịch sử còn giữ để truy vết nguồn |
| Giao diện kỹ thuật giữa các phần | [`contracts/`](contracts/CORE_FOUNDATION.md) — [Core Foundation](contracts/CORE_FOUNDATION.md) · [Plugin ABI](contracts/PLUGIN_ABI.md) · [Admin Navigation & Single Edit (G2 AGREED)](contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) | Contract có version, trạng thái `AGREED` khác `verified` |
| Quyết định kiến trúc & điều phối | [`decisions/`](decisions/ADR-0001-workspace-layout.md) — [ADR-0001](decisions/ADR-0001-workspace-layout.md) · [ADR-0002](decisions/ADR-0002-core-extension-contract.md) · [ADR-0003](decisions/ADR-0003-agent-consensus-governance.md) · [ADR-0005](decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md) · [ADR-0008](decisions/ADR-0008-harbor-forge-autonomous-delivery.md) · [ADR-0010](decisions/ADR-0010-agent-project-and-merge-authority.md) · [ADR-0016](decisions/ADR-0016-admin-navigation-and-single-edit.md) | ADR có bối cảnh, quyết định, hệ quả, reviewer nếu cần và trạng thái |
| Roadmap và danh mục module | [Roadmap V1](plans/ROADMAP_V1.md) · [Module Catalog](plans/MODULE_CATALOG_V1.md) · [Quy trình](plans/PLANNING_WORKFLOW.md) | Hai trục chức năng và tiến độ, ID module, research/design gates, mẫu task |
| Kế hoạch triển khai | [`plans/`](plans/README.md) · [Milestones](plans/milestones/README.md) · [M3/Slices](plans/milestones/M3/README.md) | Milestone quy định điều kiện đóng mốc; slice là phần thực hiện, không phân thư mục theo trạng thái Ready/Done |
| Module/Capability xuyên milestone | [`plans/modules/`](plans/modules/README.md) · [Module Catalog](plans/MODULE_CATALOG_V1.md) | Nguồn mô tả chức năng, G0–G3 và dependency; module có thể thuộc nhiều milestone |
| Hồ sơ lịch sử | [`archive/`](archive/README.md) | Chỉ lưu hồ sơ lịch sử/retired, không tự archive một task chỉ vì đã triển khai code |
| Bằng chứng thực thi | [`verification/`](verification/README.md) | Chỉ ghi `verified` khi có lệnh, môi trường và kết quả thực tế |
| Điều phối agent | [Giao thức hiện hành](discussions/PROTOCOL.md) · [Quy trình GitHub](../.agents/GITHUB_WORKFLOW.md) · [Handoff cũ](discussions/AGENT_COORDINATION.md) | Handoff trước đây chỉ để tham khảo, không còn là cơ chế hoạt động |

Không tạo thư mục/module rỗng chỉ để khớp sơ đồ. Khi nguồn mã được đưa vào, đối chiếu cây thư mục và kiểm thử trước khi sửa tài liệu trạng thái.

Xem [báo cáo chuyển thư mục ngày 07/11/2026](verification/2026-11-07-layout-migration.md) cho thay đổi `cms/`, `database/` và các kiểm chứng sau di chuyển.


### Content foundation follow-up (18/11/2026)

[ADR-0019](decisions/ADR-0019-content-foundation.md) ·
[contract](contracts/CONTENT_FOUNDATION_2026_11.md) ·
[task index](plans/TASK_INDEX.md#content-foundation-follow-up--18112026).
CPT configuration, custom taxonomy, shared permalink resolver and provider integration;
design only until separately implemented/verified.

### Builder resolver follow-up (19/11/2026)

[ADR-0020](decisions/ADR-0020-builder-template-resolver.md) ·
[Resolver G2](contracts/BUILDER_TEMPLATE_RESOLVER.md): hierarchy trước, nguồn
database/theme sau; parts/content riêng; Settings chọn Page Trang chủ và Page
Blog làm archive post. Không convert homepage legacy thành Page content.
Final Decision của Atlas trong working tree, contract hiệu lực sau merge;
G3/API/schema tổng thể chưa hoàn chỉnh, runtime NOT_RUN.
