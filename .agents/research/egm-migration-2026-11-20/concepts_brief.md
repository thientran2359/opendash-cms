# OpenDash official EGM adoption

* **Author:** Atlas
* **Date:** 2026-11-20
* **State:** Refined concept after seven BRAINSTORM lenses. Not an approved implementation specification.
* **Input:** [Owner-approved working hypothesis](../2026-11-20-egm-migration-concepts.md).
* **Candidate:** `e1a239126829a64c40df1ed86605dadb9ca2320c` on `origin/agent/nova`.
* **Main baseline:** `ff957054d10077b5adc0cf1b704457319ba7a6c4`.

## Overview

Use the installed official EGM plugin to develop ideas, draft specifications, conduct independent critique, maintain documentation and handle small engineering iterations. Replace active G0–G5 vocabulary with explicit design, implementation, verification and release states while retaining every existing acceptance obligation. GitHub remains the live task and integration record. EGM does not create a second task queue or release authority.

The owner already approved this direction, Goldfish readiness, drafting and later eligible implementation and merge by Atlas. That authorization is retained. Newly written design sections and concrete finding resolutions still follow the applicable official EGM checkpoints; no later section is marked approved in advance.

## Analysis

The candidate changes 228 files in three Nova commits. Existing checks pass, yet actual boundary probes reproduce empty implementation subheadings being accepted, generated EGM IDs being rejected by transport, plain Verified escaping evidence checks, and Unverified falsely triggering them. [Host audit](host-audit.md) records exact commands and outputs. These are migration blockers even though the current fixtures pass.

The reviewers converge on three contradictions. First, candidate entry instructions require owner approval for all technical design, while ADR-0010 preserves delegated technical authority. Separate the design procedure from the authority policy and record exactly who approves each decision. Preserve actual upstream human gates until any adaptation is explicitly approved; do not call an adapted workflow unmodified upstream. Second, new document terminology does not justify new persistent task identities. Preserve existing Issue IDs, task IDs, immutable source pins, receipt keys, ambiguous reservations and append-only resume generations. Third, useful verification requires evidence matched to changed behavior, rather than a Rust release run for every prose edit or a link checker being treated as runtime acceptance.

Other findings require meaningful plan bodies, explicit acceptance/dependencies/ownership, approval provenance, and semantic status checks independent of Markdown formatting. Design approval, Project state, delivery, merged implementation, scoped verification and release authorization must be separate claims. A PRD catalog row cannot certify all V1 requirements as implemented. Reorganization must map retained obligations and authoritative source pointers, including CodeGraph indexing of actual design content.

The smallest useful transform is to use official EGM for design and critique while retaining the existing GitHub, ownership, integration and release substrate. A bounded compatibility layer can admit both new EGM sources and ongoing legacy tasks without duplicate identities or receipt stores. This is a migration mechanism, not permanent continued use of G0–G5 for new designs. The final identifier grammar, authority mapping and cutover/rollback details remain Technical Plan decisions.

## Goals vs. Non-Goals

### In-Scope Goals

1. Verify actual plugin discovery, persona/standards access and native reviewer capability on the coordinator host, recording observed isolation limits.
2. Reconcile AGENTS, ADR-0010, GitHub workflow, operating standard and active manual skills around one EGM process with preserved delegated technical authority.
3. Preserve immutable source/base, exact file ownership, dependencies, Project Ready, reviewed-head CI, post-merge evidence, security/data holds, native platform acceptance, human dogfood and owner release authorization.
4. Make Issue generation, RUN, STATUS, RESUME, structured ACKs, prompts, locks and task identifiers compatible end to end. Preserve existing legacy identity and receipts byte-for-byte.
5. Reject empty, example-only or misleading target-file, acceptance and verification sections. Treat Approved metadata as a claim requiring authority and review provenance.
6. Enforce evidence obligations independently of formatting and retain bounded PASS/FAIL/NOT_RUN and candidate/platform scope.
7. Preserve requirements and historical verdicts when moving documents; reconcile active and immutable historical links and actual CodeGraph document content.
8. Complete a bounded real pilot with commands, useful findings, host/version, report locations, measured duration and limitations before broad adoption and merge.

### Out-of-Scope Non-Goals

- CMS runtime features, schema or runtime-data migration.
- Automatic dispatch/watchers, new queues, lock deletion, blind resends or bypass of repository protection.
- Installing or certifying EGM on other agents' machines without host evidence.
- Native macOS Beta, human usability or common RC/V1 acceptance from a Windows documentation/process pilot.
- A new transport telemetry system. Timestamp enrichment, pagination redesign and broader operational diagnostics are optional follow-ups unless needed to establish migration compatibility.

## Alternatives Weighed

These alternatives were evaluated by reviewers; none is attributed to an upstream maintainer or presented as a new owner-approved architectural decision.

| Alternative | Mechanism and trade-off | Disposition |
| --- | --- | --- |
| One hard cutover with wholesale retirement | Moves everything at once but can strand ongoing Issues, source pins and locks and obscure preserved obligations. | Incompatible with the owner's required preservation if it drops legacy continuity. A safe hard cutover would need evidence not yet provided. |
| Staged adoption with bounded legacy admission | Uses EGM for new design work, retains the pinned contracts and identities of ongoing tasks, then retires compatibility under explicit criteria. | Candidate for the Technical Plan; must not become two competing live workflows. |
| Keep stable assigned task IDs for EGM-backed work | Separates document format from identity and minimizes transport changes. | Must be compared in the Technical Plan with introducing a tested EGM namespace. |
| New EGM ID namespace | Can improve naming but requires canonical validation, collision rules and producer-to-consumer compatibility. | No approval inferred from Nova's generated default. Existing identities must not be renamed. |
| Extend existing validators and GitHub evidence | Adds structural checks and explicit provenance to current scripts and records. | Fits the approved scope; exact schema and test contract still require design. |
| Build a separate approval/workflow database | Adds a competing state store and migration burden. | Outside the approved scope; no demonstrated need. |

## Key Decisions & Staging for Next Steps

### Settled owner constraints

Official EGM adoption is the destination. Coordinator technical autonomy, active ownership and Ready/dependency checks remain. Product-scope changes and public release remain owner decisions. Preserve F08 security/provenance obligations, F09 failed Windows receipts/follow-up, same-candidate macOS B01–B10 and negative regressions, human dogfood and separate common RC/V1 requirements. The candidate's macOS retest explicitly records TECHNICAL_BLOCKED and NOT_GRANTED; it must not be promoted by documentation edits.

Preserve all eight dirty Builder files and the user's input concept. Do not edit or push Nova without handoff. Review uses the detached exact-SHA checkout. Implementation requires recorded ownership and an eligible task; this concept does not create Ready status or an Issue.

### New design decisions still open

- Exact approver matrix and wording of any upstream EGM adaptation that preserves coordinator authority.
- Stable assigned task IDs versus a new tested namespace; source renames, multiple tasks per spec and collision behavior.
- Admission rules for continuing active legacy tasks, formerly Ready legacy tasks and entirely new EGM tasks.
- Provenance fields, meaningful-section contract, semantic states and evidence bindings.
- Canonical documentation inventory/CodeGraph synchronization, cutover stop conditions and rollback.

Proceed with official egm-draft. The user already selected drafting; do not ask that selection again. Draft Overview & Goals first and present the actual section for approval. Draft remaining sections only after their corresponding checkpoints, then perform the separate five-stage egm-review and concrete egm-resolve process. Seven brainstorm lenses are not a completed specification review. Implementation and merge follow only when their gates are met.

## Reviewer Completion and Evidence

| Lens | Result | Complete raw report |
| --- | --- | --- |
| Design / economics | Completed | [raw](raw-egm-design.md) |
| Codebase / observability | Completed | [raw](raw-egm-codebase-details.md) |
| Zoom out / minimal complexity | Completed | [raw](raw-egm-zoom-out.md) |
| Comprehension / ergonomics | Completed | [raw](raw-egm-comprehension.md) |
| Implementation readiness / migration | Completed | [raw](raw-egm-implementation-readiness.md) |
| Structural / maintenance | Completed | [raw](raw-egm-structural.md) |
| Inventive catalyst | Completed on recovery dispatch | [raw](raw-egm-inventive-catalyst.md) |

The original synthesizer stopped at a usage limit after six reports were saved. The parent preserved those results, launched only the missing lens in a fresh context and synthesized this brief from the actual reports. No missing lens is counted as clean. Each raw report preserves read attestations, partial/truncated reads and unavailable old-path attempts. Those limits prevent treating this as an exhaustive 228-file audit.

All reviewer launches requested `fork_turns: none`; host instructions and memory remain visible. A separate controlled native probe returned no parent-only marker and no parent assistant/tool history, while confirming AGENTS and injected memory visibility. This supports only the observed parent-history boundary on that probe; it does not prove instruction exclusion or broader isolation. See the [host audit](host-audit.md).

Elapsed-time reporting is incomplete because the first orchestrator was interrupted and no reliable start timestamp was captured. Do not invent a full-pilot duration or token total. Record a new measured start/end for the subsequent design/implementation pilot. Capability discovery and seven completed concept reviews are actual results; a complete EGM drafting/review/implementation pilot is still pending.
