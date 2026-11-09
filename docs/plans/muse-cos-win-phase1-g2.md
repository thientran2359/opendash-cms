# Muse → CoS Windows — Phase 1: G2 AGREED / G3 APPROVED (P0–P4)

- **Quyết định chủ dự án:** **G2 Phase 1 APPROVED / design = AGREED**, 2026-11-09. Phạm vi phê duyệt **chỉ P0–P4** và phép thử `TEST-PONG-01`; không phê duyệt Phase 2/P5.
- **Track:** Full-Track cho phần hạ tầng điều phối (GitHub auth, local CoS Control API, secrets, durable receipt, failure recovery); **không thuộc code runtime CMS**.
- **Scope:** AGREED · **Research:** REVIEWED · **Design/G2:** AGREED (chỉ Phase 1) · **G3 Breakdown:** APPROVED bởi chủ dự án (09/11/2026), sau [Nova Round 4 — READY_FOR_G3_APPROVAL](https://github.com/thientran2359/opendash-cms/pull/4) · **Implementation Issues:** chưa có task READY · **Implementation:** NOT_STARTED · **Verification:** NOT_RUN.
- **Nguồn:** [PR #4](https://github.com/thientran2359/opendash-cms/pull/4), [Nova Round 2 — READY_FOR_G2_APPROVAL](https://github.com/thientran2359/opendash-cms/pull/4#pullrequestreview-5823313832), [architecture research tại baseline đã review](https://github.com/thientran2359/opendash-cms/blob/edea54d94dd6be457f75159e407e87b90466f820/.agents/research/muse-cos-win-orchestrator-poc.md), [test matrix tại baseline](https://github.com/thientran2359/opendash-cms/blob/edea54d94dd6be457f75159e407e87b90466f820/.agents/research/muse-cos-win-poc-test-matrix.md).
- **Kênh thử nghiệm duy nhất:** [GitHub Issue #5 — TEST-PONG-01](https://github.com/thientran2359/opendash-cms/issues/5), tạo để phối hợp/test; **không phải Issue implementation G3**, chưa có tín hiệu dispatch nào được phát.

## 1. Quyết định và ranh giới quyền

Chủ dự án đã **chọn Outbound GitHub Poller**: Muse phát tín hiệu có định danh trên GitHub; Windows Poller chủ động đọc qua HTTPS outbound, xác minh tín hiệu, sau đó gọi CoS Local Control API tại `127.0.0.1`. Poller ghi receipt/status rút gọn về đúng GitHub Issue để Muse đọc. **Không triển khai Inbound Relay, Cloudflare Tunnel/Access, public endpoint** trong Phase 1. Nếu Poller không khả thi trong thực nghiệm, báo BLOCKED; không tự chuyển transport.

Chấp nhận tạo main chat mới ở **đúng CoS Project/worktree Forge**; **không yêu cầu** ChatGPT Web Project mapping. Bắt buộc xác minh `projectId` UUID ↔ canonical worktree và không gửi vào worker/helper hoặc chat khác. Mỗi lần thử chỉ PONG cố định, một active job, quyền tối thiểu. Không nhận prompt tự do, không cho chat thử quyền sửa file/terminal, không chạy lệnh CMS, không tạo PR code/deploy/merge.

**Phân quyền:** chủ dự án phê duyệt G2 Phase 1; Forge sở hữu nghiên cứu/triển khai Windows Poller **khi có task G3 riêng với file ownership**; Muse chỉ phát tín hiệu và quan sát qua GitHub theo quyền đã cấp; Nova review kiến trúc và evidence. GitHub Issue #5 là kênh phối hợp, không tự cấp task G3 `READY` hoặc quyền thao tác CoS.

## 2. Contract/acceptance đã chốt cho Phase 1

| Lớp | Quy tắc bắt buộc |
| --- | --- |
| Signal | Chỉ lấy từ repo và **Issue #5** được allowlist. Mỗi tín hiệu có GitHub comment/timeline **signal ID cụ thể**, actor đã phê duyệt, `task_id=TEST-PONG-01`, `target=forge/windows`, mode PONG và `request_id` gắn/dẫn xuất tất định từ signal ID. Không lấy snapshot label/Issue body làm chứng cứ actor hoặc lệnh. |
| Auth/integrity | Kiểm `user.login`/`actor.login` theo allowlist, quyền actor và `author_association` nếu là comment; với comment yêu cầu `created_at == updated_at`. Pin payload hash và re-fetch/check actor + hash trước dispatch. Từ chối signal giả/sửa sau khi phát. |
| Transport | Chỉ GitHub HTTPS outbound + CoS loopback. Conditional polling dùng `ETag` / `If-None-Match`, xử lý `304`/backoff/secondary limits; không mở HTTP relay inbound. |
| Dedup/receipt | Ledger bền vững cục bộ với unique `(repo_id, github_signal_id)` và `(repo_id, request_id)`, gắn input UUID + payload hash. Trùng hợp lệ = receipt cũ, trùng xung đột = `CONFLICT_PAYLOAD_MISMATCH`/`BLOCKED`, **0 CoS send**; không có HTTP 409. |
| Crash/recovery | Commit+flush `DISPATCHING_UNKNOWN` trước POST CoS; timeout/crash => reconcile theo `/v1/inputs` và `/v1/sessions/{input_uuid}/events`, không gửi lại mù. CLI operator local có audit, không mở qua GitHub. |
| CoS identity | P0 xác minh cặp file user-data `control-api/endpoint.json` + `control-api/token` riêng (đường dẫn đúng theo bản cài), PID/start time, health, `Allow actions`, port/token rotation và `projectId` ↔ canonical Forge worktree. Sai/lệch/mất quyền => fail-closed. |
| Lifecycle | Supervisor chặn dispatch khi CoS offline; mục tiêu **≤5 giây** từ khi CoS đóng/mất health, đo bằng runtime test. Khi reconnect không replay `UNKNOWN`. |
| Output | CoS HTTP `202` chỉ là queued; chỉ sau khi quan sát assistant `PONG` + turn end mới báo responded. GitHub chỉ nhận status/error code/receipt đã redacted, không raw chat/token. |

**Các thông số chờ P0/G3 xác minh và chốt trước khi chạy signal thật:** GitHub comment hay timeline event cụ thể, allowlist actor/app và quyền repository, cách tạo/pin `request_id`, quyền ghi receipt, polling interval phù hợp rate limit, project UUID/path thật, local secret storage/ACL, local operator CLI/supervisor startup. Đây là những **đầu ra bắt buộc của thiết kế chi tiết/khảo sát**, không phải quyền tự suy diễn/thay đổi boundary phía trên.

## 3. Cổng thực nghiệm và bằng chứng

| Gate | Điều kiện nghiệm thu (tất cả hiện NOT_RUN) |
| --- | --- |
| **P0 — Discovery** | CoS API/token/project mapping và quyền GitHub Muse/Windows có evidence; schema PONG/Issue #5/actor đã allowlist và rõ cách ghi receipt |
| **P1 — Local PONG** | CoS nhận một input UUID mới, đúng main chat của Forge/CoS Project, có assistant PONG + `turn_end`; không worker/helper |
| **P2 — Poller security/recovery** | Mock và integration negative cases (actor giả, comment sửa, payload conflict, replay, idempotency, 304/ETag, crash/reconcile, redaction, 0 inbound listener) PASS |
| **P3 — Lifecycle** | CoS open/close/restart/rotate/health fail và GitHub API lỗi không gây send trùng hoặc rò token; chặn dispatch ≤5 giây khi CoS offline |
| **P4 — Muse round-trip** | Muse phát đúng một signal được duyệt trên Issue #5; Poller → CoS main chat → PONG; Muse đọc receipt kết quả trên GitHub; trace ID/evidence đầy đủ |

Từng case cần expected/actual, thời điểm, Windows/CoS phiên bản, commit, outcome và redacted evidence tại `docs/verification/`, qua PR/CI/review. **Không đổi verification thành VERIFIED vì tài liệu/PR đã duyệt**. Mọi runtime P0–P4 hiện **NOT_RUN**.

## 4. Bước tiếp theo và điều kiện chặn

1. Tài liệu G2 Phase 1 được review và merge theo [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md); **chấp thuận trong hội thoại không tự merge PR #4**.
2. Chuẩn bị **G3 breakdown/task riêng** cho khám phá P0/P1 và Poller P2/P3/P4: mục tiêu, file ownership, baseline SHA đã merge, quyền, dependencies và test/rollback. Chỉ chuyển implementation Issue `READY` khi đủ G3 theo [Planning Workflow](PLANNING_WORKFLOW.md).
3. Định danh **Issue #5** là kênh coordination duy nhất, **trạng thái `NO_DISPATCH`** cho đến khi chốt schema/actor/permissions và CoS local preflight. Tạo Issue không phải tín hiệu PONG.
4. Sau runtime evidence P0–P4 được review thành `VERIFIED`, mới xem xét một **G2 Phase 2 riêng** cho P5 (GitHub Issue G3 thật, code changes, PR/G5). Không tự cấp Phase 2 từ kết quả PONG.

**Quyết định này không cho phép:** tự động dispatch mã nguồn, tạo agent worker, mở Cloudflare/inbound, thay đổi GitHub governance, bỏ qua giới hạn provider hoặc triển khai/deploy hệ thống.

## 5. G3 Breakdown — 8 work packages đã được phê duyệt

**Tình trạng:** **G3 BREAKDOWN APPROVED (09/11/2026)** theo quyết định chủ dự án sau khi Nova xác nhận G3-01..04 RESOLVED và `READY_FOR_G3_APPROVAL` tại HEAD `6864e7c`. Phê duyệt áp dụng cho **cấu trúc và scope W01–W08**, **không phải lệnh thực thi**; không tự chuyển GitHub implementation Issue nào sang `READY`. Từng work package chỉ có thể thành Issue `READY` **sau khi PR #4 review/CI/merge vào `main`**, pin source/base SHA, owner/file ownership, dependencies và gate riêng.

**Quy ước ID và cách phát hành Issue (G3-01):** `P1-W01..P1-W08` là **ID work package hạ tầng POC đã được duyệt G3**, giữ ID ổn định trong phạm vi POC, **không phải Task ID sản phẩm** `Mx-Sxx-Txx`. Poller không phải module/capability M3 và không nhận GitHub milestone M3 giả. **Chọn cách phát hành thủ công cho POC**: coordinator tạo Issue implementation riêng bằng [mẫu Implementation Issue](../../.github/ISSUE_TEMPLATE/implementation.yml), tiêu đề `[P1-Wxx] <Tên work package>`, nhãn `type:implementation`, `agent:forge`, `area:agent-workflow`; điền đủ **8 mục** (tổng quan, mục tiêu, phạm vi, ngoài phạm vi, acceptance checklist, dependency, kỹ thuật/ownership, kiểm thử/rủi ro/rollback), đường dẫn nguồn **blob cố định theo `source_sha` đã merge vào `main`**, `base_sha`, G2/G3 và evidence/dependency. Không gán module/capability hoặc milestone CMS giả. **Không dùng `.agents/scripts/prepare-github-issue.py` cho `P1-Wxx`**: tool này hiện chỉ hỗ trợ ID `Mx-Sxx-Txx`, source `docs/plans/{modules,milestones,tasks}` và metadata module. Đối chiếu Issue open/closed theo ID trước khi tạo để tránh trùng; tạo Issue không kích hoạt dispatch. [Issue #5](https://github.com/thientran2359/opendash-cms/issues/5) giữ vai trò **coordination/signal test**, không được tái dùng làm Issue implementation.

**Track:** Full-Track xuyên suốt vì có credential GitHub/CoS, local Control API, durable SQLite ledger, security boundary và crash recovery. **Đầu vào cố định:** G2 Phase 1 tại chính tài liệu này, [test matrix](../../.agents/research/muse-cos-win-poc-test-matrix.md), [GITHUB_WORKFLOW](../../.agents/GITHUB_WORKFLOW.md) và commit `origin/main` đã merge khi từng task bắt đầu. **Kết quả cần đạt:** một tín hiệu `TEST-PONG-01` hợp lệ từ Muse đi qua Issue #5 tới đúng Forge main chat, phản hồi PONG được xác minh và receipt đã redacted; thử lỗi và restart không gây gửi trùng.

### 5.1. Thứ tự và dependency

| Work package (G3 APPROVED, Issue NOT_READY) | Gate | Owner / reviewer | Dependency | Đầu ra có thể nghiệm thu |
| --- | --- | --- | --- | --- |
| **P1-W01 — Khảo sát CoS Windows** | P0 | Forge / Nova | G2 Phase 1 merged; chủ máy cấp quyền đọc local | Evidence CoS API/permissions, `endpoint.json`+`token`, `projectId`↔worktree đúng |
| **P1-W02 — Chốt GitHub signal/permissions** | P0 | Forge + Muse / Nova | G2 merged; Issue #5 tồn tại; W01 P0 evidence merge trước khi sync brief | Contract signal ID/actor/receipt, least-privilege quyền, ETag/rate-limit |
| **P1-W03 — CoS adapter + Local PONG** | P1 | Forge / Nova | W01 PASS + W01/W02 evidence/schema sync merged `main` | Client loopback chỉ gửi PONG cố định, xác nhận main chat/session events |
| **P1-W04 — GitHub Poller read/validate** | P2 | Forge / Nova | W02 contract và brief sync merged `main` | Đọc event bằng ID, auth/integrity, conditional GET, reject nguồn lạ |
| **P1-W05 — Durable ledger + dispatcher** | P2 | Forge / Nova | W03 + W04 | Exactly-once admission attempt, dedup/recovery, redacted receipt |
| **P1-W06 — Local supervisor + operator controls** | P3 | Forge / Nova | W05 | Fail-closed lifecycle CoS, token rotation, manual resolve/kill switch |
| **P1-W07 — Muse → GitHub → CoS round-trip** | P4 | Forge (Windows) + Muse (signal) / Nova | W06 và P0–P3 evidence PASS | PONG từ Muse→Forge→GitHub receipt, 1 UUID/session, không sửa CMS |
| **P1-W08 — Verification/hardening Phase 1** | P0–P4 / G5 evidence | Forge / Nova độc lập | W07 | Báo cáo test, negative/restart/rate limit, PASS/FAIL và residual risk |

**Staged G3 activation (G3-03):** ở lượt mở task đầu tiên, chỉ **W01 và W02** có thể được xem xét chuyển `READY` (từng task vẫn cần gate/Issue/ownership riêng sau PR #4 được merge). W01/W02 độc lập về nội dung nhưng dùng **một writer trên `agent/forge`**, thứ tự khuyến nghị W01 → W02. Sau khi W01 hoàn thành và **evidence P0 CoS được review/merge vào `main`**, W02 chốt GitHub signal contract dựa trên kết quả thực; cùng PR W02 hoặc một lượt tài liệu rõ owner, **đồng bộ schema/hằng số thực tế từ W01/W02 vào các brief W03–W07 ở §5.2**, không đổi G2 security boundary. Phải review/merge cả **P0 CoS evidence** và **GitHub signal contract + P0 evidence + brief sync** trước khi xem xét **W03–W08** chuyển `READY`. Khi mở từng task sau P0, pin lại `source_sha` từ commit `origin/main` mới chứa thông số đã xác minh và `base_sha` code phù hợp; **không** lấy SHA PR #4 cũ, không mở sớm W03/W04 chỉ vì bản G2 ban đầu AGREED. Nếu thông số P0 đòi đổi G2 contract/trust boundary, **Stop-and-Propose** qua review và phê duyệt trước khi cập nhật brief.

Các lượt triển khai đề xuất: **W01 → W02 → (P0 sync reviewed/merged) → W03 → W04 → W05 → W06 → W07 → W08**. Các task phải tuân thủ dependency/evidence riêng trong bảng và một active implementation task trên branch. Không chia hai người đồng thời ghi cùng file/ledger, không dùng worker/helper để thay Forge main chat.

### 5.2. Task briefs — phạm vi, file ownership và acceptance

**Định hướng implementation để review ở G3 (G3-04):** dùng Python 3 đang có trong repo, ưu tiên `sqlite3` + HTTP client và `unittest` từ standard library; **không thêm dependency, Windows service hoặc quyền OS mới** nếu chưa được review. Unit tests **100% chạy được trên Ubuntu lẫn Windows**, giả lập GitHub/CoS HTTP, PID/process inspection và filesystem/token rotation qua mock/fake; mọi SQLite/ETag cache/lock/audit-log test ghi trong `tempfile.TemporaryDirectory()` **thuộc system temp ngoài repository**, không ghi runtime dưới `.agents/`. Lệnh chuẩn **sau khi có test**: `python -m unittest discover -s .agents/scripts/muse_poller/tests -p 'test_*.py' -v`. **W03 sở hữu việc bổ sung test step vào `.github/workflows/governance.yml`** (cả `ubuntu-latest` và `windows-latest`) ngay khi suite đầu tiên tồn tại; các W04–W08 phải chứng minh step CI PASS cho test mới, không chỉ chạy local. Tests thực nghiệm CoS/P4 đòi credential/máy Windows của chủ dự án **không** tự kích hoạt trong CI. `git diff --check`, `python cms/scripts/check-plans.py`, `python .agents/scripts/check-agent-artifacts.py` là regression docs. Nếu môi trường Python/CoS không đáp ứng thì Stop-and-Propose, không giả định test đã chạy.

#### P1-W01 — Khảo sát CoS Windows và preflight P0

- **Giá trị/mục tiêu:** chứng minh API trên **CoS bản đang cài** thực sự có thể tạo main chat đúng workspace; không dựa hoàn toàn vào upstream HEAD. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** xác minh app version; Local Control API/Allow actions do chủ máy bật; file user-data `control-api/endpoint.json` và `control-api/token` riêng; PID/creation time, health/actions thực tế; chứng minh bằng UI/config hoặc API **nếu có** `projectId` UUID ↔ canonical Forge root/worktree. Không đọc/chat dump phiên riêng tư.
- **Ownership lượt này:** **chỉ** `docs/verification/muse-cos-win-phase1-p0-cos.md` (ghi evidence đã redacted). Không sửa CMS, tool runtime, file cấu hình CoS hoặc tài liệu G2. **Phần ánh xạ P0→brief W03–W07 sẽ do W02 cập nhật sau khi evidence W01 đã review/merge**, không mở rộng W01 ownership ngầm.
- **Acceptance/test:** P0-01/02/03 có expected/actual, phản ứng khi API/actions bị tắt, port/token stale, sai project → `BLOCKED`; không gửi input ở task này. Ghi API/route thực có, không tưởng tượng endpoint project-list.
- **Rủi ro/rollback:** Local API có quyền đọc chat rộng — chỉ kiểm health/metadata tối thiểu, không log token/ID riêng tư; nếu thiếu quyền hoặc mapping không chứng minh được, dừng tại `BLOCKED`, tắt các công cụ thử nghiệm; không tự bật Allow actions.

#### P1-W02 — Chốt GitHub Issue #5 signal contract và quyền P0

- **Giá trị/mục tiêu:** xác định chính xác **tín hiệu nào** của Muse đủ tin cậy để Windows Poller nhận, trước khi cho phép dispatch. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** kiểm Muse GitHub connector và GitHub API; đề xuất/chứng minh loại **comment hoặc timeline event** trên Issue #5, actor allowlist + quyền repo, kiểm `user.login`/`actor.login`, `author_association` và `created_at == updated_at` nếu dùng comment; `github_signal_id`, deterministic `request_id`, ETag/304, token scope Windows và receipt writer. Xác định ai duyệt signal và cơ chế `NO_DISPATCH → armed` **không dựa trên comment bất kỳ**.
- **Ownership lượt này:** `docs/plans/muse-cos-win-phase1-github-signal-contract.md` (contract bổ trợ cần Nova/chủ dự án review trước khi dùng), `docs/verification/muse-cos-win-phase1-p0-github.md` và **chỉ phần thông số/schema của W03–W07 tại §5.2 trong `docs/plans/muse-cos-win-phase1-g2.md`** để đồng bộ với evidence W01 + W02. Không thay G2 scope/permission/trust boundary; mọi G2 delta phải Stop-and-Propose, review/chốt riêng. Không chỉnh Issue #5 thành signal, không viết Poller code.
- **Acceptance/test:** P0-04/05 chứng minh quyền đọc/ghi GitHub và actor identity trên event cụ thể; diễn giải idempotent ack và permission denied/403/429, sửa comment hoặc label lạ → từ chối; GitHub auth secrets tuyệt đối không ghi vào docs/Issue.
- **Rủi ro/rollback:** nếu Muse không có quyền phát signal hoặc Windows không thể đọc/ack thì ghi `BLOCKED`; không tự chuyển Inbound Relay. Nếu contract mới đụng G2 trust boundary thì **Stop-and-Propose**, yêu cầu review trước khi triển khai.

#### P1-W03 — CoS loopback adapter và P1 Local PONG

- **Giá trị/mục tiêu:** có client tối thiểu để một input UUID tạo **một** main chat Forge và nhận kết quả PONG có bằng chứng. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** typed local client cho `GET /v1/health`, `POST /v1/inputs`, `GET /v1/inputs`, `GET /v1/sessions/{id}/events`; giữ token Windows local, pin CoS project/worktree từ W01; prompt PONG **hard-coded**; không lấy text từ GitHub.
- **Ownership lượt này:** `.agents/scripts/muse_poller/cos_client.py`, `.agents/scripts/muse_poller/tests/test_cos_client.py`, `docs/verification/muse-cos-win-phase1-p1-local-pong.md` và `.github/workflows/governance.yml` **chỉ để thêm bước chạy unit tests Poller bằng Python 3.11 trên Ubuntu/Windows**. Có thể thêm `__init__.py` **chỉ** trong package trên nếu cần import. Không sửa CI job names/branch protection hoặc check governance hiện hữu.
- **Acceptance/test:** P1-01..07: HTTP 202 ≠ completed; PONG assistant event+`turn_end` và input UUID/session ID; sai UUID/project, disabled actions, timeout, model override sai đều fail-closed; không đụng existing chat, không worker, không file/terminal tools. **CI Ubuntu/Windows chạy `unittest discover` với mocks và PASS**, không gửi live PONG từ CI.
- **Rủi ro/rollback:** chỉ gửi phép thử sau khi chủ máy cho phép và W01 PASS; nếu delivery `UNKNOWN` không tạo UUID mới để retry; dừng local client, không để background service.

#### P1-W04 — GitHub Poller reader + event validation P2

- **Giá trị/mục tiêu:** Poller **đọc đúng signal**, không biến bất kỳ Issue/comment thành lệnh điều khiển máy. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** GitHub HTTPS authenticated GET, conditional `ETag`/`If-None-Match`, xử lý 304/403/429/secondary limits; canonical event identity/actor/allowlist, comment edited detection, re-fetch/payload hash trước khi chuyển dispatcher. Chỉ đọc Issue #5.
- **Ownership lượt này:** `.agents/scripts/muse_poller/github_client.py`, `.agents/scripts/muse_poller/signals.py`, `.agents/scripts/muse_poller/tests/test_github_client.py`, `.agents/scripts/muse_poller/tests/test_signals.py`.
- **Acceptance/test:** P2-01/02/10/12/13 với mocked GitHub: forged actor, mutated comment, label-only signal, foreign Issue/repo, prompt/shell trong body, 304 repeated, missing ETag, rate limits đều không dispatch; **không mở HTTP inbound listener**. No raw GitHub tokens in test/log.
- **Rủi ro/rollback:** reader ở chế độ **dry-run** đến khi W05/W06 PASS; rollback là dừng process, không xóa receipt/state của job đã gửi.

#### P1-W05 — Durable ledger, dedup và CoS dispatcher P2

- **Giá trị/mục tiêu:** crash/retry không tạo hai chat và không xác nhận thành công khi chưa có PONG. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi (G3-02/04):** ledger SQLite, ETag cache, lock file và audit logs trên Windows đặt mặc định tại **`%LOCALAPPDATA%\OpenDash\muse-poller\`**, với ACL hạn chế user hiện tại; **tuyệt đối không tạo** các file `.sqlite/.db/.log` hoặc thư mục `runtime/logs/cache/secrets/tokens` dưới `.agents/` (kể cả trong unit tests). Nếu `LOCALAPPDATA` không có hoặc đường dẫn không bảo đảm nằm ngoài repo, fail-closed; tests dùng `tempfile.TemporaryDirectory()` thuộc system temp. Ledger có unique `(repo_id, github_signal_id)` và `(repo_id, request_id)`, input UUID, snapshot hash và status, single-job lock; flush `DISPATCHING_UNKNOWN` trước socket, reconcile outbox+session events. `dispatcher.py` cung cấp **constructor-injected `receipt_publisher` callback/protocol** nhận **redacted receipt**, mặc định mock/no-op trong W05 tests; **W07 mới cắm `github_receipts.py` thật ở composition root `main.py`**, không sửa `dispatcher.py`. Publication fail chỉ giữ receipt để retry publish idempotent, **không gửi lại CoS**.
- **Ownership lượt này:** `.agents/scripts/muse_poller/ledger.py`, `.agents/scripts/muse_poller/dispatcher.py`, `.agents/scripts/muse_poller/tests/test_ledger.py`, `.agents/scripts/muse_poller/tests/test_dispatcher.py`. Tích hợp với client/reader qua interface đã review; không tự sửa file W03/W04 ngoài một **Design Delta** được review.
- **Interface contract cho W06 (G3-02):** W05 phải định nghĩa/test sẵn public API **`ledger.inspect_job(job_id)`**, **`ledger.get_active_job()`**, **`ledger.resolve_unknown_job(job_id, resolution, evidence_ref, operator_id)`** (chỉ kết quả `DELIVERED` khi có evidence, hoặc `ABORTED_UNCONFIRMED` sau manual reconcile + audit; cấm replay UUID cũ) và **`ledger.fail_close_inflight_jobs(reason)`** (chuyển in-flight mơ hồ thành `UNKNOWN`, không đánh dấu completed). W06 chỉ gọi API này từ `operator_cli.py`/`supervisor.py`, **không sửa `ledger.py` hay `dispatcher.py`**. Nếu thiếu seam, xử lý ở W05 trước khi merge/nhận W06 READY.
- **Acceptance/test:** P2-03..08/11: identical replay = 0 new sends; changed hash/ID = `CONFLICT_PAYLOAD_MISMATCH`/`BLOCKED`; kill before/after durable checkpoint, network timeout, outbox 50 finished evicted, `UNKNOWN` manual reconciliation, dual dispatcher racing, redaction và storage ACL. Có unit test cho injected publisher success/failure/retry **chỉ ghi receipt**, và cả 4 public ledger methods cho W06. Không dùng HTTP 409 (Poller không có relay).
- **Rủi ro/rollback:** không xóa ledger để "sửa" UNKNOWN; backup/redacted evidence và manual resolution, cấm tự resend; schema SQLite POC không chạm DB CMS.

#### P1-W06 — Supervisor/lifecycle + operator CLI P3

- **Giá trị/mục tiêu:** Poller chỉ dispatch lúc CoS thực sự available và có thể dừng an toàn. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** health watcher kiểm PID + process creation time, cặp endpoint/token và `Allow actions`, pin UUID↔worktree; stop/kill switch và resume khi CoS restart; local `poller-ctl inspect/resolve` có audit/quyền OS, không expose qua GitHub. **Chỉ sử dụng seam W05 đã review** (`inspect_job`, `get_active_job`, `resolve_unknown_job`, `fail_close_inflight_jobs`) khi supervisor phải fail-close in-flight jobs hoặc operator giải quyết UNKNOWN; không tự sửa ledger/dispatcher. Không install service hoặc autorun Windows trước khi có scope riêng.
- **Ownership lượt này:** `.agents/scripts/muse_poller/supervisor.py`, `.agents/scripts/muse_poller/operator_cli.py`, `.agents/scripts/muse_poller/tests/test_supervisor.py`, `.agents/scripts/muse_poller/tests/test_operator_cli.py`.
- **Acceptance/test:** P2-09; P3-04..09: offline **≤5 giây end-to-end** (đo từ lúc đóng CoS), rotate một/both files, restart/auto-update/sleep-wake, API 401/403, operator thiếu quyền, in-flight UNKNOWN → fail-closed/0 new sends. Không chạy Cloudflare/tunnel.
- **Rủi ro/rollback:** nếu timer/health không đạt SLA thì giữ `NO_DISPATCH`; rollback dừng Poller và bảo toàn ledger/cursor, không dùng start-up task bí mật.

#### P1-W07 — Muse/GitHub → Forge round-trip P4

- **Giá trị/mục tiêu:** chứng minh user không phải chuyển tay nhiệm vụ giữa Muse và Forge trong **một** trường hợp PONG an toàn. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** ghép Poller coordinator (reader, ledger, supervisor, CoS client), kết nối `github_receipts.py` với **`receipt_publisher` seam đã có từ W05 qua `main.py`** để xuất status rút gọn tới Issue #5; không sửa W05 dispatcher. Không cho phép arm live Poller nếu receipt publisher thật chưa được cấu hình/xác thực; no-op publisher chỉ dành cho W05 mock/dry-run tests. Chỉ sau W01–W06 PASS và signal contract được duyệt mới chuyển NO_DISPATCH sang chế độ thử bằng thao tác rõ của operator. Muse phát **một** approved event/comment; Windows dispatch và Forge trả PONG; Muse đọc kết quả.
- **Ownership lượt này:** `.agents/scripts/muse_poller/main.py`, `.agents/scripts/muse_poller/github_receipts.py`, `.agents/scripts/muse_poller/README.md`, `.agents/scripts/muse_poller/tests/test_roundtrip.py`, `docs/verification/muse-cos-win-phase1-p4-roundtrip.md`. Không tự đổi contract đã duyệt.
- **Acceptance/test:** P4-01..04, P3-01..03/10: trace ID GitHub signal → request ID → input UUID → CoS main chat → `PONG`+`turn_end` → GitHub receipt; retry GH ack sau outage không gửi lại CoS; missing auth/CoS offline → BLOCKED. Log/status chỉ redacted. **Không chấp nhận full task code, PR, worker hoặc shell instructions.**
- **Rủi ro/rollback:** stop Poller/disable armed signal; giữ Issue #5 làm lịch sử thử nghiệm, không xóa receipt sau timeout; ghi BLOCKED và dừng nếu sai project/credential.

#### P1-W08 — Verification/hardening và G5 evidence cho Phase 1

- **Giá trị/mục tiêu:** kiểm chứng độc lập cả đường thành công và đường lỗi trước khi nói Phase 1 `VERIFIED`. **G3:** APPROVED; Issue implementation NOT_READY.
- **Phạm vi:** fault-injection matrix tự động trên fixture/mock cho P2/P3, kiểm lại P0–P4 trên Windows với expected vs actual; đối chiếu SHA version CoS/Poller, permissions, restart, event identity, concurrency, redaction; tổng hợp điểm chưa đạt và review riêng.
- **Ownership lượt này:** `.agents/scripts/muse_poller/tests/test_fault_injection.py`, `docs/verification/muse-cos-win-phase1-final.md`. Các verification files của W01/W02/W03/W07 là **input chỉ đọc**, không tự thay kết quả lịch sử; cần bổ sung evidence phải làm follow-up rõ ownership.
- **Acceptance/test:** toàn bộ case P0–P4 trong [matrix](../../.agents/research/muse-cos-win-poc-test-matrix.md) ghi `PASS/FAIL/BLOCKED/NOT_RUN` với redacted evidence, manual run logs (không secrets), commit+Windows environment. Nova review kết quả độc lập, chủ dự án quyết định đóng Phase 1; **chỉ VERIFIED khi evidence đúng**, không do PR merge/AI tự báo.
- **Rủi ro/rollback:** nếu một case security/lifecycle FAIL, ngừng dispatch, giữ ledger và Issue #5, không tự mở Phase 2/P5; G5 report chỉ mô tả đúng phần đã chứng minh.

### 5.3. Điều kiện tạo implementation Issue và chuyển sang READY

**Staged activation:** sau khi PR #4 được review/merge vào `main`, **chỉ xem xét W01/W02** để chốt G3 `READY`; với mỗi task phải có Issue riêng và một writer trên branch. Sau khi W01 evidence và W02 signal contract/P0 evidence **cùng đồng bộ thông số W03–W07** được review/merge vào `main`, mới xem xét **W03–W08**: `source_sha` và `base_sha` đều lấy từ baseline mới được duyệt, không từ bản draft tại PR #4. W05/W06/W07 còn phải chứng minh interface seam W05 và CI unit tests từ W03 đã hoạt động. **Chưa có P0 PASS thì không mở READY cho W03/W04.**

Chỉ cho một task `READY` sau khi **đồng thời** có: (1) G2 Phase 1 + G3 breakdown và các P0 dependencies tương ứng đã review/merge `main` tại immutable `source_sha`; (2) scope/track rõ, dùng **`P1-Wxx` infra exception thủ công**, không dùng `prepare-github-issue.py` hoặc cấp M3 module/milestone giả; (3) implementation Issue mới **theo đúng 8 mục trong form**, tiêu đề `[P1-Wxx] ...`, nhãn `type:implementation`/`agent:forge`/`area:agent-workflow`, bản sao acceptance, baseline `base_sha`, owner/file ownership không giao đồng thời và link blob tại approved `source_sha`; (4) quyền GitHub/CoS và dependency của task đã được kiểm chứng; (5) acceptance/negative tests/rollback/evidence; (6) Nova/CI/chủ dự án review theo workflow. Muse/coordinator tìm cả Issue open và closed theo **P1-Wxx chính xác** để tránh trùng, không phát signal `TEST-PONG-01` khi tạo Issue. Nếu thiếu điều kiện, task giữ `DRAFT/BLOCKED`, không tự bật `READY`.

**Tình trạng tại thời điểm phê duyệt G3:** đã chốt scope W01–W08, PR #4 cần được merge vào `main` làm baseline; **chưa có Issue implementation nào được tạo hoặc đánh dấu READY**. Mọi P0–P4 runtime **NOT_RUN**; Issue #5 vẫn **NO_DISPATCH**. Phê duyệt G3 không tự gửi PONG hay bật Poller trước khi P0/P1 và quyền đã kiểm chứng.
