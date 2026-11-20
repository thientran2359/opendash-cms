# Documentation integration — Goldfish findings and resolution

Date: 2026-11-20. Responsible coordinator: Aster. Issue: [#414](https://github.com/thientran2359/opendash-cms/issues/414).
Scope: the documentation integration slice and successor guide, not all EGM
migration or the Builder product specification. Base: d897c66cddf39a46d901f24335c25b4823afcb73.

## Method and authority

EGM 0.1.1 Stage 9 only, with three actual native reviewers requested using
`fork_turns: none`: implementation-readiness, codebase-details and style.
Each returned a completed report and a read inventory. No five-stage review,
instruction/memory exclusion, cross-host capability or runtime PASS is claimed.
The coordinator selected the bounded stage and resolves technical findings
under the owner-approved OpenDash authority adaptation. No product decision
or security/release obligation is changed by these resolutions.

Raw findings remain unchanged in [readiness](docs-review-readiness.md),
[consistency](docs-review-consistency.md) and [style](docs-review-style.md).

## Dispositions

All findings are accepted as PROPOSE FIX, including the two small check/link
items. No finding severity is downgraded; duplicate findings share a correction.

| Finding | Original severity | Technical decision and applied correction |
| --- | --- | --- |
| Readiness 1 / Consistency 1: interim admission | Major / Major | EGM owns design; a supported module/milestone task section owns task scope/acceptance using the current template and stable ID. Merge and pin both sources; existing generator and GitHub publication/pickup rules remain. No manual implementation bypass. |
| Readiness 2: terminology mapping | Major | Explicit G0–G5 obligation/authority mapping; retain machine-required fields and source-preserving Ready/resume rules. |
| Readiness 3 / Consistency 4: promotion links | Major / Nit | Successor guide has destination-relative links, active policy wording and next-pilot wording; research source is marked historical with a successor pointer. |
| Readiness 4: skill boundary | Major | All four exact skill bodies are read-only; a per-skill compatibility table records supported behavior and blocking conflicts. No pointer silently overrides tools. |
| Readiness additional Nit / Consistency 3 | Nit / Major | Include the existing agent-artifact checker in local documentation acceptance, with required CI separate. |
| Consistency 2: Full-Track scope | Major | Cross-reference retained risk triggers, including auth/routes/uploads/rendering/sandbox; uncertain impact uses full design. |
| Style 1: primary draft location | Major | Primary drafts stay in research until accepted; promote by PR with one successor. Existing Proposed Builder transcription is a historical exception. |

The correction uses the actual generator source path restriction observed through
CodeGraph: plans/modules, plans/milestones or plans/tasks. The guide chooses the
existing owning module/milestone source, not unsupported docs/designs input.
No script was changed or compatibility behavior inferred from a Markdown label.

## Integration decision

Final Decision: Aster accepts the documentation policy at its bounded scope,
subject to final corrected-diff review, documentation checks and required PR CI.
ADR-0010's owner-directed docs/design route applies; this is not an implementation
pickup or a claim that all migration tooling is Ready. Keep the full migration
draft pending its remaining design and complete specification review.

The guide will become active only through the owning PR merge. The next pilot
requires current design ownership confirmation and resolution of the Builder
specification's open technical questions. No remote agent is dispatched here.

## One bounded corrected-diff re-audit

Three new reviewer contexts completed the single re-audit after correction:
[readiness](docs-review-readiness-recheck.md),
[consistency](docs-review-consistency-recheck.md) and
[style](docs-review-style-recheck.md). All report no material remaining findings
within the documentation integration scope. Their read inventories and limits
remain in the actual reports. No full five-stage migration verdict is inferred.

Local documentation acceptance ran successfully: `python3 cms/scripts/check-plans.py`,
`python3 cms/scripts/check-plans.py --self-test` (12 fixtures),
`python3 .agents/scripts/check-agent-artifacts.py` and `git diff --check`.
The final staged diff is checked again before commit. Required CI is separately
checked on the PR head; these local results do not predict CI or merge.

`node cms/scripts/codegraph-index-docs.mjs` was attempted and exited 1 because
its configured engine was not found on this Mac. The available CodeGraph MCP/CLI
can inspect code but that does not establish successful canonical docs indexing.
No indexer/config/plugin was modified. Agent entry points and relative links
are updated directly; canonical CodeGraph docs refresh remains a tooling follow-up.
Runtime/native/human testing is not part of this docs-only acceptance.
