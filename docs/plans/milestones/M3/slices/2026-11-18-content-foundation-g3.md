# M3 — Content foundation G3

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

### M3-S31-T01 — Cấu hình hành vi CPT và cơ chế Lưu/Xuất bản

- **Thuộc:** V1 macOS Beta follow-up, M3, M3-S31
- **Module:** CNT-01
- **Capability:** CNT-01.1
- **Primary Acceptance Slice của capability:** Existing baseline retained; this task extends configuration behavior, no second baseline completion claim
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; effective only after merged ADR/contract/spec, not Project Ready
- **Nguồn quyết định:** Owner approval 18/11/2026, ADR-0019, contract section C1
- **Người triển khai / review:** harbor / self-review; coordinator Aster

#### Tổng quan và giá trị

Cấu hình hành vi CPT và cơ chế Lưu/Xuất bản. Người dùng cần mô hình nội dung có cấu hình dùng được xuyên Admin,
public và extensions, thay vì danh mục cố định hoặc mỗi nơi sinh URL khác nhau.

#### Mục tiêu và hành vi quan sát được

Thực hiện toàn bộ section C1 của contract ở phạm vi dưới đây; lưu/reload và
restart giữ đúng cấu hình, dữ liệu và quyền. Tiêu chí bên dưới là hành vi nghiệm thu.

#### Phạm vi công việc

Task sở hữu interfaces, validation, storage và integration tương ứng section C1;
các phần section khác chỉ làm dependency/caller theo G2, không tự triển khai trước.

#### Ngoài phạm vi

UI polish, full Builder redesign, revisions/schedule, entry hierarchy/Page hierarchical
URLs, arbitrary WP regex/tokens/mutation filters, cloud library, owner live site,
Beta closure và deployment. Không thay authorization hoặc private Section policy.

#### Phụ thuộc và điều kiện chặn

Không có task mới chặn; baseline0037 và ADR-0019 đã merge. Các common gates ở đầu tài liệu bắt buộc.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-core/src/content.rs`
- `cms/crates/opendash-server/src/storage/content.rs`
- `cms/crates/opendash-server/src/storage/entries.rs`
- `cms/crates/opendash-server/src/storage/public_content.rs`
- `cms/crates/opendash-server/src/storage/media.rs`
- `cms/crates/opendash-server/src/storage/entry_table.rs`
- `cms/crates/opendash-server/src/routes/content.rs`
- `cms/crates/opendash-server/src/plugin/host_ops.rs`
- `cms/admin/src/api/content.ts`
- `cms/admin/src/features/content/ContentTypeForm.tsx`
- `cms/admin/src/features/content/ContentTypeSchemaSettings.tsx`
- `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
- `cms/migrations/0038_content_type_settings.sql`

Thêm focused tests/runner/fixtures và wiring theo shared ownership ở đầu. DTO/API,
transaction/error behavior theo contract; mở rộng file ngoài quyền cần coordinator.

#### Cách triển khai theo contract

Tái sử dụng canonical domain APIs và serialization/backward defaults, không copy SQL
vào routes/UI hoặc lập registry riêng. Stop-and-Propose nếu phát hiện contract/schema/
permission delta; nội bộ function decomposition do Harbor quyết định và ghi Design Delta.

#### Nghiệm thu

- [ ] CPT cũ không đổi trạng thái/URL sau migration; API cũ thiếu settings vẫn hoạt động; metadata/schema token/version nhất quán qua restart.
- [ ] Admin tạo CPT draft_publish, Lưu nháp không có public/archive/sitemap/media; Publish rồi View đúng nội dung, Editor không đổi schema.
- [ ] Đổi immediate sang draft_publish giữ bài cũ published; chiều ngược có draft trả409 và không thay đổi dữ liệu.
- [ ] public false và has_archive được thực thi server-side, private Section không thể bị expose; hỗ trợ editor/image tắt rồi bật lại giữ values/media.
- [ ] 401/403/CSRF,412, concurrent settings/save và malformed settings không tạo ghi một phần.

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
