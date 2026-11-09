# Research: Muse Orchestrator POC — CoS Windows / OpenDash CMS

- **Agent / ngày:** Forge (Chat On Steroids Windows), 2026-11-09
- **Source SHA đối chiếu:** `8833fc0f9f13903373ace8cdb4e67ee29b8cfb16` (`main`, PR #3 đã merge)
- **Trạng thái:** **PROPOSED — CHƯA PHÊ DUYỆT THIẾT KẾ / CHƯA TRIỂN KHAI**
- **Liên quan:** [GitHub workflow](../GITHUB_WORKFLOW.md), [Agent registry](../agents.yaml), [Issue #1 đã verified](https://github.com/thientran2359/opendash-cms/issues/1), [Project #3](https://github.com/users/thientran2359/projects/3)
- **Kế hoạch kiểm thử:** [Muse POC test matrix](muse-cos-win-poc-test-matrix.md)
- **Phản biện:** [Nova — PR #4, 09/11/2026](https://github.com/thientran2359/opendash-cms/pull/4) (F-01–F-09); các bổ sung bên dưới là phương án sửa sau review, **chưa phải G2 AGREED**.

## 1. Mục tiêu và nguyên tắc

Chứng minh **Muse từ cloud điều phối một task vào một _main chat_ mới của CoS trên Windows**, thuộc đúng Project `opendash-cms`; quan sát được việc gửi, phản hồi và lỗi, không dùng CoS broker worker/helper, không thay chat đang làm việc, không triển khai mã CMS trong phép thử đầu tiên.

POC đi theo nguyên tắc: **xác minh local CoS → P0 chọn transport có bằng chứng → kiểm tra đường gửi/nhận và lifecycle → Muse round-trip**. Với transport inbound mới cần relay, Cloudflare Access và tunnel; với outbound phải kiểm chứng GitHub poller riêng. Phải PASS từng cổng trước khi bật cổng sau. Việc viết tài liệu hoặc bật Cloudflare không đồng nghĩa đã có năng lực điều phối. Không tự động mở rộng sang Mac, Antigravity, deploy, merge PR hoặc task G3 chưa `READY`.

**Quy ước nguồn sự thật:** tài liệu `docs/plans/` và `docs/contracts/` chứa đặc tả đã duyệt; GitHub Issue/Project giữ assignment và trạng thái; PR/`docs/verification/` là review và evidence. Relay giữ **receipt kỹ thuật** cục bộ để chống gửi trùng, không tạo thêm bảng trạng thái task song song trong Git. `MERGED` không bằng `VERIFIED`.

## 2. Facts đã kiểm tra và những điều chưa xác minh

| Thành phần | Có bằng chứng | Chưa được suy ra |
| --- | --- | --- |
| GitHub | Issue Forms, Milestones, Labels, Project #3, Ruleset và CI đã verified tại [G5](../../docs/verification/2026-11-09-github-governance-g5.md) | Chưa có Muse tự cập nhật Project hay tự giao task |
| Windows CoS | Ứng dụng v2.1.50 đang chạy khi khảo sát; có file endpoint của Local Control API | Chưa xác minh Allow actions, khả năng tạo direct main chat và Project mapping trên chính máy này |
| CoS API theo upstream | Local API chỉ bind loopback; `POST /v1/inputs` có `id`, `projectId`, `text`, không có `sessionId` thì tạo new chat với session ID bằng input UUID; events/outbox để theo dõi | Không mặc định build cài trên máy giống chính xác HEAD upstream; không coi 202 là chat đã trả lời; `projectId` không chứng minh ChatGPT Web Project |
| Cloudflare | Windows đã có `cloudflared` | Chưa chạy tunnel; chưa xác thực named tunnel, DNS, Access policy hoặc vòng đời tự động |
| Muse | Có GitHub connector và hội thoại Muse đang sử dụng; từng phác thảo relay thử nghiệm | **Chưa xác minh** Muse có công cụ HTTP/MCP gọi endpoint có xác thực và quản lý service token an toàn |
| Agent Forge | `agent/forge` được quy định ở [agents.yaml](../agents.yaml) | Chưa chứng minh POC có thể tạo chat trong đúng **cả CoS Project và ChatGPT Project** |

**Nguồn bên ngoài (kiểm tra lại theo bản CoS được cài khi thực thi):**

- [CoS Local Control API — docs/setup.md](https://github.com/totec448-spec/chat-on-steroids/blob/main/docs/setup.md#local-control-api). `endpoint.json` chứa PID/port; bearer `token` thay khi app restart; `GET /v1/health`, `GET /v1/inputs`, `GET /v1/sessions/{id}/events` và SSE; second switch `Allow actions` cần bật riêng.
- [Cloudflare Access service tokens](https://developers.cloudflare.com/cloudflare-one/access-controls/service-credentials/service-tokens/) — Access policy `Service Auth`, Client ID và Secret được giữ trong kho bí mật ngoài Git.
- [Old dispatch skill — RETIRED](../archive/skills/dispatch-plan-tasks/SKILL.md) chỉ tham khảo các điều kiện an toàn, **không chạy skill/scripts cũ**. Quy ước worktree/branch cũ của skill không thay thế [GITHUB_WORKFLOW.md](../GITHUB_WORKFLOW.md) hiện hành.

## 3. Phạm vi POC và ngoài phạm vi

**Phase 1 (P0–P4):** một máy Windows / agent Forge, một CoS Project đã được user phê duyệt, một ChatGPT Project đích `opendash-cms`, một chat mới cho thử nghiệm `TEST-PONG-01`; cơ chế nhận request, ack, tra cứu trạng thái, session receipt; phân biệt online/offline; thử retry/idempotency và kiểm soát truy cập. **Phase 2 (P5)** là một lần phê duyệt G2 độc lập sau khi Phase 1 verified: dispatch GitHub Issue G3 thật, kiểm tra local Git và PR/review. Không triển khai Phase 2 cùng Phase 1.

**Ngoài phạm vi POC đầu:** Mac/Harbor, Antigravity, nhiều agent đồng thời, worker/helper, quyền đọc mọi lịch sử chat, điều khiển shell tổng quát, giao source code thật khi chưa qua G3, tự phê duyệt thiết kế, auto-merge PR, deployment, thực thi tác vụ do Muse suy đoán và thay đổi CoS source code.

## 4. Các phương án và lựa chọn đề xuất

| Phương án | Điểm mạnh | Điểm yếu | Đánh giá |
| --- | --- | --- | --- |
| Muse → Cloudflare → **Local Control API trực tiếp** | Ít thành phần | Phơi ra API có thể đọc chat và gửi tin toàn quyền | **Loại bỏ** |
| Muse → Cloudflare Access → **relay hẹp trên Windows** → CoS loopback (inbound push) | Receipt ngay qua API, auth/rate limit/audit/idempotency | Chỉ khả thi khi Muse/connector inject bí mật **server-side**, cần tunnel + supervisor | **Ứng viên nếu P0-04 chứng minh được auth** |
| Muse → GitHub Issue/Project → **Windows GitHub Poller** → CoS loopback (outbound pull) | Chỉ dùng HTTPS outbound, không cần public domain/tunnel/CF service token; tái dùng GitHub task state | Phải cấp token GitHub tối thiểu tại Windows, thiết kế nguồn tín hiệu/claim/receipt, polling delay và bảo vệ chống comment/Issue giả | **Ứng viên thay thế nếu Muse chỉ có GitHub connector** |
| Muse → authenticated **Remote MCP connector hẹp** → relay Windows | Bí mật giữ phía connector, scope transport rõ | Cần chứng minh connector tồn tại, vận hành và auth riêng; có thể vẫn cần tunnel | Ứng viên sau P0, chưa kiểm chứng |
| SSH + dispatch-plan-tasks retired | Có ghi chép vận chuyển cũ | Không đúng điều phối cloud, không theo dõi lâu dài, skill retired | Không tái kích hoạt |
| Browser GUI automation | Không cần Local API | Dễ gãy khi UI cập nhật, khó nhận receipt chắc chắn | Chỉ phương án dự phòng sau khi local API không phù hợp |

**Architecture fork tại P0-04 (F-01):** chỉ chọn inbound khi một công cụ Muse **thực sự** gửi được credential qua secret store của connector, ngoài LLM prompt/tool parameters, có receipt xác thực. Nếu không, **BLOCKED đối với inbound** và trình hai ứng viên outbound poller/remote MCP để chủ dự án chọn; không tự bật tunnel hoặc tự chuyển transport. Với poller, Muse chỉ phát tín hiệu bằng workflow GitHub đã cho phép; Windows đọc trạng thái qua GitHub API, tự xác minh và gọi CoS loopback. Poller không được coi Issue body/comment là lệnh, không tạo hàng đợi task thứ hai trong Git. Phase 1 cần chốt một tín hiệu `TEST-PONG-01` được allowlist qua một Issue quy trình đã phê duyệt hoặc kênh GitHub thích hợp; **không tạo Issue implementation giả**.

**Nếu chọn inbound:** xây **Muse Dispatch Relay** độc lập với mã Rust CMS. Relay chỉ bind `127.0.0.1`, gọi CoS Local API loopback, không expose raw `/v1/*` hoặc lịch sử chat. Đường network công khai chỉ vào Cloudflare Tunnel → Cloudflare Access → relay có xác thực riêng; ngôn ngữ/khung web cần chốt khi thiết kế G2. **Nếu chọn outbound:** poller/supervisor cục bộ giữ receipt/idempotency tương đương và không mở endpoint nhận public.

## 5. Sơ đồ và ranh giới tin cậy

```text
Muse (cloud; GitHub Issue/PR tools)
  │  [A] inbound: connector auth server-side (chỉ nếu P0-04 PASS)
  ▼
Cloudflare Access (service token + policy theo app)
  ▼  cos-win.woocommerce.vn  [named tunnel; không public CoS port]
cloudflared trên Windows
  ▼  localhost ONLY
Muse Dispatch Relay  [allowlist / Access JWT + bearer nội bộ / rate limit / durable ledger]
  ▼  127.0.0.1 + token CoS chỉ đọc tại Windows
CoS Local Control API
  ▼  POST /v1/inputs {id, projectId, text} (không sessionId)
CoS direct main chat ↔ ChatGPT Project `opendash-cms` (chưa verified)
  ▼  GET /v1/inputs, sessions/{id}/events (redacted)
Relay job status → Muse → GitHub Issue/Project (sau gate)

[B] outbound alternative (chỉ sau quyết định P0):
Muse → GitHub approved signal/Issue → Windows poller/supervisor
     → local validation + receipt ledger → CoS loopback
     → redacted receipt/status → GitHub → Muse
     (không Cloudflare, không endpoint public)
```

**Không nhầm Project (F-03):** `projectId` là ID CoS Project/workspace, **không có trường `chatgptProjectId` trong request Local API đã được mô tả**. P0-03 phải đọc metadata CoS Project tại Windows (ID, root/worktree), quan sát URL/DOM của **main chat mới** trong trình duyệt, đối chiếu với ChatGPT Web Project `opendash-cms`, và kiểm `GET /v1/sessions/{input_uuid}` + events (nếu có project metadata) để chứng minh session là chat chính, không phải worker/helper. Phải giữ bằng chứng riêng cho cả hai lớp project, không suy lớp này từ lớp kia; không log ID hoặc chat riêng tư. Nếu bản CoS đang cài chỉ hỗ trợ CoS Project mà không đưa chat vào ChatGPT Web Project, **P0-03 BLOCKED và trình chủ dự án chọn**: tiếp tục yêu cầu đủ hai project hoặc chấp nhận chỉ CoS workspace isolation cho *Phase 1*. Không tự đổi tiêu chí.

**Khả năng Muse gọi mạng:** P0-04 phải chứng minh Muse có authenticated HTTP/remote MCP với static headers được inject **từ vault phía connector**, không do LLM truyền giá trị bí mật. Nếu chỉ có GitHub connector, so sánh outbound poller với remote MCP và xin quyết định; không giả định cloud agent gọi được URL bất kỳ.

## 6. Giao thức relay đề xuất (draft, chưa phải API đã cài)

| Endpoint relay | Phương thức | Giới hạn |
| --- | --- | --- |
| `/health` | GET | Chỉ báo `online/offline/limited`, phiên bản và lý do từ chối không nhạy cảm; không tiết lộ token, PID, danh sách chat |
| `/dispatch` | POST | Chỉ chấp nhận 1 job hợp lệ, đích `forge/windows`, project allowlist, request ID ổn định; ở POC đầu chỉ cho phép `TEST-PONG-01` |
| `/jobs/{job_id}` | GET | Receipt/trạng thái, delivery, phản hồi đã rút gọn có kiểm soát (với PONG chỉ `PONG`), session reference nội bộ nếu được phép; **không** trả raw chat log hoặc toàn bộ lịch sử |

Không có `proxy`, `exec`, `browse`, đọc file tùy ý, `stop`, `workers/*`, liệt kê toàn bộ sessions hoặc URL bất kỳ trong API public. **Xác thực lớp thứ hai là bắt buộc (F-04):** mọi request vào relay, kể cả từ một process khác trên `127.0.0.1`, phải có `Authorization: Bearer <Relay-Secret>` riêng; từ tunnel còn phải xác minh Cloudflare Access JWT (`Cf-Access-Jwt-Assertion`, chữ ký qua JWKS, issuer, audience, thời hạn), deny-by-default nếu thiếu/bất hợp lệ. Cloudflare Access phải chặn caller không có Service Auth trước relay; không tin các header do client tự giả mạo khi request trực tiếp trên localhost. `GET /health` cũng yêu cầu auth cho remote và local. Cả Cloudflare service credential và Relay-Secret phải được connector giữ/inject **server-side**; LLM chỉ được gọi tool không chứa secrets trong arguments. Cơ chế JWT, key rotation và bảo vệ kết nối tunnel→relay phải được xác minh ở P0/P2 trước khi chọn inbound.

**Ví dụ body nháp (chỉ để review schema, không gửi thật):**

```json
{
  "request_id": "6b5ce0b7-84e4-4aa1-a387-2a82aef332a2",
  "mode": "poc-pong",
  "target": "forge/windows",
  "project": "opendash-cms",
  "poc_task": "TEST-PONG-01"
}
```

Relay/poller sở hữu việc tạo text cố định cho `poc-pong`; **không** nhận command hoặc prompt tùy ý từ Internet. **Security invariant cho Phase 2 (F-02):** `/dispatch` chỉ nhận allowlisted IDs/metadata (`issue_number`, `task_id`, `source_path`, `source_sha`, `base_sha`, target) và request ID; từ chối trường `prompt`/`text` lạ. Không chèn `issue.body`, comments, Project field hoặc văn bản tùy ý lấy từ GitHub vào tin gửi CoS. Windows tự `git fetch`, xác minh commit `source_sha` là immutable Git object được phép và nằm trong `origin/main` đã duyệt, chỉ đọc file `source_path` trong `docs/plans/` tại **chính SHA đó**, đối chiếu task ID/gate G3 `READY` và metadata Issue/PR, `base_sha` và file ownership. Sau khi xác minh, bộ gửi sinh **bootstrap prompt cố định từ code**, chỉ dẫn Forge đọc đặc tả đã pin SHA bằng Git local; nội dung đặc tả vẫn là dữ liệu cần đối chiếu với PRD/contract và quyền đã cấp, không có thẩm quyền vượt guard. Issue body chỉ dùng để đối chiếu metadata và phát hiện bất nhất/thu hồi, không phải nguồn lệnh. Relay không tự cấp G3 hoặc quyền file.

**Local Git preflight cho Phase 2 (F-07):** chỉ tiến trình tin cậy trên Windows mới kiểm tra được worktree thực: canonical root được allowlist, `git branch --show-current` = `agent/forge`, `git status --porcelain` rỗng (gồm untracked), HEAD đúng `base_sha` đã duyệt và remote branch không có divergence/PR đang chiếm branch. Lỗi hoặc worktree bận trả `409 WORKTREE_DIRTY_OR_MISMATCH`/`BLOCKED`, **0 CoS send**. Muse kiểm tra GitHub Issue/Project từ cloud nhưng không thể thay kiểm tra local. Cần tái kiểm preflight ngay trước enqueue trong cùng single-job lock, chặn race với tác nhân khác trong phạm vi POC.

**Receipt nháp** cho `POST /dispatch`: `202 {"job_id":"<opaque>","state":"QUEUED","delivery":"pending"}` khi CoS xác nhận đã ghi vào outbox; khi mới tiếp nhận local hoặc chưa gửi được CoS, không được gán `QUEUED`. `GET /jobs/{job_id}` trả `state`, `delivery`, `last_updated_at`, `error_code` đã làm sạch và `result` rút gọn (`PONG` trong phép thử). Không chuyển nguyên văn nội dung sự kiện SSE, tool calls, bearer, file content hay chat history về Muse. Cần chốt schema chi tiết sau P0 và kiểm thử mọi trạng thái biên.

### Idempotency và retry

1. Lưu `request_id`, payload hash, `input_uuid`, project và trạng thái vào **durable local ledger** (ví dụ SQLite WAL với commit bền vững) trước khi gọi CoS; kho này là receipt giao nhận, không lưu planning state và không commit lên Git.
2. Một `request_id` luôn gắn với một payload hash, một CoS input UUID, một target/project; trùng ID + cùng payload trả **receipt cũ**, trùng ID + payload khác trả `409 conflict`.
3. **Trước khi mở socket/POST**, commit+flush trạng thái `DISPATCHING_UNKNOWN` cho UUID đã dành sẵn. Crash ở bất kỳ bước nào từ đây cho tới khi có chứng cứ phải phục hồi thành `UNKNOWN`, tuyệt đối không gửi lại tự động. Chỉ khi nhận HTTP `202` mới chuyển `QUEUED`; `202` = stored, chưa phải delivered/completed.
4. Reconcile theo UUID cố định: (a) `GET /v1/inputs` và đối chiếu delivery (`pending`, `sent`, `not_sent`, `unconfirmed`); (b) nếu outbox đã evict (tối đa 50 finished inputs), kiểm `GET /v1/sessions/{input_uuid}/events` vì new-chat `sessionId = input_uuid` **theo upstream**, rồi xác minh cụ thể trên CoS bản cài. Có sự kiện receipt/turn thì gắn bằng chứng; thiếu cả hai hoặc lỗi không xác định giữ `UNKNOWN`/BLOCKED, **không tạo UUID mới hoặc retry mù**.
5. Ghi cursor cuối đã xử lý để resume sau restart; event/response có thể đến nhiều lần. `turn_end` chỉ xác nhận lượt kết thúc, không suy code task hoàn thành. Giới hạn **một job active trên Forge** trong POC, job mới trả `busy`. Có **CLI operator cục bộ, xác thực bằng quyền hệ điều hành** (ví dụ `relay-ctl inspect <job_id>`, `relay-ctl resolve <job_id> --mark aborted-unconfirmed|delivered --evidence <ref>`): khóa thao tác khi đang chạy, ghi audit, yêu cầu bằng chứng cho `delivered`; `aborted-unconfirmed` chỉ giải phóng slot sau đối chiếu thủ công và **cấm replay cùng job/UUID**. Không expose CLI qua HTTP/Muse.

**Trạng thái local dispatch (không thay thế Project Status):** `RECEIVED → VALIDATED → DISPATCHING_UNKNOWN → QUEUED → DELIVERED → RESPONDED` hoặc `BLOCKED` / `UNKNOWN` / `ABORTED_UNCONFIRMED`. `RESPONDED` chỉ khi có message/turn result quan sát được; `202` chỉ tới `QUEUED`. Khi dùng task code, GitHub `In review` cần PR/verification riêng và `Done` chỉ sau G5 thực chạy.

## 7. Bảo mật, quyền và dữ liệu

- **Deny-by-default:** cloud không được truy cập trực tiếp CoS port, token, events, logs, file system hay MCP rộng. Relay giữ allowlist host, project, agent, mode, kích thước request; validate JSON/UUID/length và rate limit.
- **Secrets:** CoS token mới mỗi lần app khởi động, chỉ được đọc cục bộ từ CoS user-data; không in ra console, đưa lên GitHub Issue/PR hay prompt Muse. Với inbound, Cloudflare service token và Relay-Secret được inject từ **connector server-side secret store**, không từ tool parameters/LLM context; rotate/revoke và giữ ACL tối thiểu. Với outbound, GitHub token chỉ ở Windows secret store, không trong Issue/chat và chỉ có quyền repo cần thiết.
- **Access:** inbound phải có Cloudflare Access `Service Auth`, xác minh JWT có audience hợp lệ **và** Bearer Relay-Secret tại relay. Không coi DNS hoặc gọi `/health` là bằng chứng caller được phép `/dispatch`. Outbound poller chỉ gọi GitHub API với quyền tối thiểu, allowlist repository/Issue/action, không thực thi nội dung issue/comment.
- **Audit:** chỉ ghi request ID hash, state transition, timestamp, target, kết quả auth và mã lỗi đã làm sạch; loại bỏ raw prompt, full chat history, secret, headers auth và private messages. Giới hạn thời gian lưu local, quyền file theo user; định nghĩa retention trong G2.
- **Least privilege CoS:** ở phép thử PONG không cấp quyền viết file/terminal cho chat mới; kiểm tra quyền app/Project trước khi gửi. Lời dặn "không dùng tools" trong prompt **không thay thế** permission enforcement.
- **Giới hạn provider:** không tự tiếp tục khi ChatGPT từ chối/giới hạn sử dụng; không dùng tunnel/worker/chat mới để vượt hạn chế. Khi không đủ quyền hoặc network bị chặn => ghi `BLOCKED`.

## 8. Lifecycle đúng yêu cầu người dùng

**Đích:** khi CoS **đang mở và Local API hợp lệ**, supervisor phục vụ transport đã chọn (inbound relay+tunnel hoặc outbound poller); khi CoS **đóng**, không tiếp nhận dispatch và không còn bề mặt điều khiển CoS hữu dụng; domain/DNS (nếu dùng) giữ nguyên. Không yêu cầu người dùng khởi động thủ công mỗi lần mở CoS sau khi cơ chế lifecycle được chốt.

**Đề xuất lifecycle sau review F-06:** dùng **Local Supervisor / Health Watcher** thay cho launcher-wrapper của ứng dụng Electron nhiều process. Đọc `endpoint.json` + token với ACL cục bộ, kiểm PID **kèm process creation time** và gọi `GET /v1/health` qua bearer; xác minh tình trạng `Allow actions` bằng capability/route thực tế của bản CoS cài ở P0, **không phỏng đoán** field `actions.enabled` luôn có. Chỉ khi auth+health+quyền đều hợp lệ mới nhận dispatch; nếu endpoint stale, token rotate, app update, sleep/wake, crash hoặc CoS đóng: fail-closed, không gửi job mới. **Mục tiêu P3:** chuyển offline và từ chối dispatch trong **≤5 giây tính từ lúc CoS đóng hoặc mất health** (đo end-to-end cả chu kỳ polling/timeout), kiểm cả request đang in-flight. Nếu chọn inbound, supervisor khởi chạy `cloudflared` trong **Windows Job Object** với `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (đảm bảo attach tiến trình trước khi nhận remote, không cho breakaway) để tự dừng khi supervisor mất; thử crash/auto-update/reboot và xử lý orphan. Nếu chọn outbound, dừng poller khi CoS offline. Đây là thiết kế **PROPOSED**, cần thực nghiệm/lựa chọn Windows startup mode trước G2.

## 9. Các cổng triển khai và điều kiện chốt

| Gate | Phạm vi | Điều kiện để sang gate tiếp theo |
| --- | --- | --- |
| **P0 — contract/khả thi** | Xác nhận version CoS, Local API/Allow actions, CoS Project + ChatGPT Project mapping, **transport fork: inbound auth hoặc outbound GitHub** | Evidence để chủ dự án chọn transport/project policy, không có blocker cho nhánh được chọn |
| **P1 — loopback** | Test 1 `TEST-PONG-01` qua CoS loopback, không remote | Chat chính mới đúng project, không worker; outbox delivery + event response verified |
| **P2 — local dispatcher** | Thử auth nếu inbound, allowlist, retry, restart, redaction, local ledger/CLI với mock + CoS thật | Unit/integration + negative cases PASS, không phát lộ raw CoS API |
| **P3 — transport/lifecycle** | **Inbound:** named tunnel+Access+relay auth; **Outbound:** GitHub poller HTTPS; đều thử CoS stop/start/crash/reconnect | Không dispatch khi CoS offline (mục tiêu ≤5 giây), không leak hay gửi trùng |
| **P4 — Muse round-trip** | Muse kích hoạt PONG qua transport đã chốt, theo dõi receipt/status, nhận phản hồi qua kênh cho phép | Một job duy nhất, retry/resume idempotent, không chỉnh CMS code |
| **P5 — GitHub end-to-end (Phase 2)** | Issue đủ G3 READY → Git blob SHA+local Git preflight → Forge → PR/review → G5 | **G2 riêng** sau P0–P4 VERIFIED; truy vết ID/SHA, không tự sửa contract/merge/Done |

**Bàn giao từng gate:** file evidence trong `docs/verification/` gồm timestamp, môi trường, SHA CoS/dispatcher, test steps, expected/actual, PASS/FAIL/NOT_RUN, log đã redacted, rủi ro còn lại; một PR qua CI/review trước khi xem gate VERIFIED. **G2 Phase 1** chỉ bao P0–P4 PONG và cần quyết định của chủ dự án trước G3; **G2 Phase 2** cho P5 chỉ được đề xuất chốt sau khi Phase 1 thực sự VERIFIED. Không khai P5 READY chỉ vì PONG thành công.

## 10. Câu hỏi cần chốt trước khi viết code

1. Nếu CoS chỉ gắn chat với **CoS Project** mà không gắn vào ChatGPT Web Project, chủ dự án muốn **BLOCKED** hay chấp nhận CoS workspace isolation chỉ cho Phase 1?
2. Muse có connector thực sự inject auth headers ở tầng server không? Nếu P0-04 fail, chủ dự án chọn **outbound GitHub poller** hay **remote MCP connector hẹp**? Tín hiệu GitHub PONG nào được phép thử?
3. Dispatcher/supervisor nằm ở thư mục hạ tầng nào (tách CMS runtime), lưu idempotency ledger/secret ở đâu trên Windows và cách rotate/revoke? Không đưa runtime data vào Git.
4. Supervisor theo dõi CoS/app updates và tự quản lý tunnel/poller qua Windows startup như thế nào để đạt mục tiêu ≤5 giây?
5. Cấp độ quyền của phép thử PONG và task thật: CoS Read-only, file approvals, chủ thể được phép approve, quy tắc pause/kill switch?
6. Giữ nguyên branch cố định `agent/forge` và một task active theo [quy trình hiện hành](../GITHUB_WORKFLOW.md); nếu cần task worktree riêng, phải duyệt thay đổi quy ước trước.

## 11. Decision / promotion reference (chỉ điền sau khi được phê duyệt)

**Chưa có quyết định G2 cho cả hai phase.** Sau review Nova F-01–F-09, đề xuất tách **G2 Phase 1** (P0–P4, chỉ `TEST-PONG-01`, không cấp quyền thao tác source/terminal; CoS vẫn cần quyền tạo chat mới do chủ máy bật) khỏi **G2 Phase 2** (P5 GitHub Issue G3 thật). Chủ dự án phải chốt transport fork, project acceptance và phạm vi Phase 1; chỉ sau evidence P0–P4 VERIFIED mới đề xuất/chốt contract Phase 2 riêng. Tổng hợp kết luận đã duyệt vào `docs/plans/` hoặc ADR/contract; không tạo Issue implementation G3 READY từ bản `PROPOSED`, không coi PR review/merge là test runtime.
