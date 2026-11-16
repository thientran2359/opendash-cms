# SITE-01 — Site settings và navigation · Living Module Brief

**Date:** 2026-11-14 · **Scope:** PRD V1 · **Track:** Full-Track · **G2:** AGREED on merge · **G3:** READY on merge · **Implementation/G5 M7:** NOT_STARTED / NOT_RUN.
Sources: [PRD](../../product/PRODUCT_REQUIREMENTS.md), [ADR-0014](../../decisions/ADR-0014-m7-site-features.md), [M7 contract](../../contracts/M7_SITE_FEATURES.md), [G1](../../../.agents/research/2026-11-14-m7-site-features-g1.md), [exhaustive G3](../milestones/M7/slices/2026-11-14-m7-site-features-g3.md).

## G0 — Giá trị và ranh giới

Admin đổi cấu hình public và tự quản lý menu đã nhập từ theme; giữ định danh/nội dung khi chuyển provider.
Sở hữu settings/menu; THM-02 và PLG-03 là consumers. General SITE-01.1 M3 giữ nguyên. SMTP secrets thuộc SITE-03, không lẫn public/provider options. Một ngôn ngữ nội dung, preserve site-owned data/layout qua theme/plugin/restart; preview không public. M8 backup/restore/Linux release nằm ngoài scope.

## Capability và Primary Acceptance Slice

| Capability | Hành vi | Primary Acceptance Slice | Task / acceptance |
| --- | --- | --- | --- |
| SITE-01.1 | General Settings M3 (giữ acceptance lịch sử) | M3-S25 | Không giao lại việc đã có; evidence theo Issue#49 |
| SITE-01.2 | Public origin/language/indexability | M7-S01 | M7-S01-T01 |
| SITE-01.3 | Menu CRUD/preview/slot binding trên model M5 | M7-S02 | M7-S02-T01 |

## G1 — Source và phương án

Baseline main@4b60388; actual source inspected: storage/settings.rs, storage/menu.rs, migration0010/0019, GeneralSettingsPanel.tsx. Chi tiết hiện có/khoảng trống/nguồn upstream và lý do chọn core reuse thay vì plugin/parallel persistence nằm trong G1 linked ở trên. CodeGraph zero nodes, rg/source fallback. Không benchmark/prototype/runtime M7.

## G2 — Selected design

Contract/ADR linked là nguồn normative cho schema/API/CAS/limits/privacy/rollback. Không thêm schema ngoài contract; SQL storage-only, auth/id/registry/hook Base APIs. Thiết kế có hiệu lực sau merge, không tự kết luận implementation/verification từ consensus.

## G3 — Breakdown

- `M7-S01-T01`: xem toàn bộ scope/files/interfaces/dependency/acceptance/tests trong G3 nguồn.
- `M7-S02-T01`: xem toàn bộ scope/files/interfaces/dependency/acceptance/tests trong G3 nguồn.
Mọi task có coordinator Atlas, owner dự kiến trong G3; Issue/Project chưa phát hành, M5/M6 Windows development gate (ADR-0014§macOS deferral) và seam handoff là gate thực thi. READY documentation khác Project Ready.

## G5 — Bằng chứng và rollback

Chưa có M7 runtime evidence. Integration/negative/native browser ở Primary Acceptance Slice; cross-domain S08 chạy cùng immutable SHA Windows+macOS, snapshot restore matching old binary. Evidence ghi exact commands/hash/OS/PASS/FAIL/NOT_RUN; security/data hoặc required Windows case NOT_RUN chặn Windows acceptance; deferred macOS NOT_RUN vẫn chặn full multi-OS/V1 closure. Không sửa applied migration hoặc dùng Git revert để phục hồi DB.

**Owner-directed development track:** implementation dependencies may use scoped Windows acceptance after M5/M6 Windows entry gate; deferred macOS cases remain NOT_RUN and still required for S08/V1 closure. [ADR amendment](../../decisions/ADR-0014-m7-site-features.md#owner-directed-macos-deferral--2026-11-14).
