Inventive Catalyst — BRAINSTORM. Confirmed inspected checkout HEAD: `e1a239126829a64c40df1ed86605dadb9ca2320c`.

The working hypothesis avoids the main false compromise: official EGM adoption does not require weakening implementation or release gates. Three concrete contradictions in the proposed source migration need explicit resolutions.

1. **Major — Contradiction Elimination & Inventive Transforms**
   - **Affected:** `AGENTS.md` §1 opening, draft/resolve approval steps and §1.2; ADR-0010 authority and task-pickup rules.
   - **Contradiction:** Standardizing official EGM versus preserving delegated coordinator technical authority. Source AGENTS requires owner approval throughout and says old coordination is wholly replaced, while ADR/workflow retain coordinator decisions and autonomous eligible pickup.
   - **Pattern: Extraction.** Separate EGM design/review procedure from the authority policy. Record a narrow, explicitly approved adaptation identifying who may approve technical sections, who decides product scope, and which release actions require owner authorization. Do not represent the adapted procedure as unmodified upstream.
   - **Secondary trade-off:** One small policy mapping must remain synchronized with plugin upgrades; ambiguous approval provenance becomes an explicit blocker.
   - **IFR:** A reviewer can identify the design approver and source evidence immediately without reopening settled delegation or weakening owner product/release authority.

2. **Major — Contradiction Elimination & Inventive Transforms**
   - **Affected:** `AGENTS.md` §1.2 and historical-process retirement statement; `.agents/AGENT_OPERATING_STANDARD.md` §§2/4/5; `.agents/GITHUB_WORKFLOW.md` lifecycle and Project statuses; `.agents/scripts/cos_dispatch.py` `prompt`, `issue_lock_file`, `latest_task_record`, `check_resume`, `status_task`.
   - **Contradiction:** Replacing legacy workflow terminology versus maintaining ongoing Issues and safe dispatch/resume receipts. Task identity participates in lock and successor lookup; terminology migration cannot safely imply identity migration.
   - **Pattern: Intermediary.** Use a small compatibility mapping at task admission: legacy G3 evidence and new approved EGM specifications satisfy the same explicit readiness obligations, while exact existing Issue/task IDs, receipt keys, source/base pins and resume chains remain intact. Keep one live GitHub state machine.
   - **Secondary trade-off:** Two accepted source formats temporarily require validation and retirement criteria. Do not add a second queue or duplicate receipt store.
   - **IFR:** Legacy and new tasks travel through the same dispatch/status/resume pipeline without renamed locks, duplicated sends or lost failure history.

3. **Major — Contradiction Elimination & Inventive Transforms**
   - **Affected:** `AGENTS.md` §3 mandatory verification commands; `.agents/skills/opendash-elephant/SKILL.md` verification requirement; `.agents/GITHUB_WORKFLOW.md` documentation checklist.
   - **Contradiction:** Strong verification versus bounded, useful iteration. AGENTS requires a Rust release suite after either code or documentation changes; the workflow already allows checks appropriate to documentation.
   - **Pattern: Segmentation.** Define verification classes by affected behavior: documentation/link/schema consistency; dispatcher and receipt semantics; runtime changes; native candidate acceptance; human dogfood/release. Each class states triggers and required evidence. Preserve mandatory CI and security/data/native/human/release obligations independently of local prose-only checks.
   - **Secondary trade-off:** Classification needs explicit rationale and a conservative escalation rule for workflow/script changes; otherwise authors can underclassify changes.
   - **IFR:** Every change runs the checks that can establish its claims, and release evidence remains tied to the exact candidate and platform.

**Minimal adoption shape:** Official EGM provides authoring and critique; existing GitHub admission, identity, integration, acceptance and release rules remain the operational substrate. A compact mapping plus one end-to-end pilot is preferable to a new orchestration framework. Security/data preservation and human/native acceptance are immutable boundaries, not contradictions to optimize away.

**Read inventory**
- Fully read installed `agents/inventive_catalyst.md`, `docs/codex.md`, `resources/document_standards/feature-spec-standard.md`.
- Fully read target `.agents/research/2026-11-20-egm-migration-concepts.md`.
- Fully read pinned `.agents/AGENT_OPERATING_STANDARD.md`.
- Inspected pinned `AGENTS.md`, `.agents/GITHUB_WORKFLOW.md` through tool output, which was truncated; do not treat these as complete reads.
- Targeted search inspection of pinned ADR-0010, `cos_dispatch.py` and `opendash-elephant/SKILL.md`.
- File inventory under pinned `docs/`.
- `docs/plans/PLANNING_WORKFLOW.md` and the attempted `cms/scripts/prepare-github-issue.py` location were absent; neither was reviewed.

**Limits:** No source edits, writes, builds, network actions, memory/history retrieval or other lens reports. Fresh conversation isolation was requested by dispatch, but host-supplied project/user instructions are visible; instruction-file exclusion is unverified. Findings establish conceptual migration risks, not executable compatibility or a clean full-document review.
