# Test matrix (PROPOSED): Muse → CoS Windows → GitHub

- **Agent / ngày:** Forge, 2026-11-09
- **Trạng thái:** **PROPOSED / toàn bộ test chưa chạy**
- **Source SHA:** `8833fc0f9f13903373ace8cdb4e67ee29b8cfb16`
- **Thiết kế:** [Muse Orchestrator POC](muse-cos-win-orchestrator-poc.md)
- **Nguồn phản biện:** [Nova F-01–F-09 trên PR #4](https://github.com/thientran2359/opendash-cms/pull/4); thay đổi expected bên dưới vẫn `PROPOSED`, chưa chạy.

## Nguyên tắc nghiệm thu

Không chuyển trạng thái test sang PASS nếu chỉ nhìn thấy `HTTP 202`, URL DNS, một file script, một câu trả lời agent tự khẳng định hoặc PR đã merge. Mỗi case có evidence thực chạy và **expected vs actual**. Chỉ ghi `PASS`, `FAIL`, `BLOCKED` hoặc `NOT_RUN` theo quan sát. **Phase 1 = P0–P4 (chỉ `TEST-PONG-01`, không đụng CMS code), Phase 2 = P5 (G2 và G3 riêng sau Phase 1 VERIFIED).** Không tạo GitHub Issue implementation giả để thử. P0-04 quyết định chọn **inbound relay** hoặc **outbound GitHub poller** sau khi chủ dự án duyệt; case đặc thù transport chưa được chọn ghi `NOT_RUN` hoặc `BLOCKED`, không coi là PASS.

### Preflight: xác minh trước khi tương tác

- [ ] Đối chiếu CoS version đã cài và docs Local Control API tương ứng.
- [ ] Kiểm tra Local API enabled và `Allow actions` do chủ máy cho phép; không tự bật/đổi quyền ngầm.
- [ ] Xác minh CoS Project ID thật + root/worktree, và ChatGPT Project `opendash-cms` đích; không log ID nhạy cảm ở nơi công khai.
- [ ] Xác minh hành vi main chat, không worker/helper; không làm thay đổi chat/cửa sổ đang làm việc ngoài chat mới của POC.
- [ ] Xác minh tính năng mạng Muse thực sự có và cách giữ service token; không giả định connector có quyền arbitrary HTTP.
- [ ] Ghi quyết định P0 về transport inbound/outbound, cách tạo tín hiệu PONG được phép, chính sách mapping ChatGPT Project và người duyệt; không chuyển nhánh tự động.
- [ ] Mọi request từ cloud phải không chứa secret trong LLM prompt/tool arguments; xác minh quyền kết nối ở server/host, không chép token vào evidence.
- [ ] Xác định vị trí lưu receipt/secret local với ACL; xác định kill switch và thời điểm offline.

## P0 — Discovery và contract

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P0-01 | Đọc `GET /v1/health` qua loopback với token local hợp lệ; khám phá `GET /v1/sessions/{id}/events` trên phiên bản CoS thực tế | 200, có route/version đúng; ghi **tên event kinds, trường session/seq, dấu hiệu assistant reply + `turn_end`** vào evidence redacted; không in bearer/raw chat |
| P0-02 | Local API disabled hoặc `Allow actions` disabled | Không tạo được chat; 403/không kết nối được theo build; không tự nâng quyền |
| P0-03 | So sánh CoS Project metadata/root, session endpoint, browser URL/DOM main chat với ChatGPT Project | Có bằng chứng **riêng biệt** cho CoS Project và ChatGPT Project, hoặc **BLOCKED chờ chủ dự án chọn** chỉ CoS workspace isolation hay bắt buộc cả hai |
| P0-04 | Kiểm tra Muse có connector gọi inbound HTTP với headers được inject ở **server-side vault**, không qua LLM; kiểm GitHub connector và outbound poller feasibility | Nếu auth an toàn thực sự PASS thì inbound là ứng viên; nếu không, inbound **BLOCKED** và lập so sánh poller/remote MCP cho chủ dự án chọn; không tự bật tunnel |
| P0-05 | Chọn transport và nguồn tín hiệu PONG cụ thể (inbound request / GitHub allowlisted process signal) | Có quyết định của chủ dự án và phạm vi P0–P4; không tạo Issue G3 giả hay dùng Issue body/comment làm prompt |

## P1 — Local Control API / một chat chính

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P1-01 | `POST /v1/inputs` với UUID mới, `projectId` approved, `text` chỉ PONG, **không có `sessionId`** | 202 queued và chỉ một main chat mới; không worker/helper, đúng project |
| P1-02 | Kiểm tra `GET /v1/inputs` sau P1-01 | Quan sát delivery rõ `sent/pending/unconfirmed/not_sent`, không gán completed từ 202 |
| P1-03 | Theo dõi `GET /v1/sessions/{input_uuid}/events` hoặc SSE theo đúng `event kind`/`seq` đã đo tại P0-01 | Có **event assistant response mang PONG** và **`turn_end` tương ứng**, cùng session ID bằng input UUID; không nhầm ack/inbound message với answer hoặc dùng tên event phỏng đoán |
| P1-04 | Sai `projectId` | 404 `project_not_found`; không fallback vào chat chung |
| P1-05 | Gửi lại cùng UUID trong cửa sổ outbox còn lưu | Không có chat/tin nhắn thứ hai |
| P1-06 | `projectId` cùng `sessionId` hoặc model override sai | Request bị từ chối, không đổi model/chat đang mở |
| P1-07 | `Allow actions` bị thu hồi giữa quá trình | Ngừng nhận việc mới; thể hiện BLOCKED/UNKNOWN chính xác |

## P2 — Local dispatcher an toàn (inbound relay hoặc outbound poller; ưu tiên mock trước live)

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P2-01 | **Inbound:** process khác trên chính Windows gọi `127.0.0.1/dispatch` thiếu/sai Bearer Relay-Secret; remote gọi thiếu/sai Cloudflare Access JWT/AUD; body vượt giới hạn | 401/403/400/413 theo contract, **0 CoS send**; cả local và remote phải vượt qua hai lớp phù hợp, không tin header do localhost tự giả |
| P2-02 | Request tới project/agent/mode không allowlist, có prompt/shell tự do | 400/403, không forward sang CoS |
| P2-03 | Cùng request ID + cùng payload sau retry | Trả cùng receipt và session ID, chỉ một send |
| P2-04 | Cùng request ID + payload khác | 409, không ghi đè job cũ |
| P2-05 | Kill process trước/sau commit trạng thái `DISPATCHING_UNKNOWN`, khi POST đã ra socket nhưng chưa nhận 202; restart | Kill **trước checkpoint**: chưa có POST, có thể giữ `VALIDATED`; kill **sau checkpoint**: `UNKNOWN` dù có/không có 202, **không replay tự động**; UUID bền vững |
| P2-06 | CoS timeout/429/503, outbox đã cắt mất dữ liệu (giới hạn 50) | Reconcile `GET /v1/inputs` theo UUID, tiếp `GET /v1/sessions/{input_uuid}/events`; thiếu chứng cứ vẫn `UNKNOWN`/BLOCKED, không tạo UUID mới |
| P2-07 | Hai task active cùng agent; yêu cầu quá dày | Chỉ một active, trả busy/rate limit, không chen ngang tin user |
| P2-08 | Kiểm tra log, HTTP errors, GET `/jobs`, temp/ledger files | Không có bearer, CF secret, raw lịch sử chat, dữ liệu private trong output |
| P2-09 | Operator dùng local `relay-ctl inspect/resolve` với job UNKNOWN; có/không có evidence, sai quyền | Không có HTTP API mở lock; ghi audit, `delivered` cần evidence; `aborted-unconfirmed` mở slot nhưng **không replay cùng UUID/job**; sai quyền bị từ chối |
| P2-10 | **Outbound:** GitHub poller đọc tín hiệu allowlist, từ chối Issue/comment/task ID lạ, không có GitHub credential hoặc CoS offline | Chỉ job đã duyệt mới tới dispatcher; không ingest mutable Issue body làm prompt, không có inbound HTTP, không rò GitHub token |

## P3 — Transport đã chốt và lifecycle cùng CoS

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P3-01 | **Inbound:** từ ngoài máy, request chưa có Cloudflare Service Auth hoặc chỉ có bearer nội bộ | Access từ chối, không tới relay; không có CoS send |
| P3-02 | **Inbound:** Service token hợp lệ / sai / revoked; Relay-Secret hợp lệ / sai; JWT sai AUD/signature/expiry | Chỉ caller qua Access **và** JWT hợp lệ **và** bearer riêng mới được truy cập |
| P3-03 | **Inbound:** named tunnel đang chạy; **outbound:** GitHub poller HTTPS hoạt động; CoS healthy/actions được cho phép | Chỉ kênh được chủ dự án chọn mới deliver PONG, có receipt; không bật hai transport cùng lúc |
| P3-04 | Đóng CoS hoàn toàn, đo từ thời điểm đóng đến khi health/dispatch từ chối | **≤5 giây** chuyển offline và `0` CoS send mới; inbound ngừng đường nhận/job (domain DNS có thể còn), outbound dừng poll; báo thời gian đo thực tế |
| P3-05 | Bật CoS trở lại, token local được rotate | Dispatcher khôi phục với token mới và đúng permissions; không dùng token cũ |
| P3-06 | Crash CoS, supervisor crash, auto-update/relaunch, restart Windows, sleep/wake | Supervisor xác minh PID + creation time + health + actions; fail-closed, phục hồi có giới hạn, không resend UNKNOWN |
| P3-07 | **Inbound:** Access auth bị revoke hoặc tunnel mất mạng | Không lộ CoS API; Muse có thể phân biệt OFFLINE/BLOCKED/UNKNOWN |
| P3-08 | **Inbound:** kill supervisor khi `cloudflared` là child trong Windows Job Object `KILL_ON_JOB_CLOSE` | Child `cloudflared` kết thúc, không có tunnel mồ côi; hồi phục khi app healthy không tạo tunnel trùng |
| P3-09 | Đóng CoS ngay khi PONG đang `DISPATCHING_UNKNOWN` / in-flight | ≤5 giây từ chối dispatch mới, job cũ ghi `UNKNOWN` và reconcile; không báo completed giả hay gửi lại tự động |
| P3-10 | **Outbound:** mất GitHub token/HTTPS hoặc GitHub trả 403/429 | Poller fail-closed, status được ghi rõ; không fallback sang inbound hoặc dùng Issue body làm lệnh |

## P4 — Muse round-trip qua transport được duyệt

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P4-01 | Muse kích hoạt `TEST-PONG-01` qua **connector inbound auth server-side** hoặc **GitHub approved signal** | Dispatcher nhận đúng một lần, receipt có request ID, chat mới đúng Project theo quyết định P0 |
| P4-02 | Muse đọc trạng thái qua `/jobs/{id}` (inbound) hoặc GitHub receipt/status được allowlist (outbound) | Không xem `202` là xong; hiển thị trạng thái theo evidence, phản hồi PONG (đã redacted) |
| P4-03 | Muse retry cùng ID sau timeout hoặc network break | Không tạo thêm main chat, kết quả có idempotency ổn định |
| P4-04 | Muse bị thiếu quyền/không có connector thật | Dừng tại BLOCKED và báo chính xác thiếu quyền/capability, không tự đổi transport hoặc cấp G2 |

## P5 — GitHub integration (Phase 2, sau khi P0–P4 VERIFIED và G2 riêng)

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P5-01 | Issue G1/G2 hoặc task G3 không READY | Không dispatch code, không đổi Project Ready/In progress giả |
| P5-02 | Issue thật đã duyệt, có source SHA, base SHA, file owner và dependency | Muse kiểm tra Issue/Project/PR; **Windows** xác minh immutable Git blob tại `source_sha`, G3 READY, `source_path`, branch/HEAD/base SHA, `git status --porcelain` sạch; ghi receipt Issue↔Agent↔Session |
| P5-03 | Trùng Issue ID, đã có PR hoặc branch busy | Cloud kiểm trạng thái Issue/PR; local dispatcher kiểm single-job lock + remote divergence, chặn giao trùng trước enqueue |
| P5-04 | Chat gửi xong nhưng agent chưa commit/PR | Không chuyển `In review` hoặc `Done` |
| P5-05 | Agent tạo PR, review/CI, merge, verification | Chỉ đổi status theo bằng chứng, không auto-merge, `Done` chỉ sau G5 được duyệt |
| P5-06 | Windows Forge worktree dirty (gồm untracked), sai `agent/forge`, sai HEAD/`base_sha`, remote diverged hoặc PR đang chiếm branch | `409 WORKTREE_DIRTY_OR_MISMATCH` / `BLOCKED`, **0 CoS send**; Muse không thể thay thế local check |
| P5-07 | GitHub Issue body/comment chứa prompt injection hoặc bị sửa sau khi pin `source_sha`; payload gửi `prompt`/`text` | Reject free text, không chèn Issue body vào CoS; chỉ bootstrap prompt cố định dẫn đến Git blob đã được phép và G3 READY; thiếu nguồn được duyệt => BLOCKED |

## Evidence form cho mỗi case

```text
Test ID:
Date/time (timezone):
Host / OS / CoS version / dispatcher build SHA / transport:
Git baseline SHA / Issue / PR (nếu có):
Điều kiện tiền đề, mức quyền:
Steps đã thực hiện:
Expected:
Actual (đã redacted):
Result: PASS / FAIL / BLOCKED / NOT_RUN
Evidence link hoặc hash (không chứa secret/raw chat):
Risk/next action:
Quan sát event kinds/seq hoặc độ trễ offline (nếu liên quan):
Reviewer/owner decision về transport + ChatGPT Project mapping (nếu liên quan):
```

**Tổng trạng thái lúc tạo tài liệu: `NOT_RUN` cho toàn bộ test.** Các lệnh kiểm tra Markdown/CI của repository (nếu chạy) chỉ kiểm tra **chất lượng tài liệu**, không được đánh dấu thành POC runtime PASS.
