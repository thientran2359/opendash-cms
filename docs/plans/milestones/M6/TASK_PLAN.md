# M6 — Task planning: Plugin Extensions

`M6-Pxx` chỉ là **candidate của milestone**, không phải issue/code READY. [Plugin ABI baseline](../../../contracts/PLUGIN_ABI.md) · [M2 baseline](../M2/README.md) · [M3 provider bridge](../M3/README.md).

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance dự kiến |
| --- | --- | --- | --- | --- |
| M6-P01 | Hoàn thiện event/actions ContentBefore/AfterSave, server APIs | PLG-02 (CNT-04/FND-01) | M2.5 HookDispatcher, M3 transactions; Full-Track G2 ordering/failure/veto | Hook fired đúng thời điểm, sau commit không late writes; invalid permission, timeout và lỗi plugin không phá dữ liệu |
| M6-P02 | Khai báo và thực thi external service calls theo quyền | PLG-02 (PLG-01) | P01, G2 capability/network scope | CMS hiển thị quyền yêu cầu và tự áp dụng quyền hợp lệ, chặn ngoài quyền, quota/timeout/log; không có bước approval thủ công |
| M6-P03 | Mở rộng declarative registration bridge cho blocks/templates/settings | PLG-03 (BLD-01/02, THM-02, SITE-01) | M3 CPT/field bridge, M4/M5 domain registries; G2 manifest/versioning | Plugin ZIP register declaratively qua core owners, collision/invalid schema bị chặn; disable/reinstall không mất dữ liệu user |
| M6-P04 | Taxonomy declaration tích hợp với plugin lifecycle | PLG-03 (CNT-03) | M3 taxonomy đã có contract, P03 | Provider inactive không xóa taxonomy/entry relation; re-enable vẫn phục hồi lookup; no duplicate owner |
| M6-P05 | Declarative Admin UI từ schema: form/table/list/tabs/button | PLG-04 (FND-05, PLG-02) | P01/P03, G2 schema/action guards | Admin render UI theo khai báo, RBAC/CSRF, validation, access denied và tab switch không mất dữ liệu |
| M6-P06 | Sandbox, permission và failure isolation regression | PLG-01/02 | P01..P05, M2 worker baseline | Plugin lỗi/quá thời gian/dừng không ngăn CMS/Admin phục hồi; không cho provider disabled chạy, logs hữu ích không lộ secret |
| M6-P07 | E2E Plugin ZIP xuyên Content/Theme/Builder | PLG-03/04 (CNT-01–03, BLD-01, THM-02) | P01..P06 + G5 M3/M5 liên quan | Install→register CPT/fields/taxonomy/blocks/template/settings→edit/save→disable/restart/reinstall→data/layout còn; permission negative/rollback |

**Ngoài phạm vi:** plugin viết giao diện HTML tự do và scheduled jobs (ngoài PRD V1), backup engine (M8), UI chỉnh sửa theme theo code. Khi plugin bị tắt phải giữ dữ liệu do website sở hữu và quản trị được qua domain APIs.
