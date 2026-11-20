# M6 — Content foundation G3

Coordinator Aster, implementer Harbor/self-review, một task đang code mỗi lần.
[ADR-0019](../../../../../decisions/ADR-0019-content-foundation.md) và
[contract](../../../../../contracts/CONTENT_FOUNDATION_2026_11.md) là G1/G2 authority khi merge.
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

### M6-S11-T01 — Plugin/theme khai báo CPT taxonomy rewrite và kiểm chứng hooks

- **Thuộc:** V1 macOS Beta follow-up, M6, M6-S11
- **Module:** PLG-03
- **Capability:** PLG-03.1
- **Primary Acceptance Slice của capability:** Existing baseline retained; this task extends configuration behavior, no second baseline completion claim
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective only after merged ADR/contract/spec, not Project Ready
- **Nguồn quyết định:** Owner approval 18/11/2026, ADR-0019, contract section C4
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Plugin/theme khai báo CPT taxonomy rewrite và kiểm chứng hooks. Người dùng cần mô hình nội dung có cấu hình dùng được xuyên Admin,
public và extensions, thay vì danh mục cố định hoặc mỗi nơi sinh URL khác nhau.

#### Mục tiêu và hành vi quan sát được

Thực hiện toàn bộ section C4 của contract ở phạm vi dưới đây; lưu/reload và
restart giữ đúng cấu hình, dữ liệu và quyền. Tiêu chí bên dưới là hành vi nghiệm thu.

#### Phạm vi công việc

Task sở hữu interfaces, validation, storage và integration tương ứng section C4;
các phần section khác chỉ làm dependency/caller theo G2, không tự triển khai trước.

#### Ngoài phạm vi

UI polish, full Builder redesign, revisions/schedule, entry hierarchy/Page hierarchical
URLs, arbitrary WP regex/tokens/mutation filters, cloud library, owner live site,
Beta closure và deployment. Không thay authorization hoặc private Section policy.

#### Phụ thuộc và điều kiện chặn

M7-S17-T02 merged và G5 VERIFIED. Các common gates ở đầu tài liệu bắt buộc.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-core/src/content.rs`
- `cms/crates/opendash-core/src/hooks.rs`
- `cms/crates/opendash-server/src/theme.rs`
- `cms/crates/opendash-server/src/storage/theme.rs`
- `cms/crates/opendash-server/src/storage/plugin.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/taxonomy.rs`
- `cms/crates/opendash-server/src/storage/seo.rs`
- `cms/crates/opendash-server/src/plugin/host_ops.rs`
- `cms/crates/opendash-server/src/storage/admin_extensions.rs`
- `cms/sdk/`
- `cms/examples/`
- `cms/fixtures/`

Thêm focused tests/runner/fixtures và wiring theo shared ownership ở đầu. DTO/API,
transaction/error behavior theo contract; mở rộng file ngoài quyền cần coordinator.

#### Cách triển khai theo contract

Tái sử dụng canonical domain APIs và serialization/backward defaults, không copy SQL
vào routes/UI hoặc lập registry riêng. Stop-and-Propose nếu phát hiện contract/schema/
permission delta; nội bộ function decomposition do Harbor quyết định và ghi Design Delta.

#### Nghiệm thu

- [ ] Old plugin/theme ZIP hoạt động không đổi; new ZIP khai báo own CPT draft_publish, custom taxonomy, associations và rewrite; Admin và public dùng cùng canonical data.
- [ ] Trùng owner/key/base,invalid hierarchy/unknown settings hoặc thiếu quyền bị từ chối atomically; previous active revision vẫn dùng được.
- [ ] Site override thắng package default; update/theme switch/disable/uninstall/re-enable/restart giữ definitions/terms/entries/values và tiếp tục quản lý từ Admin.
- [ ] Real Wasmi ContentBeforeSave Allow/Veto và ContentAfterSave/ContentSave với custom taxonomy: CAS,publication,permissions,generation,timeout đúng; Veto không ghi một phần.
- [ ] SDK example và support matrix chỉ rõ declarations/actions/veto, không tuyên bố arbitrary mutation hook hay runtime theme code; package/browser E2E trên isolated macOS PASS.

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
