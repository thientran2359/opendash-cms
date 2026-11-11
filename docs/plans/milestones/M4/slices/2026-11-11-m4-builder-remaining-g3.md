# M4 — G3 cho toàn bộ phần còn lại của Visual Builder

**Final Decision:** Atlas, theo [ADR-0010](../../../../decisions/ADR-0010-agent-project-and-merge-authority.md). **G2:** [ADR-0011](../../../../decisions/ADR-0011-m4-builder-editor-responsive-list.md). **G3:** đặc tả được duyệt khi merge vào main. `READY` trong từng brief là mức hoàn chỉnh của tài liệu để phát hành Issue; Project `Ready` cần xác minh dependency, owner và file handoff tại thời điểm nhận. Baseline audit `7b7e2201f475950d5a17bde7b0d417eb6a19351a`.

P01/P02/P09/P10 dùng [G3 nền tảng](2026-11-11-m4-foundation-g3.md). P05 backend dùng [G3 Field Binding](2026-11-11-m4-field-binding-g3.md) và đã G5. Các brief dưới đây bao phủ phần còn lại của [candidate plan](../TASK_PLAN.md). `M4-S06-T01` và `M4-S08-T01` có thể chạy song song. Forge bàn giao UI/renderer sau `M4-S07-T01`; Harbor sửa Inspector cho Field Binding rồi tích hợp list. Mỗi agent một task triển khai active trên branch của mình. Dùng snapshot/site tạm và ghi SHA thực vào bằng chứng.

| Candidate | Mã task còn lại | Thứ tự |
| --- | --- | --- |
| M4-P03 | M4-S06-T01 | Editor trước responsive |
| M4-P04 | M4-S07-T01 | Sau Editor |
| M4-P05 | M4-S05-T02 | Inspector sau responsive |
| M4-P06 | M4-S08-T01, M4-S08-T02 | Query trước; tích hợp sau responsive và Binding Inspector |
| M4-P07 | M4-S09-T01 | Sau toàn bộ Editor/Responsive/Binding/List |
| M4-P08 | M4-S10-T01, M4-S10-T02, M4-S10-T03 | macOS và Windows cùng SHA; tổng hợp sau cả hai |

### M4-S06-T01 — Tree, canvas và inspector homepage

- **Module:** BLD-03
- **Capability:** BLD-03.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Forge · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Editor M3 chỉ chỉnh một block; workspace cần chỉnh cây homepage.

#### Mục tiêu và hành vi quan sát được

Chọn cùng node từ tree hoặc canvas, sửa props, Preview và Lưu với ETag.

#### Phạm vi công việc

Thực hiện quyết định Editor trong ADR-0011. Mount workspace từ Admin, tạo ID theo core validator và quản lý immutable draft/revision. Tree hỗ trợ bàn phím, focus và add/delete/reorder; inspector giữ dữ liệu khác trên node khi sửa. Preview HTML opaque, canvas cấu trúc chỉ render DOM kiểm soát. Bảo toàn cả props binding đã lưu khi client chỉnh static props.

#### Ngoài phạm vi

Responsive, UI Field Binding, list, HTML theme tương tác trong Editor, persistence bản nháp qua reload.

#### Phụ thuộc và điều kiện chặn

M4-S01-T01 và M4-S02-T01 đã hoàn tất. M4-S05-T01 đã merge, cần giữ định dạng binding khi round-trip. Forge giữ UI đến lúc bàn giao cho M4-S07-T01.

#### Kỹ thuật và file ownership

Forge sở hữu `cms/admin/src/App.tsx`, `cms/admin/src/api/builder.ts`, `cms/admin/src/api/index.ts`, `cms/admin/src/builder/BuilderWorkspace.tsx`, `BuilderTree.tsx`, `BuilderCanvas.tsx`, `BuilderInspector.tsx`, `document.ts` trong cùng thư mục và `cms/scripts/builder_editor_browser.py`. Dùng `requestJson`, session hiện hành và homepage API P02; giữ CSP/router/storage. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Tree ↔ canvas chọn theo ID, focus bàn phím, giữ ID khi add/reorder và giữ props hợp lệ sau round-trip.
- [ ] Mở homepage legacy và tree; GET/Preview không ghi; node không resolve hiển thị trong editor, giữ neo và chỉ xóa sau xác nhận.
- [ ] Preview stale bị loại; 412 giữ nháp; 401 cùng user khôi phục, user khác/logout xóa; edit sau snapshot PUT vẫn còn.
- [ ] HTML theme hiển thị đúng trong sandbox opaque, script/điều hướng top không chạy; Preview nháp giữ quyền media.

#### Kiểm thử, rủi ro và rollback

Admin typecheck/build, Builder homepage HTTP smoke và Chrome browser test trên site tạm. Ghi SHA, API response và bằng chứng keyboard/401/412; rollback bằng binary và snapshot đọc được.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S07-T01 — Responsive homepage ba viewport

- **Module:** BLD-04
- **Capability:** BLD-04.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Forge · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Người biên tập cần sửa bố cục homepage theo kích thước màn hình.

#### Mục tiêu và hành vi quan sát được

Cùng document cho ba viewport, cascade dự đoán được và Save/reopen giữ overrides.

#### Phạm vi công việc

Triển khai schema, allowlist, breakpoint, cascade, Row grid opt-in và CSS từ ADR-0011; Inspector có chọn viewport và xóa override; render cùng CSS cho Preview và public.

#### Ngoài phạm vi

Visibility, arbitrary CSS, grid span, theme composition M5.

#### Phụ thuộc và điều kiện chặn

M4-S06-T01 tích hợp và bàn giao UI; M4-S05-T01 đã tích hợp schema binding; chốt head mới trước sửa.

#### Kỹ thuật và file ownership

`cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/page.rs`, `cms/admin/src/builder/document.ts`, `BuilderWorkspace.tsx`, `BuilderCanvas.tsx`, `BuilderInspector.tsx`, `cms/scripts/builder_responsive_browser.py`. Forge bàn giao file renderer/schema và Inspector sau merge cho lượt Harbor. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] 767/768/1023/1024 CSS px và breakpoint phân số đúng, hàng 3/2/1 cột đúng thứ tự.
- [ ] Kiểm cascade, giá trị 0, xóa override, Save/restart và computed style Preview/public.
- [ ] Payload sai kiểu/giới hạn bị chặn; document v1 không có responsive và binding hiện có giữ hành vi.

#### Kiểm thử, rủi ro và rollback

Rust fmt/build/tests locked, Admin typecheck/build, homepage smoke và browser responsive. Snapshot rollback trước lần lưu metadata mới.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S05-T02 — Cấu hình Field Binding trong Inspector

- **Module:** BLD-05
- **Capability:** BLD-05.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Harbor · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Backend binding đã đạt G5; người biên tập cần thao tác trong Admin.

#### Mục tiêu và hành vi quan sát được

Chọn Post/Page/CPT, entry và field tương thích cho core element rồi Preview/Lưu/gỡ binding.

#### Phạm vi công việc

Dùng listContentTypes/listContentEntries/getContentEntry, media picker hiện có và ma trận ADR-0009. Persist đúng type_key, entry_slug, source và target. Giữ binding khi sửa thuộc tính khác, thông báo diagnostic khi nguồn mất; giữ giá trị đã chọn khi lookup lỗi.

#### Ngoài phạm vi

API lookup mới, đổi quyền, đổi backend binding, list query.

#### Phụ thuộc và điều kiện chặn

M4-S05-T01 đã đạt G5; sau M4-S06-T01 và M4-S07-T01 bàn giao Inspector. Cùng agent với M4-S08-T01; hoàn tất một task trước khi nhận task tiếp theo.

#### Kỹ thuật và file ownership

`cms/admin/src/builder/BuilderInspector.tsx`, `BuilderBindingFields.tsx`, `document.ts` và `cms/scripts/builder_binding_inspector_browser.py`. Dùng API client hiện hành, không sửa routes/storage. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Chọn và lưu hợp lệ cho Heading/Text/Image/Button theo ADR-0009; reload rồi gỡ binding.
- [ ] Field không tương thích không chọn được; lookup lỗi/mất entry không làm mất draft hoặc binding.
- [ ] Admin/Editor qua quyền hiện có; Preview draft đúng quyền, public giữ published-only và media ACL.

#### Kiểm thử, rủi ro và rollback

Admin typecheck/build, Field Binding smoke và Chrome Inspector trên site tạm. Rollback UI giữ document binding đã lưu.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S08-T01 — Truy vấn published cho list block

- **Module:** BLD-05
- **Capability:** BLD-05.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Harbor · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

List block cần lấy đúng nội dung công khai theo CPT và taxonomy.

#### Mục tiêu và hành vi quan sát được

Truy vấn có kiểu trả tối đa page size và cờ trang tiếp theo, theo thứ tự ổn định.

#### Phạm vi công việc

Thêm hàm storage cho list dùng chung predicate của list_published_filtered/EntryFilters, enum id_desc/id_asc và kiểm type/term đúng loại; trả trạng thái cấu hình đã mất riêng với lỗi storage. Áp giới hạn ADR-0011 và lấy thêm một hàng. Giữ caller cũ dùng id_desc.

#### Ngoài phạm vi

AST, renderer, routes, UI, ACL mới và migration.

#### Phụ thuộc và điều kiện chặn

M4-S03-T01, M4-S04-T01 và M4-S05-T01 đã hoàn tất. Có thể làm song song M4-S06-T01 vì ownership độc lập.

#### Kỹ thuật và file ownership

`cms/crates/opendash-server/src/storage/entries.rs` gồm tests, và `storage/public_content.rs` chỉ nếu cần dùng chung predicate/helper. Không sửa `builder.rs`, `page.rs` hay Admin trong lượt này. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Fixture Post/Page/CPT, category cha/con, tag, draft chứng minh lọc published/type/AND trước phân trang.
- [ ] Sort tăng/giảm, exact/descendants, trang đầu/cuối và term/type bị xóa cho kết quả đúng.
- [ ] Caller hiện có giữ thứ tự và phân trang; giá trị ngoài biên bị chặn.

#### Kiểm thử, rủi ro và rollback

Rust fmt/build/test locked và public_content/entry_terms smoke trên SQLite tạm. Không cần đổi DB schema.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S08-T02 — Tích hợp list block vào Builder và homepage

- **Module:** BLD-05
- **Capability:** BLD-05.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Harbor · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Người biên tập cần tạo danh sách nội dung động trên homepage.

#### Mục tiêu và hành vi quan sát được

Inspector cấu hình list, Preview trang đầu, public phân trang riêng cho từng node.

#### Phạm vi công việc

Nối query M4-S08-T01 theo ADR-0011. Resolve list trước render theo lifecycle hiện có; Save phải dùng validation/render hiện hành và tránh query ngoài snapshot ghi khi cần nhất quán. Cấu hình tham chiếu bị mất vẫn được round-trip. Public render canonical links và giữ pagination hợp lệ của list khác; bỏ tham số dư.

#### Ngoài phạm vi

Custom item template, ảnh/ACL, routes mới và URL policy M5.

#### Phụ thuộc và điều kiện chặn

M4-S08-T01 đã merge và kiểm query; M4-S07-T01 và M4-S05-T02 hoàn tất bàn giao renderer/schema và Inspector. Bắt đầu từ SHA chứa các task này.

#### Kỹ thuật và file ownership

`cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/page.rs`, `routes/public.rs`, `routes/builder.rs`, `storage/builder.rs` trong server, `cms/admin/src/builder/BuilderInspector.tsx`, `BuilderListFields.tsx`, `BuilderCanvas.tsx`, `document.ts`, `cms/scripts/builder_list_smoke.py` và `cms/scripts/builder_list_browser.py`. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Hai list độc lập; params thiếu/trùng/sai và trang ngoài phạm vi xử lý đúng.
- [ ] List preview và public không lộ draft; publish/unpublish đổi kết quả, title/URL được escape đúng.
- [ ] Term/CPT bị xóa hiển thị trạng thái rỗng + diagnostics; fixture legacy/binding/responsive vẫn hoạt động.
- [ ] Kiểm giới hạn 8 list, 20 mục, trang 100 và giữ ETag khi Save.

#### Kiểm thử, rủi ro và rollback

Rust fmt/build/test locked, Admin typecheck/build, list smoke/browser và hồi quy public content/media. Snapshot trước Save document có list để rollback binary cũ.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S09-T01 — Nghiệm thu tích hợp Preview và Save

- **Module:** BLD-03
- **Capability:** BLD-03.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Forge · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Kiểm xuyên suốt các phần Builder sau khi tích hợp.

#### Mục tiêu và hành vi quan sát được

Mở → chỉnh → Preview → Lưu → mở lại và restart trên fixture SQLite; kiểm quyền và xung đột.

#### Phạm vi công việc

Dùng các runner Builder hiện hành để chuẩn bị site, thêm ca tích hợp ở browser. Kiểm hai tab cùng ETag, Preview không ghi, 401 recovery đúng account, CSRF/Origin, responsive/binding/list và đổi theme A→B→A. Ghi lỗi runtime riêng để owner sửa.

#### Ngoài phạm vi

Sửa runtime trong task kiểm thử; deploy và đóng milestone.

#### Phụ thuộc và điều kiện chặn

M4-S06-T01, M4-S07-T01, M4-S05-T02 và M4-S08-T02 đã tích hợp với evidence; dùng source SHA chứa toàn bộ.

#### Kỹ thuật và file ownership

`cms/scripts/builder_editor_integration_browser.py` và `docs/verification/2026-11-11-m4-s09-t01-builder-integration.md`. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Preview đổi theo draft, public giữ bản cũ đến Save; reload/restart giữ bản đã lưu.
- [ ] Hai tab: một Save, một 412; Draft qua 401 đúng account; CSRF/Origin sai bị chặn.
- [ ] Theme không hỗ trợ ẩn block ngoài public nhưng dữ liệu Admin vẫn giữ; đổi lại theme render đúng node/nội dung.
- [ ] Responsive, binding và list pass trong cùng fixture, gồm negative cases đã chốt.

#### Kiểm thử, rủi ro và rollback

Homepage smoke và Chrome E2E trên site tạm. Ghi SHA, browser, fixture, lệnh và PASS/FAIL/NOT_RUN; lỗi bắt buộc chuyển owner và chạy lại trước release.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S10-T01 — Nghiệm thu release M4 trên macOS

- **Module:** BLD-01
- **Capability:** BLD-01.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Harbor · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Xác minh release M4 trên macOS với site nâng cấp và snapshot.

#### Mục tiêu và hành vi quan sát được

Mở/chỉnh/Lưu homepage, đổi theme, kiểm taxonomy và khôi phục bằng binary tương ứng.

#### Phạm vi công việc

Chạy locked build/tests và các Builder, theme, media, public content, entry terms smokes. Chạy browser tích hợp M4; nâng site M3 fixture, kiểm layout legacy/tree, theme ZIP, media private/public, quyền, taxonomy và backup/restore. Ghi OS, browser, binary và source SHA.

#### Ngoài phạm vi

Sửa runtime, dùng DB người dùng, deploy hoặc báo G5 khi còn ca bắt buộc chưa chạy.

#### Phụ thuộc và điều kiện chặn

M4-S09-T01 PASS, mọi task triển khai đã tích hợp. Atlas ghi cùng release SHA cho hai OS; dùng đúng binary từ SHA đó.

#### Kỹ thuật và file ownership

`docs/verification/2026-11-11-m4-s10-t01-macos.md` và runner riêng theo OS `cms/scripts/m4_release_macos.py` nếu runner hiện có chưa gom đủ ca. Không sửa runner của agent khác. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Browser E2E, responsive, binding/list/taxonomy và sandbox/media quyền đúng.
- [ ] Đổi A→B→C→A giữ homepage Admin, block không hỗ trợ ẩn rồi render lại đúng.
- [ ] Nâng cấp bản sao SQLite và restore snapshot bằng binary tương ứng; so sánh hành vi và dữ liệu sau restart.

#### Kiểm thử, rủi ro và rollback

Fixture và binary cô lập. Ghi lệnh, checksum/binary và release SHA. Regression bắt buộc phải xử lý và xác minh trên SHA release đã chốt.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S10-T02 — Nghiệm thu release M4 trên Windows

- **Module:** BLD-01
- **Capability:** BLD-01.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Forge · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Xác minh release M4 trên Windows với site nâng cấp và snapshot.

#### Mục tiêu và hành vi quan sát được

Mở/chỉnh/Lưu homepage, đổi theme, kiểm taxonomy và khôi phục bằng binary tương ứng.

#### Phạm vi công việc

Chạy locked build/tests và các Builder, theme, media, public content, entry terms smokes. Chạy browser tích hợp M4; nâng site M3 fixture, kiểm layout legacy/tree, theme ZIP, media private/public, quyền, taxonomy và backup/restore. Ghi OS, browser, binary và source SHA.

#### Ngoài phạm vi

Sửa runtime, dùng DB người dùng, deploy hoặc báo G5 khi còn ca bắt buộc chưa chạy.

#### Phụ thuộc và điều kiện chặn

M4-S09-T01 PASS, mọi task triển khai đã tích hợp. Atlas ghi cùng release SHA cho hai OS; dùng đúng binary từ SHA đó.

#### Kỹ thuật và file ownership

`docs/verification/2026-11-11-m4-s10-t02-windows.md` và runner riêng theo OS `cms/scripts/m4_release_windows.py` nếu runner hiện có chưa gom đủ ca. Không sửa runner của agent khác. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Browser E2E, responsive, binding/list/taxonomy và sandbox/media quyền đúng.
- [ ] Đổi A→B→C→A giữ homepage Admin, block không hỗ trợ ẩn rồi render lại đúng.
- [ ] Nâng cấp bản sao SQLite và restore snapshot bằng binary tương ứng; so sánh hành vi và dữ liệu sau restart.

#### Kiểm thử, rủi ro và rollback

Fixture và binary cô lập. Ghi lệnh, checksum/binary và release SHA. Regression bắt buộc phải xử lý và xác minh trên SHA release đã chốt.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.

### M4-S10-T03 — Tổng hợp nghiệm thu M4 và bàn giao M5

- **Module:** BLD-01
- **Capability:** BLD-01.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Người triển khai / review:** Atlas · self-review theo ADR-0010. Atlas điều phối.

#### Tổng quan và giá trị

Khép milestone với đủ bằng chứng và contract để Theme M5 dùng lại.

#### Mục tiêu và hành vi quan sát được

Đối chiếu hai OS cùng release SHA và ghi các contract đã kiểm chứng.

#### Phạm vi công việc

Tổng hợp G5, checklist coverage, backward compatibility và open caveats. Bàn giao AST, layout/theme, responsive, field binding, taxonomy/list, ACL, giới hạn và đường dẫn evidence cho M5. Chỉ cập nhật trạng thái milestone sau khi chứng cứ thực đạt.

#### Ngoài phạm vi

Runtime, migration mới, production deploy, đóng Issue thiếu evidence.

#### Phụ thuộc và điều kiện chặn

M4-S10-T01 và M4-S10-T02 PASS cùng release SHA. Các G5 bắt buộc khác đã verified.

#### Kỹ thuật và file ownership

`docs/verification/2026-11-11-m4-release.md`, `docs/plans/milestones/M4/README.md` và `docs/plans/TASK_INDEX.md` sau khi xác minh GitHub. Thay đổi contract hoặc mở rộng ownership phải ghi Design Delta và xử lý trước khi sửa.

#### Nghiệm thu

- [ ] Hai báo cáo đủ source/binary SHA, OS, lệnh, kết quả và rollback, không có gate bắt buộc chưa đạt.
- [ ] Contract bàn giao cho M5 có link nguồn và evidence; GitHub Done/closure đối chiếu đúng.

#### Kiểm thử, rủi ro và rollback

Chạy planning checker, self-test, diff check; đối chiếu SHA, Issue/Project và evidence trước đóng mốc.

#### Bàn giao và review

PR ghi mã task, immutable G3 source SHA, base SHA, head SHA, self-review và evidence. Xác minh riêng G5 trước khi đánh Done.
