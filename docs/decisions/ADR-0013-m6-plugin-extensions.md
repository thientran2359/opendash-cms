# ADR-0013 — M6 Plugin Extensions: G2 Decision Record

**Agent:** Nova (Antigravity Windows, tiếp nhận hoàn tất thay Atlas theo chỉ đạo chủ dự án) · **Date:** 2026-11-13.
**Decision status:** **G2 AGREED** (all blockers `D01–D10` resolved in [M6 Plugin Extensions Contract](../contracts/M6_PLUGIN_EXTENSIONS.md)); **G3 READY** upon merge of [M6 G3 specification](../plans/milestones/M6/slices/2026-11-13-m6-plugin-extensions-g3.md).
**Authority:** [ADR-0010](ADR-0010-agent-project-and-merge-authority.md), [PRD](../product/PRODUCT_REQUIREMENTS.md), [Planning Workflow](../plans/PLANNING_WORKFLOW.md).

## Sources and baseline

- **G1/G2 Research:** [PR #177](https://github.com/thientran2359/opendash-cms/pull/177) (`codex/atlas-m6-g1-g2-research@70bb47787188bf3ce01f6714d780f6f0ab202450`), `.agents/research/2026-11-13-plg-m6-g1-g2.md`, authored by Atlas.
- **Independent Security/Boundary Review:** [Nova review on PR #177](https://github.com/thientran2359/opendash-cms/pull/177#issuecomment-6857065823) (reviewed head `70bb47787188bf3ce01f6714d780f6f0ab202450`).
- **Verified `main` baseline:** `176b46922b6ce961a874d68a804ee74390908bca` (highest migration on `main`: `0021_theme_import_journal.sql`).
- **Supporting existing contracts:** [ADR-0012](ADR-0012-m5-theme-templates.md), [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md), [PLUGIN_ABI.md](../contracts/PLUGIN_ABI.md), [M5_THEME_TEMPLATES.md](../contracts/M5_THEME_TEMPLATES.md).

## Decision matrix (`D01–D10`)

| ID | Decision selected | Key boundary / Nova review resolution |
| --- | --- | --- |
| `D01` | Keep Wasmi transport ABI v1 (`opendash_plugin_v1`), add versioned extension manifest (`manifest_schema: 2`) with strict minimum host & capability allowlist | Unknown capabilities or unsupported `manifest_schema` / `min_host_version` fail closed at install/enable; no partial execution. |
| `D02` | Immutable plugin revisions under `<site_root>/plugins/revisions/<revision_digest>/` (`A-DURABILITY` staging), monotonic `revision_generation` + CAS pointer, atomic auto-enable on valid install | Disable/update bumps/invalidates `revision_generation` in SQLite **before** cancelling worker permits; in-flight IPC commits with stale `revision_generation` fail closed. |
| `D03` | Owner-neutral CNT adapter (`ResourceOwner::Theme \| Plugin \| Site`), preserving M5 theme-switch isolation and existing `post`/`page` built-in protections | Theme switch/withdrawal never mutates `Plugin(id)` declarations; plugin withdrawal/disable never deletes entries, field values, or Editor CRUD. |
| `D04` | `ContentBeforeSave` runs as pure bounded validator/veto **before** SQLite write tx (`5s` total phase cap); `ContentAfterSave` runs post-commit read/diagnostic only (`5s` total phase cap) | **Nova #1:** Write tx re-verifies slug uniqueness, reserved roots, media visibility, and term validity/eligibility after `allow`. Veto diagnostics return raw length-bounded UTF-8 plain text (no server HTML-entity escaping). |
| `D05` | Explicit typed standalone `host_call` actions; reject mixed KV + content writes in V1; Save hooks cover create/update only (not Publish/Unpublish/Delete) | **Nova #3:** Enforce actor RBAC ∩ plugin capabilities + `enabled` + `revision_generation` both at admission and inside SQLite write tx before commit. |
| `D06` | V1 origin compatibility + v2 `(scheme, host, port, methods, path_prefix)` scope, HTTPS default, no redirects, `10` HTTP req/invocation sub-quota, `1 MiB` request/response cap | **Nova #2:** Custom `reqwest::dns::Resolve` rejects entire answer set if *any* IP is non-public (incl. IPv4-mapped IPv6, NAT64, ULA, link-local, CGNAT, metadata), `.no_proxy()` disables env proxy bypass, and `1 MiB` response cap is enforced incrementally per streamed chunk. |
| `D07` | Domain owner-neutral blocks, explicit plugin template selection, and plugin settings stored in canonical domain/SITE-01 stores with `ResourceOwner` | No second catch-all plugin registry; disabled/missing plugins keep user AST/settings intact with safe core fallback. |
| `D08` | Core `category` and `tag` kinds only; plugin claims eligibility only for its own canonical CPTs; retained valid entry-term links remain editable when provider inactive | No custom taxonomy kinds in M6; legacy assignments are preserved. |
| `D09` | Declarative Admin UI Schema v1 (`form`, `table`, `list`, `tabs`, `button`), max depth 8 / 200 nodes/screen / 20 screens/plugin / page <=100, host-held drafts keyed by `(plugin, screen, form, object_id, schema_version)` | No arbitrary HTML/JS/CSS/eval; server validates all mutations with session + RBAC + CSRF + Origin; 401 re-auth never auto-submits. |
| `D10` | Integrated M6 E2E on both native Windows and native macOS from one common source SHA, plus snapshot/matching-binary restore | M5 `#157` macOS G5 remains independent from M6; M6 G5 requires native evidence on both OSes before closing Milestone #7. |

## Migration allocation

- Highest existing migration on `main@176b469`: `0021_theme_import_journal.sql`.
- Sequential M6 migrations allocated (verified at claim time against `origin/main`):
  - `0022_plugin_revisions.sql` (`M6-S01-T01`)
  - `0023_owner_neutral_cnt_bridge.sql` (`M6-S02-T01`)
  - `0024_owner_neutral_domain_bridge.sql` (`M6-S05-T01`)
