# Hợp đồng adapter Atlas → Chat On Steroids (one-shot, host-specific)

Adapter đã có tại [`.agents/scripts/cos_dispatch.py`](../../../scripts/cos_dispatch.py) và được gọi qua skill `RUN/STATUS`. Từng host phải qua `probe`, xác thực Project/worktree và kiểm thử PONG; `RUN` còn phải kiểm GitHub Issue/G3/Project Ready hoặc current task hợp lệ. Bằng chứng PONG **không đồng nghĩa** task sản phẩm đã hoàn tất. Trạng thái runtime, receipt ID và thông tin xác thực của host không được đưa vào Git.

## Hai đích độc lập

| Đích | Host | Điều phải xác minh tại host |
| --- | --- | --- |
| `forge` | Windows / CoS | Local Control API đã qua thử nghiệm PONG `sent`/`turn_end`/exact reply sau scoped W01 ACL hardening. Bằng chứng hậu kiểm tại [`docs/verification/2026-11-12-cos-dispatch-w01-pong.md`](../../../../docs/verification/2026-11-12-cos-dispatch-w01-pong.md); token rotation/restart còn chưa thực hiện |
| `harbor` | macOS / CoS | SSH → Mac loopback CoS API; 2 PONG test được xác nhận trong [adapter guide](../../../research/2026-11-12-cos-one-shot-adapter.md). CoS Project và branch Harbor được kiểm tra tại host |

Không sao chép cookie, token hoặc secret từ CoS chat/GitHub. Kiểm tra security blocker CoS Windows (W01), quyền đọc Project, quyền gọi API và trạng thái app trước mỗi thử nghiệm. **Không dùng watcher/Poller, Issue #5 PONG cũ hay archived dispatch scripts làm adapter mặc định.**

## Giao diện logic tối thiểu (transport cụ thể phải được chứng minh)

- `probe(target)` → định danh host, phiên đã xác thực, đúng project ID ↔ canonical worktree, capability tạo chat chính, phiên bản API; không lộ token.
- `deliver(target, dispatch_key, payload)` → receipt durable `ACCEPTED | REJECTED | UNKNOWN`, input/chat ID nằm **cục bộ** trong nơi lưu được bảo vệ. Lần giao đầu tiên được khóa bởi `(Issue, Task ID, target)`; `source_sha` và `base_sha` vẫn pin trong receipt. Đổi SHA **không cấp quyền tự gửi lần hai**; re-dispatch cần thế hệ được phê duyệt và cơ chế release riêng (chưa triển khai). Không dùng chuỗi tự do từ Issue làm lệnh shell.
- `reconcile(target, dispatch_key)` → đọc tình trạng input/chat theo ID và event nguồn trước khi đánh giá lại `UNKNOWN`; không gửi trùng nếu không chứng minh lần trước đã thất bại.
- `status(target, dispatch_key)` và `status-task(target, Issue, Task ID, source_sha)` → chứng cứ `delivered / assistant turn ended` của giao vận; `status-task` không cần UUID và báo khác biệt SHA nếu có. **Không tự suy** `CLAIMED`, PR merged hay `G5_VERIFIED` (các trạng thái đó đối chiếu GitHub).

Payload có giới hạn: `{repository, issue_number, task_id, target, source_sha, base_sha, approved_scope}`. Mỗi bản ghi dispatch lưu timestamp, idempotency key, trạng thái, lỗi đã redact và kết quả reconcile; **không** lưu raw chat hay secrets trong repo hoặc GitHub comments.

## Gate bật `SEND` cho từng host

1. Owner cấp quyền adapter và target project/worktree cụ thể; chứng minh auth/access control, token scope, audit, giới hạn host. Với CoS Windows phải giải quyết các security blocker liên quan trước khi kích hoạt.
2. Dùng task thử nghiệm không có quyền sửa sản phẩm: một yêu cầu PONG một lần tới chat chính mới, xác minh chính xác target agent/project, event assistant hoàn tất, không bị gửi vào worker hoặc chat cũ.
3. Thử duplicate/timeout/crash: input không được gửi lại mù, `UNKNOWN` được reconcile; app offline/credential sai/host sai ⇒ từ chối, không mở endpoint inbound công khai.
4. Lưu evidence đủ host, OS, phiên bản/API, request ID đã redact, result và giới hạn qua PR tài liệu được review. Chỉ khi một host PASS mới bật `SEND` cho **host đó**; host kia vẫn `BLOCKED_ADAPTER`.

Sau khi có PONG evidence, Atlas dùng `RUN` với **Issue đã giao** và `send-issue` một lần; không tạo lại assignment. `STATUS` tra theo Issue/Task/source bằng `status-task`, không cần nhớ request UUID. Với host offline hoặc `can_send=false`, chặn, không retry và không coi task đã thực hiện. Rotation/restart CoS và thử từ chối bearer cũ cần phiên kiểm chứng có kiểm soát vì có thể làm gián đoạn chat đang hoạt động.

## Author-owned documentation PR correction (2026-11-13 extension)

Owner-authorized maintenance adds `check-pr`, `send-pr`, `status-pr`. This is not implementation Issue pickup: a Draft documentation PR may be corrected while its G2/G3 remain proposed/draft. Do not invent an Issue/Task ID or move Project Ready/Done.

The GitHub preflight uses host-local authenticated `gh api --hostname github.com` against the fixed repository. It requires open same-repo PR to main, explicit agent identity plus agent-owned branch, exact head/base SHA, actionable signed Atlas review pinned to head, and a complete bounded list of 1–100 added/modified Markdown files under docs or .agents/research. Both PR and review author must be the repository's authorized shared account; a signature from an outside contributor is insufficient. No code/script/skill edits, rename/delete, free-form payload or arbitrary review text is sent. Revalidate immediately before POST; recipient rechecks all pins and author file ownership before editing.

Durable lock identity is `(target, PR number)`, independent of head/review/request UUID. Same request replays receipt only; uncertain delivery or changed head never authorizes a second send. `status-pr` finds the receipt by PR without a remembered UUID and reports changed head/review references. Recovery from lock-without-receipt is UNKNOWN and requires reconciliation, not automatic release. Agent slot/absence of conflicting work is a sender preflight requirement; GitHub scope alone does not prove the agent is idle.

Windows ACL inspection now uses the native system Windows PowerShell executable and a process-local system-only PSModulePath. Module/read errors terminate and fail closed. It does not modify global environment or any ACL/token. The SSH transport explicitly streams Python source/output as UTF-8 to preserve non-ASCII signed review markers on Windows→Mac.
