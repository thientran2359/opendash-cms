# M4 — G3 nền tảng Builder và Taxonomy

[ADR-0007](../../../../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) đã merge tại `f04a657411c6a60c4bc5cf461aa119b9bef921ee`. [M4 Task Plan](../TASK_PLAN.md) · [G1](../../../../../.agents/research/2026-11-11-m4-builder-taxonomy-g1.md).

G3 READY mô tả đặc tả sau khi PR này merge. Khi phát hành Issue, pin source SHA trên main, kiểm dependency, ownership và Project #3 trước khi chuyển Ready. Forge và Harbor là writer dự kiến. M4 theo luồng review độc lập của repo, mỗi agent dùng branch riêng và chỉ nhận một task implementation đang hoạt động. P01 và P09 có thể làm song song. P09 giữ lượt tích hợp router và migration trước P02. P02 bàn giao `storage/public_content.rs` cho P10 khi merge. Số migration ghi dưới đây là dự kiến và phải đối chiếu trước khi triển khai.

### M4-S01-T01 — Mô hình cây Builder và kiểm tra dữ liệu layout

- **Thuộc:** V1, M4, M4-S01, M4-P01.
- **Primary Acceptance Slice của capability:** M4-S01 cho BLD-02.1.
- **Module:** BLD-02
- **Capability:** BLD-02.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0007 § P01.
- **Người triển khai / review:** Forge dự kiến. Atlas hoặc Nova review độc lập theo ownership.

#### Tổng quan và giá trị

Core đọc, kiểm tra và ghi lại document v1 với cây Section → Row → Column → Element. Layout hợp lệ giữ ID và thứ tự node qua serialize/deserialize.

#### Mục tiêu và hành vi quan sát được

Round-trip layout hợp lệ, ID ổn định khi reorder và props có kiểu.

#### Phạm vi công việc

Định nghĩa document, các node, props có kiểu cho Heading/Text/Image/Button và node tham chiếu block legacy. Tái sử dụng định danh và registry core. Giữ khả năng đọc payload node đã lưu khi descriptor thiếu theo ADR-0007.

#### Ngoài phạm vi

Renderer, migration, HTTP, UI, breakpoint và binding thuộc các task sau.

#### Phụ thuộc và điều kiện chặn

ADR-0007, core registry và block schema M3. Có thể làm cùng P09 vì file riêng. Chốt giao diện kiểu và test fixture trước bàn giao P02.

#### Kỹ thuật và file ownership

`cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-core/src/lib.rs`, `docs/verification/2026-11-11-m4-s01-t01-builder-core.md`. Mở rộng qua Design Delta trước khi sửa file ngoài danh sách.

#### Cách triển khai theo contract

Dùng [ADR-0007](../../../../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) và API lõi hiện có. Giữ triển khai SQL ở storage và ghi evidence theo SHA thực chạy.

#### Nghiệm thu

- [ ] Round-trip layout hợp lệ, ID ổn định khi reorder và props có kiểu.
- [ ] Từ chối schema version lạ, node trùng, sai cha–con, vượt giới hạn và URL sai policy.
- [ ] Node legacy giữ nguyên block ID/map field. Payload chưa resolve chỉ được giữ theo quy tắc round-trip của ADR.

#### Kiểm thử, rủi ro và rollback

Chạy test crate core và build locked. Kiểm fixture M3 và cây lỗi biên. Rollback code-only vì task chưa ghi SQLite.

#### Bàn giao và review

PR trên `task/forge-m4-s01-t01` tới `main`, ghi approved source SHA, implementation base/head, lệnh, kết quả và giới hạn. Project Done sau G5 có evidence.

### M4-S02-T01 — Mở, Preview và Lưu homepage dạng cây

- **Thuộc:** V1, M4, M4-S02, M4-P02.
- **Primary Acceptance Slice của capability:** M4-S02 cho BLD-01.1.
- **Module:** BLD-01
- **Capability:** BLD-01.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0007 § P02.
- **Người triển khai / review:** Forge dự kiến. Atlas hoặc Nova review độc lập theo ownership.

#### Tổng quan và giá trị

Homepage M3 mở qua Builder API dưới dạng cây. Preview giữ draft riêng. Save đầu tiên chuyển nguồn public trong một transaction. Khi đổi theme, node thiếu hỗ trợ ẩn trên public và giữ dữ liệu gốc.

#### Mục tiêu và hành vi quan sát được

Mở và Preview trên DB M3 không ghi dữ liệu. Save chuyển đúng một lần, restart giữ ID/order/field, legacy writer nhận 409 sau chuyển và stale 412 trước hoặc sau chuyển.

#### Phạm vi công việc

Thêm singleton/version, projection legacy và API GET/Preview/Save. Nối renderer `/`, version CAS, chẩn đoán per node, hook sau commit và media ACL của Image đã Save theo ADR-0007.

#### Ngoài phạm vi

Tree/canvas/inspector, responsive, binding và list block thuộc P03–P06. Giữ URL Post/Page/CPT và semantics Publish.

#### Phụ thuộc và điều kiện chặn

P01 đã merge. Kiểm baseline M3 và trạng thái #53/#54 khi mở việc. Điều phối file `app.rs`, `routes/mod.rs`, `storage/mod.rs`, `error.rs` và số migration sau lượt tích hợp P09. `storage/public_content.rs` bàn giao cho P10 sau khi P02 merge.

#### Kỹ thuật và file ownership

Trong `cms/crates/opendash-server/src/` gồm `app.rs`, `error.rs`, `page.rs`, `theme.rs`, `routes/mod.rs`, `routes/builder.rs`, `routes/public.rs`, `storage/mod.rs`, `storage/builder.rs`, `storage/theme.rs`, `storage/public_content.rs`. Thêm `cms/migrations/0013_builder_homepage.sql` dự kiến, `cms/scripts/builder_homepage_smoke.py` và `docs/verification/2026-11-11-m4-s02-t01-builder-homepage.md`. Mở rộng qua Design Delta trước khi sửa file ngoài danh sách.

#### Cách triển khai theo contract

Dùng [ADR-0007](../../../../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) và API lõi hiện có. Giữ triển khai SQL ở storage và ghi evidence theo SHA thực chạy.

#### Nghiệm thu

- [ ] Mở và Preview trên DB M3 không ghi dữ liệu. Save chuyển đúng một lần, restart giữ ID/order/field, legacy writer nhận 409 sau chuyển và stale 412 trước hoặc sau chuyển.
- [ ] A→B dùng markup mới, A→C ẩn đúng node legacy, C→A phục hồi. Core elements giữ khả năng render và Builder nhận diagnostic theo node.
- [ ] Image draft không cấp public ACL. Save/gỡ tham chiếu kiểm 200/404 đúng policy, bảo toàn media ACL M3. Kiểm CSRF/Origin, CSP, event sau commit và lỗi hook.
- [ ] Restore snapshot M3 bằng binary tương ứng. Hai editor và race Save với đổi theme giữ một nguồn public nhất quán.

#### Kiểm thử, rủi ro và rollback

Build locked, test Rust, Builder HTTP/SQLite smoke trên site copy và chạy lại Themes/public content/media smoke. Kiểm snapshot restore bằng binary M3 thật. Chốt số migration theo main trước merge.

#### Bàn giao và review

PR trên `task/forge-m4-s02-t01` tới `main`, ghi approved source SHA, implementation base/head, lệnh, kết quả và giới hạn. Project Done sau G5 có evidence.

### M4-S03-T01 — Quản lý category cha–con và tag trong Admin

- **Thuộc:** V1, M4, M4-S03, M4-P09.
- **Primary Acceptance Slice của capability:** M4-S03 cho CNT-03.1.
- **Module:** CNT-03
- **Capability:** CNT-03.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0007 § P09.
- **Người triển khai / review:** Harbor dự kiến. Atlas hoặc Nova review độc lập theo ownership.

#### Tổng quan và giá trị

Admin tạo, sửa, chuyển cấp và xóa category/tag theo quyền. Editor có thể đọc term để chọn trong các luồng gắn nội dung sau đó.

#### Mục tiêu và hành vi quan sát được

Category có parent hợp lệ, tag phẳng, slug duy nhất theo kind, đổi tên/slug giữ ID.

#### Phạm vi công việc

Thêm term storage, API CRUD, CAS, giới hạn độ sâu và kiểm cycle theo ADR-0007. Thêm màn quản lý term trong AdminShell dùng UI primitives và API guard hiện có.

#### Ngoài phạm vi

Gắn term lên entry và lọc thuộc P10. URL archive taxonomy và khai báo từ theme/plugin thuộc M5/M6.

#### Phụ thuộc và điều kiện chặn

ADR-0007 và auth/AdminShell M3. Có thể làm cùng P01 vì file riêng. Giữ lượt tích hợp router/storage/AdminShell và migration trước P02. Chốt tên migration với baseline M3 mới nhất.

#### Kỹ thuật và file ownership

Trong `cms/crates/opendash-server/src/` gồm `app.rs`, `error.rs`, `routes/mod.rs`, `routes/taxonomy.rs`, `storage/mod.rs`, `storage/taxonomy.rs`. Trong `cms/admin/src/` gồm `App.tsx`, `shell/AdminShell.tsx`, `api/index.ts`, `api/taxonomy.ts`, `features/taxonomy/TaxonomyPage.tsx`. Thêm `cms/migrations/0012_taxonomy_terms.sql` dự kiến, `cms/scripts/taxonomy_terms_smoke.py`, `cms/scripts/taxonomy_terms_browser.py`, `docs/verification/2026-11-11-m4-s03-t01-taxonomy-terms.md`. Mở rộng qua Design Delta trước khi sửa file ngoài danh sách.

#### Cách triển khai theo contract

Dùng [ADR-0007](../../../../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) và API lõi hiện có. Giữ triển khai SQL ở storage và ghi evidence theo SHA thực chạy.

#### Nghiệm thu

- [ ] Category có parent hợp lệ, tag phẳng, slug duy nhất theo kind, đổi tên/slug giữ ID.
- [ ] Cycle, sai kind, quá 8 cấp và hai reparent tranh nhau bị chặn đúng. Xóa category còn child trả 409.
- [ ] Admin mutation có CSRF/Origin và CAS. Editor đọc được và mutation trả 403. Guest 401. Browser có loading/error, keyboard và 390 px.
- [ ] Fresh và upgrade DB copy giữ Post/Page/CPT, term tree tồn tại qua restart.

#### Kiểm thử, rủi ro và rollback

Build Admin/Rust locked, test Rust, SQLite/HTTP smoke và browser thật. Kiểm hai kết nối reparent, restart, snapshot và rollout migration. P10 bổ sung hạn chế xóa term đang được gắn.

#### Bàn giao và review

PR trên `task/harbor-m4-s03-t01` tới `main`, ghi approved source SHA, implementation base/head, lệnh, kết quả và giới hạn. Project Done sau G5 có evidence.

### M4-S04-T01 — Gắn category/tag và lọc Post, Page, CPT

- **Thuộc:** V1, M4, M4-S04, M4-P10.
- **Primary Acceptance Slice của capability:** M4-S04 cho CNT-03.2.
- **Module:** CNT-03
- **Capability:** CNT-03.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0007 § P10.
- **Người triển khai / review:** Harbor dự kiến. Atlas hoặc Nova review độc lập theo ownership.

#### Tổng quan và giá trị

Admin/Editor có thể gắn term khi sửa entry. List lọc theo đúng category/tag đã chọn, tùy chọn gồm mục con và không làm sai phân trang hay publication.

#### Mục tiêu và hành vi quan sát được

Post/Page/CPT gắn/gỡ term. Omit giữ links, `[]` gỡ hết, version stale 412 và request lỗi không đổi entry hoặc links.

#### Phạm vi công việc

Thêm bảng nối entry-term và FK, `term_ids` tùy chọn trên API hiện có, version chung với entry, bộ lọc storage dùng cho Admin và P06. Nối picker và bộ lọc với Posts/Pages/Content.

#### Ngoài phạm vi

Public taxonomy URL và UI Builder list block thuộc P06/M5. Giữ CPT publication policy và ACL media hiện hành.

#### Phụ thuộc và điều kiện chặn

P09 và M3 entry contract đã merge. Phần riêng có thể chuẩn bị trong khi P02 chạy. Chỉ sửa `storage/public_content.rs` sau khi P02 bàn giao và kiểm lại predicate ACL mới. Chốt migration number sau P02.

#### Kỹ thuật và file ownership

`cms/crates/opendash-core/src/content.rs`. Trong `cms/crates/opendash-server/src/` gồm `routes/entries.rs`, `storage/entries.rs`, `storage/taxonomy.rs`, `storage/public_content.rs`. Trong `cms/admin/src/` gồm `api/content.ts`, `api/postPage.ts`, `api/taxonomy.ts`, `features/content/types.ts`, `features/content/ContentWorkspace.tsx`, `features/posts/PostPageWorkspace.tsx`, `features/taxonomy/TermPicker.tsx`. Thêm `cms/migrations/0014_entry_terms.sql` dự kiến, `cms/scripts/entry_terms_smoke.py`, `cms/scripts/entry_terms_browser.py`, `docs/verification/2026-11-11-m4-s04-t01-entry-terms.md`. Mở rộng qua Design Delta trước khi sửa file ngoài danh sách.

#### Cách triển khai theo contract

Dùng [ADR-0007](../../../../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) và API lõi hiện có. Giữ triển khai SQL ở storage và ghi evidence theo SHA thực chạy.

#### Nghiệm thu

- [ ] Post/Page/CPT gắn/gỡ term. Omit giữ links, `[]` gỡ hết, version stale 412 và request lỗi không đổi entry hoặc links.
- [ ] Đổi slug/reparent giữ links. Xóa entry dọn link và giữ term. Xóa term còn link trả 409. Kiểm giới hạn số term, IDs trùng và term đã bị xóa.
- [ ] Exact, include descendants và category+tag lọc trước phân trang, không nhân đôi row. Draft khớp filter không lộ qua public và không ảnh hưởng đếm.
- [ ] Browser chọn/lọc được category/tag, giữ draft khi 412 và giữ links qua restart.

#### Kiểm thử, rủi ro và rollback

Build locked, test Rust, HTTP/browser trên site fixture và chạy lại entry/public/media smoke. Kiểm FK, race term delete với entry update, migration snapshot restore và không chỉnh migration đã phát hành.

#### Bàn giao và review

PR trên `task/harbor-m4-s04-t01` tới `main`, ghi approved source SHA, implementation base/head, lệnh, kết quả và giới hạn. Project Done sau G5 có evidence.
