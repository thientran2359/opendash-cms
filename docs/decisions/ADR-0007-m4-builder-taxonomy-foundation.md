# ADR-0007 — Nền tảng Visual Builder và Taxonomy M4

**Quyết định G2:** Chủ dự án chọn hướng đề xuất G1 ngày 2026-11-11. Áp dụng khi [PR #66](https://github.com/thientran2359/opendash-cms/pull/66) merge vào `main`. **Baseline:** `326ef65a0f8fa482c99d7d8f5883023bfbc907f0`. [Nghiên cứu G1](../../.agents/research/2026-11-11-m4-builder-taxonomy-g1.md) · [Kế hoạch M4](../plans/milestones/M4/TASK_PLAN.md) · [PRD](../product/PRODUCT_REQUIREMENTS.md).

Phạm vi quyết định gồm P01/P02 và P09/P10. P03–P08 tiêu thụ các hợp đồng ở đây. Chi tiết tương tác canvas, responsive và binding thuộc các lát cắt tương ứng. Template composition và URL taxonomy công khai thuộc M5. Theme/plugin registration tiếp tục theo M5/M6.

## P01/P02 · Homepage và tài liệu Builder

### Đọc và chuyển đổi homepage

- Dùng một bản ghi Builder singleton do site sở hữu với version và document JSON nullable. Migration thêm bản ghi version `1` và document `NULL`. Khi document là `NULL`, public `/` tiếp tục dùng `home_page` của M3. Admin mở layout dưới dạng cây được chiếu từ block cũ, hoặc cây rỗng nếu site chưa có homepage. Chiếu cây tạo ID ổn định từ vị trí và định danh gốc. Node legacy lưu nguyên ID block và map field. Open và Preview chỉ đọc.
- Lần Lưu cây có chủ đích ghi document và tăng version trong một transaction. Sau commit, `/` đọc cây từ snapshot nhất quán với active theme. `home_page` cũ còn để kiểm tra dữ liệu và khả năng khôi phục. Khi cây đã được lưu, `POST /save` legacy trả 409 kèm hướng dẫn mở Builder mới. Trước khi chuyển đổi, Save legacy tiếp tục hoạt động và cũng tăng version singleton trong cùng transaction. Vì vậy draft cây đọc trước một lần Save legacy sẽ nhận 412 khi ghi. Đồng thời giữ shape `{type,values}` của `/page` cho homepage legacy. Khi nguồn đang phục vụ là cây, `/page` trả 409 kèm hướng dẫn mở Builder mới.
- Phương án chiếu khi đọc cho phép mở và kiểm tra homepage trước khi thay đổi định dạng. Migration ghi lại toàn bộ homepage ngay khi startup sẽ đặt việc kiểm dữ liệu sau điểm chuyển đổi. Rollback về binary M3 dùng snapshot site trước migration và binary tương ứng. Nghiệm thu restore trên bản sao SQLite và assets.

### Schema và ranh giới render

- Document v1 gồm `schema_version: 1` và `sections`. `section.children` chỉ chứa row, `row.children` chỉ chứa column, `column.children` chỉ chứa element. Mỗi node có ID ổn định trong document, tách khỏi khóa loại element và giữ qua reorder. Thứ tự mảng là thứ tự render. Giới hạn v1 là 512 node và 256 KiB JSON cho document, với kiểm giới hạn body trước decode và kiểm cấu trúc ở core. Từ chối schema version không hỗ trợ, node ID trùng hoặc sai quan hệ cha–con. Node dùng khóa loại namespaced và đi qua registry do core quản lý.
- Core sở hữu `core/heading`, `core/text`, `core/image` và `core/button`. Heading có cấp 1–6 và text. Text dùng text thuần. Image dùng ID Media Library và alt text. Button dùng label và URL được kiểm theo ngữ cảnh, cho phép path nội bộ hoặc `http`/`https`. Validation và render dùng kiểu field xác định. Giá trị text, thuộc tính và URL được escape theo ngữ cảnh. HTML/CSS/JS tùy ý nằm ngoài schema v1. Responsive thêm props có kiểu ở P04.
- Legacy node đi qua resolver của M3 với dữ liệu và field schema gốc. Khi theme active hỗ trợ, render bằng template active. Khi theme thiếu hỗ trợ, chỉ bỏ qua node đó trong public HTML. Các core node còn lại vẫn render. Admin nhận chẩn đoán theo ID node và tiếp tục thấy giá trị đã lưu. Provider hoặc descriptor thiếu thì giữ payload và vị trí node, báo trạng thái ở Admin và bỏ qua node ở public. Validation cho phép round-trip node chưa resolve đã có trong layout, giữ nguyên payload của node đó. Thêm hoặc sửa nội dung của node cần descriptor hợp lệ.
- Image trong bản Preview dùng media đã xác thực. Public media chỉ có thêm quyền từ tham chiếu Image của layout đã Save và có thể render, cùng với quyền từ các entry public hiện có. Lưu layout và tập tham chiếu Image trong cùng transaction. Xóa tham chiếu cuối cùng khiến request public mới về 404. Bảo toàn media ACL khi preview, Save xung đột hoặc đổi theme. Renderer chỉ mở nguồn ảnh nội bộ cần thiết trong CSP. P03 kiểm chọn node từ canvas mà vẫn giữ iframe sandbox và không cho nội dung template chạy script.

### API và Save

| Endpoint | Hợp đồng |
| --- | --- |
| `GET /api/admin/builder/homepage` | Admin/Editor theo quyền Builder hiện hành. Trả `{document,source,version}` và ETag `"vN"`. `source` là `empty`, `legacy` hoặc `layout`. |
| `POST /api/admin/builder/homepage/preview` | Nhận document draft. Session và CSRF/Origin. Trả `{html,diagnostics}` với `Cache-Control: no-store`. Preview giữ DB và public snapshot hiện hành. |
| `PUT /api/admin/builder/homepage` | Nhận document, session và CSRF/Origin, `If-Match: "vN"`. Trả `{document,source,version,html,diagnostics}` và ETag mới sau commit. Thiếu precondition 428, sai dạng 400, stale 412. UI giữ draft để người dùng xử lý xung đột. |

Preview và Save dùng chung validation/render ở server. Lựa chọn theme và ghi Save dùng snapshot giao dịch như M3. Save legacy tiếp tục phát `page/after_save` với `block_type` gốc sau commit. Save cây cũng phát đúng một sự kiện cùng tên sau commit. Với cây còn đúng một element legacy, `block_type` giữ ID gốc. Các cây khác dùng `core/layout` và có thêm `layout_version` số nguyên. Giữ cấu trúc envelope của HookDispatcher và không đưa props hoặc dữ liệu draft vào event. Nghiệm thu plugin listener hiện có với cả hai payload. Lỗi hook sau commit được ghi nhận theo cơ chế hiện hành.

## P09/P10 · Category, tag và liên kết entry

- Dữ liệu thuộc site. Dùng bảng `taxonomy_terms` với ID số nguyên dương ổn định, `kind`, `name`, `slug`, `parent_id` nullable và `version`. Slug có độ dài 1–64, dùng kebab validator của core, duy nhất theo `(kind, slug)`. `name` trim, 1–120 ký tự Unicode. Category được có parent là category. Tag phẳng. Giới hạn category tối đa 8 cấp, root tính là cấp 1. Reparent kiểm parent, chu trình và độ sâu của cả nhánh dưới cùng write reservation. Đổi tên/slug và chuyển parent giữ ID và membership.
- Bảng `entry_terms` có khóa chính `(entry_id,term_id)`, FK tới `content_entries` với `ON DELETE CASCADE`, FK term với `ON DELETE RESTRICT` và index đảo `(term_id,entry_id)` để lọc. Chọn Post, Page và CPT hiện hành dùng chung category/tag mặc định, mỗi entry tối đa 32 category và 64 tag. Khi provider ngừng hoạt động, quan hệ vẫn được giữ và quyền chỉnh entry tiếp tục theo policy hiện có. Mô hình bảng nối dùng khóa ngoại hiện hành, có thể truy vấn và kiểm tính toàn vẹn trực tiếp. [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html).
- Admin quản lý term. Admin/Editor đọc term và gắn term sẵn có cho entry được phép sửa. Delete term còn child hoặc entry link trả 409 và nêu loại phụ thuộc. Người dùng chủ động chuyển child hoặc gỡ link trước khi xóa. P06 dùng ID ổn định trong cấu hình list. G2 của P06 cần quy định hành vi khi một term được cấu hình đã biến mất, với kết quả công khai rỗng và chẩn đoán Admin.

| Endpoint | Hợp đồng |
| --- | --- |
| `GET /api/admin/taxonomy/terms` | Admin/Editor. Lọc `kind`, phân trang mặc định 50, tối đa 100, sắp `name,id`. Trả `{items,total}`. Term DTO gồm `{id,kind,name,slug,parent_id,version}`. |
| `POST /api/admin/taxonomy/terms` | Admin + CSRF/Origin. Tạo `{kind,name,slug,parent_id?}`, trả 201 và ETag. Parent thiếu hoặc sai kind trả lỗi validation. Duplicate slug trả 409. |
| `PATCH/DELETE /api/admin/taxonomy/terms/{id}` | Admin + CSRF/Origin và `If-Match`. PATCH nhận `name`, `slug`, `parent_id` theo trường cần đổi. Kind bất biến. Trả term và ETag mới, hoặc 204 khi xóa. |
| Entry create/update/list hiện hành | Đọc thêm `term_ids`. Create mặc định không có term. Update không gửi `term_ids` thì giữ quan hệ. `term_ids: []` gỡ hết. Thay field và membership cùng transaction và version entry, lỗi validation giữ dữ liệu trước request. List nhận bộ lọc bên dưới. |

Entry `term_ids` là tập ID duy nhất được xác thực theo kind và giới hạn số lượng. CAS vẫn áp dụng cho toàn entry, bao gồm Save, publish/unpublish và xóa. Guard giữ 401/403 hiện hành. Term không tồn tại trả 404 cho route term. Reparent gây cycle hoặc vượt độ sâu trả 409. Missing/stale `If-Match` dùng 428/412. Mọi SQL thuộc storage.

### Bộ lọc dùng chung

Bộ lọc M4 nhận tối đa một `category_id` và một `tag_id`. Nếu có cả hai thì entry phải khớp cả hai. Category mặc định khớp đúng mục đã chọn. `include_descendants=true` mở rộng sang mục con khi có `category_id`. Dùng cây đã kiểm độ sâu và truy vấn không nhân đôi entry, lọc trước phân trang và giữ thứ tự ổn định có ID làm khóa phụ. Public/list block luôn áp dụng `publication_status='published'` trước phân trang và giữ URL M3. Category con được xác định từ cây hiện tại, nên reparent ảnh hưởng kết quả lọc mà không đổi membership đã lưu. [SQLite recursive CTE](https://www.sqlite.org/lang_with.html) · [ADR-0004](ADR-0004-m3-post-page-publication-boundary.md).

## Nghiệm thu và chuẩn bị G3

| Lát cắt | Điều kiện phải chứng minh |
| --- | --- |
| P01/P02 | Mở homepage rỗng và bản M3 đã chỉnh. Kiểm Open/Preview không đổi DB, Save chuyển định dạng đúng một lần, restart giữ cây/ID/order. Hai editor stale trả 412. Client legacy sau chuyển đổi nhận 409. A→B đổi markup, A→C chỉ ẩn node legacy, C→A phục hồi. Kiểm Image preview/public 404 trước Save và sau gỡ tham chiếu cuối. Kiểm CSP, giới hạn JSON, provider thiếu, hook payload và rollback bằng snapshot cũ. |
| P09/P10 | Fresh/upgrade trên SQLite copy, Post/Page/CPT gắn được term đúng quyền. Kiểm slug trùng, parent sai kind, cycle/depth kể cả reparent đồng thời, xóa bị chặn khi còn phụ thuộc. Legacy update giữ links, `[]` gỡ hết, stale 412 và xóa entry giữ term. Exact/descendants và category+tag cho kết quả đúng trước phân trang. Draft khớp filter vẫn vắng khỏi public và số đếm. |

G3 tách Builder foundation và taxonomy foundation theo quyền sở hữu file. Router, storage module declarations, AdminShell và migration numbering có một writer mỗi lượt tích hợp. P06 chờ binding P05 và interface filter P10. Dùng baseline M3 đã nghiệm thu theo dependency khi phát hành G3, giữ điều kiện release của [#53](https://github.com/thientran2359/opendash-cms/issues/53) và [#54](https://github.com/thientran2359/opendash-cms/issues/54). Các kiểm thử trong bảng là tiêu chí G4/G5, runtime M4 chưa chạy.
