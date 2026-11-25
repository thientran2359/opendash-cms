---
name: atlas-task-dispatch
description: "Dùng khi Atlas hoặc Aster được yêu cầu RUN, RESUME hoặc STATUS các Issue triển khai (từ Design Spec EGM đã duyệt) giao cho Forge (Windows) và Harbor (macOS), hoặc yêu cầu sửa PR tài liệu qua CoS adapter."
---

# Atlas / Aster → Forge / Harbor: RUN / RESUME / STATUS (Chuẩn EGM)

Skill này được **Atlas** (Windows) hoặc **Aster** (macOS) gọi trong một lượt theo yêu cầu của chủ dự án để giao việc triển khai từ một **Design Spec EGM đã duyệt** (`docs/designs/<feature>.md`, `Status: Approved`, đã merge lên `origin/main`) sang **Forge** (CoS Windows) và **Harbor** (CoS macOS) thông qua adapter [`.agents/scripts/cos_dispatch.py`](../../scripts/cos_dispatch.py). **Không chạy watcher, cron hay polling nền.**

- **Nguồn trạng thái:** GitHub Issues + Project #3 + PR/CI; trạng thái giao vận: CoS local receipt.
- **Tài liệu liên quan:** [`AGENTS.md`](../../../AGENTS.md), [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md), [ADR-0010](../../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md), [GitHub workflow](../../GITHUB_WORKFLOW.md), [Operating Standard](../../AGENT_OPERATING_STANDARD.md), [`github-task-pickup`](../github-task-pickup/SKILL.md) và [hợp đồng adapter](references/direct-adapter-contract.md).

## Các lệnh hỗ trợ

| Lệnh | Khi nào | Hành động |
| --- | --- | --- |
| `RUN forge`, `RUN harbor`, `RUN both` | Giao / kích hoạt Issue triển khai đã gắn với Design Spec EGM `Approved` | Kiểm GitHub, `probe`, `status-task`, gọi `send-issue` **một lần** nếu đủ gate |
| `RESUME Issue #N` | Tiếp tục cùng implementation task sau khi có ACK/feedback mới | Gọi `resume-issue` với structured ACK, pin PR/head/base, lượt cũ đã kết thúc và slot rảnh |
| `STATUS forge`, `STATUS harbor`, `STATUS both` | Kiểm tra tiến độ, receipt, blocker | Chỉ đọc GitHub + `status-task`/`status`; không gửi chat |
| `RUN PR #N` / `STATUS PR #N` | Yêu cầu tác giả sửa PR Markdown tài liệu có review actionable | Chạy `check-pr`/`status-pr`, rồi `send-pr` một lần nếu đủ gate |

## Bước kiểm tra trước `RUN Issue`

1. Xác minh định danh coordinator (Atlas/Aster) và cập nhật remote refs an toàn. Đọc live open Issues có label `agent:forge` / `agent:harbor`, Project #3 Status, comments, PR/CI, Task ID, `source_sha` (40 hex trên `origin/main` chứa `docs/designs/<feature>.md` đã `Approved`), `base_sha`, dependencies và file ownership.
2. Xác minh trạng thái `Ready` trên Project #3 (hoặc đồng bộ `Backlog → Ready` khi Design Spec `Approved` đã merge, dependencies clear và agent còn slot, rồi re-fetch xác nhận đúng `Ready`). Không thể xác minh `Ready` ⇒ `BLOCKED_PROJECT_READY`, không gọi `send-issue`.
3. Chạy `.agents/scripts/cos_dispatch.py` đã đồng bộ từ `main`. Trên Atlas Windows gọi Harbor qua `OPENDASH_COS_MAC_SSH` (hoặc `--ssh-host`); trên Aster macOS gọi Harbor bằng `--local`. Token CoS luôn nằm trên từng host, không mang ra khỏi host.

## `RUN` — Kích hoạt một main chat CoS mới

```powershell
python .agents/scripts/cos_dispatch.py probe forge
python .agents/scripts/cos_dispatch.py status-task forge --issue <N> --task-id <Task-ID> --source-sha <approved-spec-source-sha>
python .agents/scripts/cos_dispatch.py send-issue forge --request-id <lowercase-uuid> --issue <N> --task-id <Task-ID> --source-sha <approved-spec-source-sha> --base-sha <approved-base-sha>
python .agents/scripts/cos_dispatch.py status-task forge --issue <N> --task-id <Task-ID> --source-sha <approved-spec-source-sha>
```

- Thay `forge` bằng `harbor` cho CoS macOS (Aster trên macOS thêm `--local`).
- Chỉ tạo UUID mới **trước lần gửi hợp lệ đầu tiên**. Nếu `status-task` đã có `ACCEPTED`/`UNKNOWN`, **không tạo UUID mới để gửi lại**; chỉ đối chiếu `status-task` hoặc dùng `RESUME Issue`.
- `HTTP 202` (`ACCEPTED`), `outbox_delivery=sent` và `assistant_turn_ended` chỉ là biên nhận giao vận, **không chứng minh code đã merge hay test đã PASS**.

## `STATUS` — Chỉ đọc, không tạo chat mới

```powershell
python .agents/scripts/cos_dispatch.py probe harbor
python .agents/scripts/cos_dispatch.py status-task harbor --issue <N> --task-id <Task-ID> --source-sha <approved-spec-source-sha>
```

Báo mỗi agent: **Issue/Task | Project gate | CoS receipt (`NOT_SENT`/`ACCEPTED`/`sent`/`turn_end`) | PR & verification thực tế | blocker / bước tiếp theo**.

## `RESUME Issue` — Tiếp tục task đã có `send-issue` khi có ACK mới

Chỉ dùng sau khi lượt trước đã kết thúc (`ACCEPTED`, `outbox_delivery=sent`, `latest turn ended`), có comment ACK có cấu trúc (ký `Agent: Atlas` hoặc `Agent: Aster` kèm marker `<!-- opendash-resume-v1 ... -->`) pin đúng repo/target/Issue/Task/source/base và slot project rảnh. Hỗ trợ cả 2 trường hợp:

1. **Đã có PR mở** (pin thêm `--pr` và `--head-sha`):
```bash
python3.13 .agents/scripts/cos_dispatch.py --local resume-issue harbor \
  --request-id <uuid> --issue <N> --task-id <Task-ID> \
  --source-sha <approved-spec-source-sha> --base-sha <current-PR-base-SHA> \
  --pr <PR> --head-sha <current-PR-head-SHA> --ack-id <new-Issue-comment-ID>
```

2. **Chưa có PR (pre-PR resume — lượt trước dừng sớm do lỗi worktree/môi trường):** bỏ `--pr` và `--head-sha` (trong marker `<!-- opendash-resume-v1 ... -->` đặt `"pr": null, "head_sha": null` hoặc bỏ trống 2 trường này):
```bash
python3.13 .agents/scripts/cos_dispatch.py --local resume-issue harbor \
  --request-id <uuid> --issue <N> --task-id <Task-ID> \
  --source-sha <approved-spec-source-sha> --base-sha <approved-base-SHA> \
  --ack-id <new-Issue-comment-ID>
```

## `RUN PR` — Tác giả sửa PR tài liệu theo review

Chỉ áp dụng cho PR Markdown (`docs/` hoặc `.agents/research/`, 1–100 file, không sửa code/scripts/skills) có review `CHANGES_REQUESTED` actionable trên đúng `head_sha`:

```powershell
python .agents/scripts/cos_dispatch.py probe forge
python .agents/scripts/cos_dispatch.py check-pr forge --pr <N> --head-sha <40-hex-head> --base-sha <40-hex-PR-base> --review-id <review-ID>
python .agents/scripts/cos_dispatch.py status-pr forge --pr <N> --head-sha <40-hex-head> --review-id <review-ID>
python .agents/scripts/cos_dispatch.py send-pr forge --request-id <lowercase-uuid> --pr <N> --head-sha <40-hex-head> --base-sha <40-hex-PR-base> --review-id <review-ID>
```
