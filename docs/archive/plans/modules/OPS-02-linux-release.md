# OPS-02 — Linux operations và V1 release · Living Module Brief

**Scope:** PRD V1 · **Track:** Full-Track · **Date:** 2026-11-14.
**G1:** source reviewed · **G2:** AGREED on merge · **G3:** READY on merge · **Implementation:** NOT_STARTED · **Runtime verification:** NOT_RUN.
[PRD](../../../product/PRODUCT_REQUIREMENTS.md) · [ADR-0015](../../../decisions/ADR-0015-m8-operations-release.md) · [contract](../../../contracts/M8_OPERATIONS_RELEASE.md) · [G1](../../../../.agents/research/2026-11-14-m8-operations-release-g1.md) · [full G3](../milestones/M8/slices/2026-11-14-m8-operations-release-g3.md).

## G0 — Giá trị và ranh giới

Developer cài một executable CMS trên Linux; operator có quy trình upgrade/recovery lặp lại được và release dựa trên native evidence.
Own Linux runtime/distribution/native release proof; OPS-01 owns recovery format, all domains supply acceptance. Không auto deploy/update/public publish, không tự chọn giấy phép sản phẩm. Preserve content/layout/IDs, sessions/secret privacy and provider data across recovery; restore safety resets are explicit contract exceptions, not silent data loss.

## Capability và Primary Acceptance

| Capability | Hành vi | Primary Acceptance Slice | Task |
| --- | --- | --- | --- |
| OPS-02.1 | Runtime config/proxy/shutdown | M8-S06 | S06-T01 |
| OPS-02.2 | Linux artifact/install/upgrade | M8-S06 | S06-T02 |
| OPS-02.3 | Linux security/reliability/metrics | M8-S07 | S07-T01 |
| OPS-02.4 | Native Linux full acceptance | M8-S08 | S08-T03 |
| OPS-02.5 | Operator runbooks/rehearsal | M8-S09 | S09-T01 |
| OPS-02.6 | Evidence audit/release readiness | M8-S09 | S09-T02 |

## G1 — Actual source and alternatives

Baseline main@8c5b90072c231c1f0813eb593d7be4447fb0a11d. Inspected main/app/auth/paths, Cargo pins, embedded Admin, M7 Windows reports, current governance CI (does not execute product on Linux). Full inventory/alternatives/official upstream references are in G1 linked above. CodeGraph0 nodes, source/rg fallback. No M8 prototype/native Linux/performance claim.

## G2 — Selected contracts

ADR/contract define exact locks/writer coverage, paths, manifests/encryption/limits, schemas/endpoints, privacy/rollback, native and release requirements. Existing dependency pins preserved; new age0.11.2 belongs implementation with compilation/corpus gate. SQL only storage; reuse auth/id/package/registry/hook APIs. Shared writer barrier covers post-HTTP asynchronous completion. Contract changes require Atlas decision, not two competing implementations.

## G3 — Complete breakdown

All listed tasks have exact source files, ownership/dependencies/API/acceptance/negative tests/rollback in full G3. M7#213 Windows acceptance permits scoped development, owner claim and Project Ready still required; proposed owner is not active assignment. Native macOS/Linux and owner license decision remain explicit final release dependencies. No implementation Issues/dispatch created by preparing this brief.

## G5 — Verification plan and limits

Component tests on copied sites; native browser/permissions/locking/restart/restore plus platform-specific reports on a common final source. Required failure/NOT_RUN blocks its acceptance. Final V1 requires Linux measured performance, deferred Mac and existing milestone closure; no inference from governance CI. Preserve previous binary+complete snapshot for new-root rollback; never downgrade migrated DB in place. Record exact commands/SHA/OS/hash and honest gaps.
