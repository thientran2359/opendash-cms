# Immutable Nova / #413 documentation comparison

Date: 2026-11-20. Source scope: immutable Nova `e1a239126829a64c40df1ed86605dadb9ca2320c` compared with #413 `d897c66cddf39a46d901f24335c25b4823afcb73`. The original findings below concern those source snapshots, not the final integrated conflict resolutions. This report does not assert that these findings remain unresolved in the integration worktree.

Comparison is **not clean for wholesale Nova adoption**. Retain Nova’s EGM structure, amend the conflicts below, and preserve #413’s settled Builder decisions and research provenance. #413’s migration draft explicitly remains incomplete; it is not blanket implementation approval.

1. **Blocker — Builder requirements/provenance absent from Nova.** Comparing immutable `d897c66c` with `e1a2391` shows these four files exist only on #413:

   - `docs/product/BUILDER_TEMPLATE_AND_CONTENT.md`
   - `docs/contracts/BUILDER_TEMPLATE_RESOLVER.md`
   - `docs/decisions/ADR-0020-builder-template-resolver.md`
   - `docs/designs/BUILDER_THEME_CONTENT.md`

   Nova also removes their follow-up provisions from `docs/product/PRODUCT_REQUIREMENTS.md`, `docs/contracts/M5_THEME_TEMPLATES.md`, `docs/contracts/ADMIN_REVIEW_2026_11.md`, and `docs/contracts/CONTENT_FOUNDATION_2026_11.md`. This loses settled root-template/header/footer structure, `template-part/`, `page-layouts/`, `sections/`, `assets/`, candidate-first DB/theme resolution, explicit Page settings, independent Builder capability, and the no-experimental-backward-compatibility directive. Minimal disposition: preserve all four #413 files and those four source documents’ settled follow-ups. Add the Builder spec to Nova’s `docs/designs/SITEMAP.md` as **Proposed**, retaining source provenance and unresolved implementation questions. Reconcile `docs/README.md`; do not resolve it with Nova alone.

2. **Blocker — publication contradiction.** Nova’s PRD §2 says Visual Builder, explicitly including “exact Page Builder, Section Builder,” saves directly to public state. #413’s PRD and Builder source/spec require record content Builder to follow the record’s publication workflow; a draft Page, including a selected homepage Page, stays private after Save. Minimal disposition: amend `docs/product/PRODUCT_REQUIREMENTS.md`, `AGENTS.md` publication summary, and `.agents/skills/opendash-elephant/SKILL.md` to distinguish shared template overrides from record content. Retain #413’s explicit prohibition on Builder implicitly publishing a record.

3. **Major — owner technical approval contradicts recorded authority adaptation.** Nova `AGENTS.md` lines 5, 13, 23 and 25 impose owner approval for technical drafting/resolution/code admission. `docs/README.md` and `docs/designs/README.md` repeat this. #413 `design_draft.md`, Technical Plan §1, records owner-approved OpenDash adaptation v1: coordinators decide technical sections/findings inside approved product scope; owner retains product scope/priorities/release. Minimal disposition: amend those three entry documents consistently and retain the adaptation provenance. Do not modify upstream plugin/persona instructions. Reconcile `.agents/GITHUB_WORKFLOW.md`, `.agents/AGENT_OPERATING_STANDARD.md`, and the three active coordination skills to that same policy.

4. **Major — archive retirement strands accepted legacy work.** Nova `docs/archive/README.md` says archived documents must not coordinate current work; pickup/dispatch skills require an Approved EGM source. Yet `.agents/GITHUB_WORKFLOW.md` still specifies G3/module task admission and updating archived `TASK_INDEX`. #413 explicitly retains active legacy tasks under exact pinned contracts/identities and requires rechecking Ready for previously Ready work. Minimal disposition: preserve Nova archive organization only with an explicit bounded legacy-admission exception in:

   - `AGENTS.md`
   - `docs/archive/README.md`
   - `.agents/GITHUB_WORKFLOW.md`
   - `.agents/skills/github-task-pickup/SKILL.md`
   - `.agents/skills/atlas-task-dispatch/SKILL.md`
   - `.agents/skills/github-pr-review-merge/SKILL.md`

   Historical sources/identities must remain valid without relabelling tasks as new EGM work. No live task/receipt inventory was inspected here, so this is a policy incompatibility, not a claim about a particular active Issue.

5. **Major — active skill references remain contradictory.** Nova moves `discussion-turn-responder` to `.agents/archive/skills/`, while `.agents/AGENT_OPERATING_STANDARD.md` still directs agents to follow that archived skill. `AGENTS.md` line 81 prohibits invoking archived skills. Minimal disposition: decide explicitly whether discussion response remains supported, then align `.agents/AGENT_OPERATING_STANDARD.md`, `.agents/README.md`, `.agents/skills/README.md`, and the skill’s actual location. A path rewrite alone does not establish retirement or replacement.

6. **Major — catalog overclaims implementation.** Nova `docs/designs/SITEMAP.md` labels the complete V1 PRD “Implemented (Baseline)” and calls listed documents implemented contracts. #413 expressly rejects using a requirements catalog to certify V1 completion; its Builder requirements are explicitly unimplemented. Minimal disposition: retain the sitemap but scope implementation claims to actual bounded features/evidence. The Builder spec must remain Proposed; its registered filename cannot grant Approved/Implemented status.

7. **Major — authoritative contract compression loses substantive detail.** Nova’s `docs/contracts/PLUGIN_ABI.md` replaces full host-import semantics/error responses and detailed worker cancellation/commit-handover guarantees with short summaries. For example, #413 requires initial `busy_timeout` restoration, dirty-connection eviction, cleanup before semaphore release, and success after successful COMMIT rather than reporting Timeout. Nova’s “commit … khi chưa quá hạn” summary does not preserve that handover distinction. `CORE_FOUNDATION.md` similarly removes explicit registry conflict/update-status behavior and error/auth exceptions. Minimal disposition: preserve these normative subsections from #413 while adding Nova’s newer API inventory. Do not accept these compressed rewrites solely because links/checks pass.

**Read inventory and limits:** Read the complete codebase-details persona and implementation-readiness standard; complete #413 `design_draft.md` and `concepts_brief.md`; Nova `AGENTS.md`, `docs/README.md`, `docs/archive/README.md`, `docs/designs/{README,SITEMAP}.md`, `.agents/skills/opendash-elephant/SKILL.md`, and complete Nova pickup/review/dispatch skill content across reads; focused diffs for operating standard/GitHub workflow and the three Builder-adjacent contracts; full changed-file status inventory. Builder product/spec reads and the larger PRD/architecture/core/ABI diff batches were partially truncated; findings above use visible concrete provisions. Did not inspect CMS runtime, use CodeGraph, review script behavior, run checks, inspect private receipts/GitHub live state, or mutate files/Git/GitHub. This is a bounded immutable-document comparison, not exhaustive certification of all 243 differing files.
