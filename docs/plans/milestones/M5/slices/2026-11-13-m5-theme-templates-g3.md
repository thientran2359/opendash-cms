# M5 — G3 đặc tả triển khai: Theme & Templates (13 task / 10 candidate)

**Milestone:** M5 — Theme & Templates · [GitHub Milestone #6](https://github.com/thientran2359/opendash-cms/milestone/6) · **G2:** AGREED · **G3:** READY (đặc tả sau merge PR này) · **Implementation/Verification:** NOT_STARTED / NOT_RUN.
**Baseline đối chiếu:** `origin/main@18690572c32cd72fe96ab57fa582d3dd97fe4a5a` (2026-11-13) · [ADR-0012 (G2 AGREED)](../../../../decisions/ADR-0012-m5-theme-templates.md) · [M5 Contract (`M5_THEME_TEMPLATES.md`)](../../../../contracts/M5_THEME_TEMPLATES.md) · [M4 Release Handoff `a176414`](../../../../verification/2026-11-11-m4-release.md) · [M5 Task Plan](../TASK_PLAN.md) · Bản nháp lịch sử trước rebaseline: PR #135 / research `a09173a28910f3d3e7372d43999f18e37fabe579`.
**Quy trình:** [PLANNING_WORKFLOW](../../../PLANNING_WORKFLOW.md) · [SLICE_TASK template](../../../templates/SLICE_TASK.md) · [Module Catalog](../../../MODULE_CATALOG_V1.md) · [PRD](../../../../product/PRODUCT_REQUIREMENTS.md) · [ADR-0010](../../../../decisions/ADR-0010-agent-project-and-merge-authority.md).
**Capability IDs & Primary Acceptance:** Dùng các module trong [Module Catalog](../../../MODULE_CATALOG_V1.md) (`THM-01`, `THM-02`, `THM-03`, `SITE-01`, `CNT-04`) và giữ các capability ID M5 (`THM-01.2/.3`, `THM-02.1/.2`, `THM-03.1/.2`, `SITE-01.2/.3`, `CNT-04.2`) theo từng slice bên dưới (do chưa tách file Living Module Brief riêng cho `THM-*`/`SITE-*`/`CNT-04` trong `docs/plans/modules/`). `M5-S08-T01` là cross-module verification, **không tạo capability trùng hoặc primary acceptance thứ hai**.

## Quy tắc phát hành và hàng rào M5

1. **G2 đã chốt đầy đủ:** Toàn bộ 4 sub-contract bị chặn (`A-DURABILITY`, `C-SEC`, `route-context binding`, `G-RECOVERY`) đã đạt **`G2 AGREED`** trong [M5 Contract](../../../../contracts/M5_THEME_TEMPLATES.md) và [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md). Không ai được hạ chuẩn bảo mật hoặc tự ý đổi contract khi không có Stop-and-Propose + ADR cập nhật.
2. **Thứ tự phát hành Issue (owner rule):** Merge tài liệu G3 này lên `main` **trước**, lấy immutable merged G3 commit SHA trên `main`, rồi mới tạo GitHub Issues dưới Milestone #6, đồng bộ Project #3 (`Ready` chỉ cho các task không có predecessor chưa xong và không xung đột file ownership; task phụ thuộc đặt `Backlog`) và cập nhật [TASK_INDEX.md](../../../TASK_INDEX.md).
3. **Cấp phát migration tuần tự (highest verified on `main@1869057`: `0014_entry_terms.sql`):** Các migration `0001`–`0014` là **bất biến**. Phân bổ tuần tự dự kiến cho M5: `0015` (`M5-S01-T01`), `0016` (`M5-S03-T02`), `0017` (`M5-S04-T01`), `0018` (`M5-S09-T01`), `0019` (`M5-S10-T01`), `0020` (`M5-S05-T01`). Mỗi implementer phải kiểm tra `origin/main` tại thời điểm claim để xác nhận số migration kế tiếp chưa bị chiếm.
4. **Một writer trên mỗi seam dùng chung:** Các seam `theme.rs`, `storage/theme.rs`, `package.rs`, `app.rs`, `routes/mod.rs`, `page.rs`, `content_render.rs`, `storage/content.rs`, `storage/mod.rs`, `opendash-core/src/builder.rs`, `opendash-core/src/registry.rs`, Admin shell chỉ có **một** task sở hữu tại mỗi lượt. Bắt buộc handoff + merge G5 trước khi task dependent chạm cùng file.
5. **Rollback DB+media:** Không dùng `git revert` để rollback migration SQLite; luôn dùng site snapshot (DB + `themes/revisions` + `uploads/media`) cùng binary tương ứng.

## Ma trận coverage và đường găng

| Candidate | G3 task | Owner dự kiến | Migration | Dependency |
| --- | --- | --- | --- | --- |
| M5-P01 | M5-S01-T01 | Forge | `0015` | G2-A / A-DURABILITY (`main@1869057`) — **có thể Ready ngay** |
| M5-P01 | M5-S01-T02 | Forge | — | M5-S01-T01 G5 |
| M5-P02 | M5-S02-T01 | Harbor | — | M5-S01-T02 G5 (bàn giao `app.rs` single-writer; dựa trên `main@1869057`, M4 G5) |
| M5-P03 | M5-S03-T01 | Forge | — | M5-S01-T02 G5, M5-S02-T01 G5 |
| M5-P03 | M5-S03-T02 | Forge | `0016` | M5-S03-T01 G5 |
| M5-P04 | M5-S04-T01 | Harbor | `0017` | M5-S01-T02 G5 |
| M5-P09 | M5-S09-T01 | Harbor | `0018` | M5-S02-T01 G5 |
| M5-P10 | M5-S10-T01 | Forge | `0019` | M5-S01-T02 G5 |
| M5-P05 | M5-S05-T01 | Harbor | `0020` | M5-S01-T02, M5-S04-T01, M5-S09-T01, M5-S10-T01 G5 |
| M5-P05 | M5-S05-T02 | Harbor | — | M5-S05-T01 G5 |
| M5-P06 | M5-S06-T01 | Forge | — | M5-S03-T02, M5-S05-T02, M5-S10-T01 G5 |
| M5-P07 | M5-S07-T01 | Harbor | — | M5-S06-T01 G5 |
| M5-P08 | M5-S08-T01 | Forge (Win) + Harbor (Mac) | — | M5-S07-T01 và toàn bộ M5 predecessors G5 |

**Critical path:** `S01-T01 → S01-T02 → S10-T01 → S05-T01 → S05-T02 → S06-T01 → S07-T01 → S08-T01`. Nhánh song song hợp lệ: `S02-T01 → S09-T01` (tách file với `S01-T01`), và sau `S01-T02` + `S02-T01`: `S03-T01 → S03-T02`, `S04-T01`, `S10-T01` (theo lượt bàn giao seam). M6 plugin registration, M7 menu CRUD/SEO UI và M8 backup/restore nằm ngoài M5.

---

## M5-S01-T01 — Registry theme package, immutable revisions và A-DURABILITY staging

- **Thuộc:** V1 · M5 · M5-S01 · M5-P01
- **Primary Acceptance Slice của capability:** M5-S01 (`THM-01.2` package lifecycle nâng cao)
- **Module:** THM-01
- **Capability:** THM-01.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (không có M5 predecessor; tách file với `S02-T01`)
- **Nguồn quyết định:** [M5 Contract §1 & A-DURABILITY](../../../../contracts/M5_THEME_TEMPLATES.md#1-package-identity-revisions-and-selection-a); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md); migration `0011_theme_activation.sql`.
- **Người triển khai / review:** Forge (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
M3 hiện chỉ kích hoạt một block ID trong `0011_theme_activation.sql`. M5 cần package identity bất biến qua update/restart, immutable revisions lưu trên `{site_root}/themes/revisions/{revision_digest}/` theo giao thức `A-DURABILITY`, và một singleton active pointer có CAS để rollback không mất liên kết hay dữ liệu site.

### Mục tiêu và hành vi quan sát được
Trên bản copy DB M3 (cả trường hợp NULL và explicit active block), migration `0015` giữ nguyên trạng thái legacy; một `theme_id` có thể chứa nhiều immutable revisions, chuyển active/rollback nguyên tử với CAS `412`/`428`, và khởi động lại cho kết quả nhất quán.

### Phạm vi công việc
- Forward migration `cms/migrations/0015_theme_packages_revisions.sql`: thêm bảng `theme_packages`, `theme_revisions`, `theme_legacy_mappings`, `theme_active_selection` (singleton `id = 1`, `revision_id NULLABLE`, monotonic `version`). Ánh xạ từng legacy full block ID sang identity legacy riêng (không suy owner từ prefix, không sửa `0011`).
- Định nghĩa DTOs (`ThemePackageRecord`, `ThemeRevisionRecord`, `ActiveThemeSelection`, `RevisionDurabilityState`) và giao thức `A-DURABILITY` trong `storage/theme.rs` + helper filesystem staging (`themes/.staging/{run_uuid}/` → `sync_all()` + drop handles trên Windows → rename vào `themes/revisions/{revision_digest}/` + `.complete` marker + `sync_all()`, rồi mới commit `BEGIN IMMEDIATE` SQLite; startup quarantine `.staging/*` mồ côi).
- Typed storage API cho `S01-T02`: idempotent digest reuse, `409` khi trùng `(theme_id, version)` khác digest, `412` stale version, `428` thiếu precondition, diagnostics khi thiếu `.complete` hoặc mismatch digest trên đĩa.

### Ngoài phạm vi
Không triển khai ZIP v2 parser, HTTP routes `/api/themes/*` hay `/theme/*` assets (`S01-T02`), không làm declarations CNT (`S10-T01`) hay renderer (`S03-T01`).

### Phụ thuộc và điều kiện chặn
Không phụ thuộc task M5 khác. Đọc `cms/migrations/0011_theme_activation.sql`, `cms/crates/opendash-server/src/storage/theme.rs`, `cms/crates/opendash-server/src/theme.rs`.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/migrations/0015_theme_packages_revisions.sql` (mới), `cms/crates/opendash-server/src/storage/theme.rs`, `cms/crates/opendash-server/src/storage/builder.rs` (chỉ cập nhật query đọc trong `read_home_tx` từ `site_active_theme` sang `theme_active_selection`), `cms/crates/opendash-server/src/storage/page.rs` (chỉ cập nhật query đọc trong `active_content_templates` từ `site_active_theme` sang `theme_active_selection`), `cms/crates/opendash-server/src/storage/mod.rs` (chỉ nếu export thêm type), unit/integration tests trong `cms/crates/opendash-server/tests/` (tên riêng `m5_s01_t01_theme_revisions.rs`).
- **Không sửa đồng thời:** Không chạm `src/theme.rs`, `src/package.rs`, `src/routes/themes.rs` (dành cho `S01-T02`); không chạm `app.rs` hay `routes/public*.rs` (`S02-T01`).
- **Contract/API:** Một singleton active pointer; legacy NULL giữ nguyên behavior; không dual-write pointer cũ; `A-DURABILITY` write-before-commit.

### Cách triển khai theo contract
Tuân thủ `opendash_core::id`, `ApiError`, SQLite queries chỉ nằm trong `storage/theme.rs`. Nếu phát hiện thay đổi schema ảnh hưởng M3 `0011`, thực hiện **Stop-and-Propose**.

### Nghiệm thu
- [ ] Upgrade DB copy M3 (NULL và explicit active) giữ nguyên lựa chọn, mapping nhiều legacy block ID cùng prefix độc lập, restart nhất quán.
- [ ] Hai revisions cùng `theme_id` tách biệt; duplicate canonical digest idempotent; trùng `(theme_id, version)` khác digest trả `409`.
- [ ] CAS `412` trên stale version, `428` khi thiếu version; không ghi nửa transaction.
- [ ] `A-DURABILITY`: thiếu `.complete` hoặc corrupt file trên đĩa bị từ chối khi activate/read kèm diagnostic an toàn; stale `.staging/*` được dọn khi startup mà không chạm `revisions/`.
- [ ] `cargo test --manifest-path cms/Cargo.toml --workspace --locked` PASS và ghi evidence SHA/OS/lệnh thực chạy.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, test migration từ snapshot `0014` có `0011` data.
- **Rủi ro/rollback:** Sai mapping legacy có thể làm hỏng `home_page` M1/M3; bảo toàn bằng snapshot DB+`themes/` trước migration.

### Bàn giao và review
Bàn giao `storage/theme.rs` typed APIs, DTOs và G5 SHA cho `M5-S01-T02`.

---

## M5-S01-T02 — ZIP v2 validation, staging install, activation/rollback và `/theme/` asset route

- **Thuộc:** V1 · M5 · M5-S01 · M5-P01
- **Primary Acceptance Slice của capability:** M5-S01 (tiếp `THM-01.2`, không tạo primary thứ hai)
- **Module:** THM-01
- **Capability:** THM-01.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S01-T01` G5)
- **Nguồn quyết định:** [M5 Contract §1 & A-DURABILITY](../../../../contracts/M5_THEME_TEMPLATES.md#1-package-identity-revisions-and-selection-a); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Forge (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Mở rộng bộ nạp ZIP theme để hỗ trợ manifest v2 an toàn (giữ tương thích v1 64 KiB), tách biệt **install** (validate + stage `A-DURABILITY`, không tự kích hoạt) khỏi **activate/rollback** có `If-Match`/CSRF/Origin, và phục vụ asset tĩnh revision-scoped dưới `/theme/`.

### Mục tiêu và hành vi quan sát được
Upload ZIP v2 tạo immutable revision ở trạng thái chưa kích hoạt; activate/rollback kiểm `If-Match`, không hỏng active revision cũ khi lỗi; ZIP traversal, zip bomb, symlink, path alias hoặc manifest chứa `declarations` chưa được `S10` hỗ trợ bị từ chối sạch sẽ.

### Phạm vi công việc
- Mở rộng `cms/crates/opendash-server/src/package.rs` với helper v2 path policy (relative ASCII, cấm `..`/backslash/drive/symlink/case-insensitive collision, kiểm byte thực đọc cho quota 8 MiB ZIP / 32 MiB decoded / 256 files / 256 KiB text / 8 MiB raster / 100:1 ratio) **mà giữ nguyên 100% hành vi plugin và ZIP v1 64 KiB**.
- Cập nhật `src/theme.rs` và `src/routes/themes.rs`: parse manifest v2 (`theme_id` kebab 1–64, `version`, templates/roles, optional `options`/`menus`/`sample`/`declarations`), gọi `S01-T01` `A-DURABILITY` staging & SQLite commit.
- **Chặn vòng phụ thuộc `S01-T02` ↔ `S10-T01`:** Ở `S01-T02`, nếu revision chứa `declarations` (CPT/fields/taxonomy), **từ chối kích hoạt** (`409` / `422` `unsupported_declarations_pending_cnt_bridge`) cho tới khi `S10-T01` tích hợp đăng ký nguyên tử.
- Cập nhật `cms/crates/opendash-server/src/app.rs`: nâng body limit `POST /api/admin/themes` lên 8 MiB (giữ nguyên limit v1/plugin) và đăng ký route phục vụ asset revision-scoped `GET /theme/{revision_digest}/{*path}` với `X-Content-Type-Options: nosniff`, kiểm `.complete` + MIME allowlist + no-traversal, không lộ file ngoài `themes/revisions/{digest}/`.

### Ngoài phạm vi
Không triển khai MiniJinja rendering (`S03-T01`), Theme Options UI (`S04-T01`), CNT declarations (`S10-T01`), hay sample import (`S05`).

### Phụ thuộc và điều kiện chặn
`M5-S01-T01` G5 đã merge lên `main`.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/crates/opendash-server/src/package.rs`, `cms/crates/opendash-server/src/theme.rs`, `cms/crates/opendash-server/src/routes/themes.rs`, `cms/crates/opendash-server/src/routes/mod.rs`, `cms/crates/opendash-server/src/app.rs` (đăng ký `/theme/{revision_digest}/{*path}` và nâng body limit `POST /api/admin/themes` lên 8 MiB, giữ nguyên limit v1/plugin), tests `cms/crates/opendash-server/tests/m5_s01_t02_zip_v2.rs`.
- **Không sửa đồng thời:** `S02-T01`, `S03-T01` và `S10-T01` đợi handoff `src/app.rs` / `src/theme.rs` sau khi task này G5.
- **Contract/API:** Install ≠ activate; `401`/`403` RBAC/CSRF/Origin, `412` stale `If-Match`, `413` quota, `428` missing `If-Match`.

### Cách triển khai theo contract
Dùng `opendash_server::auth` guards, `opendash_server::package` primitives, `ApiError`.

### Nghiệm thu
- [ ] ZIP v1 M3 và plugin ZIP callers hiện có không regression.
- [ ] ZIP v2 install không tự activate; activate/rollback với `If-Match` giữ nguyên revision cũ nếu lỗi.
- [ ] Malicious ZIP corpus (traversal, zip bomb, zero-compressed-size bomb, backslash, case collision, symlink, oversized text/raster) bị reject trước khi chạm `revisions/` hay DB.
- [ ] Revision có `declarations` bị từ chối kích hoạt có chẩn đoán rõ ràng (không partial activation) trước khi `S10-T01` tồn tại.
- [ ] `/theme/` trả `nosniff`, chỉ phục vụ asset trong active/valid revision, chặn traversal và missing `.complete`.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`.
- **Rủi ro/rollback:** Regression plugin ZIP loader nếu sửa sai `package.rs`; giữ test plugin package chạy trong cùng suite.

### Bàn giao và review
Bàn giao `src/theme.rs` manifest v2 parser/revision reader và `/theme/` asset URL helper cho `S03-T01`, `S04-T01`, `S10-T01`.

---

## M5-S02-T01 — Public RequestContext, URL resolver, canonical builders và consumers

- **Thuộc:** V1 · M5 · M5-S02 · M5-P02
- **Primary Acceptance Slice của capability:** M5-S02 (`CNT-04.2` public route dispatch & RequestContext)
- **Module:** CNT-04
- **Capability:** CNT-04.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S01-T02` G5 để bàn giao `app.rs` single-writer)
- **Nguồn quyết định:** [M5 Contract §2](../../../../contracts/M5_THEME_TEMPLATES.md#2-public-url-and-requestcontext-b); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Harbor (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
HTTP public routes phải do core giải quyết độc lập với theme, cung cấp `RequestContext` typed cho Home, Single, Archive, TaxonomyArchive và NotFound, đồng thời thay thế mọi chỗ tự ghép canonical URL rải rác trong codebase.

### Mục tiêu và hành vi quan sát được
`GET /`, `/content/{type_key}`, `/content/{type_key}/{slug}`, `/taxonomy/category/{slug}`, `/taxonomy/tag/{slug}` phân giải đúng `RequestContext`; old slug trả `404`; draft/private trả `404`; encoded separator / double decode / reserved prefix bị chặn; mọi canonical link trong `content_render.rs`, `page.rs`, `storage/builder_binding.rs` dùng chung một canonical builder.

### Phạm vi công việc
- Tạo `cms/crates/opendash-server/src/public_context.rs` định nghĩa `PublicRequestContext` (`Home`, `Single`, `Archive`, `TaxonomyArchive`, `NotFound`), `CanonicalUrlBuilder` (ID/type/slug/term → canonical path), query parser (whitelisted typed keys, duplicate/invalid recognized query → `400`, bỏ qua unknown tracking keys và loại khỏi canonical output, giữ archive page size 20 / max 10,000 tách biệt với M4 list 1–20 / 1–100).
- Cập nhật `src/routes/public.rs`, `src/routes/public_content.rs`, `src/app.rs` cho `/taxonomy/category/{slug}` và `/taxonomy/tag/{slug}` (category exact mặc định, `descendants=true` tường minh, AND filter trước khi phân trang, không lộ draft trong count/page).
- Cập nhật các consumer hiện có trong `src/content_render.rs`, `src/page.rs`, `src/storage/builder_binding.rs` để dùng `CanonicalUrlBuilder` thay vì hardcode URL string.

### Ngoài phạm vi
Không cho theme khai báo route; không làm redirect/alias cũ hay SEO sitemap M7; không làm MiniJinja role composition (`S03-T01`) hay menu model (`S09-T01`).

### Phụ thuộc và điều kiện chặn
`M5-S01-T02` G5 đã merge lên `main` (bàn giao `cms/crates/opendash-server/src/app.rs` single-writer; dựa trên M3 public routes và M4 taxonomy `0012`/`0014` đã có trên `main@1869057`).

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/crates/opendash-server/src/public_context.rs` (mới), `cms/crates/opendash-server/src/lib.rs` (khai báo module), `cms/crates/opendash-server/src/app.rs`, `cms/crates/opendash-server/src/routes/public.rs`, `cms/crates/opendash-server/src/routes/public_content.rs`, `cms/crates/opendash-server/src/storage/public_content.rs`, `cms/crates/opendash-server/src/content_render.rs`, `cms/crates/opendash-server/src/page.rs`, `cms/crates/opendash-server/src/storage/builder_binding.rs`, tests `cms/crates/opendash-server/tests/m5_s02_t01_public_context.rs`.
- **Không sửa đồng thời:** `S03-T01` và `S09-T01` đợi task này G5 trước khi chạm `content_render.rs`, `page.rs`, `app.rs`.
- **Contract/API:** Old slug `404`; bad query `400`; draft/missing/wrong-kind `404`; `PublicRequestContext` không chứa session/token/private path.

### Cách triển khai theo contract
SQL queries giữ trong `storage/public_content.rs` và `storage/taxonomy.rs` (chỉ đọc nếu cần, không sửa schema).

### Nghiệm thu
- [ ] `GET /`, `/content/{type_key}`, `/content/{type_key}/{slug}` M3 không regression; `/taxonomy/category/{slug}` và `/taxonomy/tag/{slug}` hoạt động đúng với pagination và `descendants=true`.
- [ ] Draft/unpublished không xuất hiện và không làm lệch total count/pagination; old slug và wrong taxonomy kind trả `404`.
- [ ] Encoded `%2F`/double-decode/reserved namespace bị chặn; duplicate/invalid query trả `400`; tracking query bị loại khỏi canonical URL.
- [ ] `content_render.rs`, `page.rs`, `storage/builder_binding.rs` dùng chung `CanonicalUrlBuilder`.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`.
- **Rủi ro/rollback:** Thay đổi route precedence có thể ảnh hưởng `/` hoặc `/assets/`; kiểm negative matrix và M3/M4 regression.

### Bàn giao và review
Bàn giao `PublicRequestContext` và `CanonicalUrlBuilder` cho `S03-T01` và `S09-T01`.

---

## M5-S03-T01 — Template roles, C-SEC MiniJinja safe subset và `BindingSource::RouteContext`

- **Thuộc:** V1 · M5 · M5-S03 · M5-P03
- **Primary Acceptance Slice của capability:** M5-S03 (`THM-02.1` role renderer & composition)
- **Module:** THM-02
- **Capability:** THM-02.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S01-T02` và `M5-S02-T01` G5)
- **Nguồn quyết định:** [M5 Contract §3 & C-SEC](../../../../contracts/M5_THEME_TEMPLATES.md#3-templates-and-overrides-ch); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Forge (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Cung cấp bộ chọn template theo role (`home|page|header|footer|archive|single|taxonomy_archive`), ghép `header + body + footer` một lần trong core, thực thi **ngay từ đầu** bộ kiểm tra an toàn `C-SEC` trên MiniJinja `=3.0.5`, và bổ sung `BindingSource::RouteContext` trong `opendash-core` cho ngữ cảnh entry hiện tại.

### Mục tiêu và hành vi quan sát được
Public routes dựng trang qua role phù hợp trên active revision (giữ nguyên Builder homepage site-owned), fallback về core an toàn khi thiếu/lỗi template, từ chối template vi phạm `C-SEC` ngay lúc install/compile, và hỗ trợ `BindingSource::RouteContext` cho `single`/`page`.

### Phạm vi công việc
- Bổ sung feature `unstable_machinery` cho `minijinja = "=3.0.5"` trong `cms/Cargo.toml` (giữ nguyên version `=3.0.5`, đồng bộ `cms/Cargo.lock`).
- Tạo `cms/crates/opendash-server/src/template_sec.rs` thực thi đầy đủ `C-SEC`: `Environment::empty()`, unconditional `AutoEscape::Html`, `UndefinedBehavior::Strict`, `set_fuel(Some(10_000))`, `set_recursion_limit(16)`, output cap `512 KiB`, AST allowlist (`Stmt` & `Expr`, chặn `Call`/`Macro`/`Include`/`Extends`/`Import`/`AutoEscape`/`FilterBlock`/dunder), contextual HTML/URL/CSS validator (cấm `script`, `svg`, `on*`, `style="{{ expr }}"`, `@import`, `expression(`, URL động ngoài typed fields), gắn vào **cả** install-time (`theme.rs`) lẫn compile/render (`content_render.rs`, `page.rs`). Áp dụng cùng validator cho legacy v1 (`block.html`, `content-list.html`, `content-single.html`).
- Cập nhật `cms/crates/opendash-core/src/builder.rs` thêm `BindingSource::RouteContext { field_key: String }` (allowlist `title`, `slug`, `url`, `cover_url`, hoặc valid field key của scoped CPT; giữ nguyên `FixedEntry` 100%; từ chối `RouteContext` trên role khác `single`/`page`).
- Ghép `header + body + footer` một lần trong core (`page.rs` / `content_render.rs`), giữ nguyên public CSP và fallback core an toàn.

### Ngoài phạm vi
Không làm override CRUD/Preview/Save (`S03-T02`) hay adversarial browser a11y/corpus mở rộng (`S07-T01`), dù unit security tests cơ bản của `C-SEC` phải có ngay trong task này.

### Phụ thuộc và điều kiện chặn
`M5-S01-T02` G5 và `M5-S02-T01` G5 đã merge.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/template_sec.rs` (mới), `cms/crates/opendash-server/src/lib.rs`, `cms/crates/opendash-server/src/theme.rs`, `cms/crates/opendash-server/src/page.rs`, `cms/crates/opendash-server/src/content_render.rs`, `cms/crates/opendash-server/src/storage/builder_binding.rs`, tests `cms/crates/opendash-server/tests/m5_s03_t01_roles_csec.rs`.
- **Không sửa đồng thời:** `S03-T02` đợi task này G5.
- **Contract/API:** `C-SEC` mandatory ở install/compile/render; không có raw HTML interpolation để lồng role; `RouteContext` không fallback sang entry khác khi missing/unpublished.

### Cách triển khai theo contract
Giữ `cms/Cargo.lock` đồng bộ (`cargo test --manifest-path cms/Cargo.toml --workspace --locked`).

### Nghiệm thu
- [ ] `header`/`footer`/`home`/`page`/`single`/`archive`/`taxonomy_archive` chọn đúng role và fallback core an toàn khi thiếu hoặc invalid (không 500, không lộ host path).
- [ ] `C-SEC` chặn thành công `Call`, `Macro`, `Include`, `safe` filter, thẻ `script`/`svg`, `on*`, `style="{{ expr }}"`, `javascript:` href, CSS `@import` hoặc đóng thẻ `style`, fuel exhaustion (>10,000), recursion (>16), output (>512 KiB).
- [ ] Legacy v1 non-conforming fallback sang core renderer an toàn; Builder homepage M4 không bị theme mới ghi đè.
- [ ] `BindingSource::RouteContext` resolve đúng field của current entry trên `single`/`page`, omit an toàn khi unpublished/non-entry, và giữ nguyên 100% `FixedEntry`.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`.
- **Rủi ro/rollback:** Sai AST check có thể làm gãy template hợp lệ; kiểm fixture `DEFAULT_ARCHIVE`/`DEFAULT_SINGLE` và sample theme hợp lệ.

### Bàn giao và review
Bàn giao `template_sec.rs`, role composition contract và `BindingSource::RouteContext` cho `S03-T02` và `S07-T01`.

---

## M5-S03-T02 — Site-owned template overrides và Preview/Save có CAS

- **Thuộc:** V1 · M5 · M5-S03 · M5-P03
- **Primary Acceptance Slice của capability:** M5-S03 (tiếp `THM-02.1`, không tạo primary thứ hai)
- **Module:** THM-02
- **Capability:** THM-02.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S03-T01` G5)
- **Nguồn quyết định:** [M5 Contract §3](../../../../contracts/M5_THEME_TEMPLATES.md#3-templates-and-overrides-ch); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Forge (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Cho phép Admin/Editor tùy biến layout theo role (`page|header|footer|archive|single|taxonomy_archive`) dưới dạng site-owned typed AST lưu độc lập với theme ZIP, giữ nguyên bản đã sửa qua update/switch/restart và tách bạch Preview chưa lưu khỏi public.

### Mục tiêu và hành vi quan sát được
Mở Builder cho role override (kể cả `RouteContext` trên `single`/`page`), Preview hiển thị bản chưa Save cho phiên đã xác thực (không lộ public), Save kiểm CAS `If-Match`, Reset to default yêu cầu xác nhận + CAS, và đổi theme `A → B → A` vẫn giữ override.

### Phạm vi công việc
- Forward migration `cms/migrations/0016_template_overrides.sql`: bảng `template_overrides` (`id`, `role`, `scope_type_key`, `scope_entry_id`, `ast_json`, `version`, `base_theme_id`, `base_revision_id`, `updated_at`), unique index trên `(role, COALESCE(scope_type_key,''), COALESCE(scope_entry_id,''))`. Không chạm `home_page` M1/M4.
- Tạo `cms/crates/opendash-server/src/storage/overrides.rs` và routes Admin/Preview/Save/Reset (RBAC, CSRF/Origin, `If-Match` `412`/`428`, Preview `Cache-Control: no-store` trong iframe `sandbox=""`).
- Tích hợp độ ưu tiên vào resolver (`S03-T01`): most specific saved override → less specific saved override → active revision role → core fallback. Giữ round-trip node khi descriptor missing.
- Cập nhật Admin Builder UI (`cms/admin/`) cho phép chọn role override, gắn `RouteContext` trên `single`/`page`, Preview, Save và Reset.

### Ngoài phạm vi
Không viết lại Builder engine; không làm menu Builder M7; không tự động ghi đè override bằng theme default.

### Phụ thuộc và điều kiện chặn
`M5-S03-T01` G5 đã merge.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/migrations/0016_template_overrides.sql` (mới), `cms/crates/opendash-server/src/storage/overrides.rs` (mới), `cms/crates/opendash-server/src/storage/mod.rs`, `cms/crates/opendash-server/src/routes/builder.rs` (hoặc route override mới gắn vào `routes/mod.rs`), `cms/crates/opendash-server/src/page.rs`, `cms/admin/src/` (Builder role selector), tests `cms/crates/opendash-server/tests/m5_s03_t02_overrides.rs`.
- **Không sửa đồng thời:** `S06-T01` đợi task này G5.
- **Contract/API:** Preview never writes; post/page draft Save không tự publish; Builder role override Save cập nhật public rendering ngay mà không publish draft entry.

### Cách triển khai theo contract
Build lại `cms/admin` (`npm ci && npm run build`) nếu sửa frontend và kiểm rust workspace locked.

### Nghiệm thu
- [ ] Preview chưa Save không lộ ra public; unauthenticated Preview bị `401`/`403`.
- [ ] CAS `412` giữ nguyên edit mới; Reset cần explicit confirmation + CAS; CSRF/Origin bảo vệ đầy đủ.
- [ ] `A → A2` và `A → B → A` + restart bảo toàn override AST và `RouteContext` binding.
- [ ] Draft post/page Save không làm lộ draft ra public; `single`/`page` override render đúng entry hiện tại trên nhiều entry khác nhau (không trói cố định 1 entry).

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, `npm --prefix cms/admin run build`.
- **Rủi ro/rollback:** Xung đột giữa `home_page` và `home` override; giữ `home_page` là authoritative home body theo `M5_THEME_TEMPLATES.md`.

### Bàn giao và review
Bàn giao override schema, precedence và evidence cho `S06-T01`.

---

## M5-S04-T01 — Typed Theme Options qua SITE-01 Settings core và media ACL

- **Thuộc:** V1 · M5 · M5-S04 · M5-P04
- **Primary Acceptance Slice của capability:** M5-S04 (`SITE-01.2` Theme Options)
- **Module:** SITE-01
- **Capability:** SITE-01.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S01-T02` G5)
- **Nguồn quyết định:** [M5 Contract §4](../../../../contracts/M5_THEME_TEMPLATES.md#4-theme-options-and-site-01-menus-df); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Harbor (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Cho phép theme khai báo options có kiểu dữ liệu rõ ràng (`string`, `number`, `boolean`, `select`, `image_ref`) dùng chung hạ tầng SITE-01 Settings, tách riêng default của revision khỏi giá trị người dùng đã lưu, bảo vệ `image_ref` bằng transactional saved-reference ACL, và thể hiện hiệu ứng thật trên public header/footer.

### Mục tiêu và hành vi quan sát được
Cài theme có manifest options hợp lệ, Admin hiển thị auto-form và Save (Admin-only, CSRF/Origin, CAS); đổi theme `A → B → A` hoặc nâng cấp tương thích giữ nguyên giá trị người dùng; public header/footer hiển thị option đã whitelist và ảnh `image_ref` được phép đọc.

### Phạm vi công việc
- Forward migration `cms/migrations/0017_theme_options.sql`: bảng `theme_option_values` (`theme_id`, `option_key`, `value_json`, `version`, `updated_at`) và `theme_option_media_refs` (`theme_id`, `option_key`, `media_id` FK `RESTRICT` tới `media_assets(id)` để bảo vệ soft-delete/ACL).
- Định nghĩa DTO & giới hạn chính xác trong `storage/settings.rs`: tối đa 64 options/theme, `option_key` snake_case 1–64, `string` max 4,096 chars (plain text, không raw HTML/CSS), `number` finite `f64` kèm `min`/`max`, `boolean`, `select` max 64 choices, `image_ref` tham chiếu `media_assets(id)` hợp lệ.
- API Admin `GET/PUT /api/themes/{theme_id}/options` (Admin-only — Editor nhận `403`, CSRF/Origin, `If-Match` `412`/`428`) + API transaction-aware (`&mut SqliteConnection`) để `S05-T02` dùng chung.
- Tách default vs saved value: xóa/thay/switch `image_ref` cập nhật `theme_option_media_refs` nguyên tử trong transaction (default chưa save không tự public hóa media); incompatible option type/default change khi activate revision mới bị từ chối `409`.
- Whitelist public exposure keys đưa vào view model cho header/footer templates + auto-form trong `cms/admin/`.

### Ngoài phạm vi
Không sửa General Settings M3; không hỗ trợ regex/secret/raw CSS options; không làm plugin options M6.

### Phụ thuộc và điều kiện chặn
`M5-S01-T02` G5 đã merge.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/migrations/0017_theme_options.sql` (mới), `cms/crates/opendash-server/src/storage/settings.rs`, `cms/crates/opendash-server/src/routes/settings.rs`, `cms/admin/src/` (Theme Options auto-form), tests `cms/crates/opendash-server/tests/m5_s04_t01_theme_options.rs`.
- **Không sửa đồng thời:** `S09-T01` không chạm `settings.rs`; `S05-T01/02` đợi G5.
- **Contract/API:** Admin-only (`Editor` → `403`); transactional saved-reference ACL cho `image_ref`; chỉ expose key có `public: true` vào template context.

### Cách triển khai theo contract
Dùng chung `opendash_server::auth`, `ApiError`, SQLite transaction trong `storage/settings.rs`.

### Nghiệm thu
- [ ] Auto-form render đúng 5 kiểu dữ liệu; từ chối string quá dài, NaN/Inf/out-of-range number, invalid select, invalid `image_ref`.
- [ ] Admin-only (`401`/`403` cho Guest/Editor), CSRF/Origin, `412` CAS, `409` khi upgrade incompatible option schema.
- [ ] Giá trị đã lưu giữ nguyên qua `A → A2`, `A → B → A` và restart; option bị bỏ trở thành dormant retained data.
- [ ] Transactional `image_ref` ACL: ảnh chỉ truy cập công khai khi được lưu trong option đang active và `public: true`; đổi/gỡ option thu hồi ACL ngay; default chưa save không tự public media.
- [ ] Browser/SSR kiểm chứng option làm thay đổi thực tế output public header/footer.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, `npm --prefix cms/admin run build`.
- **Rủi ro/rollback:** Schema conflict khi update; từ chối `409` trước khi activate, không tự xóa dữ liệu.

### Bàn giao và review
Bàn giao `storage/settings.rs` transaction-aware Theme Options API và public whitelist adapter cho `S05-T01/02`.

---

## M5-S09-T01 — Menu model SITE-01: stable IDs, ordering, slot rendering và canonical refs

- **Thuộc:** V1 · M5 · M5-S09 · M5-P09
- **Primary Acceptance Slice của capability:** M5-S09 (`SITE-01.3` menu storage tối thiểu)
- **Module:** SITE-01
- **Capability:** SITE-01.3
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S02-T01` G5)
- **Nguồn quyết định:** [M5 Contract §4](../../../../contracts/M5_THEME_TEMPLATES.md#4-theme-options-and-site-01-menus-df); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Harbor (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Xây dựng mô hình menu site-owned chuẩn cho `SITE-01` (dùng lại trực tiếp ở M7) với stable IDs, parent-child ordering, target tham chiếu theo entity ID qua `CanonicalUrlBuilder`, và cơ chế render menu slot ra public header/footer.

### Mục tiêu và hành vi quan sát được
Menu và items lưu bền vững độc lập với theme; đổi slug entry/term làm URL menu tự cập nhật; target bị xóa/draft bị ẩn khỏi public (kèm diagnostic Admin, không tự xóa item); đổi theme `A → B → A` không mất menu hay slot assignment.

### Phạm vi công việc
- Forward migration `cms/migrations/0018_site_menus.sql`: bảng `site_menus` (`id`, `menu_key`, `title`, `version`, `origin_provenance`), `site_menu_items` (`id`, `menu_id`, `parent_id`, `position`, `label`, `target_kind`, `target_entry_id`, `target_term_id`, `external_url`), `site_menu_slot_assignments` (`theme_id`, `slot_key`, `menu_id`).
- Tạo `cms/crates/opendash-server/src/storage/menu.rs` với transaction-aware API (`&mut SqliteConnection`): giới hạn 200 items/menu, depth tối đa 8, cùng parent trong cùng menu, deterministic sort `(parent_id, position, id)`, chặn cycle / duplicate sibling position / duplicate origin / invalid external URL (chỉ HTTP(S), không credentials/control/protocol-relative), CAS `412`.
- Tạo `MenuViewModel` resolver dùng `CanonicalUrlBuilder` (`S02-T01`) để render menu slot vào public header/footer view model; ẩn item trỏ tới entry/term chưa publish hoặc đã xóa mà không xóa row DB.

### Ngoài phạm vi
Không làm Admin drag-and-drop menu CRUD UI đầy đủ (thuộc M7-P02); không tạo bảng menu riêng trong module theme.

### Phụ thuộc và điều kiện chặn
`M5-S02-T01` G5 đã merge (`CanonicalUrlBuilder`).

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/migrations/0018_site_menus.sql` (mới), `cms/crates/opendash-server/src/storage/menu.rs` (mới), `cms/crates/opendash-server/src/storage/mod.rs`, `cms/crates/opendash-server/src/page.rs` (hoặc slot helper gắn view model), tests `cms/crates/opendash-server/tests/m5_s09_t01_site_menus.rs`.
- **Không sửa đồng thời:** `S04-T01` giữ `settings.rs`; `S05-T01/02` đợi G5.
- **Contract/API:** Site-owned IDs; max 200 items, max depth 8; transaction-aware API cho `S05`.

### Cách triển khai theo contract
SQLite transactions trong `storage/menu.rs`; dùng `CanonicalUrlBuilder` thay vì ghép path thủ công.

### Nghiệm thu
- [ ] Menu/item IDs, hierarchy và thứ tự giữ nguyên qua restart và theme switch `A → B → A`.
- [ ] Entry/term đổi slug → menu render URL canonical mới; unpublished/deleted target bị ẩn khỏi public (có diagnostic Admin) mà không xóa row.
- [ ] Cycle, depth > 8, > 200 items, cross-menu parent, duplicate sibling position, unsafe URL (`javascript:`, `//host`, userinfo) và stale `412` bị từ chối nguyên tử.
- [ ] Public header/footer render được slot menu qua `MenuViewModel`.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`.
- **Rủi ro/rollback:** Dangling target refs; giữ row kèm diagnostic an toàn, snapshot DB trước migration.

### Bàn giao và review
Bàn giao `storage/menu.rs` transaction-aware API và `MenuViewModel` cho `S05-T01/02` và M7-P02.

---

## M5-S10-T01 — Theme CPT/fields/taxonomy declarations qua canonical CNT registries

- **Thuộc:** V1 · M5 · M5-S10 · M5-P10
- **Primary Acceptance Slice của capability:** M5-S10 (`THM-01.3` theme declarations registration)
- **Module:** THM-01
- **Capability:** THM-01.3
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S01-T02` G5)
- **Nguồn quyết định:** [M5 Contract §5](../../../../contracts/M5_THEME_TEMPLATES.md#5-cnt-lifecycle-and-taxonomy-e); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Forge (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Cho phép ZIP theme v2 khai báo CPT, custom fields và taxonomy eligibility (`category`/`tag` lõi) qua canonical registry và CNT storage trong cùng transaction kích hoạt theme, tách biệt **quyền sở hữu schema/dữ liệu của site** khỏi trạng thái khả dụng của provider.

### Mục tiêu và hành vi quan sát được
Kích hoạt theme A có declarations tạo/cập nhật CPT + fields + category/tag eligibility nguyên tử; collision với Core (`post`/`page`) hoặc owner khác (kể cả inactive/withdrawn) trả `409`; nâng cấp bỏ declaration chỉ đánh dấu `withdrawn` mà giữ nguyên schema/entries/term links; đổi theme hay restart vẫn giữ schema và dữ liệu.

### Phạm vi công việc
- Forward migration `cms/migrations/0019_theme_cnt_declarations.sql`: bổ sung metadata ownership/declaration state (`owner_kind`, `owner_id`, `declaration_state`: `declared` | `withdrawn`, `provider_status`) và bảng/cột taxonomy eligibility (`category`/`tag` only) mà **không sửa `0004_content_types.sql` hay `0012_taxonomy_terms.sql`**.
- Cập nhật `cms/crates/opendash-core/src/registry.rs` (lưu ý đường dẫn đúng ở `opendash-core`, **không** có SQL/IO trong core) để hỗ trợ rõ trạng thái declaration (`declared`/`withdrawn`) tách khỏi `ProviderStatus` (`active`/`inactive`/`missing`).
- Xây dựng canonical transaction-aware registration/evolution adapter trong `cms/crates/opendash-server/src/storage/content.rs` (hàm nhận `&mut SqliteConnection`): kiểm owner collision (`409` kể cả inactive/withdrawn, cấm claim `post`/`page`), kiểm schema compatibility (cho phép label/help/default và optional field mới; từ chối đổi type, đổi key, thêm `required` hoặc xóa choice đang có dữ liệu cũ), giữ in-memory `RegistryCatalog` chỉ đồng bộ **sau khi** DB commit thành công.
- Mở khóa kích hoạt revision có `declarations` trong `src/theme.rs` (thay thế reject tạm ở `S01-T02` bằng transaction nguyên tử gồm declarations + active pointer; lỗi declarations rollback toàn bộ activation).

### Ngoài phạm vi
Không làm M6 plugin ABI bridge; không cho phép custom taxonomy kind mới ngoài `category`/`tag`; không tự xóa dữ liệu khi withdrawn.

### Phụ thuộc và điều kiện chặn
`M5-S01-T02` G5 đã merge.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/migrations/0019_theme_cnt_declarations.sql` (mới), `cms/crates/opendash-core/src/registry.rs`, `cms/crates/opendash-server/src/storage/content.rs`, `cms/crates/opendash-server/src/theme.rs` (tích hợp activation hook), tests `cms/crates/opendash-server/tests/m5_s10_t01_theme_cnt_declarations.rs`.
- **Không sửa đồng thời:** `S06-T01` đợi task này G5.
- **Contract/API:** Không SQL/IO trong `opendash-core`; collision `409` kể cả inactive/withdrawn; `in-memory registry` refresh chỉ sau commit; core `category`/`tag` sở hữu terms.

### Cách triển khai theo contract
Mọi thao tác CNT phải đi qua `opendash_core::id`, `opendash_core::registry` và `storage/content.rs`, không tạo bảng CNT song song.

### Nghiệm thu
- [ ] Theme A v2 khai báo CPT + fields + category/tag eligibility thành công; Admin tạo/sửa entry và gán category/tag bình thường.
- [ ] Chiếm `post`/`page` hoặc owner khác (kể cả inactive/withdrawn) bị `409` và không ghi partial DB.
- [ ] Same-owner compatible re-declare idempotent; incompatible schema change bị reject bảo toàn dữ liệu cũ; bỏ declaration ở bản cập nhật chỉ chuyển `withdrawn` mà giữ entries/terms.
- [ ] In-memory registry không bị ô nhiễm nếu transaction DB rollback; restart khôi phục đúng trạng thái từ DB.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`.
- **Rủi ro/rollback:** Lệch sync giữa DB và `RegistryCatalog`; đồng bộ sau commit và verify qua restart test.

### Bàn giao và review
Bàn giao CNT theme lifecycle/declaration API và fixture `A → B` cho `S05-T01/02` và `S06-T01`.

---

## M5-S05-T01 — Selective sample import dry-run và logical provenance plan

- **Thuộc:** V1 · M5 · M5-S05 · M5-P05
- **Primary Acceptance Slice của capability:** M5-S05 (`THM-03.1` selective sample import)
- **Module:** THM-03
- **Capability:** THM-03.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S01-T02`, `M5-S04-T01`, `M5-S09-T01`, `M5-S10-T01` G5)
- **Nguồn quyết định:** [M5 Contract §6 & G-RECOVERY](../../../../contracts/M5_THEME_TEMPLATES.md#6-selective-import-and-provenance-g); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Harbor (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Cho phép Admin xem trước kế hoạch nhập dữ liệu mẫu có chọn lọc (`pages`, `menus`, `media`, `settings`) với logical provenance và conflict detection trước khi chạm tới DB hay filesystem.

### Mục tiêu và hành vi quan sát được
Gọi dry-run trên site bất kỳ trả về plan xác định (deterministic) gồm từng đối tượng sẽ `create` / `skip` / `conflict` / `missing_dependency`, kèm `plan_digest` và `expected_versions`, hoàn toàn read-only đối với DB và `uploads/`.

### Phạm vi công việc
- Forward migration `cms/migrations/0020_theme_import_journal.sql`: tạo sẵn cả `import_runs` (cho `G-RECOVERY` ở `S05-T02`), `import_provenance` (unique logical key `(theme_id, sample_key, resource_kind, object_key)` — **không** gồm `revision_id`, lưu `revision_id` + `target_id` làm execution provenance) và `import_tombstones`.
- Tạo `cms/crates/opendash-server/src/storage/import.rs` và `cms/crates/opendash-server/src/routes/import.rs` (hoặc gắn vào `routes/themes.rs`): parser sample descriptor, tính dependency closure (không tự động kéo nhóm chưa chọn mà báo cho người dùng), phát hiện conflict với dữ liệu site-owned hoặc tombstone (mặc định `skip`, không ghi đè), báo rõ publication implication (Post/Page luôn `draft`, thay đổi public trên menu/options cần explicit acknowledgement).
- Endpoint `POST /api/themes/{theme_id}/import/dry-run` (Admin-only, CSRF/Origin, read-only transaction).

### Ngoài phạm vi
Không ghi domain rows hoặc media files khi dry-run; thực thi 3 pha thuộc `S05-T02`.

### Phụ thuộc và điều kiện chặn
`M5-S01-T02`, `M5-S04-T01`, `M5-S09-T01`, `M5-S10-T01` đều phải G5 trước khi triển khai.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/migrations/0020_theme_import_journal.sql` (mới), `cms/crates/opendash-server/src/storage/import.rs` (mới), `cms/crates/opendash-server/src/storage/mod.rs`, `cms/crates/opendash-server/src/routes/import.rs` (mới) + `routes/mod.rs`, tests `cms/crates/opendash-server/tests/m5_s05_t01_import_dry_run.rs`.
- **Không sửa đồng thời:** `S05-T02` đợi task này G5.
- **Contract/API:** Dry-run 100% DB/FS read-only; logical provenance không chứa `revision_id`; mặc định conflict = `skip`.

### Cách triển khai theo contract
Dùng `opendash_server::auth`, `ApiError`, đọc trạng thái qua `storage/settings.rs`, `storage/menu.rs`, `storage/content.rs`, `storage/media.rs`.

### Nghiệm thu
- [ ] Chọn độc lập `pages` / `menus` / `media` / `settings` hoạt động đúng; unselected dependencies được báo cáo rõ thay vì âm thầm nhập thêm.
- [ ] Chạy dry-run 2 lần trên cùng state cho `plan_digest` giống hệt nhau; 0 row mới trong DB và 0 file mới trong `uploads/`.
- [ ] Target đã tồn tại / đã bị user chỉnh / có tombstone được đánh dấu `skip` với lý do rõ ràng, không bao giờ đề xuất overwrite mặc định.
- [ ] Guest/Editor nhận `401`/`403`; bad refs hoặc vượt quota bị từ chối sạch sẽ.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`.
- **Rủi ro/rollback:** Stale plan khi site đổi sau dry-run; `plan_digest` + `expected_versions` sẽ được `S05-T02` kiểm bằng `412`.

### Bàn giao và review
Bàn giao schema `0020`, `ImportPlan` DTO và `storage/import.rs` cho `S05-T02`.

---

## M5-S05-T02 — Selective import execute: G-RECOVERY 3-phase journal và crash recovery

- **Thuộc:** V1 · M5 · M5-S05 · M5-P05
- **Primary Acceptance Slice của capability:** M5-S05 (tiếp `THM-03.1`, không tạo primary thứ hai)
- **Module:** THM-03
- **Capability:** THM-03.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S05-T01` G5)
- **Nguồn quyết định:** [M5 Contract §6 & G-RECOVERY](../../../../contracts/M5_THEME_TEMPLATES.md#g-recovery--g2-agreed-multi-object-import-journal--recovery); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Harbor (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Thực thi nhập mẫu an toàn trước crash/retry/nâng cấp theme (`A1 → A2`): đảm bảo không bao giờ có entry/menu/option đã commit nào trỏ tới file media chưa bền vững, và giữ nguyên dữ liệu người dùng đã chỉnh.

### Mục tiêu và hành vi quan sát được
Execute tạo đúng các nhóm đã chọn (Post/Page luôn `draft`); retry cùng identity hay sau `A1 → A2` là idempotent (không nhân bản); crash giữa Phase 1/2/3 được startup recovery dọn sạch mà không xóa media dùng chung hay để lộ nội dung nửa vời.

### Phạm vi công việc
- Hiện thực giao thức `G-RECOVERY` 3 pha trong `storage/import.rs` & `routes/import.rs`:
  - **Phase 1 (`staged`):** Transaction ngắn kiểm preconditions/`plan_digest`/`expected_versions` (`412` nếu stale) và ghi `import_runs(state='staged')`.
  - **Phase 2 (`files_verified`):** Decode/re-encode raster images, tính SHA-256 `media_id`; tái sử dụng không ghi đè nếu đã có file khớp digest (`already_existed = true`); nếu chưa có thì ghi `uploads/.staging/{run_id}/`, `sync_all()`, drop handles (Windows), rename nguyên tử vào `uploads/media/`, cập nhật `import_runs(state='files_verified')`.
  - **Phase 3 (`committed`):** Một transaction `BEGIN IMMEDIATE` duy nhất gọi các transaction-aware API (`&mut SqliteConnection` của `media`, `content`, `settings`, `menu`) để ghi domain rows + `import_provenance` + `import_runs(state='committed')`.
- Hiện thực startup recovery hook (`app.rs` / `storage/import.rs`): chuyển các run `staged`/`files_verified` mồ côi sang `quarantined`, xóa `uploads/.staging/{run_id}/`, chỉ xóa `{media_id}.*` khi `already_existed == false` và `media_assets` có 0 row cho `media_id`.
- Admin UI trigger dry-run & execute + hiển thị outcome/conflicts (`cms/admin/`).

### Ngoài phạm vi
Không ghi đè nội dung/menu/option user đã chỉnh; không auto-publish Post/Page; không làm backup engine M8.

### Phụ thuộc và điều kiện chặn
`M5-S05-T01` G5 đã merge.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/crates/opendash-server/src/storage/import.rs`, `cms/crates/opendash-server/src/routes/import.rs`, `cms/crates/opendash-server/src/app.rs` (gắn startup recovery hook), `cms/admin/src/` (Import UI), tests `cms/crates/opendash-server/tests/m5_s05_t02_import_execute.rs`.
- **Không sửa đồng thời:** Không tự sửa nội bộ `settings.rs`/`menu.rs`/`content.rs` (chỉ gọi transaction-aware APIs đã bàn giao); `S06-T01` đợi task này G5.
- **Contract/API:** Bất biến `G-RECOVERY`: không có visible row nào trỏ tới media chưa durable; stale plan trả `412`; media import mặc định private trừ khi có saved public ref hợp lệ.

### Cách triển khai theo contract
Tuân thủ `G-RECOVERY` trong `M5_THEME_TEMPLATES.md` và media ACL của `CNT-05`.

### Nghiệm thu
- [ ] Nhập riêng `pages` không kéo theo `menus`/`options` ngoài dependency đã được xác nhận; Post/Page nhập vào ở trạng thái `draft`.
- [ ] Retry lặp lại và retry sau `A1 → A2` không tạo bản sao; target user đã sửa hoặc tombstone được `skip` kèm diagnostic.
- [ ] Crash-cutpoint tests trước/sau Phase 1, Phase 2, Phase 3 và startup recovery: không rò rỉ dữ liệu nửa vời, không xóa nhầm file media đang dùng chung (`already_existed == true` hoặc còn row `media_assets`).
- [ ] Auth/CSRF/Origin, stale `412`, invalid media/quota đều có bằng chứng no-mutation.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, `npm --prefix cms/admin run build`.
- **Rủi ro/rollback:** Lệch trạng thái SQLite vs filesystem nếu bỏ qua `sync_all()`/drop handle trên Windows; test cutpoint bắt buộc chạy thật.

### Bàn giao và review
Bàn giao idempotent import fixture, journal recovery evidence và commit SHA cho `S06-T01`.

---

## M5-S06-T01 — Theme switch/update bảo toàn schema, entries, layout, options và menu

- **Thuộc:** V1 · M5 · M5-S06 · M5-P06
- **Primary Acceptance Slice của capability:** M5-S06 (`THM-03.2` data preservation khi đổi/cập nhật provider)
- **Module:** THM-03
- **Capability:** THM-03.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S03-T02`, `M5-S05-T02`, `M5-S10-T01` G5)
- **Nguồn quyết định:** [M5 Contract §5 & §7](../../../../contracts/M5_THEME_TEMPLATES.md#5-cnt-lifecycle-and-taxonomy-e); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Forge (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Đảm bảo việc đổi theme (`A → B → A`), cập nhật theme (`A1 → A2`), hay thiếu/hỏng file theme trên đĩa không làm mất CPT, fields, term links, entries, overrides, Theme Options hay menus của website — **và Admin/Editor vẫn mở/sửa được nội dung thực tế trong Admin**.

### Mục tiêu và hành vi quan sát được
Trên site có theme A khai báo CPT + fields + category/tag, entries, user overrides, Theme Options và menus: khi chuyển sang theme B (hoặc theme A missing/inactive) rồi restart, Admin/Editor vẫn mở và chỉnh sửa được entry/field/term bình thường (Editor vẫn bị chặn `403` với Theme Options/install/menu/import); khi quay lại A2 mọi ID và giá trị giữ nguyên không nhân bản.

### Phạm vi công việc
- Kiểm tra & gia cố các luồng đọc/sửa Admin/Editor (`storage/content.rs`, `storage/entries.rs`, `storage/page.rs`, `routes/entries.rs`, `routes/pages.rs`) để editability của CPT/entries/terms/overrides hoàn toàn độc lập với `ProviderStatus::Inactive`/`Missing`.
- Bổ sung diagnostics rõ ràng trên Admin khi provider của CPT/override/menu/option đang inactive hoặc missing (không ẩn nội dung đã publish trên public chỉ vì theme gốc inactive, và fallback core an toàn cho template thiếu).
- Xây dựng integration/browser fixture đầy đủ `A1 → B (missing provider) → Editor CRUD → restart → A2`, kiểm tra row counts, stable IDs, và quyền Editor (`403` với các thao tác Admin-only).

### Ngoài phạm vi
Không thêm schema bảng mới; không làm backup/restore M8.

### Phụ thuộc và điều kiện chặn
`M5-S03-T02`, `M5-S05-T02`, `M5-S10-T01` đều đã G5.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/crates/opendash-server/src/storage/content.rs`, `cms/crates/opendash-server/src/storage/entries.rs`, `cms/crates/opendash-server/src/storage/page.rs`, `cms/crates/opendash-server/src/routes/entries.rs`, `cms/admin/src/` (provider-missing diagnostics), tests `cms/crates/opendash-server/tests/m5_s06_t01_preservation.rs`.
- **Không sửa đồng thời:** `S07-T01` đợi task này G5.
- **Contract/API:** Provider availability ≠ data ownership; Editor giữ quyền CRUD entry khi provider inactive nhưng **không** được cấp quyền Admin-only (Theme Options, install/activate, menu mutation, import).

### Cách triển khai theo contract
Dùng `opendash_server::auth` RBAC matrix hiện hành; không tự xóa hoặc reset site-owned data.

### Nghiệm thu
- [ ] `A1 → B → restart → A2` giữ nguyên 100% IDs, field values, term links, overrides, options, menus; không duplicate khi kích hoạt lại A2.
- [ ] Trong lúc A inactive hoặc missing trên đĩa, **Editor** mở, tạo mới và cập nhật được entry/field/category-tag của CPT do A khai báo.
- [ ] **Editor** vẫn nhận `403` khi gọi Theme Options, theme install/activate, hoặc sample import.
- [ ] Public fallback không 500 và không ẩn toàn bộ nội dung published chỉ vì theme gốc inactive.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, `npm --prefix cms/admin run build`.
- **Rủi ro/rollback:** Lọc nhầm entry/CPT theo `active_theme_id` ở query list; kiểm tra tất cả read/write paths Admin.

### Bàn giao và review
Bàn giao preservation fixture `A1 → B → A2` và evidence SHA cho `S07-T01` và `S08-T01`.

---

## M5-S07-T01 — Missing-provider fallback, adversarial C-SEC regression và public accessibility

- **Thuộc:** V1 · M5 · M5-S07 · M5-P07
- **Primary Acceptance Slice của capability:** M5-S07 (`THM-02.2` public fallback & hardening)
- **Module:** THM-02
- **Capability:** THM-02.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S06-T01` G5)
- **Nguồn quyết định:** [M5 Contract §3, C-SEC & §7](../../../../contracts/M5_THEME_TEMPLATES.md#c-sec--g2-agreed-minijinja-305-safe-subset--contextual-validator); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Harbor (dự kiến) / Review độc lập hoặc self-review theo ADR-0010.

### Tổng quan và giá trị
Gia cố lớp phòng thủ thứ hai cho public delivery sau khi toàn bộ các luồng theme, override, option, menu và preservation đã ghép nối: bảo đảm mọi hỏng hóc package/role/provider đều fallback an toàn (không `500`), chạy bộ negative corpus đối kháng cho `C-SEC`/ACL/CSRF/CSP, và kiểm chứng accessibility cơ bản trên trình duyệt.

### Mục tiêu và hành vi quan sát được
Thiếu/hỏng package, `.complete` hoặc role vẫn render core fallback sạch sẽ (không lộ path/stack); toàn bộ adversarial XSS/CSS/CSP/recursion/fuel/ACL corpus bị chặn; public page đạt smoke accessibility (landmarks, heading, keyboard focus, pagination, không console error).

### Phạm vi công việc
- Gia cố precedence fallback trong `page.rs`, `content_render.rs`, `theme.rs` khi file revision trên đĩa bị mất/hỏng sau khi activate (tự động fallback core renderer, phát diagnostic Admin an toàn, không dùng HTML của theme inactive).
- Xây dựng bộ test đối kháng mở rộng (`m5_s07_t01_security_fallback.rs`): forged filters, nested/recursive includes, fuel bomb, oversized output >512 KiB, SVG/MathML/script/event-handler/CSS breakout/`@import`, draft & private media ACL leak qua Theme Options hoặc `RouteContext`, CSRF/Origin/CAS matrix.
- Kiểm tra public accessibility smoke trên trình duyệt (landmarks landmark `header`/`main`/`nav`/`footer`, heading hierarchy, keyboard Tab navigation qua menu & pagination, 0 console errors).

### Ngoài phạm vi
Không xây template marketplace hay tính năng SEO M7; không thay test thực thi bằng checklist.

### Phụ thuộc và điều kiện chặn
`M5-S06-T01` G5 đã merge.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/crates/opendash-server/src/page.rs`, `cms/crates/opendash-server/src/content_render.rs`, `cms/crates/opendash-server/src/theme.rs`, `cms/crates/opendash-server/tests/m5_s07_t01_security_fallback.rs` (mới), browser/a11y smoke script/report.
- **Không sửa đồng thời:** `S08-T01` đợi task này G5.
- **Contract/API:** Public renderer tuyệt đối không dùng theme inactive; CSP và Preview `sandbox=""` giữ nguyên; không leak path/stack/draft/private media.

### Cách triển khai theo contract
Mở rộng trực tiếp `template_sec.rs` và core fallback đã có từ `S03-T01`.

### Nghiệm thu
- [ ] Thiếu/hỏng revision, `.complete`, hoặc role fallback về core an toàn (`200` với nội dung hợp lệ hoặc `404` chuẩn, không `500`, không lộ đường dẫn host).
- [ ] 100% adversarial security corpus (`C-SEC`, draft/private media ACL, CSRF/Origin/412) pass.
- [ ] Browser accessibility smoke pass: keyboard navigation dùng được qua header menu & archive pagination, landmarks/headings hợp lệ, 0 console errors.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, headless browser a11y/CSP smoke.
- **Rủi ro/rollback:** Bất kỳ P1/P2 security gap nào đều block merge tới khi khắc phục triệt để.

### Bàn giao và review
Bàn giao failure matrix, negative corpus và a11y evidence cho `S08-T01`.

---

## M5-S08-T01 — E2E public delivery, selected sample import và preservation đa OS (Windows + macOS)

- **Thuộc:** V1 · M5 · M5-S08 · M5-P08
- **Primary Acceptance Slice của capability:** Không cấp primary mới (`M5-S03` giữ primary `THM-02.1`; `M5-S08` là cross-module E2E verification)
- **Module:** THM-02
- **Capability:** THM-02.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** BACKLOG (chờ `M5-S07-T01` và toàn bộ M5 predecessors G5)
- **Nguồn quyết định:** [M5 Contract §7](../../../../contracts/M5_THEME_TEMPLATES.md#7-ownership-sequencing-and-release-fixture-h); [ADR-0012](../../../../decisions/ADR-0012-m5-theme-templates.md).
- **Người triển khai / review:** Forge (accountable implementer & Windows G5) + Harbor (independent native macOS G5) / Coordinator Atlas/Nova.

### Tổng quan và giá trị
Xác nhận toàn bộ milestone M5 hoạt động end-to-end trên **cùng một immutable source SHA** ở cả Windows và macOS với binary native thật, snapshot nâng cấp từ M4 (`a176414`), và đầy đủ negative matrix trước khi đóng Milestone #6.

### Mục tiêu và hành vi quan sát được
Chạy deterministic fixture trên site copy M4 (`v1` NULL/explicit active): nâng cấp migration `0015`–`0020`, cài A v2 (CPT+fields+category/tag), selective import + retry, tạo/sửa `RouteContext` override, Theme Options, menu, Preview/Save/Publish, đổi sang B (A missing) + Editor CRUD + restart + quay lại A2, kiểm tra public delivery + negative corpus + rollback pre-M5 snapshot với binary M4 cũ.

### Phạm vi công việc
- Tạo deterministic E2E harness/fixture trong `cms/crates/opendash-server/tests/m5_s08_t01_e2e_delivery.rs` (và runner riêng cho từng OS nếu cần, không ghi đè cùng file báo cáo).
- Forge chạy và ghi bằng chứng Windows vào `docs/verification/2026-11-13-m5-g5-windows.md`; Harbor chạy độc lập trên native macOS tại **cùng source SHA** và ghi vào `docs/verification/2026-11-13-m5-g5-macos.md` (tách biệt file, không hai writer cùng một file).
- Kiểm chứng rollback pre-M5 snapshot bằng binary M4 gốc (`a176414`), browser E2E (public header/footer/menu/options, `RouteContext`, fallback, 0 console error), và GitHub CI (`governance-ubuntu-latest`, `governance-windows-latest`).

### Ngoài phạm vi
Không sửa code sản phẩm trong task này (mọi bug phát sinh mở issue fix riêng trước khi chốt common source SHA); không nghiệm thu M6/M7/M8.

### Phụ thuộc và điều kiện chặn
Toàn bộ `M5-S01-T01`..`M5-S10-T01` và `M5-S07-T01` đã G5 trên `main`; `M4` release handoff `a176414` sẵn có.

### Kỹ thuật và file ownership
- **File được sửa/tạo độc quyền:** `cms/crates/opendash-server/tests/m5_s08_t01_e2e_delivery.rs` (Forge), `docs/verification/2026-11-13-m5-g5-windows.md` (Forge), `docs/verification/2026-11-13-m5-g5-macos.md` (Harbor).
- **Không sửa đồng thời:** Không sửa source code core trong PR evidence; báo cáo Windows và macOS tách file hoàn toàn.
- **Contract/API:** Cùng immutable source SHA cho Windows và macOS; ghi rõ binary hash, OS, lệnh chạy, PASS/FAIL/NOT_RUN; không suy ra VERIFIED chỉ từ CI hay một OS.

### Cách triển khai theo contract
Tuân thủ [verification/README.md](../../../../verification/README.md) và `ADR-0010`/`ADR-0012`.

### Nghiệm thu
- [ ] Legacy v1 M3/M4 và ZIP v2 themes A/B install/activate/update/rollback hoạt động trên bản copy nâng cấp `0015`–`0020`, kèm kiểm thử restore pre-M5 snapshot với binary M4 `a176414`.
- [ ] CPT/fields/category-tag, selected sample import/retry, `RouteContext` override, Theme Options (`image_ref` ACL), menu slot và Preview/Save/Publish chạy trọn vẹn trên server + browser (0 console error).
- [ ] `A1 → B (missing) → Editor CRUD → restart → A2` bảo toàn 100% IDs, dữ liệu và quyền RBAC.
- [ ] Negative matrix (`C-SEC` XSS/CSS/CSP, URL collisions, ZIP traversal, unauthorized Preview, CSRF/CAS, import crash recovery) pass trên cả Windows và macOS tại cùng source SHA.
- [ ] Cả `2026-11-13-m5-g5-windows.md` và `2026-11-13-m5-g5-macos.md` được ghi đầy đủ SHA/binary/commands/results trước khi đề xuất đóng Milestone #6.

### Kiểm thử, rủi ro và rollback
- **Lệnh dự kiến:** `cargo build --manifest-path cms/Cargo.toml --locked`, `cargo test --manifest-path cms/Cargo.toml --workspace --locked`, `npm --prefix cms/admin run build`, browser E2E trên Windows và macOS.
- **Rủi ro/rollback:** Không nâng trạng thái M5 thành `VERIFIED` nếu thiếu bất kỳ OS nào hoặc có test `NOT_RUN`/FAIL chưa giải quyết.

### Bàn giao và review
Đăng 2 báo cáo verification và đề xuất đóng GitHub Milestone #6 (M5) sau khi xác nhận đầy đủ bằng chứng.
