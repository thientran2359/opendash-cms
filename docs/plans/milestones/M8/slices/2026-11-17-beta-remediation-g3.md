# M8 — Beta recovery blocker and browser harness G3

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

## G1/G2 security disposition

F08 remains an S0-candidate security acceptance blocker. The existing M8 contract
already requires generated recovery_safe_mode=true, noindex and denied side effects
until authenticated operator activation. Restore helper code ALREADY sets true for
PortableConfig and default branches. Therefore the audit is not explained simply
by the config loader condition: reproduce the actual Admin/worker/archive/target path
and reconcile source/binary/artifact provenance. Do not assert a precise root cause
or claim SMTP/plugin/session exploit without evidence. A stricter READY-only loader
that always forces true would break restart-after-valid-activation; that is not an
approved shortcut. Repair the invariant under existing contract; any new durable
activation marker/schema/permission semantics needs a separate G2 contract review.

### M8-S10-T01 — Reconcile native restored-root Safe Mode and repair activation gate

- **Thuộc:** V1 internal Beta regression · M8 · M8-S10
- **Module:** OPS-01
- **Capability:** OPS-01.5
- **Primary Acceptance Slice của capability:** M8-S05; regression only, no new capability
- **Track:** Full-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Security-sensitive existing-invariant repair; no policy relaxation.
- **G3:** READY
- **Trạng thái:** PLANNED; conditional on peer review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** aster / Forge; coordinator Aster

#### Tổng quan và giá trị

F08: audit reports READY true/config false/public 200 without noindex. restore.rs lines around PortableConfig/default set true; config.rs loader accepts existing false. These facts require end-to-end reconciliation.

#### Mục tiêu và hành vi quan sát được

Newly restored root starts in Safe Mode before operator activation; explicit authenticated activation works durably, SMTP remains off and source site is untouched.

#### Phạm vi công việc

Reproduce exact Admin restore and worker path with fresh encrypted backup whose source config has safe=false. Identify target/config writer and provenance; fix only confirmed path. Add regression on actual binary start, not only helper.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

M8 existing contract unchanged and this Full-Track G3 peer-reviewed/merged; exclusive Aster repair slot; native Windows validation scheduled with Forge after UI task slot release. If new security persistence semantics required, BLOCKED until distinct G2 PR.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/restore.rs`
- `cms/crates/opendash-server/src/config.rs`
- `cms/crates/opendash-server/src/backup_worker.rs`
- `cms/crates/opendash-server/tests/m8_s10_t01_restored_startup.rs`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Reuse typed PortableRuntimeConfigV1 validation, canonical package/secret path guards, complete staging validation/fsync/no-replace publish and existing authenticated activate endpoint. Compare generated READY/config, DB and binary hashes. Do not clear enabled plugin flags to simulate gating. No network, guest or SMTP effects during validation/start; sessions invalidated by existing restore sanitization. Do not unconditionally force safe mode on every READY boot after valid activation. New state or route changes require separate contract/ownership review before implementation.

#### Nghiệm thu

- [ ] Native Admin backup→restore-to-new-root with copied false config yields true durable config and safe_mode=true on binary start/restart; homepage/post noindex, public contact503, plugin/action/egress denied.
- [ ] Old sessions invalid; fresh Admin can login/re-edit in recovery; Editor cannot activate; wrong password/CSRF/Origin/generation denied with no state change.
- [ ] Valid activation clears safe mode durably across restart; SMTP stays disabled, no queued work replay; original site content/config unchanged.
- [ ] Malformed/missing/oversize config/marker, symlink/reparse and interrupted publication fail closed without exposing target; existing format1 compatibility preserved.
- [ ] Windows and macOS actual restore/start evidence reconciled; failure to reproduce keeps security blocker until reviewer-approved explanation with provenance, not cosmetic downgrade.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m8_s10_t01_restored_startup; existing m8_s05_t01_restore/m8_s05_t02_restore_admin/m8_s06_t01_runtime; native Windows and macOS end-to-end loopback restored-root launch, exact source/hash. No public host, no live backups. Evidence must distinguish actual public noindex policy from optional HTTP denial.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.

### M8-S10-T02 — Update tracked backup/restore browser runner for current navigation

- **Thuộc:** V1 internal Beta regression · M8 · M8-S10
- **Module:** OPS-01
- **Capability:** OPS-01.7
- **Primary Acceptance Slice của capability:** M8-S08; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Harness-only selectors/lifecycle under unchanged acceptance.
- **G3:** READY
- **Trạng thái:** PLANNED; conditional on peer review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** forge / Aster; coordinator Aster

#### Tổng quan và giá trị

F06: m8_browser_e2e.py clicks button Builder; current navigation exposes Visual Builder. #297 scratch navigation passes do not replace tracked runner.

#### Mục tiêu và hành vi quan sát được

Tracked runner completes encrypted backup, new-root recovery, reauth, explicit activation and Builder re-edit using current Admin UI.

#### Phạm vi công việc

Repair stale selectors with accessible semantic navigation; bound waits and cleanup; retain all backup/recovery/security assertions.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

M8-S10-T01 G5 and affected M7 editor fixes merged/G5; free Forge slot; Harbor coordinates macOS rerun without overlapping #214 files. No scratch runner as substitute.

#### Kỹ thuật và file ownership

- `cms/scripts/m8_browser_e2e.py`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Use observed current route/accessible name or stable existing test ID, not broad text matching unrelated controls. Run against real binary/Chrome; verify safe mode BEFORE activation and distinguish active source vs recovered root. No direct SQL/API workaround replacing required Admin journey, no assertion deletion. Only change selectors/readiness/diagnostics unless additional assertions strengthen the same contract.

#### Nghiệm thu

- [ ] Tracked Windows Chrome runner exit0 with backup/artifact/source/candidate hashes and full restore/reauth/activation/re-edit assertions.
- [ ] Native macOS tracked-runner receipt closes only applicable #297 harness gap, not M5/M7/Linux release.
- [ ] Missing navigation/server exit/wrong password produce bounded nonzero; owned processes cleaned, original site and recovered copy preserved.

#### Kiểm thử, rủi ro và rollback

python3 -m py_compile cms/scripts/m8_browser_e2e.py; tracked runner --help then actual flags on native Windows/macOS with isolated site-root/artifact-dir --keep-artifacts --timeout 180 (record actual binary paths privately). T03 consumes exact integrated candidate evidence.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.
