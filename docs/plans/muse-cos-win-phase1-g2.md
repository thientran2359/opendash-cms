# Muse → CoS Windows — Phase 1: G2 AGREED / G3 Breakdown DRAFT (P0–P4)

- **Quyết định chủ dự án:** **G2 Phase 1 APPROVED / design = AGREED**, 2026-11-09. Phạm vi phê duyệt **chỉ P0–P4** và phép thử `TEST-PONG-01`; không phê duyệt Phase 2/P5.
- **Track:** Full-Track cho phần hạ tầng điều phối (GitHub auth, local CoS Control API, secrets, durable receipt, failure recovery); **không thuộc code runtime CMS**.
- **Scope:** AGREED · **Research:** REVIEWED · **Design/G2:** AGREED (chỉ Phase 1) · **G3:** DRAFT (chưa có task READY) · **Implementation:** NOT_STARTED · **Verification:** NOT_RUN.
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

## 5. G3 Breakdown — các work package ứng viên

**Tình trạng:** **G3 DRAFT / FOR REVIEW**, lập ngày 2026-11-09. Đây là breakdown chuẩn bị cho implement/test, **không phải lệnh thực thi**. Mỗi work package có thể trở thành một GitHub implementation Issue **sau** khi contract này được review/merge vào `main`, owner/file ownership, baseline SHA và gate G3 của từng task được xác nhận. Không mở task triển khai `READY` từ một commit chỉ tồn tại trên `agent/forge`.

**Quy ước ID:** `P1-W01..P1-W08` là **mã work package tạm**, không phải Task ID sản phẩm chuẩn `Mx-Sxx-Txx`. Poller là hạ tầng phối hợp agent, **không** tự nhận là module/capability M3 hay tăng số lượng module V1. Nếu muốn cấp Task ID chính thức, coordinator phải chốt namespace hoặc milestone hạ tầng tương ứng theo [Planning Workflow](PLANNING_WORKFLOW.md) trước khi tạo Issue implementation. [Issue #5](https://github.com/thientran2359/opendash-cms/issues/5) giữ vai trò **coordination/signal test**, không được dùng làm task implementation.

**Track:** Full-Track xuyên suốt vì có credential GitHub/CoS, local Control API, durable SQLite ledger, security boundary và crash recovery. **Đầu vào cố định:** G2 Phase 1 tại chính tài liệu này, [test matrix](../../.agents/research/muse-cos-win-poc-test-matrix.md), [GITHUB_WORKFLOW](../../.agents/GITHUB_WORKFLOW.md) và commit `origin/main` đã merge khi từng task bắt đầu. **Kết quả cần đạt:** một tín hiệu `TEST-PONG-01` hợp lệ từ Muse đi qua Issue #5 tới đúng Forge main chat, phản hồi PONG được xác minh và receipt đã redacted; thử lỗi và restart không gây gửi trùng.

### 5.1. Thứ tự và dependency

| Work package (DRAFT) | Gate | Owner / reviewer | Dependency | Đầu ra có thể nghiệm thu |
| --- | --- | --- | --- | --- |
| **P1-W01 — Khảo sát CoS Windows** | P0 | Forge / Nova | G2 Phase 1 merged; chủ máy cấp quyền đọc local | Evidence CoS API/permissions, `endpoint.json`+`token`, `projectId`↔worktree đúng |
| **P1-W02 — Chốt GitHub signal/permissions** | P0 | Forge + Muse / Nova | G2 Phase 1 merged; Issue #5 tồn tại | Contract signal ID/actor/receipt, least-privilege quyền, ETag/rate-limit |
| **P1-W03 — CoS adapter + Local PONG** | P1 | Forge / Nova | W01 PASS | Client loopback chỉ gửi PONG cố định, xác nhận main chat/session events |
| **P1-W04 — GitHub Poller read/validate** | P2 | Forge / Nova | W02 contract reviewed | Đọc event bằng ID, auth/integrity, conditional GET, reject nguồn lạ |
| **P1-W05 — Durable ledger + dispatcher** | P2 | Forge / Nova | W03 + W04 | Exactly-once admission attempt, dedup/recovery, redacted receipt |
| **P1-W06 — Local supervisor + operator controls** | P3 | Forge / Nova | W05 | Fail-closed lifecycle CoS, token rotation, manual resolve/kill switch |
| **P1-W07 — Muse → GitHub → CoS round-trip** | P4 | Forge (Windows) + Muse (signal) / Nova | W06 và P0–P3 evidence PASS | PONG từ Muse→Forge→GitHub receipt, 1 UUID/session, không sửa CMS |
| **P1-W08 — Verification/hardening Phase 1** | P0–P4 / G5 evidence | Forge / Nova độc lập | W07 | Báo cáo test, negative/restart/rate limit, PASS/FAIL và residual risk |

W01/W02 là hai nhánh khảo sát **độc lập về nội dung** nhưng mọi công việc ghi lên nhánh `agent/forge` vẫn phải **tuần tự: một active implementation task trên branch tại một thời điểm**. Thứ tự triển khai đề xuất: W01 → W02 → W03 → W04 → W05 → W06 → W07 → W08. Không chia hai người đồng thời ghi cùng file/ledger, không dùng worker/helper để thay Forge main chat.

### 5.2. Task briefs — phạm vi, file ownership và acceptance

**Định hướng implementation để review ở G3:** dùng Python 3 đang có trong repo cho POC hạ tầng Windows, ưu tiên `sqlite3` + HTTP client trong standard library và `unittest` để dễ chạy mock/cross-platform; **không thêm dependency, Windows service hoặc quyền hệ điều hành mới** nếu chưa được review. Quy ước test dự kiến sau khi code tồn tại: `python -m unittest discover -s .agents/scripts/muse_poller/tests -v`; các bài thực nghiệm CoS/P4 phải chạy **trên Windows** với quyền chủ máy, không tự kích hoạt trong CI. `git diff --check`, `python cms/scripts/check-plans.py`, `python .agents/scripts/check-agent-artifacts.py` vẫn là regression docs. Nếu môi trường Python/CoS không đáp ứng thì Stop-and-Propose, không giả định test đã chạy.

#### P1-W01 — Khảo sát CoS Windows và preflight P0

- **Giá trị/mục tiêu:** chứng minh API trên **CoS bản đang cài** thực sự có thể tạo main chat đúng workspace; không dựa hoàn toàn vào upstream HEAD. **G3:** DRAFT.
- **Phạm vi:** xác minh app version; Local Control API/Allow actions do chủ máy bật; file user-data `control-api/endpoint.json` và `control-api/token` riêng; PID/creation time, health/actions thực tế; chứng minh bằng UI/config hoặc API **nếu có** `projectId` UUID ↔ canonical Forge root/worktree. Không đọc/chat dump phiên riêng tư.
- **Ownership lượt này:** **chỉ** `docs/verification/muse-cos-win-phase1-p0-cos.md` (ghi evidence đã redacted). Không sửa CMS, tool runtime, file cấu hình CoS hoặc tài liệu G2 khi chưa có quyết định delta.
- **Acceptance/test:** P0-01/02/03 có expected/actual, phản ứng khi API/actions bị tắt, port/token stale, sai project → `BLOCKED`; không gửi input ở task này. Ghi API/route thực có, không tưởng tượng endpoint project-list.
- **Rủi ro/rollback:** Local API có quyền đọc chat rộng — chỉ kiểm health/metadata tối thiểu, không log token/ID riêng tư; nếu thiếu quyền hoặc mapping không chứng minh được, dừng tại `BLOCKED`, tắt các công cụ thử nghiệm; không tự bật Allow actions.

#### P1-W02 — Chốt GitHub Issue #5 signal contract và quyền P0

- **Giá trị/mục tiêu:** xác định chính xác **tín hiệu nào** của Muse đủ tin cậy để Windows Poller nhận, trước khi cho phép dispatch. **G3:** DRAFT.
- **Phạm vi:** kiểm Muse GitHub connector và GitHub API; đề xuất/chứng minh loại **comment hoặc timeline event** trên Issue #5, actor allowlist + quyền repo, kiểm `user.login`/`actor.login`, `author_association` và `created_at == updated_at` nếu dùng comment; `github_signal_id`, deterministic `request_id`, ETag/304, token scope Windows và receipt writer. Xác định ai duyệt signal và cơ chế `NO_DISPATCH → armed` **không dựa trên comment bất kỳ**.
- **Ownership lượt này:** `docs/plans/muse-cos-win-phase1-github-signal-contract.md` (contract bổ trợ cần Nova/chủ dự án review trước khi dùng), `docs/verification/muse-cos-win-phase1-p0-github.md`. Không chỉnh Issue #5 thành signal, không viết Poller code.
- **Acceptance/test:** P0-04/05 chứng minh quyền đọc/ghi GitHub và actor identity trên event cụ thể; diễn giải idempotent ack và permission denied/403/429, sửa comment hoặc label lạ → từ chối; GitHub auth secrets tuyệt đối không ghi vào docs/Issue.
- **Rủi ro/rollback:** nếu Muse không có quyền phát signal hoặc Windows không thể đọc/ack thì ghi `BLOCKED`; không tự chuyển Inbound Relay. Nếu contract mới đụng G2 trust boundary thì **Stop-and-Propose**, yêu cầu review trước khi triển khai.

#### P1-W03 — CoS loopback adapter và P1 Local PONG

- **Giá trị/mục tiêu:** có client tối thiểu để một input UUID tạo **một** main chat Forge và nhận kết quả PONG có bằng chứng. **G3:** DRAFT.
- **Phạm vi:** typed local client cho `GET /v1/health`, `POST /v1/inputs`, `GET /v1/inputs`, `GET /v1/sessions/{id}/events`; giữ token Windows local, pin CoS project/worktree từ W01; prompt PONG **hard-coded**; không lấy text từ GitHub.
- **Ownership lượt này:** `.agents/scripts/muse_poller/cos_client.py`, `.agents/scripts/muse_poller/tests/test_cos_client.py`, `docs/verification/muse-cos-win-phase1-p1-local-pong.md`. Có thể thêm `__init__.py` **chỉ** trong package trên nếu cần import.
- **Acceptance/test:** P1-01..07: HTTP 202 ≠ completed; PONG assistant event+`turn_end` và input UUID/session ID; sai UUID/project, disabled actions, timeout, model override sai đều fail-closed; không đụng existing chat, không worker, không file/terminal tools.
- **Rủi ro/rollback:** chỉ gửi phép thử sau khi chủ máy cho phép và W01 PASS; nếu delivery `UNKNOWN` không tạo UUID mới để retry; dừng local client, không để background service.

#### P1-W04 — GitHub Poller reader + event validation P2

- **Giá trị/mục tiêu:** Poller **đọc đúng signal**, không biến bất kỳ Issue/comment thành lệnh điều khiển máy. **G3:** DRAFT.
- **Phạm vi:** GitHub HTTPS authenticated GET, conditional `ETag`/`If-None-Match`, xử lý 304/403/429/secondary limits; canonical event identity/actor/allowlist, comment edited detection, re-fetch/payload hash trước khi chuyển dispatcher. Chỉ đọc Issue #5.
- **Ownership lượt này:** `.agents/scripts/muse_poller/github_client.py`, `.agents/scripts/muse_poller/signals.py`, `.agents/scripts/muse_poller/tests/test_github_client.py`, `.agents/scripts/muse_poller/tests/test_signals.py`.
- **Acceptance/test:** P2-01/02/10/12/13 với mocked GitHub: forged actor, mutated comment, label-only signal, foreign Issue/repo, prompt/shell trong body, 304 repeated, missing ETag, rate limits đều không dispatch; **không mở HTTP inbound listener**. No raw GitHub tokens in test/log.
- **Rủi ro/rollback:** reader ở chế độ **dry-run** đến khi W05/W06 PASS; rollback là dừng process, không xóa receipt/state của job đã gửi.

#### P1-W05 — Durable ledger, dedup và CoS dispatcher P2

- **Giá trị/mục tiêu:** crash/retry không tạo hai chat và không xác nhận thành công khi chưa có PONG. **G3:** DRAFT.
- **Phạm vi:** local SQLite ledger dưới user-data **ngoài Git repo**; unique `(repo_id, github_signal_id)` và `(repo_id, request_id)`; gắn input UUID, snapshot hash và status; single-job lock; flush `DISPATCHING_UNKNOWN` trước socket; reconcile outbox+session events và tạo receipt object đã redacted (publishing GitHub thuộc W07), không raw chat.
- **Ownership lượt này:** `.agents/scripts/muse_poller/ledger.py`, `.agents/scripts/muse_poller/dispatcher.py`, `.agents/scripts/muse_poller/tests/test_ledger.py`, `.agents/scripts/muse_poller/tests/test_dispatcher.py`. Tích hợp với client/reader qua interface đã review; không tự sửa file W03/W04 ngoài một **Design Delta** được review.
- **Acceptance/test:** P2-03..08/11: identical replay = 0 new sends; changed hash/ID = `CONFLICT_PAYLOAD_MISMATCH`/`BLOCKED`; kill before/after durable checkpoint, network timeout, outbox 50 finished evicted, `UNKNOWN` manual reconciliation, dual dispatcher racing, redaction và storage ACL. Không dùng HTTP 409 (Poller không có relay).
- **Rủi ro/rollback:** không xóa ledger để "sửa" UNKNOWN; backup/redacted evidence và manual resolution, cấm tự resend; schema SQLite POC không chạm DB CMS.

#### P1-W06 — Supervisor/lifecycle + operator CLI P3

- **Giá trị/mục tiêu:** Poller chỉ dispatch lúc CoS thực sự available và có thể dừng an toàn. **G3:** DRAFT.
- **Phạm vi:** health watcher kiểm PID + process creation time, cặp endpoint/token và `Allow actions`, pin UUID↔worktree; stop/kill switch và resume khi CoS restart; local `poller-ctl inspect/resolve` có audit/quyền OS, không expose qua GitHub. Không install service hoặc autorun Windows trước khi có scope riêng.
- **Ownership lượt này:** `.agents/scripts/muse_poller/supervisor.py`, `.agents/scripts/muse_poller/operator_cli.py`, `.agents/scripts/muse_poller/tests/test_supervisor.py`, `.agents/scripts/muse_poller/tests/test_operator_cli.py`.
- **Acceptance/test:** P2-09; P3-04..09: offline **≤5 giây end-to-end** (đo từ lúc đóng CoS), rotate một/both files, restart/auto-update/sleep-wake, API 401/403, operator thiếu quyền, in-flight UNKNOWN → fail-closed/0 new sends. Không chạy Cloudflare/tunnel.
- **Rủi ro/rollback:** nếu timer/health không đạt SLA thì giữ `NO_DISPATCH`; rollback dừng Poller và bảo toàn ledger/cursor, không dùng start-up task bí mật.

#### P1-W07 — Muse/GitHub → Forge round-trip P4

- **Giá trị/mục tiêu:** chứng minh user không phải chuyển tay nhiệm vụ giữa Muse và Forge trong **một** trường hợp PONG an toàn. **G3:** DRAFT.
- **Phạm vi:** ghép Poller coordinator (reader, ledger, supervisor, CoS client), xuất **receipt/status** rút gọn trở lại Issue #5; chỉ sau khi W01–W06 PASS và signal contract duyệt mới chuyển NO_DISPATCH sang chế độ thử bằng thao tác rõ của operator. Muse phát **một** approved event/comment; Windows dispatch và Forge trả PONG; Muse đọc kết quả.
- **Ownership lượt này:** `.agents/scripts/muse_poller/main.py`, `.agents/scripts/muse_poller/github_receipts.py`, `.agents/scripts/muse_poller/README.md`, `.agents/scripts/muse_poller/tests/test_roundtrip.py`, `docs/verification/muse-cos-win-phase1-p4-roundtrip.md`. Không tự đổi contract đã duyệt.
- **Acceptance/test:** P4-01..04, P3-01..03/10: trace ID GitHub signal → request ID → input UUID → CoS main chat → `PONG`+`turn_end` → GitHub receipt; retry GH ack sau outage không gửi lại CoS; missing auth/CoS offline → BLOCKED. Log/status chỉ redacted. **Không chấp nhận full task code, PR, worker hoặc shell instructions.**
- **Rủi ro/rollback:** stop Poller/disable armed signal; giữ Issue #5 làm lịch sử thử nghiệm, không xóa receipt sau timeout; ghi BLOCKED và dừng nếu sai project/credential.

#### P1-W08 — Verification/hardening và G5 evidence cho Phase 1

- **Giá trị/mục tiêu:** kiểm chứng độc lập cả đường thành công và đường lỗi trước khi nói Phase 1 `VERIFIED`. **G3:** DRAFT.
- **Phạm vi:** fault-injection matrix tự động trên fixture/mock cho P2/P3, kiểm lại P0–P4 trên Windows với expected vs actual; đối chiếu SHA version CoS/Poller, permissions, restart, event identity, concurrency, redaction; tổng hợp điểm chưa đạt và review riêng.
- **Ownership lượt này:** `.agents/scripts/muse_poller/tests/test_fault_injection.py`, `docs/verification/muse-cos-win-phase1-final.md`. Các verification files của W01/W02/W03/W07 là **input chỉ đọc**, không tự thay kết quả lịch sử; cần bổ sung evidence phải làm follow-up rõ ownership.
- **Acceptance/test:** toàn bộ case P0–P4 trong [matrix](../../.agents/research/muse-cos-win-poc-test-matrix.md) ghi `PASS/FAIL/BLOCKED/NOT_RUN` với redacted evidence, manual run logs (không secrets), commit+Windows environment. Nova review kết quả độc lập, chủ dự án quyết định đóng Phase 1; **chỉ VERIFIED khi evidence đúng**, không do PR merge/AI tự báo.
- **Rủi ro/rollback:** nếu một case security/lifecycle FAIL, ngừng dispatch, giữ ledger và Issue #5, không tự mở Phase 2/P5; G5 report chỉ mô tả đúng phần đã chứng minh.

### 5.3. Điều kiện chuyển G3 DRAFT → READY

Chỉ cho một task `READY` sau khi **đồng thời** có: (1) G2 Phase 1 + G3 breakdown review/merge vào `main` và `source_sha` immutable; (2) scope/track rõ và namespace Task ID hạ tầng được phê duyệt **không giả M3 ID**; (3) Issue implementation riêng đọc được độc lập, task owner, file ownership **không giao song song**, `base_sha`, dependency đã đạt; (4) cụ thể được GitHub/CoS auth permissions ở task liên quan; (5) acceptance, negative tests, rollback và vùng evidence; (6) Nova/CI/owner review theo workflow. Muse phải xác nhận Issue open+closed để tránh tạo trùng và chỉ mở đúng task theo thứ tự.

**Hiện trạng lúc lập G3:** PR #4 vẫn chưa merge; tất cả W01–W08 **DRAFT**, không Issue implementation nào được tạo hoặc đánh dấu READY, và mọi P0–P4 runtime **NOT_RUN**. Issue #5 vẫn **NO_DISPATCH**. G3 chấp nhận review không đồng nghĩa cho chạy PONG trước khi P0/P1 và quyền đã kiểm chứng.
