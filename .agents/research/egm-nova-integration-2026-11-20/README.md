# Nova EGM migration compared with PR #413

Date: 2026-11-20. Integrator: Aster. Tracking: [#418](https://github.com/thientran2359/opendash-cms/issues/418).
Verdict: **NOT SUITABLE FOR MERGE AS-IS.** Open a Draft integration PR for the
actual Nova code and these findings. Do not activate the candidate on main or
claim the complete migration is approved. The requested comparison is complete
at the bounded scope below; remediation remains separate review work.

## Requested order and immutable sources

1. Owner explicitly requested reverting #415 before pulling Nova, comparing
   against #413 and creating a separate integration PR.
2. [Revert PR #419](https://github.com/thientran2359/opendash-cms/pull/419) merged
   at `84a838be72735e4a9281459501fae941a0fcdd57`, reviewed revert head
   `4bc575b889fccb086ff64ea7dde48de407a31db1`. Required Ubuntu/Windows governance
   checks passed. All 19 reverted paths match the pre-#415/#413 baseline bytes.
3. Only after that merge, Aster merged Nova source
   `e1a239126829a64c40df1ed86605dadb9ca2320c` into `agent/aster` with its three
   original commits/authorship retained. No Nova branch was changed.
4. Reference [#413](https://github.com/thientran2359/opendash-cms/pull/413):
   `d897c66cddf39a46d901f24335c25b4823afcb73`, especially the
   [migration design](../egm-migration-2026-11-20/design_draft.md), approved
   authority adaptation and preserved Builder/theme decisions.

The #413 migration design remains a draft for unresolved details; its presence
on main does not approve all Nova behavior. #417 remains historical pilot
evidence from before the #415 revert, not an active workflow.

## What is actually imported

| Nova commit | Imported work |
| --- | --- |
| `ffafed0` | EGM entry documentation, design sitemap, README bootstrap, archive reorganization and documentation validators/indexer |
| `3c9cf8a` | EGM-aware pickup/review/dispatch skills and the project context skill |
| `e1a2391` | Direct EGM design parsing and offline Issue generation, retaining a legacy parser |

The [candidate file inventory](candidate-files.txt) is the complete name/status
list from Nova's common baseline `ff957054d10077b5adc0cf1b704457319ba7a6c4`.
It covers 228 changed files. Review is bounded: complete key script/skill reads,
targeted contract comparisons and the recorded read inventories, not exhaustive
semantic certification of every historical document.

## Compatibility assessment

| #413 requirement | Nova candidate | Disposition before main merge |
| --- | --- | --- |
| Official EGM skills for design/review; GitHub as live ledger | Direction matches; direct EGM generator branch exists | Retain actual code, subject to admission repairs |
| Owner product/release authority; Atlas/Aster technical section/findings authority | Entry docs require owner technical approvals and broaden coordinator roles | Amend entry docs/skills consistently to the approved adaptation |
| Preserve Builder/theme decisions and publication boundaries | Nova predates the four #413 files and its PRD summary conflicts with record publication | Preserve #413 files and PRD during merge; reconcile remaining summaries/contracts |
| Stable assigned task IDs and receipt identity | Generator defaults to EGM-filename; unchanged adapter rejects it | Blocking producer/consumer contract fix, no receipt/lock renaming |
| Meaningful complete task handoff and approval provenance | Empty/example-only sections and Approved-only metadata are accepted | Require actual plan/ownership/dependency/acceptance bodies and decision/review provenance |
| Formatting-independent evidence status | Plain/backtick Verified bypass; bold Unverified falsely triggers | Semantic status validation with focused negative coverage |
| Active legacy task continuity | Source reader supports old/archived paths, but skills retire legacy admission | Explicit supported legacy route with unchanged source/ID/receipt semantics |
| Authoritative contracts and historical evidence retained | Several normative contracts compressed; all plans/protocol moved | Retain lost normative obligations and resolve archive policy before acceptance |
| Active skill inventory consistent | Discussion responder archived, operating standard still invokes it | Choose and implement one consistent active/retired disposition |
| Index actual eligible design content | Indexer lists design README/SITEMAP but not feature specs | Define inventory eligibility and index the actual specifications |
| Bounded real migration pilot | Parser tests exist but miss the reproduced failures | Later corrected producer-to-consumer offline/mocked pilot; no live dispatch to claim success |

## Conflict resolutions already applied

`docs/product/PRODUCT_REQUIREMENTS.md` retains the #413 product requirements and
record-publication boundary; `docs/README.md` retains the #413 index plus links
to Nova's design guide/sitemap and this comparison. Relative plan/discussion
links in these two resolved files follow the candidate's archive locations.
The four #413 Builder/product/resolver/ADR/spec files remain present, and the
Proposed Builder spec is registered in the new sitemap. These are preservation
and merge hygiene, not blanket acceptance of Nova's archive or authority policy.

The imported generator, validator, indexer and skill bodies are otherwise kept
as Nova's actual candidate. Known defects remain visible instead of being hidden
behind another documentation-only workflow. The Draft PR must not be merged
until its blocking differences are repaired and reviewed.

## Actual comparison evidence

- [Documentation/authority comparison](docs-comparison.md): fresh-context EGM
  codebase-details reviewer, actual installed persona and readiness standard.
- [Scripts/tooling comparison](scripts-comparison.md): fresh-context EGM
  implementation-readiness reviewer, actual persona/standard and CodeGraph-first
  code inspection. Raw results preserve inventory and limitations.

Both reviewers were requested with `fork_turns: none`; parent-history exclusion
was requested, host instruction/memory exclusion was not established. This is
a bounded comparison, not the full five-stage specification review.

Aster independently ran the actual imported modules in a read-only Python probe:

```text
empty_plan_accepted=True generated_task_id=EGM-probe adapter_accepts=False
'Verified' False
'*Verified*' False
'**Verified**' True
'`Verified`' False
'**Unverified**' True
'NOT_RUN' False
```

These results reproduce material incompatibilities despite green existing tests.

## Checks and limits

On the imported candidate, actually ran:

- `python3 -m unittest discover -s .agents/scripts -p test_prepare_github_issue.py`: 15 PASS.
- `python3 -m unittest discover -s .agents/scripts -p 'test_cos_*.py'`: 45 PASS.
- `python3 cms/scripts/check-plans.py --self-test`: 8 fixtures PASS.
- `python3 .agents/scripts/check-agent-artifacts.py`: PASS.
- `node cms/scripts/codegraph-index-docs.mjs --list-sources`: listed sources exist;
  this does not prove feature-spec inclusion or real index execution.

The initial link check exposed the missing Builder sitemap registration and
the pending comparison-file link; both are addressed in this PR's merge hygiene.
Final link/whitespace checks are run before commit. Required CI is attached to
the actual Draft PR head, not predicted by these local checks. No runtime/native,
human, release, private receipt, transport mutation or live dispatch was performed.

## Next concrete work before merge

Repair the scoped generator/admission/status/indexing defects and reconcile the
authority/legacy/contracts/skill policy against #413. Keep one writer and explicit
file ownership for that PR. Add meaningful failing cases for the reproduced
script behaviors, then run the targeted suites and required CI. Record a real
offline producer-to-consumer pilot and the remaining scope limits. Recheck this
matrix at the corrected head before marking the integration merge-eligible.
