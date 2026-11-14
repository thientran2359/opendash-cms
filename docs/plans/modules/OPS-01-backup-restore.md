# OPS-01 — Manual backup, restore và recovery · Living Module Brief

**Scope:** PRD V1 · **Track:** Full-Track · **Date:** 2026-11-14.
**G1:** source reviewed · **G2:** AGREED on merge · **G3:** READY on merge · **Implementation:** NOT_STARTED · **Runtime verification:** NOT_RUN.
[PRD](../../product/PRODUCT_REQUIREMENTS.md) · [ADR-0015](../../decisions/ADR-0015-m8-operations-release.md) · [contract](../../contracts/M8_OPERATIONS_RELEASE.md) · [G1](../../../.agents/research/2026-11-14-m8-operations-release-g1.md) · [full G3](../milestones/M8/slices/2026-11-14-m8-operations-release-g3.md).

## G0 — Giá trị và ranh giới

Admin tạo backup nhất quán, giữ kín thông tin SMTP và khôi phục một site riêng có thể đăng nhập/chỉnh sửa; operator kích hoạt và rollback bằng quy trình đã kiểm chứng.
Own snapshot/archive/job/recovery policy; CNT-05 owns media reachability; FND-03 paths and SQLite; THM-01/PLG-01 immutable packages. Không cloud/scheduler/hot-overwrite current site. Preserve content/layout/IDs, sessions/secret privacy and provider data across recovery; restore safety resets are explicit contract exceptions, not silent data loss.

## Capability và Primary Acceptance

| Capability | Hành vi | Primary Acceptance Slice | Task |
| --- | --- | --- | --- |
| OPS-01.1 | Instance leases/writer barrier | M8-S01 | S01-T01/T02 |
| OPS-01.2 | Consistent snapshot/manifest | M8-S03 | S03-T01 |
| OPS-01.3 | Bounded encrypted codec | M8-S04 | S04-T01 |
| OPS-01.4 | Admin backup/download/cancel | M8-S04 | S04-T02 |
| OPS-01.5 | Verified separate-root restore | M8-S05 | S05-T01 |
| OPS-01.6 | Admin recovery/operator activation | M8-S05 | S05-T02 |
| OPS-01.7 | Full V1 recovery integration | M8-S08 | S08-T01/T02/T04 |

## G1 — Actual source and alternatives

Baseline main@8c5b90072c231c1f0813eb593d7be4447fb0a11d. Inspected SitePaths, storage::init_db/import/theme/plugin/media/smtp, SecretStore, safe ZIP package helpers. Full inventory/alternatives/official upstream references are in G1 linked above. CodeGraph0 nodes, source/rg fallback. No M8 prototype/native Linux/performance claim.

## G2 — Selected contracts

ADR/contract define exact locks/writer coverage, paths, manifests/encryption/limits, schemas/endpoints, privacy/rollback, native and release requirements. Existing dependency pins preserved; new age0.11.2 belongs implementation with compilation/corpus gate. SQL only storage; reuse auth/id/package/registry/hook APIs. Shared writer barrier covers post-HTTP asynchronous completion. Contract changes require Atlas decision, not two competing implementations.

## G3 — Complete breakdown

All listed tasks have exact source files, ownership/dependencies/API/acceptance/negative tests/rollback in full G3. M7#213 Windows acceptance permits scoped development, owner claim and Project Ready still required; proposed owner is not active assignment. Native macOS/Linux and owner license decision remain explicit final release dependencies. No implementation Issues/dispatch created by preparing this brief.

## G5 — Verification plan and limits

Component tests on copied sites; native browser/permissions/locking/restart/restore plus platform-specific reports on a common final source. Required failure/NOT_RUN blocks its acceptance. Final V1 requires Linux measured performance, deferred Mac and existing milestone closure; no inference from governance CI. Preserve previous binary+complete snapshot for new-root rollback; never downgrade migrated DB in place. Record exact commands/SHA/OS/hash and honest gaps.
