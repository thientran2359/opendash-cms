---
name: github-pr-review-merge
description: "Dùng khi Atlas, Aster, Nova hoặc Orion được yêu cầu review hoặc merge PR OpenDash (Design Spec EGM hoặc Feature PR feat/<slug> đủ kiểm chứng Tầng 1 và đã được Owner chốt theo quy trình 7 bước)."
---

# Review và Merge Feature PR (Chuẩn 7 Bước + ADR-0010)

> Theo [ADR-0010](../../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md) và `Owner Directive 2026-11-27`, quy trình chuẩn là **1 Feature = 1 Worktree (`feat/<slug>`) = 1 Feature PR** vào `main`. **Chờ Owner chốt ở Bước 7 trước khi merge Feature PR** (trừ khi Owner ủy quyền merge ngay từ đầu).

Đọc [`AGENTS.md`](../../../AGENTS.md), [`.agents/skills/opendash-elephant/SKILL.md`](../opendash-elephant/SKILL.md), [Agent Operating Standard](../../AGENT_OPERATING_STANDARD.md), [GitHub workflow](../../GITHUB_WORKFLOW.md) và [agents.yaml](../../agents.yaml). Chỉ thao tác trên repo `thientran2359/opendash-cms`, đích `main`. Không đổi ruleset, không bypass required checks hoặc dùng quyền admin để vượt gate.

## Chế độ hoạt động

- **`REVIEW_ONLY` (Mặc định trước Bước 7):** Mở Feature PR sau khi hoàn tất Bước 5 + kiểm chứng Tầng 1, kiểm tra diff/CI/drift, hoặc khi được yêu cầu review PR `#N` ⇒ báo cáo bằng ngôn ngữ tự nhiên cho Owner (Bước 6); **chưa merge** khi Owner chưa chốt.
- **`REVIEW_AND_MERGE` (Bước 7 sau khi Owner chốt):** Chỉ kích hoạt sau khi Owner đồng ý chốt merge (hoặc chỉ thị merge thẳng nếu đủ gate), mọi check Tầng 1 và required CI đều PASS.

## 1. Kiểm tra trên snapshot thực

1. Đọc metadata + diff của head SHA `H` và base SHA `B`, đối chiếu với Design Spec gốc (`docs/designs/<feature>.md`). Không tin lời "đã test" nếu thiếu bằng chứng hoặc CI fail.
2. **Đối chiếu theo loại PR:**
   - **PR Design Spec (EGM):** xác minh đã qua Goldfish review (theo cỡ tính năng), giải thích tự nhiên cho Owner, đăng ký trong [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md), có `## Detailed Implementation Plan` + `### Verification Plan`, và `python3 cms/scripts/check-plans.py` PASS.
   - **Feature PR (`feat/<slug>`):** xác minh đã code đủ các Slice trong spec, chạy **1 lần đầy đủ Tầng 1** (`check-standards.py`, `check-plans.py`, `rustfmt --check`, `npm run build` + `check-i18n.mjs --strict` nếu sửa Admin, `python3 cms/scripts/run-tests.py --changed`, E2E của tính năng) và kiểm tra drift gọn trước khi mở PR.
3. Kiểm tra CI/check-runs **tại đúng head SHA `H`**: mọi required check phải `SUCCESS` (nếu ruleset đang bật); empty/pending/failed ⇒ chưa eligible.
4. Ghi `Self-review: <Agent> — reviewed head <SHA>` (hoặc verdict reviewer kèm file/dòng nếu review PR khác) cùng kết quả kiểm chứng thực chạy lên PR.

## 2. Gate Merge (`MERGE_ELIGIBLE`)

Chỉ merge khi **tất cả** điều kiện sau đạt:
- Đã có xác nhận chốt của Owner ở Bước 7 (hoặc ủy quyền tường minh cho PR đó).
- PR open, non-draft, base `main`, không conflict (nếu `main` vừa tiến lên, sync `origin/main` vào `feat/<slug>` 1 lần cuối và chạy lại Tầng 1 nếu có merge code mới).
- Không còn findings bắt buộc (`P0`/`P1`) hoặc drift blocker chưa xử lý.
- Mọi kiểm chứng Tầng 1 và required CI checks đều PASS trên head SHA `H`.

## 3. Merge, Cập nhật SITEMAP & Dọn Worktree (Bước 7)

1. Ngay trước merge, re-fetch PR head/base và checks (`STALE_REVIEW` nếu head đổi khác `H`).
2. Merge có precondition `--match-head-commit <H>` (ưu tiên `merge` hoặc phương thức hợp lệ theo ruleset), không bật auto-merge, không force/admin bypass.
3. Nếu API lỗi/timeout sau lệnh merge ⇒ `MERGE_UNKNOWN`; đọc lại PR để xác minh thực tế trước khi tiếp tục.
4. Sau khi xác nhận đã merge vào `main`:
   - Cập nhật trạng thái trong [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md) (`Implemented` hoặc `**Verified**` kèm link `docs/verification/*.md`) và chạy `node cms/scripts/codegraph-index-docs.mjs`.
   - **Dọn dẹp worktree & branch:** xóa branch `feat/<slug>` (local + remote), chạy `git worktree remove ...` và dọn `cms/target` của worktree vừa hoàn thành.

## Kết quả

| Kết quả | Ý nghĩa |
| --- | --- |
| `CHANGES_REQUESTED` / `AWAITING_AUTHOR` | Còn lỗi bắt buộc; tác giả sửa trên nhánh `feat/<slug>` |
| `APPROVED_REVIEW_ONLY` | Đủ kiểm chứng Tầng 1, chờ Owner chốt ở Bước 7 |
| `BLOCKED` / `STALE_REVIEW` | Thiếu gate/evidence hoặc head đã thay đổi; không merge |
| `MERGE_UNKNOWN` | Lỗi/timeout lúc merge; cần đọc lại trạng thái thực tế |
| `MERGED` | Đã merge thành công, cập nhật SITEMAP và dọn branch/worktree |
