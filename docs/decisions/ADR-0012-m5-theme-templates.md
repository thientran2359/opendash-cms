# ADR-0012 — M5 Theme & Templates: Atlas G2 decision record

**Agent:** Atlas (Codex Windows) & Nova (Antigravity Windows) · **Date:** 2026-11-13.
**Decision status:** **G2 AGREED** (all blocked sub-contracts `C-SEC`, `route-context binding`, `A-DURABILITY`, `G-RECOVERY` resolved in [M5 Theme contract](../contracts/M5_THEME_TEMPLATES.md)); **G3 remains DRAFT / NOT RELEASED** pending PR #135 update & merge.
**Authority:** [ADR-0010](ADR-0010-agent-project-and-merge-authority.md), [PRD](../product/PRODUCT_REQUIREMENTS.md), owner request to review [PR #134](https://github.com/thientran2359/opendash-cms/pull/134).
**Recorded Final Decision:** [Atlas turn 2 on PR #134](https://github.com/thientran2359/opendash-cms/pull/134#issuecomment-6839120123).
This record requires independent Nova review and merge. It does not claim M5 IMPLEMENTED, VERIFIED, G2 AGREED or G3 READY.

## Sources and baseline

- Reviewed research: PR #134 head `a09173a28910f3d3e7372d43999f18e37fabe579`, authored by Forge.
- Reviewed G3: PR #135 head `11d3fa326b1b23ddd63814b8811e425b58728bc4`, 13 task drafts for P01–P10.
- Current reviewed main: `067ebacd884bcd630e3de68fc22cd90dfae1bac6`.
- Forge research baseline: `2e6b5c53db10fd23a069ff795cc845cc1fd40f6b`. Product diff to current main is empty for `cms/admin cms/crates cms/migrations cms/Cargo.toml cms/Cargo.lock themes plugins`; only M4 runner/evidence additions intervened.
- **M4 release handoff:** `a176414bba5b7b85f408137ef0e0b964c2329843`, [Atlas final pin / #110](https://github.com/thientran2359/opendash-cms/issues/110#issuecomment-6838596973). Source equality and two native binary hashes are recorded in the [Mac G5](../verification/2026-11-12-m4-s10-t01-g5-macos.md) and [Windows G5](../verification/2026-11-12-m4-s10-t02-g5-windows.md). Later evidence commits are not replacement release source.
- Supporting existing contracts: [ADR-0007](ADR-0007-m4-builder-taxonomy-foundation.md), [ADR-0009](ADR-0009-m4-core-element-field-binding.md), [ADR-0011](ADR-0011-m4-builder-editor-responsive-list.md), [Core Foundation](../contracts/CORE_FOUNDATION.md).
- The exact selected behavior, limitations, acceptance and stop conditions are in [M5 Theme contract](../contracts/M5_THEME_TEMPLATES.md).

## Decision matrix

| Owner question / Forge areas | Atlas decision | Trade-off / remaining gate |
| --- | --- | --- |
| 1. Package / revision / ZIP v2 / one pointer (A) | Stable package identity, immutable content-addressed revisions, one CAS active selection. Legacy packages map one-to-one by full block ID, never by namespace. Canonical validated revision files; original ZIP only provenance. Preserve v1 adapter and old DB rows. | More storage and migration work than extending a block row; enables update/rollback without overwriting layouts. ZIP quotas are conservative design limits, not benchmark results. C-SEC and A-DURABILITY still block activation/installer contract release. |
| 2. URL namespace / taxonomy / slug (B) | Keep M3 content routes; add category/tag taxonomy routes. Central typed RequestContext and URL builders; slug changes yield 404 at old URL in M5, no redirect history. | Stable IDs preserve menu references; redirects/custom permalinks remain M7. Keep existing archive paging limits, separate from M4 list paging limits. |
| 3. Roles / overrides / MiniJinja / CSP (C/H) | Select role resolver and site-owned AST overrides; core composes header/body/footer. No arbitrary user template source. Select required safe-subset restrictions, **do not approve an enforcement implementation**. | **BLOCKED C-SEC:** no parser/validator strategy or adversarial evidence for HTML/URL/CSS and autoescape bypass. Nova independent review required before this contract can release implementation. |
| 4. Theme Options / SITE-01 menu (D/F) | Extend SITE-01 with typed theme values, retained dormant values, explicit public exposure and media refs. Site-owned menu/item IDs, typed entity references and theme slot mapping. | General Settings is a concrete existing feature, not an existing generic options engine. S04 must build the extension; S09 owns menu storage. M7 adds full menu CRUD UI. |
| 5. CNT declarations / withdrawn / taxonomy (E) | Persist same-owner schema through canonical CNT storage; declared/withdrawn is independent of provider availability. Preserve editor access and reject incompatible updates. Taxonomy binding only category/tag; terms site-owned. | Inactive/withdrawn keys stay reserved. Generic activate_owner cannot revive withdrawn declarations. S10 must add transaction-aware APIs; they do not exist yet. |
| 6. Import provenance / atomicity / recovery (G) | Read-only versioned plan, logical provenance independent of revision, all selected DB objects committed together after verified file preparation. No overwrite and no implicit publish. | **BLOCKED G-RECOVERY:** recovery state machine, cross-OS durable publication protocol and concurrent file ownership proof are absent. SQLite transaction plus rename is insufficient evidence. Nova reviews before S05 contract release. |
| 7. M4 handoff / ownership / fixture (H) | Pin M4 release above; implementation baselines rechecked at claim. Exact one-writer seams and forward-only migrations. S08 has one accountable implementer; distinct Windows/macOS evidence artifacts on one final source SHA. | Contract/PR merge does not grant task Ready or prove native G5. Same OS test report cannot certify another OS. |

## Substantive findings and independent review

**C-SEC (P1, release-blocking design gap):** `theme.rs::environment` uses MiniJinja `Environment::new`, HTML autoescape, Strict undefined and fuel 10,000. The locally installed pinned MiniJinja 3.0.5 source confirms default `safe/escape/e` filters; it also offers `Environment::empty`, removal of filters and recursion limits. None of these alone validates URL/CSS/HTML context or disallows template autoescape directives. Current CSP permits inline CSS. A filter blacklist or CSP alone cannot establish the proposed safe subset. Required remediation/evidence: grammar and enforceable HTML/CSS/context policy, exact pinned API/dependency choice, role/legacy compatibility plan, quotas and a negative corpus covering safe/e aliases, autoescape false, event handlers, URL encodings, CSS URL/exfiltration, SVG/foreign markup, dynamic includes, recursion and output amplification. **Nova must independently review this proposal; it is not pre-approved to pass.**

**G-RECOVERY (P1, release-blocking data gap):** `media.rs::write_once` fsyncs one temporary file and renames it; `MediaStore::persist` separately writes original and thumbnail; `storage/media.rs::save` owns a separate transaction. There is no multi-object import journal or transaction-aware CNT/menu/options executor. Required remediation/evidence: durable journal state table, exact commit/visibility point, retry identity across revisions, content-addressed shared-file ownership and reference/lock checks, Windows/macOS crash-cutpoint results and handling of missing/corrupt original or thumbnail after restart. Do not reinterpret process-kill tests as power-loss guarantees. [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html) describes database atomicity, not an application transaction spanning uploads.

**A-DURABILITY (P1, release-blocking data gap):** canonical revision files and DB metadata/pointer also span filesystem and SQLite. S01 needs a durable staging/publication protocol, concurrent install ownership, recovery and a gate preventing selection of incomplete revisions. Reuse the resolved durability principles from G-RECOVERY, without creating an S01-to-S05 implementation dependency cycle.

These are actionable objections to areas A/C/G, not evidence of a production vulnerability. Overall G2 stays blocked until Nova records a signed verdict and the author supplies the missing contract. For G2, require state/grammar/DTO definitions, pinned API/OS semantics and a bounded feasibility prototype only where those semantics remain uncertain. Full native implementation crash-cutpoint, browser and security corpus execution belongs to the specified G4/G5 acceptance gates, not a prerequisite to writing G3. A prototype is not M5 VERIFIED. No new implementation Issue or Project transition is authorized.

## Ownership and sequence

1. Atlas owns only this ADR and `docs/contracts/M5_THEME_TEMPLATES.md` on `docs/atlas-m5-g2-decision`; docs-only PR, Nova reviewer, **no self-merge** under this owner request.
2. Forge retains ownership of PR #134 research and PR #135 G3 source. Atlas records findings and exact task deltas there; author fixes/commits/non-force pushes on those branches. Do not edit the author's branch or pretend a handoff comment updated its Git content.
3. Nova reviews C-SEC/G-RECOVERY and this Atlas-authored docs PR. No automated dispatch, watcher, polling, or fictional signed Nova review.
4. After remediation, independent review and merged authoritative ADR/contract, Forge updates all 13 task drafts. G2/G3 gate promotion and Issue publication must follow the owner's stricter order: **G3 merge first, then recorded G2 AGREED / G3 READY and immutable-source Issue release**. A merge request must state which gate text becomes effective on merge; no task Ready before it actually merges.
5. Project Ready/Done stays unchanged. G3 Ready in docs and GitHub Project Ready remain distinct. Dependencies and ownership are rechecked per task.

## G3 correction checklist for PR #135

- Rebaseline source/evidence to current main and the immutable M4 handoff, retaining old research SHA as historical input.
- Keep all 13 tasks and P01–P10 coverage. Resolve capability IDs/primary acceptance in the existing module briefs before publication; proposed IDs are not approved merely by this ADR.
- S01-T01 owns package/pointer storage and legacy migration. S01-T02 owns v2 parser/installer and new guarded package primitives in `src/package.rs`; it must preserve plugin callers. Installation is not activation. Resolve A-DURABILITY before contract release; implementation/native crash tests remain G4/G5.
- S01-T02 must not require S10 declarations to work before S10 exists: initially reject activation of revisions with declarations as unsupported; S10 integrates declarations + selection atomically after S01-T02. Invalid declarations never produce a partially activated revision.
- S02 includes central URL builders and real consumers in `content_render.rs`, `page.rs`, `storage/builder_list.rs` where applicable; sequence those seams before S03/S09 instead of leaving hardcoded canonical URLs behind. No cross-writer overlap.
- S03-T01/T02 must include complete page/header/footer/archive/single Builder context-binding workflow. M4 exact-entry binding is not a current-entry template context and must not bind every single page to a fixed entry. Add explicit typed route-context source or mark this sub-contract BLOCKED; no magical reuse.
- S03-T01 requires C-SEC resolution at install/compile/render; S07 adds regression/hardening, it is not the first security enforcement task.
- S04 owns typed options extension and transactional saved option media reference ACL; default image refs alone must not make imports public. Specify public header/footer effects in browser acceptance.
- S09 defines menu depth/items, public slot rendering, transaction-aware storage API and preservation; menu IDs/order survive theme switch. M7 still owns full menu CRUD UI.
- S10 wrong path `cms/crates/opendash-server/src/core/registry.rs` must be replaced with the actual `cms/crates/opendash-core/src/registry.rs`; include `src/storage/content.rs` and the canonical transaction-aware adapter. No SQL or I/O in core.
- S05-T01 implements stable logical provenance and all preconditions without DB/files mutations; S05-T02 implements the resolved journal/recovery protocol, using existing domains via shared transaction. Mark G-RECOVERY blocked explicitly.
- S06 tests Admin/Editor **entry/content editability** during inactive/withdrawn states; Editor does not gain Theme Options, install/activate, menu mutation or import rights.
- S08-T01 accountable implementer Forge, coordinator Atlas, native Mac verifier Harbor. Separate OS runner/report paths and clear handoffs; no concurrent authors of a shared evidence file. Cover all predecessors and the negative matrix.
- Give exact owned new filenames/API DTOs/migration allocation and call out future APIs as work to build, not available inputs. Existing migrations 0001–0014 are immutable; Atlas serializes allocation after checking main at claim. S01 starts after 0014 only if main still ends there.

## Verification limits

Read-only baseline comparison, actual code/migration inspection, pinned local MiniJinja source inspection and GitHub PR/threads review were performed. CodeGraph queries returned zero indexed nodes, so no graph conclusion was used.

**NOT_RUN:** M5 runtime/build/migration/browser, safe-subset prototype, crash recovery/power-loss experiments, native M5 binary checksums. None exists in this docs-only decision PR. Docs guards must pass separately; they cannot discharge C-SEC/G-RECOVERY.
