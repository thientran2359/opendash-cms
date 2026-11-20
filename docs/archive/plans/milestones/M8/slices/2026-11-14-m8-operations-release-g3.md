# M8 — G3 Operations & Release V1 (18 task / 8 candidate)

> **Staging addendum 16/11/2026:** [ADR-0017](../../../../../decisions/ADR-0017-usable-beta-staging.md)
> và [Beta G3](../../M7/slices/2026-11-16-usable-beta-g3.md) thêm checkpoint Windows.
> Giữ nguyên 18 task, dependencies, native/common RC, security/data/release acceptance.
> Source thay đổi cần impact/repin/rerun. Beta không đóng #248 hoặc deferred milestones.
> Proposed owners/status bên dưới là snapshot 14/11; assignment live #242/#245 hiện
> là Forge, #188 vẫn Harbor. Issue/Project là nguồn pickup; không sửa evidence lịch sử.

**Coordinator / Final Decision:** Atlas · **Date:** 2026-11-14.
**Baseline:** main@8c5b90072c231c1f0813eb593d7be4447fb0a11d (highest migration0031).
**G2:** AGREED · **G3:** READY effective after this design PR merges to main.
**Implementation:** NOT_STARTED · **Runtime/native verification:** NOT_RUN.
[PRD](../../../../../product/PRODUCT_REQUIREMENTS.md) · [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md) · [contract](../../../../../contracts/M8_OPERATIONS_RELEASE.md) · [source G1](../../../../../../.agents/research/2026-11-14-m8-operations-release-g1.md) · [planning workflow](../../../PLANNING_WORKFLOW.md) · [task template](../../../templates/SLICE_TASK.md) · [M8 candidates](../TASK_PLAN.md).

## Full milestone contract and boundaries

Build manual media lifecycle, consistent encrypted backup and safe separate-site restore, native Linux package/runtime, measured security/reliability and full V1 acceptance. Every candidate below has complete specs now, including dependent tasks and unavailable macOS/Linux gates. This planning delivery does not run product tests or publish implementation Issues/Project Ready.
No scheduled backup/GC, external storage provider, generic hot swap, media transcoder, licensing decision by agents, automatic release tag/deployment or security waiver. Owner chooses license; its absence blocks publication, not completion of technical G3.

## Coverage and Primary Acceptance Slice

| Candidate | Module / Capability | Primary acceptance | Tasks |
| --- | --- | --- | --- |
| M8-P01 | OPS-01.1 writer coordination, OPS-01.2 canonical snapshot | M8-S01, M8-S03 | S01-T01/T02, S03-T01 |
| M8-P08 | CNT-05.7 referenced delete, CNT-05.8 reconcile/purge | M8-S02 | S02-T01/T02 |
| M8-P02 | OPS-01.3 archive codec, OPS-01.4 Admin backup | M8-S04 | S04-T01/T02 |
| M8-P03 | OPS-01.5 verified new-root restore, OPS-01.6 Admin/operator activation | M8-S05 | S05-T01/T02 |
| M8-P04 | OPS-02.1 Linux runtime, OPS-02.2 executable distribution | M8-S06 | S06-T01/T02 |
| M8-P05 | OPS-02.3 native Linux security/performance | M8-S07 | S07-T01 |
| M8-P06 | OPS-01.7 full V1 recovery, OPS-02.4 native Linux full acceptance | M8-S08 | S08-T01/T02/T03/T04 |
| M8-P07 | OPS-02.5 runbooks, OPS-02.6 release readiness audit | M8-S09 | S09-T01/T02 |

Native S08-T02/T04 regress OPS-01.7, not new duplicate primary capabilities. Existing CNT-05.4 backup integration is exercised by S08, not renumbered. Candidate IDs unchanged.

## Sequence, ownership and release gates

Safe sequence: S01-T01→S01-T02→S02-T01→S02-T02→S03→S04-T01→S04-T02→S05-T01→S05-T02→S06-T01→S06-T02→S07→S08-T01→(T02 Windows / T03 Linux / T04 Mac in parallel)→S09-T02. S09-T01 runbooks can proceed after S06-T02/S07 and before final native acceptance; rehearse current operator steps before final audit.
0032 belongs S01-T01,0033 S02-T01; check highest migration at claim, never modify applied0001–0031. No concurrent writers for shared code/schema; optimize sequence only through coordinator-recorded handoff, not hidden overlaps.
M7#213 Windows evidence plus actual source/ownership allow development while macOS deferred. M5#157/M6#188/M7#214/#215 full native closure remains release dependency. No circular requirement that those already close before M8 native Mac can supply the missing evidence; Atlas reviews them separately before final release readiness.
Proposed owners are not active assignments. Only immutable merged G3+Issue ownership+actual predecessor acceptance+Project Ready authorizes implementation. No queue/cron/dispatch. One active implementation task/agent branch.

Every task adding a writer may update docs/contracts/M8_WRITER_INVENTORY.md with exact source/lease lifetime/test mapping; treat this as a shared coordinator-owned seam.

Shared product wiring allowed ONLY for implementation slices S01–S06 and current slice and coordinator-assigned seam: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Read-only source discoveries outside ownership trigger explicit scope handoff or separate fix Issue, no silent two-writer edits.
All tasks Full-Track. SQL storage-only; Base APIs auth/CSRF/Origin/id/RegistryCatalog/HookDispatcher/package reused. No local/global Git identity changes outside per-worktree instructions; commits retain actual Agent trailer.

## Common test, evidence and rollback contract

For implementation tests, start with a behavior fixture that fails for the missing contract, implement minimal coherent slice, then run actual test and regressions. No DTO-only mirrored tests. From root:
- `cargo build --manifest-path cms/Cargo.toml --locked`
- `cargo test --manifest-path cms/Cargo.toml --test TEST_NAME --locked`
- `cargo test --manifest-path cms/Cargo.toml --workspace --locked` before full acceptance; appropriate focused M5–M7 regressions per slice.
- UI changes: `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, native real-binary browser flows. Locked new dependency only after intentional lockfile resolution; existing pins never upgraded silently.
- Docs/governance: `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test`, `python cms/scripts/check-standards.py`, `python .agents/scripts/check-agent-artifacts.py`, `git diff --check`.
- Once implemented: `python cms/scripts/m8_browser_e2e.py --binary CMS_BINARY --site-root ISOLATED_SITE --artifact-dir OUTSIDE_GIT`; `python cms/scripts/m8_linux_smoke.py --binary CMS_BINARY --site-root ISOLATED_SITE --artifact-dir OUTSIDE_GIT --seed 1`; performance runner same seed/host context per contract.
Use exact real paths/flags/versions in receipts, not placeholders from command descriptions. No runtime secrets/PII/raw private artifacts committed. Finite tasks only, no automatic publishing/deployment.
Fresh+upgrade tests use copy of real pre-M8 schema0031/files and matching old binary. After migration, rollback means complete old encrypted backup/snapshot+binary into NEW root, not git revert/down-migration. SQL integrity/foreignkeys and durable domain IDs/content equality checked; implicit rowids/SQLite layout may change under VACUUM.
Windows-scoped component acceptance may unlock next development task; platform-specific cases remain NOT_RUN until actual native evidence. Security/data failure or required platform case missing blocks that platform's G5, final V1 requires all specified native/runtime/browser/restore/performance/license evidence.
Stop-and-Propose for PRD/new protocol/security/ABI/applied migration changes; Atlas Final Decision and contract/ADR delta required. Internal refinement recorded with Design Delta/real tests. Missing host/tool/library feasibility is a blocker, not reason to claim VERIFIED.

### M8-S01-T01 — Instance leases, operation coordinator và job ledger

- **Thuộc:** V1 · M8 · M8-S01 · M8-P01
- **Module:** OPS-01
- **Capability:** OPS-01.1
- **Primary Acceptance Slice của capability:** M8-S01; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §2](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Instance leases, operation coordinator và job ledger cung cấp phần có thể kiểm chứng của M8-P01; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Canonical root/external DB paths, lifetime OS locks, one-operation state machine, bounded admission/drain/cancel APIs and persistent generation/job transitions; no snapshot claim until S01-T02 covers all writers.

#### Phạm vi công việc

Canonical root/external DB paths, lifetime OS locks, one-operation state machine, bounded admission/drain/cancel APIs and persistent generation/job transitions; no snapshot claim until S01-T02 covers all writers.
Forward migration `cms/migrations/0032_operations_jobs.sql`; kiểm số chưa bị chiếm trước claim.

#### Ngoài phạm vi

Không backup/delete UI hoặc enabled production backup route khi writer coverage chưa xong.

#### Phụ thuộc và điều kiện chặn

M7 #213 Windows acceptance có evidence, immutable G2/G3, không active task/file conflict; macOS deferred không chặn development. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/migrations/0032_operations_jobs.sql`
- `cms/crates/opendash-server/src/operations.rs`
- `cms/crates/opendash-server/src/storage/operations.rs`
- `cms/crates/opendash-server/src/paths.rs`
- `cms/crates/opendash-server/src/main.rs`
- `cms/crates/opendash-server/tests/m8_s01_t01_coordinator.rs`
- `docs/verification/2026-11-14-m8-s01-t01-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

SiteCoordinator::acquire(SitePaths)->Coordinator; admit_mutation()->MutationLease; begin_maintenance(kind,deadline)->ExclusiveOpsContext; generated JobId và CAS job ledger. Shared read leases borrowed by nested services, lock handles never inherited by worker.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Hai processes cùng root hoặc cùng external DB fail busy; root khác+DBkhác hoạt động; lock ownership survives handle cloning/drop as specified.
- [ ] Symlink/reparse/relative root/case aliases không mở duplicate lease; không unlink stale lock để bypass.
- [ ] Exclusive drain<=30s, new writers503RetryAfter3, cancellation waits actual work join; restart jobs interrupted/never auto-run.
- [ ] 0032 fresh/upgrade giữ all031data, CAS concurrent transitions412; max200receipts/oneheavyjob bounded.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s01_t01_coordinator --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Fresh+upgrade0031copy and integrity/FK/domain ID equality; preserve pre-migration DB+all required files+matching binary for new-root rollback.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s01-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S01-T02 — Nối toàn bộ writers và recovery vào maintenance barrier

- **Thuộc:** V1 · M8 · M8-S01 · M8-P01
- **Module:** OPS-01
- **Capability:** OPS-01.1
- **Primary Acceptance Slice của capability:** M8-S01; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §2](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Nối toàn bộ writers và recovery vào maintenance barrier cung cấp phần có thể kiểm chứng của M8-P01; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Wire auth/content/Builder/media/import/theme/plugin/KV/hooks/logs/settings/taxonomy/SEO/contact/mail/secrets/startup paths including writes after HTTP completion. Audit source inventory and freeze/race tests; no SQL in routes/no parallel guard framework.

#### Phạm vi công việc

Wire auth/content/Builder/media/import/theme/plugin/KV/hooks/logs/settings/taxonomy/SEO/contact/mail/secrets/startup paths including writes after HTTP completion. Audit source inventory and freeze/race tests; no SQL in routes/no parallel guard framework.
Không có migration mới trong task.

#### Ngoài phạm vi

Không né gate bằng method-only middleware hoặc cho guest forge lease; no independent snapshot until all writer paths accounted.

#### Phụ thuộc và điều kiện chặn

M8-S01-T01 G5 Windows; coordinator ownership handoff. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/operations.rs`
- `cms/crates/opendash-server/src/auth.rs`
- `cms/crates/opendash-server/src/hooks.rs`
- `cms/crates/opendash-server/src/page.rs`
- `cms/crates/opendash-server/src/contact.rs`
- `cms/crates/opendash-server/src/smtp.rs`
- `cms/crates/opendash-server/src/secrets.rs`
- `cms/crates/opendash-server/src/media.rs`
- `cms/crates/opendash-server/src/plugin/manager.rs`
- `cms/crates/opendash-server/src/plugin/runtime.rs`
- `cms/crates/opendash-server/src/plugin/worker.rs`
- `cms/crates/opendash-server/src/plugin/host_ops.rs`
- `cms/crates/opendash-server/src/storage/auth.rs`
- `cms/crates/opendash-server/src/routes/users.rs`
- `cms/crates/opendash-server/src/storage/plugin.rs`
- `cms/crates/opendash-server/src/storage/plugin_kv.rs`
- `cms/crates/opendash-server/src/storage/import.rs`
- `cms/crates/opendash-server/src/storage/media.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/settings.rs`
- `cms/crates/opendash-server/src/storage/menu.rs`
- `cms/crates/opendash-server/src/storage/seo.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/overrides.rs`
- `cms/crates/opendash-server/src/storage/page.rs`
- `cms/crates/opendash-server/src/storage/theme.rs`
- `cms/crates/opendash-server/src/storage/contact.rs`
- `cms/crates/opendash-server/src/storage/smtp.rs`
- `cms/crates/opendash-server/src/routes/auth.rs`
- `cms/crates/opendash-server/src/routes/builder.rs`
- `cms/crates/opendash-server/src/routes/content.rs`
- `cms/crates/opendash-server/src/routes/entries.rs`
- `cms/crates/opendash-server/src/routes/import.rs`
- `cms/crates/opendash-server/src/routes/media.rs`
- `cms/crates/opendash-server/src/routes/menu.rs`
- `cms/crates/opendash-server/src/routes/plugin.rs`
- `cms/crates/opendash-server/src/routes/seo.rs`
- `cms/crates/opendash-server/src/routes/settings.rs`
- `cms/crates/opendash-server/src/routes/smtp.rs`
- `cms/crates/opendash-server/src/routes/taxonomy.rs`
- `cms/crates/opendash-server/src/routes/themes.rs`
- `cms/crates/opendash-server/src/routes/contact.rs`
- `docs/contracts/M8_WRITER_INVENTORY.md`
- `cms/crates/opendash-server/tests/m8_s01_t02_writer_barrier.rs`
- `docs/verification/2026-11-14-m8-s01-t02-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

SiteCoordinator is injected explicitly through AppState/Extension and service arguments, not a hidden global service locator. MutationLease acquired once by top-level service/dispatch and borrowed down canonical storage/file/network phases; ExclusiveOpsContext only coordinator-created. Writer inventory links every top-level mutator and async/background completion to owning permit.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Pause gate during each writer family: admitted mutation finishes completely or cancel acknowledged before frozen; fresh attempts503 without partial FS/DB.
- [ ] HTTP handler returns while worker/log/SMTP persists: maintenance still waits; no late KV/upload/secret write after frozen.
- [ ] Nested save/hooks do not deadlock/reacquire exclusive; worker manual run/recovery respects leases.
- [ ] All M5–M7 regression suites pass; audited inventory coverage lists source functions, not only route names.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s01_t02_writer_barrier --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s01-t02-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S02-T01 — Media reference inventory và journaled referenced-delete

- **Thuộc:** V1 · M8 · M8-S02 · M8-P08
- **Module:** CNT-05
- **Capability:** CNT-05.7
- **Primary Acceptance Slice của capability:** M8-S02; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §3](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Media reference inventory và journaled referenced-delete cung cấp phần có thể kiểm chứng của M8-P08; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

All saved/private/draft/provider/opaque pin inventory, generation/CAS delete202, original+thumbnail quarantine journal, readers/deleting ACL, retry/reupload rules and Admin delete diagnostics.

#### Phạm vi công việc

All saved/private/draft/provider/opaque pin inventory, generation/CAS delete202, original+thumbnail quarantine journal, readers/deleting ACL, retry/reupload rules and Admin delete diagnostics.
Forward migration `cms/migrations/0033_media_lifecycle.sql`; kiểm số chưa bị chiếm trước claim.

#### Ngoài phạm vi

Không force-remove content refs, Editor delete, automatic purge hoặc xóa file ngoài canonical grammar.

#### Phụ thuộc và điều kiện chặn

M8-S01-T02 G5; exclusive media/ref/storage seam handoff. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/migrations/0033_media_lifecycle.sql`
- `cms/crates/opendash-server/src/storage/media_refs.rs`
- `cms/crates/opendash-server/src/storage/media.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/overrides.rs`
- `cms/crates/opendash-server/src/storage/settings.rs`
- `cms/crates/opendash-server/src/storage/seo.rs`
- `cms/crates/opendash-server/src/media.rs`
- `cms/crates/opendash-server/src/media_lifecycle.rs`
- `cms/crates/opendash-server/src/routes/media.rs`
- `cms/admin/src/api/media.ts`
- `cms/admin/src/features/media/MediaLibraryPage.tsx`
- `cms/crates/opendash-server/tests/m8_s02_t01_media_delete.rs`
- `docs/verification/2026-11-14-m8-s02-t01-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

collect_pins_in_tx(conn,id)->MediaPinSummary; tombstone_media_in_tx(id,expected_generation)->DeleteJob; recover_delete_job(job,maintenance). Media metadata exposes lifecycle version distinct from file SHA ETag; streaming Body holds per-ID read lease.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Draft/inactive/missing block/private options/opaque plugin64hex pins prevent delete409; malformed persistedJSON failclosed.
- [ ] Concurrent ref-vs-delete and dedup/reupload cannot bypass transaction; deleted-ID ABA generation protected, two sequential delete cycles succeed.
- [ ] Crash after each file move/metadata stage recovers idempotently; Win open stream sharing leaves pending safe state.
- [ ] Unauthorized HEAD/Range/304 on deleting asset404; current readers drain before rename, source data/layout untouched.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s02_t01_media_delete --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Fresh+upgrade0031copy and integrity/FK/domain ID equality; preserve pre-migration DB+all required files+matching binary for new-root rollback.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s02-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S02-T02 — Manual reconcile, quarantine purge và Admin diagnostics

- **Thuộc:** V1 · M8 · M8-S02 · M8-P08
- **Module:** CNT-05
- **Capability:** CNT-05.8
- **Primary Acceptance Slice của capability:** M8-S02; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §3](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Manual reconcile, quarantine purge và Admin diagnostics cung cấp phần có thể kiểm chứng của M8-P08; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Finite dry-run/apply, startup deletion recovery, orphan/missing/staging/secret diagnostics, grace periods and safe purge UI, no scheduled tasks.

#### Phạm vi công việc

Finite dry-run/apply, startup deletion recovery, orphan/missing/staging/secret diagnostics, grace periods and safe purge UI, no scheduled tasks.
Không có migration mới trong task.

#### Ngoài phạm vi

Không delete missing DB rows or guess unknown refs, filesystem traversal, recurring cleanup worker.

#### Phụ thuộc và điều kiện chặn

M8-S02-T01 G5; operations/media ownership released. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/media_lifecycle.rs`
- `cms/crates/opendash-server/src/storage/media_refs.rs`
- `cms/crates/opendash-server/src/storage/operations.rs`
- `cms/crates/opendash-server/src/routes/operations.rs`
- `cms/admin/src/api/operations.ts`
- `cms/admin/src/features/operations/ReconcilePanel.tsx`
- `cms/crates/opendash-server/tests/m8_s02_t02_reconcile.rs`
- `docs/verification/2026-11-14-m8-s02-t02-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

plan_reconcile(exclusive)->Plan{id,generation,digest,candidates}; apply_reconcile(plan,expected_version)->JobReceipt; purge only referenced-safe quarantine objects past7days, explicit Admin request.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Plan10000records/20000files ceiling explicit, no partial success; generation/fileidentity change=>412.
- [ ] New/inflight/original-referenced media/secretattempt snapshots never quarantined; orphanage24h and purge7d enforced.
- [ ] Unknown/symlink/unreadable files fail relevant scan with safe diagnostic; missing files remain metadata.
- [ ] Apply/retry/restart/purge idempotent; private secret diagnostics redact payloads/paths; Editor403.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s02_t02_reconcile --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s02-t02-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S03-T01 — Snapshot SQLite và canonical file manifest nhất quán

- **Thuộc:** V1 · M8 · M8-S03 · M8-P01
- **Module:** OPS-01
- **Capability:** OPS-01.2
- **Primary Acceptance Slice của capability:** M8-S03; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §4](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Snapshot SQLite và canonical file manifest nhất quán cung cấp phần có thể kiểm chứng của M8-P01; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Isolated --operations-worker snapshot mode + native512MiB memory limit + kill/wait protocol and frozen managed data copy/VACUUM INTO/bounded hash manifest, external DB rebasing, all required media/package/secrets/config included, safe runtime/backups exclusions.

#### Phạm vi công việc

Isolated --operations-worker snapshot mode + native512MiB memory limit + kill/wait protocol and frozen managed data copy/VACUUM INTO/bounded hash manifest, external DB rebasing, all required media/package/secrets/config included, safe runtime/backups exclusions.
Không có migration mới trong task.

#### Ngoài phạm vi

Không live SQLite/WAL copy, wholeDB serialization, public backup stream hoặc crypto yet.

#### Phụ thuộc và điều kiện chặn

M8-S02-T02 G5 (P08 lifecycle settled); shared paths/operations ownership. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-core/src/backup.rs`
- `cms/crates/opendash-server/src/backup.rs`
- `cms/crates/opendash-server/src/backup_worker.rs`
- `cms/Cargo.toml`
- `cms/Cargo.lock`
- `cms/crates/opendash-server/Cargo.toml`
- `cms/crates/opendash-server/src/storage/backup.rs`
- `cms/crates/opendash-server/src/paths.rs`
- `cms/crates/opendash-server/src/secrets.rs`
- `cms/crates/opendash-server/tests/m8_s03_t01_snapshot.rs`
- `docs/verification/2026-11-14-m8-s03-t01-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

create_snapshot(paths,ExclusiveOpsContext)->SnapshotReceipt; storage::backup::vacuum_into_snapshot(db,destination); pure BackupManifestV1/FileRecord/PortableRuntimeConfigV1 DTOs; domain logical key maps, no raw path join.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Concurrent admitted upload/import/KV/mail/secretrotation drained; snapshot never mismatches referenced files/DB.
- [ ] External DB outside root copied correctly; zero source changes; SQLite integrity/foreignkeys/migrationchecksum pass.
- [ ] Missing referenced file/revision/secret/hash fails whole snapshot; no silent omissions; unsafe filenames blocked.
- [ ] 4GiB/20000 payload/DB1GiB bounded; at120s cancel and reopen admission only after confirmed worker termination (otherwise remain maintenance-blocked) with bounded buffers; cancel/ENOSPC closes work before admission reopened.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s03_t01_snapshot --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s03-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S04-T01 — Encrypted backup archive codec và bounded verifier

- **Thuộc:** V1 · M8 · M8-S04 · M8-P02
- **Module:** OPS-01
- **Capability:** OPS-01.3
- **Primary Acceptance Slice của capability:** M8-S04; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §5](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Encrypted backup archive codec và bounded verifier cung cấp phần có thể kiểm chứng của M8-P02; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Flat uncompressedZIP + manifest, age streaming finish/auth, capped scrypt/header/size, native ACL plaintext lifecycle, file-backed tamper verifier shared restore.

#### Phạm vi công việc

Flat uncompressedZIP + manifest, age streaming finish/auth, capped scrypt/header/size, native ACL plaintext lifecycle, file-backed tamper verifier shared restore.
Không có migration mới trong task.

#### Ngoài phạm vi

Không unencrypted export, shellcrypto, custom protocol, key inDB/log/argv hoặc raise theme/plugin caps.

#### Phụ thuộc và điều kiện chặn

M8-S03-T01 G5; Cargo/package/secrets seams exclusive. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/Cargo.toml`
- `cms/Cargo.lock`
- `cms/crates/opendash-server/Cargo.toml`
- `cms/crates/opendash-server/src/backup_archive.rs`
- `cms/crates/opendash-server/src/package.rs`
- `cms/crates/opendash-server/src/secrets.rs`
- `cms/crates/opendash-server/tests/m8_s04_t01_backup_codec.rs`
- `docs/verification/2026-11-14-m8-s04-t01-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

write_encrypted_archive(snapshot,SecretString,out)->ArtifactReceipt; decrypt_and_validate_archive(input,SecretString,private_dir)->ValidatedArchive; package shared safe-flat/profile over Read+Seek file. Exact age0.11.2, ZIP9.3.0 unchanged limits for themes/plugins.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Valid roundtrip all payload hashes; wrongpass/truncation/tamper/extraentries/pathcasecollision/malicious logical keys reject.
- [ ] Header/stanza/KDF highwork rejected before expensive work; onecrypto worker bounded memory, wholefile never inRAM.
- [ ] Encrypted artifact complete only after ZIP/AEAD finish+sync; partial failure leaves no downloadable plaintext.
- [ ] Windows protected DACL/Unix0700/0600 before bytes; native build with pinned age/lock verified, existing package fixtures unchanged.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s04_t01_backup_codec --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s04-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S04-T02 — Admin backup jobs, download/cancel và retention UI

- **Thuộc:** V1 · M8 · M8-S04 · M8-P02
- **Module:** OPS-01
- **Capability:** OPS-01.4
- **Primary Acceptance Slice của capability:** M8-S04; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §5](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Admin backup jobs, download/cancel và retention UI cung cấp phần có thể kiểm chứng của M8-P02; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Admin passphrase confirmation/no persistent secret, manual jobs/progress/cancel/restartInterrupted, bounded artifact/receipt retention and download/delete UI.

#### Phạm vi công việc

Admin passphrase confirmation/no persistent secret, manual jobs/progress/cancel/restartInterrupted, bounded artifact/receipt retention and download/delete UI.
Không có migration mới trong task.

#### Ngoài phạm vi

Không scheduled backup, auto replay after401, bearer public download, production server changes.

#### Phụ thuộc và điều kiện chặn

M8-S04-T01 G5; operations/routes/AdminApp handoff. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/backup.rs`
- `cms/crates/opendash-server/src/storage/operations.rs`
- `cms/crates/opendash-server/src/routes/operations.rs`
- `cms/admin/src/api/operations.ts`
- `cms/admin/src/features/operations/BackupsPanel.tsx`
- `cms/crates/opendash-server/tests/m8_s04_t02_backup_admin.rs`
- `docs/verification/2026-11-14-m8-s04-t02-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

POST backups/passphrase request, sanitized JobView/list/cancel/GETHEAD encrypted file/DELETE artifact; request_id idempotency + CAS, streaming download lease.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Admin backup→encrypted download works in browser; Editor/noCSRF403, expiredsession401 preserves nonsecret state but no passphrase persistence.
- [ ] Repeat same nonce returns same operation not duplicate sideeffect; changednonsecretpayload409; max10artifacts/200receipts reported.
- [ ] Download64KiB/private/no-store, no raw paths/secrets/keys; delete waits active read lease; cancellation joins work.
- [ ] Crash/restart no automatic secret-dependent retry; old completed artifacts/live site survive cleanup/diskfull.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s04_t02_backup_admin --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s04-t02-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S05-T01 — Validate restore, rebase và publish separate site root

- **Thuộc:** V1 · M8 · M8-S05 · M8-P03
- **Module:** OPS-01
- **Capability:** OPS-01.5
- **Primary Acceptance Slice của capability:** M8-S05; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §6](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Validate restore, rebase và publish separate site root cung cấp phần có thể kiểm chứng của M8-P03; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Bounded restore upload/decrypt/validate, allowlisted logical extraction, actual file/package/secrets check, native ACL reconstruction, externalDB→newroot rebasing, session/queue/sending sanitation and safe-mode config.

#### Phạm vi công việc

Bounded restore upload/decrypt/validate, allowlisted logical extraction, actual file/package/secrets check, native ACL reconstruction, externalDB→newroot rebasing, session/queue/sending sanitation and safe-mode config.
Không có migration mới trong task.

#### Ngoài phạm vi

Không overwrite active root/DB, automatic plugin/mail execution, arbitrary target, archive binary execution or auto-start restored site.

#### Phụ thuộc và điều kiện chặn

M8-S04-T02 G5; validated codec/paths/DB boundaries handed off. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/restore.rs`
- `cms/fixtures/m8/schema_fingerprints.json`
- `cms/crates/opendash-server/src/storage/restore.rs`
- `cms/crates/opendash-server/src/storage/backup.rs`
- `cms/crates/opendash-server/src/backup_archive.rs`
- `cms/crates/opendash-server/src/paths.rs`
- `cms/crates/opendash-server/src/secrets.rs`
- `cms/crates/opendash-server/tests/m8_s05_t01_restore.rs`
- `docs/verification/2026-11-14-m8-s05-t01-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

restore_to_new_root(ValidatedArchive,RestorePolicy,generated_target)->RestoreReceipt; storage::restore::validate_and_rebase_copy(pool) and schema/history checks before migrate copy. READY marker/no-replace publish; never source overwrite.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Actual backup restore retains all content/IDs/layout/media/secretrefs; explicit reset states only; targetAdmin login available without bootstrap bypass.
- [ ] Newer/divergent schema/manifest/tamper/missing file/secret/package reject prepublish; forged-valid migration ledger with extra malicious trigger/view/virtual table is rejected by compiled sqlite_schema fingerprint before any write; legitimate0008/0012 triggers retained. Source unchanged byte-for-byte.
- [ ] Crash every decrypt/extract/migrate/READY/rename phase leaves source intact, incomplete private target never published; ambiguous result inspected.
- [ ] CrossOS ACL recreated; external DB paths never reused; DB/FK checks and poolsclosed/fsync before atomic samevolume publish.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s05_t01_restore --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s05-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S05-T02 — Admin restore workflow, recovery mode và operator activation

- **Thuộc:** V1 · M8 · M8-S05 · M8-P03
- **Module:** OPS-01
- **Capability:** OPS-01.6
- **Primary Acceptance Slice của capability:** M8-S05; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §6](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Admin restore workflow, recovery mode và operator activation cung cấp phần có thể kiểm chứng của M8-P03; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Admin creates restored copy, polls status/diagnostics; starts target via documented operator command; login/activate/re-edit/Save/public and source rollback. Safe mode noindex/noanonymouscontact/noguestegress.

#### Phạm vi công việc

Admin creates restored copy, polls status/diagnostics; starts target via documented operator command; login/activate/re-edit/Save/public and source rollback. Safe mode noindex/noanonymouscontact/noguestegress.
Không có migration mới trong task.

#### Ngoài phạm vi

Không hidden hot-swap/restart daemon, current site deletion, auto-enable SMTP/send old queued work.

#### Phụ thuộc và điều kiện chặn

M8-S05-T01 G5; config/interface contract consumed by S06. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/restore.rs`
- `cms/crates/opendash-server/src/config.rs`
- `cms/crates/opendash-server/src/operations.rs`
- `cms/crates/opendash-server/src/routes/operations.rs`
- `cms/crates/opendash-server/src/smtp.rs`
- `cms/crates/opendash-server/src/plugin/manager.rs`
- `cms/crates/opendash-server/src/contact.rs`
- `cms/admin/src/api/operations.ts`
- `cms/admin/src/features/operations/RestorePanel.tsx`
- `cms/crates/opendash-server/tests/m8_s05_t02_restore_admin.rs`
- `docs/verification/2026-11-14-m8-s05-t02-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

Creates server config.rs foundation now: typed RuntimeConfigV1 loader using PortableRuntimeConfigV1 from core/backup (S03), strict precedence and safe-mode persistence before target start. Later S06 extends network/proxy/shutdown behavior, not this prerequisite. restore-uploads/restore-job API, RecoveryState core policy enforced by service admission; POST recovery/activate checks currentpassword+CSRF+READYgeneration; preserves stored plugin flags while denying effects in safe mode.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] Browser upload artifact/passphrase→readycopy, credentials not persisted; sourcepublic stays unchanged.
- [ ] Restored target launches loopback with own leases, knownAdmin reauth, no stale sessions; guest/mail/contact sideeffects denied before activation.
- [ ] Activate auth/password/CAS/Origin failure no effects; explicit success clears safemode only, SMTP still disabled/pendingnotreplayed.
- [ ] Operator demonstrates switch-to-newroot + rollback matchedoldroot/binary, no loose migratedDB downgrade.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s05_t02_restore_admin --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s05-t02-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S06-T01 — Linux runtime config, proxy authority và graceful shutdown

- **Thuộc:** V1 · M8 · M8-S06 · M8-P04
- **Module:** OPS-02
- **Capability:** OPS-02.1
- **Primary Acceptance Slice của capability:** M8-S06; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §7](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Linux runtime config, proxy authority và graceful shutdown cung cấp phần có thể kiểm chứng của M8-P04; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Strict loopback-only bind/configured public proxy authority, HTTPS Secure cookies under trusted deploymentconfig, mandatoryAdminOrigin, no forwardedheadertrust, signals/readiness/logredaction/recoverymode.

#### Phạm vi công việc

Strict loopback-only bind/configured public proxy authority, HTTPS Secure cookies under trusted deploymentconfig, mandatoryAdminOrigin, no forwardedheadertrust, signals/readiness/logredaction/recoverymode.
Không có migration mới trong task.

#### Ngoài phạm vi

Không automaticTLS/proxyprovisioning, publicbinddefault, userglobalconfig, securitygatewaiver.

#### Phụ thuộc và điều kiện chặn

M8-S05-T02 G5; config snapshot/restore contract pinned, core App/main/auth shared seams released. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/config.rs`
- `cms/crates/opendash-server/src/main.rs`
- `cms/crates/opendash-server/src/app.rs`
- `cms/crates/opendash-server/src/auth.rs`
- `cms/crates/opendash-server/src/paths.rs`
- `cms/crates/opendash-server/src/operations.rs`
- `cms/crates/opendash-server/src/routes/health.rs`
- `cms/crates/opendash-server/src/routes/contact.rs`
- `cms/crates/opendash-server/tests/m8_s06_t01_runtime.rs`
- `docs/verification/2026-11-14-m8-s06-t01-g5.md`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

RuntimeConfigV1 strict JSON/env loader; configured exact public Host/Origin distinct from listener; shutdown token drains coordinator/workers/SQLx before lock release; check-config CLI mode without side effects.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] CleanLinux+Windows localmode still starts; malformed/duplicateHost/Origin/null/proxyspoof reject; legitconfiguredHTTPSproxy works.
- [ ] Cookie flags correct, no external insecureAdmin; actualbrowserPreview/Save/contact401/412 regressions pass.
- [ ] SIGTERM duringbackup/upload/plugin/SMTP stops within30s or safeinterruptedstate; no orphanworker/lateDB writes; lock released only afterjoin.
- [ ] Restart recovers journalsbeforelistening; configdoesnotleaksecrets/paths and backuprestoreeffectiveconfig consistent.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s06_t01_runtime --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s06-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S06-T02 — Reproducible-input Linux executable bundle và install/upgrade aids

- **Thuộc:** V1 · M8 · M8-S06 · M8-P04
- **Module:** OPS-02
- **Capability:** OPS-02.2
- **Primary Acceptance Slice của capability:** M8-S06; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §8](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Reproducible-input Linux executable bundle và install/upgrade aids cung cấp phần có thể kiểm chứng của M8-P04; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Binary/buildprovenance/notices, leastprivilegedinstall/run/unit/proxyexamples, versionedsidebysideupgrade/rollback commands; pin Node22.16.0 and record build dependencies.

#### Phạm vi công việc

Binary/buildprovenance/notices, leastprivilegedinstall/run/unit/proxyexamples, versionedsidebysideupgrade/rollback commands; pin Node22.16.0 and record build dependencies.
Không có migration mới trong task.

#### Ngoài phạm vi

Không release tag/upload/deploy, unattendedupdate, secretsintarball hoặc changingruleset/governancechecks.

#### Phụ thuộc và điều kiện chặn

M8-S06-T01 G5; nativeLinuxrunner/toolchain available; no claim from crosscompile only. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/rust-toolchain.toml`
- `cms/scripts/build-release-linux.sh`
- `cms/scripts/verify-release-artifact.py`
- `cms/deploy/opendash.service`
- `cms/deploy/nginx.conf.example`
- `cms/deploy/install-linux.sh`
- `.github/workflows/release-validation.yml`
Cộng shared wiring khi slice cần: server src/app.rs/main.rs/routes/mod.rs/storage/mod.rs, core src/lib.rs, Admin src/App.tsx/api/index.ts; coordinator giao từng seam, không hai writer cùng file.

#### Cách triển khai theo contract

Explicit release validation build pipeline (not publish), release-manifest.json/checksums.txt schema; pinnedRust1.99.1/x86_64-musl/npmci embeddedAdmin; native Linux artifact verification using clean environment.
Chỉ sửa files thuộc task; product failure cần fix riêng và repin source/retest, không hạ acceptance cho report đẹp.

#### Nghiệm thu

- [ ] Linux binary runswithoutNode/sqlite3/age/OpenSSL executables or dynamicloaderdependency; readelf/ldd evidence recorded.
- [ ] EmbeddedAdmin hash/source/schema/toolversions/checksums match; artifactnofixtureprivatekeys/runtimecredentials.
- [ ] Cleanoperatorinstall appuser/data0600+0700/binaryreadonly, servicehealth/restart, externalDB allowpath bounded.
- [ ] Two-build comparison records match/differencehonestly; no reproducibilityclaim without evidence; upgradesnapshotrollback rehearsed.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run common native/build/browser/operational checklist applicable to this task; record actual commands, exit codes/OS/hash and deferred items; missing required runner/evidence means NOT_RUN/BLOCKED.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Evidence/report files exactly as listed; keep platform assertions scoped. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S07-T01 — Linux adversarial reliability và performance smoke

- **Thuộc:** V1 · M8 · M8-S07 · M8-P05
- **Module:** OPS-02
- **Capability:** OPS-02.3
- **Primary Acceptance Slice của capability:** M8-S07; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §9](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Linux adversarial reliability và performance smoke cung cấp phần có thể kiểm chứng của M8-P05; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Real Linux public/Admin/security/failure/crash/restart/backup/delete/restore concurrency and boundedmeasurements percontract§9.

#### Phạm vi công việc

Real Linux public/Admin/security/failure/crash/restart/backup/delete/restore concurrency and boundedmeasurements percontract§9.
Không có migration mới trong task.

#### Ngoài phạm vi

Không inferredLinuxPASSfromWindows/CIgovernance, unboundedbenchmark hoặc silently relaxedbudgets.

#### Phụ thuộc và điều kiện chặn

M8-S06-T02 nativeartifactverified; actualLinuxhost/referencehardware available. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/scripts/m8_linux_smoke.py`
- `cms/scripts/m8_perf_smoke.py`
- `cms/crates/opendash-server/tests/m8_s07_t01_security.rs`
- `docs/verification/2026-11-14-m8-linux-smoke.md`
- `docs/verification/2026-11-14-m8-s07-t01-g5.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

Finite seededdataset runner accepts --binary/--site-root/--artifact-dir/--seed; outputs metrics/rawcommandreceipt with OS/CPU/kernel/fs/resourcecontext, no loops/services beyond task lifetime.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] All specified security negative cases pass; known failures remain blockers, not risk acceptance.
- [ ] Reference 300 reads/100 writes p95 budgets and RSS/startup/frozen limits measured; wrong hardware means NOT_COMPARABLE.
- [ ] ENOSPC/ACLdenied/ZIPbomb/lockedstream/cancel/signal tests recover sourcewithoutloss; no secretsinlogs.
- [ ] Actual binary/source/hash/commands and metrics archived outside Git with a bounded summary; missing runner means NOT_RUN.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s07_t01_security --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s07-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S08-T01 — Full V1 backup→restore→re-edit E2E harness

- **Thuộc:** V1 · M8 · M8-S08 · M8-P06
- **Module:** OPS-01
- **Capability:** OPS-01.7
- **Primary Acceptance Slice của capability:** M8-S08; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §10](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Full V1 backup→restore→re-edit E2E harness cung cấp phần có thể kiểm chứng của M8-P06; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

All V1 features → backup → mutate source → Admin restore separate root → start target/re-auth/activate/re-edit/Save/public, plus negative corpus; no production mail/credentials.

#### Phạm vi công việc

All V1 features → backup → mutate source → Admin restore separate root → start target/re-auth/activate/re-edit/Save/public, plus negative corpus; no production mail/credentials.
Không có migration mới trong task.

#### Ngoài phạm vi

Không fix product insideevidencetask; openfixPR/re-pin/retest, noG5allOSfromsingleharness.

#### Phụ thuộc và điều kiện chặn

M8-S07-T01 G5Linux + allS01..S06acceptance; fixtures/nativefilesystem established. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/tests/m8_s08_t01_e2e.rs`
- `cms/scripts/m8_browser_e2e.py`
- `cms/fixtures/m8/README.md`
- `docs/verification/2026-11-14-m8-s08-t01-g5.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

Runrealbinaryisolatedfresh+upgrade, deterministictheme/plugin/media/formfixtures, encryptedartifacthash and snapshotcomparison; browserrunner --binary/--site-root/--artifact-dir.
Tái sử dụng primitives hiện có; SQL chỉ storage, trusted path constructors/private ACL, admission token lifetime và failpoint recovery đúng contract; không duplicate auth/ZIP/id logic.

#### Nghiệm thu

- [ ] 100%mandatorydata/layout/ID/refequivalence with explicitresetexceptionlist; actual publicroutes/media/forms/settings/KV work afterrestore.
- [ ] Fresh/upgrade0031→0033, externalDBandsecretACL, oldbinarymatchedsnapshotrollback; no publicdraft/privateleak.
- [ ] Ciphertexttamper/wrongpass/newerschema/interrupt/quotafailure sourceintact and targetnotpublished.
- [ ] Evidence includes source/binary/artifacthashes and nativebrowser/no-console-error, no opaque reportedPASSalone.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run `cargo test --manifest-path cms/Cargo.toml --test m8_s08_t01_e2e --locked`; exit0 và tất cả behavior/negative assertions ở checklist pass. Chạy focused predecessors và native browser/UI build theo phạm vi, rồi governance chung; required case NOT_RUN chặn scope acceptance.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Report `docs/verification/2026-11-14-m8-s08-t01-g5.md` thuộc ownership task, no raw secret/runtime artifacts. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S08-T02 — Native Windows V1 recovery acceptance

- **Thuộc:** V1 · M8 · M8-S08 · M8-P06
- **Module:** OPS-01
- **Capability:** OPS-01.7
- **Primary Acceptance Slice của capability:** M8-S08; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §10](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Native Windows V1 recovery acceptance cung cấp phần có thể kiểm chứng của M8-P06; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Native Windows full V1 flows: locks/sharing/rename/external DB/backup/delete/restore/private ACL, including race and recovery cases.

#### Phạm vi công việc

Native Windows full V1 flows: locks/sharing/rename/external DB/backup/delete/restore/private ACL, including race and recovery cases.
Không có migration mới trong task.

#### Ngoài phạm vi

Không countWine/LinuxCIasnativeWin, no producteditsinthe evidence report.

#### Phụ thuộc và điều kiện chặn

M8-S08-T01 G5; Atlas pin common final RC and matchingpreM8snapshot/binary. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m8-g5-windows.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

Consumes commonRC/artifacts/harness; produces Windowsarchitecture/binaryhash/commands/browser/DACL/crash/re-edit receipts.
Chỉ sửa files thuộc task; product failure cần fix riêng và repin source/retest, không hạ acceptance cho report đẹp.

#### Nghiệm thu

- [ ] FullcontractG5Winmatrix actualPASS inclbinarybrowser/TLSfixtures/video/snapshotrollback.
- [ ] No macOS/Linux inference; reportlimits/NOT_RUN; sourcechangesforceaffectedrechecks.
- [ ] Alldata/permissiontestscorrect; filehandles/locksclosed and DACLrecreatednotcopiedfromsourceSID.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run common native/build/browser/operational checklist applicable to this task; record actual commands, exit codes/OS/hash and deferred items; missing required runner/evidence means NOT_RUN/BLOCKED.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Evidence/report files exactly as listed; keep platform assertions scoped. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S08-T03 — Native Linux V1 install/upgrade/restore acceptance

- **Thuộc:** V1 · M8 · M8-S08 · M8-P06
- **Module:** OPS-02
- **Capability:** OPS-02.4
- **Primary Acceptance Slice của capability:** M8-S08; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §10](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Native Linux V1 install/upgrade/restore acceptance cung cấp phần có thể kiểm chứng của M8-P06; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Fresh native Linux site from release bundle: install → author → backup → restore → activate → re-edit → upgrade/rollback, including system service hygiene.

#### Phạm vi công việc

Fresh native Linux site from release bundle: install → author → backup → restore → activate → re-edit → upgrade/rollback, including system service hygiene.
Không có migration mới trong task.

#### Ngoài phạm vi

Không containerbuildsuccessalone, performancebudgetwaiver, productiondeployment hoặc realSMTPmail.

#### Phụ thuộc và điều kiện chặn

M8-S08-T01 G5; sameRCasWindows and nativeLinuxartifact/host. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m8-g5-linux.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

Consumes commonRC+muslbundle; produces cleanLinuxinstall/browser/proxy/performance+encryptedrestore evidence.
Chỉ sửa files thuộc task; product failure cần fix riêng và repin source/retest, không hạ acceptance cho report đẹp.

#### Nghiệm thu

- [ ] Exacttarget/tooling/noexternalruntimeexecverified oncleanUbuntu24.04; realbinary/browserandallsecuritymatricesPASS.
- [ ] Performanceenvironmentcomparable+thresholdsPASS; configproxy/cookie/Originlegitimate+adversarialflowsPASS.
- [ ] Encryptedrestoreincludingsecrets works with Unix0700/0600/externalDBrebasing; sameimmutableRCasWin/Mac.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run common native/build/browser/operational checklist applicable to this task; record actual commands, exit codes/OS/hash and deferred items; missing required runner/evidence means NOT_RUN/BLOCKED.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Evidence/report files exactly as listed; keep platform assertions scoped. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S08-T04 — Native macOS và bù deferred M5/M6/M7 evidence

- **Thuộc:** V1 · M8 · M8-S08 · M8-P06
- **Module:** OPS-01
- **Capability:** OPS-01.7
- **Primary Acceptance Slice của capability:** M8-S08; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §10](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Native macOS và bù deferred M5/M6/M7 evidence cung cấp phần có thể kiểm chứng của M8-P06; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Mac real binary/browser/TLS/video/file locks/APFS name collision/ACL/backup-restore plus previous milestone matrices. Mac unavailable stays NOT_RUN.

#### Phạm vi công việc

Mac real binary/browser/TLS/video/file locks/APFS name collision/ACL/backup-restore plus previous milestone matrices. Mac unavailable stays NOT_RUN.
Không có migration mới trong task.

#### Ngoài phạm vi

Không sayAppleSiliconifIntelonly, no guessedapproval/closingexistingIssues, no securitywaiver.

#### Phụ thuộc và điều kiện chặn

M8-S08-T01 G5; actualMacavailable/commonRC; coordinate ownership existing#157/#188/#214 notauto-takeover. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m8-g5-macos.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

Produces nativeMacarchitecture/hash/commands and explicitmappingof deferredM5/M6/M7macacceptance against commonRC; coordinator separatelyreviewsclosures.
Chỉ sửa files thuộc task; product failure cần fix riêng và repin source/retest, không hạ acceptance cho report đẹp.

#### Nghiệm thu

- [ ] Native Mac full required acceptance on common RC including deferred milestones, with complete commands/artifact hashes.
- [ ] Casefold/atomicrename/directorypermissions/restore/re-edit andexternalDB testsPASS.
- [ ] If Mac failure needs a product fix, select new RC and rerun affected Windows/Linux checks; no final V1 release until done.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run common native/build/browser/operational checklist applicable to this task; record actual commands, exit codes/OS/hash and deferred items; missing required runner/evidence means NOT_RUN/BLOCKED.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Evidence/report files exactly as listed; keep platform assertions scoped. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S09-T01 — Operator runbooks, dependency notices và cold-start rehearsal

- **Thuộc:** V1 · M8 · M8-S09 · M8-P07
- **Module:** OPS-02
- **Capability:** OPS-02.5
- **Primary Acceptance Slice của capability:** M8-S09; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Atlas dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §10](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Operator runbooks, dependency notices và cold-start rehearsal cung cấp phần có thể kiểm chứng của M8-P07; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Allcontractoperationalflowsdocumentation, newrootactivation/passphraseloss/privacy/nohot-swap constraints, independentcoldoperatorLinuxrehearsalreceipt.

#### Phạm vi công việc

Allcontractoperationalflowsdocumentation, newrootactivation/passphraseloss/privacy/nohot-swap constraints, independentcoldoperatorLinuxrehearsalreceipt.
Không có migration mới trong task.

#### Ngoài phạm vi

Không inventlicense, unexecuteddeploymentclaims hoặc defaultdeletingrecoverycopies.

#### Phụ thuộc và điều kiện chặn

M8-S06-T02 + S07-T01 evidence, API/CLI contracts stable; ownerlicense decision external beforepublicrelease. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `docs/operations/README.md`
- `docs/operations/INSTALL_LINUX.md`
- `docs/operations/BACKUP_RESTORE.md`
- `docs/operations/UPGRADE_RECOVERY.md`
- `docs/operations/MEDIA_LIFECYCLE.md`
- `docs/operations/RELEASE_CHECKLIST.md`
- `README.md`
- `docs/README.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

Operator-readableexactcommands and rollbackdecisiontree, provenance/thirdpartynoticelist/versionedinstallunit mapping, no productlicenseguess.
Chỉ sửa files thuộc task; product failure cần fix riêng và repin source/retest, không hạ acceptance cho report đẹp.

#### Nghiệm thu

- [ ] Cold operator follows clean Linux steps for install/author/backup/restore/re-edit/upgrade/rollback without unwritten steps.
- [ ] Wrongpassword/missingblob/newerschema/diskfull/reconcilefail paths documentedandobservedrecoverable.
- [ ] No secrets/absoluteprivatehostpaths committed; READMEcurrentoperatingcommands alignactualbinary, originalhistoryretained.
- [ ] Thirdpartylicense/notices inventory prepared; projectlicenseownerdecision missing explicitlyRELEASE_BLOCKED.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run common native/build/browser/operational checklist applicable to this task; record actual commands, exit codes/OS/hash and deferred items; missing required runner/evidence means NOT_RUN/BLOCKED.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Evidence/report files exactly as listed; keep platform assertions scoped. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Không tự đóng full milestone/V1 từ task riêng.

### M8-S09-T02 — Final V1 evidence audit và release readiness decision

- **Thuộc:** V1 · M8 · M8-S09 · M8-P07
- **Module:** OPS-02
- **Capability:** OPS-02.6
- **Primary Acceptance Slice của capability:** M8-S09; native tasks regression existing capability as coverage matrix.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Atlas dự kiến; coordinator Atlas, self-review/peer review theo ADR-0010, không active assignment.
- **Nguồn quyết định:** [ADR-0015](../../../../../decisions/ADR-0015-m8-operations-release.md), [Contract §10](../../../../../contracts/M8_OPERATIONS_RELEASE.md).

#### Tổng quan và giá trị

Final V1 evidence audit và release readiness decision cung cấp phần có thể kiểm chứng của M8-P07; bảo vệ dữ liệu website và giảm bước vận hành phải suy đoán.

#### Mục tiêu và hành vi quan sát được

Technical audit of current Issues/Project/CI/security/native/performance/license/notices and binary manifest; final operator handoff, Done only with full proof.

#### Phạm vi công việc

Technical audit of current Issues/Project/CI/security/native/performance/license/notices and binary manifest; final operator handoff, Done only with full proof.
Không có migration mới trong task.

#### Ngoài phạm vi

Không claimV1DONEwhenMac/Linux/restore/license missing, no autonomousdeploy/publish, no fabricatedpeerreview.

#### Phụ thuộc và điều kiện chặn

All17 predecessor tasks G5 (exclude M8-S09-T02 itself)/common RC, #157/#188/#214/#215fullnativeclosure, operatorrehearsal+ownerlicensechoice; explicitreleaseauthorization separate. Trước pickup: G3 source merged immutable, coordinator xác minh Issue/Project Ready/ownership, một task active/nhánh và exact source base. Không bỏ security/data hoặc native release gates; đọc full contract và global constraints của G3, không chỉ bản trích Issue.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m8-release.md`
- `docs/verification/README.md`
- `docs/plans/TASK_INDEX.md`
- `docs/plans/ROADMAP_V1.md`
- `docs/plans/milestones/M8/README.md`
- `docs/operations/RELEASE_CHECKLIST.md`
Chỉ sở hữu files được liệt kê; không có quyền shared product wiring. Lỗi sản phẩm mở fix task/PR riêng, coordinator giao ownership và repin/rerun acceptance; không sửa sản phẩm trong report/docs/harness để làm test xanh.

#### Cách triển khai theo contract

ReleaseAcceptanceMatrix links eachmandatorycapability toactualPrimaryAcceptance/commonRC+nativeevidence; verdict READY_FOR_OWNER_RELEASE orRELEASE_BLOCKED; noautomaticpublictag.
Chỉ sửa files thuộc task; product failure cần fix riêng và repin source/retest, không hạ acceptance cho report đẹp.

#### Nghiệm thu

- [ ] Allmandatorycapabilitiescovered, no unresolvedsecurity/datafinding/requiredNOT_RUN, Linux/Windows/MacsameRCconfirmed.
- [ ] Deferredmilestonesclosurevalidatedseparately, no Issueautoclosefrommerge; snapshot/binariesoperatorrollbackavailable.
- [ ] RequiredCIexactheadandnativeevidencePASS; releaseartifactchecksums/releaseinputs/noticescomplete.
- [ ] External owner license/release approval missing means RELEASE_BLOCKED with specific dependency; technical G3 remains complete.
- [ ] Evidence exact SHA/binary/OS/commands/result/limits; không ghi VERIFIED từ thiết kế/CI/agent consensus.
- [ ] Không còn security/data blocker và không phá preservation/preview/public/auth của các mốc trước.

#### Kiểm thử, rủi ro và rollback

Run common native/build/browser/operational checklist applicable to this task; record actual commands, exit codes/OS/hash and deferred items; missing required runner/evidence means NOT_RUN/BLOCKED.
Test failure/cancel/restart cannot damage source state; recovery uses complete matched snapshot+binary/newroot, never partial files or in-place DB downgrade.

#### Bàn giao và review

Commit/PR head+base, ownership release, changed files, commands PASS/FAIL/NOT_RUN, artifact hashes and risks. Evidence/report files exactly as listed; keep platform assertions scoped. Issue/Project authoritative; no background dispatch, automatic production tag/deployment or fabricated independent review. Release-readiness decision separate from owner's public release approval.
