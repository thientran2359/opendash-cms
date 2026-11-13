# ADR-0014 — M7 site domains and bounded public delivery

**Decision owner:** Atlas · **Date:** 2026-11-14 · **State:** ACCEPTED / G2 AGREED, effective on merge of this decision, contract and G3 to main under ADR-0010. Design authority is distinct from implementation/G5.

## Context

Owner requested full M7 G1→G2→G3 while Nova completes M6. Baseline `main@4b60388683a01df8d6332c88449b4fa6821ef5fc` includes M6 Windows E2E, not complete multi-OS/browser closure. [PRD](../product/PRODUCT_REQUIREMENTS.md), [candidates](../plans/milestones/M7/TASK_PLAN.md) and [source inventory](../../.agents/research/2026-11-14-m7-site-features-g1.md) define scope and actual reuse. CodeGraph was unindexed; source read directly.

## Final Decision

| ID | Selected design | Reason |
| --- | --- | --- |
| D01 | Separate versioned site-public settings, preserve General API and provider owners | No duplicate settings engine or Host-derived origin. |
| D02 | M5 menu storage/renderer plus Admin CRUD/preview and CAS slot aggregate | Preserve imported IDs/provenance and theme assignments. |
| D03 | Site-owned entry path claims, automatic old-path301 to live current entry ID | Explicit M7 extension of M5 no-alias rule; one canonical resolver for every consumer. |
| D04 | Typed core SEO projection and single trusted head; preview always noindex | No raw head HTML/scripts or draft OG/media leakage. |
| D05 | Request-time snapshot sitemap, deterministic5000-URL partitions | Avoid cache invalidation races/scheduler; bound output. |
| D06 | Site-owned basic forms, native HTML core/contact_form, durable submissions and strict Origin/abuse limits | Forms usable through Builder/Admin; no uploads/arbitrary HTML/public inbox. |
| D07 | Async Lettre0.11.23 mandatory TLS, OS-protected private secrets and durable bounded send attempts | No automatic retries; ambiguous SMTP delivery stays unknown. Secrets are ACL protected, not claimed encrypted. |
| D08 | Bounded MP4 AVC/AAC metadata subset, streaming Range, existing media ACL and core/video | No external transcoder, generic parser, new CPT field kind or video in image fields. |
| D09 | Seven forward migrations, canonical storage, serial shared-file ownership and snapshot restore | Existing migrations immutable; data survives theme/plugin lifecycle. |
| D10 | All8 candidates mapped to14 G3 tasks including native G5/closure | G3 docs do not imply Project Ready; Windows development gate below replaces full multi-OS predecessor closure for implementation pickup only. |

## Alternatives and consequences

Core slices reuse canonical domain primitives. Plugin implementation would couple mandatory V1 features to disable lifecycle; a second menu/settings store would fork data. External ffmpeg/sendmail breaks distribution requirements. A generic MP4 parser input cap does not prove allocation bounds. Automatic email retry adds scheduled jobs outside V1; unrecorded sends lose crash observability.

Selected design adds explicit route uniqueness transactions, narrow core form/video variants and secret-file lifecycle. Unsupported media is rejected with diagnostics. New dependencies must be pinned and compiled on real platforms; existing dependency upgrades are not authorized.

## Authority and handoff

### Owner-directed macOS deferral — 2026-11-14

Owner cannot currently run macOS and explicitly approved deferring that verification to continue M7 development on Windows. This overrides the original D10 requirement to close full M5/M6 G5 before any M7 implementation. It changes sequencing and evidence scope, not security/data acceptance or historical test results.

**Windows development gate:** Atlas may release M7 implementation tasks once M5/M6 Windows acceptance is complete on a coordinator-pinned source: locked builds/tests, real-browser flows for both milestones, M5 complete pre-M5 snapshot restore with matching M4 binary `a176414`, and M6 lifecycle/recovery/rollback evidence. Required CI and all known security/data failures must be resolved. Record exact source/binary hashes, commands/results and remaining macOS NOT_RUN in a merged supplemental verification report linked from #157/#188. Existing reports still lack browser tests and M5 binary rollback; this decision does not declare that gate passed.

M5#157/M6#188 may stay open for macOS while development proceeds; do not claim multi-OS VERIFIED or close their milestones because Windows passes. M7 implementation-slice acceptance may be recorded as VERIFIED on Windows, with macOS explicitly NOT_RUN; that scoped acceptance can satisfy the next M7 implementation dependency. Coordinator verifies ownership, immutable G3, Issue/Project Ready and actual Windows evidence before pickup. Proposed Harbor owners are not active assignments; another authorized Windows agent may claim an unowned task through the normal workflow, never take an existing owner's task silently.

**Deferred release gate:** retain M7-S08-T03 native macOS and M7-S08-T04 multi-OS closure. Mac tests, including secret permissions/TLS/video/browser/upgrade/rollback, must run before V1 release; macOS-specific failures block release and require fixes plus affected Windows regression. Final multi-OS acceptance must use a common immutable source for each milestone; if original Windows source is superseded, run Windows acceptance again on the selected final source. M8 Linux/release requirements remain. No rule requiring fresh native evidence is satisfied by CI or Windows alone.

Normative entry checklist is [contract §9](../contracts/M7_SITE_FEATURES.md#9-release-and-operations-boundary). This amendment authorizes no test-result fabrication, Project transition without evidence, dispatch or product implementation in the documentation change itself.

Normative [M7 contract](../contracts/M7_SITE_FEATURES.md), exhaustive [G3](../plans/milestones/M7/slices/2026-11-14-m7-site-features-g3.md). Atlas coordinates/chốt technical decisions under [ADR-0010](ADR-0010-agent-project-and-merge-authority.md). Proposed implementation owners become active only through Issue/Project claim. Nova retains M6 ownership. No product implementation, dispatch, predecessor closure or deployment in this docs delivery.

**Evidence:** source/standards inspection; exact validation, review head and CI recorded on PR. M7 build/migration/native/browser tests NOT_RUN. Security/data findings must be resolved before merge; PRD expansion uses Stop-and-Propose.
