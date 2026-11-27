# Làm việc trong OpenDash (Quy trình 7 Bước EGM Tinh gọn)

## 1. Quy trình Chuẩn 7 Bước (`Owner Directive 2026-11-27`) & Tối ưu Token

OpenDash áp dụng **quy trình 7 bước EGM Tinh gọn (Feature Worktree + Test cuối tính năng)** cho mọi tính năng, thay đổi kiến trúc và bản sửa lỗi có phạm vi thiết kế:

1. **Đề bài (Owner):** Chủ dự án nêu ý tưởng hoặc tính năng cần triển khai.
2. **Nghiên cứu EGM (Agent — Elephant + Goldfish):**
   - Nạp ngữ cảnh qua [`.agents/skills/opendash-elephant/SKILL.md`](.agents/skills/opendash-elephant/SKILL.md), [`docs/README.md`](docs/README.md) và [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md). Luôn kiểm tra mã nguồn thực tế trong `cms/` (ưu tiên CodeGraph) trước khi kết luận.
   - Soạn [`docs/designs/<feature>.md`](docs/designs/README.md) (kèm `## Detailed Implementation Plan` chia Slice nội bộ `S01..Sn` & `### Verification Plan`, đăng ký vào `SITEMAP.md`; nếu chạy song song nhiều tính năng có SQL migration, **đặt trước số migration cố định** ngay bước này).
   - **Chọn cỡ Goldfish:** tính năng lớn hoặc đụng lõi / DB / bảo mật / C-SEC / Plugin ABI chạy 1–2 vòng `/egm-review-loop` + `/egm-resolve`; tính năng nhỏ/vừa/UI cô lập chạy 1 vòng Goldfish gọn (hoặc bỏ qua nếu Owner yêu cầu làm nhanh).
3. **Giải thích bằng ngôn ngữ tự nhiên (Agent):** Tóm tắt ngắn gọn, dễ hiểu trong chat cho Owner: **làm gì cho người dùng, thay đổi gì trong hệ thống, phần không làm, và rủi ro/đánh đổi** (nếu có).
4. **Duyệt hướng (Owner):** Owner đồng ý hoặc chỉnh sửa → chốt `Status: Approved` trên spec với đủ 3 trường provenance (`**Approved By:**`, `**Approval Evidence:**`, `**Goldfish Review:**`) và không còn `TODO`/`TBD` ([ADR-0010](docs/decisions/ADR-0010-agent-project-and-merge-authority.md)).
5. **Triển khai trọn gói trên Feature Worktree (`feat/<slug>`):**
   - Agent code tuần tự từng Slice theo kế hoạch trên nhánh/worktree riêng `feat/<slug>`.
   - **Tầng 0 giữa các Slice trung gian (`S01..S(n-1)`):** **KHÔNG build binary, KHÔNG chạy test, KHÔNG mở PR lẻ** — chỉ chạy `python cms/scripts/run-tests.py --check` (và `npm --prefix cms/admin run typecheck` nếu sửa Admin) rồi `git commit` checkpoint.
   - **Chỉ build & chạy test 1 lần duy nhất sau khi code xong toàn bộ tính năng:** chạy Tầng 1 (`check-standards.py`, `check-plans.py`, `rustfmt --check`, `npm run build` nếu sửa Admin, `run-tests.py --changed`, E2E của tính năng) kèm 1 lượt kiểm tra drift gọn (`/egm-drift-report`) ngay trên `feat/<slug>` trước khi mở 1 Feature PR duy nhất.
6. **Báo cáo bằng ngôn ngữ tự nhiên (Agent):** Trình bày rõ ràng trong chat: **các tính năng/hạng mục đã làm xong, kết quả test thực chạy đạt mức nào, giới hạn còn lại (nếu có) và link Feature PR**.
7. **Chốt, Merge & Dọn Worktree (Owner duyệt → Agent thực hiện):**
   - Kéo `origin/main` vào `feat/<slug>` 1 lần cuối (nếu `main` đã tiến lên), xác nhận không conflict & test xanh → merge **1 Feature PR duy nhất** vào `main`.
   - Cập nhật `docs/designs/SITEMAP.md` (`Implemented` hoặc `**Verified**` kèm link [`docs/verification/`](docs/verification/README.md)) + chạy `node cms/scripts/codegraph-index-docs.mjs`.
   - **Xóa branch, xóa worktree và dọn `cms/target` của feature ngay sau khi merge.**

### Quy tắc Tiết kiệm Token bắt buộc cho EGM
- **Lọc `GoldfishContext` theo phạm vi:** Lõi mặc định chỉ gồm [`PRODUCT_REQUIREMENTS.md`](docs/product/PRODUCT_REQUIREMENTS.md) + [`CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md). Chỉ thêm [`PLUGIN_ABI.md`](docs/contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md) hoặc [`UNIFIED_FIELD_SCHEMA.md`](docs/contracts/UNIFIED_FIELD_SCHEMA.md) khi spec thực sự đụng plugin / writer-lease / field schema.
- **Truyền sẵn `file#Lstart-Lend` từ CodeGraph/`rg`:** Elephant tra cứu trước để truyền đúng file + dải dòng cho Goldfish, tránh đọc tuần tự file lớn (`i18n.rs`, `template_sec.rs`, `entries.rs`, `settings.rs`, `field_schema.rs`); Goldfish ưu tiên `codegraph_explore` trước `read_file`.
- **Giới hạn vòng review:** Tối đa **2 vòng** `/egm-review-loop`; vòng 2 chỉ gọi lại đúng reviewer còn Blocker/Major trên phạm vi diff vừa sửa.
- **Phân tầng model subagent:** Truyền `Model: "flash"` cho `research`, `browser`, `egm-structural`, `egm-style`, `egm-comprehension`; tránh mặc định `inherit` (XHigh) cho mọi subagent.

---

## 2. Tự chủ Thực thi, Chạy Song song & Ngừng giao Harbor/Forge

- **Ngừng giao Harbor/Forge & bỏ Issue lẻ (`Owner Directive 2026-11-27`):** Ngừng giao Slice cho **Harbor** và **Forge** (`atlas-task-dispatch`, `github-task-pickup`, CoS dispatch và tạo GitHub Issue lẻ theo từng Slice chuyển sang trạng thái standby/lưu trữ). Agent làm việc trực tiếp với Owner (`Orion`/`Nova`/`Atlas`/`Aster`) tự chủ thực hiện trọn vẹn quy trình 7 bước trên worktree `feat/<slug>`.
- **Quy tắc chạy nhiều Feature song song (Parallel Lanes):**
  - `1 Feature = 1 Worktree = 1 Branch (feat/<slug>) = 1 PR`.
  - **Đặt trước số migration** ngay tại Bước 2 để các nhánh không trùng số khi merge.
  - **Đóng băng đồng bộ giữa chừng:** trong lúc đang code S01–Sn không kéo `origin/main` liên tục (tránh mất cache compile Rust); chỉ sync `origin/main` **1 lần** ở cuối Bước 5 / Bước 7.
  - Giới hạn tối đa **2–3 worktree active/máy** và dọn worktree + `cms/target` ngay sau khi merge ở Bước 7.
- **Default to Action (`Owner Directive 2026-11-21`):** Làm trọn luồng trong thẩm quyền [ADR-0010](docs/decisions/ADR-0010-agent-project-and-merge-authority.md) trong cùng một lượt. Chỉ dừng ở **2 cổng Owner** (Bước 4 duyệt spec và Bước 7 chốt merge) hoặc khi: (a) cần **Owner quyết định** đổi PRD/schema/bảo mật; (b) có ≥ 2 hướng trái ngược không suy được từ repo; (c) gặp Blocker thật sự hoặc 2 lượt lỗi/CI fail liên tiếp.
- **Không trả lời thụ động:** Tự kiểm tra repo/branch/PR live trước và đề xuất sẵn phương án mặc định.
- **Khối bắt buộc cuối mọi lượt trả lời:** `Bước tiếp theo (Next Step)` gồm 3 ý: **Đã xong** (link PR/SHA) · **Đang chờ** · **Bước tiếp theo đề xuất**.
- **Tài liệu vận hành liên quan:** [`.agents/AGENT_OPERATING_STANDARD.md`](.agents/AGENT_OPERATING_STANDARD.md), [`.agents/GITHUB_WORKFLOW.md`](.agents/GITHUB_WORKFLOW.md), [`.agents/agents.yaml`](.agents/agents.yaml), [`github-pr-review-merge`](.agents/skills/github-pr-review-merge/SKILL.md).

---

## 3. Ranh giới Kỹ thuật Bắt buộc

- **Cấu trúc Workspace:** Mã nguồn, migrations (`0001+`), SDK ở [`cms/`](cms/README.md); tài liệu ở [`docs/`](docs/README.md); dữ liệu runtime (`database/`, `themes/`, `plugins/`, `uploads/`) ở root. Xem [`README.md`](README.md), [`CMS_ARCHITECTURE.md`](docs/architecture/CMS_ARCHITECTURE.md), [`PRODUCT_REQUIREMENTS.md`](docs/product/PRODUCT_REQUIREMENTS.md).
- **Bất biến Sản phẩm:** Rust + SQLite **một file thực thi duy nhất**; Visual Builder 4 tầng (`Section → Row → Column → Element`); plugin chạy qua **Wasmi (`=2.0.0`)** trong worker cô lập; bảo toàn dữ liệu khi đổi theme/tắt plugin và giữ bản nháp khi gặp `401`/`412`; phân biệt **Lưu (`draft`)** vs **Xuất bản** cho `post`/`page`/CPT `draft_publish` (Builder và CPT `immediate` cập nhật công khai ngay).
- **Phân tầng & Tái sử dụng ([`CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md), [`M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md), [`TECH_STACK.md`](docs/architecture/TECH_STACK.md)):**
  - `opendash-core` thuần Rust (`serde`, `serde_json`, `url`), không I/O, HTTP hay SQL.
  - **100% truy vấn SQLite (`sqlx::query*`, `sqlx::raw_sql`) đặt trong `cms/crates/opendash-server/src/storage/`**; không viết SQL trong `routes/`.
  - React Admin gọi HTTP qua `cms/admin/src/api/client.ts`.
  - Tái sử dụng base APIs (`id`, `registry`, `HookDispatcher`, `package`, `SiteCoordinator`/`AdmittedMutation`, `auth`, `ApiError`) và giữ nguyên phiên bản dependency đã pin.
- **Giới hạn độ dài & Tổ chức Module (`Feature-First`):** Mọi file nguồn mới (`.rs`, `.ts`, `.tsx` trong `cms/`) **không vượt quá 600 dòng (LOC)**; các file cũ trong `cms/scripts/loc-baseline.json` bị khóa theo cơ chế ratchet (kiểm tra tự động qua `cms/scripts/check-standards.py`). Luôn gom nhóm theo tính năng; khi một tính năng lớn vượt ngưỡng mới tách thành các module con đặt tên theo đúng tính năng/trách nhiệm thực tế, **cấm đặt tên chung chung kiểu `part_*`**.

---

## 4. Kiểm chứng 3 Tầng (`Owner Directive 2026-11-27`), CodeGraph & Git

```bash
# Tầng 0 (giữa các Slice trung gian — chỉ kiểm biên dịch nhanh, KHÔNG chạy test):
python3 cms/scripts/run-tests.py --check
# (+ `npm --prefix cms/admin run typecheck` nếu sửa Admin)

# Tầng 1 (1 lần duy nhất khi code xong toàn bộ tính năng, trước Bước 6):
python3 cms/scripts/check-standards.py && python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py && python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
# (+ `npm --prefix cms/admin run build && node cms/scripts/check-i18n.mjs --strict` nếu sửa cms/admin/)
python3 cms/scripts/run-tests.py --changed
git diff --check

# Tầng 2 (theo đợt release sau khi gom 3–5 tính năng, hoặc khi Owner yêu cầu):
# python3 cms/scripts/run-tests.py --group all --release
```

- Không báo đã build/test nếu chưa chạy thật; phân biệt rõ `Approved` / `Implemented` / `Verified`.
- Ưu tiên CodeGraph (`codegraph_explore` / `node cms/scripts/codegraph-query.mjs`) trước khi sửa code, rồi đọc file thật để xác minh; chạy `node cms/scripts/codegraph-index-docs.mjs` khi cập nhật tài liệu chính.
- Dùng đường dẫn Markdown tương đối; giữ nguyên Git author identity ([`.agents/GIT_WORKTREE_IDENTITY.md`](.agents/GIT_WORKTREE_IDENTITY.md)); không commit secrets, dữ liệu runtime hoặc SQLite; không kích hoạt lại hồ sơ G0–G5 trong [`docs/archive/`](docs/archive/README.md).
