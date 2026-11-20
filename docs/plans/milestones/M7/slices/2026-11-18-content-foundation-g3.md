# M7 — Content foundation G3

Coordinator Aster, implementer Harbor/self-review, một task đang code mỗi lần.
[ADR-0019](../../../../decisions/ADR-0019-content-foundation.md) và
[contract](../../../../contracts/CONTENT_FOUNDATION_2026_11.md) là G1/G2 authority khi merge.
Source inspected `2ebe51cbe1099c4721304e2b31895aa78b7c1241`. Full-Track; G3 đủ đặc tả
khác Project Ready. Thứ tự M3-S31-T01 → M4-S12-T01 → M4-S12-T02 → M7-S17-T01 →
M7-S17-T02 → M6-S11-T01. Chỉ pickup từ immutable merged specs, verified dependency G5,
Issue ownership, branch rảnh và live Project Ready. Không dispatch từ tài liệu.

File ownership độc quyền trong lượt task; predecessor bàn giao trước khi successor
sửa shared file. Ngoài danh sách riêng, mỗi task sở hữu focused integration test tại
cms/crates/opendash-server/tests/content_foundation_TASK_ID.rs, browser runner tại
cms/scripts/content_foundation_TASK_ID_e2e.py, fixtures tương ứng và receipt tại
docs/verification/. Shared integration files storage/mod.rs, routes/mod.rs, app.rs,
core/lib.rs và migration include list chỉ được sửa để wire phần task đó; struct
literal/DTO compile updates trong tests hiện có được phép, không đổi semantics ngoài
contract. Task M6 được sửa SDK/example/fixture subtree chỉ cho declaration mới.
Aster sở hữu ADR/contract/plan; cần contract delta thì Stop-and-Propose, không tự
đổi G2. Migration0038→0039→0040 phải kiểm tra lại main trước khi dùng.

Dùng id/registry/HookDispatcher/package/auth/ApiError, toàn bộ SQL ở storage. Không
nâng dependency. Baseline failing regression trước implementation. Validate bằng
cargo build --manifest-path cms/Cargo.toml --locked, cargo test --manifest-path
cms/Cargo.toml --locked -p opendash-server --test content_foundation_TASK_ID,
npm run build --prefix cms/admin nếu UI thay đổi. TASK_ID trong commands thay bằng
ID task thực, dấu gạch ngang đổi underscore. Browser runner phải hỗ trợ --binary
và --artifact-dir, bounded lifetime, exit nonzero khi assertion lỗi; isolated site,
không dùng root/port18964. Mỗi acceptance cần actual observed receipt và screenshots
nếu UI; không lấy API test thay browser hay CI thay native G5.

Storage changes: DB copy trước/sau upgrade, FK/integrity, restart, provider lifecycle,
backup→restore→re-edit theo hiện có; không redesign recovery. Chỉ forward migration
hoặc restore snapshot với binary phù hợp; Git revert không downgrade schema. Ghi
head/base/tree/binary/Admin/fixture hashes, OS/arch, exact commands/exits, negative
cases, giới hạn. Required CI+self-review current head trước non-force push/merge;
không auto-close #302 hay nâng Beta/Windows/Linux/common RC.

### M7-S17-T01 — Giải quyết permalink và redirect bằng một route authority

- **Thuộc:** V1 macOS Beta follow-up, M7, M7-S17
- **Module:** SITE-02
- **Capability:** SITE-02.1
- **Primary Acceptance Slice của capability:** Existing baseline retained; this task extends configuration behavior, no second baseline completion claim
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective only after merged ADR/contract/spec, not Project Ready
- **Nguồn quyết định:** Owner approval 18/11/2026, ADR-0019, contract section C3
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Giải quyết permalink và redirect bằng một route authority. Người dùng cần mô hình nội dung có cấu hình dùng được xuyên Admin,
public và extensions, thay vì danh mục cố định hoặc mỗi nơi sinh URL khác nhau.

#### Mục tiêu và hành vi quan sát được

Thực hiện toàn bộ section C3 của contract ở phạm vi dưới đây; lưu/reload và
restart giữ đúng cấu hình, dữ liệu và quyền. Tiêu chí bên dưới là hành vi nghiệm thu.

#### Phạm vi công việc

Task sở hữu interfaces, validation, storage và integration tương ứng section C3;
các phần section khác chỉ làm dependency/caller theo G2, không tự triển khai trước.
Backend resolver/claims/configuration API và toàn bộ server URL consumers; màn hình cấu hình thuộc T02.

#### Ngoài phạm vi

UI polish, full Builder redesign, revisions/schedule, entry hierarchy/Page hierarchical
URLs, arbitrary WP regex/tokens/mutation filters, cloud library, owner live site,
Beta closure và deployment. Không thay authorization hoặc private Section policy.

#### Phụ thuộc và điều kiện chặn

M4-S12-T02 merged và G5 VERIFIED. Các common gates ở đầu tài liệu bắt buộc.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/src/public_context.rs`
- `cms/crates/opendash-server/src/seo.rs`
- `cms/crates/opendash-server/src/app.rs`
- `cms/crates/opendash-server/src/routes/seo.rs`
- `cms/crates/opendash-server/src/routes/public_content.rs`
- `cms/crates/opendash-server/src/routes/sitemap.rs`
- `cms/crates/opendash-server/src/routes/settings.rs`
- `cms/crates/opendash-server/src/storage/seo.rs`
- `cms/crates/opendash-server/src/storage/settings.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/storage/menu.rs`
- `cms/crates/opendash-server/src/storage/builder_binding.rs`
- `cms/crates/opendash-server/src/storage/sitemap.rs`
- `cms/crates/opendash-server/src/content_render.rs`
- `cms/migrations/0040_permalink_claims.sql`

Aster recorded ownership consolidation and regression disposition, 19/11/2026,
for [#376](https://github.com/thientran2359/opendash-cms/issues/376) and
[Draft #398](https://github.com/thientran2359/opendash-cms/pull/398), reviewed
candidate `eecb83019378476cbae4aee6ef8dea1e3fd5543d`. Harbor remains the sole
writer for this task. The earlier Builder integration ACK covers
`cms/crates/opendash-server/src/page.rs` and
`cms/crates/opendash-server/src/storage/builder.rs`. The new canonical resolver
module `cms/crates/opendash-server/src/storage/permalinks.rs` is within C3 storage
ownership; `storage/mod.rs` remains common wiring only.

Additional **test-only** ownership is granted for the following legacy regression
fixtures. Production code in `storage/entry_table.rs` is outside this extension.

- `cms/crates/opendash-server/src/storage/entry_table.rs` — only its
  `#[cfg(test)] c1_public_url_tests` module and test-only helpers.
- `cms/crates/opendash-server/tests/m7_s16_t05_entry_table.rs`
- `cms/crates/opendash-server/tests/m5_s02_t01_public_context.rs`
- `cms/crates/opendash-server/tests/m5_s06_t01_preservation.rs`
- `cms/crates/opendash-server/tests/m5_s08_t01_e2e_delivery.rs`
- `cms/crates/opendash-server/tests/m5_s09_t01_site_menus.rs`
- `cms/crates/opendash-server/tests/m5_s10_t01_theme_cnt_declarations.rs`
- `cms/crates/opendash-server/tests/m6_s02_t01_plugin_cnt_bridge.rs`
- `cms/crates/opendash-server/tests/m6_s02_t02_plugin_taxonomy.rs`
- `cms/crates/opendash-server/tests/m6_s08_t01_plugin_e2e.rs`
- `cms/crates/opendash-server/tests/m7_s09_t01_schema_field_groups.rs`

Bounded accepted corrections follow the existing C1/C2/C3/C4 contracts and the
[M5](../../../../contracts/M5_THEME_TEMPLATES.md#5-cnt-lifecycle-and-taxonomy-e)
and [M6](../../../../contracts/M6_PLUGIN_EXTENSIONS.md#2-owner-neutral-cnt--taxonomy-bridge-p08-p04-d03-d08)
clarifications. After direct-SQL fixture changes, synchronize canonical claims
through the serialized storage helper for unit fixtures or authenticated
same-value permalink GET/PUT with matching CAS/CSRF/Origin for binary fixtures.
Keep custom/legacy URL, draft denial, menu escaping, filter/count/pagination,
restart, collision rollback and data-preservation assertions. Only individually
identified malformed encoding/traversal404 expectations may become the already
specified C3 status400; valid missing/private/draft targets retain404.

For provider-owned inactive CPT public routes, assert404 and authenticated
retention/editability, then200 after matching reactivation when otherwise public.
For the two builtin-hijack fixtures, supply valid core `post` declaration settings
(`public:true,has_archive:true,publication:draft_publish,supports:[editor,featured_image]`)
and retain the original409 collision/rollback assertions. For inactive-provider
entry updates, omit `term_ids` to exercise valid title/value editing with200 and
unchanged memberships; preserve explicit membership-change coverage after
reactivation or an active site override, with an ineligible-selection negative
case and durable-state checks. Do not turn expected200 into400 to pass the suite.
For migrated builtin lists, assert exact keys post/page/section, retain post/page
version/schema-token and migration/CAS checks, and assert Section core ownership
and immutable private settings, rather than weakening the check to an arbitrary
count. No runtime/API change, disabled test or swallowed failure is authorized
by this test-fixture extension.

Aster independently ran all five disputed provider/schema integration targets
on the exact **pre-C3** baseline `49fcd730be526fee17b5a5f240d58f1cc919d626` with
`--no-fail-fast -- --test-threads=1`: all five reproduced the reported failures
(theme422 versus409, builtin-plugin400 versus409, two inactive-selection400
versus200, builtin count3 versus2). These failures predate #398 and establish
the baseline for correction. Corrected regression execution remains pending.
Harbor must run the corrected
focused targets and full affected serial suites, preserve old failure receipts,
and report exact source/commands/results before G4 acceptance. New unexplained
failures require triage; no baseline waiver or migration/G5 gate is granted.

Thêm focused tests/runner/fixtures và wiring theo shared ownership ở đầu. DTO/API,
transaction/error behavior theo contract; mở rộng file ngoài quyền cần coordinator.

#### Cách triển khai theo contract

Tái sử dụng canonical domain APIs và serialization/backward defaults, không copy SQL
vào routes/UI hoặc lập registry riêng. Stop-and-Propose nếu phát hiện contract/schema/
permission delta; nội bộ function decomposition do Harbor quyết định và ghi Design Delta.

#### Nghiệm thu

- [ ] Upgrade giữ nguyên URLs; chọn /blog/%postname%, CPT /projects/slug và taxonomy /genres/term: direct request, canonical/OG/sitemap/menu/Builder đều trùng resolver.
- [ ] Đổi format/base/slug/term tạo301 một bước từ đường cũ, theo stable ID; custom_path thắng generated path, reset trở về đúng format hiện hành.
- [ ] 409 collision với entry/term/archive/custom path/redirect khác owner và route reserved abort toàn bộ; concurrent publication/config không chiếm trùng path.
- [ ] Không có anonymous draft/private/Section/media leak qua URL mới hay redirect cũ; disable provider áp dụng policy cũ, restart không cần flush.
- [ ] Malformed token/encoding/traversal/mixed aliases400, stale settings412; giới hạn50000 claims từ chối trước ghi; template/static route không bị shadow.

#### Kiểm thử, rủi ro và rollback

Chạy common checks, focused acceptance trên isolated native macOS và regression
category/tag, post/page, Section, media/Builder/provider bị tác động. Test lỗi phải
assert DB/public state không đổi; giữ failed receipts. Shared migration/rollback
policy áp dụng; báo rõ case chưa chạy, không hạ gate vì compile hoặc CI PASS.

#### Bàn giao và review

PR ghi Agent Harbor, task/base/head, self-review exact head, acceptance matrix,
commands/exits/artifacts và verification report. G4 merged không tự là G5 VERIFIED;
Issue/Project là trạng thái live, Task Index chỉ snapshot. Không nhắn agent khác
hay tự chuyển #302/parent theo kết quả slice.

### M7-S17-T02 — Cấu hình permalink và URL xem bài trong Admin

- **Thuộc:** V1 macOS Beta follow-up, M7, M7-S17
- **Module:** SITE-02
- **Capability:** SITE-02.1
- **Primary Acceptance Slice của capability:** Existing baseline retained; this task extends configuration behavior, no second baseline completion claim
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective only after merged ADR/contract/spec, not Project Ready
- **Nguồn quyết định:** Owner approval 18/11/2026, ADR-0019, contract section C3
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Cấu hình permalink và URL xem bài trong Admin. Người dùng cần mô hình nội dung có cấu hình dùng được xuyên Admin,
public và extensions, thay vì danh mục cố định hoặc mỗi nơi sinh URL khác nhau.

#### Mục tiêu và hành vi quan sát được

Thực hiện toàn bộ section C3 của contract ở phạm vi dưới đây; lưu/reload và
restart giữ đúng cấu hình, dữ liệu và quyền. Tiêu chí bên dưới là hành vi nghiệm thu.

#### Phạm vi công việc

Task sở hữu interfaces, validation, storage và integration tương ứng section C3;
các phần section khác chỉ làm dependency/caller theo G2, không tự triển khai trước.
Admin configuration và URL consumers, không tạo client-side URL resolver.

#### Ngoài phạm vi

UI polish, full Builder redesign, revisions/schedule, entry hierarchy/Page hierarchical
URLs, arbitrary WP regex/tokens/mutation filters, cloud library, owner live site,
Beta closure và deployment. Không thay authorization hoặc private Section policy.

#### Phụ thuộc và điều kiện chặn

M7-S17-T01 merged và G5 VERIFIED. Các common gates ở đầu tài liệu bắt buộc.

#### Kỹ thuật và file ownership

- `cms/admin/src/api/settings.ts`
- `cms/admin/src/api/seo.ts`
- `cms/admin/src/api/content.ts`
- `cms/admin/src/features/settings/PermalinkSettingsPage.tsx`
- `cms/admin/src/features/seo/SeoPanel.tsx`
- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/features/content/EntryListPage.tsx`
- `cms/admin/src/features/content/ContentTypeSchemaSettings.tsx`
- `cms/admin/src/App.tsx`
- `cms/admin/src/shell/router.ts`
- `cms/admin/src/shell/AdminShell.tsx`

Thêm focused tests/runner/fixtures và wiring theo shared ownership ở đầu. DTO/API,
transaction/error behavior theo contract; mở rộng file ngoài quyền cần coordinator.

#### Cách triển khai theo contract

Tái sử dụng canonical domain APIs và serialization/backward defaults, không copy SQL
vào routes/UI hoặc lập registry riêng. Stop-and-Propose nếu phát hiện contract/schema/
permission delta; nội bộ function decomposition do Harbor quyết định và ghi Design Delta.

#### Nghiệm thu

- [ ] Admin chọn preset/custom post format,CPT/taxonomy bases rồi Lưu; preview là server-resolved URL và public chỉ đổi sau Lưu thành công.
- [ ] View/row actions/edit preview dùng canonical API response, không tự ghép đường dẫn; draft chỉ authenticated preview.
- [ ] Conflict409 hiển thị targets liên quan và giữ draft form;412 không ghi đè config mới;401/re-auth không lưu bằng tài khoản sai.
- [ ] Không hiển thị token/category/date/hierarchy chưa hỗ trợ như tính năng dùng được; reload giữ cấu hình.

#### Kiểm thử, rủi ro và rollback

Chạy common checks, focused acceptance trên isolated native macOS và regression
category/tag, post/page, Section, media/Builder/provider bị tác động. Test lỗi phải
assert DB/public state không đổi; giữ failed receipts. Shared migration/rollback
policy áp dụng; báo rõ case chưa chạy, không hạ gate vì compile hoặc CI PASS.

#### Bàn giao và review

PR ghi Agent Harbor, task/base/head, self-review exact head, acceptance matrix,
commands/exits/artifacts và verification report. G4 merged không tự là G5 VERIFIED;
Issue/Project là trạng thái live, Task Index chỉ snapshot. Không nhắn agent khác
hay tự chuyển #302/parent theo kết quả slice.
