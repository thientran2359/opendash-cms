# M7 — Usable Beta integration G3 (4 tasks)

**Coordinator / author:** Atlas · **Date:** 2026-11-16.
**Baseline code:** `d1338d631e1421db075db6f024afabee988de99a`.
**G2:** AGREED · **G3:** READY effective only after Forge review and docs PR merge.
**Execution:** NOT_STARTED · **Runtime / human verification:** NOT_RUN.
[PRD](../../../../product/PRODUCT_REQUIREMENTS.md) ·
[ADR-0017](../../../../decisions/ADR-0017-usable-beta-staging.md) ·
[Final Decision](https://github.com/thientran2359/opendash-cms/issues/292#issuecomment-6917897791) ·
[Planning Workflow](../../../PLANNING_WORKFLOW.md) ·
[task template](../../../templates/SLICE_TASK.md).

## Scope and dependency graph

This is a Windows integration checkpoint across existing capabilities, hosted in M7;
not a new numbered milestone, module, feature candidate or replacement primary acceptance.
No product code/schema/API/ABI changes in these four tasks.
T01 audit → T02 findings disposition + complete G3 for fixes → separately owned fix
Issues → T03 retest/human dogfood → T04 Beta readiness. If T02 finds no required fixes,
its explicit evidence-backed no-fix disposition unlocks T03. New blocking findings
create bounded follow-up fix Issues, not automatic agent loops or reopening old evidence.

Atlas owns coordination and docs; Forge is independent Windows verifier (not author of
Nova UX changes); Nova owns only individually assigned UX fixes. Harbor keeps #188/macOS.
Forge's outstanding Windows receipt for #188 must be completed or explicitly handed off
before claiming T01 if it still occupies his branch. Full #188/macOS/Linux closure is
not an audit prerequisite. #242/#245 remain Forge-owned release work; no simultaneous claim.

G3 completeness does not set Project Ready. Publish all four task Issues after this G3
merges, pin immutable G3 source SHA and code base, keep downstream Backlog until gates.
Create a Beta tracking Issue linking these Issues and any resulting fixes, not a second
live task ledger. The discussion #292 remains a decision record.
At pickup re-fetch main/Issues/Project, record actual candidate SHA and artifact hashes.
A product change from this baseline requires Atlas impact note and explicit candidate
repin before runtime use. Docs-only drift may retain the same product tree with evidence.

## Evidence contract and execution environment

Use native Windows, real built executable with embedded Admin, clean isolated site roots
outside repository website data, loopback only. Build from the pinned candidate:
`powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release`.
Record source/tree SHA, binary SHA256, Admin/package/fixture hashes, tool/OS/browser
versions, commands/exits, timestamps and sanitized artifact manifest. Never commit
passwords, backups, database, private host paths, session tokens or raw user data.
Private artifacts may stay outside Git with hashes and reproducible steps in reports.

Operator provisions binary, test packages and empty root; editor/administrator performs
ordinary authoring in Admin. No dev.py seed or API seeding of the primary fresh-site path.
Use a synthetic Vietnamese studio site: home/about, two posts (one draft), a Project CPT
with text/image field group, category/tag, navigation, uploaded image, theme/template edits
and test plugin data. Existing demo ZIP/fixtures may be imported deliberately in the
separate sample-import path, never prepopulate the primary path. Record exact ZIP hashes.
Two browser sessions/roles: Admin for system configuration, Editor for content.
Desktop edit at 1440x900; preview/public at 768 and 390 widths. Responsive viewport evidence
does not claim real-device or mobile Builder authoring certification.

For each case record PASS/FAIL/NOT_RUN/BLOCKED with actual result, steps, artifact, elapsed
time, help required and reason. No invented timing SLA. Human, agent-driven UI and
automated harness results are separate columns. Audit must attempt reachable cases and
document dependency chains for unreachable ones; it must not hide broken setup by seeding.
A finding records stable ID, severity, case, expected/actual, reproducibility, evidence,
impact and proposed responsible agent. S0 = security/data loss/unrecoverable;
S1 = mandatory journey blocked; S2 = friction with safe UI-only completion;
S3 = cosmetic. S0/S1 cannot be waived by relabeling. Atlas resolves severity disputes.
No security or data-preservation failure accepted even if placed in S2/S3.

## Mandatory case matrix

| Case | Procedure and expected result | Existing primary scope (regression only) |
| --- | --- | --- |
| B01 | Empty root → bootstrap → login → Dashboard/navigation; refresh/deep link/back/logout/login; role-appropriate menus and useful empty/error states | M3 Admin/auth; M7-S09 through S13, FND-05.5 |
| B02 | Admin creates CPT + field group/panel position and site navigation menu; Editor creates/edits Post/Page/CPT, uploads/picks image and permitted category/tag; persisted after restart | M3 content/media; M4 taxonomy; M5 menu model; M7 menu/Admin |
| B03 | Save draft stays off public URL/archive/sitemap/media; authenticated preview; publish then Save updates public. Unsaved preview leaves public unchanged. Builder Save updates homepage. No new draft workflow for other CPT | PRD publication boundary; M3/M4/M5/M7 |
| B04 | Install/activate theme ZIP; separate explicit sample import into nonempty test site preserves existing entry/menu; cancel/duplicate import produces clear feedback without overwrite | M5 theme/sample import |
| B05 | Edit homepage/header/footer/single/archive via Builder/template UI; save/reopen and compare saved public rendering at desktop/tablet/mobile; missing block visible in Admin, safe public fallback | M4 Builder; M5 templates/preservation |
| B06 | Theme switch/update and plugin install/disable/restart preserve CPT/fields/content/user layouts; plugin trap/timeout leaves independent site and Admin recovery usable | M5 preservation; M6 extension/recovery |
| B07 | Dirty navigation, session expiry (401), two-session stale save (412), role downgrade; no silent lost edits or unauthorized draft recovery; Editor direct URL/operation denied for Admin-only work; CSRF/Origin boundaries intact | M3 auth/concurrency; M7-S09 through S13 |
| B08 | Admin backup → encrypted download → restore separate root → operator explicit activation → login/re-edit/public; old root unchanged; restored sessions cleared and outgoing SMTP disabled until explicit re-enable | M8-S03 through S05, S08; ADR-0015 |
| B09 | Wrong password/tamper/missing asset or secret/newer schema/disk-full fail safely without publishing bad restore or damaging source; denied ZIP/permissions have recoverable diagnostics | M8 codec/restore/security |
| B10 | Final candidate regress affected SEO/contact/video/public ACL and existing automated security checks; complete V1 native/performance obligations remain tracked separately | M7 site features; M8 native release |

Existing harnesses may support negatives without requiring a human to corrupt backups:
`python cms/scripts/admin_nav_single_edit_e2e.py --binary cms/target/release/opendash.exe`
and `python cms/scripts/m8_browser_e2e.py --binary cms/target/release/opendash.exe`.
Both create test state; their output is **separate** from the primary unseeded walkthrough.
Inspect harness flags and record actual isolated paths/artifact locations before execution.
Use existing Rust M8 codec/restore/review tests for hostile fixtures, and auth/plugin tests
for technical negatives; record exact test targets and candidate, never infer all negatives
from an unrelated PASS. Harness gaps are explicit findings, not permission to edit scripts.
At T01, missing fixture/runner is NOT_RUN with a resolution owner. At T03/T04 every
required negative needs applicable current-candidate evidence or Beta remains blocked.

## Shared task rules

All four tasks consume this full spec, PRD, ADR-0017 and existing M8 contract.
One writer for verification index and planning files; Atlas integrates links into
`docs/verification/README.md` at T04. Forge writes only his report paths below.
Artifacts are private/untracked. No code/script changes, production data, deployment,
license choice, issue auto-close via PR keywords, native-gate waiver or agent dispatch.
Report-only tasks may finish with an honest blocked product verdict; Beta PASS is separate.
Docs checks: `python cms/scripts/check-plans.py`,
`python cms/scripts/check-plans.py --self-test`,
`python .agents/scripts/check-agent-artifacts.py`, `git diff --check`.
Reports identify the tested candidate separately from their documentation commit.
Rollback is stopping the owned test process and retaining isolated recovery copies;
never reset/delete a real website, overwrite live DB or down-migrate.
Before each commit use per-worktree identity and `Agent: Forge` or `Agent: Atlas`.
PR receipt includes reviewed head, checks, evidence, limits and Issue link.

### M7-S14-T01 — Audit Windows từ site trống và lập baseline usability

- **Thuộc:** V1 internal Beta · M7 · M7-S14
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; M7-S14 chỉ integration regression, không primary mới
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Evidence/planning only, không đổi runtime API/schema/security
- **G3:** READY
- **Trạng thái:** PLANNED; hiệu lực sau docs merge, không tự Project Ready
- **Nguồn quyết định:** ADR-0017 / Issue #292 và toàn bộ shared contract của tài liệu này
- **Người triển khai / review:** forge / Atlas; coordinator Atlas

#### Tổng quan và giá trị

Xác định chỗ người vận hành không hoàn thành được website qua Admin, thay vì suy từ demo và unit tests.

#### Mục tiêu và hành vi quan sát được

Có report B01–B10 tái lập được trên native Windows, phân biệt lỗi sản phẩm, thiếu fixture và trợ giúp cần thiết.

#### Phạm vi công việc

Chạy walkthrough primary fresh-site và sample-import riêng; thực hiện các negative có fixture; ghi baseline, screenshots và findings. Đây là audit đọc sản phẩm nhưng có thao tác ghi trên site thử.

#### Ngoài phạm vi

Không sửa product/harness; không human usability certification, full native release hoặc claim Beta PASS.

#### Phụ thuộc và điều kiện chặn

G3 này merged sau Forge review, Issue giao Forge và Project Ready; clean/free Forge branch, Windows binary/build tools/browser và isolated roots. Atlas xác nhận #188 Windows receipt không còn chiếm slot (receipt hoàn tất hoặc handoff rõ), không đòi #188 full closure. Chưa có human tester không chặn T01.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-16-usable-beta-windows-audit.md`

Chỉ sửa các file trên; private evidence ngoài Git. Không viết shared product wiring.
Đọc toàn bộ evidence/matrix/shared rules phía trên trước claim.

#### Cách triển khai theo contract

Pin source/artifacts; ghi setup trước khi chạy. Thực hiện B01–B10 từ browser; khi bị chặn ghi finding và chỉ tiếp tục các case độc lập, không dùng API workaround để ghi PASS. Nộp report kèm artifact manifest và danh sách findings để Atlas triage.

#### Nghiệm thu

- [ ] Report đủ B01–B10: đã thử case reachable; unreachable có prerequisite failure/evidence và người xử lý.
- [ ] Không SQL/API/edit-file cho đường biên tập chính; mọi trợ giúp/workaround ghi riêng, không biến thành PASS.
- [ ] Findings S0–S3 có expected/actual, steps, ảnh/hashes và scope; trạng thái human/Linux/macOS rõ NOT_RUN khi chưa chạy.
- [ ] Report được Atlas review trên SHA; audit Done chỉ chứng minh khảo sát hoàn thành, không Beta hay native G5.

#### Kiểm thử, rủi ro và rollback

Chạy build release và hai harness tham chiếu nếu môi trường hỗ trợ, ghi đúng kết quả; manual browser và negative fixtures theo matrix. Thiếu môi trường/fixture ghi NOT_RUN, không tự sửa code. Chạy docs checks trước report PR. Stop-and-Propose nếu cần thay contract. Rollback giữ test copies,
không xóa site gốc hay down-migrate. Không có migration trong task.

#### Bàn giao và review

PR report/spec với Task ID, tested SHA khác docs commit khi áp dụng, artifact hashes,
commands/exits và limits; Atlas review exact head. Không closing keywords trước
coordinator xác minh task acceptance; không tự đổi Project Done từ merge.

### M7-S14-T02 — Phân loại findings và hoàn thiện G3 các sửa lỗi Beta

- **Thuộc:** V1 internal Beta · M7 · M7-S14
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; M7-S14 chỉ integration regression, không primary mới
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Evidence/planning only, không đổi runtime API/schema/security
- **G3:** READY
- **Trạng thái:** PLANNED; hiệu lực sau docs merge, không tự Project Ready
- **Nguồn quyết định:** ADR-0017 / Issue #292 và toàn bộ shared contract của tài liệu này
- **Người triển khai / review:** atlas / Forge; coordinator Atlas

#### Tổng quan và giá trị

Biến kết quả audit thành các việc nhỏ có acceptance và ownership, không giao Nova một yêu cầu sửa UX mơ hồ.

#### Mục tiêu và hành vi quan sát được

Mọi finding T01 có disposition và Issue/G3 fix hoặc lý do không cần sửa; downstream biết chính xác dependency.

#### Phạm vi công việc

Triage S0/S1 trước S2/S3; chốt Design Delta và Fast/Full-Track; chuẩn bị tất cả fix đã xác định, dependency và file ownership; Nova mặc định UI fixes, backend giao riêng.

#### Ngoài phạm vi

Không triển khai fix trong task này; không hạ security/data hoặc tự chốt thêm G0 ngoài PRD.

#### Phụ thuộc và điều kiện chặn

T01 report merged và Atlas review; evidence đủ tái lập finding. Chưa có product fix không chặn việc chuẩn bị G3. Issue/Project Ready và Atlas branch free.

#### Kỹ thuật và file ownership

- `docs/plans/milestones/M7/slices/2026-11-16-usable-beta-fixes-g3.md`
- `docs/plans/milestones/M7/TASK_PLAN.md`
- `docs/plans/TASK_INDEX.md`

Chỉ sửa các file trên; private evidence ngoài Git. Không viết shared product wiring.
Đọc toàn bộ evidence/matrix/shared rules phía trên trước claim.

#### Cách triển khai theo contract

Lập bảng finding→disposition→task/source→responsible→files→acceptance. G3 từng fix theo template repo, nếu API/schema/security thay đổi phải có G1/G2/contract PR trước Ready. IDs mới kiểm trên main; không tạo task ID cho lỗi giả. Product thuộc milestone khác có G3 tại đúng milestone sau Atlas ghi ownership mở rộng riêng, không dùng quyền task này sửa ngoài ba file. Xuất fix Issues từ spec merged; giữ dependency chưa đạt ở Backlog.

#### Nghiệm thu

- [ ] 100% findings có disposition có căn cứ; không bỏ security/data bằng cosmetic label.
- [ ] Mọi fix cần thiết có G3 đầy đủ, exact files, negative regression và dependency; chỉ Issue đủ gate Ready.
- [ ] T03 nhận danh sách fix bắt buộc với acceptance/evidence links; nếu không có fix, có explicit no-fix verdict và lý do.
- [ ] Forge review delta; code, ADR cũ và evidence cũ không bị sửa để làm PASS.

#### Kiểm thử, rủi ro và rollback

Đối chiếu từng finding với T01 và source thật; chạy docs checks. Không tuyên bố runtime PASS; nếu cần đổi contract ngoài ownership thì ghi blocker và PR riêng trước phát hành fix Ready. Stop-and-Propose nếu cần thay contract. Rollback giữ test copies,
không xóa site gốc hay down-migrate. Không có migration trong task.

#### Bàn giao và review

PR report/spec với Task ID, tested SHA khác docs commit khi áp dụng, artifact hashes,
commands/exits và limits; Forge review exact head. Không closing keywords trước
coordinator xác minh task acceptance; không tự đổi Project Done từ merge.

### M7-S14-T03 — Kiểm lại candidate Windows và dùng thử với người không viết code

- **Thuộc:** V1 internal Beta · M7 · M7-S14
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; M7-S14 chỉ integration regression, không primary mới
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Evidence/planning only, không đổi runtime API/schema/security
- **G3:** READY
- **Trạng thái:** PLANNED; hiệu lực sau docs merge, không tự Project Ready
- **Nguồn quyết định:** ADR-0017 / Issue #292 và toàn bộ shared contract của tài liệu này
- **Người triển khai / review:** forge / Atlas; coordinator Atlas

#### Tổng quan và giá trị

Xác minh fixes giải quyết hành trình thật, tách kiểm tra kỹ thuật khỏi khả năng người dùng tự thao tác.

#### Mục tiêu và hành vi quan sát được

Một candidate cố định có B01–B10 evidence và ít nhất một human cold walkthrough, log trợ giúp/thời gian rõ.

#### Phạm vi công việc

Rerun must-pass/negative và regressions bị ảnh hưởng; human dùng brief synthetic studio site, Forge quan sát và ghi. Atlas điều phối người thử không phải tác giả implementation, không mặc định owner.

#### Ngoài phạm vi

Không sửa product tại chỗ; không deployment, full V1 verdict, thu thập PII hoặc gọi agent browser là human test.

#### Phụ thuộc và điều kiện chặn

T02 merged, mọi required fix Issue có G5/evidence và không còn S0/S1 đã biết; Atlas pin candidate và impact matrix; human tester sẵn sàng, được thông báo recording phạm vi; Issue Ready, Forge free. Thiếu tester giữ Backlog/BLOCKED, vẫn cho independent fix work tiếp tục.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-16-usable-beta-dogfood.md`

Chỉ sửa các file trên; private evidence ngoài Git. Không viết shared product wiring.
Đọc toàn bộ evidence/matrix/shared rules phía trên trước claim.

#### Cách triển khai theo contract

Operator chuẩn bị binary/ZIPs/root trống và runbook, không soạn sẵn nội dung. Human theo brief B01–B08, technical negatives B07/B09/B10 bởi Forge/harness ghi riêng. Đo thời gian và help; bất kỳ cần SQL/API/file workaround là hành trình không đạt. Lỗi mới mở Issue có reproduction, Atlas chuẩn bị G3 follow-up trước sửa; không sửa khi đang đo rồi gọi cùng candidate.

#### Nghiệm thu

- [ ] Ít nhất một human không viết code có receipt sử dụng (ẩn danh), log steps/help/time; agent và automated evidence tách riêng.
- [ ] B01–B10 đủ kết quả trên candidate pin, negative evidence đủ, không ghép SHA tùy tiện.
- [ ] Các must-pass FAIL hoặc required NOT_RUN dẫn product verdict BLOCKED và Issue có owner; report không che lỗi.
- [ ] Candidate đổi thì affected rerun và fresh end-to-end lại trước Beta PASS; preserving old evidence.

#### Kiểm thử, rủi ro và rollback

Build/harness/manual cases theo shared contract, record real commands và binary hash; chạy docs checks. Chỉ test roots, không private/live site. Human consent thiếu thì không recording, ghi thiếu evidence thay vì giả. Stop-and-Propose nếu cần thay contract. Rollback giữ test copies,
không xóa site gốc hay down-migrate. Không có migration trong task.

#### Bàn giao và review

PR report/spec với Task ID, tested SHA khác docs commit khi áp dụng, artifact hashes,
commands/exits và limits; Atlas review exact head. Không closing keywords trước
coordinator xác minh task acceptance; không tự đổi Project Done từ merge.

### M7-S14-T04 — Nghiệm thu Usable Beta và bàn giao common RC

- **Thuộc:** V1 internal Beta · M7 · M7-S14
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; M7-S14 chỉ integration regression, không primary mới
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Evidence/planning only, không đổi runtime API/schema/security
- **G3:** READY
- **Trạng thái:** PLANNED; hiệu lực sau docs merge, không tự Project Ready
- **Nguồn quyết định:** ADR-0017 / Issue #292 và toàn bộ shared contract của tài liệu này
- **Người triển khai / review:** atlas / Forge; coordinator Atlas

#### Tổng quan và giá trị

Có quyết định Beta rõ evidence, không đồng nhất Done của task với V1 sẵn phát hành.

#### Mục tiêu và hành vi quan sát được

Báo cáo READY_FOR_INTERNAL_BETA hoặc BETA_BLOCKED, mapping current evidence và release work còn lại.

#### Phạm vi công việc

Review T01/T02/T03, fix Issues, candidate manifest, open findings; đối chiếu Project live; cập nhật index/docs và handoff native common RC.

#### Ngoài phạm vi

Không đóng #248/#215/#157/#188/#214/#246 theo Beta, không license decision/public release hay tự chứng nhận Linux/macOS.

#### Phụ thuộc và điều kiện chặn

T03 report merged, all required fixes G5 và evidence cùng candidate; Issue Ready/Atlas branch free. Nếu còn blocker chỉ phát hành audit verdict BETA_BLOCKED, không cấp Beta PASS.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-16-usable-beta-readiness.md`
- `docs/verification/README.md`
- `docs/plans/TASK_INDEX.md`
- `docs/plans/ROADMAP_V1.md`
- `docs/plans/milestones/M7/README.md`

Chỉ sửa các file trên; private evidence ngoài Git. Không viết shared product wiring.
Đọc toàn bộ evidence/matrix/shared rules phía trên trước claim.

#### Cách triển khai theo contract

So B01–B10 với candidate và human receipt. Review all open findings/current Project; verdict phải cite source/hashes và missing cases. Chỉ READY_FOR_INTERNAL_BETA khi mọi mandatory case PASS và không security/data/recovery/journey blocker. Handoff #242/#245/#246/#247/#248 với native/source requirements chưa thay đổi; public release vẫn owner authorized.

#### Nghiệm thu

- [ ] Verdict có ma trận đầy đủ, không required NOT_RUN/FAIL được chấp nhận khi READY_FOR_INTERNAL_BETA.
- [ ] Known cosmetic issues có owner và giới hạn; security/data blocker không được risk-accept.
- [ ] Done của task audit chỉ nói đã ra report; Beta tracker chỉ Done khi Beta PASS; các native/milestone tasks giữ gate riêng.
- [ ] Forge review report; links/index đúng live state và evidence; không claim deploy hoặc human approval không có.

#### Kiểm thử, rủi ro và rollback

Read/review evidence trên exact candidate/PR heads, Project/CI trực tiếp; không cần rerun mọi test nếu không có delta, nhưng stale/missing evidence phải trả BLOCKED. Chạy docs checks. Stop-and-Propose nếu cần thay contract. Rollback giữ test copies,
không xóa site gốc hay down-migrate. Không có migration trong task.

#### Bàn giao và review

PR report/spec với Task ID, tested SHA khác docs commit khi áp dụng, artifact hashes,
commands/exits và limits; Forge review exact head. Không closing keywords trước
coordinator xác minh task acceptance; không tự đổi Project Done từ merge.

