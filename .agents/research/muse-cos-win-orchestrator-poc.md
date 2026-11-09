# Research: Muse Orchestrator POC — CoS Windows / OpenDash CMS

- **Agent / ngày:** Forge (Chat On Steroids Windows), 2026-11-09
- **Source SHA đối chiếu:** `8833fc0f9f13903373ace8cdb4e67ee29b8cfb16` (`main`, PR #3 đã merge)
- **Trạng thái:** **PROPOSED — CHƯA PHÊ DUYỆT THIẾT KẾ / CHƯA TRIỂN KHAI**
- **Liên quan:** [GitHub workflow](../GITHUB_WORKFLOW.md), [Agent registry](../agents.yaml), [Issue #1 đã verified](https://github.com/thientran2359/opendash-cms/issues/1), [Project #3](https://github.com/users/thientran2359/projects/3)
- **Kế hoạch kiểm thử:** [Muse POC test matrix](muse-cos-win-poc-test-matrix.md)

## 1. Mục tiêu và nguyên tắc

Chứng minh **Muse từ cloud điều phối một task vào một _main chat_ mới của CoS trên Windows**, thuộc đúng Project `opendash-cms`; quan sát được việc gửi, phản hồi và lỗi, không dùng CoS broker worker/helper, không thay chat đang làm việc, không triển khai mã CMS trong phép thử đầu tiên.

POC đi theo nguyên tắc: **local trước → relay an toàn → tunnel đúng vòng đời → Muse → GitHub**. Phải PASS từng cổng trước khi bật cổng sau. Việc viết tài liệu hoặc bật Cloudflare không đồng nghĩa đã có năng lực điều phối. Không tự động mở rộng sang Mac, Antigravity, deploy, merge PR hoặc task G3 chưa `READY`.

**Quy ước nguồn sự thật:** tài liệu `docs/plans/` và `docs/contracts/` chứa đặc tả đã duyệt; GitHub Issue/Project giữ assignment và trạng thái; PR/`docs/verification/` là review và evidence. Relay giữ **receipt kỹ thuật** cục bộ để chống gửi trùng, không tạo thêm bảng trạng thái task song song trong Git. `MERGED` không bằng `VERIFIED`.

## 2. Facts đã kiểm tra và những điều chưa xác minh

| Thành phần | Có bằng chứng | Chưa được suy ra |
| --- | --- | --- |
| GitHub | Issue Forms, Milestones, Labels, Project #3, Ruleset và CI đã verified tại [G5](../../docs/verification/2026-11-09-github-governance-g5.md) | Chưa có Muse tự cập nhật Project hay tự giao task |
| Windows CoS | Ứng dụng v2.1.50 đang chạy khi khảo sát; có file endpoint của Local Control API | Chưa xác minh Allow actions, khả năng tạo direct main chat và Project mapping trên chính máy này |
| CoS API theo upstream | Local API chỉ bind loopback; `POST /v1/inputs` có `id`, `projectId`, `text`, không có `sessionId` thì tạo new chat; events/outbox để theo dõi | Không mặc định build cài trên máy giống chính xác HEAD upstream; không coi 202 là chat đã trả lời |
| Cloudflare | Windows đã có `cloudflared` | Chưa chạy tunnel; chưa xác thực named tunnel, DNS, Access policy hoặc vòng đời tự động |
| Muse | Có GitHub connector và hội thoại Muse đang sử dụng; từng phác thảo relay thử nghiệm | **Chưa xác minh** Muse có công cụ HTTP/MCP gọi endpoint có xác thực và quản lý service token an toàn |
| Agent Forge | `agent/forge` được quy định ở [agents.yaml](../agents.yaml) | Chưa chứng minh POC có thể tạo chat trong đúng **cả CoS Project và ChatGPT Project** |

**Nguồn bên ngoài (kiểm tra lại theo bản CoS được cài khi thực thi):**

- [CoS Local Control API — docs/setup.md](https://github.com/totec448-spec/chat-on-steroids/blob/main/docs/setup.md#local-control-api). `endpoint.json` chứa PID/port; bearer `token` thay khi app restart; `GET /v1/health`, `GET /v1/inputs`, `GET /v1/sessions/{id}/events` và SSE; second switch `Allow actions` cần bật riêng.
- [Cloudflare Access service tokens](https://developers.cloudflare.com/cloudflare-one/access-controls/service-credentials/service-tokens/) — Access policy `Service Auth`, Client ID và Secret được giữ trong kho bí mật ngoài Git.
- [Old dispatch skill — RETIRED](../archive/skills/dispatch-plan-tasks/SKILL.md) chỉ tham khảo các điều kiện an toàn, **không chạy skill/scripts cũ**. Quy ước worktree/branch cũ của skill không thay thế [GITHUB_WORKFLOW.md](../GITHUB_WORKFLOW.md) hiện hành.

## 3. Phạm vi POC và ngoài phạm vi

**Trong phạm vi:** một máy Windows / agent Forge, một CoS Project đã được user phê duyệt, một ChatGPT Project đích `opendash-cms`, một chat mới cho thử nghiệm `TEST-PONG-01`; cơ chế nhận request, ack, tra cứu trạng thái, session receipt; phân biệt online/offline; thử retry/idempotency và kiểm soát truy cập. Khi các phần trên PASS mới kết nối một GitHub Issue thực sự đủ gate.

**Ngoài phạm vi POC đầu:** Mac/Harbor, Antigravity, nhiều agent đồng thời, worker/helper, quyền đọc mọi lịch sử chat, điều khiển shell tổng quát, giao source code thật khi chưa qua G3, tự phê duyệt thiết kế, auto-merge PR, deployment, thực thi tác vụ do Muse suy đoán và thay đổi CoS source code.

## 4. Các phương án và lựa chọn đề xuất

| Phương án | Điểm mạnh | Điểm yếu | Đánh giá |
| --- | --- | --- | --- |
| Muse → Cloudflare → **Local Control API trực tiếp** | Ít thành phần | Phơi ra API có thể đọc chat và gửi tin toàn quyền | **Loại bỏ** |
| Muse → Cloudflare Access → **relay hẹp trên Windows** → CoS loopback | Có scope/auth/audit/deny-by-default, theo dõi idempotency | Phải viết và kiểm thử relay | **Ưu tiên POC** |
| SSH + dispatch-plan-tasks retired | Có ghi chép vận chuyển cũ | Không đúng điều phối cloud, không theo dõi lâu dài, skill retired | Không tái kích hoạt |
| Browser GUI automation | Không cần Local API | Dễ gãy khi UI cập nhật, khó nhận receipt chắc chắn | Chỉ phương án dự phòng sau khi local API không phù hợp |

**Đề xuất:** xây **Muse Dispatch Relay** độc lập với mã Rust CMS. Relay là process chỉ bind `127.0.0.1`, gọi CoS Local API loopback, không expose raw `/v1/*` hoặc lịch sử chat ra ngoài. Đường network công khai chỉ vào Cloudflare Tunnel → Cloudflare Access → relay. Chưa chọn ngôn ngữ/khung web cho relay ở bước tài liệu; ưu tiên ít dependency, test được trên Windows.

## 5. Sơ đồ và ranh giới tin cậy

```text
Muse (cloud; GitHub Issue/PR tools)
  │  [A] transport capability phải được chứng minh; Service Auth
  ▼
Cloudflare Access (service token + policy theo app)
  ▼  cos-win.woocommerce.vn  [named tunnel; không public CoS port]
cloudflared trên Windows
  ▼  localhost ONLY
Muse Dispatch Relay  [allowlist / auth / rate limit / durable ledger]
  ▼  127.0.0.1 + token CoS chỉ đọc tại Windows
CoS Local Control API
  ▼  POST /v1/inputs {id, projectId, text} (không sessionId)
CoS direct main chat ↔ ChatGPT Project `opendash-cms` (chưa verified)
  ▼  GET /v1/inputs, sessions/{id}/events (redacted)
Relay job status → Muse → GitHub Issue/Project (sau gate)
```

**Không nhầm Project:** `projectId` của API có thể là CoS Project; nó **không tự chứng minh** chat thuộc ChatGPT Project tương ứng. Phải xác nhận đồng thời: (1) CoS Project đúng, root/worktree được phép; (2) tab/main chat nằm trong ChatGPT Project `opendash-cms`; (3) không phải worker/helper; (4) khởi chạy mới không làm xáo trộn chat đang mở. Thiếu một tiêu chí => **BLOCKED**, không âm thầm gửi vào chat/project khác.

**Khả năng Muse gọi mạng:** phải khám phá Muse có sẵn authenticated HTTP call hay remote MCP tool thực sự, cấp quyền riêng cho relay, và nhận được receipt. Nếu Muse chỉ có GitHub connector, cần chọn **một** connector/MCP hẹp tương đương; không giả định cloud agent có thể gọi mọi URL hoặc dùng custom HTTP tự do.

## 6. Giao thức relay đề xuất (draft, chưa phải API đã cài)

| Endpoint relay | Phương thức | Giới hạn |
| --- | --- | --- |
| `/health` | GET | Chỉ báo `online/offline/limited`, phiên bản và lý do từ chối không nhạy cảm; không tiết lộ token, PID, danh sách chat |
| `/dispatch` | POST | Chỉ chấp nhận 1 job hợp lệ, đích `forge/windows`, project allowlist, request ID ổn định; ở POC đầu chỉ cho phép `TEST-PONG-01` |
| `/jobs/{job_id}` | GET | Receipt/trạng thái, delivery, phản hồi đã rút gọn có kiểm soát (với PONG chỉ `PONG`), session reference nội bộ nếu được phép; **không** trả raw chat log hoặc toàn bộ lịch sử |

Không có `proxy`, `exec`, `browse`, đọc file tùy ý, `stop`, `workers/*`, liệt kê toàn bộ sessions hoặc URL bất kỳ trong API public. `GET /health` vẫn cần Cloudflare Access cho remote. Có thể có xác thực thứ hai trong relay để giảm thiểu sai cấu hình Access; phải thiết kế cơ chế lưu/rotate khóa trước khi bật remote.

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

Relay sở hữu việc tạo text cố định cho `poc-pong`; **không** nhận command hoặc prompt tùy ý từ Internet trong phép thử đầu. Với task thật (sau khi duyệt mở scope), chỉ dùng nội dung Issue đã xác minh ở commit nguồn, có `issue_number`, `task_id`, `source_sha`, `base_sha`, gate, assignee, file ownership, exclusions và expected checks. Relay không tự cấp G3 hoặc quyền file.

**Receipt nháp** cho `POST /dispatch`: `202 {"job_id":"<opaque>","state":"QUEUED","delivery":"pending"}` khi CoS xác nhận đã ghi vào outbox; khi mới tiếp nhận local hoặc chưa gửi được CoS, không được gán `QUEUED`. `GET /jobs/{job_id}` trả `state`, `delivery`, `last_updated_at`, `error_code` đã làm sạch và `result` rút gọn (`PONG` trong phép thử). Không chuyển nguyên văn nội dung sự kiện SSE, tool calls, bearer, file content hay chat history về Muse. Cần chốt schema chi tiết sau P0 và kiểm thử mọi trạng thái biên.

### Idempotency và retry

1. Lưu trạng thái `request_id` vào **durable local ledger trước khi gọi CoS**; kho này là receipt giao nhận, không lưu planning state và không commit lên Git.
2. Một `request_id` luôn gắn với một payload hash, một CoS input UUID, một target/project; trùng ID + cùng payload trả **receipt cũ**, trùng ID + payload khác trả `409 conflict`.
3. Gọi CoS `POST /v1/inputs` **một lần** với input UUID đã lưu. HTTP `202` = queued/stored, không phải delivered/completed. Kiểm tra `GET /v1/inputs` cho delivery (`pending`, `sent`, `not_sent`, `unconfirmed`) và session events theo `seq`.
4. Timeout/disconnect => `UNKNOWN`; phải đối chiếu outbox/session trước khi retry. CoS outbox giữ **tối đa 50 finished inputs** (có thể ít hơn); nếu không thể phân giải, chuyển manual reconciliation, **không tạo UUID mới rồi gửi lần nữa**.
5. Ghi cursor cuối đã xử lý để resume sau restart; event/response có thể đến nhiều lần. `turn_end` chỉ xác nhận lượt kết thúc, không tự suy task code hoàn thành. Giới hạn **một job active trên Forge** trong POC, job mới trả `busy` thay vì chen vào chat khác.

**Trạng thái relay (không thay thế Project Status):** `RECEIVED → VALIDATED → QUEUED → DELIVERED → RESPONDED` hoặc `BLOCKED` / `UNKNOWN`. `RESPONDED` được gán chỉ khi có message/turn result quan sát được; `202` chỉ tới `QUEUED`. Khi dùng task code, GitHub `In review` cần PR/verification riêng và `Done` chỉ sau G5 thực chạy.

## 7. Bảo mật, quyền và dữ liệu

- **Deny-by-default:** cloud không được truy cập trực tiếp CoS port, token, events, logs, file system hay MCP rộng. Relay giữ allowlist host, project, agent, mode, kích thước request; validate JSON/UUID/length và rate limit.
- **Secrets:** CoS token mới mỗi lần app khởi động, chỉ được đọc cục bộ từ CoS user-data; không in ra console, đưa lên GitHub Issue/PR hay đưa vào prompt Muse. Cloudflare service token trong secret store có quyền hạn tối thiểu; có kế hoạch rotate/revoke.
- **Access:** dùng Cloudflare Access `Service Auth` và named tunnel. Không coi DNS hoặc khả năng gọi `/health` là bằng chứng caller đã được phép `/dispatch`.
- **Audit:** chỉ ghi request ID hash, state transition, timestamp, target, kết quả auth và mã lỗi đã làm sạch; loại bỏ raw prompt, full chat history, secret, headers auth và private messages. Giới hạn thời gian lưu local, quyền file theo user; định nghĩa retention trong G2.
- **Least privilege CoS:** ở phép thử PONG không cấp quyền viết file/terminal cho chat mới; kiểm tra quyền app/Project trước khi gửi. Lời dặn "không dùng tools" trong prompt **không thay thế** permission enforcement.
- **Giới hạn provider:** không tự tiếp tục khi ChatGPT từ chối/giới hạn sử dụng; không dùng tunnel/worker/chat mới để vượt hạn chế. Khi không đủ quyền hoặc network bị chặn => ghi `BLOCKED`.

## 8. Lifecycle đúng yêu cầu người dùng

**Đích:** khi CoS **đang mở và Local API hợp lệ**, relay + tunnel có thể phục vụ; khi CoS **đóng**, kết nối chuyển offline và không còn bề mặt dispatch công khai hữu dụng; domain/DNS giữ nguyên. Không yêu cầu người dùng khởi động relay/tunnel thủ công mỗi lần mở CoS sau khi cơ chế lifecycle đã được chốt.

**Chưa chốt cơ chế:** launcher bao CoS hoặc watcher process/health (có giám sát PID, state và timeout), lifecycle Windows startup hay theo app, hành vi crash/restart, recovery token, nhiều tiến trình Electron, reboot và sleep/wake. POC phải so sánh hai cách bằng test thật. Nếu CoS mất Local API hoặc `Allow actions` tắt, **fail closed**: không nhận dispatch, không tái dùng token cũ, không tự bật công tắc quyền. Không coi việc tắt DNS là cần thiết; chỉ dừng đường truyền/relay đúng nghĩa.

## 9. Các cổng triển khai và điều kiện chốt

| Gate | Phạm vi | Điều kiện để sang gate tiếp theo |
| --- | --- | --- |
| **P0 — contract/khả thi** | Xác nhận version CoS, Local API/Allow actions, CoS Project + ChatGPT Project mapping, khả năng Muse gọi authenticated endpoint | Có evidence và quyết định phương án; không có blocker về quyền/API |
| **P1 — loopback** | Test 1 `TEST-PONG-01` qua CoS loopback, không remote | Chat chính mới đúng project, không worker; outbox delivery + event response verified |
| **P2 — relay nội bộ** | Thử auth, allowlist, retry, restart, redaction với mock + CoS thật | Unit/integration + negative cases PASS, không phát lộ raw CoS API |
| **P3 — Cloudflare/lifecycle** | Named tunnel + Access, host Windows, stop/start/crash/reconnect | Remote chỉ được phép qua Access; không dispatch khi CoS offline; no leakage |
| **P4 — Muse round-trip** | Muse gọi relay PONG, theo dõi receipt, nhận phản hồi | Một job duy nhất, có thể retry/resume idempotent; không chỉnh code |
| **P5 — GitHub end-to-end** | Issue đủ G3 READY → Muse → Forge → PR → review → G5 | Truy vết mọi ID/SHA, không tự sửa contract/merge/Done; test thật được duyệt riêng |

**Bàn giao từng gate:** file evidence trong `docs/verification/` gồm timestamp, môi trường, SHA CoS/relay, test steps, expected/actual, PASS/FAIL/NOT_RUN, log đã redacted, rủi ro còn lại; một PR qua CI/review trước khi xem gate VERIFIED. Không khai P5 READY chỉ vì PONG thành công.

## 10. Câu hỏi cần chốt trước khi viết code

1. `projectId` là gì trên chính CoS Windows? Folder/root nào của Forge được phép; liên kết ChatGPT Project `opendash-cms` có hoạt động với chat tạo qua Local API không?
2. Muse có thể gọi HTTP endpoint hoặc remote MCP **với Cloudflare Service Token** bằng công cụ thực nào? Nếu không có, chọn connector hẹp nào mà không cấp shell quyền rộng?
3. Relay nằm ở thư mục hạ tầng nào (tách khỏi CMS runtime), runtime và nơi lưu idempotency ledger/secret trên Windows? Không đưa runtime data vào Git.
4. Cơ chế bật/tắt theo CoS: launcher hay watcher? Cách xử lý crash/reboot/sleep, App update, rotation và độ trễ offline được nghiệm thu ra sao?
5. Cấp độ quyền của phép thử PONG và task thật: CoS Read-only, file approvals, chủ thể được phép approve, quy tắc pause/kill switch?
6. Giữ nguyên branch cố định `agent/forge` và một task active theo [quy trình hiện hành](../GITHUB_WORKFLOW.md); nếu cần task worktree riêng, phải duyệt thay đổi quy ước trước.

## 11. Decision / promotion reference (chỉ điền sau khi được phê duyệt)

**Chưa có quyết định G2.** Đề xuất này và [test matrix](muse-cos-win-poc-test-matrix.md) chỉ là nghiên cứu để người quản lý và các agent phản biện. Sau khi chốt kiến trúc, tổng hợp các kết luận đã duyệt vào `docs/plans/` hoặc ADR/contract tương ứng, rồi mới lập task triển khai. Không tạo Issue implementation G3 READY từ tài liệu PROPOSED này.
