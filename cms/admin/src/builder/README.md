# Visual Builder Workspace (`cms/admin/src/builder/`)

Không gian dựng bố cục 4 tầng (`Section → Row → Column → Element`) tự dựng bằng React + TypeScript (không dùng `dnd-kit` hay `Puck`), hỗ trợ Homepage, Template Overrides (`header`, `footer`, `archive`, `single`, `taxonomy_archive`), Page-scoped Builder và private Section CPT.

- `BuilderWorkspace.tsx`: Điều phối tải/lưu AST (CAS `If-Match`), chọn viewport (`desktop`/`tablet`/`mobile`), đồng bộ iframe preview và bảo toàn thay đổi chưa lưu.
- `BuilderTree.tsx`: Cây phân cấp `Section → Row → Column → Element` hỗ trợ thêm, xóa, sắp xếp và chọn node.
- `BuilderCanvas.tsx`: Khung `iframe sandbox=""` hiển thị HTML preview từ server và tương tác chọn phần tử.
- `BuilderInspector.tsx`: Bảng thuộc tính của node đang chọn theo viewport.
- `BuilderBindingFields.tsx`: Cấu hình `FieldBinding` (`FixedEntry` và `RouteContext`).
- `BuilderListFields.tsx`: Cấu hình block `core/list` (CPT, taxonomy, sắp xếp, phân trang).
- `BuilderContactFormFields.tsx`: Cấu hình block `core/contact_form`.
- `document.ts`: Tiện ích thao tác cây `BuilderDocument` phía client.
