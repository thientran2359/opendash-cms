# M6 — Owner Admin review follow-ups G3

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

### M6-S10-T01 — Enable safe provider columns actions and taxonomy panels

- **Thuộc:** V1 Usable Beta macOS · M6 · M6-S10
- **Module:** PLG-04
- **Capability:** PLG-04.1
- **Primary Acceptance Slice của capability:** Existing milestone baseline; this is owner-requested extension/regression, not a second baseline signoff
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; merged source/dependencies and live Project Ready required before implementation
- **Nguồn quyết định:** Owner completed review notes01–07; Aster decision ADR-0018 and shared contract
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Owner notes02/04 explicitly require theme/plugin extension points, not hardcoded tables and term editors.

#### Mục tiêu và hành vi quan sát được

Ship contract admin_extensions on theme/plugin v2, durable owner-aware lifecycle, ACL projections, guarded row actions and atomic term extension Save.

#### Phạm vi công việc

Implement the applicable normative contract section end-to-end, including the exact
interfaces, data limits, lifecycle and acceptance below. Reuse current typed APIs;
new behavior must work through UI and durable storage where specified, not labels only.

#### Ngoài phạm vi

No note08 full Builder redesign, cloud library, arbitrary provider scripts, trash,
scheduled publishing, revisions, synced block propagation, live site edits or release.
No unrelated module refactor; no weakening ACL/CSRF/Origin/media/publication guards.

#### Phụ thuộc và điều kiện chặn

Required predecessor G5: M7-S16-T06.
Immutable merged source/base, own Issue, exact files, Project Ready verified. Security/data blocker stays blocked; ambiguous delivery is not a task claim.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-core/src/admin_extensions.rs`
- `cms/crates/opendash-core/src/lib.rs`
- `cms/crates/opendash-core/src/plugin.rs`
- `cms/crates/opendash-server/src/theme.rs`
- `cms/crates/opendash-server/src/plugin/package.rs`
- `cms/crates/opendash-server/src/plugin/manager.rs`
- `cms/crates/opendash-server/src/storage/plugin.rs`
- `cms/crates/opendash-server/src/storage/theme.rs`
- `cms/crates/opendash-server/src/storage/admin_extensions.rs`
- `cms/crates/opendash-server/src/storage/mod.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/entry_table.rs`
- `cms/crates/opendash-server/src/routes/taxonomy.rs`
- `cms/crates/opendash-server/src/routes/entries.rs`
- `cms/crates/opendash-server/src/routes/plugin.rs`
- `cms/crates/opendash-server/src/app.rs`
- `cms/migrations/0036_admin_extensions.sql`
- `cms/admin/src/features/content/EntryListPage.tsx`
- `cms/admin/src/features/taxonomy/TermEditPage.tsx`
- `cms/admin/src/features/extensions/AdminContributionRenderer.tsx`
- `cms/admin/src/api/content.ts`
- `cms/admin/src/api/taxonomy.ts`
- `cms/crates/opendash-server/tests/m6_s10_t01_admin_extensions.rs`
- `cms/scripts/m6_s10_t01_admin_extensions_e2e.py`
- `cms/fixtures/admin-review/plugin/plugin.json`
- `cms/fixtures/admin-review/theme/theme.json`

#### Cách triển khai theo contract

Read normative ADMIN_REVIEW_2026_11 contract above and existing source. Interfaces and schema choices are fixed there; internal function decomposition belongs to implementer. Canonical DTO/backward compatibility and transaction boundaries are mandatory.

#### Nghiệm thu

- [ ] Real ZIP theme+plugin installs register text/badge/link columns and term panels; plugin action executes on explicit click only; no guest invocation for cells.
- [ ] Negative namespace/collision/schema/cardinality/combined-cap/action role/unsafe URL cases reject before package publication; no partial registry or values updates.
- [ ] Unreadable field absent from config/rows; direct forged action403; CSRF/Origin denied; same-user downgrade scrubs term drafts; current schema_token mismatch409 and CAS412 retain edits.
- [ ] Disable/crash/theme switch withdraws contributions and preserves values; restart/re-enable restores; old clients omitting extensions preserve values; uninstall/upgrade/rollback follow owner contract.
- [ ] Term core+extensions commit all-or-nothing; malformed or denied patch leaves term version and values unchanged; backup/restore and old DB forward upgrade preserve data.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m6_s10_t01_admin_extensions; python3 cms/scripts/m6_s10_t01_admin_extensions_e2e.py --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR

Run common checks plus focused existing affected regressions; replace capitalized path tokens with actual isolated paths and record exact command. Preserve baseline FAIL and verify restart/provider/data boundaries. Shared rollback and native receipt rules apply.

#### Bàn giao và review

Provide bounded diff, before/after screenshots, exact tested source/head/base and artifacts, commands/exits, acceptance matrix and limits. Self-review current head and required CI; merge is not automatic VERIFIED. Update live Issue/Project only with actual evidence; no automatic Beta/parent closure.
