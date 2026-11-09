# Research: Muse Orchestrator POC — CoS Windows / OpenDash CMS

- **Agent / ngày:** Forge (Chat On Steroids Windows), 2026-11-09
- **Source SHA đối chiếu:** `8833fc0f9f13903373ace8cdb4e67ee29b8cfb16` (`main`, PR #3 đã merge)
- **Trạng thái:** **PROPOSED — CHƯA PHÊ DUYỆT THIẾT KẾ / CHƯA TRIỂN KHAI**
- **Liên quan:** [GitHub workflow](../GITHUB_WORKFLOW.md), [Agent registry](../agents.yaml), [Issue #1 đã verified](https://github.com/thientran2359/opendash-cms/issues/1), [Project #3](https://github.com/users/thientran2359/projects/3)
- **Kế hoạch kiểm thử:** [Muse POC test matrix](muse-cos-win-poc-test-matrix.md)
- **Phản biện:** [Nova — PR #4, 09/11/2026](https://github.com/thientran2359/opendash-cms/pull/4) (F-01–F-09 đã được Nova xác nhận RESOLVED; Round 2 R2-01–R2-03 và khuyến nghị `READY_FOR_G2_APPROVAL` cho Phase 1). Khuyến nghị reviewer **không phải quyết định G2 của chủ dự án**.
- **Quyết định của chủ dự án (09/11/2026, sau review):** **chọn Outbound GitHub Poller** làm transport mục tiêu; nếu Poller được P0–P4 chứng minh khả thi thì **loại Inbound Relay/Cloudflare khỏi kiến trúc triển khai**. Không yêu cầu chat phải thuộc ChatGPT Web Project; cần giao được việc đúng CoS/Forge workspace. Đây là quyết định về **hướng thiết kế và tiêu chí chấp nhận**, không phải xác nhận runtime đã PASS hay phê duyệt toàn bộ G2.

## 1. Mục tiêu và nguyên tắc

Chứng minh **Muse từ cloud điều phối một task vào một _main chat_ mới của CoS trên Windows**, gắn với đúng **CoS Project/worktree của Forge** trong OpenDash CMS; quan sát được việc gửi, phản hồi và lỗi, không dùng CoS broker worker/helper, không thay chat đang làm việc, không triển khai mã CMS trong phép thử đầu tiên. **Không bắt buộc** chat thuộc ChatGPT Web Project `opendash-cms`.

POC đi theo nguyên tắc: **xác minh local CoS → xác minh GitHub Poller outbound → kiểm tra dispatch/receipt/lifecycle → Muse round-trip**. Poller chỉ thực hiện HTTPS outbound từ Windows đến GitHub và gọi CoS loopback, **không** mở relay HTTP nhận lệnh từ Internet hoặc chạy Cloudflare Tunnel trong scope P0–P4. Phải PASS từng cổng trước khi bật cổng sau. Không tự động mở rộng sang Mac, Antigravity, deploy, merge PR hoặc task G3 chưa `READY`.

**Quy ước nguồn sự thật:** tài liệu `docs/plans/` và `docs/contracts/` chứa đặc tả đã duyệt; GitHub Issue/Project giữ assignment và trạng thái; PR/`docs/verification/` là review và evidence. **Poller/dispatcher** giữ receipt kỹ thuật cục bộ để chống gửi trùng, không tạo thêm hàng đợi hay bảng trạng thái planning cạnh tranh trong Git. `MERGED` không bằng `VERIFIED`.

## 2. Facts đã kiểm tra và những điều chưa xác minh

| Thành phần | Có bằng chứng | Chưa được suy ra |
| --- | --- | --- |
| GitHub | Issue Forms, Milestones, Labels, Project #3, Ruleset và CI đã verified tại [G5](../../docs/verification/2026-11-09-github-governance-g5.md) | Chưa có Muse tự cập nhật Project hay tự giao task |
| Windows CoS | Ứng dụng v2.1.50 đang chạy khi khảo sát; có file endpoint của Local Control API | Chưa xác minh Allow actions, khả năng tạo direct main chat **đúng CoS Project/worktree** trên chính máy này |
| CoS API theo upstream | Local API chỉ bind loopback; `POST /v1/inputs` có `id`, `projectId`, `text`, không có `sessionId` thì tạo new chat với session ID bằng input UUID; events/outbox để theo dõi | Không mặc định build cài trên máy giống chính xác HEAD upstream; không coi 202 là chat đã trả lời; `projectId` không chứng minh ChatGPT Web Project |
| Cloudflare | Windows đã có `cloudflared` | **Không thuộc scope Poller**; không có test/triển khai tunnel được yêu cầu |
| Muse | Có GitHub connector và hội thoại Muse đang sử dụng | **Chưa chứng minh** Muse có quyền tạo tín hiệu task đúng schema trên GitHub và Windows có thể đọc/claim/ack tín hiệu đó |
| Agent Forge | `agent/forge` được quy định ở [agents.yaml](../agents.yaml) | Chưa chứng minh POC có thể tạo chat trong đúng CoS Project/worktree |

**Nguồn bên ngoài (kiểm tra lại theo bản CoS được cài khi thực thi):**

- [CoS Local Control API — docs/setup.md](https://github.com/totec448-spec/chat-on-steroids/blob/main/docs/setup.md#local-control-api). `endpoint.json` chứa PID/port; bearer `token` thay khi app restart; `GET /v1/health`, `GET /v1/inputs`, `GET /v1/sessions/{id}/events` và SSE; second switch `Allow actions` cần bật riêng.
- [Old dispatch skill — RETIRED](../archive/skills/dispatch-plan-tasks/SKILL.md) chỉ tham khảo các điều kiện an toàn, **không chạy skill/scripts cũ**. Quy ước worktree/branch cũ của skill không thay thế [GITHUB_WORKFLOW.md](../GITHUB_WORKFLOW.md) hiện hành.

## 3. Phạm vi POC và ngoài phạm vi

**Phase 1 (P0–P4):** một máy Windows / agent Forge, đúng CoS Project và approved root/worktree, một main chat mới cho thử nghiệm `TEST-PONG-01`; Muse phát tín hiệu GitHub đã cho phép, Poller nhận qua HTTPS outbound, kiểm quyền và chống trùng, gọi CoS loopback, cập nhật receipt/status rút gọn lên GitHub cho Muse quan sát. **Không cần** ChatGPT Web Project mapping. **Phase 2 (P5)** là một lần phê duyệt G2 độc lập sau khi Phase 1 verified: dispatch GitHub Issue G3 thật, kiểm tra local Git và PR/review. Không triển khai Phase 2 cùng Phase 1.

**Ngoài phạm vi POC đầu:** Inbound Relay, Cloudflare Tunnel/Access, Remote MCP để gửi việc, yêu cầu ChatGPT Web Project mapping; Mac/Harbor, Antigravity, nhiều agent đồng thời, worker/helper, quyền đọc mọi lịch sử chat, điều khiển shell tổng quát, giao source code thật khi chưa qua G3, tự phê duyệt thiết kế, auto-merge PR, deployment, thực thi tác vụ do Muse suy đoán và thay đổi CoS source code.

## 4. Các phương án và lựa chọn đề xuất

| Phương án | Điểm mạnh | Điểm yếu | Đánh giá |
| --- | --- | --- | --- |
| Muse → GitHub Issue/Project → **Windows GitHub Poller** → CoS loopback (outbound pull) | Chỉ dùng HTTPS outbound, không cần domain/tunnel/CF service token; tái dùng GitHub task state | Cần xác minh Muse ghi tín hiệu được phép, GitHub token quyền tối thiểu, claim/idempotency/receipt, polling delay và chống tín hiệu giả | **ĐÃ CHỌN cho POC; tính khả thi vẫn phải test P0–P4** |
| Muse → Cloudflare Access → **Inbound Relay** → CoS loopback | Receipt có thể nhanh hơn, nhưng thêm public route/auth/tunnel/secret | Nhiều thành phần, rủi ro cấp auth header cho Muse | **Loại khỏi phạm vi POC; chỉ xem xét lại nếu Poller không khả thi và chủ dự án yêu cầu** |
| Muse → Cloudflare → Local Control API trực tiếp | Ít thành phần | Phơi raw API có quyền đọc chat và gửi tin | **Loại bỏ hoàn toàn** |
| Muse → Remote MCP connector để gửi việc | Có thể truyền trực tiếp | Cần hạ tầng và quyền kết nối khác | Không phát triển trong hướng đã chọn |
| SSH + dispatch-plan-tasks retired | Có ghi chép vận chuyển cũ | Không đúng điều phối cloud, không theo dõi lâu dài, skill retired | Không tái kích hoạt |
| Browser GUI automation | Không cần Local API | Dễ gãy khi UI cập nhật, khó nhận receipt chắc chắn | Chỉ phương án dự phòng sau khi local API không phù hợp |

**Kết luận F-01 theo quyết định chủ dự án:** **Outbound GitHub Poller** là hướng duy nhất cần kiểm chứng trong P0–P4; không khảo sát khả năng inject Cloudflare headers của Muse như một cổng bắt buộc. Muse chỉ phát tín hiệu bằng GitHub workflow được phép; Windows chủ động đọc tín hiệu qua GitHub API, tự xác minh, gọi CoS loopback. Poller **không** coi Issue body/comment là lệnh, không tạo hàng đợi task thứ hai trong Git. Phase 1 cần xác định và phê duyệt một tín hiệu `TEST-PONG-01` từ Issue phối hợp/test được phép (hoặc trường trạng thái GitHub đã chốt), **không tạo Issue implementation G3 giả**. Nếu quyền GitHub/Poller không đáp ứng P0, ghi `BLOCKED` và báo lại chủ dự án; **không tự chuyển sang Inbound Relay**.

**Thiết kế mục tiêu:** Poller/dispatcher là process hạ tầng **tách khỏi Rust CMS**, chạy trên Windows dưới quyền user đã chốt, chỉ dùng GitHub HTTPS outbound + CoS `127.0.0.1` loopback. Không expose raw CoS `/v1/*`, không mở endpoint `/dispatch` qua HTTP. Có local durable ledger, audit đã redacted, single-job lock, kill switch, kiểm ACL/allowlist và supervisor cùng vòng đời CoS. **Inbound Relay có thể bỏ hẳn khi Poller được kiểm chứng end-to-end; không xây, không vận hành song song để dự phòng tự động.**

## 5. Sơ đồ và ranh giới tin cậy

```text
Muse (cloud; GitHub connector theo quyền được cấp)
  │  chỉ tạo tín hiệu/metadata thuộc Issue/Project được phê duyệt
  ▼
GitHub Issue/Project (source of truth: task ID, owner, gate, SHA)
  ▲                                        │
  │ receipt/status redacted                │ HTTPS outbound / read / claim
  │                                        ▼
  └────────────── Windows GitHub Poller + Local Supervisor
                      │ validate allowlist, trusted source, local state
                      │ durable ledger + one-active-job + idempotency
                      ▼
                  CoS Local Control API (127.0.0.1 + local bearer token)
                      │ POST /v1/inputs {id, projectId, text} (no sessionId)
                      ▼
                  Forge main chat ở approved CoS Project/worktree
                      │ GET /v1/inputs, /v1/sessions/{id}/events
                      └── redacted result/receipt ──► GitHub ──► Muse

Không inbound port/relay/tunnel; không phụ thuộc ChatGPT Web Project.
```

**Điều kiện CoS Project (F-03, R2-03; chủ dự án đã chốt):** `projectId` là ID CoS Project/workspace. **P0-03 phải khám phá và ghi rõ cơ chế ánh xạ thực tế** `projectId` (UUID) ↔ canonical root/worktree Forge trên CoS v2.1.50: kiểm tra CoS Project trong UI/cấu hình local được phép và đối chiếu với route/metadata API **nếu phiên bản cài đặt thực sự hỗ trợ**; không tự đặt tên endpoint liệt kê projects. Sau khi có bằng chứng đúng root, **pin UUID + canonical path vào allowlist cục bộ** để Poller kiểm tra trước P1-01 và mỗi lần dispatch; nếu mapping đổi/mất bằng chứng thì BLOCKED. P1 đối chiếu `GET /v1/sessions/{input_uuid}` + events và main chat mới để xác nhận đúng CoS Project/Forge, không phải worker/helper hoặc chat đang làm việc; không log ID/chat private. **Không kiểm hay bắt buộc ChatGPT Web Project mapping**: chat nằm ngoài Web Project vẫn đạt nếu tạo/giao đúng CoS workspace và trả kết quả được. Nếu ngay cả CoS Project/worktree không xác định được, P0-03 `BLOCKED`; không fallback vào chat/project chung. CoS workspace isolation không đồng nghĩa tự động có ngữ cảnh ChatGPT Web Project; nhiệm vụ phải cung cấp đủ hướng dẫn/đường dẫn Git nguồn theo contract.

**Khả năng Muse/GitHub (P0-04):** phải chứng minh Muse qua GitHub connector có thể tạo/cập nhật **tín hiệu task được phép**, với Issue/Project/repo ID xác thực; Windows có credential API tối thiểu để đọc tín hiệu và ghi receipt/status, nhận diện nhiệm vụ đã xử lý và chống replay. Không yêu cầu Muse có HTTP/MCP tùy ý, không gửi token qua GitHub Issue/chat, không giả định mọi GitHub label/comment từ user khác là lệnh hợp lệ. Chốt schema tín hiệu P0/P1 trên cơ sở GitHub API/permission thực sự kiểm chứng được.

## 6. Giao thức GitHub Poller đề xuất (draft, chưa có implementation)

Không có endpoint `/dispatch`, `/jobs` hoặc `/health` mở qua HTTP để Muse gọi đến Windows. Giao tiếp từ xa chỉ qua GitHub API theo quyền đã cấp; Local CoS API tiếp tục bind `127.0.0.1` và bearer token chỉ đọc trên Windows.

| Hợp đồng | Phạm vi, trách nhiệm |
| --- | --- |
| **Task signal (R2-01)** | Muse phát tín hiệu trên **Issue phối hợp đã allowlist**, gắn **`github_signal_id` là ID cụ thể của GitHub issue comment/timeline event**, repo/Issue ID, task ID, agent `forge/windows`, mode và `request_id` ổn định. **ID của comment/event cố định nhưng nội dung comment vẫn sửa được**; không tin snapshot label/body/Project field là chứng cứ actor |
| **Poll/validate (R2-01)** | Poller truy xuất event/comment bằng ID, xác minh actor thực sự (`user.login` hoặc `actor.login`) theo **allowlist danh tính** và quyền trên repo; với comment, kiểm `author_association` theo policy, ví dụ `OWNER` cho chủ repo, **và** `created_at == updated_at` để chặn comment đã sửa. Snapshot payload/metadata đã chấp nhận được băm và pin bền vững; re-fetch + kiểm phiên bản/hash trước dispatch để chặn sửa sau lúc đọc. Timeline labeled event cần kiểm actor từ event, không dùng `issue.user` thay actor. Không nhận prompt/shell qua Issue/comment |
| **Claim/dedup (R2-02)** | Một active job cho Forge; ledger cục bộ persist `github_signal_id`, `request_id`, payload hash, target, input UUID; unique constraint riêng cho **(repo, github_signal_id)** và **(repo, request_id)**. Trùng event hoặc request chỉ trả receipt đã lưu, không tạo chat; conflict metadata/hash ghi BLOCKED. Sau restart/delivery mơ hồ reconcile trước khi hành động mới |
| **Local dispatch** | Chỉ local Poller sinh prompt cố định; `POST /v1/inputs` với `id`, approved CoS `projectId`, không `sessionId`; CoS 202 chỉ là queued, không phải finished |
| **Receipt/status** | Poller ghi status rút gọn, lỗi đã làm sạch và kết quả PONG vào **chính Issue/Project đã được phép**, kèm `request_id` để Muse truy xuất. Quy tắc ai được ghi và status nào phải được kiểm trước P4; không push raw chat/events/token lên GitHub |

**Phase 1 tín hiệu mẫu (logic, không phải API hay Issue đã tạo):** `task_id=TEST-PONG-01`, `target=forge/windows`, `mode=poc-pong`, `github_signal_id=<GitHub event/comment ID>`, `request_id=<UUID ổn định gắn với signal ID>`, `source=approved POC signal`. **Chưa chốt dạng GitHub event/comment thực tế;** P0-04/P0-05 phải tìm schema có thể đọc/ghi bằng GitHub connector với quyền và actor được kiểm chứng, xác định cơ chế sinh/pin `request_id` duy nhất từ `github_signal_id` (ví dụ deterministic UUID từ repo ID + signal ID, không dựa vào ID do comment tự khai) trước khi dispatch. Không yêu cầu Issue triển khai G3 giả, không thực thi Issue bất kỳ do người ngoài sửa.

**Conditional polling (R2-01):** lưu `ETag` theo URL/API representation và gửi `If-None-Match` ở lần poll tiếp theo; `304 Not Modified` không parse/dispatch lại, dùng backoff và tôn trọng cả primary/secondary rate limit (`Retry-After`, reset headers). GitHub conditional `304` được xác thực thường không tính vào **primary** rate limit theo API docs, **không bảo đảm polling liên tục miễn phí**. ETag là tối ưu cache, **không thay** xác minh signal ID, actor, version/hash hay ledger; khi API không cấp ETag thì dùng polling giới hạn hợp lý.

**Security invariant cho Phase 2 (F-02):** Poller không bao giờ lấy `issue.body`, comments hoặc text mutable từ GitHub làm prompt, không ghép shell/tool commands từ signal. Chỉ nhận ID/metadata allowlisted (`issue_number`, `task_id`, `source_path`, `source_sha`, `base_sha`, target, request ID). Windows tự `git fetch`, xác minh commit `source_sha` là immutable Git object **được phê duyệt** và nằm trong `origin/main` đã duyệt, đọc file `source_path` trong `docs/plans/` tại chính SHA đó, đối chiếu gate G3 `READY`, Issue/PR và file ownership. Chỉ sau đó local dispatcher sinh một **bootstrap prompt cố định** hướng Forge đọc nguồn Git tại SHA đã pin; đặc tả được đọc vẫn là dữ liệu cần đối chiếu với PRD/contract, không phải chỉ thị có quyền vượt guard. Không tự cấp quyền G3, viết file ngoài ownership hoặc tự merge.

**Local Git preflight cho Phase 2 (F-07):** Windows mới có thể xác minh root/worktree thật: canonical root allowlist, `git branch --show-current` = `agent/forge`, `git status --porcelain` rỗng (gồm untracked), HEAD đúng `base_sha` đã duyệt, không có remote divergence/PR đang chiếm branch. Nếu sai, lưu `WORKTREE_DIRTY_OR_MISMATCH`/`BLOCKED` vào local ledger, **0 CoS send**; không còn HTTP 409 vì không có relay HTTP. Cần tái kiểm ngay trước enqueue trong cùng single-job lock, bảo vệ uncommitted changes.

**Receipt:** trạng thái `RECEIVED/VALIDATED/DISPATCHING_UNKNOWN/QUEUED/DELIVERED/RESPONDED/BLOCKED/UNKNOWN/ABORTED_UNCONFIRMED` lưu bền vững local; khi publish lên GitHub chỉ ghi `request_id`, trạng thái đã kiểm chứng, thời điểm, mã lỗi an toàn và `PONG` nếu thực sự có phản hồi. Muse đọc GitHub receipt, không đọc CoS logs/historical sessions, không xem CoS `202` là task xong. Nếu GitHub API thất bại khi ack, retry **ghi receipt** idempotent theo ID, **không** gửi lại input CoS.

### Idempotency và retry

1. Lưu `github_signal_id` (định danh event/comment gốc, kèm repo ID), `request_id` UUID **gắn hoặc dẫn xuất tất định từ signal ID**, payload hash (snapshot tín hiệu được phép), `input_uuid`, project và trạng thái vào **durable local ledger** (ví dụ SQLite WAL với commit bền vững) trước khi gọi CoS. Đặt **hai UNIQUE constraints độc lập** cho `(repo_id, github_signal_id)` và `(repo_id, request_id)`; một composite unique `(github_signal_id, request_id)` đơn lẻ **không đủ** chống reuse một phía. Ledger không chứa planning state và không commit lên Git.
2. Cùng `github_signal_id`/`request_id` + cùng payload hash trả **receipt cũ** (qua GitHub status/ledger), **0 CoS send**; cùng signal ID hoặc request ID nhưng payload/ID gắn kết khác thì ghi `CONFLICT_PAYLOAD_MISMATCH` (`BLOCKED`, **0 CoS send**) vào ledger/audit và publish **error code đã rút gọn** lên GitHub receipt được allowlist. **Không có HTTP 409** vì Poller không mở relay; không ghi đè receipt cũ, không replay. Re-check actor + immutable signal identity + snapshot hash trước bước `DISPATCHING_UNKNOWN`.
3. **Trước khi mở socket/POST**, commit+flush trạng thái `DISPATCHING_UNKNOWN` cho UUID đã dành sẵn. Crash ở bất kỳ bước nào từ đây cho tới khi có chứng cứ phải phục hồi thành `UNKNOWN`, tuyệt đối không gửi lại tự động. Chỉ khi nhận HTTP `202` mới chuyển `QUEUED`; `202` = stored, chưa phải delivered/completed.
4. Reconcile theo UUID cố định: (a) `GET /v1/inputs` và đối chiếu delivery (`pending`, `sent`, `not_sent`, `unconfirmed`); (b) nếu outbox đã evict (tối đa 50 finished inputs), kiểm `GET /v1/sessions/{input_uuid}/events` vì new-chat `sessionId = input_uuid` **theo upstream**, rồi xác minh cụ thể trên CoS bản cài. Có sự kiện receipt/turn thì gắn bằng chứng; thiếu cả hai hoặc lỗi không xác định giữ `UNKNOWN`/BLOCKED, **không tạo UUID mới hoặc retry mù**.
5. Ghi cursor cuối đã xử lý để resume sau restart; event/response có thể đến nhiều lần. `turn_end` chỉ xác nhận lượt kết thúc, không suy code task hoàn thành. Giới hạn **một job active trên Forge** trong POC, signal tiếp theo được đánh dấu `busy`/deferred chứ không chen vào chat khác. Có **CLI operator cục bộ, xác thực bằng quyền hệ điều hành** (ví dụ `poller-ctl inspect <job_id>`, `poller-ctl resolve <job_id> --mark aborted-unconfirmed|delivered --evidence <ref>`): khóa thao tác khi đang chạy, ghi audit, yêu cầu bằng chứng cho `delivered`; `aborted-unconfirmed` chỉ giải phóng slot sau đối chiếu thủ công và **cấm replay cùng job/UUID**. Không expose CLI qua GitHub/Muse.

**Trạng thái local dispatch (không thay thế Project Status):** `RECEIVED → VALIDATED → DISPATCHING_UNKNOWN → QUEUED → DELIVERED → RESPONDED` hoặc `BLOCKED` / `UNKNOWN` / `ABORTED_UNCONFIRMED`. `CONFLICT_PAYLOAD_MISMATCH` là **mã lỗi của BLOCKED**, không phải HTTP response. `RESPONDED` chỉ khi có message/turn result quan sát được; `202` chỉ tới `QUEUED`. Khi dùng task code, GitHub `In review` cần PR/verification riêng và `Done` chỉ sau G5 thực chạy.

## 7. Bảo mật, quyền và dữ liệu

- **Deny-by-default:** cloud không được truy cập trực tiếp CoS port, token, events, logs, file system hay MCP rộng. Poller allowlist repo/Issue/project/agent/mode/ID, quyền actor và expected gate; validate schema/size/UUID, quota/rate limits; không có route relay HTTP từ remote.
- **Secrets:** CoS token mới mỗi lần app khởi động, chỉ được đọc cục bộ từ CoS user-data; không in ra console, GitHub Issue/PR hay prompt Muse. GitHub token chỉ ở Windows secret store có ACL, quyền repo tối thiểu và lịch rotate/revoke; không đưa secrets vào GitHub signal/LLM tool arguments.
- **Access (F-04, theo transport đã chọn):** không cần Cloudflare Service Auth/JWT/Relay-Secret vì **không có HTTP inbound**. Thay bằng **GitHub authenticated actor + GitHub API permissions + kiểm nguồn task/Issue/Project allowlist ở Poller**. GitHub login hoặc thay đổi label một mình **không đủ** để cấp quyền task; hành động dispatch phải đi qua gate đã phê duyệt, không dùng nội dung Issue/comment làm lệnh.
- **Audit:** chỉ ghi request ID hash, state transition, timestamp, target, kết quả auth và mã lỗi đã làm sạch; loại bỏ raw prompt, full chat history, secret, headers auth và private messages. Giới hạn thời gian lưu local, quyền file theo user; định nghĩa retention trong G2.
- **Least privilege CoS:** ở phép thử PONG không cấp quyền viết file/terminal cho chat mới; kiểm tra quyền app/Project trước khi gửi. Lời dặn "không dùng tools" trong prompt **không thay thế** permission enforcement.
- **Giới hạn provider:** không tự tiếp tục khi ChatGPT từ chối/giới hạn sử dụng; không dùng tunnel/worker/chat mới để vượt hạn chế. Khi không đủ quyền hoặc network bị chặn => ghi `BLOCKED`.

## 8. Lifecycle đúng yêu cầu người dùng

**Đích:** khi CoS **đang mở và Local API hợp lệ**, supervisor cho Poller hoạt động qua GitHub HTTPS outbound; khi CoS **đóng**, không gửi dispatch và không giữ đường điều khiển CoS hữu dụng. Không tạo tunnel/domain cho POC. Không yêu cầu người dùng mở Poller thủ công mỗi lần bật CoS sau khi lifecycle được thiết kế/kiểm chứng.

**Đề xuất lifecycle sau review F-06 / R2-03:** dùng **Local Supervisor / Health Watcher**, không bọc launcher Electron. Trên Windows, P0-01 xác minh **đúng đường dẫn user-data của bản CoS cài đặt** (ứng viên `%APPDATA%/chat-on-steroids/control-api/`): `endpoint.json` chứa port/PID/version; **`token` là file riêng** `control-api/token` theo CoS upstream/archived script (không chứa token trong `endpoint.json`). Supervisor theo dõi `mtime`/**nội dung thay đổi** của **cả hai file**, kiểm file permissions, nạp lại **đồng bộ** cặp `(port, bearer)` sau restart, đối chiếu PID **kèm process creation time**, rồi gọi `GET /v1/health` qua bearer. Không log token/đường dẫn riêng tư; nếu một file thiếu/stale, đọc giữa lúc rotate hoặc version khác contract: **BLOCKED/fail-closed**, xác minh lại thay vì dùng token/port cũ. Kiểm `Allow actions` theo capability/route **thực tế** trên bản cài ở P0 (không phỏng đoán field `actions.enabled`). Chỉ khi auth+health+quyền và `projectId`↔worktree pinned allowlist đều hợp lệ mới dispatch; nếu app update/sleep/wake/crash/close thì fail-closed. **Mục tiêu P3:** trong **≤5 giây tính từ khi CoS đóng hoặc mất health**, Poller ngăn dispatch mới (đo cả polling/timeout), kiểm cả in-flight request. Poller giữ ledger/cursor khi offline để resume an toàn, không resend `UNKNOWN`. Không quản lý `cloudflared`/Job Object. Windows startup/lifecycle phải được nghiệm thu trước khi PASS.

## 9. Các cổng triển khai và điều kiện chốt

| Gate | Phạm vi | Điều kiện để sang gate tiếp theo |
| --- | --- | --- |
| **P0 — contract/khả thi** | Xác nhận version CoS, Local API/Allow actions, CoS Project/worktree, quyền Muse GitHub connector và khả năng Windows poll/ack GitHub | Có evidence khả thi của Poller và approved PONG signal; **không** yêu cầu ChatGPT Web Project hoặc Muse HTTP auth |
| **P1 — loopback** | Test 1 `TEST-PONG-01` qua CoS loopback | Main chat mới đúng **CoS Project/Forge**, không worker; outbox delivery + event response verified |
| **P2 — Poller nội bộ** | Thử GitHub signal trust/allowlist, dedup, retry, restart, redaction, local ledger/CLI với mock + CoS | Negative cases PASS; không nhận lệnh từ text GitHub hoặc public HTTP |
| **P3 — Poller/lifecycle** | GitHub API outbound + supervisor; thử CoS stop/start/crash/reconnect, token revoke, mạng 403/429 | Không dispatch khi CoS offline (mục tiêu ≤5 giây), không leak hay gửi trùng |
| **P4 — Muse round-trip** | Muse phát PONG GitHub signal, Poller dispatch và cập nhật receipt/status PONG trên GitHub | Một job duy nhất, Muse đọc kết quả qua GitHub, retry/resume idempotent, không chỉnh CMS code |
| **P5 — GitHub end-to-end (Phase 2)** | Issue đủ G3 READY → Git blob SHA+local Git preflight → Forge → PR/review → G5 | **G2 riêng** sau P0–P4 VERIFIED; truy vết ID/SHA, không tự sửa contract/merge/Done |

**Bàn giao từng gate:** file evidence trong `docs/verification/` gồm timestamp, môi trường, SHA CoS/dispatcher, test steps, expected/actual, PASS/FAIL/NOT_RUN, log đã redacted, rủi ro còn lại; một PR qua CI/review trước khi xem gate VERIFIED. **G2 Phase 1** chỉ bao P0–P4 PONG và cần quyết định của chủ dự án trước G3; **G2 Phase 2** cho P5 chỉ được đề xuất chốt sau khi Phase 1 thực sự VERIFIED. Không khai P5 READY chỉ vì PONG thành công.

## 10. Câu hỏi cần chốt trước khi viết code

1. **ĐÃ CHỐT:** Chỉ yêu cầu giao việc đúng **CoS Project/worktree và Forge main chat**, không cần ChatGPT Web Project. P0 còn phải xác minh root/ID và khả năng tạo chat thật.
2. **ĐÃ CHỐT:** Dùng **Outbound GitHub Poller**; không triển khai Inbound Relay/Cloudflare trong Phase 1 và bỏ phương án đó khi Poller PASS. **Còn phải chốt/verify:** schema GitHub PONG signal, quyền Muse ghi và Windows đọc/ack, allowed Issue/Project, polling interval và rate limit.
3. Dispatcher/supervisor nằm ở thư mục hạ tầng nào (tách CMS runtime), lưu idempotency ledger/secret ở đâu trên Windows và cách rotate/revoke? Không đưa runtime data vào Git.
4. Supervisor theo dõi CoS/app updates và tự dừng/khôi phục Poller qua Windows startup như thế nào để đạt mục tiêu ≤5 giây?
5. Cấp độ quyền của phép thử PONG và task thật: CoS Read-only, file approvals, chủ thể được phép approve, quy tắc pause/kill switch?
6. Giữ nguyên branch cố định `agent/forge` và một task active theo [quy trình hiện hành](../GITHUB_WORKFLOW.md); nếu cần task worktree riêng, phải duyệt thay đổi quy ước trước.

## 11. Decision / promotion reference (chỉ điền sau khi được phê duyệt)

**ĐÃ CÓ QUYẾT ĐỊNH CỤ THỂ của chủ dự án:** dùng Outbound GitHub Poller và cho phép CoS Project isolation; **không còn mở** transport fork hay yêu cầu ChatGPT Web Project mapping. **CHƯA có phê duyệt G2 đầy đủ cho hai phase hoặc bằng chứng P0–P5 runtime.** Sau Nova F-01–F-09, đề xuất tách **G2 Phase 1** (P0–P4, chỉ `TEST-PONG-01`, không cấp quyền thao tác source/terminal; CoS vẫn cần quyền tạo chat mới do chủ máy bật) khỏi **G2 Phase 2** (P5 GitHub Issue G3 thật). Cần duyệt contract GitHub signal/permissions/receipt và scope Phase 1 trước implementation; chỉ sau evidence P0–P4 VERIFIED mới đề xuất/chốt contract Phase 2. Đưa kết luận đã duyệt vào `docs/plans/` hoặc ADR/contract; không tạo Issue implementation G3 READY từ tài liệu `PROPOSED`, không coi PR review/merge là test runtime.
