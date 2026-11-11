# M4 G1 — Visual Builder và Taxonomy

**Atlas · 2026-11-11 · Baseline `origin/main@326ef65a0f8fa482c99d7d8f5883023bfbc907f0` · Nghiên cứu để phản biện G2.** Phạm vi chính gồm M4-P01/P02 và P09/P10. P03/P04/P05/P06/P07/P08 xuất hiện ở các điểm phụ thuộc. Nguồn phạm vi gồm [PRD](../../docs/product/PRODUCT_REQUIREMENTS.md), [M4 Task Plan](../../docs/plans/milestones/M4/TASK_PLAN.md), [Core Foundation](../../docs/contracts/CORE_FOUNDATION.md), [ADR-0004](../../docs/decisions/ADR-0004-m3-post-page-publication-boundary.md), [ADR-0006](../../docs/decisions/ADR-0006-m3-admin-and-release-contracts.md) và [kết luận nghiên cứu B/E tại Issue #15](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410). M5 tiếp tục sở hữu template composition, URL taxonomy công khai, nhập mẫu và Theme Options. Khai báo taxonomy từ plugin thuộc M6.

## Hiện trạng Builder

| Mã hiện tại | Hành vi cần giữ và khoảng trống |
| --- | --- |
| [`Block` và `BlockDescriptor`](../../cms/crates/opendash-core/src/theme.rs), [migration 0001](../../cms/migrations/0001_builder_slice.sql) | Một `home_page` chứa ID block và object giá trị chuỗi. Validate theo bộ field của block gốc. Chưa có cây, ID từng node, schema version hoặc ETag cho layout. |
| [`public_home`, `select_template`, `save_home_page_and_render`](../../cms/crates/opendash-server/src/storage/theme.rs), [migration 0011](../../cms/migrations/0011_theme_activation.sql) | Homepage và active theme là hai nguồn riêng. Public đọc cả hai trong một snapshot. Save và lựa chọn template quan sát cùng active theme trong transaction. Theme tương thích được chọn qua ID hoặc alias khai báo. Kích hoạt theme không ghi lại `home_page`. |
| [`render_block`, `render_saved`, `save_page`](../../cms/crates/opendash-server/src/page.rs), [`preview`, `save`](../../cms/crates/opendash-server/src/routes/builder.rs) | Preview lấy input chưa lưu, kiểm session và CSRF, trả HTML `no-store`. Save ghi xong rồi mới phát `PageAfterSave` với `block_type` hiện hành. Với theme thiếu hỗ trợ, public có vùng nội dung rỗng an toàn, còn Builder thấy `X-OpenDash-Block-Visibility: unsupported` và giữ dữ liệu gốc. |
| [`handlePreview`, `handleSave`](../../cms/admin/src/App.tsx), [BuilderWorkspace](../../cms/admin/src/builder/BuilderWorkspace.tsx) | Builder giữ field đang chỉnh trên client và hiển thị HTML trong iframe sandbox. Cây, đồng bộ selection với canvas, keyboard, responsive props và cách giữ draft khi 412 là phần M4 cần thiết kế. |

[Bằng chứng Themes macOS](../../docs/verification/2026-11-11-m3-s27-t01-themes-macos.md) đã kiểm A→B tương thích, A→C ẩn block và C→A phục hồi kể cả sau restart. Đây là bộ ca hồi quy trực tiếp cho Builder mới. Đối chiếu hành vi từ source `storage/theme.rs` vì ví dụ luồng Save trong Core Foundation mô tả cấu trúc trước lần tích hợp Theme Activation. [Nghiên cứu Flatsome của dự án](../../docs/research/FLATSOME_UX_BUILDER_RESEARCH.md) đã khảo sát tree/canvas/inspector và responsive preview để làm đầu vào UX.

## P01/P02 — Các phương án mở rộng homepage

| Phương án | Lợi ích | Chi phí và điểm phải kiểm |
| --- | --- | --- |
| Chuyển tất cả homepage M3 thành cây ngay khi upgrade | Runtime chỉ đọc một định dạng. | Mỗi site phải tin vào chuyển đổi trước lần mở Admin đầu tiên. Cần chứng minh ID, giá trị, theme alias và cách quay về snapshot trước migration trên dữ liệu cũ. |
| Đọc legacy và chiếu sang cây trong Builder. Lần Save layout đầu tiên chuyển sang tài liệu versioned | Có thể mở, preview và kiểm dữ liệu trước khi chuyển. Migration chỉ thêm khả năng lưu tài liệu. | Hai định dạng cần một điểm chọn nguồn public duy nhất. Cần chặn client legacy ghi đè layout đã chuyển và kiểm cả hai đường đọc. |
| Giữ builder cũ riêng và chỉ dùng cây cho homepage tạo mới | Rủi ro nâng cấp ban đầu thấp. | Người dùng phải dựng lại homepage để chuyển sang editor mới. Hai luồng chỉnh sửa tồn tại lâu và làm phức tạp handoff M5. |

**Khuyến nghị cho G2:** thử phương án chiếu legacy khi đọc. Bọc block M3 vào một node tham chiếu block gốc, dưới Section → Row → Column, với ID ổn định qua reload và giữ nguyên map field. Mở Editor hoặc Preview chỉ đọc. Lần Save có chủ đích ghi tài liệu versioned và chuyển nguồn public trong một transaction với kiểm version. Mỗi homepage chỉ có một nguồn đang phục vụ. Nếu giữ row legacy để hỗ trợ kiểm chứng, các handler legacy phải nhận biết homepage đã chuyển và từ chối ghi vào nguồn không còn hoạt động. Khôi phục binary cũ dùng snapshot nhất quán trước nâng cấp, theo quy trình đã dùng với migration 0011.

- **Mô hình dữ liệu đề xuất:** document schema version, ID riêng cho từng node, loại node, props có kiểu và thứ tự con. Core sở hữu Heading/Text/Image/Button. Node tham chiếu block theme giữ ID gói, dữ liệu gốc và resolve renderer theo active theme. Dùng định danh có namespace và registry hiện có. Chốt tại G2 giới hạn byte/depth/node, ma trận cha–con, version chưa biết, vòng lặp hoặc ID trùng, chính sách giữ payload khi provider thiếu, và ngữ cảnh escape cho text/HTML/URL/CSS. Dữ liệu node bị ẩn vẫn chỉnh được trong Admin theo schema còn khả dụng.
- **Preview và Save:** cùng validation và renderer. Preview nhận draft riêng, giữ CSP và không ghi DB hoặc chạy hook. Save dùng CAS tương tự entry, cập nhật public sau commit, giữ draft và thông báo khi 401/412. G2 cần chốt giao thức editor mới và cách giữ payload `PageAfterSave` tương thích với plugin M3 khi layout có nhiều node. Kiểm xung đột giữa hai phiên Save và giữa Save với đổi theme.
- **Hai ranh giới phải thử trước P03/P05:** iframe hiện sandbox nghiêm ngặt. Thử cách chọn node từ preview mà vẫn giữ ranh giới iframe, ví dụ click overlay do Admin quản lý hoặc cơ chế chọn có tập message cố định với kiểm tra nguồn. Public media hiện chỉ mở asset có tham chiếu `image`/`gallery` từ entry public. Image element của homepage cần G2 riêng về tham chiếu từ layout đã Save. Preview draft phải dùng quyền Admin và không làm asset xuất hiện trên URL public. [ACL media](../../cms/crates/opendash-server/src/storage/public_content.rs) · [ADR-0004](../../docs/decisions/ADR-0004-m3-post-page-publication-boundary.md).

## Hiện trạng Taxonomy

| Mã hiện tại | Điểm M4 cần nối |
| --- | --- |
| [Migration 0004](../../cms/migrations/0004_content_types.sql), [migration 0005](../../cms/migrations/0005_content_entries.sql), [migration 0008](../../cms/migrations/0008_post_page_publication.sql) | `content_entries.id` là khóa ổn định dùng chung cho post/page/CPT. Slug theo từng content type, entry có `version` và `publication_status`. `values_json` là custom field, chưa có taxonomy hoặc liên kết term. |
| [`update_entry`, `delete_entry`, `set_publication_status`](../../cms/crates/opendash-server/src/storage/entries.rs), [entry routes](../../cms/crates/opendash-server/src/routes/entries.rs) | Admin/Editor được sửa entry với session, CSRF và ETag theo route hiện hành. Cần chốt ranh giới quyền quản lý term và quyền gắn term. Liên kết term và entry Save phải có cơ chế xử lý xung đột nhất quán. |
| [`list_published`, `media_is_referenced`](../../cms/crates/opendash-server/src/storage/public_content.rs), [public routes](../../cms/crates/opendash-server/src/routes/public_content.rs) | Lọc `published` trong SQL trước phân trang. Filter taxonomy phải dùng cùng điều kiện public và giữ các URL M3. Preview có auth là luồng riêng. |
| [`init_db`](../../cms/crates/opendash-server/src/storage/mod.rs) và [registry](../../cms/crates/opendash-core/src/registry.rs) | Pool đang bật FK. Domain có `ResourceOwner` và `ProviderStatus`; khi mở provider M5/M6, việc thay trạng thái provider cần giữ dữ liệu site và entry links. |

## P09/P10 — Mô hình và điểm cần chốt

| Phương án | Tác động |
| --- | --- |
| Lưu term ID trong `values_json` mỗi entry | Thêm ít bảng, nhưng xóa/reparent và lọc liên type phải quét JSON. SQLite FK không thể kiểm từng ID trong mảng JSON theo mô hình hiện tại. |
| Term site-owned và bảng nối `entry_id`–`term_id` | FK bảo vệ liên kết, lọc/index rõ và giữ một nguồn dữ liệu cho Post/Page/CPT. Cần transaction cho cập nhật membership và có policy khi term còn được dùng. |
| Mô hình taxonomy đăng ký tổng quát ngay, có term, định nghĩa taxonomy và object relations | Mở rộng mạnh cho theme/plugin sau này. Scope M4 phải gánh thêm lifecycle, registry và đối tượng ngoài entry. WordPress dùng các bảng term, term taxonomy và term relationships riêng làm đối chứng cho mô hình rộng hơn. [WordPress schema](https://developer.wordpress.org/reference/functions/wp_get_db_schema/) |

**Khuyến nghị cho G2:** term site-owned với ID ổn định, phân biệt category/tag và dùng bảng nối duy nhất trỏ `content_entries.id`. Bắt đầu với category có parent trực tiếp và tag phẳng. Kiểm cycle và giới hạn độ sâu trong transaction khi reparent, kể cả hai thao tác đồng thời. SQLite hỗ trợ FK và truy vấn cây bằng recursive CTE. FK kiểm sự tồn tại của parent, còn giới hạn độ sâu và quy tắc cùng taxonomy cần validation/domain constraints phù hợp. [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html) · [SQLite recursive CTE](https://www.sqlite.org/lang_with.html).

- **Identity và eligibility:** dùng ID ổn định để gắn entry và lưu cấu hình list block, dùng slug để tìm kiếm/hiển thị. Đề xuất slug duy nhất theo kind trong site ở M4, kiểm riêng kind/parent và chỉ cho liên kết ở các content type đã được phép. G2 phải quyết định Post/Page/CPT nào được bật mặc định, giới hạn số category/tag trên entry và cách khai báo eligibility mà không tạo provider bridge sớm.
- **Quyền và xóa:** đề xuất Admin quản lý term và reparent, Admin/Editor gắn term đã tồn tại theo quyền sửa entry. Bảo vệ mutation bằng CSRF/Origin và quyền server. Đề xuất từ chối xóa term còn child hoặc còn entry reference, hiện lý do và yêu cầu người dùng chủ động chuyển child/gỡ liên kết. Entry delete chỉ xóa các liên kết của entry, không xóa term. Đổi tên/slug không làm mất quan hệ. G2 chốt HTTP error, version term và UX xử lý conflict.
- **Ghi đồng thời và lọc:** dùng version entry chung cho chỉnh nội dung và membership, để stale Save trả 412. Khi form Save thay cả hai, ghi trong một transaction và phân biệt client cũ không gửi taxonomy với yêu cầu gỡ hết. Filter trên storage trước phân trang, ưu tiên `EXISTS` hoặc query tránh nhân đôi entry, sắp xếp ổn định. Public/list block chỉ đọc entry public theo ADR-0004. G2 chốt category filter khớp chính xác hay gồm con, any/all khi chọn nhiều term, và contract dùng chung cho P06. M4 giữ nguyên namespace URL công khai hiện có.

## Tiêu chí nghiệm thu cần đưa sang G2

| Phạm vi | Ca tối thiểu và bằng chứng |
| --- | --- |
| P01/P02 · Dữ liệu cũ | Mở bản sao M3 có homepage đã chỉnh và một site chưa có homepage. So snapshot khi Open/Preview, Save cây rồi restart/reopen. Kiểm node IDs, thứ tự, toàn bộ legacy fields, behavior route cũ sau chuyển đổi, rollback bằng snapshot và binary M3 đúng phiên bản. |
| P01/P02 · Theme | A→B dùng markup B với dữ liệu cũ. A→C ẩn đúng legacy node và giữ node core tương thích. C→A phục hồi node gốc. Kiểm draft đang mở qua đổi theme và race đổi theme/Save. |
| P01/P02 · Input và render | Cây sai cha–con, ID trùng, version chưa biết, vượt giới hạn, provider thiếu, text/URL không hợp lệ, 401/403/412. Preview không ghi DB, không đổi public và không phát hook. Save hợp lệ mới đổi public và hook chạy sau commit. |
| P09 · Category/tag | Duplicate slug, parent sai kind, cycle trực tiếp/gián tiếp, depth, hai reparent cùng lúc, xóa term còn child/reference, quyền Admin/Editor và CSRF. Kiểm restart và FK trên SQLite thật. |
| P10 · Links/query | Gắn và gỡ term trên các content type được phép, client cũ Save vẫn giữ links, stale edit 412, delete entry giữ term và dọn links, đổi slug/reparent giữ membership. Draft khớp filter vẫn vắng khỏi public count/pagination. Có fixture Post/Page/CPT và media ref M3. |

**Thứ tự đề xuất:** P01/P02 và P09/P10 có thể đi G2 song song. Chốt tài liệu/renderer rồi mới nối editor P03 và responsive P04. P06 cần interface filter của P10 và binding của P05. Khi chuẩn bị G3, bàn giao lần lượt `app.rs`, `routes/mod.rs`, `storage/mod.rs`, `App.tsx` và migration numbering để tránh hai writer. Cập nhật baseline bằng SHA nghiệm thu M3 và giữ các regression M3 trong acceptance M4. Hai báo cáo phát hành macOS và Windows vẫn thuộc [Issue #53](https://github.com/thientran2359/opendash-cms/issues/53) và [Issue #54](https://github.com/thientran2359/opendash-cms/issues/54).

**Phạm vi bằng chứng lượt này:** kiểm source và tài liệu tại SHA ghi trên, truy vấn CodeGraph không có kết quả do workspace index rỗng nên đối chiếu trực tiếp các file và caller. Các mô hình nêu trên là đề xuất G2. Runtime, migration prototype và browser M4 chưa chạy.
