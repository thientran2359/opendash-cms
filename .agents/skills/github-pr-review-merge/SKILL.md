---
name: github-pr-review-merge
description: "Dùng khi Atlas, Aster hoặc Nova được yêu cầu review hoặc merge PR OpenDash (Design Spec EGM hoặc PR triển khai của Forge/Harbor) đủ điều kiện theo ADR-0010. Hỗ trợ review độc lập và self-review có bằng chứng."
---

# Review và Merge PR (Chuẩn EGM + Multi-Agent)

> Theo [ADR-0010](../../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md), Atlas và Aster có quyền điều phối & merge mọi PR đủ gate; Nova, Harbor và Forge có thể self-review và merge PR của chính mình khi đạt đầy đủ gate và CI checks.

Đọc [`AGENTS.md`](../../../AGENTS.md), [`.agents/skills/opendash-elephant/SKILL.md`](../opendash-elephant/SKILL.md), [Agent Operating Standard](../../AGENT_OPERATING_STANDARD.md), [GitHub workflow](../../GITHUB_WORKFLOW.md) và [agents.yaml](../../agents.yaml). Chỉ thao tác trên repo `thientran2359/opendash-cms`, đích `main`. Không đổi ruleset, không bypass required checks hoặc dùng quyền admin để vượt gate.

## Chế độ hoạt động

- **`REVIEW_ONLY`:** Người dùng chỉ yêu cầu "review PR #N" hoặc "kiểm tra diff" ⇒ review, đăng findings/verdict; **không merge**.
- **`REVIEW_AND_MERGE`:** Người dùng gọi rõ `$github-pr-review-merge` hoặc yêu cầu "review và merge PR đủ điều kiện" ⇒ rà soát, review và **tự merge các PR đủ toàn bộ gate** (cả PR Design Spec EGM lẫn PR code triển khai của Forge/Harbor).

## 1. Chọn PR trong một lượt

- Nếu có PR cụ thể, xử lý đúng PR đó. Nếu rà hàng chờ, lấy danh sách PR **open, non-draft, base `main`** theo thứ tự dependency rồi thời điểm tạo.
- Mỗi PR phải rõ loại:
  - **PR Design Spec (EGM):** tạo/cập nhật `docs/designs/<feature>.md` và `docs/designs/SITEMAP.md`.
  - **PR Triển khai (Implementation):** code/tests/migrations trong `cms/` (thường do Forge trên Windows hoặc Harbor trên macOS thực hiện) bám sát `## Detailed Implementation Plan` của một Design Spec đã `Approved`.

## 2. Review trên snapshot thực

1. Đọc metadata + **toàn bộ diff** của head SHA `H` và base SHA `B`, Issue liên kết, Design Spec gốc (`docs/designs/<feature>.md`) và các review threads hiện có. Không tin lời "đã test" nếu thiếu bằng chứng hoặc CI fail.
2. **Đối chiếu theo loại PR:**
   - **Với PR Design Spec (EGM):** xác minh đã qua review Goldfish (`/egm-review` + `/egm-resolve`), đăng ký trong `docs/designs/SITEMAP.md`, có đủ `## Detailed Implementation Plan` + `### Verification Plan`, tuân thủ `CORE_FOUNDATION.md`, `PLUGIN_ABI.md`, `M8_WRITER_INVENTORY.md`, và `check-plans.py` PASS.
   - **Với PR Triển khai (Implementation):** đối chiếu code thật trong `cms/` với từng mục trong `## Detailed Implementation Plan` và `### Verification Plan`; dùng CodeGraph kiểm tra callers/impact nếu cần; xác minh ranh giới kiến trúc (`check-standards.py`), `cargo test --workspace --release --locked` và E2E/native tests phù hợp OS.
3. Kiểm tra CI/check-runs **tại đúng head SHA `H`**: mọi required check phải `SUCCESS`; empty/pending/failed ⇒ chưa eligible.
4. Phân loại findings `P0/P1/P2` (file, dòng, bằng chứng, tác động, hướng sửa):
   - Nếu còn lỗi bắt buộc ⇒ `CHANGES_REQUESTED`, để tác giả sửa và push trên branch của họ (reviewer không tự sửa branch của tác giả). Dừng PR đó ở `AWAITING_AUTHOR`.
   - Nếu là PR do chính reviewer viết ⇒ ghi `Self-review: <Agent> — reviewed head <SHA>` kèm kết quả kiểm chứng thực chạy.

## 3. Gate Merge (`MERGE_ELIGIBLE`)

Chỉ merge khi **tất cả** điều kiện sau đạt:
- Chế độ `REVIEW_AND_MERGE` có hiệu lực, PR open, non-draft, base `main`, không conflict.
- Không còn findings bắt buộc (`P0`/`P1`) hoặc review threads chưa giải quyết.
- Mọi required CI checks và tiêu chí kiểm chứng trước merge đều PASS trên head SHA `H`.
- Kiểm tra `Fixes/Closes/Resolves` trước merge để không tự động đóng Issue còn thiếu bước kiểm chứng hậu merge (ví dụ test trên OS còn lại).

## 4. Merge có kiểm Head & Đối chiếu Kết quả

1. **Ngay trước merge**, re-fetch PR head/base, threads và required checks:
   - Head đổi khác `H` ⇒ dừng, trả `STALE_REVIEW` và review delta mới trước.
   - Base đổi khác `B` ⇒ đánh giá lại tích hợp với `main` mới trước khi merge.
2. Merge có precondition `expected_head_sha=H` (hoặc `gh pr merge --match-head-commit H`), ưu tiên `merge` để giữ lịch sử agent; **không bật auto-merge**, không xóa branch `agent/<name>`, không force/admin bypass.
3. Nếu API lỗi/timeout sau lệnh merge ⇒ `MERGE_UNKNOWN`; đọc lại PR để xác minh thực tế trước khi làm bất kỳ bước nào tiếp theo.
4. Sau khi xác nhận đã merge, ghi receipt ngắn trên PR/Issue (reviewer, reviewed head, checks/evidence, merge SHA, phần kiểm chứng đa OS còn lại nếu có). Khi toàn bộ verification đã đạt, cập nhật trạng thái tương ứng trong [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md).

## Kết quả

| Kết quả | Ý nghĩa |
| --- | --- |
| `CHANGES_REQUESTED` / `AWAITING_AUTHOR` | Còn lỗi bắt buộc; tác giả sửa trên branch của mình |
| `APPROVED_REVIEW_ONLY` | Review đạt, không gọi merge |
| `BLOCKED` / `STALE_REVIEW` | Thiếu gate/evidence hoặc head đã thay đổi; không merge |
| `MERGE_UNKNOWN` | Lỗi/timeout lúc merge; cần đọc lại trạng thái thực tế |
| `MERGED` | Đã merge thành công kèm merge SHA và receipt kiểm chứng |
