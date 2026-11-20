# Làm việc trong OpenDash (Quy trình Chuẩn EGM)

## 1. Quy trình Phát triển Chuẩn: Elephant-Goldfish Model (EGM)

OpenDash áp dụng quy trình **EGM (Elephant soạn spec → Goldfish review độc lập → Chủ dự án duyệt → Mới viết code)** cho mọi tính năng, thay đổi kiến trúc và bản sửa lỗi có phạm vi thiết kế, thay thế hoàn toàn quy trình G0–G5 và điều phối liên-agent cũ:

1. **Nạp Ngữ cảnh Dự án (`Elephant`):**
   - Trước khi bắt đầu thiết kế hoặc review, đọc skill ngữ cảnh [`.agents/skills/opendash-elephant/SKILL.md`](.agents/skills/opendash-elephant/SKILL.md), [`docs/README.md`](docs/README.md) và [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md).
   - Kiểm tra mã nguồn thực tế trong `cms/` (và tra cứu CodeGraph) trước khi tuyên bố một chức năng đã có hay chưa có.
2. **Bước 1 — Khám phá & Phản biện Ý tưởng (`/egm-brainstorm`, khi cần làm rõ bài toán):**
   - **Elephant** thảo luận cùng chủ dự án để làm rõ bài toán, các phương án kỹ thuật và đánh đổi (trade-offs), sau đó gọi swarm **Goldfish** phản biện sớm và lưu `concepts_brief.md` (ở vùng artifact/scratch của phiên chat).
3. **Bước 2 — Soạn thảo Đặc tả Thiết kế (`/egm-draft`):**
   - **Elephant** soạn thảo đặc tả kỹ thuật theo từng mục có cổng phê duyệt của chủ dự án (`Overview & Goals` → `Technical Plan` → `Alternatives Considered` → `Detailed Implementation Plan`).
   - Lưu đặc tả hoàn chỉnh vào [`docs/designs/<feature>.md`](docs/designs/README.md) và đăng ký vào [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md).
   - Phần `## Detailed Implementation Plan` ở nửa dưới file phải liệt kê đầy đủ từng file sẽ tạo/sửa trong `cms/` và `docs/`, lý do thay đổi, chữ ký hàm/schema/API và kế hoạch kiểm chứng (`### Verification Plan`), không để lại quyết định thiết kế mơ hồ cho lúc code.
4. **Bước 3 — Review Độc lập bằng Goldfish (`/egm-review` / `/egm-review-loop` + `/egm-resolve`):**
   - Khởi chạy các subagent **Goldfish** trong ngữ cảnh mới hoàn toàn (zero-context) để kiểm tra đặc tả tại `docs/designs/<feature>.md`, ánh xạ đúng preset của EGM upstream `0.1.1`:
     - **Kiến trúc lớn:** chạy `/egm-review` preset `uninterrupted` hoặc `interactive` (Stages `5 → 6 → 7 → 8 → 9`: `egm-zoom-out`, `egm-comprehension`, `egm-design`, `egm-structural`, `egm-style`, `egm-implementation-readiness`, `egm-codebase-details`).
     - **Tính năng nhỏ/vừa hoặc bản sửa lỗi có phạm vi thiết kế:** chạy `/egm-review-loop` với 3 reviewer lõi (`design_critic`, `codebase_details`, `implementation_readiness`).
     - **Giới hạn harness:** Chỉ chạy Goldfish review trên harness hỗ trợ fresh-context subagent (Antigravity App/IDE Extension, Codex `spawn_agent` đọc `agents/*.md`); không tự review trong cùng context trên môi trường thiếu subagent (CoS / standalone IDE).
   - Luôn nạp `GoldfishContext` chuẩn gồm: [`docs/product/PRODUCT_REQUIREMENTS.md`](docs/product/PRODUCT_REQUIREMENTS.md), [`docs/contracts/CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md), [`docs/contracts/PLUGIN_ABI.md`](docs/contracts/PLUGIN_ABI.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md) và các file mã nguồn `cms/` liên quan.
   - **Elephant** chạy `/egm-resolve` để phân loại findings (`PROPOSE FIX` vs `CONTEST`), lưu tóm tắt kết quả review vào PR Design Spec, và cập nhật đầy đủ 3 trường provenance bắt buộc khi chuyển sang `Status: Approved`: `**Approved By:**`, `**Approval Evidence:**`, `**Goldfish Review:**` (không còn `Open Questions` mở hay `TODO`/`TBD`).
   - **Phân quyền duyệt Spec (3 tầng — đồng bộ [ADR-0010](docs/decisions/ADR-0010-agent-project-and-merge-authority.md)):**
     - **Owner bắt buộc duyệt:** Đổi PRD/phạm vi sản phẩm, đổi kiến trúc lõi/schema breaking, đổi quy trình/phân quyền hoặc release public (`Approved By: Owner`).
     - **Coordinator (`Atlas`, `Aster`) được ủy quyền duyệt:** Design Spec nằm trong phạm vi PRD đã chốt, bản sửa lỗi có phạm vi thiết kế (`Approved By: Atlas (delegated)` hoặc `Aster (delegated)`).
     - **Implementer (`Nova`, `Forge`, `Harbor`) tự chủ:** Nhận Issue `Ready`, triển khai đúng `Detailed Implementation Plan`, tự review & tự merge PR của mình khi đạt 100% required CI (`governance-*`, `cms-ci`) và `Verification Plan`.
5. **Bước 4 — Triển khai & Kiểm chứng (`Implement & Verify`):**
   - Chỉ bắt đầu viết code sau khi Design Spec đạt `Status: Approved` (đủ provenance) và đã merge vào `main`. Với Design Spec lớn, chia `### Slice S01: ...` trong `## Detailed Implementation Plan` (mỗi slice bắt buộc có `**Dependencies:**`, `**File Ownership:**`, `**Acceptance Criteria:**`, `**Rollback:**`) và tạo Issue từng slice bằng `prepare-github-issue.py --source docs/designs/<feature>.md --slice S01`.
   - Sau khi triển khai, bắt buộc chạy bộ kiểm tra chuẩn và ghi nhận kết quả thực tế (kèm giới hạn nếu có); với các đợt kiểm chứng lớn hoặc release, lưu biên bản tại [`docs/verification/`](docs/verification/README.md).
   - Khi hoàn tất merge & kiểm chứng thực tế: cập nhật `Status` trong [`docs/designs/SITEMAP.md`](docs/designs/SITEMAP.md) (`Approved` → `Implemented` → `Verified`), gắn ghi chú `Superseded by docs/designs/<feature>.md` tại mục bị thay thế ở contract nền tảng cũ (nếu có) và chạy `node cms/scripts/codegraph-index-docs.mjs`.
6. **Bước 5 — Duy trì Ngữ cảnh & Chống Trôi Tài liệu (`/egm-bootstrap` & `/egm-drift-report`):**
   - Dùng `/egm-bootstrap` để cập nhật hệ thống `README.md` theo cấu trúc cây trong `cms/`.
   - **Trigger `/egm-drift-report`:** Coordinator đóng spec bắt buộc chạy `/egm-drift-report` **sau khi mỗi Design Spec chuyển sang `Implemented`** (trước khi đánh dấu `**Verified**` trên `SITEMAP.md`) và trước mỗi đợt release, lưu báo cáo tại `.agents/research/` hoặc đính kèm PR closure.

### 1.2. Phối hợp Đa-Agent (`Atlas / Nova / Aster → Forge / Harbor`)

EGM phụ trách **tạo và review đặc tả kỹ thuật**, còn việc triển khai và kiểm chứng đa nền tảng (Windows / macOS) tiếp tục tận dụng đội ngũ agent độc lập thông qua GitHub Issues/PR và 3 skill điều phối đã chuẩn hóa theo EGM:

- **Nghiên cứu, soạn spec & điều phối (`Atlas`, `Nova`, `Aster`):** Đóng vai **Elephant** chạy `/egm-brainstorm` → `/egm-draft` → `/egm-review` + `/egm-resolve` để chốt `docs/designs/<feature>.md` (`Status: Approved`), mở GitHub Issue trỏ thẳng tới `## Detailed Implementation Plan` và `source_sha` trên `origin/main`.
- **Giao việc sang subagent (`atlas-task-dispatch`):** `Atlas` (Windows) hoặc `Aster` (macOS) dùng [`.agents/skills/atlas-task-dispatch/SKILL.md`](.agents/skills/atlas-task-dispatch/SKILL.md) để `RUN` / `RESUME` / `STATUS` các Issue giao cho **Forge** (Windows) và **Harbor** (macOS) qua `cos_dispatch.py`.
- **Nhận việc & triển khai (`github-task-pickup`):** **Forge** và **Harbor** dùng [`.agents/skills/github-task-pickup/SKILL.md`](.agents/skills/github-task-pickup/SKILL.md) để nhận Issue trên nhánh mặc định `agent/<name>`, triển khai đúng `## Detailed Implementation Plan`, chạy đủ kiểm chứng native và mở PR.
- **Review & Merge PR (`github-pr-review-merge`):** `Atlas`, `Aster`, `Nova` dùng [`.agents/skills/github-pr-review-merge/SKILL.md`](.agents/skills/github-pr-review-merge/SKILL.md) để review diff đối chiếu với Design Spec EGM, kiểm tra required CI checks và merge với `expected_head_sha` (theo thẩm quyền [ADR-0010](docs/decisions/ADR-0010-agent-project-and-merge-authority.md)).
- Các quy định về định danh agent và vận hành GitHub/CoS đặt tại [`.agents/agents.yaml`](.agents/agents.yaml), [`.agents/AGENT_OPERATING_STANDARD.md`](.agents/AGENT_OPERATING_STANDARD.md) và [`.agents/GITHUB_WORKFLOW.md`](.agents/GITHUB_WORKFLOW.md).

---

## 2. Ngữ cảnh Hệ thống và Ranh giới Kỹ thuật Bắt buộc

- **Cấu trúc Workspace:** Mã nguồn, build scripts, migrations (`0001`–`0041`) và SDK đặt trong [`cms/`](cms/README.md); tài liệu đặt tại [`docs/`](docs/README.md); dữ liệu vận hành của website (`database/`, `themes/`, `plugins/`, `uploads/`) nằm ở root và tách biệt với mã nguồn. Xem [README.md](README.md) và [Kiến trúc CMS](docs/architecture/CMS_ARCHITECTURE.md).
- **Ràng buộc Sản phẩm ([`PRODUCT_REQUIREMENTS.md`](docs/product/PRODUCT_REQUIREMENTS.md)):**
  - Rust + SQLite, đóng gói thành **một file thực thi duy nhất**, Visual Builder 4 tầng (`Section → Row → Column → Element`) và plugin chạy logic server qua **Wasmi (`=2.0.0`)** trong worker cô lập tiến trình.
  - **Bảo toàn dữ liệu:** Giữ khả năng quản lý CPT, custom fields, taxonomy terms và nội dung khi đổi theme hoặc tắt plugin; bảo toàn bố cục đã chỉnh khi cập nhật theme; giữ thay đổi chưa lưu trong preview và khi gặp lỗi `401`/`412`.
  - **Lưu vs Xuất bản:** Với `post`, `page` và CPT cấu hình `publication: "draft_publish"`, **Lưu** bản `draft` chỉ ghi DB (không công khai); chỉ **Xuất bản** mới đưa nội dung ra URL công khai/archive/sitemap/media ACL. Với Builder (homepage/overrides) và CPT `immediate`, **Lưu** cập nhật trang công khai ngay.
- **Tái sử dụng Base APIs & Phân tầng ([`CORE_FOUNDATION.md`](docs/contracts/CORE_FOUNDATION.md)):**
  - Dùng chung bộ kiểm tra định danh (`opendash_core::id`, `ScopedResourceId`), bộ đăng ký gắn quyền sở hữu (`opendash_core::registry`), điều phối sự kiện (`HookDispatcher`), bộ đọc gói ZIP an toàn (`opendash_server::package`), khóa bảo trì và admission lease (`SiteCoordinator`, `AdmittedMutation` theo [`M8_WRITER_INVENTORY.md`](docs/contracts/M8_WRITER_INVENTORY.md)), guard xác thực/CSRF (`opendash_server::auth`) và kiểu lỗi (`ApiError`).
  - `opendash-core` thuần Rust (`serde`, `serde_json`, `url`), tuyệt đối không chứa I/O, HTTP hay SQL.
  - **100% truy vấn SQLite (`sqlx::query*`, `sqlx::raw_sql`) phải đặt trong `cms/crates/opendash-server/src/storage/`**. Không viết lặp hàm hoặc viết SQL trực tiếp trong `routes/`.
  - Mọi lời gọi HTTP từ React Admin phải đi qua `cms/admin/src/api/client.ts`.
- **Thư viện đã pin ([`TECH_STACK.md`](docs/architecture/TECH_STACK.md)):** Dùng đúng phiên bản thư viện đã pin trong `cms/Cargo.toml` và `cms/admin/package.json` (giữ `cms/Cargo.lock` đồng bộ với `cms/Cargo.toml`). Không thay API bằng phỏng đoán; không tự ý nâng dependency để né vấn đề.

---

## 3. Kiểm chứng, Git và CodeGraph

- **Lệnh kiểm chứng bắt buộc sau khi sửa code hoặc tài liệu:**
  ```powershell
  python cms/scripts/check-standards.py
  python cms/scripts/check-standards.py --self-test
  python cms/scripts/check-plans.py
  python cms/scripts/check-plans.py --self-test
  python .agents/scripts/check-agent-artifacts.py
  cargo fmt --manifest-path cms/Cargo.toml --all --check
  cargo build --manifest-path cms/Cargo.toml --locked
  cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
  git diff --check
  ```
- **Không báo đã build/test/benchmark nếu chưa thực chạy trên mã thật.** Phân biệt rõ trạng thái thiết kế (`Approved` trong `docs/designs/`) với trạng thái đã triển khai (`Implemented`) và đã kiểm chứng (`Verified` có biên bản trong `docs/verification/`).
- **Tra cứu quan hệ mã nguồn với CodeGraph:**
  - Trước khi sửa code hoặc khi Goldfish/Elephant đối chiếu thiết kế với codebase, ưu tiên dùng MCP CodeGraph (`codegraph_symbol_search`, `codegraph_get_callers`, `codegraph_get_callees`, `codegraph_get_call_graph`, `codegraph_analyze_impact`, `codegraph_get_edit_context`, `codegraph_search_docs`) hoặc CLI `node cms/scripts/codegraph-query.mjs`, sau đó luôn đọc file mã nguồn thật để xác minh.
  - Khi cập nhật các tài liệu kiến trúc/contract/design chính, chạy `node cms/scripts/codegraph-index-docs.mjs` để làm mới chỉ mục tài liệu của CodeGraph.
- **Git & Bảo mật:**
  - Dùng đường dẫn Markdown tương đối trong repository.
  - Bảo toàn global Git author identity của người dùng trên máy; nếu dùng worktree identity riêng thì tuân theo [`.agents/GIT_WORKTREE_IDENTITY.md`](.agents/GIT_WORKTREE_IDENTITY.md).
  - Không commit secrets, private keys, dữ liệu runtime hoặc file SQLite cục bộ vào Git.
  - Hồ sơ quy trình G0–G5 và các skill điều phối cũ trong [`docs/archive/`](docs/archive/README.md) và `.agents/archive/` chỉ giữ để tham khảo lịch sử, không kích hoạt lại.
