# ADR-0011 — Editor, responsive và danh sách homepage M4

**Final Decision: Atlas · AGREED khi merge · Baseline `7b7e2201f475950d5a17bde7b0d417eb6a19351a`.** Theo [ADR-0010](ADR-0010-agent-project-and-merge-authority.md). Nguồn gồm [PRD](../product/PRODUCT_REQUIREMENTS.md), [ADR-0007](ADR-0007-m4-builder-taxonomy-foundation.md), [ADR-0009](ADR-0009-m4-core-element-field-binding.md), [thử nghiệm Atlas](../../.agents/research/2026-11-11-m4-p03-builder-editor-g2.md) và [đề xuất Forge](https://github.com/thientran2359/opendash-cms/pull/96). Phạm vi homepage Builder M4. [Bộ handoff G3](../plans/milestones/M4/slices/2026-11-11-m4-builder-remaining-g3.md).

## M4-P03 · Editor và Preview

- Một document nháp, một node đang chọn, một revision nội bộ và ETag của bản lưu. Tree và canvas cấu trúc do React Admin dựng từ AST có kiểu, cả hai chọn theo ID. Canvas hiển thị core props bằng DOM do Admin kiểm soát; dùng placeholder có tên cho legacy/binding/list và node không resolve. Tab Xem trước thực tế dùng HTML server trong iframe `sandbox=""`; giữ CSP, không cấp same-origin, scripts hay cầu sự kiện. Canvas cấu trúc thực hiện thao tác chọn trực tiếp mà vẫn cô lập HTML từ theme. Probe Chrome xác nhận overlay/keyboard và giới hạn DOM trên iframe opaque; nghiệm thu Builder thật thuộc G4/G5.
- Dùng GET và PUT `/api/admin/builder/homepage`, POST `/api/admin/builder/homepage/preview`. Preview chỉ nhận response cùng user và revision; chưa lưu không ghi DB. PUT gửi snapshot và `If-Match`; Save thành công cập nhật ETag và giữ các edit đến sau snapshot; 412 giữ draft và cung cấp lựa chọn tải bản server với xác nhận bỏ bản nháp. Khóa Save song song trong một tab. Draft ở memory qua chuyển màn và 401 khi đăng nhập lại cùng ID, xóa khi logout chủ đích hoặc user khác. Cảnh báo trước khi đóng/reload có thay đổi.
- Chỉ tạo/move đúng tầng, giữ ID ổn định; tree cho add/delete/reorder bằng chuột và bàn phím, focus theo node. Core validation và `is_preserved` là authority cho node mất provider; chặn thay đổi vị trí/ancestor/nội dung của node chưa resolve và cho xóa có xác nhận. Sau khi đổi theme, dữ liệu vẫn ở Builder; renderer bỏ node thiếu support, render lại khi theme có support. Không nâng quyền iframe, thêm workflow xuất bản hay migration.

## M4-P04 · Responsive

- Document v1 thêm `responsive?` trên Section/Row/Column với object thưa `desktop/tablet/mobile`; desktop từ 1024 CSS px, tablet từ 768 tới dưới 1024, mobile dưới 768. CSS áp desktop → `width < 1024px` → `width < 768px`, thuộc tính thiếu kế thừa từng giá trị, xóa override khôi phục kế thừa, 0 là giá trị hợp lệ.
- Cho `padding_px` 0–64 trên container, `gap_px` 0–64 và `columns_per_row` 1–6 trên Row, `text_align` start/center/end trên Column. Số nguyên, từ chối khóa sai/ngữ cảnh sai. Row opt-in grid nếu có gap hoặc columns ở bất kỳ viewport nào, mặc định một cột, thứ tự AST. Chỉ sinh CSS từ allowlist, scope dưới Builder root và node ID hợp lệ, bỏ object rỗng khi serialize. Giữ 512 node/256 KiB và document v1 thiếu metadata giữ render cũ. Preview dùng iframe có viewport thật theo mode; responsive UI state không lưu trong document.

## M4-P06 · List block

- Thêm `core/list` với `type_key`, `category_id?`, `tag_id?`, `include_descendants=false`, `sort=id_desc|id_asc` (mặc định desc) và `page_size` 1–20 (mặc định 6). Category/tag đúng loại, AND trước phân trang; descendants cần category. Giới hạn 8 list/document, 100 trang/list, giữ giới hạn document. Lọc published ở storage cho public và authenticated Preview. P05 exact-entry binding giữ semantics riêng. SQL dùng enum sort và bound params; giữ thứ tự desc cho caller hiện tại. Trả thêm một row xác định trang kế.
- Chỉ render title đã escape và URL canonical `/content/{type}/{slug}` theo policy hiện có. CPT/term mất hoặc sai loại cho list rỗng và chẩn đoán node trong Admin; lỗi storage theo `ApiError`. Không thêm media hoặc ACL mới.
- Public đọc một `list_page_<node-id>` hợp lệ (1–100) cho từng list. Thiếu/trùng/sai về trang 1, bỏ key node không tồn tại. Link pagination nội bộ giữ trang hợp lệ của list khác; trang ngoài dữ liệu có link về đầu; trang 100 dừng. Preview/Save HTML luôn ở trang đầu. Không thêm migration, route mới, custom item template hoặc thay URL policy M5.

## M4-P05, P07 và P08

P05 backend đã nghiệm thu. Inspector dùng API nội dung và media hiện hành trong quyền Admin/Editor; chỉnh binding có kiểu theo ADR-0009, không tự thêm API hoặc quyền. P07 kiểm toàn luồng Builder và bảo toàn khi đổi theme. P08 chạy macOS và Windows trên cùng release SHA và site fixture phù hợp, khôi phục snapshot bằng binary tương ứng rồi bàn giao M5. Toàn bộ phần này giữ hành vi **Lưu homepage cập nhật public**, độc lập với trạng thái publication của Post/Page/CPT.
