# M8 — Qualified mutation-handler coverage

Coordinator Aster, 18/11/2026. Follow-up CS-05 from
[reviewed research](../../../../../.agents/research/2026-11-18-coding-standards-audit-for-atlas.md)
merged by [PR #382](https://github.com/thientran2359/opendash-cms/pull/382),
source `042f1692e0b69cf8b91a2b9a6c98db3d82295b5c`.
[Coordination #385](https://github.com/thientran2359/opendash-cms/issues/385)
records all thirteen dispositions. Aster owns this plan and its index row.
Harbor is actively implementing #375 and retains its complete file ownership.
This task has no shared source files with #375.

## Final Decision

Aster accepts CS-05 as a checker coverage defect. The current checker collects
bare mutation names from app.rs and keys route functions by bare name, so qualified
registrations can be skipped and equal function names can collide. This observation
does not establish an exploitable unguarded production route.

Extend the existing Python checker and its temporary-repository self-tests. Resolve
registered mutation handlers to canonical module-qualified identities using actual
module declarations/imports. Analyze the existing source grammar conservatively.
Unresolved, ambiguous or unsupported mutation registrations fail with actionable
file/line diagnostics. Do not silently skip a registration to keep the repository
green. A bounded tokenizer/parser in Python stdlib is acceptable. New parser
dependencies, runtime route changes and broad Rust formatting require another task.

Preserve existing architecture checks, CLI options and exit-code behavior. Ordinary
session mutations require a guard in the resolved handler's executable body.
Comments, strings and a different function with the same name cannot satisfy it.
Bootstrap exemptions use the fully resolved login/setup identities. The two
reviewed policy paths below have separate checked classifications and counts.

### Reviewed admission-policy delta — 18/11/2026

Forge's [PR #389](https://github.com/thientran2359/opendash-cms/pull/389) at
`ddd8b5e8991ca712deb33899a4bf9a152b6dd451` resolves all 82 registrations and
flags two actual handlers. Aster inspected these source paths at baseline
`8ac77781a6770688d3a66d9b5b338a9de81cb106` and accepts the following bounded
checker correction. Runtime endpoint protection remains governed by the existing
[M7 contact contract](../../../../contracts/M7_SITE_FEATURES.md) and
[M8 operations contract](../../../../contracts/M8_OPERATIONS_RELEASE.md).

1. `crate::routes::contact::public_submit` is an anonymous contact submission.
   M7 S05 requires explicit strict Origin admission and bounded contact validation,
   not an authenticated session token. Classify only this canonical handler as
   `public_origin_guarded`. Check the executable call to the existing
   `check_strict_contact_origin` helper and the rejecting branch before submission
   processing, including the reviewed trusted-origin alternative from M8 runtime
   config. Resolve and inspect the same module's helper, which rejects missing,
   null and foreign Origin and compares the listener/configured public origin.
   Merely finding the handler name or a string/comment is insufficient. Removing
   the call, helper, rejection branch or Origin checks must fail checker fixtures.
   This does not authorize a second anonymous exemption or infer policy from names.
2. `crate::routes::operations::upload_restore_archive` is a thin wrapper whose
   returned result is the awaited `upload_restore_archive_with_timeout` call.
   Classify it as `delegated_csrf_guarded` only after checking the exact canonical
   wrapper-to-helper pair. Require the wrapper to propagate that helper's result,
   with no successful bypass, and the helper to call
   `require_admin_with_csrf(&headers, &state.db).await?` before upload side effects.
   Missing/renamed/unresolved helper, removed guard, nonpropagated guard result,
   changed delegation or a wrapper bypass fails closed. Do not accept arbitrary
   transitive calls or name-based exemptions.

Implement these narrowly in the checker with named policy metadata and focused
positive/negative synthetic fixtures. Preserve canonical handler resolution and
the existing direct-guard tests. Inventory must distinguish direct, delegated,
public-Origin and bootstrap classifications so all resolved handlers reconcile.
Unexpected production policy shapes produce actionable failures for review.
Static inspection remains bounded lint evidence, not general control-flow proof.
No Rust routes/auth/storage or Harbor-owned files are granted to Forge by this delta.

### M8-S11-T01 — Kiểm đủ CSRF handlers với định danh module đầy đủ

- **Thuộc:** V1 quality follow-up, M8, M8-S11
- **Module:** FND-01
- **Capability:** FND-01.1
- **Primary Acceptance Slice của capability:** Existing core-conventions baseline retained; checker regression coverage only
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective after this specification merges, Project Ready separately verified
- **Nguồn quyết định:** Aster Final Decision above; CS-05; CORE_FOUNDATION existing CSRF guard rule
- **Người triển khai / review:** forge / self-review; coordinator Aster

#### Tổng quan và giá trị

CI phải báo lỗi khi một mutation route thiếu guard dù handler viết dưới dạng
qualified path. Việc này làm checker phản ánh đúng quy chuẩn đã có và giúp tránh
green giả khi route mới được thêm trong các module khác nhau.

#### Mục tiêu và hành vi quan sát được

Checker resolve đúng handler đã đăng ký, kiểm guard trên chính handler đó và trả
exit nonzero cho handler thiếu, mơ hồ hoặc thiếu guard. Báo tổng mutation
registrations, resolved/checked handlers và bootstrap exemptions để đối chiếu
coverage, đồng thời giữ output lỗi cụ thể cho người sửa.

#### Phạm vi công việc

Resolve standalone post/put/patch/delete, chained method registrations và qualified
routing constructors trong app.rs; theo alias/import/module identity đến route
source thật, gồm module con được dùng trong registrations. Deduplicate handler
checks nhưng giữ tổng số registration sites để đối chiếu. Self-test phải chứng minh
missing qualified handler bị từ chối trên implementation cũ và được phát hiện sau sửa.

#### Ngoài phạm vi

Runtime/auth endpoints, RBAC/CSRF semantics, SQL/storage, Admin, dependency upgrades,
CI required-check configuration, fmt/Clippy cleanup, mọi file của Harbor #375.
Không thực hiện CS-06 hoặc coi checker PASS là security audit toàn sản phẩm.

#### Phụ thuộc và điều kiện chặn

Research #382 đã merge. Spec này phải merge, Issue phải pin source/base SHA,
Forge phải rảnh một implementation slot và Project Ready phải được xác minh trước
pickup. Không phụ thuộc #375 completion vì file ownership tách biệt.
Unsupported actual syntax phải có diagnostic và disposition, không đổi route để
né checker. Branch author/reviewer và uncommitted work được bảo toàn.

#### Kỹ thuật và file ownership

- `cms/scripts/check-standards.py`
- `docs/verification/2026-11-18-m8-s11-t01-csrf-checker.md`

Self-tests/fixtures ở temporary repositories do chính checker tạo, không thêm
runtime fixture/route files vào repo. Đọc app.rs và routes để resolve, không sửa
chúng. Aster sở hữu plan/index. Tái sử dụng mask_literals, production_source,
block extraction và check/self_test entry points khi còn đúng với grammar mới.
Contract là [CORE_FOUNDATION](../../../../contracts/CORE_FOUNDATION.md) và
[Planning Workflow](../../../PLANNING_WORKFLOW.md).

#### Cách triển khai theo contract

1. Thêm negative fixture tái hiện post(crate::routes::missing_unguarded_handler)
   và duplicate bare-name collision, ghi kết quả trước sửa. Thêm positive/negative
   fixtures cho hai admission-policy paths đã chốt ở trên.
2. Resolve canonical identity trước lookup guard. Kiểm qualified bootstrap,
   public-Origin và exact delegated pair theo delta trên. Giữ các architecture
   checks cũ hoạt động.
3. Chạy inventory trên current main và self-tests, đối chiếu registration sites
   với diagnostics/counts. Mọi mutation phải được check theo một classification
   đã chốt hoặc thuộc exact bootstrap exemption.
4. Ghi tested SHA, Python/OS, commands/exits và các grammar limits trong receipt.

#### Nghiệm thu

- [ ] Guarded bare/import-alias/qualified handlers PASS cho post/put/patch/delete;
  chained .post và axum::routing::delete được kiểm đúng.
- [ ] Thiếu definition, unsupported registration và thiếu guard FAIL, diagnostic
  có registration location và canonical identity khi resolve được.
- [ ] Hai module có cùng function name không đè nhau. Guard của function khác,
  comment hoặc string không làm unguarded handler PASS.
- [ ] Nested module và alias registrations được resolve hoặc fail rõ ràng;
  bootstrap exemption chỉ đúng login/setup identities thực tế.
- [ ] Public contact admission và restore delegated guard PASS đúng policy path;
  thiếu helper/guard/rejection/propagation hoặc wrapper bypass FAIL. Một module
  khác trùng tên, string/comment hoặc arbitrary delegation không được chấp nhận.
- [ ] Counts direct/delegated/public-Origin/bootstrap cộng đúng số unique resolved
  handlers, tách biệt với tổng registration sites; không âm thầm giảm coverage.
- [ ] Inventory mutation sites trên source pin không còn registrations bị skip;
  counts và unique handler checks/exemptions đối chiếu được.
- [ ] Existing SQL/core I/O/dependency/Admin HTTP checks và self-test regressions
  vẫn bắt vi phạm. Checker và self-test PASS trên current main khi đã sửa đúng.
- [ ] Receipt có baseline negative result, exact tested source, commands/exits,
  môi trường, limitations và self-review head; required CI PASS tại merge head.

#### Kiểm thử, rủi ro và rollback

Chạy `python3 cms/scripts/check-standards.py`, cùng command với `--self-test`,
`python3 cms/scripts/check-plans.py`, cùng command với `--self-test`,
`python3 .agents/scripts/check-agent-artifacts.py` và `git diff --check`.
Windows dùng Python 3.11+ tương ứng. Không cần build/native runtime cho checker-only
task. Test trên synthetic source không gửi request hoặc sửa owner site.
Rủi ro là false positives/false negatives khi Rust syntax vượt parser. Fail rõ ràng
và ghi limitation, không widen exemption theo tên để PASS. Rollback qua revert
checker-only commit; không có migration hay database rollback.

#### Bàn giao và review

Forge ghi Agent trailer, Task ID, immutable source/base/head, exact file ownership,
negative regression và validation trong PR. Self-review/merge theo ADR-0010 với
required checks. G5 task này là checker behavior có evidence, không phải macOS
product/browser/Beta/common RC verdict. Aster đồng bộ Issue/Project/TASK_INDEX
sau khi đã kiểm bằng chứng thực tế. Stop-and-Propose nếu cần runtime/contract
delta hoặc ownership ngoài hai file trên.
