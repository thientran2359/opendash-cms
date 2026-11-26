# Làm việc trong OpenDash (Quy trình Chuẩn EGM)

## 1. Quy trình Chuẩn EGM & Tối ưu Token

OpenDash áp dụng **EGM (Elephant soạn spec → Goldfish review độc lập → Duyệt → Mới viết code)** cho mọi tính năng, thay đổi kiến trúc và bản sửa lỗi có phạm vi thiết kế:

1. **Nạp ngữ cảnh:** Đọc [`.agents/skills/opendash-elephant/SKILL.md`](.agents/skills/opendash-elephant/SKILL.md), [`docs/README.md`](docs/README.md) và [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md). Luôn kiểm tra mã nguồn thực tế trong `cms/` (ưu tiên CodeGraph) trước khi kết luận.
2. **Soạn & Review Spec:** Elephant chạy `/egm-brainstorm` (khi cần) → `/egm-draft` lưu [`docs/designs/<feature>.md`](docs/designs/README.md) (kèm `## Detailed Implementation Plan` & `### Verification Plan`, đăng ký vào `SITEMAP.md`) → `/egm-review` hoặc `/egm-review-loop` (chỉ chạy trên harness hỗ trợ fresh-context subagent) → `/egm-resolve`.
3. **Duyệt & Triển khai (3 tầng theo [ADR-0010](docs/decisions/ADR-0010-agent-project-and-merge-authority.md)):**
   - Spec chỉ đạt `Status: Approved` khi đủ 3 trường provenance (`**Approved By:**`, `**Approval Evidence:**`, `**Goldfish Review:**`) và không còn `TODO`/`TBD`.
   - **Owner duyệt:** đổi PRD/phạm vi, đổi kiến trúc lõi/schema breaking, đổi phân quyền hoặc release public. **Coordinator (`Atlas`, `Aster`, `Nova`, `Orion`) duyệt:** spec trong phạm vi PRD đã chốt. **Implementer (`Nova`, `Orion`, `Forge`, `Harbor`) tự chủ:** triển khai Slice `Ready` và tự merge khi đạt 100% required CI (`governance-*`, `cms-ci`) + `Verification Plan`.
   - Đóng Spec: sau khi triển khai & kiểm chứng ([`docs/verification/`](docs/verification/README.md)), bắt buộc chạy `/egm-drift-report` trước khi chuyển `SITEMAP.md` sang `Verified` và chạy `node cms/scripts/codegraph-index-docs.mjs`.

### Quy tắc Tiết kiệm Token bắt buộc cho EGM
- **Lọc `GoldfishContext` theo phạm vi:** Lõi mặc định chỉ gồm [`PRODUCT_REQUIREMENTS.md`](docs/product/PRODUCT_REQUIREMENTS.md) + [`CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md). Chỉ thêm [`PLUGIN_ABI.md`](docs/contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md) hoặc [`UNIFIED_FIELD_SCHEMA.md`](docs/contracts/UNIFIED_FIELD_SCHEMA.md) khi spec thực sự đụng plugin / writer-lease / field schema.
- **Truyền sẵn `file#Lstart-Lend` từ CodeGraph/`rg`:** Elephant tra cứu trước để truyền đúng file + dải dòng cho Goldfish, tránh đọc tuần tự file lớn (`i18n.rs`, `template_sec.rs`, `entries.rs`, `settings.rs`, `field_schema.rs`); Goldfish ưu tiên `codegraph_explore` trước `read_file`.
- **Giới hạn vòng review:** Tối đa **2 vòng** `/egm-review-loop`; vòng 2 chỉ gọi lại đúng reviewer còn Blocker/Major trên phạm vi diff vừa sửa.
- **Phân tầng model subagent:** Truyền `Model: "flash"` cho `research`, `browser`, `egm-structural`, `egm-style`, `egm-comprehension`; tránh mặc định `inherit` (XHigh) cho mọi subagent.

---

## 2. Tự chủ Thực thi, Bàn giao & Phân công Đa-Agent

- **Default to Action (`Owner Directive 2026-11-21`):** Làm trọn luồng trong thẩm quyền [ADR-0010](docs/decisions/ADR-0010-agent-project-and-merge-authority.md) trong cùng một lượt. Chỉ dừng khi: (a) cần **Owner quyết định**; (b) có ≥ 2 hướng trái ngược không suy được từ repo/GitHub; (c) gặp Blocker thật sự hoặc 2 lượt CI fail liên tiếp.
- **Không trả lời thụ động:** Tự kiểm tra Issue/PR/branch live trước và đề xuất sẵn phương án mặc định.
- **Khối bắt buộc cuối mọi lượt trả lời:** `Bước tiếp theo (Next Step)` gồm 3 ý: **Đã xong** (link Issue/PR/SHA) · **Đang chờ** · **Bước tiếp theo đề xuất**.
- **Phân công Slice (`Owner Directive 2026-11-22`):** Coordinator chuẩn bị Issue từng Slice (`Ready`/`Backlog`, `agent:harbor` hoặc `agent:forge`, đủ `Source SHA` & `Baseline SHA`) và **không tự triển khai Slice** trừ khi Owner yêu cầu trực tiếp.
- **Tài liệu & Skill điều phối:** [`.agents/AGENT_OPERATING_STANDARD.md`](.agents/AGENT_OPERATING_STANDARD.md) (gồm bàn giao hai chiều `Harbor ↔ Aster`), [`.agents/GITHUB_WORKFLOW.md`](.agents/GITHUB_WORKFLOW.md), [`.agents/agents.yaml`](.agents/agents.yaml), `atlas-task-dispatch`, `github-task-pickup`, `github-pr-review-merge`.

---

## 3. Ranh giới Kỹ thuật Bắt buộc

- **Cấu trúc Workspace:** Mã nguồn, migrations (`0001`–`0052`), SDK ở [`cms/`](cms/README.md); tài liệu ở [`docs/`](docs/README.md); dữ liệu runtime (`database/`, `themes/`, `plugins/`, `uploads/`) ở root. Xem [`README.md`](README.md), [`CMS_ARCHITECTURE.md`](docs/architecture/CMS_ARCHITECTURE.md), [`PRODUCT_REQUIREMENTS.md`](docs/product/PRODUCT_REQUIREMENTS.md).
- **Bất biến Sản phẩm:** Rust + SQLite **một file thực thi duy nhất**; Visual Builder 4 tầng (`Section → Row → Column → Element`); plugin chạy qua **Wasmi (`=2.0.0`)** trong worker cô lập; bảo toàn dữ liệu khi đổi theme/tắt plugin và giữ bản nháp khi gặp `401`/`412`; phân biệt **Lưu (`draft`)** vs **Xuất bản** cho `post`/`page`/CPT `draft_publish` (Builder và CPT `immediate` cập nhật công khai ngay).
- **Phân tầng & Tái sử dụng ([`CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md), [`M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md), [`TECH_STACK.md`](docs/architecture/TECH_STACK.md)):**
  - `opendash-core` thuần Rust (`serde`, `serde_json`, `url`), không I/O, HTTP hay SQL.
  - **100% truy vấn SQLite (`sqlx::query*`, `sqlx::raw_sql`) đặt trong `cms/crates/opendash-server/src/storage/`**; không viết SQL trong `routes/`.
  - React Admin gọi HTTP qua `cms/admin/src/api/client.ts`.
  - Tái sử dụng base APIs (`id`, `registry`, `HookDispatcher`, `package`, `SiteCoordinator`/`AdmittedMutation`, `auth`, `ApiError`) và giữ nguyên phiên bản dependency đã pin.

---

## 4. Kiểm chứng (2 tầng — `Owner Directive 2026-11-26`), CodeGraph & Git

```bash
python cms/scripts/check-standards.py && python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py && python cms/scripts/check-plans.py --self-test
python .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
python cms/scripts/run-tests.py --changed   # Tầng 1 (mặc định); Tầng 2 trước release: --group all --release
git diff --check
```

- Không báo đã build/test nếu chưa chạy thật; phân biệt rõ `Approved` / `Implemented` / `Verified`.
- Ưu tiên CodeGraph (`codegraph_explore` / `node cms/scripts/codegraph-query.mjs`) trước khi sửa code, rồi đọc file thật để xác minh; chạy `node cms/scripts/codegraph-index-docs.mjs` khi cập nhật tài liệu chính.
- Dùng đường dẫn Markdown tương đối; giữ nguyên Git author identity ([`.agents/GIT_WORKTREE_IDENTITY.md`](.agents/GIT_WORKTREE_IDENTITY.md)); không commit secrets, dữ liệu runtime hoặc SQLite; không kích hoạt lại hồ sơ G0–G5 trong [`docs/archive/`](docs/archive/README.md).
