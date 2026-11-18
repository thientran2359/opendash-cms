# M4 — Content foundation G3

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

### M4-S12-T01 — Đăng ký custom taxonomy và liên kết với CPT

- **Thuộc:** V1 macOS Beta follow-up, M4, M4-S12
- **Module:** CNT-03
- **Capability:** CNT-03.1
- **Primary Acceptance Slice của capability:** Existing baseline retained; this task extends configuration behavior, no second baseline completion claim
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective only after merged ADR/contract/spec, not Project Ready
- **Nguồn quyết định:** Owner approval 18/11/2026, ADR-0019, contract section C2
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Đăng ký custom taxonomy và liên kết với CPT. Người dùng cần mô hình nội dung có cấu hình dùng được xuyên Admin,
public và extensions, thay vì danh mục cố định hoặc mỗi nơi sinh URL khác nhau.

#### Mục tiêu và hành vi quan sát được

Thực hiện toàn bộ section C2 của contract ở phạm vi dưới đây; lưu/reload và
restart giữ đúng cấu hình, dữ liệu và quyền. Tiêu chí bên dưới là hành vi nghiệm thu.

#### Phạm vi công việc

Task sở hữu interfaces, validation, storage và integration tương ứng section C2;
các phần section khác chỉ làm dependency/caller theo G2, không tự triển khai trước.
Backend definitions/eligibility, term migration và public/table queries; Admin/Builder selector UI thuộc T02.

#### Ngoài phạm vi

UI polish, full Builder redesign, revisions/schedule, entry hierarchy/Page hierarchical
URLs, arbitrary WP regex/tokens/mutation filters, cloud library, owner live site,
Beta closure và deployment. Không thay authorization hoặc private Section policy.

#### Phụ thuộc và điều kiện chặn

M3-S31-T01 merged và G5 VERIFIED. Các common gates ở đầu tài liệu bắt buộc.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-core/src/content.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/storage/entry_table.rs`
- `cms/crates/opendash-server/src/routes/taxonomy.rs`
- `cms/crates/opendash-server/src/routes/content.rs`
- `cms/crates/opendash-server/src/public_context.rs`
- `cms/crates/opendash-server/src/routes/public_content.rs`
- `cms/crates/opendash-server/src/storage/admin_extensions.rs`
- `cms/migrations/0039_taxonomy_definitions.sql`

Thêm focused tests/runner/fixtures và wiring theo shared ownership ở đầu. DTO/API,
transaction/error behavior theo contract; mở rộng file ngoài quyền cần coordinator.

#### Cách triển khai theo contract

Tái sử dụng canonical domain APIs và serialization/backward defaults, không copy SQL
vào routes/UI hoặc lập registry riêng. Stop-and-Propose nếu phát hiện contract/schema/
permission delta; nội bộ function decomposition do Harbor quyết định và ghi Design Delta.

#### Nghiệm thu

- [ ] Migrate bản copy0038 có category tree,tag,membership,term extensions: IDs/version/FKs/giá trị giữ nguyên; restart và integrity/FK checks PASS.
- [ ] Admin tạo genre dạng cây và brand dạng thẻ, gắn vào CPT; cùng slug ở hai taxonomy được phép, cùng taxonomy409; parent khác taxonomy/cycle/depth9 bị từ chối.
- [ ] GET/POST/PATCH/DELETE definitions và PUT eligibility đúng CAS/quyền; category/tag immutable; taxonomy đang dùng không xóa được.
- [ ] Bỏ association giữ link cũ nhưng loại khỏi picker/query/public filter; omitted term_ids giữ link, explicit IDs kiểm eligibility.
- [ ] Generic public/table filter taxonomy_key+term_id hoạt động và giữ aliases cũ; thiếu/cặp trộn400, taxonomy riêng tư không leak qua archives.

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

### M4-S12-T02 — Quản lý taxonomy tùy chỉnh xuyên Admin và Builder

- **Thuộc:** V1 macOS Beta follow-up, M4, M4-S12
- **Module:** CNT-03
- **Capability:** CNT-03.1
- **Primary Acceptance Slice của capability:** Existing baseline retained; this task extends configuration behavior, no second baseline completion claim
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective only after merged ADR/contract/spec, not Project Ready
- **Nguồn quyết định:** Owner approval 18/11/2026, ADR-0019, contract section C2
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Quản lý taxonomy tùy chỉnh xuyên Admin và Builder. Người dùng cần mô hình nội dung có cấu hình dùng được xuyên Admin,
public và extensions, thay vì danh mục cố định hoặc mỗi nơi sinh URL khác nhau.

#### Mục tiêu và hành vi quan sát được

Thực hiện toàn bộ section C2 của contract ở phạm vi dưới đây; lưu/reload và
restart giữ đúng cấu hình, dữ liệu và quyền. Tiêu chí bên dưới là hành vi nghiệm thu.

#### Phạm vi công việc

Task sở hữu interfaces, validation, storage và integration tương ứng section C2;
các phần section khác chỉ làm dependency/caller theo G2, không tự triển khai trước.
Admin taxonomy UI và Builder query/picker integration, không thêm registry/migration thứ hai.

#### Ngoài phạm vi

UI polish, full Builder redesign, revisions/schedule, entry hierarchy/Page hierarchical
URLs, arbitrary WP regex/tokens/mutation filters, cloud library, owner live site,
Beta closure và deployment. Không thay authorization hoặc private Section policy.

#### Phụ thuộc và điều kiện chặn

M4-S12-T01 merged và G5 VERIFIED. Các common gates ở đầu tài liệu bắt buộc.

#### Kỹ thuật và file ownership

- `cms/admin/src/api/content.ts`
- `cms/admin/src/api/taxonomy.ts`
- `cms/admin/src/features/content/ContentTypeSchemaSettings.tsx`
- `cms/admin/src/features/content/ContentTypeForm.tsx`
- `cms/admin/src/features/content/EntryListPage.tsx`
- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/admin/src/features/taxonomy/TaxonomyPage.tsx`
- `cms/admin/src/features/taxonomy/TermEditPage.tsx`
- `cms/admin/src/features/taxonomy/TermPicker.tsx`
- `cms/admin/src/features/menus/MenusPanel.tsx`
- `cms/admin/src/builder/BuilderListFields.tsx`
- `cms/admin/src/builder/document.ts`
- `cms/admin/src/features/editor/PanelRegionRenderer.tsx`
- `cms/admin/src/shell/AdminShell.tsx`
- `cms/admin/src/shell/router.ts`
- `cms/admin/src/App.tsx`
- `cms/admin/src/features/taxonomy/TaxonomyDefinitionsPage.tsx`
- `cms/crates/opendash-core/src/builder.rs`
- `cms/crates/opendash-server/src/storage/builder_binding.rs`
- `cms/crates/opendash-server/src/content_render.rs`
- `cms/crates/opendash-server/src/routes/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/auth.rs`
- `cms/crates/opendash-server/src/storage/builder.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/scripts/m7_s16_t06_entry_table_e2e.py`

Aster corrective ownership addendum, 19/11/2026, reviewed head
`9017b9cb2af9b71c8b6975a88c69db6209f160d7`: the added integration files above
are exclusive to Harbor for this task. ContentTypeForm changes preserve creation
drafts across401/late responses; auth identity retains the previously documented
filesystem-generation limits. The existing M7 runner changes only focus/wait for
lazy filter options and preserve its semantic expectations. New C2 read DTOs,
bounded term-label projection and provider CAS corrections follow the
[contract clarification](../../../../contracts/CONTENT_FOUNDATION_2026_11.md#c2-corrective-read-projections-and-cas-clarification--19112026).
Forge's checker/receipt task has no overlapping write ownership. Successor pickup
still requires task-local exact merged-source G5 and an explicit file handoff.

Thêm focused tests/runner/fixtures và wiring theo shared ownership ở đầu. DTO/API,
transaction/error behavior theo contract; mở rộng file ngoài quyền cần coordinator.

#### Cách triển khai theo contract

Tái sử dụng canonical domain APIs và serialization/backward defaults, không copy SQL
vào routes/UI hoặc lập registry riêng. Stop-and-Propose nếu phát hiện contract/schema/
permission delta; nội bộ function decomposition do Harbor quyết định và ghi Design Delta.

#### Nghiệm thu

- [ ] Từ Admin tạo taxonomy, chọn CPT áp dụng, tạo/sửa term qua route riêng rồi gắn vào entry; reload/restart giữ đủ cấu hình và membership.
- [ ] Picker dựa hierarchical flag, Page mặc định không taxonomy; CPT taxonomy chỉ render một lần và lọc table đúng.
- [ ] Menu term và Builder list chọn custom taxonomy thực sự lọc đúng entry công khai; cấu hình legacy category/tag vẫn đọc và render đúng.
- [ ] Term panel provider/401/412/schema token/đổi route khi dirty không ghi nhầm taxonomy hoặc mất edit chưa lưu.
- [ ] Editor không thấy schema controls và không vượt quyền qua direct route/API; lỗi hiển thị cụ thể, không fake success.

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
