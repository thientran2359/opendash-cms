# M5 — Beta theme v2 management G2/G3 delta

## Shared gate, provenance and verification

Coordinator: Aster; owner-directed #301 handoff, 17/11/2026. Source reviewed:
`84c13959ceeb0537d29451f979eb4fb06ba53c57`. Audit #300/#306 used product
`d1338d631e1421db075db6f024afabee988de99a`; do not relabel that Windows evidence
as tests of this newer tree. #308/#309 add test isolation/macOS evidence, not a
Windows Beta retest. Implementation NOT_STARTED; runtime verification NOT_RUN.

[Audit](../../../../verification/2026-11-16-usable-beta-windows-audit.md) ·
[Planning](../../../PLANNING_WORKFLOW.md) ·
[Task #301](https://github.com/thientran2359/opendash-cms/issues/301).
G2/G3 metadata below records the coordinator decision submitted for Forge review;
effectiveness requires that review and this docs PR merge. No Issue publication,
Ready, implementation, release or dispatch from an unmerged specification.
Pin merged source SHA and current code base when publishing, check both open/closed
Issues for exact task IDs, verify Project Ready and agent slot before pickup.
One writer per file; proposed responsible agents are not active claims.

Use canonical Base APIs, ApiError and auth/CSRF/Origin guards. SQL stays in storage.
No dependency upgrades, applied-migration edits, security relaxation, fixture seeding
of the primary Beta journey, secrets or private artifacts in Git. Existing ABI,
publication semantics, rollback and provider/data preservation remain mandatory.
Stop-and-Propose before new API/schema/security policy beyond the explicit delta.
Any product fix requires focused negative regressions and affected B01–B10 rerun;
record actual commands/exits, source/tree/binary and fixture hashes, native OS,
roles and screenshot/log hashes. Build and CI are not runtime or human evidence.
Keep previous failed receipts. No retry-until-PASS, skipped assertions or timeout
inflation to hide defects. Source changes require candidate repin and impact matrix.
All tests use isolated site copies, loopback and disposable roots; retain originals
and matched snapshots/binaries. Git revert does not undo migrated data.

## G1/G2 decision

F02 spans UI and server discovery. installTheme and activateTheme assume legacy
ThemeItem/ThemeList; native v2 install already returns revision identity and activation
accepts revision_id, but GET list exposes legacy items only. Do not add a parallel
legacy install or silently activate on upload. Add a sanitized read projection under
the existing Admin GET list, preserve all existing fields. See the contract appendix
in the same PR. Full-Track: this additive wire contract needs Forge review and merge
before this G3 is effective. No migration, package format or ABI change.

### M5-S11-T01 — Expose and activate installed v2 revisions in Admin

- **Thuộc:** V1 internal Beta regression · M5 · M5-S11
- **Module:** THM-01
- **Capability:** THM-01.2
- **Primary Acceptance Slice của capability:** M5-S01; regression only, no new capability
- **Track:** Full-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Full-Track selected for additive revision-discovery DTO.
- **G3:** READY
- **Trạng thái:** PLANNED; conditional on peer review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** forge / Aster; coordinator Aster

#### Tổng quan và giá trị

F02 confirmed against routes/themes.rs, storage/theme.rs and Admin api/themes.ts/ThemesPanel.tsx.

#### Mục tiêu và hành vi quan sát được

ZIP v2 upload shows installed revision, explicit activation works after reload, sample import and template editing are reachable; legacy workflow remains intact.

#### Phạm vi công việc

Implement reviewed sanitized list projection, discriminated install response and explicit revision activation; refresh shared state after success.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

M5 contract appendix peer-reviewed and merged with this G3; M7-S15-T01 for full navigation/import journey; one free Forge slot. Reserve these files exclusively; no concurrent theme writer.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/routes/themes.rs`
- `cms/crates/opendash-server/src/storage/theme.rs`
- `cms/admin/src/api/themes.ts`
- `cms/admin/src/features/themes/ThemesPanel.tsx`
- `cms/crates/opendash-server/tests/m5_s11_t01_revision_discovery.rs`
- `cms/scripts/m5_beta_theme_v2_e2e.py`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Use list_packages/list_revisions/load_selection through storage, never raw SQL in routes. Keep old response fields; add revisions and selection per contract appendix. V2 PUT sends revision_id and If-Match selection.version, legacy sends id. Treat legacy and v2 activation result separately, then GET refreshed summary. No activation on install and no undefined success labels. Preserve data/layout/options, compatibility checks and revision digest gates.

#### Nghiệm thu

- [ ] Valid v2 201 and duplicate 200 show stable theme/revision after reload; count and active badge truthful; legacy install/list/activate regression passes.
- [ ] Admin explicitly activates v2 with correct CAS; stale 412 preserves selection/draft, Editor 403 and unauthenticated 401; invalid/missing digest rejects without mutation.
- [ ] Switch/update/restart preserves authored entries/layout/menu/options; theme IDs do not collide with legacy presentation.
- [ ] User can enter sample dry-run, cancel without writes, explicitly execute into nonempty site without overwrites/duplicates, and reach header/footer/single/archive editing.

#### Kiểm thử, rủi ro và rollback

Admin build; new Rust revision-discovery tests, existing m5_s01_t02_zip_v2 and focused theme/import/preservation tests; native Windows new browser runner. Record Python fixture ZIP hash; PowerShell packaging failure remains unconfirmed, not silently fixed.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.
