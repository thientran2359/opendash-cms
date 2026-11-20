# M7 — Beta findings disposition and remediation G3

## Owner directive — macOS Beta and Harbor, 2026-11-17

Chủ dự án chuyển ưu tiên Usable Beta sang **macOS**. Aster điều phối, chốt
kỹ thuật và tự review; không cần peer review. Aster chỉ tham gia thảo luận
liên-agent khi chủ dự án tạo cuộc thảo luận. Harbor là implementer của toàn bộ
chín task remediation F01–F09 và thực hiện technical retest macOS; một task
active trên `agent/harbor`, không hai writer trên cùng file. Aster chuẩn bị
candidate và phối hợp human dogfood; automation không thay người thử thật.

Chỉ đạo này thay ownership, nền tảng Beta và yêu cầu peer review cũ bên dưới.
Các PR #310–#313 đã merge. G3 vẫn cần immutable source/base, dependencies,
file ownership và Project Ready được xác minh trước pickup. Bằng chứng Windows
cũ giữ nguyên; không suy thành macOS PASS. Beta yêu cầu native macOS B01–B10
và negative regressions trên cùng candidate. Windows/Linux/native common RC và
V1 gates vẫn riêng; không đóng chúng từ Beta PASS. F08 giữ security hold đến
khi có giải thích provenance và kiểm chứng macOS đầy đủ; đối chiếu Windows
vẫn là follow-up bắt buộc trước common RC. F09 cần full release suite macOS
hai lượt và isolated run; receipt Windows thất bại vẫn giữ và phải được đối
chiếu trước common RC, không gọi là Windows defect resolved.

Thứ tự Harbor: M8-S10-T01 → M7-S15-T01 → M5-S11-T01 → M7-S15-T02 →
M7-S15-T03 → M7-S15-T04 → M7-S15-T05 → M6-S09-T01 → M8-S10-T02.
Mọi task sau giữ Backlog đến khi dependencies và slot đủ gate. Aster ghi
self-review/acceptance tại head SHA, checks thực chạy và giới hạn; không waive
security/data, CI, human test hoặc release authorization.


## Shared gate, provenance and verification

Coordinator: Aster; owner-directed #301 handoff, 17/11/2026. Source reviewed:
`84c13959ceeb0537d29451f979eb4fb06ba53c57`. Audit #300/#306 used product
`d1338d631e1421db075db6f024afabee988de99a`; do not relabel that Windows evidence
as tests of this newer tree. #308/#309 add test isolation/macOS evidence, not a
Windows Beta retest. Implementation NOT_STARTED; runtime verification NOT_RUN.

[Audit](../../../../../verification/2026-11-16-usable-beta-windows-audit.md) ·
[Planning](../../../PLANNING_WORKFLOW.md) ·
[Task #301](https://github.com/thientran2359/opendash-cms/issues/301).
G2/G3 metadata records the owner-authorized Aster decision; effectiveness requires
self-review and docs merge, with no mandatory peer review. No Issue publication,
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

## Source contracts and review handoff

[ADR-0016](../../../../../decisions/ADR-0016-admin-navigation-and-single-edit.md) ·
[Single Edit contract](../../../../../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) ·
[M7 contract](../../../../../contracts/M7_SITE_FEATURES.md) ·
[Core Foundation](../../../../../contracts/CORE_FOUNDATION.md).

Separate submitted specifications (Forge review still pending):
[M8 F08/F06, PR #310](https://github.com/thientran2359/opendash-cms/pull/310),
[M5 F02 contract/G3, PR #311](https://github.com/thientran2359/opendash-cms/pull/311),
[M6 F09, PR #312](https://github.com/thientran2359/opendash-cms/pull/312).
No new fix Issue has been published; use immutable merged specs at publication,
never these PR numbers as a substitute for a source SHA or Project Ready.

## T02 decision and complete findings disposition

The required repair set is F01/F02/F03/F04/F05/F08. F06/F07 are required
verification-harness work, not evidence of product restore failure. F09 requires
reproducibility investigation; an isolated pass does not erase the failed suite.
No finding is waived. Missing B01/B03/B06/B09/B10 subcases remain T03 obligations,
not invented product defects. Human dogfood is separate and still required.

| Finding | Disposition / evidence at reviewed source | Responsible task / downstream gate |
| --- | --- | --- |
| F01 | S1 confirmed source mismatch: MenusPanel reads `.items`; listContentEntries returns a bare array | M7-S15-T01 / B02,B04,B05 |
| F02 | S1 confirmed missing v2 revision discovery/UI; install/activate DTOs differ from legacy | M5-S11-T01, separate M5 contract/G3 PR / B04,B05,B06 |
| F03 | S1 data-preservation blocker: App navigateHash has no leave guard; SingleEditWorkspace reloads server record instead of restoring active RAM draft | M7-S15-T02 / B07; preserve 401 quarantine |
| F04 | S2 required fix: composite create inserts requested published state; migration0008 trigger requires draft insertion | M7-S15-T03 / B02,B03; never change the applied trigger |
| F05 | S2 candidate UI observation, source wiring defect confirmed: PanelRegionRenderer omits FieldValueInput.onPickMedia | M7-S15-T04 / B02; runtime reproduction still required |
| F06 | Harness defect confirmed: M8 runner clicks obsolete Builder button | M8-S10-T02 / B08,B10; no product PASS from selector repair |
| F07 | Harness incomplete; root cause unconfirmed, no invented product diagnosis | M7-S15-T05 / B01,B07; bounded failure/cleanup required |
| F08 | S0 candidate/security acceptance blocker retained. Loader permits config false despite READY; importantly restore.rs already writes true for PortableConfig and default branch. Audit/native outcome conflicts with helper intent: investigate actual worker, manifest paths, generated target and provenance before choosing patch | M8-S10-T01 / B08,B09; no Safe Mode waiver |
| F09 | Verification blocker, product severity unassigned. M6 redirect test shares an absolute deadline across requests; cause not established. #308 test isolation/macOS passes do not prove this Windows release failure resolved | M6-S09-T01 / B10 |

Cross-milestone specifications are separate docs PRs under the coordinator ownership
decision recorded on #301. They must merge with any necessary contract delta before
fix Issue publication. This three-file T02 PR cannot grant itself M5/M6/M8 source
ownership. Links to their submitted PRs are recorded on #301 and in this PR body.

## Delivery order and T03 contract

Aster prepares M8-S10-T01 first; no source task is claimed by this planning action.
Forge serializes M7 UI tasks and M5 theme work, after the current agent slot is free.
Aster serializes backend M7-S15-T03, M8 recovery and M6 test work. M7-S15-T02 precedes
T04 because both own SingleEditWorkspace/PanelRegionRenderer; M7-S15-T05 runs after
all affected editor fixes. M8 harness reruns after recovery and editor fixes. M6
investigation can be independent once its owner is free. No two writers on shared
App.tsx, entries.rs, editor files or harness.

T03 (#302) stays Backlog until T02 specifications/required contract delta merged,
all nine dispositions have linked fix/verification Issue receipts and required
repairs/harness work have G5, plus an independent non-coding human is available.
Aster pins ONE integrated candidate, matching binary/Admin/package hashes and impact
matrix. Forge runs fresh Windows B01–B10 including full negative coverage and all
previously missing subcases, plus targeted regressions below. F04 safe workaround
is historical evidence, not a replacement for repairing the 500. F05 no-repro closure
requires reviewer-approved runtime evidence that the required picker journey works.
F07/F09 can close with demonstrated no-product-change diagnosis and repeatable tests;
they cannot be silently deferred. F08 security regression requires native Windows
and macOS confirmation; Linux release gates stay separate. Changed candidate means
affected native rerun and fresh end-to-end journey. #303/#304 cannot PASS while a
mandatory result is FAIL/NOT_RUN or an S0/S1/security/data issue is unresolved.

## Design choices

Use bounded existing API adaptation for F01, shared RAM draft restoration plus leave
protection for F03, draft INSERT then requested publication UPDATE in the SAME
transaction for F04, canonical MediaPicker wiring for F05. Do not normalize arrays
by weakening the server contract, autosave discarded edits, remove publication
triggers or introduce a parallel media path. F07 repairs test lifecycle only.

### M7-S15-T01 — Menu target picker matches content array contract

- **Thuộc:** V1 internal Beta regression · M7 · M7-S15
- **Module:** SITE-01
- **Capability:** SITE-01.3
- **Primary Acceptance Slice của capability:** M5-S09 / M7-S02; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Existing API adaptation only; no schema or permission delta.
- **G3:** READY
- **Trạng thái:** PLANNED; owner-authorized self-review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** harbor / Aster; coordinator Aster

#### Tổng quan và giá trị

F01: MenusPanel.tsx loadMenusAndRefs casts the content response to an items envelope; api/content.ts declares ContentEntryRecord[].

#### Mục tiêu và hành vi quan sát được

Admin selects published Page/Post/CPT targets, saves a navigation menu and sees matching public links after restart.

#### Phạm vi công việc

Correct typed list consumption and bounded pagination/load-more without silently dropping other types. Keep menu model and stable IDs.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

This specification self-reviewed and merged; immutable source, ownership and Project Ready verified. No task occupies the responsible agent branch.

#### Kỹ thuật và file ownership

- `cms/admin/src/features/menus/MenusPanel.tsx`
- `cms/scripts/m7_beta_menu_picker_e2e.py`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Use existing listContentEntries, menu/slot APIs and ApiClientError; preserve auth, CSRF, CAS and draft-public filtering. Scope the runner to this repair.

#### Nghiệm thu

- [ ] Fresh UI-created Page/Post/CPT targets can be selected with zero iterable errors; empty and multi-page results work.
- [ ] Save/reopen/restart retains stable target IDs; public links match published targets.
- [ ] 401/403 and stale 412 preserve unsaved menu edits; forbidden/draft targets never become public by bypass.

#### Kiểm thử, rủi ro và rollback

npm run build --prefix cms/admin; run the new runner against a pinned native macOS binary and isolated site, then affected existing m7_browser_e2e.py. Capture exact actual invocation in receipt.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.

### M7-S15-T02 — Preserve single-editor drafts during navigation

- **Thuộc:** V1 internal Beta regression · M7 · M7-S15
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Restore ADR-0016 RAM draft/leave semantics; no new persistence or API.
- **G3:** READY
- **Trạng thái:** PLANNED; owner-authorized self-review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** harbor / Aster; coordinator Aster

#### Tổng quan và giá trị

F03: App.tsx navigateHash changes the hash unconditionally; active editor draft is not rehydrated on remount.

#### Mục tiêu và hành vi quan sát được

Editing Post/Page/CPT then sidebar/back/deep-link navigation never silently discards data; cancellation keeps route/form and return restores authorized draft.

#### Phạm vi công việc

One coherent leave strategy across sidebar, editor links, hashchange and browser history; restore full active draft and all editing fields; keep beforeunload protection.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

This specification self-reviewed and merged; immutable source, ownership and Project Ready verified. No task occupies the responsible agent branch.

#### Kỹ thuật và file ownership

- `cms/admin/src/App.tsx`
- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/features/editor/EditorDraftStore.ts`
- `cms/scripts/m7_beta_draft_navigation_e2e.py`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Use existing account/site/session_epoch provenance and quarantine. Restore active draft only after current effective config/ACL validation; scrub fields that became hidden/read-only. Preserve title/slug/status/terms/route/SEO/clears and schema/version; no localStorage, implicit Save or automatic publish. Same entry across CPTs and multiple new draft UUIDs must not collide.

#### Nghiệm thu

- [ ] Cancel/Stay, navigate-return and hard reload warning preserve expected draft/public boundaries for each editor type.
- [ ] Dirty route/SEO/status/terms without value changes also count; successful save updates baseline and clears only saved snapshot.
- [ ] 401 hides/quarantines; same-user role downgrade scrubs inaccessible fields before render; logout/other-user/site clears drafts; stale responses cannot restore old account data.
- [ ] 412/schema conflicts retain authorized edits; repeated back/forward does not trap navigation or lose hash state.

#### Kiểm thử, rủi ro và rollback

Build Admin; new focused native macOS browser runner plus existing admin_nav_single_edit_e2e.py after T05. Run 1440 and 390 viewport, two sessions and delayed responses. No content write occurs when merely navigating.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.

### M7-S15-T03 — Atomic published Post/Page creation respects draft trigger

- **Thuộc:** V1 internal Beta regression · M7 · M7-S15
- **Module:** CNT-04
- **Capability:** CNT-04.3
- **Primary Acceptance Slice của capability:** M7-S11; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Repair internal transaction sequencing under existing composite save contract; no migration/API delta.
- **G3:** READY
- **Trạng thái:** PLANNED; owner-authorized self-review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** harbor / Aster; coordinator Aster

#### Tổng quan và giá trị

F04: storage/entries.rs commit_composite_entry_save binds published on INSERT, conflicting with content_entries_core_insert_draft in migration0008.

#### Mục tiêu và hành vi quan sát được

New Page/Post explicitly published through Admin succeeds atomically; draft create stays private; failed route/SEO validation leaves no partial entry.

#### Phạm vi công việc

Preserve insertion guard: INSERT draft for post/page then apply explicitly requested published state with UPDATE within same existing transaction. Preserve initial version/hook ordering.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

This specification self-reviewed and merged; immutable source, ownership and Project Ready verified. No task occupies the responsible agent branch.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/tests/m7_s15_t03_publication_create.rs`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Use existing begin_entry_write, schema fence, merged validation, ACL, media/term/route/SEO checks. No SQLite write transaction across Wasm/network; before hook before tx, after hook only after commit through existing orchestrator. Do not edit migration0008 or publish ordinary draft Save. Keep non-post/page behavior.

#### Nghiệm thu

- [ ] Draft and explicitly published create work for Page/Post with valid fields; resulting version is the expected single create version.
- [ ] Direct SQL published INSERT still rejected by applied trigger; no guard relaxation.
- [ ] Duplicate slug, route/SEO conflict, invalid media/schema/permissions and veto rollback entry/terms/SEO/route; no after-save success on rollback.
- [ ] Admin/Editor permitted paths and 401/403/CSRF/Origin negatives remain correct; published URL/archive/sitemap/media vs draft boundary proven.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m7_s15_t03_publication_create; existing m7_s11_t01_composite_save_and_profile; Admin build; native macOS UI fresh published create in T03. Security/privacy negatives required, not only DB success.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.

### M7-S15-T04 — Wire image and gallery picker into single editor

- **Thuộc:** V1 internal Beta regression · M7 · M7-S15
- **Module:** CNT-05
- **Capability:** CNT-05.1
- **Primary Acceptance Slice của capability:** M3 Media Library / M7-S12; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Reuse existing media UI/API and ACL; no schema delta.
- **G3:** READY
- **Trạng thái:** PLANNED; owner-authorized self-review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** harbor / Aster; coordinator Aster

#### Tổng quan và giá trị

F05 observed twice; static PanelRegionRenderer passes no onPickMedia although FieldValueInput renders buttons using it.

#### Mục tiêu và hành vi quan sát được

Editor opens image/gallery dialog, chooses authorized uploaded media, cancels safely or saves IDs and sees thumbnails after reopen.

#### Phạm vi công việc

Thread a selected-field picker action through renderer/workspace; use canonical MediaPicker; preserve multiple selection and order.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

M7-S15-T02 merged/G5 and editor file ownership released; then ordinary shared pickup gates.

#### Kỹ thuật và file ownership

- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/features/editor/PanelRegionRenderer.tsx`
- `cms/scripts/m7_beta_media_picker_e2e.py`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Reuse features/media/MediaPicker.tsx and existing media client. Bind target field identity so a late picker result cannot populate another field/entry/account. No picker for noneditable fields; cancel retains draft; image single/gallery ordered multiple; current draft is source of truth.

#### Nghiệm thu

- [ ] Reproduce source wiring gap on baseline, then native macOS image/gallery open/select/cancel/save/reopen succeeds.
- [ ] Readonly/hidden provider fields cannot receive picker writes; account switch or route change drops late selection.
- [ ] 401/quarantine, upload errors and invalid media IDs handled without silent draft loss; save updates public only per publication status.

#### Kiểm thử, rủi ro và rollback

npm run build --prefix cms/admin; new focused browser runner and media regression against isolated native binary. T03 confirms complete B02, not just dialog visibility.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.

### M7-S15-T05 — Bound and diagnose Admin navigation browser runner

- **Thuộc:** V1 internal Beta regression · M7 · M7-S15
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Harness-only diagnosis, unchanged product assertions.
- **G3:** READY
- **Trạng thái:** PLANNED; owner-authorized self-review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** harbor / Aster; coordinator Aster

#### Tổng quan và giá trị

F07 ran over 18 minutes with partial screenshots and no completion; root cause is not yet proven.

#### Mục tiêu và hành vi quan sát được

Tracked runner exits with PASS only after all assertions, or bounded nonzero with phase/diagnostic and owned-process cleanup.

#### Phạm vi công việc

Reproduce hang, log stages, bound network/navigation/shutdown; preserve actual DOM/role/draft/security assertions.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

M7-S15-T01/T02/T03/T04 and M5-S11-T01 merged/G5 where their journeys are exercised; otherwise record explicit blocked phase, never PASS.

#### Kỹ thuật và file ownership

- `cms/scripts/admin_nav_single_edit_e2e.py`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Use explicit expected readiness instead of unbounded networkidle where appropriate; bounded waits and finally cleanup only processes created by runner. Never return PASS after watchdog kill; keep screenshots sanitized, assert binary/site provenance.

#### Nghiệm thu

- [ ] A normal run completes all existing assertions with explicit exit 0; intentional missing selector/server stop yields nonzero within configured watchdog.
- [ ] No orphan owned server/browser on failure; unrelated processes and test roots retained when requested.
- [ ] No removed security/role/ACL assertions or preseeded success; phases pinpoint initial hang.

#### Kiểm thử, rủi ro và rollback

Run tracked runner on native macOS current pinned candidate, once normal and once induced test-only failure; python3 -m py_compile cms/scripts/admin_nav_single_edit_e2e.py. T03 reruns complete B01/B07.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.
