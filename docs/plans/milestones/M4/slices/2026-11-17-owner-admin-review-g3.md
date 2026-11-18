# M4 — Owner Admin review follow-ups G3

Coordinator/decision: Aster. Implementer: Harbor, serial on agent/harbor.
Source inspected: `fc965b45a8ad734e2b01f41622d86db07946163c`. G2 AGREED effective only with merged
[contract](../../../../contracts/ADMIN_REVIEW_2026_11.md) and
[ADR-0018](../../../../decisions/ADR-0018-owner-admin-review.md).
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

### M4-S11-T01 — Open and save Builder for the exact Page

- **Thuộc:** V1 Usable Beta macOS · M4 · M4-S11
- **Module:** BLD-03
- **Capability:** BLD-03.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner note05: editor sends single template route and Builder ignores scoped route, API calls use null entry scope despite existing backend support.

#### Mục tiêu và hành vi quan sát được

Page edit opens its own body Builder and all operations retain exact entry scope, without changing homepage or other Pages.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M6-S10-T01.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/admin/src/App.tsx`
- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/builder/BuilderWorkspace.tsx`
- `cms/admin/src/api/builder.ts`
- `cms/admin/src/shell/router.ts`
- `cms/crates/opendash-server/src/routes/builder.rs`
- `cms/crates/opendash-server/src/storage/overrides.rs`
- `cms/crates/opendash-server/tests/m4_s11_t01_page_scope.rs`
- `cms/scripts/m4_s11_t01_page_builder_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Two Pages with different content/layout: open Page A direct/from editor, preview/save/reset/reload/restart touches only A; public header/footer composed once. Existing text retained for explicit fallback.
- [ ] Draft Page Builder Save never publishes; authenticated preview no-store; anonymous media/content remain denied until actual publication. Published Save updates correct public canonical URL.
- [ ] Editor allowed exact Page but direct global template/homepage APIs403; wrong type/id404 or400. No request for homepage on scoped initial load; no silent empty-scope save.
- [ ] Switch target while dirty supports cancel/stay and discard; 401 reauth role downgrade and account switch quarantine/scrub correctly; 412 keeps current AST; delayed stale load cannot replace new target.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m4_s11_t01_page_scope; python3 cms/scripts/m4_s11_t01_page_builder_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.

### M4-S11-T02 — Add site-owned private Section CPT with scoped Builder

- **Thuộc:** V1 Usable Beta macOS · M4 · M4-S11
- **Module:** BLD-02
- **Capability:** BLD-02.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner note01 requires a Section CPT built in Builder, not only a disabled navigation label.

#### Mục tiêu và hành vi quan sát được

Create/list/edit/reopen private reusable Section entries, canonical ownership and scoped AST with no accidental public content/media exposure.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M4-S11-T01.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/storage/sections.rs`
- `cms/crates/opendash-server/src/storage/mod.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/storage/media.rs`
- `cms/crates/opendash-server/src/storage/entry_table.rs`
- `cms/crates/opendash-server/src/routes/sections.rs`
- `cms/crates/opendash-server/src/routes/mod.rs`
- `cms/crates/opendash-server/src/app.rs`
- `cms/crates/opendash-server/src/content_render.rs`
- `cms/migrations/0037_section_documents.sql`
- `cms/admin/src/features/sections/SectionsPage.tsx`
- `cms/admin/src/api/sections.ts`
- `cms/admin/src/App.tsx`
- `cms/admin/src/shell/router.ts`
- `cms/admin/src/shell/AdminShell.tsx`
- `cms/admin/src/builder/BuilderWorkspace.tsx`
- `cms/crates/opendash-server/tests/m4_s11_t02_sections.rs`
- `cms/scripts/m4_s11_t02_sections_e2e.py`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] UI-created Section with image/block AST persists after restart/theme switch/plugin disable/re-enable; missing blocks preserved, canonical entry metadata and document CAS stay coherent.
- [ ] Anonymous entry/archive/sitemap/media-through-section denied; Editor can edit scoped Section but not global templates. Invalid body/schema/media ref and CSRF/Origin fail without partial metadata/doc write.
- [ ] Existing user/provider section collision stops upgrade without claiming/deleting data; clean upgrade from0036 preserves old DB; deletion cleans only owned refs and backup/restore retains Section.
- [ ] Navigation Section link functional only once backend and UI integrate; no hidden generic public route or client-set public status escapes private Section policy.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m4_s11_t02_sections; python3 cms/scripts/m4_s11_t02_sections_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.
