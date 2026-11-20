# OpenDash EGM operating guide

Historical drafting snapshot. The successor is
[docs/plans/EGM_WORKFLOW.md](../../../docs/plans/EGM_WORKFLOW.md), effective on
its owning integration PR merge. Update the successor, not this snapshot.

Author: Aster. Date: 2026-11-20.

State: Draft for controlled integration. The owner approved the direction in
the current Aster chat: one entry guide, work-sized workflows, delegated
authority, one primary design, stable task identities and bounded reviews.
This file makes that direction concrete; it does not replace active project
instructions before integration and merge. The upstream plugin stays unchanged.

## Start here

Read the current product requirements and the owning design, then choose the
smallest workflow that can resolve the actual open decisions. Resume an existing
task from its Issue and pinned source rather than starting its design again.
GitHub Issues/Project own task state; PRs own integration and review records.

| Work | Procedure | Required output |
| --- | --- | --- |
| New product behavior or a shared architecture/contract | EGM brainstorm when the problem is unsettled; draft; independent specification review; resolve | One primary design with decisions, remaining questions and implementable tasks |
| Bounded change inside an accepted contract | EGM lite with the applicable project authority adaptation | Scoped checklist, file ownership, change and review evidence |
| Wording, links or faithful transcription with no changed meaning | Direct documentation edit under the existing task | Focused diff, source attribution and relevant documentation checks |
| Documentation/code consistency question | Bounded EGM drift report with an explicit document set | Differences and source references; no automatic feature work |

Do not restart brainstorm for settled requirements. If a small change reveals
new product behavior, a shared schema decision or cross-task ownership conflict,
stop expansion and have the coordinator determine the necessary design work.
The Full-Track risk criteria in the [Planning Workflow](../../../docs/plans/PLANNING_WORKFLOW.md#chọn-full-track-hoặc-fast-track)
remain mandatory for new work: schema, public API/ABI, HTTP/auth/permission,
ZIP/sandbox, upload/public rendering and other listed boundaries. Uncertain
impact uses the full design route. Lite never lowers these acceptance criteria.
An upstream skill's checkpoints still apply except where the explicit authority
adaptation assigns the decision to the coordinator. This guide does not silently
remove skill stages or certify a same-context review as Goldfish review.

## Decision authority

| Role | Responsibility |
| --- | --- |
| Owner | Product goals, user-facing choices, major priorities and public release |
| Atlas or Aster, as the named coordinator | Technical decisions within approved scope, design acceptance, task decomposition, dependencies and file ownership |
| Named implementer | Implementation choices within the accepted contract and owned files |
| Goldfish reviewer | Independent criticism of the supplied artifact; no task assignment, product approval or write ownership |
| Authorized PR reviewer/merger | Exact-head integration under ADR-0010 and the current owner directives |

Name one responsible coordinator for each design or task. Atlas and Aster do not
issue competing decisions for the same task; a change of coordinator is recorded
in its Issue/PR. Existing Harbor ownership and one-active-task rules remain.
Goldfish is a temporary review role, not a new persistent project agent or branch.

The coordinator records technical section decisions and finding dispositions
under the owner-approved OpenDash authority adaptation. Ask the owner only when
the decision changes product scope/priorities or requires owner authority.
Preserve actual upstream checkpoint wording/provenance; never invent human approval.

## One primary design and supporting records

Use these existing locations; avoid a second hierarchy organized by task status:

| Location | Purpose |
| --- | --- |
| `docs/product/` | Authoritative product requirements and confirmed behavior |
| `docs/designs/` | Accepted primary EGM designs; acceptance and its scope are explicit |
| `docs/contracts/` | Shared technical interfaces and invariants owned by their existing contract |
| `docs/decisions/` | Durable cross-cutting decisions and their rationale |
| `.agents/research/<topic>/` | Brief, raw Goldfish reports and working drafts |
| `docs/plans/` | Roadmap, module relationships and task/source discovery |
| `docs/verification/` | Actual execution evidence, separately scoped |

Before acceptance, keep the primary draft in `.agents/research/<topic>/`.
On acceptance, promote that document through its owning PR to `docs/designs/`,
rebase relative links and replace the working draft with a successor pointer
or a clearly marked historical snapshot. Never maintain two editable primaries.
The existing Proposed Builder transcription in `docs/designs/` is retained as
an explicit historical exception; its location does not grant acceptance.

Each topic names its primary design and author/coordinator. Supporting reports
link back to that design. Resolve findings into the primary design and record
the disposition; readers should not have to reconstruct a decision from raw
reviews. A draft's location in `docs/` is not proof of acceptance.

Keep the product requirement in its authoritative source; the design references
it and specifies implementation behavior. Shared contracts retain one owner and
one definition. If a decision supersedes text elsewhere, update that text or
mark it superseded with a successor link. Do not copy competing definitions.

For new designs, the docs index should identify the primary file, status,
coordinator and owning Issue when present. Keep that index as a discovery aid,
not a second live task ledger. During the pilot, use `docs/README.md`; defer a
separate sitemap unless a concrete navigation need justifies it.

## Minimum design and handoff

A primary design states problem, goals/non-goals, important user journeys,
technical decisions, alternatives, open questions and task decomposition.
Implementation tasks must include the following concrete handoff:

```text
Issue / stable Task ID:
Primary design and immutable accepted source SHA:
Code baseline SHA:
Responsible coordinator / implementer:
Dependencies and their acceptance evidence:
Files to create / modify / delete:
Expected behavior and acceptance:
Checks appropriate to the change:
Out of scope and stop conditions:
Current PR / reviewed head / remaining actions, if resuming:
```

File ownership belongs to a task and time window, not an entire agent forever.
The receiving agent confirms current branch/slot, source, dependencies and live
Project Ready before new implementation. Resume uses the existing task identity,
ownership and PR; it does not manufacture a second task. Handoff is recorded on
the Issue/PR. Chat delivery supplements that record and does not establish Done.

### Interim admission using existing tooling

The EGM document owns design decisions. Until tooling migration is separately
accepted, place its task handoff in the owning existing module or milestone
source under `docs/plans/modules/` or `docs/plans/milestones/`, using the
[SLICE_TASK](../../../docs/plans/templates/SLICE_TASK.md) headings and metadata.
This handoff owns task scope/files/acceptance and links to the primary design;
it does not repeat an independently editable architecture. Any conflict blocks
publication until both sources agree in a reviewed PR.

Assign an unused stable `Mx-Sxx-Txx` ID from the actual milestone breakdown;
do not derive IDs from filenames. Retain `G2: AGREED` for full design and
`G3: READY` only when the accepted design and complete handoff have merged.
Keep the existing track, module/capability and eight-section task contract.
Pin the merged source commit containing the handoff, the accepted design
revision and the separate code baseline. Use the current read-only Issue
generator on that supported plans path, then publish its reviewed payload
through the GitHub Connector/API under the existing workflow. No new manual
implementation bypass or generator input at `docs/designs/` is introduced.

Live Ready, dependencies, slot and file ownership remain separate pickup checks.
Unsupported IDs/actors/source formats block the affected tool or dispatch route;
do not rename tasks, impersonate another agent or delete locks to get past it.
Normal on-demand pickup of an eligible Issue remains available; dispatch is
optional and needs its own supported contract. Design/coordination Issues such
as this guide's integration use ADR-0010's Final Decision documentation route,
not a fabricated runtime implementation Ready claim.

## Review without repeated reconstruction

Use the selected installed skill's actual review procedure. For a full design,
the official multi-stage review remains applicable; do not claim it completed
after a single general review. For a bounded task use the lite procedure.
Do not run the full design procedure for a faithful wording edit.

Supply the reviewer the target revision, necessary contracts and a precise
question. Use fresh conversation context with actual personas/standards;
record host limitations separately. Do not pass the author's private reasoning
as an answer the reviewer should repeat. Goldfish reviewers are read-only.

The coordinator consolidates duplicate findings, records fix/contest/defer with
reasons and updates the primary document. Follow the selected skill's re-review
rules; focus subsequent review on changed sections and affected invariants
where that procedure permits. Keep unresolved decisions visible. Do not restart
all prior design stages merely because a new chat takes over.

## States and continuity

Track design acceptance, implementation merge, scoped verification and release
authorization separately. An accepted design is not Project Ready; a merged PR
is not proof of runtime behavior. Checks match the change: prose checks for
prose, focused behavior checks for changed scripts/code, existing mandatory CI
and security/data/release requirements where applicable. No full runtime suite
is required merely to validate an unrelated wording edit.

Use explicit descriptions for new work instead of forcing agents to decode
G0–G5 labels. Retain historical labels and their source obligations. Existing
task IDs, Issue numbers, pins, receipt keys and locks remain stable. Do not
mass-convert ongoing tasks or automatically admit an incompatible new format.
The generator/adapter migration needs its own accepted technical plan.

| Retained term | Description for new work | Authority and interim fields |
| --- | --- | --- |
| G0 | Confirmed product scope | Product requirements and owner decisions |
| G1 | Current-source research and alternatives | EGM brief/research and actual source references |
| G2 | Accepted technical design | Primary design plus coordinator decision; retain task `G2` metadata |
| G3 | Complete accepted task handoff | Supported plans task section; retain `G3`, track and existing headings |
| G4 | Implemented and reviewed change | Issue/PR and exact reviewed head; merge remains separate |
| G5 | Scoped post-merge acceptance | Actual merged-source evidence; required before Project Done |

The new descriptions are the reading path; retained terms remain a compatibility
contract, not a second sequence of approvals. Existing Ready tasks keep their
accepted source and require current pickup checks. Active/resuming tasks keep
their identity, dirty task-owned changes and In progress/In review state;
never force them back to Ready for a document conversion.

## Pilot and adoption decision

First integrate this guide with project entry instructions and authority
documents through an owned documentation PR. Then use the existing Builder/theme
design as the proposed pilot, subject to current ownership confirmation. Its
settled product decisions remain settled; resolve its open technical questions
and produce a small dependency-ordered handoff before implementing product code.

Record whether a receiving agent can locate the authoritative requirement,
identify decisions versus open questions and begin its bounded task without
reconstructing chat history. Record duplicated documents, repeated questions,
review time and owner interruptions when actually observed. These are pilot
observations, not a new monitoring service or a requirement to run extra tests.

Expand adoption after the coordinator records the useful results and remaining
friction. Keep runtime acceptance and release gates in their existing scope.

### Compatibility disposition for the four project skills

| Skill body (read-only in this slice) | Disposition |
| --- | --- |
| `.agents/skills/github-task-pickup/SKILL.md` | Compatible unchanged through the supported handoff and existing pickup/resume gates |
| `.agents/skills/discussion-turn-responder/SKILL.md` | Compatible unchanged; replies remain on demand and cannot grant implementation authority |
| `.agents/skills/github-pr-review-merge/SKILL.md` | Compatible unchanged; docs Final Decision and implementation acceptance remain separate |
| `.agents/skills/atlas-task-dispatch/SKILL.md` | Compatible unchanged only for its already-supported Issue/actor/ID routes; no new-format dispatch enabled |

Any later contradictory skill requirement blocks the affected route until an
owned correction is accepted. A guide pointer cannot override tool validation.

## Related sources

- [Migration design](design_draft.md)
- [Concepts and prior review evidence](concepts_brief.md)
- [Project authority](../../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md)
- [GitHub workflow](../../GITHUB_WORKFLOW.md)
- [Operating standard](../../AGENT_OPERATING_STANDARD.md)
- [Builder/theme pilot candidate](../../../docs/designs/BUILDER_THEME_CONTENT.md)
