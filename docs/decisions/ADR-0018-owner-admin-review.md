# ADR-0018 — Owner Admin review notes01–07

**Owner:** Aster · **Date:** 2026-11-17 · **Status:** ACCEPTED, effective after merge.
**Authorization:** Owner completed seven-note review and instructed Aster to prepare
contracts and Harbor tasks. Aster has final technical/self-review authority under
ADR-0010; no peer discussion requested. Product implementation and runtime NOT_RUN.

## Decision

Adopt [Admin review contract](../contracts/ADMIN_REVIEW_2026_11.md): grouped navigation,
one feature per route, single-editor taxonomy/publication repair, separate term editors,
server-filtered list tables with declarative provider extension points, entry-scoped
Page Builder, private Section CPT and local pattern library. React/Tailwind, Rust/SQLite,
canonical APIs, permission model and existing AST remain. Builder redesign and cloud
library are deferred. This delta supersedes only conflicting ADR-0016 UI/defaults;
M4/M5 AST, ADR-0015 recovery and all security/data/publication guarantees remain.

A cosmetic-only patch would leave incorrect route/body behavior; arbitrary JS/HTML
extensions would break the declarative plugin/UI boundary. Choose bounded typed
queries and provider declarations, reuse scoped overrides rather than new Page storage.
Section insert copies AST rather than synced references (outside current scope).

## Source and consequences

Source inspected: `fc965b45a8ad734e2b01f41622d86db07946163c`, CodeGraph followed by actual
source inspection; owner screenshots localhost18964. Not a fresh runtime reproduction.
Ten serial Harbor tasks under M7-S16, M6-S10, M4-S11, M5-S12. Specs cover all seven
notes, including Section/library rather than placeholder navigation. New migration
numbers reserved serially; exact migration/schema policy in contract. Old manifests
and entry list clients remain compatible. Provider surface data remains site-owned
across provider withdrawal. No arbitrary frontend code or unauthenticated draft/media.

Only publish implementation Issues from merged immutable specs, record source/base,
check dependencies and live Project Ready before pickup. Aster owns docs/index, Harbor
owns code only after claim. This decision does not authorize automatic dispatch,
Project transition, parent Issue closure, Beta PASS or deployment.

## Implementation specifications

- `M7 navigation/editor/taxonomy/table` (Git history)
- `M6 declarative contributions` (Git history)
- `M4 Page and Section Builder` (Git history)
- `M5 local library` (Git history)

## Validation

Docs checks and self-review are recorded on the PR at its exact head. Actual build,
Chrome, ACL/migration/rollback and human tests belong to implementation receipts.
Keep #302 Open and Project unchanged; owner review notes are requirements, not a
completed human Beta acceptance receipt.
