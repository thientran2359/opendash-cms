# M4 · Đặc tả bàn giao cho các phần còn lại

**Atlas · Bản nháp G2/G3 · Baseline `main@62d85ece47813d90079c78b3b84aae1a339a0d98`.** [Kế hoạch M4](../../docs/plans/milestones/M4/TASK_PLAN.md) · [ADR-0007](../../docs/decisions/ADR-0007-m4-builder-taxonomy-foundation.md) · [Quy trình G3](../../docs/plans/PLANNING_WORKFLOW.md). Dùng mã candidate `M4-Pxx` cho phần đang thiết kế. Sau khi đặc tả G3 có hiệu lực, dùng mã triển khai `M4-Sxx-Txx` trong trao đổi, commit và PR. GitHub Issue và Project ghi trạng thái và liên kết mã task với đặc tả trên `main`.

P01/P02 và P09/P10 đã hoàn thành. [Field Binding P05](https://github.com/thientran2359/opendash-cms/pull/93) có đặc tả do Harbor viết, mã `M4-S05-T01` nằm trong PR đó và chờ quy trình tích hợp. Các gói dưới đây chờ phản biện G2, G3 chính thức và xác minh Project Ready trước khi nhận triển khai. Forge và Harbor tự chủ khi task đủ điều kiện theo [ADR-0008](../../docs/decisions/ADR-0008-harbor-forge-autonomous-delivery.md).

| Candidate | Người làm dự kiến | Thứ tự và file dùng chung |
| --- | --- | --- |
| P03 · Editor | Forge | Làm song song với P05. Forge giữ UI workspace. Harbor giữ renderer/backend binding. |
| P04 · Responsive | Forge | Sau P03. Đồng bộ với P05 trước khi đổi schema/renderer. Forge giữ lượt sửa `builder.rs` và `page.rs` cho responsive trước khi P06 tích hợp vào hai file đó. |
| P06 · List block | Harbor | Sau P05 và taxonomy P09/P10. Có thể chuẩn bị storage/query riêng khi P04 đang làm. Ghi bàn giao `builder.rs`, `page.rs` và phần UI inspector trước lúc tích hợp. |
| P07 · Kiểm thử luồng Editor | Forge | Sau P03–P06. Dùng file browser regression riêng. |
| P08 · Nghiệm thu M4 | Harbor điều phối, Forge chạy Windows | Sau P07. Cùng source SHA, báo cáo theo hệ điều hành và bàn giao M5. |

## M4-P03 · Tree, Canvas và Inspector

**Hành vi.** Workspace dùng GET homepage để lấy cây/version, POST preview để nhận HTML/chẩn đoán và PUT `If-Match` để lưu. Người dùng chọn cùng node từ tree hoặc canvas chỉnh sửa và sửa props ở inspector. Cây và canvas dùng chung ID. Đề xuất dùng canvas do Admin dựng từ dữ liệu có kiểu, còn Preview giao diện theme hiển thị HTML server trong iframe sandbox. Cần xác minh trên Builder thật rằng thao tác chọn, cuộn và focus hoạt động với layout legacy lẫn layout cây trước khi chốt cách làm. [Đề xuất P03 và thử nghiệm Chrome cô lập](2026-11-11-m4-p03-builder-editor-g2.md).

**Phạm vi giao dự kiến.** `cms/admin/src/App.tsx` cho mount/session, `cms/admin/src/api/builder.ts` và `api/index.ts` cho API, `cms/admin/src/builder/` cho `BuilderWorkspace.tsx`, `BuilderTree.tsx`, `BuilderCanvas.tsx`, `BuilderInspector.tsx` và trạng thái draft, `cms/scripts/builder_editor_browser.py` cho browser. UI giữ draft qua chuyển màn và phiên hết hạn nếu đúng tài khoản. Dùng đúng version cho Save, bỏ response Preview đến muộn và giữ draft khi 412. Gói này dự kiến dùng API P02 hiện có. Field binding, responsive và list block có gói riêng.

**Nghiệm thu.** Mở homepage M3 và document cây, chọn một node từ tree rồi từ canvas, thêm/xóa/reorder bằng chuột và bàn phím, sửa props và giữ ID. Kiểm legacy thiếu renderer, 401 cùng tài khoản và tài khoản khác, 412 hai tab, Preview chưa lưu và Save cập nhật `/`. Browser dùng site fixture, không dùng DB đang chạy của người dùng. Phương án kiểm chứng gồm Admin typecheck/build, P02 smoke và browser Editor mới. Rollback dùng code phù hợp và snapshot đã xác minh nếu có thay đổi định dạng sau Save.

## M4-P04 · Responsive desktop, tablet, mobile

**Hành vi đề xuất.** Cho phép ghi override có kiểu theo viewport trên cùng node. Desktop dùng giá trị cơ sở, tablet kế thừa desktop, mobile kế thừa tablet rồi áp dụng override riêng. G2 cần chốt breakpoint cụ thể, allowlist thuộc tính đầu tiên và cơ chế tương thích version với binding P05. Khi xóa override, giá trị kế thừa hiện lại. Preview viewport và CSS public dùng chung quy tắc.

**Phạm vi giao dự kiến.** `cms/crates/opendash-core/src/builder.rs` cho schema/validation, `cms/crates/opendash-server/src/page.rs` cho CSS từ thuộc tính đã kiểm, các file editor trong `cms/admin/src/builder/` và `cms/scripts/builder_responsive_browser.py`. Tái dùng document, renderer và API Save. Giới hạn CSS vào thuộc tính/giá trị cho phép. Tách runtime và UI nếu quyền sở hữu không thể bàn giao trong cùng lượt.

**Nghiệm thu.** Một document lưu rồi mở lại giữ cả ba viewport, đổi một override không làm sai viewport khác, xóa override khôi phục kế thừa. Kiểm hai phía breakpoint, giá trị sai, layout v1 cũ và Preview/public giống quy tắc. Kiểm Rust locked và Chrome trên DB copy, giữ rollback bằng snapshot/version tương ứng.

## M4-P06 · Khối danh sách và lọc taxonomy

**Hành vi đề xuất.** Một list block chọn content type, category, tùy chọn gồm mục con, tag, thứ tự và số mục mỗi trang. Storage dùng `EntryFilters` và `list_published_filtered` hiện có, lọc trước phân trang. G2 cần chốt sort allowlist, giới hạn page size/offset, tham số phân trang theo ID block nếu có nhiều list và projection field tối thiểu. Term đã bị xóa cho danh sách rỗng và chẩn đoán Admin. Kết quả public chỉ lấy published theo ADR-0007. P05 phụ trách binding tới một entry xác định, P06 phụ trách tập entry.

**Phạm vi giao dự kiến.** `cms/crates/opendash-server/src/storage/entries.rs`, `storage/public_content.rs` và file query riêng nếu cần. Sau bàn giao, nối `cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/page.rs` và inspector trong `cms/admin/src/builder/`. Thêm `cms/scripts/builder_list_smoke.py` và browser list nếu có UI. Giữ URL `/content/{type}/{slug}` hiện hành, site data và policy media ACL.

**Nghiệm thu.** Fixture có category cha/con, tag và draft khớp lọc. Kiểm exact/descendants, AND category+tag, sort ổn định, trang kế tiếp, hai list block độc lập và term mất. Draft không xuất hiện ở danh sách, count hoặc pagination public. Preview có session và cảnh báo hợp lệ. Chạy locked tests, query/HTTP smoke, browser và regression public content/media trên site copy.

## M4-P07 · Nghiệm thu luồng Preview và Save

**Hành vi.** Từ Admin, mở layout đã lưu, chỉnh cấu trúc, Preview thấy bản đang chỉnh và `/` vẫn trả phiên bản trước đó. Save thành công cập nhật public, reload/restart giữ layout. Khi phiên hết hạn hoặc hai tab tranh ghi, người dùng giữ được draft theo contract P03 và ADR-0007.

**Phạm vi giao dự kiến.** Forge sở hữu `cms/scripts/builder_editor_integration_browser.py` và một báo cáo `docs/verification/` có mã task. Tái dùng fixture P02/P03, thêm ca responsive, binding và list. Nếu thấy lỗi sản phẩm, ghi bằng chứng, phạm vi và giao lại đúng owner trước khi sửa runtime.

**Nghiệm thu.** Hai tab với cùng ETag cho một Save thành công và một 412. Guest bị chặn, sai CSRF/Origin bị từ chối, cùng account khôi phục draft sau 401. Preview chưa lưu giữ public/media ACL. Hành vi sau Save được xác nhận qua browser, HTTP và restart trên bản sao SQLite.

## M4-P08 · Nghiệm thu M4 và bàn giao sang M5

**Hành vi.** Chạy cùng source SHA cho macOS và Windows sau khi các gói phụ thuộc tích hợp. Ghi rõ SHA, binary, browser, fixture, lệnh và kết quả. Handoff M5 gồm các contract AST, responsive, binding, list và taxonomy đã chạy được, cùng các hành vi bảo toàn layout khi đổi theme.

**Phạm vi giao dự kiến.** Harbor giữ runner/evidence macOS, Forge giữ runner/evidence Windows. Dùng `cms/scripts/builder_homepage_smoke.py`, `themes_smoke.py`, `media_smoke.py`, `public_content_smoke.py`, `entry_terms_smoke.py` và browser suites M4. Mỗi OS ghi file verification riêng, một người sở hữu cập nhật tổng hợp. Chuẩn bị fixture layout M3, layout cây, category/tag, media private/public và site snapshot rollback.

**Nghiệm thu.** Mở/sửa/Preview/Save/reopen trên hai OS, ba viewport, binding/list có quyền đúng, đổi theme A→B→C→A, legacy ZIP, node thiếu provider, sandbox/CSP và dữ liệu nháp. Chạy migration trên bản sao, so sánh dữ liệu và khôi phục với binary tương ứng. G5 cần đủ bằng chứng trên SHA đã chốt trước khi đóng M4.

**Giới hạn hiện tại.** Đây là bản bàn giao dự thảo dựa trên source đã đọc và kế hoạch M4. Các ca nghiệm thu trong tài liệu chưa chạy. P03 còn cần kiểm nghiệm thực tế trên Builder, P04 và P06 còn quyết định G2 như đã ghi ở từng mục. Các mã triển khai còn lại được cấp khi đặc tả G3 có hiệu lực.
