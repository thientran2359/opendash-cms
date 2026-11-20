# EGM Stage 9 documentation consistency review

Date: 2026-11-20

Review result: **3 Major findings, 1 Nit; no Blocker.** The documentation-first scope is viable, and the recorded authority adaptation does not require a new owner approval gate.

1. **Major — The interim new-design admission rule remains undecided.**

   Affected scope: `.agents/research/egm-migration-2026-11-20/design_draft.md:171–181`, `operating-guide.md:81–104`, `operating-guide.md:133–145`.

   The guide permits new EGM designs and proposes producing a Builder handoff before product implementation, while excluding generator/adapter changes and largely deferring skill-body changes. Existing operational sources still require specific G3 admission:

   - `.agents/skills/github-task-pickup/SKILL.md:22`: merged G3/Full-Track G2 and Issue eight-section contract.
   - `.agents/skills/atlas-task-dispatch/SKILL.md:27–28`: merged G3 source.
   - `.agents/GITHUB_WORKFLOW.md:126–145`: G3 READY, prescribed task headings and task template before generator use.
   - `docs/plans/PLANNING_WORKFLOW.md:119`: no implementation READY Issue from proposed design content.

   The prohibition against automatically admitting an incompatible format is sound, but does not select an executable interim path. Specify one documentation-only rule: either all new-format implementation admission remains blocked pending the separate compatibility work, or EGM designs must supply an accepted task handoff using the currently supported task-source contract, with explicit source provenance and no duplicate authority definition. State this consistently in the guide, workflow and Planning Workflow. Do not imply that a guide pointer changes an active skill or adapter contract. No transport redesign is needed for this fix.

2. **Major — Workflow selection lacks the retained Full-Track trigger mapping.**

   Affected scope: `operating-guide.md:18–30`; `design_draft.md:169–173`, `design_draft.md:183–189`.

   The new entry guide selects lite for a bounded change inside an accepted contract, and lists only new product behavior, shared contracts/schema and ownership conflicts as escalation triggers. Existing `docs/plans/PLANNING_WORKFLOW.md:94–99` mandates Full-Track for HTTP routes, auth/RBAC/CSRF/Origin, uploads, public rendering/CSP, ZIP/plugin permissions and sandbox boundaries, as well as schema/API/ABI changes. Its rule also defaults uncertain impact to Full-Track. These retained obligations are broader than the new guide’s examples.

   Because the plan explicitly cuts over new-design entry while mapping ongoing G0–G5 tasks, an implementer must currently decide whether those rules survive only for legacy work. Add an explicit statement and source pointer that EGM procedure selection does not lower those existing risk criteria; map the retained criteria to the new design/checkpoint vocabulary, retaining failure, permission and data-preservation acceptance. This can be a short authoritative cross-reference rather than duplicated lists.

3. **Major — The slice’s prescribed validation omits an existing mandatory documentation check.**

   Affected scope: `design_draft.md:183–189`.

   The executable slice names the plans checker, its self-test and `git diff --check`, but `.agents/GITHUB_WORKFLOW.md:391–400` also requires:

   ```text
   python .agents/scripts/check-agent-artifacts.py
   ```

   This slice modifies `.agents/` operational documentation and presents its checks as the acceptance sequence. Add that existing command, record actual PASS/FAIL/NOT_RUN, and retain required CI at the reviewed head. No runtime testing or new test suite is necessary.

4. **Nit — Promotion must explicitly rebase the guide’s relative links.**

   Affected scope: `design_draft.md:168`; `operating-guide.md:156–163`.

   The current links are correct for `.agents/research/egm-migration-2026-11-20/operating-guide.md`, but cannot be copied unchanged to `docs/plans/EGM_WORKFLOW.md`. For example, `../../GITHUB_WORKFLOW.md` would resolve to a nonexistent repository-root file, and the three `../../../docs/...` links would leave the repository root.

   Specify these successor destinations:

   - Migration draft and concepts: `../../.agents/research/egm-migration-2026-11-20/{design_draft,concepts_brief}.md`
   - Authority: `../decisions/ADR-0010-agent-project-and-merge-authority.md`
   - Workflow and operating standard: `../../.agents/{GITHUB_WORKFLOW,AGENT_OPERATING_STANDARD}.md`
   - Builder pilot: `../designs/BUILDER_THEME_CONTENT.md`

   Keep the research copy’s current links, replacing its entry status with the planned successor pointer.

Exact read inventory:

- Full persona: `/Users/thientran/.codex/plugins/cache/egm/egm/0.1.1/agents/codebase_details.md`.
- Full standard: `/Users/thientran/.codex/plugins/cache/egm/egm/0.1.1/resources/document_standards/implementation-readiness-standard.md`.
- Full targets: `design_draft.md` (203 lines), `operating-guide.md` (163 lines).
- Full requested baseline: `.agents/skills/README.md`; all four active project `SKILL.md` files; `.agents/GITHUB_WORKFLOW.md`; `.agents/AGENT_OPERATING_STANDARD.md`; `docs/plans/PLANNING_WORKFLOW.md`; repository `AGENTS.md`.
- Additional full integration sources: `docs/decisions/ADR-0010-agent-project-and-merge-authority.md`, `docs/README.md`, `docs/plans/README.md`, `.agents/README.md`.

Limits: Read-only documentation review. No file edits, Git writes, GitHub access/mutations, memory/history inspection, subagents, runtime/script inspection, tests or live compatibility probes. Findings concern the documentation slice’s executability against existing operational documents; they do not claim generator/adapter defects were independently reproduced.
