# M5-S02-T01 / Issue #147 — Public RequestContext, taxonomy archives and canonical URLs

**Agent:** Forge (Windows). **Gate:** G4 implementation/test evidence. **Date:** 2026-11-13.

- [Issue #147](https://github.com/thientran2359/opendash-cms/issues/147), approved source/base SHA: 904960e3cb44c3fb90d42c116bc63f18865be0e8.
- Branch: task/forge-m5-s02-t01 in an isolated worktree.
- References: [M5 URL contract](../contracts/M5_THEME_TEMPLATES.md), [approved M5 G3](../plans/milestones/M5/slices/2026-11-13-m5-theme-templates-g3.md).

## Implemented

- Shared typed PublicRequestContext, CanonicalUrlBuilder, query parsing, ID-to-current-slug resolvers, canonical pagination and existing render/Builder consumers.
- Kept /, /content/{type_key}, /content/{type_key}/{slug}, including built-in page; added /taxonomy/category/{slug} and /taxonomy/tag/{slug} covering published entries across content types.
- Category is exact by default. Explicit descendants=true uses a bounded category tree. SQL AND EXISTS predicates filter published entries before paging; single SQLite read snapshot covers term identity, ancillary filters, COUNT and page.
- Invalid or draft identities, old slugs, wrong term kinds and encoded path ambiguity are rejected. Duplicate/invalid known query keys return 400; tracking query keys are dropped in canonical output. Invalid entry links are omitted instead of producing an empty link.
- Preserved independent M4 Builder list paging and existing public media ACL; no schema migration or plugin/theme import change. Role composition and menu CRUD remain S03/S09.

## Executed verification (Windows)

| Exact command or check | Result |
| --- | --- |
| npm --prefix cms/admin ci | PASS |
| npm --prefix cms/admin run build | PASS (tsc + Vite production) |
| cargo test --manifest-path cms/Cargo.toml --workspace --locked | PASS: 29 core + 67 server + 1 prior integration + 6 new integration = 103 |
| python cms/scripts/check-standards.py | PASS |
| python .agents/scripts/check-agent-artifacts.py | PASS |
| git diff --check | PASS |
| rustfmt --edition 2024 --config skip_children=true (issue-owned Rust files and test) | PASS |

New tests include HTTP requests to a temporary binary instance and SQLite fixtures for cross-CPT exact/descendant/tag filtering, publication/count/paging, duplicate memberships, 400/404 errors, encoded identities, current-slug URLs and Builder M4 query limits.

**G5 remains unverified:** required PR CI, merge and post-merge acceptance are separate gates. Project Done is not inferred from this G4 report.
