Read-only BRAINSTORM report — Maintainability-Brainstormer

I read the actual `agents/structural.md` persona and `feature-spec-standard.md` / `implementation-readiness-standard.md` in the installed EGM 0.1.1 bundle. I applied their maintainability mandate, Alternatives Rule, explicit risk requirement and implementation boundary/Zero-Decision criteria as upstream design requirements, without treating this working hypothesis as an implementation-ready specification.

Paths below are relative to source checkout `C:/Users/Thientran/Local Sites/opendash-workspace/opendash-egm-review-20261120`, except the concept hypothesis, which is in the parent’s main checkout.

1. **Major — Long-Term Tech Debt & Maintainability: preserve obligations through an explicit migration crosswalk.**

   The hypothesis promises to retain gates and historical failures (`.agents/research/2026-11-20-egm-migration-concepts.md:18–23`), but does not yet define how retired terminology remains connected to live obligations. The proposed source says EGM completely replaces the old process (`AGENTS.md:5`) while the operational workflow still requires merged G3 READY sources and an agreed G2 (`.agents/GITHUB_WORKFLOW.md:127–159`). Meanwhile, ADR-0010 retains precise candidate, platform, human-test, security-hold and release requirements (`docs/decisions/ADR-0010-agent-project-and-merge-authority.md:42–66`).

   **Maintainability requirement:** Before moving/archiving documents, record a bounded crosswalk from each active obligation to its EGM successor, authoritative source and responsible owner. Distinguish changing terminology from changing authority. Retain immutable legacy pointers and unresolved receipts; do not rewrite historical verdicts. Include macOS B01–B10 and negative regressions on one candidate, human dogfood, F08 provenance/security requirements, F09 Windows follow-up, separate common RC/V1 gates, and reviewed-head CI. Compare wholesale renaming with gradual coexistence and justify the chosen migration.

2. **Major — Long-Term Tech Debt & Maintainability: task identity is persistent operational state, not presentation.**

   Issue generation defaults to `EGM-{spec stem}` (`.agents/scripts/prepare-github-issue.py:220–221`), whereas dispatch validates only milestone IDs (`.agents/scripts/cos_dispatch.py:33,612–614,797–798,882–883`). More consequentially, the dispatch root lock hashes `(target, issue, task_id)` and intentionally survives source/base changes (`cos_dispatch.py:438–444`); resume and status walk that identity (`:683–698,815–823,890–905`). A later rename or normalization could orphan an existing lock and permit duplicate work.

   **Maintainability requirement:** Define one stable task identity contract shared by generation, dispatch, acknowledgments, status and resume, with legacy IDs preserved byte-for-byte. Specify namespace validation separately from receipt keys and define behavior for spec renames, multiple implementation tasks per spec, changed source pins, and existing Issues. Require compatibility fixtures covering legacy receipt lookup, duplicate dispatch prevention and authorized resume. Do not migrate this by merely widening regular expressions.

3. **Major — Long-Term Tech Debt & Maintainability: separate design metadata from authorization and evidence.**

   Parsing accepts `Status: Approved` and nonempty sections, then merely checks that target-file and verification headings occur (`prepare-github-issue.py:187–202`). This cannot establish approval provenance, meaningful file boundaries or acceptance. The index independently marks all V1 requirements “Implemented (Baseline)” (`docs/designs/SITEMAP.md:24`), although the verification index explicitly retains native limitations and Windows follow-ups (`docs/verification/README.md:35–43`). Evidence validation applies only to bold labels containing “verified” (`cms/scripts/check-plans.py:62–65,142–146`).

   **Maintainability requirement:** Specify separate design, implementation and verification states, with scoped evidence and approval provenance. A catalog row must not substitute for Project Ready, ownership, dependencies or required checks. Define meaningful section validation and malformed/empty-section cases, and validate status semantics independent of Markdown emphasis. Keep human review responsible for sufficiency; automate structural claims only.

4. **Major — Long-Term Tech Debt & Maintainability: avoid multiple manually maintained document registries.**

   New designs must be registered in the sitemap (`docs/designs/README.md:11`), and the checker discovers all design files (`cms/scripts/check-plans.py:152–157`). However, CodeGraph uses a fixed source array containing only the design README and sitemap, with no feature specs (`cms/scripts/codegraph-index-docs.mjs:25–49`). Consequently, a correctly registered new design is still absent from semantic indexing unless somebody edits another script.

   **Maintainability requirement:** Choose one canonical source inventory, with deterministic derivation for sitemap validation and CodeGraph input, or explicitly justify separate inventories and enforce synchronization. Define draft/approved inclusion rules, update/removal behavior and historical preservation. Check actual spec content is indexed rather than assuming indexing its table-of-contents row is equivalent.

5. **Major — Long-Term Tech Debt & Maintainability: centralize verification policy by change scope.**

   `AGENTS.md:61–71` mandates the release Rust suite after documentation edits; the context skill repeats another mandatory list (`.agents/skills/opendash-elephant/SKILL.md:39–47`), the design guide repeats it (`docs/designs/README.md:17`), and generated Issues hard-code it again (`prepare-github-issue.py:255–257`). Four surfaces will drift, while prose-only iterations incur expensive runtime checks unrelated to their risks.

   **Maintainability requirement:** Establish one canonical verification policy with scope-dependent commands and explicit escalation rules. Templates and generated Issues should reference that policy while preserving task-specific acceptance. Documentation checks cannot replace runtime/security/data/native tests, human dogfood or release authorization when those scopes apply.

**Read inventory:** Full concept hypothesis; full source AGENTS; source GitHub workflow sections on authority, source-of-truth, Issue generation, task IDs, Ready, branch handling and PR checks; full opendash-elephant skill; full design README/sitemap; full `check-plans.py` and CodeGraph index script; targeted issue-generator and dispatch implementations; targeted ADR-0010, product requirements and verification index. No requested files were unavailable. No edits, builds, tests, Git writes, network mutations, history/memory retrieval or subagents were performed.

**Isolation attestation:** I cannot identify or verify any parent-only marker from the material visible to me. Parent conversation/instructions are visible in this session, so this review does not demonstrate conversation isolation or instruction-file isolation.
