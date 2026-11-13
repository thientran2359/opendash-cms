# M6 — G3 đặc tả triển khai: Plugin Extensions (10 task / 8 candidate)

**Milestone:** M6 — Plugin Extensions · [GitHub Milestone #7](https://github.com/thientran2359/opendash-cms/milestone/7) · **G2:** AGREED · **G3:** READY (hiệu lực sau khi merge PR G2/G3 lên `main`) · **Implementation/Verification:** NOT_STARTED / NOT_RUN.
**Baseline đối chiếu:** `origin/main@176b46922b6ce961a874d68a804ee74390908bca` (2026-11-13, highest migration `0021_theme_import_journal.sql`) · [ADR-0013 (G2 AGREED)](../../../../decisions/ADR-0013-m6-plugin-extensions.md) · [M6 Contract (`M6_PLUGIN_EXTENSIONS.md`)](../../../../contracts/M6_PLUGIN_EXTENSIONS.md) · [M6 Task Plan](../TASK_PLAN.md) · Nghiên cứu G1/G2 của Atlas: [PR #177](https://github.com/thientran2359/opendash-cms/pull/177) (`70bb477`).
**Quy trình:** [PLANNING_WORKFLOW](../../../PLANNING_WORKFLOW.md) · [SLICE_TASK template](../../../templates/SLICE_TASK.md) · [Module Catalog](../../../MODULE_CATALOG_V1.md) · [PRD](../../../../product/PRODUCT_REQUIREMENTS.md) · [ADR-0010](../../../../decisions/ADR-0010-agent-project-and-merge-authority.md).
**Capability IDs & Primary Acceptance:** Dùng các module trong [Module Catalog](../../../MODULE_CATALOG_V1.md) (`PLG-01`, `PLG-02`, `PLG-03`, `PLG-04`, `CNT-01`, `CNT-02`, `CNT-03`, `BLD-01`, `THM-02`, `SITE-01`, `FND-01`, `FND-05`). `M6-S08-T01` là cross-module E2E verification, không tạo capability trùng.

## Quy tắc phát hành và hàng rào M6

1. **G2 đã chốt đầy đủ (`D01–D10`):** Toàn bộ 10 quyết định mở trong nghiên cứu của Atlas và 4 góp ý bảo mật/biên của Nova trên PR `#177` đã đạt **`G2 AGREED`** trong [ADR-0013](../../../../decisions/ADR-0013-m6-plugin-extensions.md) và [M6 Contract](../../../../contracts/M6_PLUGIN_EXTENSIONS.md).
2. **Thứ tự phát hành Issue:** Merge PR G2/G3 này lên `main` **trước**, lấy immutable merged G3 commit SHA trên `main`, rồi mới tạo GitHub Issues dưới Milestone #7, đồng bộ Project #3 (`Ready` chỉ cho `M6-S01-T01` và các task không xung đột file ownership; các task phụ thuộc đặt `Backlog`) và cập nhật [TASK_INDEX.md](../../../TASK_INDEX.md).
3. **Cấp phát migration tuần tự (highest verified on `main@176b469`: `0021_theme_import_journal.sql`):** Các migration `0001`–`0021` là **bất biến**. Phân bổ M6 tuần tự: `0022` (`M6-S01-T01`), `0023` (`M6-S02-T01`), `0024` (`M6-S05-T01`). Implementer phải kiểm tra `origin/main` tại thời điểm claim để xác nhận số migration kế tiếp chưa bị chiếm.
4. **Một writer trên mỗi seam dùng chung:** Các seam `opendash-core/src/plugin.rs`, `opendash-core/src/registry.rs`, `opendash-core/src/content.rs`, `storage/plugin.rs`, `storage/content.rs`, `storage/entries.rs`, `plugin/runtime.rs`, `plugin/worker.rs`, `plugin/host_ops.rs`, `routes/plugins.rs`, `routes/entries.rs`, `cms/admin/src/features/plugins/` chỉ có **một** task sở hữu tại mỗi lượt.
5. **Rollback DB+media:** Không dùng `git revert` để rollback migration SQLite; luôn dùng site snapshot (DB + `plugins/revisions` + `themes/revisions` + `uploads/media`) cùng binary tương ứng.

## Ma trận coverage và đường găng

| Candidate | G3 task | Owner dự kiến | Migration | Dependency |
| --- | --- | --- | --- | --- |
| M6-P08 / P06 base | M6-S01-T01 | Forge | `0022` | G2 AGREED (`main@176b469`) — **có thể Ready ngay** |
| M6-P08 | M6-S02-T01 | Harbor | `0023` | M6-S01-T01 G5 (hoặc song song nếu tách seam `storage/content.rs` & `core/content.rs` trước khi merge) |
| M6-P04 | M6-S02-T02 | Harbor | — | M6-S02-T01 G5 |
| M6-P01 | M6-S03-T01 | Forge | — | M6-S01-T01 G5, M6-S02-T02 G5 |
| M6-P01 / P06 base | M6-S03-T02 | Forge | — | M6-S03-T01 G5 |
| M6-P02 | M6-S04-T01 | Forge | — | M6-S01-T01 G5 (sau handoff `plugin/host_ops.rs` từ `S03-T02` hoặc trước `S03-T02`) |
| M6-P03 | M6-S05-T01 | Harbor | `0024` | M6-S01-T01 G5, M6-S02-T01 G5 |
| M6-P05 | M6-S06-T01 | Harbor | — | M6-S03-T02 G5, M6-S05-T01 G5 |
| M6-P06 | M6-S07-T01 | Nova / Forge | — | M6-S03-T02 G5, M6-S04-T01 G5, M6-S06-T01 G5 |
| M6-P07 | M6-S08-T01 | Forge (Win) + Harbor (Mac) | — | M6-S07-T01 và toàn bộ M6 predecessors G5 |

**Critical path:** `S01-T01 → S02-T01 → S02-T02 → S03-T01 → S03-T02 → S06-T01 → S07-T01 → S08-T01`. Nhánh song song hợp lệ sau `S01-T01`: `S04-T01` (egress, theo lượt bàn giao `plugin/host_ops.rs`) và sau `S02-T01`: `S05-T01` (domain bridge).

---

## M6-S01-T01 — Plugin package revisions, `revision_generation`/CAS và staging durability

- **Thuộc:** V1 · M6 · M6-S01 · M6-P08 / M6-P06 base
- **Primary Acceptance Slice của capability:** M6-S01 (`PLG-01.2` revision & lifecycle lease)
- **Module:** PLG-01 (phối hợp PLG-03)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** READY (không có M6 predecessor)
- **Nguồn quyết định:** [M6 Contract §1](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#1-package-revisions-capabilities--isolation-d01-d02-p06); [ADR-0013 (`D01`, `D02`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Forge / Self-review hoặc Nova review theo ADR-0010.

### Phạm vi công việc
- Forward migration `cms/migrations/0022_plugin_revisions.sql`: thêm bảng `plugin_revisions` (`revision_id`, `plugin_id`, `version`, `revision_digest`, `manifest_schema`, `manifest_json`, `created_at`), bổ sung cột `active_revision_id`, `revision_generation` (monotonic integer) và `cas_version` vào `plugins`.
- Áp dụng giao thức staging `A-DURABILITY` trong `storage/plugin.rs` & `package.rs`: `plugins/.staging/<run_uuid>/` -> `sync_all()` + drop handles -> rename sang `plugins/revisions/<revision_digest>/` + `.complete` marker, rồi mới commit `BEGIN IMMEDIATE` (atomic auto-enable cho bản cài hợp lệ). Startup quarantine `.staging/*` mồ côi.
- Enforce `manifest_schema: 2`, `min_host_version`, và fail-closed khi gặp capability không xác định hoặc thiếu `.complete`.
- Disable/update fencing: tăng/vô hiệu hóa `revision_generation` trong SQLite trước khi hủy worker permit; kiểm tra `(plugin_id, enabled = 1, revision_generation)` bên trong mọi write tx từ plugin.

### Ngoài phạm vi
Chưa triển khai CNT bridge (`S02-T01`), `ContentBeforeSave` (`S03-T01`), hoặc egress v2 (`S04-T01`).

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/migrations/0022_plugin_revisions.sql`, `cms/crates/opendash-core/src/plugin.rs`, `cms/crates/opendash-server/src/package.rs`, `cms/crates/opendash-server/src/storage/plugin.rs`, `cms/crates/opendash-server/src/plugin/manager.rs`, test `cms/crates/opendash-server/tests/m6_s01_t01_plugin_revisions.rs`.

### Tiêu chí nghiệm thu
- Cài đặt/cập nhật/disable/re-enable plugin qua crash staging, startup cleanup, trùng `(plugin_id, version)` khác digest (`409`), CAS mismatch (`412`/`428`), và từ chối IPC commit lệch `revision_generation`.

---

## M6-S02-T01 — Owner-neutral CNT bridge & plugin CPT/field declarations (`M6-P08`)

- **Thuộc:** V1 · M6 · M6-S02 · M6-P08
- **Primary Acceptance Slice của capability:** M6-S02 (`PLG-03.1` CPT/field provider bridge)
- **Module:** PLG-03 (phối hợp CNT-01/02)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S01-T01` G5)
- **Nguồn quyết định:** [M6 Contract §2](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#2-owner-neutral-cnt--taxonomy-bridge-p08-p04-d03-d08); [ADR-0013 (`D03`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Harbor / Self-review hoặc Nova review theo ADR-0010.

### Phạm vi công việc
- Forward migration `cms/migrations/0023_owner_neutral_cnt_bridge.sql`: tổng quát hóa bảng khai báo CNT sang `owner_kind` (`site` | `theme` | `plugin`) + `owner_id` (bảo toàn dữ liệu `theme_cnt_declarations` của M5 không làm hỏng đổi theme).
- Trong `storage/content.rs` và `opendash-core/src/registry.rs`: cho phép plugin khai báo CPT và custom fields gắn quyền `ResourceOwner::Plugin(plugin_id)` trong transaction cài/enable; chặn đè `post`/`page` hoặc CPT của owner khác (`409`); khi plugin disable/uninstall chỉ đánh dấu `provider_inactive`/`withdrawn`, giữ nguyên schema/entries/values và Editor CRUD.

### Ngoài phạm vi
Không triển khai taxonomy eligibility (`S02-T02`) hay save hooks (`S03-T01`).

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/migrations/0023_owner_neutral_cnt_bridge.sql`, `cms/crates/opendash-core/src/registry.rs`, `cms/crates/opendash-core/src/content.rs`, `cms/crates/opendash-server/src/storage/content.rs`, `cms/crates/opendash-server/src/routes/content.rs`, test `cms/crates/opendash-server/tests/m6_s02_t01_plugin_cnt_bridge.rs`.

### Tiêu chí nghiệm thu
- Plugin khai báo CPT + fields; đổi theme không ảnh hưởng declaration của plugin; tắt/xóa plugin vẫn giữ entries và Editor CRUD; xung đột key hoặc nâng cấp incompatible schema bị chặn (`409`).

---

## M6-S02-T02 — Taxonomy declaration & eligibility lifecycle (`M6-P04`)

- **Thuộc:** V1 · M6 · M6-S02 · M6-P04
- **Primary Acceptance Slice của capability:** M6-S02 (`PLG-03.2` taxonomy eligibility lifecycle)
- **Module:** PLG-03 (phối hợp CNT-03)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S02-T01` G5)
- **Nguồn quyết định:** [M6 Contract §2](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#2-owner-neutral-cnt--taxonomy-bridge-p08-p04-d03-d08); [ADR-0013 (`D08`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Harbor / Self-review hoặc Nova review.

### Phạm vi công việc
- Cho phép plugin khai báo eligibility cho `category` và `tag` (chỉ 2 kind lõi) trên các CPT do chính plugin sở hữu trong transaction declaration. Chặn plugin nhận quyền trên `post`/`page` hoặc CPT của owner khác.
- Khi provider `provider_inactive`/`withdrawn`: giữ nguyên `entry_terms`, Editor/Admin vẫn sửa được gắn kết category/tag hợp lệ đã tồn tại; re-enable dùng lại canonical IDs, không nhân bản term và không hồi sinh eligibility đã bị rút.

### Ngoài phạm vi
Không thêm custom taxonomy kinds ngoài `category`/`tag`.

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-server/src/storage/taxonomy.rs`, `cms/crates/opendash-server/src/storage/content.rs`, `cms/crates/opendash-server/src/routes/taxonomy.rs`, test `cms/crates/opendash-server/tests/m6_s02_t02_plugin_taxonomy.rs`.

### Tiêu chí nghiệm thu
- Khai báo `category`/`tag` trên plugin CPT, bảo toàn `entry_terms` qua disable/restart/reinstall, chặn claim `post`/`page`, không tạo duplicate owner hoặc leak bài draft qua archive taxonomy.

---

## M6-S03-T01 — `ContentBeforeSave` validator/veto & `ContentAfterSave` (`M6-P01`)

- **Thuộc:** V1 · M6 · M6-S03 · M6-P01
- **Primary Acceptance Slice của capability:** M6-S03 (`PLG-02.1` content save hooks)
- **Module:** PLG-02 (phối hợp CNT-04/FND-01)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S01-T01` và `M6-S02-T02` G5)
- **Nguồn quyết định:** [M6 Contract §3](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#3-contentbeforesave-contentaftersave--actions-p01-d04-d05); [ADR-0013 (`D04`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Forge / Nova review hoặc self-review.

### Phạm vi công việc
- Tích hợp `ContentBeforeSave` (pure bounded validator/veto, tổng deadline pha `5s`, tối đa 32 handler theo thứ tự `(priority, id)`, chỉ nhận candidate khi plugin có capability đọc type đó; cấm HTTP, KV writes, content writes, nested dispatch) vào luồng create/update entry.
- Trong SQLite write tx sau `Allow`: re-verify actor session/role, entry CAS (`412`), schema/provider generation, **slug uniqueness, reserved-root collision, media existence/visibility, và term validity/eligibility**.
- Lỗi veto trả **raw UTF-8 plain text** đã giới hạn độ dài & lọc control char (React tự render text node); trap/timeout/invalid output fail closed với 0 ghi DB/KV.
- Tích hợp `ContentAfterSave` sau khi commit bền vững (read/diagnostic only, `5s` total cap, lỗi sau commit không revert kết quả Save).

### Ngoài phạm vi
Chưa triển khai standalone `host_call` actions (`S03-T02`); không gắn Save hooks vào Publish/Unpublish/Delete.

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-core/src/hooks.rs`, `cms/crates/opendash-server/src/storage/entries.rs`, `cms/crates/opendash-server/src/routes/entries.rs`, `cms/crates/opendash-server/src/plugin/runtime.rs`, `cms/crates/opendash-server/src/plugin/worker.rs`, test `cms/crates/opendash-server/tests/m6_s03_t01_content_hooks.rs`.

### Tiêu chí nghiệm thu
- Thứ tự `(priority, id)` tất định, veto dừng ngay với 0 DB/KV writes, TOCTOU slug/media/term recheck trong tx, `ContentAfterSave` fail không làm hỏng Save đã commit, không có late writes sau timeout.

---

## M6-S03-T02 — Standalone `host_call` actions, RBAC/generation fencing & lease cleanup (`M6-P01` / `M6-P06` base)

- **Thuộc:** V1 · M6 · M6-S03 · M6-P01 / M6-P06
- **Primary Acceptance Slice của capability:** M6-S03 (`PLG-02.2` guarded server actions)
- **Module:** PLG-02 (phối hợp PLG-01)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S03-T01` G5)
- **Nguồn quyết định:** [M6 Contract §3 & §1](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#3-contentbeforesave-contentaftersave--actions-p01-d04-d05); [ADR-0013 (`D05`, `D02`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Forge / Nova review hoặc self-review.

### Phạm vi công việc
- Đăng ký và thực thi authenticated standalone actions qua `host_call` JSON (allowlist named ops, default deny unknown ops, không SQL trực tiếp, không HTTP loopback tới Admin).
- Từ chối mixed KV + content writes ở V1; single content command đi qua canonical Save orchestration; giao capability của plugin với RBAC hiện hành của actor (`actor RBAC ∩ plugin capabilities`) và recheck cùng `enabled` + `revision_generation` bên trong SQLite write tx trước khi commit.
- Cấm đệ quy self-dispatch trong cùng chuỗi action.

### Ngoài phạm vi
Không tạo public unauthenticated plugin route hay scheduler/cron.

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-server/src/plugin/host_ops.rs`, `cms/crates/opendash-server/src/plugin/manager.rs`, `cms/crates/opendash-server/src/routes/plugins.rs`, test `cms/crates/opendash-server/tests/m6_s03_t02_host_actions.rs`.

### Tiêu chí nghiệm thu
- Action RBAC intersection cả lúc admission và commit, chặn mixed KV+content, chặn nested recursive dispatch, thu hồi role hoặc disable plugin giữa chừng không thể commit.

---

## M6-S04-T01 — Scoped external egress v2, pinned DNS & chunked cap (`M6-P02`)

- **Thuộc:** V1 · M6 · M6-S04 · M6-P02
- **Primary Acceptance Slice của capability:** M6-S04 (`PLG-02.3` capability-enforced egress)
- **Module:** PLG-02 (phối hợp PLG-01)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S01-T01` G5 và bàn giao `plugin/host_ops.rs` từ `S03-T02`)
- **Nguồn quyết định:** [M6 Contract §4](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#4-external-egress--ssrf-pinning-p02-d06); [ADR-0013 (`D06`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Forge / Nova review hoặc self-review.

### Phạm vi công việc
- Giữ tương thích v1 origin grants (ghi nhãn legacy), bổ sung v2 network scope `(scheme, host, effective_port, methods[], path_prefix?)` mặc định HTTPS, chặn credentials/fragments/encoded path bypass/`Host`/hop-by-hop/CRLF headers.
- Triển khai custom `reqwest::dns::Resolve` từ chối toàn bộ tập DNS nếu *bất kỳ* IP nào không public (loopback, RFC1918, `169.254.0.0/16`, `100.64.0.0/10`, `::1`, `fc00::/7`, `fe80::/10`, `::ffff:0:0/96`, `64:ff9b::/96`, multicast/unspecified) và ghim `SocketAddr` đã kiểm cho kết nối; bật `.no_proxy()` và `.redirect(Policy::none())`.
- Giới hạn `2s`/request, `10` HTTP requests/invocation, `1 MiB` request body, và `1 MiB` response body kiểm tra **tăng dần theo từng chunk đang stream**; redact `Authorization`/`Cookie`/`Set-Cookie` khỏi logs và UI diagnostics.

### Ngoài phạm vi
Không dùng proxy env fallback; không bật approval thủ công (quyền hợp lệ được hiển thị và tự động áp dụng).

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-core/src/plugin.rs` (network scope policy), `cms/crates/opendash-server/src/plugin/egress.rs` (mới), `cms/crates/opendash-server/src/plugin/host_ops.rs`, `cms/crates/opendash-server/src/plugin/mod.rs`, test `cms/crates/opendash-server/tests/m6_s04_t01_egress_ssrf.rs`.

### Tiêu chí nghiệm thu
- Test-only mock/loopback harness kiểm chứng: allow trong scope, chặn sibling prefix/encoded path/wrong port, chặn private IPv4/IPv6/mapped/NAT64/mixed DNS, chặn redirect & proxy env, chặn chunked >1 MiB, sub-quota 11th HTTP call, và redact secret logs.

---

## M6-S05-T01 — Domain owner-neutral blocks, templates & settings (`M6-P03`)

- **Thuộc:** V1 · M6 · M6-S05 · M6-P03
- **Primary Acceptance Slice của capability:** M6-S05 (`PLG-03.3` domain-owner bridge)
- **Module:** PLG-03 (phối hợp BLD-01/02, THM-02, SITE-01)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S01-T01` và `M6-S02-T01` G5)
- **Nguồn quyết định:** [M6 Contract §5](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#5-domain-owner-neutral-blocks-templates--settings-p03-d07); [ADR-0013 (`D07`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Harbor / Self-review hoặc Nova review.

### Phạm vi công việc
- Forward migration `cms/migrations/0024_owner_neutral_domain_bridge.sql`: tổng quát hóa settings/template selection metadata với `owner_kind` (`theme` | `plugin`) + `owner_id` qua canonical `SITE-01`/Builder/Theme owners (không tạo bảng registry plugin tách rời).
- Cho phép plugin khai báo Builder blocks (MiniJinja `C-SEC` safe-subset), template selection cho CPT của mình, và typed settings; tắt plugin giữ nguyên user AST nodes & giá trị settings với fallback an toàn.

### Ngoài phạm vi
Không cho phép plugin xuất raw HTML không qua `C-SEC`.

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/migrations/0024_owner_neutral_domain_bridge.sql`, `cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/storage/builder.rs`, `cms/crates/opendash-server/src/storage/settings.rs`, `cms/crates/opendash-server/src/storage/theme.rs`, `cms/crates/opendash-server/src/page.rs`, test `cms/crates/opendash-server/tests/m6_s05_t01_domain_bridge.rs`.

### Tiêu chí nghiệm thu
- Cài plugin đăng ký block + template + settings; Preview không ghi DB; Save cập nhật public; tắt/xóa plugin giữ AST và settings, public fallback an toàn không lỗi 500; chặn XSS/C-SEC bypass.

---

## M6-S06-T01 — Declarative Admin UI schema v1 & guarded actions (`M6-P05`)

- **Thuộc:** V1 · M6 · M6-S06 · M6-P05
- **Primary Acceptance Slice của capability:** M6-S06 (`PLG-04.1` declarative Admin UI)
- **Module:** PLG-04 (phối hợp FND-05, PLG-02)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S03-T02` và `M6-S05-T01` G5)
- **Nguồn quyết định:** [M6 Contract §6](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#6-declarative-admin-ui--guarded-actions-p05-d09); [ADR-0013 (`D09`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Harbor / Self-review hoặc Nova review.

### Phạm vi công việc
- Backend schema endpoint + action dispatch route qua RBAC/CSRF/Origin; chuẩn hóa schema v1 (`form`, `table`, `list`, `tabs`, `button`; depth `<=8`, nodes `<=200`, screens `<=20`, page `<=100`).
- Frontend `cms/admin/src/features/plugins/` render declarative UI tái sử dụng `FormPrimitives`, giữ bản nháp trong bộ nhớ theo key `(plugin_id, screen_id, form_id, object_id, schema_version)` qua chuyển tab, `401` re-login (không tự submit lại), `403`/`412`/`422`, và timeout.
- Hiển thị danh sách quyền hợp lệ tự động áp dụng khi cài/bật plugin (không có bước approval thủ công).

### Ngoài phạm vi
Không cho phép HTML/JS/CSS tùy ý hay secret input field trong schema v1.

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-server/src/routes/plugins.rs`, `cms/admin/src/features/plugins/` (declarative renderer + tests), `cms/admin/src/App.tsx` (route seam), test `cms/crates/opendash-server/tests/m6_s06_t01_declarative_ui.rs`.

### Tiêu chí nghiệm thu
- API + UI RBAC Admin/Editor, CSRF/Origin (`403`), forged action/schema (`400`/`404`), double-click race, `401` re-auth giữ draft không auto-submit, schema quota/XSS bị chặn.

---

## M6-S07-T01 — Sandbox, SSRF, permission & failure isolation adversarial regression (`M6-P06`)

- **Thuộc:** V1 · M6 · M6-S07 · M6-P06
- **Primary Acceptance Slice của capability:** M6-S07 (`PLG-01.3` sandbox & isolation verification)
- **Module:** PLG-01 / PLG-02
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S03-T02`, `M6-S04-T01`, `M6-S06-T01` G5)
- **Nguồn quyết định:** [M6 Contract §1, §3, §4](../../../../contracts/M6_PLUGIN_EXTENSIONS.md); [ADR-0013](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Nova / Forge.

### Phạm vi công việc
- Xây dựng bộ test đối kháng hợp nhất kiểm tra: plugin trap/timeout/OOM/fuel exhaustion, disabled plugin không bao giờ thực thi, lỗi plugin không làm sập Admin/Public, SSRF/DNS rebinding/chunked overflow, late IPC write sau cancel/disable, và redaction secret khỏi logs/UI.

### Ngoài phạm vi
Không sửa tính năng mới ngoài vá lỗi phát hiện bởi bộ regression (nếu có).

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-server/tests/m6_s07_t01_isolation_security.rs`.

### Tiêu chí nghiệm thu
- 100% test case đối kháng pass, không còn late write khi đợi quá deadline cũ và mở lại DB, Admin/Public vẫn phục hồi và hoạt động bình thường.

---

## M6-S08-T01 — E2E Plugin ZIP xuyên Content/Theme/Builder & G5 đa OS (`M6-P07`)

- **Thuộc:** V1 · M6 · M6-S08 · M6-P07
- **Primary Acceptance Slice:** Cross-module verification (không tạo capability trùng)
- **Module:** PLG-03 / PLG-04 (phối hợp CNT-01–03, BLD-01, THM-02)
- **Track:** Full-Track · **G2:** AGREED · **G3:** READY
- **Trạng thái:** Backlog (chờ `M6-S07-T01` và toàn bộ M6 predecessors G5)
- **Nguồn quyết định:** [M6 Contract §7](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#7-cross-module-e2e--cross-os-acceptance-p07-d10); [ADR-0013 (`D10`)](../../../../decisions/ADR-0013-m6-plugin-extensions.md).
- **Người triển khai / review:** Forge (Windows) + Harbor (macOS) / Atlas hoặc Nova review.

### Phạm vi công việc
- Chạy E2E từ một source SHA chung trên **cả Windows và macOS bản địa**:
  - Install Plugin ZIP v2 -> đăng ký CPT + fields + category/tag + Builder block + template + settings + declarative UI;
  - Chỉnh sửa & Save (cùng `ContentBeforeSave` veto/allow, `host_call` action, scoped egress mock);
  - Disable -> restart server -> verify dữ liệu/entries/layout/settings vẫn còn và Editor quản trị được -> re-enable/reinstall;
  - Snapshot & rollback bằng binary tương ứng.
- Ghi riêng hai báo cáo `docs/verification/YYYY-MM-DD-m6-g5-windows.md` và `docs/verification/YYYY-MM-DD-m6-g5-macos.md`.

### Ngoài phạm vi
Không đóng Milestone #7 nếu thiếu báo cáo thực chạy bản địa của một trong hai OS.

### Kỹ thuật và file ownership
- **File sở hữu:** `cms/crates/opendash-server/tests/m6_s08_t01_plugin_e2e.rs`, `docs/verification/YYYY-MM-DD-m6-g5-windows.md` (Windows runner), `docs/verification/YYYY-MM-DD-m6-g5-macos.md` (Harbor macOS runner).

### Tiêu chí nghiệm thu
- Cả Windows và macOS đều có kết quả `PASS` trên cùng source SHA, xác nhận bảo toàn dữ liệu và rollback.
