Read-only BRAINSTORM report — UX & Developer Ergonomics.

The hypothesis has the right preservation goals, but a fresh agent still needs an executable entry path, an unambiguous state vocabulary, and recovery instructions before the migration can become implementation-ready.

1. **Major — Define the first-run and unavailable-plugin journey.**
   **Scope:** target `.agents/research/2026-11-20-egm-migration-concepts.md:13,17,24,30`; source `AGENTS.md:8–23`; `docs/designs/README.md:7–14`.
   **Usability friction:** The target says Desktop discovery is unverified and requires a fresh session, then directs the reader into several EGM commands. A newcomer cannot tell how to establish that these commands and reviewer capabilities are actually available, what counts as successful activation, or where to stop if the host exposes only some capabilities.
   **Ergonomic requirement:** Include one short host-capability checklist and a worked entry journey: identify plugin/version, verify command discovery and persona/standards reads, establish reviewer context limitations, select the appropriate workflow, and save the resulting artifacts. Define a safe blocked outcome and next action for missing capabilities. A fallback must preserve review obligations and must not silently claim upstream-equivalent isolation.

2. **Major — Separate design, execution, transport, verification, and release states visibly.**
   **Scope:** target lines 8,19,22; source `.agents/skills/github-task-pickup/SKILL.md:13–16,30–31`; `.agents/skills/atlas-task-dispatch/SKILL.md:38–48`; `docs/designs/SITEMAP.md:14,20–24`.
   **Usability friction:** The current source combines `Approved`, Project statuses, `Implemented`, `Verified`, and delivery receipts. `Implemented (Baseline)` labels the entire V1 requirements document, while another instruction permits changing the sitemap to either Implemented or Verified after merged code and passing tests. A newcomer lacks a precise rule for what each state establishes and which evidence or platform it covers.
   **Ergonomic requirement:** Provide a compact state table with state owner, qualifying evidence, scope/platform, and allowed next action. Keep independent fields for design approval, Project execution, transport delivery, implementation, verified scope, and release authorization. Include a legacy mapping example that preserves historical failures, open security holds, native testing and human acceptance; avoid a single global “Verified” label for partially verified requirements.

3. **Major — Resolve who approves each decision before presenting a linear workflow.**
   **Scope:** target line 18; source `AGENTS.md:5,13,23,25,35–38`; `docs/designs/README.md:10,14–16`; `.agents/skills/github-task-pickup/SKILL.md:16,27`.
   **Usability friction:** The hypothesis preserves delegated technical authority and reserves product scope/release for the owner. The source’s entry instructions repeatedly require owner approval before code, while pickup also recognizes autonomous eligible task work. A newcomer can follow one entry page and wait unnecessarily, or follow another and proceed without understanding which approval is authoritative.
   **Ergonomic requirement:** Define an approval matrix with concrete examples: technical design within delegated scope; changes to product scope; release authorization; and exceptions requiring escalation. Reference the same rule from all entry pages and skill gates. Record approval provenance at an immutable source/head, rather than treating `Status: Approved` as sufficient. This must preserve delegated technical authority and all ownership/Ready/CI/security/data gates.

4. **Major — Specify actionable refusal and recovery behavior across the entire dispatch journey.**
   **Scope:** target line 20; source `.agents/scripts/cos_dispatch.py:33,546–557,880–905`; `.agents/skills/atlas-task-dispatch/SKILL.md:24–25,32–39,50–58`; dispatch reference lines 17–19.
   **Usability friction:** Commands show an undefined `<Task-ID>`, while implementation accepts only the historical milestone format. The generated prompt still asks for approved G3. Errors such as `INVALID_ISSUE_ID_OR_TASK_ID`, `TASK_LOCK_INCOMPLETE`, and a SHA-change flag do not themselves explain a safe recovery. A newcomer can mistake a terminology mismatch for absent authorization or attempt a new dispatch identity to get past a blocked receipt.
   **Ergonomic requirement:** Define identifier examples and one consistent task identity spanning Issue generation, RUN, STATUS, RESUME, ACK and locks. For each refusal category, return the failing field/gate, expected form or evidence, safe next action, and retry constraints. Include a worked existing-legacy-Issue example and an ambiguous-delivery example. Preserve receipt identity and append-only history; changing terminology or source SHA must never authorize duplicate delivery.

**Read attestations and inventory**

I actually read:

- Installed bundle: `agents/comprehension.md`; `resources/document_standards/feature-spec-standard.md`; `resources/document_standards/heilmeier-standard.md` — complete.
- Target hypothesis — complete, lines 1–30.
- Source checkout `opendash-egm-review-20261120`: `AGENTS.md` 1–81; `.agents/README.md` 1–14; `docs/designs/README.md` 1–36; pickup skill 1–44; dispatch skill 1–70 — complete tool output.
- Additional actual snippets: adapter 1–50, 546–558, 880–910; sitemap 1–35. The dispatch reference and other adapter occurrences were inspected through line-numbered `rg` matches only.

No requested read failed. The supplied source/base SHA identities were not independently verified; no build, test, Git write, network mutation, history/memory lookup, or subagent operation was performed.

No parent-only marker is identifiable in the context visible to me. This observation establishes no instruction-file exclusion or isolation guarantee.
