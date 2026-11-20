# M7 — Owner Admin review follow-ups G3

Coordinator/decision: Aster. Implementer: Harbor, serial on agent/harbor.
Source inspected: `fc965b45a8ad734e2b01f41622d86db07946163c`. G2 AGREED effective only with merged
[contract](../../../../../contracts/ADMIN_REVIEW_2026_11.md) and
[ADR-0018](../../../../../decisions/ADR-0018-owner-admin-review.md).
G3 specifications complete; PLANNED is not Project Ready. No runtime evidence claimed.

## Shared pickup and delivery rules

Read the entire contract plus this task section, not just an Issue excerpt. The
owner accepted notes01–07 and delegated technical choices to Aster. Aster self-review
is sufficient; no peer discussion/review required. GitHub Issue/Project remain live
state. One task/file writer at a time across all four milestone documents; order:
M7-S16-T01 → T02 → T03 → T04 → T05 → T06 → M6-S10-T01 → M4-S11-T01 → T02 → M5-S12-T01.
Dependencies mean merged/G5 in their task scope, not merely closed Issue. Re-fetch
source/base, free branch and Project Ready before pickup. No dispatch from this plan.

Canonical Base APIs: id/registry/HookDispatcher/package/auth/ApiError; SQL in storage.
No pinned dependency upgrades. New migrations0035/0036/0037 reserved by T03/extensions/
Section respectively; check main at pickup, collision requires coordinator update,
never alter an applied migration. Exact files below are exclusive per task; ask Aster
for explicit expansion before writing another shared integration file. New tests,
fixtures and evidence are owned by their task, no private data/secrets in Git.

Each task: reproduce baseline → add meaningful failing regression → implement minimal
contract delta → verify negative and positive paths → commit with Agent: Harbor,
non-force push, PR/self-review exact head, required CI, merge only eligible. Use
native macOS isolated copy; never write the owner's port18964 root. Every browser
runner named below must expose --binary and --artifact-dir, bounded lifetime and
PASS/FAIL receipt, no omitted assertions/retry-until-green. Angle-bracket command
arguments in prose mean replace with actual pinned private paths in execution receipt.
Common checks: npm run build --prefix cms/admin for UI; cargo build --manifest-path
cms/Cargo.toml --locked for Rust; Python py_compile for new runners; relevant existing
regressions. Docs changes require check-plans, --self-test, agent-artifacts, diff check.
Record source/tree/binary/Admin/fixture hashes, OS/architecture, commands/exits and
screenshots of observed behavior. CI/static checks are not native G5 or human review.

Rollback preserves original DB and matched binary: forward schema repair or consistent
pre-upgrade snapshot, never infer SQL rollback from Git revert. Verify old→new copies,
integrity/FK, restart, theme/provider changes, backup/restore when storage changes.
Keep failed receipts and unrun cases explicit. #302 human acceptance, #303/#304 Beta,
Windows/Linux/common RC and release gates remain separate; do not auto-close them.

### M7-S16-T01 — Make image and video upload independently usable

- **Thuộc:** V1 Usable Beta macOS · M7 · M7-S16
- **Module:** CNT-05
- **Capability:** CNT-05.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Existing upload APIs and CSS/accessible control repair only; no permission/schema delta.
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner note07: image upload is obscured; CSS puts both upload labels in the same grid cell. Backend image/video paths already exist.

#### Mục tiêu và hành vi quan sát được

On Media library/upload, both image and video upload controls are visible, clickable and keyboard operable at desktop and 390px, and upload into the existing canonical library.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

No product predecessor; merged docs/G3 and free Harbor branch required.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/admin/src/features/media/MediaLibraryPage.tsx`
- `cms/admin/src/features/media/media-library.css`
- `cms/admin/src/shell/MediaRoute.tsx`
- `cms/scripts/m7_s16_t01_media_upload_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Reproduce overlap on baseline; upload JPEG, PNG and WebP through visible image control, plus supported MP4 through separate control; reopen/restart shows correct kinds.
- [ ] 1440/768/390px: control bounding boxes do not overlap, pointer and Tab/Enter or Space both work; picker cancel harmless and selected file can be chosen again.
- [ ] Wrong type/oversize and 401/CSRF fail visibly without ghost media; preserve current quota, sanitization, private media and progress behavior.

#### Kiểm thử, rủi ro và rollback

python3 cms/scripts/m7_s16_t01_media_upload_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.

### M7-S16-T02 — Group Admin navigation and isolate destination screens

- **Thuộc:** V1 Usable Beta macOS · M7 · M7-S16
- **Module:** FND-05
- **Capability:** FND-05.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner notes01/06: different appearance/tools/settings routes currently share aggregate React content; grouping alone would leave incorrect screens.

#### Mục tiêu và hành vi quan sát được

Render Nội dung/Giao diện/Tùy chỉnh groups and exactly one feature screen per destination, with canonical/legacy route behavior from the contract.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M7-S16-T01.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/admin/src/App.tsx`
- `cms/admin/src/shell/AdminShell.tsx`
- `cms/admin/src/shell/router.ts`
- `cms/admin/src/features/themes/ThemesPanel.tsx`
- `cms/admin/src/features/plugins/PluginsPanel.tsx`
- `cms/admin/src/features/users/UsersPanel.tsx`
- `cms/admin/src/styles.css`
- `cms/scripts/m7_s16_t02_navigation_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Visit every old and new destination by click, deep link, reload and Back/Forward; breadcrumb/active item/content agree; menu route has no theme install/options UI.
- [ ] Theme options separated from theme list/import and menu editor; tools/health, reconcile, backup and settings general/SEO/contact/SMTP show only their intended feature; tabs have labels and keyboard navigation.
- [ ] Admin/Editor visibility and direct-route denial; dirty editor cancellation, 401 reauth, same-site draft restoration and account switch remain safe.
- [ ] Section/library links only become enabled when their dependent features land; truthful unavailable state before then, never route to homepage or fake a functional screen.

#### Kiểm thử, rủi ro và rollback

python3 cms/scripts/m7_s16_t02_navigation_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.

### M7-S16-T03 — Fix publication actions and render taxonomy once in editor

- **Thuộc:** V1 Usable Beta macOS · M7 · M7-S16
- **Module:** CNT-04
- **Capability:** CNT-04.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner notes03/05: editor renders an external term picker after region panels and includes Page regardless of eligibility.

#### Mục tiêu và hành vi quan sát được

One configured taxonomy panel, hierarchical categories and tag chips; draft Save and Publish distinct, published Update plus canonical View link; Page excludes taxonomy by core default without deleting historical links.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M7-S16-T02.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/features/editor/PanelRegionRenderer.tsx`
- `cms/admin/src/features/editor/EditorDraftStore.ts`
- `cms/admin/src/features/taxonomy/TermPicker.tsx`
- `cms/admin/src/features/content/types.ts`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/migrations/0035_page_taxonomy_default.sql`
- `cms/crates/opendash-server/tests/m7_s16_t03_editor_publication.rs`
- `cms/scripts/m7_s16_t03_editor_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Post new/draft: Lưu nháp stays private after restart; Đăng bài explicitly publishes atomically. Published Cập nhật changes public; View uses current custom/canonical URL in separate tab with safe rel.
- [ ] Exactly one taxonomy panel; categories show hierarchy and tags selected chips; save/reopen preserves term IDs, role limits and missing/inactive-provider handling.
- [ ] Old DB upgrade: default Page panel hidden but existing terms survive title/SEO/content Save; explicit non-core Page eligibility preserved and rendered. Test clean core defaults and user/provider grants separately.
- [ ] 401→downgrade→reauth hides forbidden fields; 412 keeps draft/terms; publication never occurs on navigation/preview. Existing composite hooks and SQL draft INSERT guard retained.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m7_s16_t03_editor_publication; python3 cms/scripts/m7_s16_t03_editor_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.

### M7-S16-T04 — Separate category and tag lists with dedicated term editors

- **Thuộc:** V1 Usable Beta macOS · M7 · M7-S16
- **Module:** CNT-03
- **Capability:** CNT-03.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner note04: router has category/tag paths but component owns its own kind and inline edit. Dedicated route state must drive the actual screen.

#### Mục tiêu và hành vi quan sát được

Each taxonomy kind has a separate list and full term edit screen, stable term ID route, core fields and future contribution slots.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M7-S16-T03.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/admin/src/App.tsx`
- `cms/admin/src/shell/router.ts`
- `cms/admin/src/features/taxonomy/TaxonomyPage.tsx`
- `cms/admin/src/features/taxonomy/TermEditPage.tsx`
- `cms/admin/src/api/taxonomy.ts`
- `cms/crates/opendash-server/src/routes/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/crates/opendash-server/src/app.rs`
- `cms/crates/opendash-server/tests/m7_s16_t04_term_routes.rs`
- `cms/scripts/m7_s16_t04_taxonomy_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Categories and tags click/reload/back keep fixed kind; Edit opens ID route and cancel returns to correct list without modifying saved term.
- [ ] Create/rename/reparent/delete preserve links and IDs; cycles/depth8/wrong-kind/unknown term rejected; stale ETag412 and forbidden CSRF write denied.
- [ ] Unsaved term form prompts on route/history/refresh; 401 quarantines, role/account change scrubs, no accidental term creation on return. Editor permissions match existing taxonomy policy.
- [ ] Extension slots render no fake fields; later provider integration can mount without another route rewrite.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m7_s16_t04_term_routes; python3 cms/scripts/m7_s16_t04_taxonomy_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.

### M7-S16-T05 — Add bounded permission-aware entry table query

- **Thuộc:** V1 Usable Beta macOS · M7 · M7-S16
- **Module:** CNT-04
- **Capability:** CNT-04.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner note02 needs filtering all rows, not only first 50. Existing bare-array list is relied on by menu picker and other code.

#### Mục tiêu và hành vi quan sát được

Add contract entry-table endpoint with full-result search/filter/sort/count and canonical public URLs without breaking legacy entry lists.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M7-S16-T04.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/routes/entries.rs`
- `cms/crates/opendash-server/src/storage/entry_table.rs`
- `cms/crates/opendash-server/src/storage/mod.rs`
- `cms/crates/opendash-server/src/app.rs`
- `cms/admin/src/api/content.ts`
- `cms/admin/src/features/content/types.ts`
- `cms/crates/opendash-server/tests/m7_s16_t05_entry_table.rs`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] More than100 entries: exact total/status counts, q literal percent/underscore, status/month/category/tag AND+descendants before paging; stable ties and no duplicates across pages on fixed snapshot.
- [ ] Bounds, invalid sort/query/duplicate scalars return400; ACL omits unreadable fields and inaccessible rows/counts. Draft public_url null; custom route public_url accurate.
- [ ] Legacy GET entries remains bare array and unchanged behavior; existing menu picker and content filters regressions pass; SQL only in storage.
- [ ] Read snapshot guarantees internal rows/count consistency; concurrent writes yield fresh subsequent snapshot, not a claim of cursor isolation across independent requests.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m7_s16_t05_entry_table

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.

### M7-S16-T06 — Build useful Posts Pages and CPT list tables

- **Thuộc:** V1 Usable Beta macOS · M7 · M7-S16
- **Module:** FND-05
- **Capability:** FND-05.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner note02: current list only shows title/slug/status and Edit; needs recognizable table workflows.

#### Mục tiêu và hành vi quan sát được

Use the new query for semantic tables, search/filters/counts/sorting/paging, row actions and bounded versioned bulk operations.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M7-S16-T05.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/features/content/EntryListPage.tsx`
- `cms/admin/src/api/content.ts`
- `cms/admin/src/shell/router.ts`
- `cms/admin/src/App.tsx`
- `cms/scripts/m7_s16_t06_entry_table_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Search/status/category/tag/month/page-size/sort update server query and URL; edit/back/reload restores filters; reset offset on filter change; no first-page-only search.
- [ ] Tables work desktop390px keyboard/screen-reader headings; loading/empty/error/retry meaningful. No fabricated author/comments/trash/quick-edit controls.
- [ ] Bulk publish/draft/delete uses existing CAS per row, confirmation for delete and publication; mixed success/412/403 shows actual outcomes and retains failures; controls prevent duplicate submits.
- [ ] Draft rows lack public View, published links canonical and safe; stale responses cannot repaint another CPT/account. Provider contribution slots are core-controlled until next task.

#### Kiểm thử, rủi ro và rollback

python3 cms/scripts/m7_s16_t06_entry_table_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.
