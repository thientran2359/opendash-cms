# M4 — Task planning: Visual Builder

**Tất cả `M4-Pxx` dưới đây là task candidates để lập G1/G2/G3**, không phải `READY`. Không chốt AST schema/API/ABI chỉ từ bảng này; code Builder thay contract cần Full-Track và reviewer độc lập. [Milestone](README.md) · [V1 scope](../../../product/PRODUCT_REQUIREMENTS.md) · [M3 prerequisites](../M3/README.md).

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance (phải thành test khi G3) |
| --- | --- | --- | --- | --- |
| M4-P01 | Thiết kế AST node Section/Row/Column/Element và descriptors Heading/Text/Image/Button | BLD-02 (BLD-01, FND-01) | M2.5 registry + M1 block compatibility; Full-Track G1/G2 AST/migration | Serialize/validate tree; invalid node/cycle/size, unknown provider fail safely; legacy one-block vẫn mở được |
| M4-P02 | Renderer và lưu/mở lại AST, fallback provider | BLD-01/02 | P01, page storage và renderer/CSP contract | Trang cũ render được; tree lưu/restart/rehydrate giữ layout, missing block có chẩn đoán Admin và public fallback an toàn |
| M4-P03 | UI tree/canvas/inspector chọn node từ cây hoặc preview | BLD-03 (FND-05) | P01/P02; keyboard/form state, RBAC | Thêm/xóa/đổi thứ tự element và chỉnh thuộc tính; tree ↔ canvas đồng bộ, thao tác bàn phím và 401 không làm mất bản nháp |
| M4-P04 | Bố cục responsive desktop/tablet/mobile | BLD-04 (BLD-02/03) | P03, breakpoint model G2 | Preview 3 chế độ và CSS output đúng thuộc tính; lưu/đọc lại không drift |
| M4-P05 | Bind core element với field/CPT, escaping theo ngữ cảnh | BLD-05 (CNT-02/04) | M3 field/entry interface, P01/P02; Full-Track query/render security | Binding chính xác cho entry, thiếu field an toàn, rich text không XSS, không gọi query không được phép |
| M4-P06 | Query/list block: CPT/category/sort/limit/pagination | BLD-05 (CNT-03/04) | Taxonomy M3 contract + P05, URL policy SITE-02/THM-02 cho public pagination | Chọn CPT/category, render list ổn định, pagination/sort có giới hạn, không lộ content sai permission |
| M4-P07 | Preview chưa lưu, Save mới public, conflict/refresh | BLD-01/03 (FND-02/05) | P02..P06, M1 preview semantics | Preview bản nháp khác public; 401/412/CSRF, reload/restart không mất layout đã Save; không tạo publishing workflow riêng |
| M4-P08 | Regression hiện thực Builder + handoff theme M5 | BLD-01–05 | P01..P07, M1 legacy theme ZIP, M3 data fixtures | Browser end-to-end: edit/preview/Save/reopen trên page thực; responsive, legacy, XSS, missing provider và test exit G5 theo SHA/OS |

**Giới hạn:** Theme composition page/header/footer/archive/single và mẫu import là M5; SEO hoàn chỉnh M7; plugin UI/manifest toàn diện M6. [M4→M5 gate](../../ROADMAP_V1.md#gates-quan-trọng-xuyên-các-milestone) yêu cầu layout và field binding thực sự được lưu/preview/test.
