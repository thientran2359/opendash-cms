Read-only design critique completed against checkout `e1a239126829a64c40df1ed86605dadb9ca2320c` (confirmed with `git rev-parse HEAD`). No edits, builds, tests, Git writes, GitHub mutations, memory retrieval or subagents.

References below use:
- **Concept** = `C:/Users/Thientran/Local Sites/opendash-workspace/opendash-cms-win-codex/.agents/research/2026-11-20-egm-migration-concepts.md`
- Other repository paths = `C:/Users/Thientran/Local Sites/opendash-workspace/opendash-egm-review-20261120/`

The concept identifies the right preservation obligations, but does not yet provide a bounded migration architecture or measurable adoption decision.

1. **Blocker — Authority/state architecture.** Concept:18–19 proposes reconciliation but does not specify the replacement state machine, decision owners or precedence. This is already a concrete contradiction in Nova’s source: `AGENTS.md:5,13,23–25` requires owner approval for technical design and implementation, while `docs/decisions/ADR-0010-agent-project-and-merge-authority.md:75–85,91–101` delegates technical decisions, self-review and eligible merges. `AGENTS.md:81` calls the old governance historical, while `.agents/GITHUB_WORKFLOW.md:242–253` still uses it for active Ready eligibility. **Mitigation requirement:** an explicit old→new obligation matrix and authoritative transition table, including who decides, immutable source/base, preconditions, evidence and forbidden transitions. Technical authority must remain delegated as already authorized; owner approval must not become a newly invented technical gate. Product-scope and release decisions remain separate.

2. **Blocker — Task identity and receipt continuity.** Concept:20 correctly rejects a regex-only migration, but leaves canonical identity and compatibility undefined. `.agents/scripts/prepare-github-issue.py:220–221` generates `EGM-<stem>` by default; `.agents/scripts/cos_dispatch.py:33,612–615,797–799,882–885` accepts only legacy task IDs. More seriously, `.agents/scripts/cos_dispatch.py:438–444` hashes `[target, issue, task_id]` into the root dispatch lock. Renaming a legacy task ID can produce another root lock and undermine duplicate prevention even if all new regexes accept it. Existing dispatch reserves UNKNOWN before POST (`:638–670`), and resume walks append-only generations (`:683–702`). **Mitigation requirement:** preserve historical task IDs and receipts, specify canonical transport identity/versioning for new tasks, and test RUN→STATUS→ACK→RESUME using the same identity. Include source change, timeout after POST, crash before receipt completion, duplicate UUID, concurrent send and mixed old/new host versions. Do not delete/recreate receipts to make the pilot pass.

3. **Major — Cost, quota and token economics are unbounded.** Concept:17,24 requires fresh reviewers and recorded elapsed time, but provides no reviewer concurrency, token/tool budget, review-round limit, corpus limit or timeout. `.agents/skills/opendash-elephant/SKILL.md:27–35` mandates seven baseline context documents plus source for each review; `AGENTS.md:16–23` describes numerous critic roles. Repeated whole-corpus reads across fresh contexts can dominate both cost and latency. This is an agent orchestration workload; CMS QPS/database load is not the relevant capacity metric because CMS runtime changes are excluded at Concept:27. **Mitigation requirement:** define a review packet manifest with bytes/estimated tokens, maximum concurrent critics compatible with host slots, maximum rounds, aggregate token/tool/time budget, cancellation rules and an explicit unresolved-findings outcome when the budget expires. Suggested pilot starting bounds, subject to coordinator decision: three concurrent critics, two review/remediation rounds, one bounded packet per critic, and no automatic additional round. Record actual usage before selecting permanent limits. Budget exhaustion cannot manufacture PASS.

4. **Major — Validation has an expensive unconditional path.** Concept:23 intends impact-based checks, but no selection matrix or policy owner is proposed. Current `AGENTS.md:61–71` demands a Rust build and full release workspace tests after *code or documentation* changes. The context skill also mandates release tests for every verification plan (`.agents/skills/opendash-elephant/SKILL.md:37–47`), and generated EGM issues repeat that requirement (`.agents/scripts/prepare-github-issue.py:253–257`). This can make small prose maintenance disproportionately costly and encourage false NOT_RUN/PASS reporting. **Mitigation requirement:** define check classes for prose/link moves, workflow/parser changes, runtime changes and release candidates. Preserve required CI and behavioral/security/data tests for affected code; retain native macOS B01–B10, human dogfood, F08 security hold, and F09’s two full suites plus isolated run under `ADR-0010:52–66`. Reducing irrelevant local Rust runs must not weaken those gates.

5. **Major — Approval/evidence validation remains fail-open unless semantics are defined.** Concept:21–22 states the requirement without choosing enforceable criteria. `.agents/scripts/prepare-github-issue.py:187–202` accepts Approved text, nonempty top-level sections and presence of two subsection headings; the inspected code does not establish useful target-file contents or approval provenance. `cms/scripts/check-plans.py:62–65` requires a verification link only when a status begins with bold text, and `:68–76` checks for an existing Markdown evidence file rather than evidence matching scope/SHA/platform. **Mitigation requirement:** define minimum semantic fields and negative fixtures before implementation: empty target list, heading-only verification, unverifiable Approved provenance, wrong-head evidence, wrong-platform evidence, partial verification presented as full verification, and plain/italic/bold Verified variants. An automated checker may verify structure and references; the coordinator must still assess actual execution evidence.

6. **Major — The pilot lacks pass/fail exams, comparative value and a rollback boundary.** Concept:8 names adopting EGM as the goal; Concept:24 names a pilot without an outcome threshold or alternative. The Heilmeier standard requires cost, timeline and exams (`resources/document_standards/heilmeier-standard.md:20–30`); the feature standard requires alternatives with concrete trade-offs (`feature-spec-standard.md:13–27`). **Mitigation requirement:** compare at least full replacement, EGM as a design/review layer retaining operational gates, and staged adoption. Define one small real pilot scope, exact owned files, a measured baseline, useful-findings criteria, compatibility tests, and an explicit coordinator decision to adopt/revise/reject. Preserve a usable active guidance path until the new workflow passes. Reverting documentation must not delete legacy receipts or historical failed verification.

7. **Major — Documentation migration needs an integrity invariant beyond link repair.** Concept:23 says preserve requirements and update source pointers, but not how lost obligations are detected. The original active path `docs/plans/PLANNING_WORKFLOW.md` is absent; its relocated file still contains active obligations: whole-milestone specifications versus per-task Ready (`docs/archive/plans/PLANNING_WORKFLOW.md:17–23`), immutable IDs (`:54`), separate verification states and retained history (`:64–77`), scoped file ownership and Stop-and-Propose (`:118–124`). Simply marking it historical can hide still-binding rules. **Mitigation requirement:** inventory each active normative obligation with its new owning location and preservation verdict before bulk relocation. Retain historical SHA links, map moved paths, and verify that required obligations exist in active entry points. Keep CodeGraph index refresh as a traceable check, not a substitute for source integrity review.

Read inventory:

- Fully read installed persona: `C:/Users/Thientran/.codex/plugins/cache/egm/egm/0.1.1/agents/design_critic.md`.
- Fully read installed standards:
  - `.../resources/document_standards/heilmeier-standard.md:1–30`
  - `.../resources/document_standards/feature-spec-standard.md:1–39`
  - `.../resources/document_standards/implementation-readiness-standard.md:1–37`
- Fully read Concept:1–30.
- Fully read review-checkout `AGENTS.md:1–81`.
- Fully read `docs/decisions/ADR-0010-agent-project-and-merge-authority.md:1–101`.
- Fully read `.agents/GITHUB_WORKFLOW.md:1–427` across bounded reads.
- Fully read `.agents/skills/opendash-elephant/SKILL.md:1–48`.
- Fully read relocated `docs/archive/plans/PLANNING_WORKFLOW.md:1–134`.
- Source excerpts read:
  - `.agents/scripts/prepare-github-issue.py:165–265`
  - `.agents/scripts/cos_dispatch.py:438–446,605–675,683–702`
  - `cms/scripts/check-plans.py:31–135`
- `rg` search excerpts only, not full reads:
  - `.agents/scripts/prepare-github-issue.py`
  - `.agents/scripts/cos_dispatch.py`
  - `.agents/skills/atlas-task-dispatch/SKILL.md`
  - `.agents/skills/github-task-pickup/SKILL.md`
  - `cms/scripts/check-standards.py`
  - `cms/scripts/check-plans.py`
- File discovery only: relevant script and relocated planning paths under `docs/` and `.agents/`.

Unavailable read: requested `docs/plans/PLANNING_WORKFLOW.md` does not exist in this checkout; read the actual relocated file instead. No other attempted file read failed. No comparison against baseline `ff957054d10077b5adc0cf1b704457319ba7a6c4` was performed, so these findings do not claim to exhaust that diff. Live GitHub state, installed Desktop discovery, native host execution, actual token cost and reviewer instruction isolation were not verified.

No identifiable parent-only marker is visible to me. I did receive the user-supplied workspace instructions and environment context alongside the delegated task. Fresh conversational context therefore does not demonstrate instruction-file exclusion.
