# Test matrix (PROPOSED): Muse → CoS Windows → GitHub

- **Agent / ngày:** Forge, 2026-11-09
- **Trạng thái:** **PROPOSED / toàn bộ test chưa chạy**
- **Source SHA:** `8833fc0f9f13903373ace8cdb4e67ee29b8cfb16`
- **Thiết kế:** [Muse Orchestrator POC](muse-cos-win-orchestrator-poc.md)
- **Nguồn phản biện:** [Nova F-01–F-09 và Round 2 R2-01–R2-03 trên PR #4](https://github.com/thientran2359/opendash-cms/pull/4) (`READY_FOR_G2_APPROVAL` cho Phase 1, **chưa phải G2 được chủ dự án phê duyệt**); test expected vẫn `PROPOSED`, chưa chạy.
- **Quyết định chủ dự án (09/11/2026):** **Outbound GitHub Poller được chọn** cho POC; không triển khai Inbound Relay/Cloudflare nếu Poller chứng minh khả thi. ChatGPT Web Project mapping **không phải điều kiện PASS**; yêu cầu đúng CoS Project/worktree và khả năng dispatch Forge. **Chưa có P0–P5 runtime PASS.**

## Nguyên tắc nghiệm thu

Không chuyển test thành PASS nếu chỉ có `HTTP 202`, GitHub label, một file script, lời tự khẳng định của agent hoặc PR merge. Mỗi case cần **expected vs actual** và bằng chứng thực chạy. Chỉ ghi `PASS`, `FAIL`, `BLOCKED` hoặc `NOT_RUN`. **Phase 1 = P0–P4 (Outbound GitHub Poller, chỉ `TEST-PONG-01`, không đụng CMS code); Phase 2 = P5 (G2/G3 riêng sau Phase 1 VERIFIED)**. Không tạo GitHub Issue implementation giả để thử; không thử Inbound Relay/Cloudflare hoặc coi ChatGPT Web Project là gate.

### Preflight: xác minh trước khi tương tác

- [ ] Đối chiếu CoS version đã cài và docs Local Control API tương ứng.
- [ ] Kiểm tra Local API enabled và `Allow actions` do chủ máy cho phép; không tự bật/đổi quyền ngầm.
- [ ] Xác minh CoS Project ID thật + canonical root/worktree Forge qua CoS UI/config local và API metadata **nếu có**, ghi cơ chế xác minh tại CoS v2.1.50, pin `projectId` UUID + path vào allowlist local trước P1; không giả định API có project-list route.
- [ ] Đối chiếu **hai file riêng** trong user-data thực tế của bản CoS cài đặt (ứng viên `%APPDATA%/chat-on-steroids/control-api/endpoint.json` và `control-api/token`) cùng ACL, port/PID, thay đổi mtime/content sau restart; không log giá trị token hoặc ID private.
- [ ] **Không yêu cầu** chat thuộc ChatGPT Web Project `opendash-cms`; không log ID private công khai.
- [ ] Xác minh hành vi main chat, không worker/helper; không làm thay đổi chat/cửa sổ đang làm việc ngoài chat mới của POC.
- [ ] Xác minh Muse có quyền GitHub connector đủ để ghi **tín hiệu task/receipt được phép**; Windows có GitHub API token tối thiểu để poll/ack, không giả định Muse có arbitrary HTTP.
- [ ] Ghi quyết định **GitHub Poller** và **CoS workspace isolation**; chốt Issue phối hợp, signal là GitHub comment/event có ID riêng, allowlist repo/actor, quyền kiểm `created_at/updated_at` hoặc timeline actor, request ID/claim và ETag/backoff trước P4.
- [ ] Không để GitHub token/CoS token trong LLM prompt, Issue body/comment, tool arguments hoặc evidence; đọc secrets từ kho cục bộ ACL.
- [ ] Xác định vị trí lưu receipt/secret local với ACL; xác định kill switch và thời điểm offline.

## P0 — Discovery và contract

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P0-01 | Xác minh user-data `control-api/endpoint.json` (port/PID/version) **và** file `control-api/token` riêng, ACL + mtime/content/rotation; sau đó đọc `GET /v1/health` và khám phá event API trên CoS thực tế | Cặp port+bearer được nạp đồng bộ, 200 đúng version/routes, không log bearer; lưu **event kinds, session/seq, dấu hiệu assistant reply + `turn_end`** đã redacted; file stale/sai quyền => BLOCKED |
| P0-02 | Local API disabled hoặc `Allow actions` disabled | Không tạo được chat; 403/không kết nối được theo build; không tự nâng quyền |
| P0-03 | Tìm ánh xạ `projectId` UUID ↔ canonical Forge worktree qua CoS UI/config local hoặc route/metadata API **chỉ nếu build hỗ trợ**; pin vào allowlist và kiểm session/main chat | Có bằng chứng đường lấy mapping thật trên CoS v2.1.50, UUID + path được pin, chỉ tạo chat chính ở đúng workspace; sai mapping => BLOCKED; **không BLOCKED vì thiếu ChatGPT Web Project mapping** |
| P0-04 | Xác minh Muse GitHub connector ghi/đọc tín hiệu, Windows Poller đọc event/comment **theo ID** và actor, claim/ack bằng HTTPS outbound | Có evidence quyền GitHub, event ID, `actor.login`/`user.login`, `author_association` theo policy nếu là comment, `created_at == updated_at` (comment); đúng allowlist thì nhận, không đúng BLOCKED; không fallback inbound |
| P0-05 | Phê duyệt `TEST-PONG-01` trên Issue phối hợp allowlist; xác minh GitHub event/comment ID và UUID `request_id` gắn/dẫn xuất tất định, ETag support | Schema/nguồn actor, allowed Issue, ID và rule request ID đã ghi evidence trước khi phát; test `If-None-Match`/`304` hoặc giới hạn polling fallback, không tạo Issue G3 giả |

## P1 — Local Control API / một chat chính

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P1-01 | `POST /v1/inputs` với UUID mới, CoS `projectId` **đã pin vào allowlist và đối chiếu path P0-03**, `text` chỉ PONG, **không có `sessionId`** | 202 queued và chỉ một main chat mới trong đúng CoS Project/worktree; không worker/helper, không yêu cầu ChatGPT Web Project |
| P1-02 | Kiểm tra `GET /v1/inputs` sau P1-01 | Quan sát delivery rõ `sent/pending/unconfirmed/not_sent`, không gán completed từ 202 |
| P1-03 | Theo dõi `GET /v1/sessions/{input_uuid}/events` hoặc SSE theo đúng `event kind`/`seq` đã đo tại P0-01 | Có **event assistant response mang PONG** và **`turn_end` tương ứng**, cùng session ID bằng input UUID; không nhầm ack/inbound message với answer hoặc dùng tên event phỏng đoán |
| P1-04 | Sai `projectId` | 404 `project_not_found`; không fallback vào chat chung |
| P1-05 | Gửi lại cùng UUID trong cửa sổ outbox còn lưu | Không có chat/tin nhắn thứ hai |
| P1-06 | `projectId` cùng `sessionId` hoặc model override sai | Request bị từ chối, không đổi model/chat đang mở |
| P1-07 | `Allow actions` bị thu hồi giữa quá trình | Ngừng nhận việc mới; thể hiện BLOCKED/UNKNOWN chính xác |

## P2 — GitHub Poller + local dispatcher an toàn (ưu tiên mock trước live)

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P2-01 | Kiểm từ Internet/process local xem Poller có `/dispatch` HTTP hoặc mở inbound listener không | **0 inbound dispatch endpoints**, 0 CoS send từ arbitrary localhost process/HTTP; chỉ Poller đã xác thực mới gọi CoS loopback |
| P2-02 | GitHub signal sai repo/Issue/agent/actor/mode; label/body snapshot không có event actor, comment edited (`updated_at != created_at`), hoặc payload thay đổi sau validate; chứa prompt/shell tự do | `BLOCKED/REJECTED_SIGNAL` trong local ledger, **0 CoS send**; kiểm event ID/actor và re-fetch + so payload hash trước dispatch, không biến GitHub text thành lệnh |
| P2-03 | Cùng `github_signal_id` + `request_id` + payload sau retry/restart | Dùng receipt/input UUID cũ, chỉ một send; `(repo_id, github_signal_id)` và `(repo_id, request_id)` được unique riêng, không chỉ composite pair |
| P2-04 | Cùng `github_signal_id` hoặc `request_id` với payload hash khác, hoặc signal khác reuse request ID | Ghi `CONFLICT_PAYLOAD_MISMATCH` / `BLOCKED` vào ledger/audit và publish **mã lỗi redacted** qua GitHub receipt, **0 CoS send**; không HTTP 409 và không ghi đè job cũ |
| P2-05 | Kill Poller trước/sau commit trạng thái `DISPATCHING_UNKNOWN`, khi POST tới CoS đã ra socket nhưng chưa nhận 202; restart | Kill **trước checkpoint**: chưa có POST, có thể giữ `VALIDATED`; kill **sau checkpoint**: `UNKNOWN` dù có/không có 202, **không replay tự động**; UUID bền vững |
| P2-06 | CoS timeout/429/503, outbox đã cắt mất dữ liệu (giới hạn 50) | Reconcile `GET /v1/inputs` theo UUID, tiếp `GET /v1/sessions/{input_uuid}/events`; thiếu chứng cứ vẫn `UNKNOWN`/BLOCKED, không tạo UUID mới |
| P2-07 | Hai task active cùng agent; yêu cầu quá dày | Chỉ một active, trả busy/rate limit, không chen ngang tin user |
| P2-08 | Kiểm tra GitHub receipt/status, errors, temp/ledger files và log Poller | Không có CoS/GitHub bearer, raw chat, token, private data trong output; không có CF secret cần quản lý |
| P2-09 | Operator dùng local `poller-ctl inspect/resolve` với job UNKNOWN; có/không có evidence, sai quyền | Không có HTTP API mở lock; ghi audit, `delivered` cần evidence; `aborted-unconfirmed` mở slot nhưng **không replay cùng UUID/job**; sai quyền bị từ chối |
| P2-10 | Poller đọc tín hiệu GitHub allowlist, gặp Issue/comment/task ID lạ, thiếu GitHub credential hoặc CoS offline | Chỉ job đã duyệt mới tới local dispatcher; không ingest mutable Issue body làm prompt, không có inbound HTTP, không rò GitHub token |
| P2-11 | Poller restart hoặc GitHub API trả `429`; trùng signal/request ID, lost ack sau dispatch | Backoff theo GitHub limits, cursor/ledger phục hồi; ack/status publish idempotent, không tạo chat mới lần hai |
| P2-12 | Poll lặp URL GitHub API với `ETag` và `If-None-Match`; không thay đổi/đổi phiên bản, API không cấp ETag | `304` không parse/dispatch lại, không tốn primary GitHub rate limit khi request được xác thực theo GitHub API; giữ backoff/secondary limits và fallback interval an toàn, không dùng ETag thay actor/hash/ledger |
| P2-13 | GitHub comment/event ID bị replay, comment bị edit trước/sau read, actor khác chủ repo tự gắn label, hoặc actor là bot chưa được ủy quyền | Chỉ event ID + actor allowlist + policy association + snapshot/hash không thay đổi mới đủ điều kiện; thay đổi sau recheck hoặc edited comment => `BLOCKED`, **0 CoS send mới** |

## P3 — GitHub Poller outbound và lifecycle cùng CoS

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P3-01 | Windows Poller gọi GitHub HTTPS outbound, không dùng Cloudflare tunnel/relay | Poller được GitHub auth và đọc đúng nguồn, không mở inbound HTTP port; `0` CoS send nếu signal chưa duyệt |
| P3-02 | GitHub credential hợp lệ/sai/expired/revoked, actor không thuộc allowlist | Chỉ credential/actor được phép thao tác tín hiệu; mất quyền => `BLOCKED`, không dispatch |
| P3-03 | Poller đọc tín hiệu test hợp lệ khi CoS Local API healthy, `Allow actions` được chủ máy bật | PONG tới đúng Forge CoS workspace, một session, có local receipt; không chạy Inbound Relay |
| P3-04 | Đóng CoS, đo từ lúc đóng đến khi Poller chặn dispatch mới | **≤5 giây** fail-closed, `0` CoS send mới; Poller ngừng dispatch, không bắt buộc dừng mọi hoạt động ledger/status; ghi thời gian thực |
| P3-05 | Bật CoS trở lại, `endpoint.json` **và** file `token` thay đổi port/bearer hoặc chỉ một file đang được viết | Supervisor theo dõi mtime/content cả hai file, nạp lại cặp đồng bộ, xác minh PID/start-time/health/Allow actions + `projectId` allowlist; mismatch/đang rotate => fail-closed, không dùng port/token cũ |
| P3-06 | Crash CoS, supervisor crash, auto-update/relaunch, restart Windows, sleep/wake | Supervisor xác minh PID + creation time + health + actions; fail-closed, phục hồi có giới hạn, không resend UNKNOWN |
| P3-07 | GitHub API mất mạng, 403/429 hoặc revoke quyền trong lúc chạy | Không phát sinh send mới, giữ receipt local; backoff và phân biệt OFFLINE/BLOCKED/UNKNOWN |
| P3-08 | CoS đóng, Poller/supervisor còn chạy hoặc restart, không có tunnel | Không có `cloudflared` hay public relay nào được tạo; fail-closed, giữ ledger và resume an toàn khi CoS mở lại |
| P3-09 | Đóng CoS ngay khi PONG đang `DISPATCHING_UNKNOWN` / in-flight | ≤5 giây từ chối dispatch mới, job cũ ghi `UNKNOWN` và reconcile; không báo completed giả hay gửi lại tự động |
| P3-10 | Poller không thể publish kết quả vì GitHub tạm lỗi sau CoS PONG | Giữ local receipt, retry **ack GitHub** idempotent; không gửi lại PONG vào CoS hoặc tự fallback inbound |

## P4 — Muse round-trip qua GitHub Poller

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P4-01 | Muse kích hoạt `TEST-PONG-01` bằng **GitHub signal đã được phép**, Windows Poller đọc/validate/dispatch | Đúng một input UUID/main chat của Forge CoS Project; có request ID và local receipt |
| P4-02 | Poller publish receipt/status/PONG đã redacted lên GitHub; Muse đọc cùng ID trên GitHub | Muse thấy **PONG thực sự** từ assistant event + `turn_end`, không nhầm CoS 202/GitHub ack là xong; không đọc raw chat |
| P4-03 | Muse retry cùng ID sau timeout hoặc network break | Không tạo thêm main chat, kết quả có idempotency ổn định |
| P4-04 | Muse/GitHub/Windows không có quyền tạo/đọc/ack signal | `BLOCKED` kèm quyền/capability thiếu; không tự chuyển Inbound Relay hoặc cấp G2 |

## P5 — GitHub integration (Phase 2, sau khi P0–P4 VERIFIED và G2 riêng)

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P5-01 | Issue G1/G2 hoặc task G3 không READY | Không dispatch code, không đổi Project Ready/In progress giả |
| P5-02 | Issue thật đã duyệt, có source SHA, base SHA, file owner và dependency | Muse kiểm tra Issue/Project/PR; **Windows** xác minh immutable Git blob tại `source_sha`, G3 READY, `source_path`, branch/HEAD/base SHA, `git status --porcelain` sạch; ghi receipt Issue↔Agent↔Session |
| P5-03 | Trùng Issue ID, đã có PR hoặc branch busy | Cloud kiểm trạng thái Issue/PR; local dispatcher kiểm single-job lock + remote divergence, chặn giao trùng trước enqueue |
| P5-04 | Chat gửi xong nhưng agent chưa commit/PR | Không chuyển `In review` hoặc `Done` |
| P5-05 | Agent tạo PR, review/CI, merge, verification | Chỉ đổi status theo bằng chứng, không auto-merge, `Done` chỉ sau G5 được duyệt |
| P5-06 | Windows Forge worktree dirty (gồm untracked), sai `agent/forge`, sai HEAD/`base_sha`, remote diverged hoặc PR đang chiếm branch | Local ledger `WORKTREE_DIRTY_OR_MISMATCH` / `BLOCKED`, **0 CoS send**; không có HTTP 409 do Poller không mở relay |
| P5-07 | GitHub Issue body/comment chứa prompt injection hoặc bị sửa sau khi pin `source_sha`; signal chứa `prompt`/`text` | Reject free text, không chèn Issue body vào CoS; chỉ bootstrap prompt cố định dẫn đến Git blob được phép và G3 READY; thiếu nguồn được duyệt => BLOCKED |

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
Owner decision: GitHub Poller + CoS Project isolation (đã chốt); quyền/signal cụ thể và G2 còn lại:
```

**Tổng trạng thái lúc tạo tài liệu: `NOT_RUN` cho toàn bộ test.** Các lệnh kiểm tra Markdown/CI của repository (nếu chạy) chỉ kiểm tra **chất lượng tài liệu**, không được đánh dấu thành POC runtime PASS.
