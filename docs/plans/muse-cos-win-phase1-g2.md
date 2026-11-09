# Muse → CoS Windows — G2 Phase 1: Outbound GitHub Poller (P0–P4)

- **Quyết định chủ dự án:** **G2 Phase 1 APPROVED / design = AGREED**, 2026-11-09. Phạm vi phê duyệt **chỉ P0–P4** và phép thử `TEST-PONG-01`; không phê duyệt Phase 2/P5.
- **Track:** Full-Track cho phần hạ tầng điều phối (GitHub auth, local CoS Control API, secrets, durable receipt, failure recovery); **không thuộc code runtime CMS**.
- **Scope:** AGREED · **Research:** REVIEWED · **Design:** AGREED (chỉ Phase 1) · **Implementation:** NOT_STARTED · **Verification:** NOT_RUN.
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
