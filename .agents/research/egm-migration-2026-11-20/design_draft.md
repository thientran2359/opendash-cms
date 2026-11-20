# OpenDash official EGM adoption

* **Author:** Atlas
* **Status:** Draft — Overview & Goals and authority adaptation approved by owner; Technical Plan drafted, detailed plan/review pending
* **Date:** 2026-11-20
* **Predecessor:** [Refined concepts brief](concepts_brief.md), grounded in the owner's already approved scope.
* **Candidate under evaluation:** `e1a239126829a64c40df1ed86605dadb9ca2320c`.
* **Main baseline:** `ff957054d10077b5adc0cf1b704457319ba7a6c4`.

## Overview

OpenDash will use the official Elephant-Goldfish Model (EGM) plugin for concept development, specification authoring, independent design critique, documentation maintenance and small engineering iterations. New work will use explicit design, implementation, verification and release states instead of active G0–G5 terminology. GitHub Issues, Project and PRs remain the live task and integration records.

This migration preserves the coordinator's delegated technical authority and the owner's authority over product scope and release. It retains file ownership, immutable source/base, dependencies, Project Ready, CI at the reviewed head, post-merge verification, security and data preservation, native platform evidence and human dogfood. Official EGM checkpoints remain applicable; any adaptation must be stated and approved explicitly rather than inferred from a metadata label.

The existing Nova work is a candidate to evaluate and repair, not an already approved process. The migration will preserve ongoing legacy tasks and receipts without introducing a second task queue or duplicate dispatch identity. The originally dirty Builder documents have now been preserved and transcribed under the owner's separate authorization, as recorded below.

Preservation explicitly includes the owner's theme-structure decisions in `docs/product/BUILDER_TEMPLATE_AND_CONTENT.md`, `docs/contracts/BUILDER_TEMPLATE_RESOLVER.md` and `docs/decisions/ADR-0020-builder-template-resolver.md`: main templates/header/footer at theme root alongside `template-part/`, `page-layouts/`, `sections/` and `assets/`. They are existing settled requirements, not new design choices to reopen. They are pinned by commit `4a334f0224866108002e8586b04f5643da0798c9`; their Proposed EGM transcription is in commit `fccada1`. They have not been incorporated into the Nova candidate or implemented in CMS runtime. Integration must retain their substance and provenance.

## The Problem

The Nova candidate changes 228 files, but its entry instructions, authority policy, generators, transport and evidence checks do not yet form one consistent process. The Issue generator emits EGM identifiers that the adapter rejects. Existing locks depend on the exact task identity, so renaming tasks can bypass duplicate-delivery protection or break resume continuity. Empty target-file and verification subheadings pass the current spec parser, and evidence validation changes with Markdown emphasis. Entry instructions introduce blanket owner technical approvals while ADR-0010 retains delegated authority. The design catalog also labels the entire V1 requirements document implemented without a bounded implementation claim.

Existing automated checks pass despite these reproduced gaps. A terminology rewrite or a green documentation checker therefore cannot establish safe adoption. The migration needs an explicit contract, retained-obligation mapping, compatible operational behavior and a bounded real pilot. The [host audit](host-audit.md) records source pins, plugin capability, actual commands, probe results and limitations; the [concepts brief](concepts_brief.md) links all seven completed brainstorm reports.

## Goals

1. Establish one documented EGM entry path using actual installed skills, personas and standards. Record host/version, reviewer completion and observed context isolation limits, with an actionable blocked result when required capabilities are unavailable.
2. Reconcile project entry instructions, ADR-0010, GitHub workflow, operating standard and active manual skills so that the approver and authority for each decision are explicit. Preserve coordinator technical autonomy and actual EGM checkpoints without reopening settled product scope.
3. Preserve every active acceptance obligation when terminology and document locations change. Map each retained obligation to its authoritative source, owner and successor state. Historical failures and unresolved security holds remain intact.
4. Make task admission, Issue generation, RUN, STATUS, RESUME, ACKs, prompts, locks and receipts compatible for new EGM work and ongoing legacy Issues. Existing identities and receipt chains remain stable; ambiguous delivery must never become permission for a second send.
5. Require meaningful target-file, acceptance, dependency, ownership and verification content. An Approved label must have review and authority provenance; it does not itself prove Ready status or permission to implement.
6. Separate approved design, Project execution, transport delivery, merged implementation, scoped verification and release authorization. Validate evidence obligations independently of Markdown emphasis and preserve exact candidate/platform and PASS/FAIL/NOT_RUN boundaries.
7. Preserve product requirements, contracts and authoritative historical references during document reorganization. Keep source pointers and actual CodeGraph design content synchronized without certifying unverified requirements through a catalog row.
8. Match checks to affected behavior while preserving required CI and all task-specific security/data/native/human/release checks. Workflow and script changes require meaningful negative and compatibility tests; prose checks cannot certify runtime behavior.
9. Complete a measured coordinator-host pilot with real EGM artifacts, useful findings, decision provenance, actual checks and explicit limits before broad adoption and integration merge. Recheck source, ownership, dependencies, Project Ready and reviewed-head checks before implementation or merge.

## Non-Goals

- CMS runtime features, database/schema changes or runtime-data migration.
- Automatic dispatch, watchers, a second queue, lock deletion or blind retries.
- Editing/pushing Nova without a recorded handoff, or changing settled Builder requirements outside the owner's separately authorized documentation conversion.
- Bypassing branch protection, security/data holds, ownership, Ready, CI or release authorization.
- Certifying another host's plugin support, native macOS Beta, Windows/Linux/common RC, human usability or public V1 from this Windows process pilot.
- Redesigning transport telemetry or adding a separate approval database without a demonstrated migration requirement and approved scope.

## Technical Plan

### 1. OpenDash EGM authority adaptation v1 — owner approved

**Decision provenance:** On 2026-11-20, Atlas proposed separating the official EGM procedure from project decision authority. The proposal included the matrix below, no repeated approval for unchanged transcriptions, evidence-based coordinator resolution of technical findings, independent product/release gates, and explicit disclosure of the upstream adaptation. The owner replied “mình đồng ý”. This approves that policy, not every subsequent design detail, runtime change or release.

| Decision or action | Responsible authority | Required boundary/evidence |
| --- | --- | --- |
| Product scope, user-facing product choices and major priorities | Owner | Record the actual scope decision; coordinators cannot approve an expansion merely by relabeling it technical. |
| Architecture, API/schema, technical design sections, decomposition, dependencies and file ownership inside approved product scope | Atlas/Aster as coordinators | Record the decision, scope and evidence in the owning design/Issue/PR. Preserve existing task owners and handoff requirements. |
| Implementation details inside the approved contract | Task implementer | Respect exact file ownership, source/base, dependencies and task acceptance. Escalate a contract/scope change rather than silently implementing it. |
| Independent design criticism | Fresh-context Goldfish reviewers | Read the actual personas/standards/target and report completion and isolation limits; criticism alone cannot authorize implementation. |
| Technical findings, section acceptance and design readiness | Authorized coordinator | Resolve, contest or escalate using evidence, preserving actual findings and review provenance. Do not fabricate human approval or a reviewer result. |
| Eligible self-review/review and merge | Agent authorized by ADR-0010 and current owner directives | Reviewed head, actual validation, required CI, protection, ownership and acceptance all still apply. Do not add mandatory peer review where it is not required. |
| Public release | Owner | Native/security/data/human/license and release acceptance remain separate; technical approval is not release authorization. |

The coordinator performs the decision function at technical EGM section/resolution checkpoints under this explicit adaptation. Technical sections do not return to the owner only because upstream wording addresses a human author. Decisions already approved and copied without changing meaning retain their source approval; a formatting conversion does not demand a new product decision. New product choices, scope changes and release authorization still return to the owner.

This is **OpenDash EGM authority adaptation v1**, not unmodified upstream EGM. Keep the installed plugin, personas and standards unchanged. Reconcile the project AGENTS, ADR-0010, GitHub workflow, operating standard and active skills with this single policy during the controlled integration. Keep Goldfish critique and approval/implementation/verification states distinct. No person or agent may waive mandatory security/data, CI, native/human or protection gates through this policy.

### 2. One live operational process

EGM supplies ideation, drafting, critique, resolution, small-iteration and documentation-maintenance procedures. GitHub Issues/Project/PRs remain the only live operational state. Do not introduce a second approval database, task queue, watcher or automatic dispatch service.

```text
approved product scope
  → EGM brief/specification + independent critique
  → coordinator technical decision and reviewed source
  → merged source + explicit task admission checks
  → GitHub Project Ready + owned implementation
  → reviewed-head validation / required CI / eligible merge
  → exact-source post-merge and platform acceptance
  → human/product/release gates where applicable
```

Approved design is insufficient for implementation admission. Confirm the immutable merged design source, code baseline, meaningful target files/acceptance, task owner, dependencies and live Ready separately. A task still in review follows its existing ownership/admission evidence; do not send it through a second new-task claim.

### 3. Task identity stays independent of EGM document format

**Coordinator technical selection:** retain assigned project task IDs. Do not add an automatic `EGM-<filename-stem>` namespace just because the source moves to an EGM specification. The issue generator should require the explicit assigned Task ID for an EGM implementation task and keep it unchanged through payload, Issue, RUN, STATUS, RESUME, ACK, prompt and receipt.

Existing task IDs, Issue numbers, source pins and receipt/lock hash inputs must be preserved byte-for-byte. A document move, filename change or new source SHA never creates a new delivery identity or permission to resend. Keep the current root-lock and append-only resume-generation protections. Source-pin changes require the recorded handoff/contract procedure and cannot evade a changed-source refusal by choosing a fresh ID or UUID.

Continue active legacy tasks under their pinned accepted contract and identity. A formerly Ready legacy task still needs current dependency/ownership/Ready verification before pickup. New tasks use EGM design sources with explicit assigned IDs. Preserving old accepted sources/receipts is operational continuity, not requiring experimental CMS theme-format compatibility.

Before changing the generator/transport contract, enumerate the IDs actually in use and test producer-to-consumer consistency. Do not silently normalize decimal milestone IDs or assume the current adapter supports every ID accepted by the older parser. An unsupported assigned ID must block delivery with an actionable diagnostic until any support change is designed and tested; it must not be renamed or bypass an old lock.

### 4. Approval, execution and evidence remain separate

| Claim | Evidence that can establish it | What it does not establish |
| --- | --- | --- |
| Design decision approved | Actual authority, approved scope/revision and decision/review provenance | Live Ready, permission to change another owner's file or runtime success |
| Project Ready | Verified source/base, dependencies, owner/slot and current Project state | Delivery, implementation completion or verification |
| Transport accepted/sent/turn ended | Private transport receipt with its stated observation limits | Task claim, correct implementation, PR merge or acceptance |
| Implementation merged | Eligible merged PR with reviewed-head checks and merge SHA | Exact merged-source acceptance or cross-platform PASS |
| Verified scope | Actual commands/fixtures, candidate/platform, outcome and limits in evidence | Other platform coverage, human dogfood or release authorization |
| Release authorized | Owner authorization after applicable release gates | Permission to discard historical failure receipts or unresolved holds |

Use explicit outcome and scope fields in prose/catalog records; Markdown bold, italics or backticks must not determine whether evidence is required. Recognize positive/negative statuses semantically: Verified, Unverified, NOT_RUN, FAIL and partial results are distinct. A requirements catalog is not blanket evidence that V1 is implemented.

Keep provenance in existing committed design/decision records and Issue/PR evidence. Automated scripts validate structure and consistency; a string `Approved` or a working link is not an authority token. The detailed plan must specify exact metadata and negative fixtures before implementing parser changes.

### 5. Document conversion preserves obligations

Build a bounded source-to-successor mapping before applying Nova's moves or compressed rewrites. Preserve the actual product requirements and technical contracts, not only their filenames. Keep immutable historical pointers usable and ongoing tasks attached to their approved source. Register the existing Builder/theme EGM transcription during integration without overwriting Nova's sitemap or dropping the original decision provenance.

Use one documented design inventory for discoverability and CodeGraph input, or enforce synchronization if the final implementation retains separate inventories. Index actual eligible design content, not only the sitemap. Draft versus approved inclusion and removal behavior must be stated rather than assumed. The exact inventory mechanism is reserved for the remaining design detail.

### 6. Verification classes and bounded pilot

Classify checks by changed behavior: prose/links and catalogs; generator/validator behavior; transport identity/locks/resume; CMS runtime; native candidate acceptance; human/release. Required CI remains mandatory regardless of local class. Workflow/script changes need meaningful compatibility and negative tests; do not use a prose-only classification to avoid them or run a Rust release suite only to certify an unrelated Markdown edit.

The pilot must actually exercise official EGM artifacts/persona/standards reads, fresh reviewers, a useful finding and its resolution, explicit approval provenance, offline Issue rendering and mocked producer-to-consumer dispatch/status/resume behavior. Record measured start/end, host/plugin version, actual reports, commands and limits. Do not perform a live dispatch merely to make the pilot look complete. Keep native macOS Beta, the blocked retest, F08/F09 follow-up, human dogfood and common RC/V1 separate.

Stop new-format admission if required capabilities, ownership, source approval or compatibility fail. Reconcile existing tasks under their pinned contract and preserve private receipts, failed reservations and history. Rollback cannot delete locks or turn ambiguous delivery into a new send. Applying a Git revert alone does not undo Issues or private transport state.

### 7. Integration ownership and remaining design work

Atlas owns this design preparation and the owner-approved Builder document conversion. Nova remains read-only without a handoff. Integrate any eligible changes only through an Atlas-owned reviewed branch/PR with the necessary source, file ownership and task admission evidence; do not create an isolation branch to bypass one-active-task or Ready gates. Preserve the user's global Git identity.

Before an executable plan: specify the exact provenance/section contract and fixtures; semantic status/evidence handling; the document inventory/index mechanism; the current-task/legacy admission mapping; the exact files and dependency sequence for preserving and repairing Nova's candidate; pilot acceptance and rollback receipts. This draft does not authorize those source changes or mark the whole migration Approved.

---

Drafted with official EGM `egm-draft` under the owner-approved OpenDash authority adaptation. Overview & Goals were approved by the owner on 2026-11-20. Authority adaptation v1 was approved in the subsequent “mình đồng ý” reply. The technical ID selection is Atlas's decision within that delegation, not a claimed owner selection. Technical Plan has been drafted; Alternatives and Detailed Implementation are still pending. The separate five-stage specification review has not run. No production implementation, Project Ready, integration merge or release is claimed.

## Owner-authorized Builder document conversion — follow-up 2026-11-20

The owner subsequently instructed: commit the Builder/theme decisions first, then transfer them into an EGM specification. This supersedes the earlier dirty-file preservation boundary only for that explicitly authorized documentation task. Atlas committed exactly the eight Builder documents at `4a334f0224866108002e8586b04f5643da0798c9`, then transcribed their settled decisions into [the Builder/theme EGM feature specification](../../../docs/designs/BUILDER_THEME_CONTENT.md) at `fccada1`. The spec preserves provenance, full theme structure, resolver matrix and a source crosswalk; it remains Proposed with unresolved API/schema and implementation readiness. The source documents remain intact. No runtime ownership, Nova branch edit, Project Ready, new Goldfish verdict or release approval is implied. This follow-up does not complete the separate process-migration review or detailed implementation gates.
