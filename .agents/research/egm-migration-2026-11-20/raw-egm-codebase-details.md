Codebase Details-Brainstormer raw report — BRAINSTORM

Source inspected: `C:/Users/Thientran/Local Sites/opendash-workspace/opendash-egm-review-20261120`; read-only `git rev-parse HEAD` returned `e1a239126829a64c40df1ed86605dadb9ca2320c`. Baseline `ff957054d10077b5adc0cf1b704457319ba7a6c4` was supplied; I did not independently inspect that revision.

Attestations: I read the complete installed `agents/codebase_details.md`, `resources/document_standards/implementation-readiness-standard.md` and `feature-spec-standard.md` before source inspection. Their BRAINSTORM observability remit, explicit file boundaries, zero-decision readiness, alternatives and risks requirements inform this report. I read the complete working hypothesis `.agents/research/2026-11-20-egm-migration-concepts.md:1–30` in the original Codex checkout.

I see no distinct, identifiable parent-only test marker. I cannot establish that no undisclosed marker exists. Inherited host instructions, memory summary and surrounding conversation are visible in my context; a fresh parent turn or reviewer session does not establish their exclusion. No memory/history lookup occurred.

Findings and required concepts-brief commitments:

1. **Blocker — Observability & Operational SRE Debuggability: generated EGM identifiers cannot traverse the existing adapter.**
   `prepare-github-issue.py:220–221,247,272` generates `EGM-<filename-stem>`, explicitly asserted by `test_prepare_github_issue.py:207–209`. `cos_dispatch.py:33` accepts only integer-milestone `M…-Sxx-Txx`; `send:611–615`, `resume_issue:794–799` and `status_task:880–885` each enforce this expression. The older issue parser also permits decimal milestones (`prepare-github-issue.py:36`), which this transport expression excludes. The dispatch skill now advertises EGM dispatch (`.agents/skills/atlas-task-dispatch/SKILL.md:8,24,32–34`), while the actual outgoing prompt still requests approved G3 (`cos_dispatch.py:551–557`).
   **Requirement:** specify one canonical task identifier contract spanning issue generation, commands, remote arguments, prompts, ACKs, receipt lookup and locks. Test an actual generated payload through mocked send/status/resume, plus legacy decimal IDs where supported. Define collision behavior for equal filename stems in different nested design directories; `source_path:85–90` permits nested designs while `egm_issue_payload:220` uses only the stem. Do not regenerate existing task IDs to adopt EGM.

2. **Blocker — Observability & Operational SRE Debuggability: metadata currently provides no approval provenance.**
   `parse_egm_design:175–190` reads string metadata and accepts `Status: Approved`; it does not require approval evidence, review head, approving authority or merged-main ancestry. Duplicate metadata silently overwrites the earlier value (`178–179`), unlike the legacy parser’s duplicate rejection (`324–330`). `read_task_source:128–137` supplies immutable blob reading, which is valuable, but `main:454–465` accepts an arbitrary `--ref` and does not establish its approval or main ancestry. Tests at `test_prepare_github_issue.py:214–217,231–258` exercise the status string and immutable blob isolation, not approval provenance.
   **Requirement:** define the provenance record and who verifies it: approved design revision, approving authority under actual project rules, review/report references and merge/source relationship. Distinguish offline format validation from coordinator verification of live GitHub/Project gates. Reject ambiguous duplicate approval metadata. An Approved label cannot invent owner approval or replace delegated technical authority.

3. **Major — Observability & Operational SRE Debuggability: heading presence can masquerade as an executable plan.**
   `parse_egm_design:193–202` requires three nonempty H2 sections, rejects only TODO/TBD, and checks the two H3 headings using substring containment. Empty Target Files and Verification Plan bodies, headings inside a code fence, arbitrary filler, missing acceptance, missing dependencies and missing ownership are not rejected by these checks. The test fixture itself has only a target list and command list (`test_prepare_github_issue.py:63–94`); negative tests only rename headings or insert TODO (`219–229`).
   **Requirement:** define meaningful independently parsed subregions, explicit Create/Modify/Delete ownership, measurable acceptance, dependencies, immutable pins and change-scoped verification. Reject headings supplied solely by examples/fences, empty sections and inconsistent claims. Capture validation errors with source path and line/section so a coordinator can correct the exact defect.

4. **Major — Observability & Operational SRE Debuggability: verification recognition depends on presentation, and evidence existence is the full machine check.**
   `check-plans.py:62–65` only recognizes a leading bold label containing the substring `verified`; plain `Verified`, backticked or differently emphasized labels evade it, while bold `Not verified`/`Unverified` could trigger it. `is_verification_link:68–77` verifies only that some row link resolves to an existing immediate child Markdown file under `docs/verification`. It does not bind report scope, candidate, command, outcome or platform to the claim. `check:125–147` applies this only to sitemap table rows; the docstring’s broader “catalog tables” language at `14–15` exceeds the inspected implementation. Self-tests `186–227` use only bold positive Verified and missing-link variants.
   **Requirement:** define semantic statuses separately from Markdown formatting, explicit negation handling and evidence bindings. Test plain/bold/backtick/link statuses, scoped partial verification, NOT_RUN/FAIL, missing evidence, wrong candidate/platform and retained failure records. A link-validity PASS must remain distinct from acceptance or native/human verification.

5. **Major — Observability & Operational SRE Debuggability: preserve receipt identity and append-only recovery while changing identifiers.**
   The existing design has important protections: root identity hashes `[target, issue, task_id]` independent of SHA (`cos_dispatch.py:438–444`); exclusive root reservation precedes receipt and POST (`645–658`); successor identity binds the predecessor (`677–680`); chain traversal refuses incomplete, malformed or cyclic chains (`683–702`); status reports UNKNOWN for missing receipt with surviving lock (`851–869`); resume rechecks pins after reservation (`815–841`). Tests specifically preserve new-UUID/no-second-send, SHA drift, timeout, crash, chain and ACK behavior (`test_cos_dispatch.py:254–318`; `test_cos_resume.py:60–177`).
   **Requirement:** keep existing filenames/hash material and raw legacy identifiers stable; any schema addition needs an explicit compatibility policy, never lock deletion or automatic regeneration. Test existing on-disk legacy receipts and roots, successor chains, lock-without-receipt, old failures and UNKNOWN states against the new code. Define a safe manual reconciliation procedure; migration must not authorize another send.

6. **Major — Observability & Operational SRE Debuggability: receipt timeline and provenance are insufficient for incident reconstruction.**
   The adapter contract says every dispatch record contains a timestamp, idempotency key, state, redacted error and reconciliation result (`direct-adapter-contract.md:21`). Actual new records (`cos_dispatch.py:639–643,818–821`) contain identifiers, pins, hash and outcome, but no creation/update/reconciliation timestamps or captured host/app/schema version. `probe:425–430` exposes app version transiently without binding it to the dispatch receipt. `status:874–877` omits source SHA from output; `status_task:904–905` reports only a changed-source boolean.
   **Requirement:** add bounded, private structured lifecycle fields for reservation, send outcome, observation time, schema/adapter/app versions and last successful/failed reconciliation, carrying correlation fields without raw transcript or secrets. Preserve compatibility for old receipts without those fields. State which fields are safe in public evidence versus local-only records.

7. **Major — Observability & Operational SRE Debuggability: sticky transport observations conceal freshness limits.**
   `reconcile_record:568–602` searches only the most recent 50 inputs, does not paginate that list and suppresses Blocked exceptions. Once `sent` or `assistant_turn_ended` is observed it remains sticky; `test_transient_status_failure_does_not_erase_verified_turn:378–386` explicitly requires preservation. This is appropriate historical evidence, but current output cannot distinguish “observed now” from “last observed before endpoint failure.” Resume adds a stronger current check with full session pagination and terminal timestamp (`705–741`).
   **Requirement:** retain sticky historical observations but expose last observation time, current reconciliation success/error and bounded-search limitations. Never equate transport `turn_end` with task acceptance or product success. Provide a clear operator path for “accepted but no current delivery visibility” without retrying POST.

8. **Major — Observability & Operational SRE Debuggability: scope-specific checks and state separation need to survive issue rendering.**
   EGM issue text unconditionally requires `cargo test --workspace --release --locked` plus docs checkers (`prepare-github-issue.py:253–257`). It loses the legacy renderer’s explicit PASS/FAIL/NOT_RUN and `MERGED` versus `VERIFIED` wording (`418–421`). The sitemap currently labels the entire V1 requirements document `Implemented (Baseline)` (`docs/designs/SITEMAP.md:24`); I did not audit every V1 requirement, so that broad claim has no verification support from this inspection.
   **Requirement:** generate explicit approved-design / implementation / reviewed-head CI / post-merge verified-scope states, each with appropriate evidence and checks selected by actual file/behavior scope. Preserve native macOS, human dogfood, F08 security hold, F09 old Windows failed receipts and common RC/release obligations exactly; an EGM status cannot collapse them.

9. **Major — Observability & Operational SRE Debuggability: artifact safety PASS is a narrow statement.**
   `check-agent-artifacts.py:13–18` is an active-skill allowlist, not approval evidence for design documents or upstream workflow adaptations. `44–67` catches selected runtime filenames/directories and strong secret patterns, and `74` accurately limits the secret claim to “obvious.” Migration changes must not treat this as verification that receipts or all secrets are absent. The allowlist and the dispatch prompt’s reference to `discussion-turn-responder/SKILL.md` (`cos_dispatch.py:533`) warrant direct reconciliation by the migration owner; I did not inspect whether that referenced source file currently exists.
   **Requirement:** document the checker’s exact assurance boundary; keep runtime receipts and credentials private, and require active-document links/prompts to align with the final approved skill inventory.

Existing safeguards to retain:

- Shared-account, exact Issue/PR/head/base/source and structured ACK binding, coordinator identity and ACK chronology (`cos_dispatch.py:744–790`).
- Complete current-session pagination and active-slot refusal (`705–741`).
- Local protected receipts and host guard before API credential access (`319–382`; contract `12,17–21`).
- No watchers, blind timeout retries or transport-to-product verification inference (contract `17–19,27,45–53`).
- Separate sender verification of Project Ready/dependencies/ownership; the adapter does not itself query Project Ready in `check_resume`, as the contract explicitly separates responsibilities (`direct-adapter-contract.md:45–49`).

Suggested acceptance evidence for the later implementation plan: fixture-based producer-to-consumer tests for EGM and legacy IDs; duplicate/timeout/crash/source-drift and resume-generation tests with preserved historical receipts; semantic verification-status parser fixtures; empty/fenced/misleading plan negatives; approval provenance negatives; structured redacted receipt output including freshness and version fields. Host/plugin workflow and native/human acceptance remain separate, real evidence requirements.

Exact read inventory:

- Installed persona and both named standards: complete files.
- Working hypothesis in original Codex checkout: complete `1–30`.
- `.agents/scripts/prepare-github-issue.py`: `1–484`; initial output omitted a small middle region, subsequently read `397–424`, completing it.
- `.agents/scripts/check-agent-artifacts.py`: complete `1–79`.
- `cms/scripts/check-plans.py`: complete `1–249`.
- `.agents/scripts/cos_dispatch.py`: full regions `1–100`, `285–382`, `395–440`, `438–447`, `530–559`, `560–705`, `705–743`, `744–905`, `901–908`; additional matching lines from an `rg` search throughout, including remote/CLI sections. Unlisted regions were not fully read.
- `.agents/scripts/test_prepare_github_issue.py`: `63–96`, `197–259`; additional matching lines from whole-file `rg`, including legacy cases.
- `.agents/scripts/test_cos_dispatch.py`: complete method-name inventory by `rg`; bodies `242–320`, `345–387`.
- `.agents/scripts/test_cos_resume.py`: complete `1–191`.
- `.agents/skills/atlas-task-dispatch/references/direct-adapter-contract.md`: complete `1–53`.
- `.agents/skills/atlas-task-dispatch/SKILL.md`: `rg` matching lines for EGM/RUN/STATUS/RESUME/readiness and identifiers; not a complete read.
- `.agents/research/2026-11-19-cos-resume.md`: matching lines from `rg`, not a complete read.
- `docs/designs/SITEMAP.md`: matching status/evidence lines `14,24–34`, not a complete read.
- File-path inventory searches under `.agents` and `cms/scripts`; read-only `git rev-parse HEAD`.

Unavailable reads: none attempted failed. Scope-limited unread regions are listed above and must not be represented as audited. No edits, builds, tests, network/GitHub calls, Git writes, memory/history reads or subagents were performed.
