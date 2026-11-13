# Working research

Document open questions, candidate designs, source revision, evidence, testing,
tradeoffs and unresolved risks. Status PROPOSED is not implementation approval.
Use ../templates/RESEARCH.md; promote only accepted conclusions into docs/.

## Open proposals (not implementation-ready)

- [M3-P07/P08/P16 — G2 publication boundary + G3 work packages](m3-post-page-integrated-g2-g3-proposal.md) — `main@658fc26`, 5 staged work packages; ngăn lộ draft/media, giữ legacy/Builder, migration collision + rollback; **G2 PROPOSED tại thời điểm viết, được owner phê duyệt trực tiếp 10/11/2026 trong [ADR-0004 chính thức](../../docs/decisions/ADR-0004-m3-post-page-publication-boundary.md)**; [G3 mới vẫn DRAFT](../../docs/plans/milestones/M3/slices/2026-11-10-post-page-publication-g3.md), không Ready. Giữ nguyên nội dung proposal lịch sử.
- [M3-P07 — built-in Post/Page G1 source audit + G2 proposal](m3-p07-built-in-post-page-g1-g2.md) — `main@dfdcae2`, Core CPT seed, migration/owner conflict, draft-public/media security dependencies; **G2 PROPOSED, peer-review pending**.
- [Muse Orchestrator POC — CoS Windows](muse-cos-win-orchestrator-poc.md) — kiến trúc remote dispatch, ranh giới quyền, phân giai đoạn và các quyết định chưa chốt (**PROPOSED**).
- [Muse → CoS Windows POC test matrix](muse-cos-win-poc-test-matrix.md) — điều kiện PASS/FAIL/BLOCKED/NOT_RUN cho từng gate (**chưa chạy test runtime**).

## M7 research promoted by design PR (2026-11-14)

- [M7 G1 source inventory](2026-11-14-m7-site-features-g1.md): all8 candidates, actual source/alternatives; authority after merge is ADR-0014 and M7 contract/G3. Runtime NOT_RUN.
