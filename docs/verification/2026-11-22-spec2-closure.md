# Spec 2 post-conformance closure — 2026-11-22

**Audited source:** main `11e3e00a6fde5b7abbaeba79f4ecba1077a3cf4b`. Agent: Aster.

## Design/code drift

[Sequential drift report](../../.agents/research/2026-11-22-spec2-closure/2026-11-22_DRIFT_REPORT.md): no drift detected in the selected [Spec 2](../designs/cpt-content-builder-and-pattern-library.md), including S01–S03 and D1–D3. Read the current source after CodeGraph reported stale source ranges: CPT capability/defaults and mode preservation; shared entry CAS/storage/guards/preview; private Pattern CRUD/depth/limits/1-hop media; Theme catalog/filter/copy/modal; Media sidebar; selected Section/descendant insertion; committed updated_at snapshot. Original D4/D5 documentation reconciliations remain applicable.

## Evidence and source boundaries

- [Issue #451](https://github.com/thientran2359/opendash-cms/issues/451) Closed, Project #3 Done. [PR #453](https://github.com/thientran2359/opendash-cms/pull/453) merged at audited SHA; reviewed head `a5a59a699e946f3631ea2753433feb495f60c297` has all three required CI checks SUCCESS.
- Independently inspected [Harbor native report](2026-11-21-egm-spec2-conformance-d1-d3-macos.md), actual Chrome receipt (9 grouped checks PASS), and actual final Rust log: SHA-256 `4dd9968930117aeac0fbfb09180e313f02d625650281be3d6f0e4be6360f38a3`, 337 passed across 74 suites, zero failures. These are Harbor executions, not new Aster browser executions.
- Main also contains intervening Admin/content changes absent from PR head; the PR head and merge trees are **not identical**. The older browser receipt alone therefore does not establish exact-main integration success.
- Aster reran on audited main: standards/self-test, plans/self-test, artifact checks, Admin build and locked Rust build PASS. Exact-main native Chrome PASS 9/9; receipt source matches audited main, binary SHA-256 `f87820fecf96747c658dff44a180a8264783f69468286bf8449ebbb7c1a597d8`. Actual local receipt: `/private/tmp/aster-spec2-final-7yc_5tno/chrome/receipt.json`. Full native locked release workspace command completed with exit 0: 337 passed, 74 suites, zero failed/ignored. Log SHA-256 `3c677e57ecba61a8e8d33ffc9ae385bb6707b526e8b2270055b4c01a12896ca9`; local log `/private/tmp/aster-spec2-final-7yc_5tno/release.log`. Private canonical TMPDIR and serial test threads were used.

## Acceptance gate

No feature drift remains. Whole-workspace formatter and docs reindex results must be disclosed separately from functional evidence; a missing engine is not successful reindexing. Do not claim every Verification Plan command passed while formatter exits nonzero.

Whole-workspace formatter returned exit 1 on 52 pre-existing Rust files. [Follow-up #454](https://github.com/thientran2359/opendash-cms/issues/454) tracks a formatting-only cleanup; SITEMAP remains Implemented because the approved Verification Plan explicitly requires this command to PASS. No unresolved D1–D3 functional defect is asserted.

Docs reindex: default lookup initially failed because the script selects a Linux engine path on this Mac. Downloaded exact pinned `@astudioplus/codegraph-mcp@0.21.0` into a temporary directory without changing repository/global npm dependencies; package downloader verified the Darwin x64 engine checksum. Reran `node cms/scripts/codegraph-index-docs.mjs` with CODEGRAPH_ENGINE_PATH pointing to that native engine: exit 0, all 28 canonical sources indexed. This is distinct from the other installed CodeGraph v1.6 CLI.

**Closure state:** Implemented; no remaining feature drift, native exact-main build/Rust/Chrome PASS, formatter gate unresolved (#454). No Owner decision is required to perform the mechanical cleanup.
