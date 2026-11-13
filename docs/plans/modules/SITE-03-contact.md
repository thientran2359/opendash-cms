# SITE-03 — Contact forms, submissions và SMTP · Living Module Brief

**Date:** 2026-11-14 · **Scope:** PRD V1 · **Track:** Full-Track · **G2:** AGREED on merge · **G3:** READY on merge · **Implementation/G5 M7:** NOT_STARTED / NOT_RUN.
Sources: [PRD](../../product/PRODUCT_REQUIREMENTS.md), [ADR-0014](../../decisions/ADR-0014-m7-site-features.md), [M7 contract](../../contracts/M7_SITE_FEATURES.md), [G1](../../../.agents/research/2026-11-14-m7-site-features-g1.md), [exhaustive G3](../milestones/M7/slices/2026-11-14-m7-site-features-g3.md).

## G0 — Giá trị và ranh giới

Admin tạo form cơ bản, đặt qua Builder, xem tin nhắn và cấu hình SMTP; gửi mail lỗi không làm mất submission.
Core-owned form/submission/notification; BLD-02/03 trình bày core element. Secrets server-only. Không upload/repeater/cron/automatic retry/arbitrary HTML/public inbox. Một ngôn ngữ nội dung, preserve site-owned data/layout qua theme/plugin/restart; preview không public. M8 backup/restore/Linux release nằm ngoài scope.

## Capability và Primary Acceptance Slice

| Capability | Hành vi | Primary Acceptance Slice | Task / acceptance |
| --- | --- | --- | --- |
| SITE-03.1 | Schema/validation/durable submission/abuse guards | M7-S05 | M7-S05-T01 |
| SITE-03.2 | Designer/Builder/native form/Admin inbox | M7-S05 | M7-S05-T02 |
| SITE-03.3 | SMTP/secret protection/delivery attempts | M7-S06 | M7-S06-T01 |

## G1 — Source và phương án

Baseline main@4b60388; actual source inspected: core/builder.rs, page.rs, storage/overrides.rs, app.rs origin guard, paths.rs, Cargo manifests. Chi tiết hiện có/khoảng trống/nguồn upstream và lý do chọn core reuse thay vì plugin/parallel persistence nằm trong G1 linked ở trên. CodeGraph zero nodes, rg/source fallback. Không benchmark/prototype/runtime M7.

## G2 — Selected design

Contract/ADR linked là nguồn normative cho schema/API/CAS/limits/privacy/rollback. Không thêm schema ngoài contract; SQL storage-only, auth/id/registry/hook Base APIs. Thiết kế có hiệu lực sau merge, không tự kết luận implementation/verification từ consensus.

## G3 — Breakdown

- `M7-S05-T01`: xem toàn bộ scope/files/interfaces/dependency/acceptance/tests trong G3 nguồn.
- `M7-S05-T02`: xem toàn bộ scope/files/interfaces/dependency/acceptance/tests trong G3 nguồn.
- `M7-S06-T01`: xem toàn bộ scope/files/interfaces/dependency/acceptance/tests trong G3 nguồn.
Mọi task có coordinator Atlas, owner dự kiến trong G3; Issue/Project chưa phát hành, M5#157/M6#188 G5 và seam handoff là gate thực thi. READY documentation khác Project Ready.

## G5 — Bằng chứng và rollback

Chưa có M7 runtime evidence. Integration/negative/native browser ở Primary Acceptance Slice; cross-domain S08 chạy cùng immutable SHA Windows+macOS, snapshot restore matching old binary. Evidence ghi exact commands/hash/OS/PASS/FAIL/NOT_RUN; security/data hoặc required case NOT_RUN chặn G5. Không sửa applied migration hoặc dùng Git revert để phục hồi DB.

