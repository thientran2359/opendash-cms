# Làm việc trong OpenDash

## Ngữ cảnh và nguồn quyết định

- `/opendash-cms` đặt mã nguồn, build scripts và SDK trong `cms/`, tài liệu tại `docs/`; `database/`, `themes/`, `plugins/`, `uploads/` thuộc website và nằm ở root. M1/M2 và migration/fixture lịch sử đã được nhập. Đối chiếu [báo cáo tại đích](docs/verification/2026-11-07-m1-m2-import.md) trước khi mở rộng phạm vi. Kiểm tra file thực tế trước khi tuyên bố chức năng đã có. Xem [README](README.md) và [verification](docs/verification/README.md).
- Đọc [README.md](README.md) và [docs/product/PRODUCT_REQUIREMENTS.md](docs/product/PRODUCT_REQUIREMENTS.md) trước khi đề xuất tech stack hoặc thay đổi kiến trúc/code.
- Dự án phát triển trước trên Windows. Lát cắt Rust và cách kiểm chứng được ghi trong [README.md](README.md). Không báo đã build/test/benchmark nếu chưa thực hiện được trên mã thật.
- Giữ các ràng buộc đã xác nhận trong [PRODUCT_REQUIREMENTS.md](docs/product/PRODUCT_REQUIREMENTS.md), bao gồm Rust + SQLite, một file thực thi, builder và plugin chạy logic server từ bản đầu. Hướng stack và Wasmi đã được chủ dự án chốt trong [docs/architecture/TECH_STACK.md](docs/architecture/TECH_STACK.md); cấu trúc dự án theo [docs/architecture/CMS_ARCHITECTURE.md](docs/architecture/CMS_ARCHITECTURE.md), bộ khung lõi và quy chuẩn code theo [docs/contracts/CORE_FOUNDATION.md](docs/contracts/CORE_FOUNDATION.md) và hợp đồng ABI theo [docs/contracts/PLUGIN_ABI.md](docs/contracts/PLUGIN_ABI.md).
- Chỉ đạo mới của chủ dự án và [PRODUCT_REQUIREMENTS.md](docs/product/PRODUCT_REQUIREMENTS.md) có ưu tiên cao nhất. Theo [ADR-0003](docs/decisions/ADR-0003-agent-consensus-governance.md), các agent **tự chốt quyết định kỹ thuật bằng consensus có bằng chứng**; khi cần phân xử thì reviewer độc lập (ưu tiên Atlas/Nova) quyết định cuối. Owner theo dõi tiến độ, không là cổng ký duyệt mặc định. Consensus khác `IMPLEMENTED`/`VERIFIED` và không được hạ chuẩn bảo mật.

## Ranh giới kỹ thuật

- Khi đánh giá phương án, giữ khả năng quản lý CPT/fields/nội dung khi đổi theme hoặc tắt plugin, bảo toàn bố cục đã chỉnh khi cập nhật theme và giữ thay đổi chưa lưu trong preview. Luồng bản đầu cập nhật website công khai khi bấm Lưu.
- So sánh runtime plugin theo khả năng cài ZIP, giới hạn quyền đã khai báo, dừng tác vụ lỗi hoặc chạy quá lâu và phục hồi qua Admin.
- Xem mô hình AST, projection, artifact và cache trong [docs/architecture/CMS_PROPOSAL.md](docs/architecture/CMS_PROPOSAL.md) là các phương án cần đối chiếu chọn lọc theo [docs/architecture/CMS_ARCHITECTURE.md](docs/architecture/CMS_ARCHITECTURE.md). Nếu đánh giá template động, kiểm tra ranh giới giữa mã template và dữ liệu người dùng, cùng xử lý URL/CSS/rich text theo ngữ cảnh.
- Dùng thư viện và API theo phiên bản đã pin; tra docs chính thức hoặc Context7 nếu có coverage. Không thay API bằng phỏng đoán; không tự nâng dependency để né vấn đề.
- Tuân thủ quy chuẩn code và nguyên tắc tái sử dụng Base APIs trong [docs/contracts/CORE_FOUNDATION.md](docs/contracts/CORE_FOUNDATION.md): dùng chung bộ kiểm tra định danh (`opendash_core::id`), bộ đăng ký gắn quyền sở hữu (`opendash_core::registry`), điều phối sự kiện (`HookDispatcher`), bộ đọc gói ZIP an toàn (`opendash_server::package`), guard xác thực/CSRF (`opendash_server::auth`), kiểu lỗi (`ApiError`) và đặt toàn bộ truy vấn SQLite trong `opendash_server::storage`. Không viết lặp hàm hoặc viết SQL trực tiếp trong `routes/`.

## Tài liệu và kiểm chứng

- Trước khi làm một slice: nêu contract liên quan, tiêu chí nghiệm thu và phần ngoài phạm vi. Sau khi làm: ghi thay đổi, lệnh/check thực chạy, kết quả và giới hạn.
- Với thay đổi hành vi, chọn kiểm chứng theo phạm vi: preview chưa lưu, cập nhật khi Lưu, dữ liệu và bố cục khi đổi theme, quyền và lỗi plugin, hoặc backup/restore. Không tạo test chỉ lặp lại implementation.
- Trạng thái thiết kế `AGREED` khác `implemented` và `verified`. Giữ các trạng thái này riêng; không đánh dấu hoàn thành khi thiếu fixtures/bằng chứng thực tế.
- Tài liệu mới đặt đúng nhóm theo [docs/README.md](docs/README.md); dùng [Planning Workflow V1](docs/plans/PLANNING_WORKFLOW.md) và [Module Catalog](docs/plans/MODULE_CATALOG_V1.md) khi chia milestone/slice/task. Quy trình Living Module Brief G0–G3, Full-Track/Fast-Track, Primary Acceptance Slice, Stop-and-Propose đã được thống nhất; **đồng thuận về planning không chốt schema M3**. Kiến trúc có ADR, bằng chứng thực chạy ở `docs/verification/`.
- Dùng đường dẫn Markdown tương đối trong repo. Sau khi đổi docs/plans, chạy `python cms/scripts/check-plans.py` và `python cms/scripts/check-plans.py --self-test` cùng `git diff --check`. Checker kiểm nhất quán link/ID/evidence, không thay thế review thiết kế hoặc test thực thi.
- Ghi lệnh chạy chính xác trong [README.md](README.md) và giữ `cms/Cargo.lock` đồng bộ với `cms/Cargo.toml`. Từ root, dùng `cargo build --manifest-path cms/Cargo.toml --locked` để kiểm tra build lặp lại được; từ `cms/` có thể dùng `cargo build --locked`.

## Multi-agent collaboration and archived skills

- Atlas (Codex Win), Nova (Antigravity Win), Forge (CoS Win), Harbor (CoS
  macOS) and Orion (Antigravity macOS) work in separate local folders, on
  `agent/<name>` branches of the same `thientran2359/opendash-cms` Git remote.
- Draft technical research, discussions and task handoffs belong in `.agents/`
  (see `.agents/README.md` and `.agents/agents.yaml`). **Agent-concluded and peer-reviewed**
  decisions, plans/contracts and actual verification belong in `docs/` after PR merge.
  Keep existing historical references and their source evidence. Use [ADR-0003](docs/decisions/ADR-0003-agent-consensus-governance.md).
- `.agents/skills/` contains three **manual chat skills**:
  [github-task-pickup](.agents/skills/github-task-pickup/SKILL.md) when asked to fetch/perform one's GitHub Issue,
  [discussion-turn-responder](.agents/skills/discussion-turn-responder/SKILL.md) when asked to check and answer one's discussion turns,
  and [github-pr-review-merge](.agents/skills/github-pr-review-merge/SKILL.md) for Atlas/Nova reviewing and merging other agents' eligible implementation PRs.
  Read the corresponding SKILL.md on request even if the host does not auto-discover repo skills.
  No skill starts any background process, cronjob or automatic CoS dispatch.
  `.agents/archive/skills/` contains retired skills for reference ONLY.
  Do not invoke archived handoff scripts, PowerShell Stop hooks or the previous
  opendash-orchestrator as if they were active automation.
- GitHub Issues are the source of truth for active task state, and PRs record
  changes and review. Follow [the GitHub workflow](.agents/GITHUB_WORKFLOW.md);
  `.agents/tasks/` must not become a competing live queue.
- Keep the user's Git global author identity unchanged on every machine.
  Before an agent commits, follow [per-worktree Git identity setup](.agents/GIT_WORKTREE_IDENTITY.md)
  and retain the `Agent: <Agent>` commit trailer. Worktree-specific identity
  is local machine configuration, not a separate authenticated GitHub account.
- Follow the shared [Agent Operating Standard](.agents/AGENT_OPERATING_STANDARD.md)
  directly **within the current agent/chat**: confirm workspace/branch, fetch
  remote Git refs without overwriting work, consult GitHub Connector, and
  enforce task/PR ownership. **Do not create a separate Agent Preflight Check
  agent, skill, script or background process.** For authenticated GitHub UI
  on CoS, use **Desktop Browser** attached to the user's already signed-in
  Chrome session, rather than the isolated Plugins Browser profile.
- For cross-agent document/design reviews, the reviewer records actionable
  findings on the GitHub PR; the PR author owns fixes, commits and non-force
  pushes on their branch, and the reviewer rechecks the updated PR. If GitHub
  comments are unavailable, relay findings to the author to record on the PR.
  A reviewer edits files only with explicit file/branch ownership reassignment.
- Branch names in `.agents/agents.yaml` describe desired agent identities,
  not proof that remote branches or AI sessions are configured.
- Assign one responsible implementer and one coordinator per task, with issue
  ID, base SHA, exact file ownership, explicit acceptance and approved gate.
  Keep one active implementation task per agent branch. Protect uncommitted
  changes and avoid two writers on the same file.
- After actual validation, stage only task-owned changes, commit with task ID
  and non-force push to its agent branch. Link commit and tests to GitHub
  Issue/PR for review. No background merge or deployment. Atlas/Nova may merge
  independently reviewed **implementation and concluded technical-document/design** PRs
  in the explicitly invoked review-and-merge flow in .agents/GITHUB_WORKFLOW.md;
  review-only stays read-only for code, and authors cannot self-review/self-merge.
  Existing security blockers, CI and preservation gates are not waived.
- Muse-to-CoS/Antigravity dispatch must be verified through authenticated
  host-specific adapters first. Chat delivery or AI consensus does not
  establish completion or VERIFIED evidence. **Reviewed Final Decision + merged docs PR**
  is the technical-contract authority under ADR-0003, not chat consensus alone.
- No infinite agent loops, blind retries of ambiguous deliveries, or secrets
  and private runtime state in Git.
- The owner has **paused the Python Task Watcher / cronjob plan** in favor of
  on-demand GitHub issue pickup and discussion replies. TW proposals do not
  authorize running watcher/PONG or bypassing Project Ready and security gates.

## Tra cứu quan hệ mã nguồn với CodeGraph

- Khi cần hiểu một hàm gọi những hàm nào, ai gọi hàm đó, dependency giữa file hoặc phạm vi ảnh hưởng trước khi sửa code, ưu tiên MCP CodeGraph (`codegraph_symbol_search`, `codegraph_get_callers`, `codegraph_get_callees`, `codegraph_get_call_graph`, `codegraph_analyze_impact`, `codegraph_get_edit_context`). Tìm symbol trước rồi dùng `uri` + `line` từ kết quả để truy vấn graph.
- Khi đối chiếu implementation với quyết định/contract, tìm trong sáu tài liệu đã nạp vào kho docs của CodeGraph bằng `codegraph_search_docs` và kiểm tra trực tiếp tài liệu nguồn. Khi các tài liệu thay đổi, chạy `node cms/scripts/codegraph-index-docs.mjs` để cập nhật chỉ mục docs.
- CodeGraph là chỉ mục phân tích tĩnh có thể bỏ sót lời gọi động, macro, hook hoặc mã không parse trọn vẹn. Luôn xác minh kết quả quan trọng bằng code thật và kiểm chứng phù hợp; nếu MCP không được kết nối thì dùng `rg`, đọc mã nguồn và tài liệu như bình thường. Xem [bằng chứng cài đặt](docs/verification/2026-11-07-codegraph-setup.md).
- Khi Chat On Steroids chưa xuất hiện các MCP tool `codegraph_*`, dùng CLI tương đương qua Core: `node cms/scripts/codegraph-query.mjs symbol HookDispatcher`, `node cms/scripts/codegraph-query.mjs callgraph cms/crates/opendash-server/src/plugin/host_ops.rs 94`, hoặc `node cms/scripts/codegraph-query.mjs docs "plugin ABI"`. Sau khi tra graph, kiểm tra các file thật trước khi sửa.
