# ADR-0019 — Configurable content foundation

**Owner:** Aster · **Date:** 2026-11-18 · **Status:** ACCEPTED on merge.
Owner approved CPT → custom taxonomy → permalinks → plugin/theme integration,
with Harbor implementing and Aster deciding/self-reviewing contracts.

Adopt [content foundation contract](../contracts/CONTENT_FOUNDATION_2026_11.md).
Extend canonical registries rather than provider-owned content stores; migrate
category/tag restrictions without changing IDs; centralize route matching and URL
generation with atomic claims. Use bounded declarative rewrite formats and existing
plugin veto/actions. Theme remains declarative. No PHP/WordPress API parity claimed.

This supersedes M6_PLUGIN_EXTENSIONS D08 core-only taxonomy restriction only after
its implementation task is verified, and extends PRD draft semantics to opt-in CPTs.
Existing immediate CPT behavior remains default; no implicit publication migration.
CPT hierarchy, hierarchical Page URLs, date/category tokens and arbitrary filters
are deferred with explicit limits, rather than implied implemented parity.

G1 findings and G2 API/schema/ownership/error decisions are in the contract. G3:
[M3 settings](../plans/milestones/M3/slices/2026-11-18-content-foundation-g3.md),
[M4 taxonomy](../plans/milestones/M4/slices/2026-11-18-content-foundation-g3.md),
[M7 permalinks](../plans/milestones/M7/slices/2026-11-18-content-foundation-g3.md),
[M6 providers](../plans/milestones/M6/slices/2026-11-18-content-foundation-g3.md).
Design approval is not implementation, Project Ready, G5 or dispatch. Current
implementation NOT_STARTED, runtime NOT_RUN; #302 and Beta remain separate.
