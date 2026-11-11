# M4 · Đặc tả responsive, list block và nghiệm thu

**Atlas · G2 PROPOSED · G3 DRAFT · Source audit `main@fa0dad08fc67958a27efd0c5d96b84a7b85ddd1e`.** Phạm vi là homepage Builder. Tài liệu áp dụng [kế hoạch M4](../../docs/plans/milestones/M4/TASK_PLAN.md), [ADR-0007](../../docs/decisions/ADR-0007-m4-builder-taxonomy-foundation.md) và [ADR-0008](../../docs/decisions/ADR-0008-harbor-forge-autonomous-delivery.md). Dùng mã candidate trong trao đổi thiết kế và mã task sau khi G3 có hiệu lực. GitHub Issue và Project ghi trạng thái, chủ sở hữu, source SHA và liên kết nghiệm thu.

## Trình tự và quyền sửa

| Phần | Người làm dự kiến | Điều kiện vào và bàn giao |
| --- | --- | --- |
| M4-P03 · Editor | Forge | Theo [đề xuất G2 của Forge](https://github.com/thientran2359/opendash-cms/pull/96). G2 cần quyết định cách chọn node và qua security review trước G3. UI có thể làm song song với Field Binding backend. |
| M4-S05-T01 · Field Binding | Harbor | Dùng [ADR-0009](../../docs/decisions/ADR-0009-m4-core-element-field-binding.md) và [G3](../../docs/plans/milestones/M4/slices/2026-11-11-m4-field-binding-g3.md). Phát hành tại [task triển khai](https://github.com/thientran2359/opendash-cms/issues/94). Handoff hiện có là API. |
| M4-P04 · Responsive | Forge | Sau Editor. Nhận `builder.rs` và `page.rs` trên SHA đã tích hợp Field Binding; giữ quyền sửa workspace trong lượt này. |
| M4-P06 · List block | Harbor | Sau Field Binding và taxonomy. Sau khi có G3, có thể làm query/test storage song song với responsive. Nhận các file schema, renderer, routes và Inspector ở lượt tích hợp sau responsive. |
| M4-P07 · Preview/Save | Forge | Sau P03–P06. Forge sở hữu test tích hợp; giao lỗi sản phẩm cho đúng owner. |
| M4-P08 · Nghiệm thu M4 | Harbor điều phối, Forge chạy Windows | Sau P07. Chốt cùng một release SHA; mỗi agent ghi evidence riêng. |

Atlas điều phối G2/G3 và bàn giao. Trước mỗi lượt sửa file chung, ghi owner, tên file và SHA chứa thay đổi đã tích hợp trong Issue triển khai. Cấp mã triển khai mới, cập nhật nguồn đặc tả trên `main` và kiểm tra ownership/Project Ready theo [Planning Workflow](../../docs/plans/PLANNING_WORKFLOW.md). Mỗi agent giữ một task triển khai đang hoạt động trên branch của mình.

**Điểm nối Editor.** Chốt một `BuilderDocument` draft và một `selectedNodeId` cho Tree/Canvas/Inspector, duy trì ETag và giữ node thiếu renderer ở tree. Atlas đề xuất giữ Preview sandbox opaque cho HTML theme và dùng canvas do Admin dựng cho thao tác chọn node. Forge đề xuất bắt sự kiện trực tiếp từ iframe `allow-same-origin`. Quyết định P03 cần bằng chứng trên Builder thật về chọn/scroll/focus, ngăn điều hướng từ HTML theme, CSP và quyền media. [Thử nghiệm Chrome cô lập của Atlas](2026-11-11-m4-p03-builder-editor-g2.md).

**Điểm nối Field Binding.** G3 của Harbor bao phủ API. Atlas đề xuất một handoff riêng để người biên tập cấu hình binding trong Inspector ở M4. Harbor nhận phần UI sau lượt Forge sửa Inspector cho P04 và có thể làm trong lượt sở hữu Inspector của mình, với mỗi task được nghiệm thu riêng. UI chỉ chỉnh kiểu nguồn/đích được ADR-0009 cho phép, giữ binding đã lưu khi chỉnh props khác, hỗ trợ gỡ binding và hiển thị diagnostics. Nghiệm thu chọn source Post/Page/CPT, field không hợp lệ, gỡ binding và reload. Tái dùng API Admin cho content types, fields và entries sau khi đối chiếu quyền; nếu thiếu lookup, review API mới trước khi mở rộng. P07 kiểm backend và bổ sung ca UI khi handoff này được duyệt.

## M4-P04 · Responsive

**Thiết kế đề xuất.** Desktop từ 1024 CSS px, tablet từ 768 đến dưới 1024, mobile dưới 768. Thuộc tính có desktop làm giá trị cơ sở, tablet kế thừa desktop, mobile kế thừa tablet. Xóa giá trị ở một viewport đưa thuộc tính đó về giá trị kế thừa. Giá trị `0` là override hợp lệ. Áp CSS theo thứ tự desktop, `@media (width < 1024px)`, `@media (width < 768px)`.

| Node | Thuộc tính ở mỗi viewport | Giá trị hợp lệ |
| --- | --- | --- |
| Section, Row, Column | `padding_px` | Số nguyên 0–64 |
| Row | `gap_px` | Số nguyên 0–64 |
| Row | `columns_per_row` | Số nguyên 1–6 |
| Column | `text_align` | `start`, `center`, `end` |

- Đặt object tùy chọn `responsive` trên container với khóa `desktop/tablet/mobile`. Mỗi khóa là object thưa theo allowlist trên. Document v1 thiếu object này giữ hành vi cũ; serializer bỏ object rỗng. Validate theo từng loại node, từ chối khóa lạ, số lẻ/ngoài khoảng và metadata trên Element. Giữ giới hạn 512 node/256 KiB và kiểm tra bảo toàn node không resolve. Không có migration.
- Tạo CSS từ thuộc tính/giá trị đã kiểm trên wrapper của Builder; scope dưới root do renderer sở hữu, theo node ID hợp lệ. Renderer hiện chỉ có wrapper Row. Row bật grid khi có `gap_px` hoặc `columns_per_row` ở bất kỳ viewport nào; grid mặc định 1 cột, cột có bề rộng bằng nhau và giữ thứ tự AST khi xuống dòng. Xóa hết hai thuộc tính trên đưa row về hành vi cũ. Preview đổi chiều rộng viewport thực, dùng cùng CSS với public. Chế độ đang chọn thuộc UI state; chỉ giá trị sửa trong Inspector vào document.
- Sau P03, Forge sở hữu `cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/page.rs`, các thành phần Inspector/Preview trong `cms/admin/src/builder/` và `cms/scripts/builder_responsive_browser.py` mới. Xác nhận tên file UI theo SHA P03 khi cấp G3. Full-Track vì thêm metadata vào document và public render. Grid spans, visibility và CSS tự do nằm ngoài slice này.
- **Nghiệm thu.** Kiểm 767/768/1023/1024 px và breakpoint phân số khi zoom. Fixture 3/2/1 cột giữ đúng thứ tự khi xuống dòng. Mở lại document sau Save, kiểm cascade theo từng thuộc tính, xóa override và giá trị 0. Preview và public có computed style tương ứng. Fixture v1 cũ giữ layout, binding và node thiếu provider. Payload sai nhận validation error, bản đã lưu giữ nguyên. Chạy Rust locked, Admin typecheck/build, Builder HTTP smoke và browser mới. Khi rollback về binary trước P04, dùng snapshot có document mà binary đó đọc được.

## M4-P06 · List block

**Thiết kế đề xuất.** Thêm `core/list` với `type_key`, `category_id?`, `tag_id?`, `include_descendants`, `sort` và `page_size`. Dùng ID taxonomy của site. Mặc định `include_descendants=false`, `sort=id_desc`, `page_size=6`. Allowlist sort `id_desc` và `id_asc` theo ID tạo; giao diện ghi “Mới tạo trước” và “Cũ tạo trước”. Category/tag lọc theo AND và phải đúng loại. Bật descendants yêu cầu category. Giá trị tối đa 20 mục/trang, 8 list trong một homepage và 100 trang/list. Giữ document v1, giới hạn node/byte và validator save hiện có.

- Dùng `EntryFilters` và mở rộng có kiểu cho sort ở truy vấn published dùng chung trong [storage entries](../../cms/crates/opendash-server/src/storage/entries.rs). Các caller hiện tại tiếp tục dùng `id_desc`. Lọc publication, type, category/tag trước LIMIT/OFFSET. Lấy thêm một record để xác định trang kế; cắt còn page size trước render. Tham số query được bind; enum sort chọn giữa hai thứ tự cố định. Không chạy truy vấn đếm riêng.
- Mỗi mục chỉ render title đã escape và URL canonical hiện hành `/content/{type}/{slug}`, dùng cùng logic đường dẫn public. Khi CPT/term mất hoặc sai loại, trả danh sách rỗng và diagnostic có node ID cho Admin. Lỗi storage đi qua `ApiError` hiện hành. Chỉ hiển thị nội dung published ở list trong cả public lẫn Preview. Giữ chính sách media hiện tại; ảnh, custom field projection và item templates để ngoài slice.
- Public `/` dùng `list_page_<node-id>` theo ID đã validate. Chấp nhận đúng một giá trị số nguyên 1–100; thiếu, sai hoặc trùng thì về trang 1. Bỏ tham số của node không còn trong document. Link Trang trước/Tiếp giữ giá trị hợp lệ của list khác, sinh tại site root, escape khi render. Trang hợp lệ ngoài dữ liệu hiển thị trạng thái rỗng và link về trang đầu; giới hạn trang 100 không tạo link tiếp. Preview và HTML trả về khi Save hiển thị trang đầu; chỉ public có navigation hoạt động.
- Harbor sở hữu `cms/crates/opendash-server/src/storage/entries.rs`, file truy vấn mới nếu cần và `cms/scripts/builder_list_smoke.py`. Khi vào lượt tích hợp, bàn giao rõ `cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/page.rs`, `routes/builder.rs`, `routes/public.rs` và Inspector sau P04. Chỉ thêm file registry/storage module nếu G3 xác định cần. Full-Track vì thêm block, query và hành vi public render. Không thêm route công khai mới hay migration.
- **Nghiệm thu.** Fixture có parent/child category, tag, draft mới hơn, Post/Page/CPT và hai list. Chứng minh exact/descendants, AND, sort hai chiều, pagination sau lọc, biên trang, hai list độc lập, term/CPT mất và input query sai. Chuyển draft sang published rồi unpublish và kiểm kết quả qua HTTP. Kiểm title/URL escape, Preview có session, không lộ draft qua mục hoặc chỉ báo trang kế. Chạy Rust locked, HTTP smoke, Admin typecheck/build, browser Inspector và regression public content/media.

## M4-P07 · Nghiệm thu luồng Preview và Save

**Handoff dự kiến.** Forge sở hữu `cms/scripts/builder_editor_integration_browser.py` và một báo cáo theo mã task trong `docs/verification/`. Dùng site/SQLite tạm, binary từ SHA tích hợp P03–P06 và fixtures P02/P03. Slice xác minh hành vi theo các contract đã duyệt; sửa runtime đi qua handoff riêng.

| Ca | Kết quả bắt buộc |
| --- | --- |
| Mở homepage M3 và homepage cây | GET/Preview giữ nguyên DB; thêm/sửa/reorder từ UI giữ ID và đúng thứ tự; layout cây vẫn mở được sau khi lưu. |
| Preview → Save → restart | Preview mới thấy draft, GET `/` vẫn cũ; Save thành công mới đổi `/`. Reload và restart giữ nội dung/phiên bản. Đối chiếu responsive, binding và list theo G3 đã merge. |
| Hai tab / response đến muộn | Cùng ETag cho một Save thành công và một 412. Tab thua giữ draft; Preview cũ không thay HTML/chẩn đoán của revision mới; Save trả về khi còn chỉnh sửa không làm mất edit sau snapshot. |
| Session và quyền | Guest bị chặn; CSRF/Origin sai bị từ chối; cùng user khôi phục draft sau 401; user khác hoặc logout chủ đích xóa draft khỏi UI. |
| Đổi theme | Homepage tiếp tục lấy layout đã lưu trong Admin. Block thiếu renderer không sinh wrapper public, tree giữ node/chẩn đoán và theme hỗ trợ trở lại render đúng ID/nội dung. |

Ghi từng ca theo browser, request/response hoặc snapshot DB phù hợp; chạy Builder smoke và browser integration. Báo lỗi với SHA, fixture và hành vi mong đợi trước khi chuyển M4-P08.

## M4-P08 · Nghiệm thu M4 và bàn giao M5

**Handoff dự kiến.** Harbor sở hữu báo cáo macOS và bản tổng hợp G5/M5, Forge sở hữu báo cáo Windows. Dùng một release SHA sau P07 và sau khi toàn bộ P01–P07/P09/P10 có bằng chứng tích hợp. Nếu sửa release SHA, chạy lại các ca trên SHA mới trước khi ký nghiệm thu. File báo cáo và runner riêng theo OS, không cùng sửa một file.

- Chạy locked build/tests, Builder/Theme/Media/Public Content/Entry Terms smokes, browser M4 và security gates theo G3 trên từng OS. Ghi hệ điều hành, browser, source/binary SHA, snapshot/fixture và lệnh với PASS/FAIL/NOT_RUN.
- Kiểm M3 → M4 trên bản sao site có homepage legacy, cây, Post/Page/CPT, taxonomy cha/con, ảnh private/public, draft và provider thiếu. Đổi theme A→B→C→A; homepage giữ layout đã Save, provider không hỗ trợ bị ẩn khi render và quay lại đúng dữ liệu. Kiểm ZIP legacy, sandbox/CSP, kiểm soát media, responsive, filter/page và session/concurrency.
- Khôi phục snapshot với binary tương ứng, xác nhận homepage, entry, term và media bằng HTTP/SQLite. Bàn giao cho M5 các contract thực chạy về AST, renderer, layout/theme, responsive và taxonomy/binding/list; chỉ ra giới hạn đã ghi trong ADR. Đóng M4 khi có đủ bằng chứng release và trạng thái GitHub được xác minh.

**Tình trạng kiểm chứng.** Các quyết định trên dựa vào PRD, code và đặc tả đã merge tại baseline. Các ca nghiệm thu sản phẩm là tiêu chí cho task triển khai. Hai điểm cần phản biện liên task là lựa chọn sandbox của P03 và phạm vi UI Field Binding của M4; P04/P06 có đề xuất schema cụ thể để reviewer chốt.
