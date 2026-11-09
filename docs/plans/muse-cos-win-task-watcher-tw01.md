# TW-01 — Hợp đồng chọn task GitHub Issues và xác định Forge idle (đề xuất)

> **Trạng thái:** PROPOSED / REVIEW_REQUIRED — hợp đồng read-only, **không phải TW-01 G3 READY**, không cấp quyền chạy Watcher hay CoS POST. **Owner:** Forge · **Reviewer:** Atlas. **Approved source nền:** [G2/G3 Python Task Watcher](muse-cos-win-phase1-g2.md) tại merge commit [`141e560bd423de479cdeae865bbcf689cf015105`](https://github.com/thientran2359/opendash-cms/commit/141e560bd423de479cdeae865bbcf689cf015105) (PR #8). Bằng chứng khảo sát trong [TW-01 read-only verification](../verification/2026-11-09-muse-cos-win-tw01-readonly.md).

## 1. Mục tiêu và phạm vi

TW-01 chốt một **hợp đồng chọn đúng Issue** và **xác định rảnh/bận/không rõ** trước khi TW-02 viết một `task_watcher.py` chạy trên Windows, mặc định chu kỳ 120 giây. Chỉ sử dụng **GitHub HTTPS outbound**, **CoS Local Control API trên loopback** và thông tin local về canonical Forge project. GitHub Issues/Project #3 là nguồn trạng thái task; Python không gọi GitHub Plugin như thư viện, không đọc raw Issue body thành lệnh; Forge mới dùng Plugin trong main chat sau khi được đánh thức.

**Không làm ở TW-01:** không viết hoặc chạy watcher, không tạo Issue rồi tự đặt `Ready`, không bật CoS `Allow actions`, không POST `/v1/inputs`, không chạy PONG hoặc task lập trình, không sửa token/ACL, không triển khai SQLite, webhook, server, relay, Windows service, dashboard hoặc worker riêng. [Issue #5](https://github.com/thientran2359/opendash-cms/issues/5) vẫn `NO_DISPATCH` và `W01-SEC-01` vẫn HIGH/open.

## 2. GitHub Project v2 — hợp đồng dữ liệu và quyền

1. **Nguồn sự thật:** đúng repository `thientran2359/opendash-cms`, user-owned [Project #3](https://github.com/users/thientran2359/projects/3), Issue `OPEN` thuộc repo đó và label chính xác `agent:forge`; không coi pull request, draft item hay Issue repo khác là task.
2. **Quyền GraphQL:** watcher phải có credential Windows được cấp quyền **đọc Project v2**. Với classic PAT, [GitHub Docs](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects) yêu cầu `read:project` cho query; không được suy từ `repo` hay từ GitHub Plugin trong chat. Cấp/cập nhật quyền do chủ máy thực hiện; watcher không tự nâng quyền, không in token/response chứa secret.
3. **Identity và pagination:** dùng GraphQL `user(login: "thientran2359") { projectV2(number: 3) }`, phân trang đầy đủ `fields` và `items`. Tìm **duy nhất** single-select field tên `Status`, pin `field.id` và **option ID** của giá trị `Ready` sau khi đọc API thực. Với từng `ProjectV2Item`, chỉ nhận `content` loại `Issue` đúng repo, so `fieldValues` thuộc **đúng field ID** và Ready option; nếu thiếu field/option, status mâu thuẫn, item REDACTED, phân trang lỗi hoặc không có quyền ⇒ `UNKNOWN`, **0 dispatch**. Không dùng nhãn `status:ready` hoặc suy từ board/view.
4. **Điều kiện bổ sung:** implementation Issue có Task ID namespace hợp lệ, G3 đã review/merge ở immutable `source_sha`, `base_sha`, owner/file ownership và acceptance đủ 8 mục; **mọi dependency đạt chính xác gate kèm evidence** (không dùng `not BLOCKED` hoặc `closed` làm completed). Thiếu quyền đọc source/dependency/PR ⇒ `UNKNOWN`, không gán Ready.
5. **Branch occupancy:** dùng GitHub Pulls API/Plugin để kiểm tra open PR liên quan `agent/forge` và task trước; PR còn open hoặc gate review/merge/verification chưa đạt ⇒ **INELIGIBLE/BUSY** dù CoS có `running=false`. Chỉ release khi có evidence task trước qua gate cần thiết và Forge xác nhận branch/worktree sync; watcher không tự chạy Git.
6. **Freshness:** trước dispatch phải re-fetch Issue open/labels, Project field value, approvals/dependencies và branch PR. `401/403/429`, rate limit, timeout, stale cache ⇒ `UNKNOWN`; tôn trọng `Retry-After`, không fallback vào kết quả cũ.

**Bằng chứng ngày 09/11/2026:** GitHub Plugin tìm được Issue theo repo/label; trình duyệt xem được Project #3. Tuy nhiên **Windows Git Credential Manager token chưa có `read:project`**, GraphQL trả lỗi permission; vì vậy **Project Status field ID, Ready option ID và candidate Ready hiện `BLOCKED_NOT_VERIFIED`**. Chỉ xác nhận tên `Ready` theo [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md), **chưa pin ID giả**. Việc cấp đúng scope và query lại là việc cần hoàn tất trước khi coi TW-01 đủ bằng chứng.

## 3. CoS Forge idle — metadata-only, ba trạng thái

**Bước nhận diện:** đọc `endpoint.json` và bearer `token` trong user-data ngoài Git, xác nhận loopback, PID/creation-time, phiên bản, `GET /v1/health`, `actions.enabled`, `GET /v1/status`, `GET /v1/agents`. Trong `state/projects.json` phải có **đúng một** UUID mapping tới canonical Windows Forge worktree; không đưa UUID/token/path riêng tư vào Git, không tự tạo/sửa project. CoS **2.1.51** đã quan sát metadata các route `/v1/sessions` và `/v1/agents`, nhưng **không có bằng chứng một GET tương đương “Forge idle” nguyên tử**.

| Kết quả | Điều kiện | Hành vi |
| --- | --- | --- |
| **BUSY / INELIGIBLE** | Có active CoS agent/turn/queued input/tool call, pending approval, session/helper/compaction liên quan Forge; **hoặc Forge còn PR/task/branch chiếm dụng** | 0 send, giữ job/claim nếu có |
| **IDLE_CANDIDATE** | Health/auth/actions/mapping hợp lệ; agent metadata không chạy; **toàn bộ** session/run/input liên quan Forge đã được liệt kê đầy đủ, không có `activeTurnId`/activity chưa hết hạn, pending user approval/queued input/helper/compaction; không có open PR/branch occupancy và task cũ đã release đúng gate | Chỉ là ứng viên; preflight lại ngay trước giao, không tự cho phép POST |
| **UNKNOWN / BLOCKED** | Thiếu/bất nhất metadata; sessions phân trang chưa đủ, session status stale, queued input/approval không có API chứng minh, process/token/project mapping không duy nhất, CoS offline, không thể xác định session thuộc Forge hoặc cửa sổ race chưa có quy ước vận hành | **0 send**; log lý do đã redacted; không suy `health 200`, `agents.running=false` hay `toolCalls.running=0` thành idle |

**Contract cần hoàn tất ở TW-01:** (a) chứng minh cách liệt kê/kiểm metadata **mọi** session/turn/queued input/approval liên quan Forge mà **không đọc transcript**; API `/v1/sessions` có `nextCursor` và trường `activeTurnId`/`activityExpiresAt`, nhưng pagination/freshness và coverage chưa chứng minh toàn phần; (b) chốt tiêu chí session đã kết thúc (`lastTurnEndAt`/`endedAt`) và helper/compaction/approval; (c) kiểm metadata ở gần thời điểm giao, xác định thời gian hiệu lực và yêu cầu người vận hành không khởi tạo turn cạnh tranh trong cửa sổ giao, do check idle và `POST` không có transaction nguyên tử đã được chứng minh. Nếu không chứng minh được một trong các mục đó: **¬IDLE**, `BLOCKED` cho live dispatch.

**Live snapshot không phải quyết định rảnh:** tại một lần kiểm tra read-only ngày 09/11, `GET /v1/health` trả `ok=true`/`actions.enabled=true`; `GET /v1/agents` trả `running=false` và không có active agent record; `GET /v1/status` và `GET /v1/sessions` trả metadata schema. **Nhưng [PR #7](https://github.com/thientran2359/opendash-cms/pull/7) của `agent/forge` vẫn open**, và chưa chứng minh đầy đủ trạng thái tất cả sessions/approval: **Forge không được coi là eligible để auto-dispatch**. Snapshot có thể đổi bất kỳ lúc nào.

## 4. Quy tắc chọn task (đề xuất cho TW-02)

```text
candidate = OPEN issue in allowlisted repo + label agent:forge
if Project-v2-Status-read != uniquely Ready: UNKNOWN => 0 send
if G3/source/dependency/ownership evidence missing: UNKNOWN => 0 send
if Forge branch has open PR or prior task not released: BUSY => 0 send
if CoS project/health/auth/idle incomplete: UNKNOWN => 0 send
if matching issue already claimed/terminal in durable state: INELIGIBLE
recheck all metadata just before delivery; any drift => 0 send
otherwise ELIGIBLE_CANDIDATE (not an authorization to POST in TW-01)
```

- Tối đa một issue một lúc; nếu nhiều eligible, chọn deterministically (Project priority **chỉ nếu** field được chứng minh, rồi `created_at`/issue number).
- TW-02 giữ **một** JSON state durable + OS lifetime lock; claim `repo`/Issue/UUID/`PRE_SEND` ghi thành công **trước POST**, 202 chỉ queued, UNKNOWN không retry mù, restart không replay; giữ terminal tombstone. Chi tiết đã chốt trong [G2 §6.2](muse-cos-win-phase1-g2.md#62-hợp-đồng-kích-hoạt-task-thực).
- **Hai đường riêng:** `--dry-run` mặc định ⇒ 0 POST/0 durable claim; controlled `--pilot-pong` chỉ [Issue #5](https://github.com/thientran2359/opendash-cms/issues/5) sau owner arm và W01 security recheck, không cần giả #5 là implementation Ready. **Real code-task mode DISABLED** đến khi pilot verified và owner quyết định riêng.

## 5. Acceptance, file ownership và gate

**TW-01 acceptance (chưa toàn bộ PASS):**

1. Gắn đúng Project owner/number, pin field/option ID thật bằng credential Windows có `read:project`, kiểm fields/items pagination; no permission/field mismatch ⇒ 0 eligible.
2. Issue ngoài repo, sai agent, Backlog/In progress/In review, Project item không rõ, không source G3, dependency thiếu evidence ⇒ 0 eligible.
3. Có open PR `agent/forge`, Forge run đang active, queued/pending approval/compaction, session unknown/freshness lỗi ⇒ 0 eligible.
4. Chứng minh mapping UUID ↔ canonical root là duy nhất, health/auth/actions và khả năng metadata-only xác định idle trên CoS **bản đang cài**; không đọc nội dung chat.
5. Có decision table, mock fixtures read-only đủ negative cases, cách đối chiếu manual với GitHub Project, rollback là ngừng đánh giá; **0 `POST /v1/inputs`**.
6. Atlas review + CI tài liệu tại đúng SHA; owner phê duyệt/merge contract trước TW-02. Không gán TW-02/03 `Ready` tự động.

**Phạm vi file của lượt đề xuất này:** `docs/plans/muse-cos-win-task-watcher-tw01.md`, `docs/verification/2026-11-09-muse-cos-win-tw01-readonly.md` và liên kết mục lục trong `docs/plans/README.md` / `docs/verification/README.md`. **Chỉ thiết kế và ghi kiểm tra read-only**. Nếu cần code probe/test riêng hay đổi workflow, tạo Issue/ownership được duyệt tiếp theo; không mở rộng ngầm.

**Gate tiếp theo:** Atlas review findings (đặc biệt feasibility pagination/idle và blocked GitHub permission), chủ máy cấp quyền đọc Project nếu lựa chọn, rồi query lại/đối chiếu evidence trong PR cập nhật. Sau review/approval/merge, coordinator có thể phát hành Issue `[TW-01]` theo [G3 ngoại lệ thủ công](muse-cos-win-phase1-g2.md#7-g3-amendment--thay-breakdown-w02w08-bằng-ba-gói-nhỏ-đề-xuất-chưa-ready) và [workflow](../../.agents/GITHUB_WORKFLOW.md), **không tự READY hay dispatch vì tài liệu này tồn tại**.
