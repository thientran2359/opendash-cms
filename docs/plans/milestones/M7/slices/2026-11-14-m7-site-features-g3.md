# M7 — G3: Site Features và Video (14 task / 8 candidate)

**Coordinator / Final Decision:** Atlas · **Date:** 2026-11-14 · **Baseline:** main@4b60388683a01df8d6332c88449b4fa6821ef5fc.
**G2:** AGREED · **G3:** READY (đặc tả có hiệu lực sau merge lên main).
**Implementation:** NOT_STARTED · **Verification:** NOT_RUN · **Project:** chưa phát hành Issue, không task nào được tự nhận Ready từ tài liệu.
Nguồn: [PRD](../../../../product/PRODUCT_REQUIREMENTS.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md), [contract](../../../../contracts/M7_SITE_FEATURES.md), [G1 inventory](../../../../../.agents/research/2026-11-14-m7-site-features-g1.md), [Task Plan](../TASK_PLAN.md), [Planning Workflow](../../../PLANNING_WORKFLOW.md), [task template](../../../templates/SLICE_TASK.md).

## Contract, acceptance và ngoài phạm vi toàn milestone

Làm đủ P01–P08 trên CMS hiện tại: settings/menu, canonical URL/SEO/sitemap, form do người dùng tạo trong Builder và Admin submissions/SMTP, video library/Range/core video. Thành công cần UI/public behavior + fresh/upgrade/preservation/security/native/browser evidence, không chỉ backend.
Không làm đa ngữ, advanced/repeater forms, arbitrary plugin UI, scheduled jobs/retries, transcoding, hard media delete/reconcile, Linux deployment hoặc backup engine M8.

Mọi task Full-Track; G2 AGREED/G3 READY là **đặc tả**, trạng thái thực thi PLANNED/BLOCKED_DEPENDENCY. M5#157/M6#188 phải G5 thật trước ANY implementation Ready. Owner giao Nova hoàn tất M6 không đồng nghĩa Nova đã hoàn tất, không dispatch/chiếm M6.
Chỉ phát hành implementation Issues từ immutable merged G3 commit, coordinator xác minh dependency/ownership/Project Ready; không tạo live queue ở .agents/tasks.

## Coverage, capability và Primary Acceptance Slice

| Candidate | Capability / nghiệm thu chính | Task | Owner dự kiến |
| --- | --- | --- | --- |
| M7-P01 | SITE-01.2 / M7-S01 | M7-S01-T01 | Harbor |
| M7-P02 | SITE-01.3 / M7-S02 | M7-S02-T01 | Harbor |
| M7-P03 | SITE-02.1 / M7-S03 URL; SITE-02.2 / M7-S03 metadata | M7-S03-T01, M7-S03-T02 | Forge |
| M7-P04 | SITE-02.3 / M7-S04 | M7-S04-T01 | Harbor |
| M7-P05 | SITE-03.1 / M7-S05 persistence; SITE-03.2 / M7-S05 usable Builder | M7-S05-T01, M7-S05-T02 | Forge |
| M7-P06 | SITE-03.3 / M7-S06 | M7-S06-T01 | Forge |
| M7-P08 | CNT-05.5 / M7-S07 ingest; CNT-05.6 / M7-S07 serving | M7-S07-T01, M7-S07-T02 | Harbor |
| M7-P07 | Regression toàn milestone, không cấp lại capability | M7-S08-T01..T04 | Nova, Forge, Harbor, Atlas |

## Thứ tự, migration và file ownership

Migration tuần tự0025→0031 theo [contract §1](../../../../contracts/M7_SITE_FEATURES.md#1-global-constraints-and-persistence). Existing0001–0024 bất biến.
Đường găng an toàn: S01→S02→S03-T01→S03-T02→S04→S05-T01→S05-T02→S06→S07-T01→S07-T02→S08-T01→(S08-T02 song song S08-T03)→S08-T04.
Có thể tối ưu lịch sau bằng handoff ghi trên Issue; không bỏ migration dependency/đổi G2 hoặc cho hai writer chung file.

**Shared wiring files** (chỉ task hiện tại được coordinator giao seam): cms/crates/opendash-server/src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; cms/crates/opendash-core/src/lib.rs; cms/admin/src/App.tsx, api/index.ts. Chỉ thêm module/routes/nav/state phục vụ slice hiện tại; không refactor unrelated.
Danh sách file riêng từng task dưới đây cộng shared wiring khi cần là ownership cho lần triển khai; mọi file ngoài danh sách cần coordinator giao bổ sung trước khi sửa. Auth/ZIP/package/plugin runtime không thuộc ownership M7; dùng Base APIs, issue bug riêng nếu phát hiện thiếu. File browser report cùng source không ghi đồng thời.

## Kiểm chứng và rollback chung

Mỗi task implementation thêm integration test đúng tên dưới đây, chạy với binary thật trên site tạm; tests phải phân biệt public/preview/save, permission và data preservation theo acceptance, không chỉ mirror DTO.
Lệnh từ root: `cargo build --manifest-path cms/Cargo.toml --locked`; `cargo test --manifest-path cms/Cargo.toml --test <test-name> --locked`; `npm --prefix cms/admin run typecheck`; `npm --prefix cms/admin run build` khi sửa UI. Không dùng --locked trước lần cập nhật Cargo.lock có chủ ý cho new dependency; sau lock phải build/test --locked.
Governance bắt buộc: `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test`, `python cms/scripts/check-standards.py`, `python .agents/scripts/check-agent-artifacts.py`, `git diff --check`. S08 chạy toàn workspace và native browser/TLS/video/ACL trên cảOS.
Browser runner khi có: `python cms/scripts/m7_browser_e2e.py --base-url <local-origin> --site-root <isolated-site> --artifact-dir <outside-git>`. Test report ghi exact command/path/SHA/OS/hash, không commit PII/live credentials/private runtime.
Migration dùng copy DB/files và pre-M7 binary snapshot, không rollback bằng git revert. Security/data FAIL hoặc required NOT_RUN chặn task G5; issue fix riêng, không hạ acceptance.
**Stop-and-Propose:** PRD/security/ABI/applied migration/new public contract thay đổi cần Atlas quyết định + ADR/contract delta; internal refinement ghi Design Delta/test, không tự upgrade dependency.
**Bàn giao chung:** commit/PR head+base, file diff, commands PASS/FAIL/NOT_RUN, native/browser limits, migration snapshot path và regression evidence; Issue/Project là nguồn live, không suy Done từ PR merged.

Type notation: OptionalString nghĩa JSON string hoặc null (Rust Option of String); SitemapRouteList nghĩa bounded Vec of SitemapRoute, không tạo type storage/framework mới.

## G3 task specifications

### M7-S01-T01 — Site-public settings và ngôn ngữ/indexability

- **Thuộc:** V1 · M7 · M7-S01- · M7-P01
- **Module:** SITE-01
- **Capability:** SITE-01.2
- **Primary Acceptance Slice của capability:** M7-S01-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §2](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Site-public settings và ngôn ngữ/indexability giúp hoàn thiện M7-P01 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Singleton public_origin/content_language/indexing_enabled, CAS API, form Admin và prompt khi thiếu origin; schema/default nâng cấp giữ General/provider options.

#### Phạm vi công việc

Singleton public_origin/content_language/indexing_enabled, CAS API, form Admin và prompt khi thiếu origin; schema/default nâng cấp giữ General/provider options.
Migration: `cms/migrations/0025_site_public_settings.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không sửa theme/plugin options, cấu hình SMTP hoặc deploy proxy.

#### Phụ thuộc và điều kiện chặn

M5 #157 G5 + M6 #188 G5; không có predecessor M7. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0025_site_public_settings.sql`
- `cms/crates/opendash-server/src/storage/settings.rs`
- `cms/crates/opendash-server/src/routes/settings.rs`
- `cms/admin/src/features/system/GeneralSettingsPanel.tsx`
- `cms/admin/src/api/settings.ts`
- `cms/crates/opendash-server/tests/m7_s01_t01_settings.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

Tạo PublicSiteSettings/PublicSiteSettingsInput và storage::settings::get_public/save_public(db,input,expected_version), kết quả Saved/Stale; API /api/admin/settings/public. Không thay payload/version của General.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Fresh/upgrade backfill vi/null/true, dữ liệu General/options cũ nguyên vẹn qua restart.
- [ ] Origin có credentials/path/query/fragment/control bị từ chối; language vượt35 bytes bị422; không dùng Host/Forwarded làm canonical.
- [ ] Admin Lưu cập nhật settings, Editor403, thiếu token403,401/412 giữ form; chưa có origin không làm hỏng website.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s01_t01_settings`: `cargo test --manifest-path cms/Cargo.toml --test m7_s01_t01_settings --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s01-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S02-T01 — Quản lý menu, preview và gắn slot có CAS

- **Thuộc:** V1 · M7 · M7-S02- · M7-P02
- **Module:** SITE-01
- **Capability:** SITE-01.3
- **Primary Acceptance Slice của capability:** M7-S02-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §3](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Quản lý menu, preview và gắn slot có CAS giúp hoàn thiện M7-P02 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Toàn bộ Admin menu routes/UI, target chooser/reorder/reparent/preview, bảo toàn origin_provenance và site-owned IDs; version cho mọi assignment writer.

#### Phạm vi công việc

Toàn bộ Admin menu routes/UI, target chooser/reorder/reparent/preview, bảo toàn origin_provenance và site-owned IDs; version cho mọi assignment writer.
Migration: `cms/migrations/0026_menu_slot_versions.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không tạo persistence/menu renderer thứ hai; không thêm custom link path registry trước S03.

#### Phụ thuộc và điều kiện chặn

M7-S01-T01 G5 (migration và shared App/settings handoff). Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0026_menu_slot_versions.sql`
- `cms/crates/opendash-server/src/storage/menu.rs`
- `cms/crates/opendash-server/src/storage/import.rs`
- `cms/crates/opendash-server/src/routes/menu.rs`
- `cms/admin/src/api/menus.ts`
- `cms/admin/src/features/menus/MenusPanel.tsx`
- `cms/crates/opendash-server/tests/m7_s02_t01_menus.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

Bọc lại create_menu_in_tx/get_menu_in_tx/replace_menu_items_in_tx/assign_menu_slot_in_tx; thêm list/update/delete và preview_menu(db,menu_id,draft)->MenuViewModel không write. GET/PUT slot trả aggregate version; import dùng chung version bump.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Menu nhập từ theme sửa qua UI giữ100% IDs/provenance, import retry không ghi đè; A→B→A/restart giữ tree/assignments.
- [ ] 200 items/depth8, cycle/cross-menu parent/duplicate IDs/invalid URL bị chặn nguyên tử; unpublished target không public.
- [ ] Hai tab edit menu/slot hoặc import cạnh tranh cho412; thiếu If-Match428; Preview không đổi DB/public.
- [ ] Slot sai revision hoặc thay active theme giữa validate/commit bị conflict; delete menu cập nhật FK/slot versions đồng bộ.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s02_t01_menus`: `cargo test --manifest-path cms/Cargo.toml --test m7_s02_t01_menus --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s02-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S03-T01 — Canonical entry paths và redirect theo ID

- **Thuộc:** V1 · M7 · M7-S03- · M7-P03
- **Module:** SITE-02
- **Capability:** SITE-02.1
- **Primary Acceptance Slice của capability:** M7-S03-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Forge dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §4](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Canonical entry paths và redirect theo ID giúp hoàn thiện M7-P03 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Migrate paths/redirects/SEO/Home/ref schemas, shared claim transaction, custom fallback route/301, cập nhật mọi URL consumer + default slug/CPT claim/import paths qua core helper; preserve M6 Save validator ordering.

#### Phạm vi công việc

Migrate paths/redirects/SEO/Home/ref schemas, shared claim transaction, custom fallback route/301, cập nhật mọi URL consumer + default slug/CPT claim/import paths qua core helper; preserve M6 Save validator ordering.
Migration: `cms/migrations/0027_seo_routes.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không custom taxonomy/archive URL, arbitrary redirect destinations hoặc script/head metadata UI.

#### Phụ thuộc và điều kiện chặn

M7-S02-T01 G5; bàn giao storage/menu.rs, app.rs và import.rs. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0027_seo_routes.sql`
- `cms/crates/opendash-server/src/page.rs`
- `cms/crates/opendash-server/src/content_render.rs`
- `cms/crates/opendash-server/src/routes/seo.rs`
- `cms/crates/opendash-server/src/seo.rs`
- `cms/crates/opendash-server/src/storage/seo.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/storage/menu.rs`
- `cms/crates/opendash-server/src/storage/builder_binding.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/import.rs`
- `cms/crates/opendash-server/src/public_context.rs`
- `cms/crates/opendash-server/src/routes/public_content.rs`
- `cms/crates/opendash-server/src/routes/public.rs`
- `cms/crates/opendash-server/tests/m7_s03_t01_routes.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

storage::seo::set_entry_route_in_tx(&mut SqliteConnection,id,expected_version,Option<&str>), resolve_entry_path_in_tx(conn,id)->ApiResult<OptionalString>, validate_route_claim_in_tx(conn,claim). Tạo sidecar SEO/Home schemas và media ref projection cho S03-T02, T01 expose GET/PUT /api/admin/content-types/{type_key}/entries/{id}/route với {custom_path}, ENTRY If-Match và entry_version response để real-binary test không bypass HTTP; T02 full SEO endpoint dùng cùng storage helper. Pass resolved URLs từ storage vào EntryView/list projection, tuyệt đối không SQL trong renderer.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Collision custom/default/alias/CPT root và concurrent claim409, transaction không để partial rows; default/custom entry draft vẫn giữ claim.
- [ ] Đổi slug/custom URL301 một hop đến current ID; unpublished/delete404 không Location/OG/data; reclaim alias chỉ cùng entry.
- [ ] Menu/list/button/archive/templates resolve cùng URL; import/CPT creation không bypass reserved root/collision.
- [ ] M5 query/paging/default route và M6 before/after-save/veto còn pass; không decode path hai lần/open redirect.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s03_t01_routes`: `cargo test --manifest-path cms/Cargo.toml --test m7_s03_t01_routes --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s03-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S03-T02 — SEO metadata, head projection và form Admin

- **Thuộc:** V1 · M7 · M7-S03- · M7-P03
- **Module:** SITE-02
- **Capability:** SITE-02.2
- **Primary Acceptance Slice của capability:** M7-S03-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Forge dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §5](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

SEO metadata, head projection và form Admin giúp hoàn thiện M7-P03 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

SEO UI Home/Entry, plain metadata/canonical/noindex/OG image, trusted head cho legacy/role/override/Home/archive/taxonomy/error/preview; typed ref ACL.

#### Phạm vi công việc

SEO UI Home/Entry, plain metadata/canonical/noindex/OG image, trusted head cho legacy/role/override/Home/archive/taxonomy/error/preview; typed ref ACL.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không raw HTML/schema.org, taxonomy metadata CRUD, media video làm OG hoặc real publish mới.

#### Phụ thuộc và điều kiện chặn

M7-S03-T01 G5; độc quyền seo/storage/entries/render seams được bàn giao. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/seo.rs`
- `cms/crates/opendash-server/src/storage/seo.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/content_render.rs`
- `cms/crates/opendash-server/src/document.rs`
- `cms/crates/opendash-server/src/template_sec.rs`
- `cms/crates/opendash-server/src/routes/seo.rs`
- `cms/crates/opendash-server/src/routes/public.rs`
- `cms/crates/opendash-server/src/routes/public_content.rs`
- `cms/admin/src/api/seo.ts`
- `cms/admin/src/features/seo/SeoPanel.tsx`
- `cms/admin/src/features/content/ContentWorkspace.tsx`
- `cms/crates/opendash-server/tests/m7_s03_t02_seo.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

SeoView typed object; compose_document_with_seo(rendered_layout,&SeoView)->Result<String,ApiError>; load/save_entry_seo uses ENTRY version; Home riêng version. GET/PUT Entry SEO và Home theo contract.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Mỗi document đúng1 head/title/canonical/robots controlled tag, lang từ settings; template styles/header/footer không mất. Bounded structural normalizer kiểm mixed-case/body attributes/comment/quoted delimiters/style end-head string/duplicate envelope và safe fallback diagnostic; old literal compose không bypass.
- [ ] XSS title/description/canonical/image escapes đúng ngữ cảnh; external canonical không fetch/redirect; malformed payload422.
- [ ] Draft/preview/errors noindex; OG/media không public từ draft hoặc inactive scope; publish/unpublish/removal cập nhật refs nguyên tử.
- [ ] Editor SEO entry được nhưng Home/settings403; SEO/content two-tab412 và giữ draft; thiếu public_origin không Host fallback.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s03_t02_seo`: `cargo test --manifest-path cms/Cargo.toml --test m7_s03_t02_seo --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s03-t02-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S04-T01 — Robots và sitemap động không lộ nội dung riêng

- **Thuộc:** V1 · M7 · M7-S04- · M7-P04
- **Module:** SITE-02
- **Capability:** SITE-02.3
- **Primary Acceptance Slice của capability:** M7-S04-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §5](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Robots và sitemap động không lộ nội dung riêng giúp hoàn thiện M7-P04 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

GET robots/index/parts, snapshot SQL ORDER/window, canonical/indexability policy, bounded output và quota diagnostic.

#### Phạm vi công việc

GET robots/index/parts, snapshot SQL ORDER/window, canonical/indexability policy, bounded output và quota diagnostic.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không persistent cache/background invalidation/search-console submission hoặc video sitemap extension.

#### Phụ thuộc và điều kiện chặn

M7-S03-T02 G5; bàn giao seo/routes rendering và app.rs. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/storage/sitemap.rs`
- `cms/crates/opendash-server/src/routes/sitemap.rs`
- `cms/crates/opendash-server/src/seo.rs`
- `cms/crates/opendash-server/tests/m7_s04_t01_sitemap.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

storage::sitemap::list_indexable_routes_in_tx(conn,offset,limit)->SitemapRouteList; count_indexable_routes_in_tx; seo::render_sitemap_index/part(settings,typed_routes)->String với fixed XML escaping.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] 5000/5001 URLs chia1/2 parts deterministic; >1M explicit503, không truncate; invalid part404.
- [ ] UTF8 XML escaped và same-origin loc; draft/noindex/redirect/alias/filter/private media/external canonical bị loại.
- [ ] Save/publish/delete/term/settings sau commit phản ánh request tiếp theo; mỗi response dùng một read snapshot.
- [ ] Origin null503, indexing off empty index+Disallow; spoof Host không đổi absolute URLs; lastmod không dùng thời gian generate.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s04_t01_sitemap`: `cargo test --manifest-path cms/Cargo.toml --test m7_s04_t01_sitemap --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s04-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S05-T01 — Form schema, submission service và chống lạm dụng

- **Thuộc:** V1 · M7 · M7-S05- · M7-P05
- **Module:** SITE-03
- **Capability:** SITE-03.1
- **Primary Acceptance Slice của capability:** M7-S05-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Forge dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §6](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Form schema, submission service và chống lạm dụng giúp hoàn thiện M7-P05 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Schema/CRUD/versioning, schema snapshots, native POST decode/validation, strict Origin middleware exception, rate/quota/dedup transaction; pending notification seam cho S06.

#### Phạm vi công việc

Schema/CRUD/versioning, schema snapshots, native POST decode/validation, strict Origin middleware exception, rate/quota/dedup transaction; pending notification seam cho S06.
Migration: `cms/migrations/0028_contact_forms.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không SMTP network, CAPTCHA/upload/auto-retention hoặc guest form actions.

#### Phụ thuộc và điều kiện chặn

M7-S04-T01 G5; migration0027 và app.rs/seo seams đã bàn giao. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0028_contact_forms.sql`
- `cms/crates/opendash-core/src/contact.rs`
- `cms/crates/opendash-server/src/contact.rs`
- `cms/crates/opendash-server/src/storage/contact.rs`
- `cms/crates/opendash-server/src/routes/contact.rs`
- `cms/crates/opendash-server/tests/m7_s05_t01_contact.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

core::contact::{FormSchema,FormField,FormKind}; contact::submit(db,peer,origin,SubmitInput)->Receipt; SubmitInput=form_id/version/nonce/values/honeypot, Receipt chỉ dùng nội bộ; form CRUD và submission read/delete qua storage.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] 20 fields/text-email-textarea-select-checkbox limits server enforced; reserved keys rejected, optional missing/checkbox wire rules pinned; repeated/unknown keys422; stale form_version409.
- [ ] 5/min20/hour/global100 +10k buckets +4 concurrent admission,429Retry-After trước DB write; forged Forwarded không đổi peer key.
- [ ] Commit submission+snapshot+dedup atomic; duplicate same nonce/payload không nhân bản, khác payload409; quota10k không xóa cũ.
- [ ] Null/foreign/missing Origin403; Admin auth/CSRF/Editor403; honeypot không persist; errors/logs không lộ inbox/credentials.
- [ ] Schema edit/disable/delete/restart giữ lịch sử; form có submission delete409; Admin explicit submission delete hoạt động.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s05_t01_contact`: `cargo test --manifest-path cms/Cargo.toml --test m7_s05_t01_contact --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s05-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S05-T02 — Form designer, core/contact_form và inbox Admin

- **Thuộc:** V1 · M7 · M7-S05- · M7-P05
- **Module:** SITE-03
- **Capability:** SITE-03.2
- **Primary Acceptance Slice của capability:** M7-S05-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Forge dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §6](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Form designer, core/contact_form và inbox Admin giúp hoàn thiện M7-P05 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Form creation/edit in Admin, Builder select/place form homepage/Page override, native no-JS submit/receipt/error pages, inbox safe text; CSP per live-form context only.

#### Phạm vi công việc

Form creation/edit in Admin, Builder select/place form homepage/Page override, native no-JS submit/receipt/error pages, inbox safe text; CSP per live-form context only.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không script runtime/form builder arbitrary HTML, plugin custom UI hoặc multi-step editor.

#### Phụ thuộc và điều kiện chặn

M7-S05-T01 G5; S04-T01 đã bàn giao public/render seams. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-core/src/builder.rs`
- `cms/crates/opendash-server/src/page.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/overrides.rs`
- `cms/crates/opendash-server/src/content_render.rs`
- `cms/crates/opendash-server/src/routes/public.rs`
- `cms/crates/opendash-server/src/routes/public_content.rs`
- `cms/admin/src/api/contact.ts`
- `cms/admin/src/features/contact/ContactPanel.tsx`
- `cms/admin/src/builder/document.ts`
- `cms/admin/src/builder/BuilderInspector.tsx`
- `cms/admin/src/builder/BuilderWorkspace.tsx`
- `cms/crates/opendash-server/tests/m7_s05_t02_form_ui.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

KnownElement::ContactForm(ContactFormProps{form_id,submit_label}); page render consumes resolved active FormView and generated nonce, preview inert. ContactPanel designer+submissions inspector; no user-return-URL prop.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Admin tạo form khác fixture mặc định, đặt trên page, Save public submit không JS; inbox hiển thị đúng schema snapshot.
- [ ] Preview unsaved form không gửi/cấp quyền public; sandbox/scripts vẫn khóa; missing/disabled form giữ AST+diagnostic.
- [ ] Browser validation422 giữ giá trị và nhãn/lỗi accessible; successful303 tới fixed /api/public/contact/received không lộ nonce/PII trong URL.
- [ ] Theme switch/plugin disable/restart giữ form/submissions/site layout;401/412 giữ editor drafts; keyboard thao tác được.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s05_t02_form_ui`: `cargo test --manifest-path cms/Cargo.toml --test m7_s05_t02_form_ui --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s05-t02-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S06-T01 — SMTP settings, private secret store và notification attempts

- **Thuộc:** V1 · M7 · M7-S06- · M7-P06
- **Module:** SITE-03
- **Capability:** SITE-03.3
- **Primary Acceptance Slice của capability:** M7-S06-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Forge dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §7](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

SMTP settings, private secret store và notification attempts giúp hoàn thiện M7-P06 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Pin Lettre/target Windows APIs, ACL private file lifecycle/CAS, settings UI/password keep-replace-clear, durable post-commit attempt claim/status/startup recovery, explicit retry and TLS fixture.

#### Phạm vi công việc

Pin Lettre/target Windows APIs, ACL private file lifecycle/CAS, settings UI/password keep-replace-clear, durable post-commit attempt claim/status/startup recovery, explicit retry and TLS fixture.
Migration: `cms/migrations/0029_smtp_notifications.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không cron/outbox worker/auto retry, SMTP arbitrary recipient, external live mail hoặc encryption-at-rest claim.

#### Phụ thuộc và điều kiện chặn

M7-S05-T02 G5; migration0028 và contact/render/App handoff. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0029_smtp_notifications.sql`
- `cms/Cargo.toml`
- `cms/Cargo.lock`
- `cms/crates/opendash-server/Cargo.toml`
- `cms/crates/opendash-server/src/smtp.rs`
- `cms/crates/opendash-server/src/secrets.rs`
- `cms/crates/opendash-server/src/storage/smtp.rs`
- `cms/crates/opendash-server/src/routes/smtp.rs`
- `cms/crates/opendash-server/src/contact.rs`
- `cms/crates/opendash-server/src/paths.rs`
- `cms/admin/src/api/smtp.ts`
- `cms/admin/src/features/contact/SmtpSettingsPanel.tsx`
- `cms/crates/opendash-server/tests/m7_s06_t01_smtp.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

SecretStore::write_smtp_secret(password)->SecretRef + read private only; SmtpSender async transport abstraction (real Lettre/test deterministic TLS fixture), send_notification(submission_id,expected_attempt_version,ack_duplicate)->AttemptOutcome; SMTP GET/PUT/test + manual notify.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Settings GET/log/error/DB settings không chứa raw password; pre-write ACL Unix0700/0600 + Win protected DACL/reparse rejection, failure enable blocked.
- [ ] Stale CAS không đổi active secret; crash orphan private; restart đọc đúng pointer, retained old secret hỗ trợ snapshot.
- [ ] Local TLS fixture verify valid cert/hostname, STARTTLS downgrade/bad cert/header injection rejected; no product insecure mode.
- [ ] Mail failed/timeout/cancel/crash-after-DATA không mất submission, preflight failed/opaque transport unknown chính xác; replay không resend.
- [ ] 10s total deadline inclDNS/4 permits/max3 attempts/manual unknown ack; pending startup không gửi; test chỉ configured recipient.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s06_t01_smtp`: `cargo test --manifest-path cms/Cargo.toml --test m7_s06_t01_smtp --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s06-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S07-T01 — Video metadata schema và upload MP4 có giới hạn

- **Thuộc:** V1 · M7 · M7-S07- · M7-P08
- **Module:** CNT-05
- **Capability:** CNT-05.5
- **Primary Acceptance Slice của capability:** M7-S07-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §8](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Video metadata schema và upload MP4 có giới hạn giúp hoàn thiện M7-P08 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Preserving media table rebuild, strict bounded MP4 validator, streaming staging/durable write+dedup metadata, upload API; typed image-ref validation all entry/theme/M6 paths.

#### Phạm vi công việc

Preserving media table rebuild, strict bounded MP4 validator, streaming staging/durable write+dedup metadata, upload API; typed image-ref validation all entry/theme/M6 paths.
Migration: `cms/migrations/0030_media_video.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không ffmpeg/transcode/generic MP4 parser/frame thumbnail hoặc CPT field kind mới.

#### Phụ thuộc và điều kiện chặn

M7-S06-T01 G5 (migration numbering0029 và Cargo/app handoff). Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0030_media_video.sql`
- `cms/crates/opendash-server/src/media_video.rs`
- `cms/crates/opendash-server/src/media.rs`
- `cms/crates/opendash-server/src/storage/media.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/settings.rs`
- `cms/crates/opendash-server/src/routes/media.rs`
- `cms/admin/src/api/media.ts`
- `cms/admin/src/features/media/library/fileValidation.ts`
- `cms/crates/opendash-server/tests/m7_s07_t01_video_upload.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

ValidatedVideo{sha256,size,width,height,duration_ms,video_codec,audio_codec}; inspect_mp4 bounded scanner; MediaAsset.kind/duration/nullable thumbnail; POST media/video streaming ingest. Existing image reference helpers enforce kind=image.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Upgrade old images preserve100% IDs/metadata/FKs/refs; image cap8MiB và image/gallery behavior unchanged.
- [ ] Valid AVC optionalAAC MP4 accepted; HEVC/WebM/encrypted/external-data/fragmented/truncated/lying dimensions/codecs rejected.
- [ ] 100MiB/2 ingests/5s/2MiB metadata/4096boxes/depth8/100k samples limits; malicious counts checked before allocation, overflow/overlap/out-of-file rejected.
- [ ] Disconnect/reject/crash staging không tạo public metadata/ref; dedup stable/restart; MIME spoof và video vào image field rejected.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s07_t01_video_upload`: `cargo test --manifest-path cms/Cargo.toml --test m7_s07_t01_video_upload --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s07-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S07-T02 — Video Range, picker/core video và public ACL

- **Thuộc:** V1 · M7 · M7-S07- · M7-P08
- **Module:** CNT-05
- **Capability:** CNT-05.6
- **Primary Acceptance Slice của capability:** M7-S07-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §8](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Video Range, picker/core video và public ACL giúp hoàn thiện M7-P08 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Typed video/poster projection on saved layouts, publication lifecycle ACL, authenticated/public GET/HEAD/conditional Range stream, UI filter/picker/controls and per-document CSP.

#### Phạm vi công việc

Typed video/poster projection on saved layouts, publication lifecycle ACL, authenticated/public GET/HEAD/conditional Range stream, UI filter/picker/controls and per-document CSP.
Migration: `cms/migrations/0031_video_builder_refs.sql` (kiểm highest migration tại claim).

#### Ngoài phạm vi

Không autoplay/remote src, new field kinds, deleting uploaded assets hoặc public refs từ preview.

#### Phụ thuộc và điều kiện chặn

M7-S07-T01 G5; S05-T02/S06 bàn giao core Builder, renderer, media refs. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/migrations/0031_video_builder_refs.sql`
- `cms/crates/opendash-core/src/builder.rs`
- `cms/crates/opendash-server/src/page.rs`
- `cms/crates/opendash-server/src/media.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/overrides.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/routes/media.rs`
- `cms/crates/opendash-server/src/routes/public.rs`
- `cms/crates/opendash-server/src/routes/public_content.rs`
- `cms/admin/src/features/media/MediaLibraryPage.tsx`
- `cms/admin/src/features/media/MediaPicker.tsx`
- `cms/admin/src/builder/document.ts`
- `cms/admin/src/builder/BuilderInspector.tsx`
- `cms/admin/src/builder/BuilderWorkspace.tsx`
- `cms/crates/opendash-server/tests/m7_s07_t02_video_delivery.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

KnownElement::Video(VideoProps{media_id,poster_image_id,controls,muted}); saved typed media refs; shared media::stream_authorized_video(request,asset,file) after ACL, never arbitrary path. Media list/picker discriminated kind.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Unreferenced/draft/preview/inactive override public404 BEFORE HEAD/ETag/Range; Save grant và unpublish/reset revoke ngay.
- [ ] bytes0-0/open/suffix/clamp206 correct; malformed/multi/unsatisfiable416; If-Range mismatch200, If-None-Match304 afterACL, HEAD ignores Range (valid/malformed/multiple) với full200/no body; conditional304 vẫn sauACL.
- [ ] 64KiB streaming buffer/range Content-Length exact; no file-size proportional read/allocation; cancel closes stream.
- [ ] Browser video valid fixture plays/seeks; poster raster, image picker không chọn video; legacy images/theme/plugin flows pass.
- [ ] Theme switch/missing provider preserves AST/settings and applies current public scope; video thumbnail404 safe placeholder.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s07_t02_video_delivery`: `cargo test --manifest-path cms/Cargo.toml --test m7_s07_t02_video_delivery --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Chạy fresh + upgrade copy DB/files; kiểm integrity/foreign_key_check, so IDs/data trước-sau; rollback complete pre-M7 snapshot với matching binary.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s07-t02-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S08-T01 — Deterministic E2E và adversarial fixtures M7

- **Thuộc:** V1 · M7 · M7-S08- · M7-P07
- **Module:** SITE-03
- **Capability:** SITE-03.2
- **Ghi chú capability:** regression/closure của SITE-03.2 và toàn M7, không tạo capability mới.
- **Primary Acceptance Slice của capability:** M7-S08-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Nova dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §9](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Deterministic E2E và adversarial fixtures M7 giúp hoàn thiện M7-P07 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

E2E settings/menu→URL/SEO/sitemap→user-designed form/submission/SMTP failed+unknown→video/Range→theme/plugin cycle→restart/upgrade/rollback fixture; adversarial negative corpus.

#### Phạm vi công việc

E2E settings/menu→URL/SEO/sitemap→user-designed form/submission/SMTP failed+unknown→video/Range→theme/plugin cycle→restart/upgrade/rollback fixture; adversarial negative corpus.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không sửa sản phẩm trong evidence task; lỗi mở fix riêng, repin SHA; không nghiệm thu native OS chỉ từ harness pass.

#### Phụ thuộc và điều kiện chặn

M7-S01..S07 toàn bộ task G5; độc quyền fixture/test files. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/tests/m7_s08_t01_e2e.rs`
- `cms/scripts/m7_browser_e2e.py`
- `cms/fixtures/m7/README.md`
- `cms/fixtures/m7/video-avc-aac.mp4`
- `cms/fixtures/m7/smtp-test-cert.pem`
- `cms/fixtures/m7/smtp-test-key.pem`
- `cms/crates/opendash-server/tests/m7_s08_t01_e2e.rs`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

Blackbox harness uses real built binary/isolated DB+files for public/UI/storage/video and disabled/failing mail; native Rust transport/app tests inject local fixture CA only cfg(test) for successful TLS/send, never production trust bypass. Tiny valid/malformed MP4 fixtures; browser runner nhận --base-url/--site-root/--artifact-dir; không credentials/live recipients.
Tái dùng storage/auth/id/registry/hook primitives, SQL chỉ storage; triển khai theo G2, không tạo parallel data store.

#### Nghiệm thu

- [ ] Fixtures tái lập được, checksum/provenance/license cho binary video/cert/test-only keys; negative inputs không dùng dữ liệu riêng.
- [ ] XSS/CSP/CSRF/Origin/RBAC/401/412, URL race, OG/preview ACL, quota/rate/dedup, mail cancel/crash và malformed video/range đủ assertions.
- [ ] Snapshot trước0025 restore bằng pre-M7 binary có checksum; secret ACL/DB/files included; browser zero console errors.
- [ ] Handoff common source candidate/test commands to Win/Mac tasks, không tự ghi G5 đaOS.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Test `m7_s08_t01_e2e`: `cargo test --manifest-path cms/Cargo.toml --test m7_s08_t01_e2e --locked`; kết quả cần exit0 và các assertion trong checklist pass. UI có thay đổi chạy typecheck/build + native browser relevant; binary/hash trong report. Scope test bắt đầu bằng fixture tái hiện behavior còn thiếu, rồi thực hiện minimal implementation và xác nhận pass.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence tại `docs/verification/2026-11-14-m7-s08-t01-g5.md` (file report thuộc ownership task) (chốt exact report filename trong Issue trước claim). Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S08-T02 — Nghiệm thu M7 bản địa Windows

- **Thuộc:** V1 · M7 · M7-S08- · M7-P07
- **Module:** SITE-03
- **Capability:** SITE-03.2
- **Ghi chú capability:** regression/closure của SITE-03.2 và toàn M7, không tạo capability mới.
- **Primary Acceptance Slice của capability:** M7-S08-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Forge dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §9](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Nghiệm thu M7 bản địa Windows giúp hoàn thiện M7-P07 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Chạy toàn bộ M7 release acceptance trên Windows x64 thật, cùng source với Mac; riêng file report, không đổi fixtures/product.

#### Phạm vi công việc

Chạy toàn bộ M7 release acceptance trên Windows x64 thật, cùng source với Mac; riêng file report, không đổi fixtures/product.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không suy từ CI/Wine/build thành browser/native PASS; không đóng milestone đơnOS.

#### Phụ thuộc và điều kiện chặn

M7-S08-T01 G5; Atlas pin common immutable source + pre-M7 binary snapshot. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m7-g5-windows.md`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

Consumes common source SHA/test fixtures; produces Windows native binary SHA256, exact command receipt, browser/video/TLS/ACL/upgrade/rollback evidence.
Chỉ evidence/harness/closure trong ownership; không sửa product để làm test xanh.

#### Nghiệm thu

- [ ] Admin build/Rust locked build/workspace+focused tests/governance PASS tại source pinned.
- [ ] Chrome real-binary create-form/submit/inbox/disabled-failing SMTP state/menu/SEO/video seek/preview/401/412 zero console errors, native Windows secret DACL negative checks.
- [ ] Fresh/upgrade/restart/site snapshot+old binary rollback PASS, hash và môi trường đủ; NOT_RUN/FAIL chặn G5.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Chạy nguyên bộ lệnh/acceptance chung và ghi đúng OS/SHA; required case thiếu=>NOT_RUN/BLOCKED, không ghi PASS.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence đúng file ownership ở trên. Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S08-T03 — Nghiệm thu M7 bản địa macOS

- **Thuộc:** V1 · M7 · M7-S08- · M7-P07
- **Module:** SITE-03
- **Capability:** SITE-03.2
- **Ghi chú capability:** regression/closure của SITE-03.2 và toàn M7, không tạo capability mới.
- **Primary Acceptance Slice của capability:** M7-S08-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Harbor dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §9](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Nghiệm thu M7 bản địa macOS giúp hoàn thiện M7-P07 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Chạy cùng acceptance Windows trên macOS native, báo architecture thực tế; chỉ report riêng.

#### Phạm vi công việc

Chạy cùng acceptance Windows trên macOS native, báo architecture thực tế; chỉ report riêng.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không nói verified Apple Silicon nếu chỉ chạy Intel; không suy macOS từ Ubuntu CI.

#### Phụ thuộc và điều kiện chặn

M7-S08-T01 G5; cùng source pin với S08-T02, không sửa fixture giữa haiOS. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m7-g5-macos.md`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

Consumes exact common SHA; produces macOS native architecture/binary hash and browser/media/TLS/POSIX secret permissions receipt.
Chỉ evidence/harness/closure trong ownership; không sửa product để làm test xanh.

#### Nghiệm thu

- [ ] Build/locked workspace+M7 focused/browser suites và site rollback PASS trên cùng SHA Windows.
- [ ] Chrome real-binary valid video playback/seeking; separate native Rust SMTP TLS fixture with test-only trust, secrets0700/0600/reparse-symlink denial, menu/SEO/forms/CSP/ACL negatives.
- [ ] Báo cáo commands/hash/OS/limits, không NOT_RUN gate bắt buộc; bug=>fix/re-pin/retest cảOS ảnh hưởng.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Chạy nguyên bộ lệnh/acceptance chung và ghi đúng OS/SHA; required case thiếu=>NOT_RUN/BLOCKED, không ghi PASS.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence đúng file ownership ở trên. Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Không tự đóng milestone hoặc deploy.

### M7-S08-T04 — Tổng hợp G5 M7 và bàn giao operations M8

- **Thuộc:** V1 · M7 · M7-S08- · M7-P07
- **Module:** SITE-03
- **Capability:** SITE-03.2
- **Ghi chú capability:** regression/closure của SITE-03.2 và toàn M7, không tạo capability mới.
- **Primary Acceptance Slice của capability:** M7-S08-; S08 là regression/closure, không cấp lại capability.
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED / BLOCKED_DEPENDENCY (Project Ready chưa xác minh).
- **Người triển khai / review:** Atlas dự kiến; Atlas coordinator/review hoặc self-review theo ADR-0010. Không phải assignment đang chạy.
- **Nguồn quyết định:** [Contract §9](../../../../contracts/M7_SITE_FEATURES.md), [ADR-0014](../../../../decisions/ADR-0014-m7-site-features.md).

#### Tổng quan và giá trị

Tổng hợp G5 M7 và bàn giao operations M8 giúp hoàn thiện M7-P07 bằng hành vi người dùng có thể kiểm chứng; không coi thiết kế là chức năng đã có.

#### Mục tiêu và hành vi quan sát được

Đối chiếu mọi acceptance/negative case haiOS và CI, quyết định G5, cập nhật docs và GitHub đúng gate; bàn giao DB/media/revisions/secrets restore cho M8.

#### Phạm vi công việc

Đối chiếu mọi acceptance/negative case haiOS và CI, quyết định G5, cập nhật docs và GitHub đúng gate; bàn giao DB/media/revisions/secrets restore cho M8.
Migration: Không có migration mới.

#### Ngoài phạm vi

Không Linux deployment/backup engine/M8 G5; không tự đóng milestone nếu bất kỳ native/security/data gate thiếu.

#### Phụ thuộc và điều kiện chặn

M7-S08-T02 và T03 G5 cùngSHA; tất cả implementation G5/CI, M5#157/M6#188 closed hợp lệ. Mọi implementation Ready phải chờ M5#157 và M6#188 actual G5, merged immutable G3 và Issue/Project Ready. Coordinator Atlas; owner dự kiến trong metadata phải được ghi lại trong Issue trước claim. Đọc toàn bộ global constraints/ownership của G3 nguồn, không chỉ bản trích Issue. Không tiến hành task tiếp theo trên cùng seam khi predecessor còn dirty/PR chưa merge/G5 chưa đạt.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-14-m7-release.md`
- `docs/verification/README.md`
- `docs/plans/TASK_INDEX.md`
- `docs/plans/ROADMAP_V1.md`
- `docs/plans/milestones/M7/README.md`
Shared wiring chỉ khi cần slice: server src/app.rs, main.rs, routes/mod.rs, storage/mod.rs; core src/lib.rs; Admin src/App.tsx, api/index.ts. Một writer tại một thời điểm, coordinator giao seam trước pickup; task khác không viết cùng lúc.

#### Cách triển khai theo contract

Produces release source/binary/migration/file manifest, acceptance matrix/native evidence links, open M8 boundaries and Issue/Project closure receipt.
Chỉ evidence/harness/closure trong ownership; không sửa product để làm test xanh.

#### Nghiệm thu

- [ ] Cùng SHA+hash/architecture evidence, không gap browser/rollback/secret ACL; distinct AGREED/IMPLEMENTED/VERIFIED rõ.
- [ ] 14 tasks/8 candidates mapped, Issues/Project nguồn live; mọi bug security/data đã giải quyết.
- [ ] M8 biết canonical files/secret permissions/quota/retention và private SMTP limitations; user data/layout không bị mất.
- [ ] Có evidence thực chạy, SHA/môi trường/giới hạn, review không còn security/data finding; old behavior liên quan được regression.
- [ ] Không đổi scope/contract ngoài thẩm quyền, không claim milestone G5 từ task riêng.

#### Kiểm thử, rủi ro và rollback

Chạy nguyên bộ lệnh/acceptance chung và ghi đúng OS/SHA; required case thiếu=>NOT_RUN/BLOCKED, không ghi PASS.
Không đổi applied migration; dùng site snapshot để rollback dữ liệu nếu task tạo/sửa state.

#### Bàn giao và review

Receipt theo mục chung; evidence đúng file ownership ở trên. Giữ capability/Primary Acceptance, mã task và dependency khi xuất Issue. Chỉ đóng milestone sau review actual Issues/Project/CI/common-source G5, cập nhật TASK_INDEX bằng PR.

