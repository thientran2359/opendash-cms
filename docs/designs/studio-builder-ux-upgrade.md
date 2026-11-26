# OpenDash Studio UI/UX Upgrade — Sidebar-First Contextual Builder & Interactive Canvas Actions

- **Author:** Orion (Elephant, tiếp quản từ Atlas)
- **Status:** Verified
- **Date:** 2026-11-23
- **Baseline:** `main@3fb9ebe`
- **Approved By:** Owner
- **Approval Evidence:** Owner chat 2026-11-23 ("d1 đồng ý, d2 làm luôn cây kéo thả, d3 làm cả 2, d4 đồng ý" — final mockup + technical decisions D1–D4 approved)
- **Goldfish Review:** Round 1 independent review (`egm-design` `e344d954`, `egm-codebase-details` `93f31c5f`, `egm-implementation-readiness` `0c057d81`) · 2026-11-23 · reviewed `main@3fb9ebe` (all Blockers, Majors, Minors & Nits resolved in v2 + Owner D1–D4 finalized)
- **Related:** [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md), [`docs/contracts/CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`docs/contracts/PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md), [`docs/designs/opendash-studio-customizer-and-ux-builder.md`](opendash-studio-customizer-and-ux-builder.md), [`docs/designs/declarative-block-json-engine.md`](declarative-block-json-engine.md), [`docs/designs/tailwind-frontend-and-starter-theme-v3.md`](tailwind-frontend-and-starter-theme-v3.md)

---

## Overview

OpenDash Studio đã hoàn thiện nền tảng kỹ thuật hai chế độ (Mode 1 `#/studio` Customizer + Template Builder; Mode 2 `#/studio/entry/:typeKey/:id` Entry Visual Builder), lưu trữ Comment Block `<!-- od:... -->`, Declarative `block.json` Block Engine với `<BlockSchemaForm>`, và Tailwind v4 + Default Theme `opendash-starter`. Tuy nhiên, bố cục UI và luồng thao tác dựng trang hiện tại (`StudioTopBar` + `BuilderEditorCore`) vẫn còn nhiều ma sát so với định hướng **Flatsome UX Builder**:

1. **Thanh công cụ ngang phía trên canvas (`StudioTopBar`) chiếm diện tích dọc** và tách rời nút Viewport / Undo / Redo / Lưu khỏi vùng thao tác chính ở sidebar trái.
2. **Sidebar bị chồng chất 3 khối dọc:** Trong `BuilderWorkspace.tsx` (retired; extracted to [`BuilderEditorCore.tsx`](../../cms/admin/src/builder/BuilderEditorCore.tsx)), `BuilderEditorCore` đặt thanh 3 tab `Blocks / Patterns / Media`, sau đó chồng tiếp dropdown `<select id="studio-element-type">` + `BuilderTree` + `BuilderInspector` trên cùng một cột cuộn, khiến phần chỉnh thuộc tính bị đẩy xuống dưới màn hình (`below the fold`).
3. **Thao tác dựng trang kém trực quan:** Muốn thêm block phải chọn trước trong dropdown `Element mới` rồi mới chọn node cha và bấm `+`; chưa có thao tác **Nhân bản (`Duplicate`)** hay **Copy/Paste options** trong cây; nút **Xóa** bị đặt lặp ở cả Tree lẫn đáy Inspector; còn trên iframe canvas (`BuilderPreviewFrame.tsx`) mới có hover/click chọn node chứ chưa có icon chỉnh block hay nhấp đúp mở menu thao tác.

Đặc tả này chuyển hóa 9 quyết định UI cuối cùng của Chủ dự án (đã chốt trong mockup `approved-mockup.fragment.html` và 2 ảnh tham chiếu ngày `2026-11-23`) thành kiến trúc kỹ thuật chi tiết, **giữ nguyên 100% AST v1, Comment Block, API backend, bảo mật CSP và toàn bộ hợp đồng dữ liệu hiện hành**.

---

## The Problem

1. **Không gian làm việc phân mảnh & trùng lặp header ([`StudioWorkspace.tsx#L925-L1049`](../../cms/admin/src/studio/StudioWorkspace.tsx#L925-L1049), `StudioTopBar.tsx` (đã thay thế trong S01), [`StudioCustomizerNav.tsx#L152-L163`](../../cms/admin/src/studio/StudioCustomizerNav.tsx#L152-L163)):**
   - `StudioTopBar` cố định 48px phía trên toàn màn hình, trong khi `StudioCustomizerNav` lại vẽ thêm `renderDrawerHeader` ở tất cả 6 màn con Mode 1, tạo hai tầng tiêu đề và nút quay lại chồng chéo.
   - `StudioWorkspace.tsx#L1106` hardcode `frameClassName="!h-[calc(100vh-120px)]"` cho thanh topbar cũ và `<aside>` dùng `h-72` trên mobile không đủ chỗ cho Header + Workspace + Footer.
2. **Thao tác cây & màn thuộc tính bị trộn lẫn (`BuilderWorkspace.tsx` — retired; extracted to [`BuilderEditorCore.tsx`](../../cms/admin/src/builder/BuilderEditorCore.tsx), `BuilderTree.tsx` — retired; replaced by [`StudioBlockTree.tsx`](../../cms/admin/src/studio/StudioBlockTree.tsx), [`BuilderInspector.tsx`](../../cms/admin/src/builder/BuilderInspector.tsx)):**
   - Không có trạng thái thu gọn/mở rộng (`collapse/expand`) cho từng nhánh `Section` / `Row` / `Column` trong cây.
   - Không có nút thêm trực tiếp theo ngữ cảnh container (`+ Thêm vào Section`, `+ Thêm vào Row`, `+ Thêm vào Column`) ở cuối danh sách con, cũng không có catalog trực quan lọc theo loại con hợp lệ.
   - `BuilderInspector` hiện luôn render phía dưới cây, hiển thị mã kỹ thuật `item.node.id` nổi bật và chứa nút `Xóa node này` ở cuối form (trái với quyết định màn Thuộc tính chỉ chứa các field của block đang sửa).
3. **Thiếu thao tác nhanh theo ngữ cảnh trên cả Tree và Preview Canvas ([`BuilderPreviewFrame.tsx#L10-L80`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx#L10-L80)):**
   - `BRIDGE_SCRIPT` hiện chỉ phát `od-studio-hover`, `od-studio-pick`, `od-studio-scroll` và nhận `od-highlight`. Chưa có nút icon bánh răng/cài đặt (`.od-canvas-gear`) trên node `section`/`element` và chưa có sự kiện nhấp đúp (`dblclick`) để mở menu thao tác (`Chỉnh sửa / Nhân bản / Copy options / Paste options / Xóa`).
4. **Ràng buộc bảo toàn dữ liệu cần giữ tuyệt đối:**
   - Trên Rust server ([`opendash-core/src/builder.rs#L522-L527,#L582-L599`](../../cms/crates/opendash-core/src/builder/mod.rs)), các block bị khóa (`ProviderInactive` / `Withdrawn` / `Unresolved`) chỉ được giữ nguyên qua `is_preserved` theo đúng `(path, ancestor_ids, element.id)` đã lưu trước đó. Mọi thao tác **Nhân bản**, **Copy/Paste options** và **Di chuyển** phải tuyệt đối tôn trọng `isLockedElement`, `isProtectedNode`, `preservesAnchors`, `canDeleteElement` (`core/content_slot`), giới hạn `core/list <= 8`, `core/pattern_ref <= 16` distinct IDs, `nodes <= 512`, `JSON <= 256 KiB`, và `#[serde(deny_unknown_fields)]` trên `PluginBlockProps`.
   - Giữ nguyên cơ chế bảo vệ dữ liệu chưa lưu khi rời Studio qua `App.tsx:mayLeave` (không gây double `window.confirm`), khôi phục qua lỗi `401`/`412`, cô lập clipboard theo phiên người dùng (`clearAllDrafts` / `resetToken`), và phân biệt rõ tất cả 4 nhánh `StudioSavePolicy`.

---

## Goals

- **G1 — Sidebar-First Studio Shell (Xóa `StudioTopBar` ngang, gom Header & Footer vào Sidebar):**
  - Toàn bộ điều hướng, tiêu đề ngữ cảnh, cây/thuộc tính/catalog, chuyển viewport (**Desktop / Tablet / Mobile**), **Undo / Redo** và **Lưu / Xuất bản** nằm gọn trong sidebar trái (`flex flex-col`, `md:w-[320px]`–`340px`, full height); canvas preview chiếm trọn 100% chiều cao khung bên phải.
  - Duy nhất một `StudioSidebarHeader` 2 vùng ở cấp `StudioWorkspace` (xóa bỏ `renderDrawerHeader` trùng lặp trong `StudioCustomizerNav.tsx`) theo ảnh tham chiếu cuối:
    - **Vùng thương hiệu:** Logo `O` + **OpenDash Studio** + dòng phụ ngữ cảnh (`Bản mẫu · Trình dựng trang` ở Mode 1 / `Nội dung · Trình dựng trang` ở Mode 2) + nút **Thoát** riêng bên phải.
    - **Vùng ngữ cảnh:** Nút quay lại có chữ (theo bảng điều hướng rõ ràng cho từng màn Mode 1 & Mode 2), tiêu đề màn/block + badge loại (`Theme` / `Template` / `Page` / `Post` / CPT), dấu `● Chưa lưu` nhỏ khi dirty, và nút phụ `Reset mặc định` / `Về văn bản thuần` (chỉ ở màn gốc `tree` khi `canReset = true`). Bỏ hoàn toàn dòng breadcrumb trung gian.
- **G2 — Sidebar 3 Màn hình theo Ngữ cảnh (`tree` / `properties` / `add`):**
  - **Bỏ thanh 3 tab** `Blocks / Patterns / Media` cũ. Mặc định mở vào màn **Cấu trúc trang (`tree`)**. Khi trang trống hiển thị trạng thái rỗng + nút **+ Thêm Section**.
  - Mỗi container (`Section`, `Row`, `Column`) có thể thu gọn/mở rộng (`collapse/expand`) và có nút **+ Thêm vào [Container]** ở cuối danh sách con (mở màn `add` chỉ liệt kê đúng loại con hợp lệ), kèm nút **+ Thêm Section** ở cấp gốc.
  - Bấm icon bên trái mỗi hàng trong cây hoặc **nhấp đúp** block (trên cây hoặc trên canvas) mở **menu thao tác theo ngữ cảnh** (tự động mở rộng các nhánh cha): **Chỉnh sửa**, **Nhân bản**, **Copy options**, **Paste options** (khi tương thích), **Di chuyển lên/xuống**, **Xóa**.
  - Chọn **Chỉnh sửa** mới chuyển sidebar sang màn **Thuộc tính (`properties`)** chỉ chứa các field của block đang sửa (không chứa nút Nhân bản hay Xóa). Nút quay lại `← Cấu trúc trang` nằm ở `StudioSidebarHeader`.
- **G3 — Interactive Preview Canvas với Icon Chỉnh Block (Không có nút Add trên Canvas):**
  - Canvas preview **tuyệt đối không có bất kỳ nút Thêm (`+`) nào** (kể cả cuối trang).
  - Giữ viền xanh liền khi chọn (`selected`), viền nét đứt khi hover (`hovered`), và **bổ sung nút icon chỉnh block (`.od-canvas-gear`)** bằng pure DOM API cho node `section` (góc trên-trái) và `element` không phải void tag (góc trên-phải, tránh chồng icon khi padding = 0; ẩn trên `row` và `column`).
  - Xử lý `[data-od-gear]` đầu tiên trong capture-phase `click` listener và xử lý `dblclick` gửi `od-studio-menu` qua CSP bridge (`BRIDGE_SCRIPT_SHA256` được kiểm chứng tự động bằng script test).
- **G4 — Không đổi Schema / Backend Rust ở Đợt 1 & Bảo toàn 100% Hợp đồng Dữ liệu:**
  - Toàn bộ thay đổi nằm gọn trong `cms/admin/`, tái sử dụng 100% AST v1, `BlockSchemaForm`, `MediaPicker`, `PatternLibraryModal`, giữ nguyên route `#/builder` tương thích cũ, và không chạm bất kỳ file nào thuộc phạm vi default theme của Harbor (`#495`).

---

## Non-Goals

- Không thay đổi Rust schema (`opendash-core`), SQLite migrations, hay REST API backend trong đợt nâng cấp UI/UX này.
- Không thêm chỉnh sửa văn bản trực tiếp (inline contenteditable) hay kéo-thả trực tiếp trên iframe canvas.
- Không thêm các thuộc tính style mới chưa có trong AST v1 (như cỡ chữ px tùy biến trên `core/heading`, tỷ lệ cột tùy chỉnh, chiều cao ảnh px) — mọi field minh họa trong mockup được ánh xạ về đúng tập field thật hiện có của `BuilderInspector` và `BlockSchemaForm`.
- Không thay đổi hay ghi đè phạm vi default theme `opendash-starter` đang được Harbor/Aster xử lý tại Issue `#495`.

---

## Approved Technical Decisions (`D1`–`D4`)

Mockup cuối đã chốt toàn bộ bố cục & 9 quy tắc UI chính, và Chủ dự án đã phê duyệt dứt khoát 4 quyết định kỹ thuật sau ngày `2026-11-23`:

| # | Khoảng trống kỹ thuật | Quyết định đã chốt (Owner Approved) |
| :--- | :--- | :--- |
| **D1** | **Copy options / Paste options** | Triển khai trọn cặp **`Copy options` + `Paste options`** trong bộ nhớ phiên Studio (`StudioWorkspace` state, tự xóa khi `clearAllDrafts()` / đổi user / `resetToken`). Chặn copy từ node bị khóa (`isLockedElement`) hoặc `core/content_slot`; với `plugin/block` chỉ copy các key thuộc active `BlockSchemaV1` (lọc bỏ unknown legacy keys); chỉ bật `Paste options` khi đích không bị khóa, cùng `kind`, cùng `type` (và cùng `block_id`), không ghi `binding` vào `plugin/block` và giữ sparse `responsive`. |
| **D2** | **Sắp xếp thứ tự (Reorder) & Drag/Drop** | Triển khai **kéo-thả HTML5 cùng tầng ngay trong cây sidebar (`StudioBlockTree.tsx`)** song song với phím tắt **`Alt+↑/↓`** và nút **`Di chuyển lên` / `Di chuyển xuống`** trong menu thao tác (cả 3 đường đều dùng hàm thuần có kiểm `isProtectedNode` & `preservesAnchors`). Chưa làm kéo-thả trực tiếp trên iframe canvas. |
| **D3** | **Lối vào Patterns & Media Library** | Làm **cả 2 lối vào**: (1) đặt nút mở **Thư viện Pattern…** trong màn `+ Thêm Section` và **Chèn ảnh từ Thư viện Media…** trong màn `+ Thêm vào Column` của `StudioBlockCatalog.tsx`; VÀ (2) đặt thêm 2 nút truy cập nhanh gọn **`Patterns`** và **`Media`** ở đáy cây cấu trúc (`StudioBlockTree.tsx`, dưới nút `+ Thêm Section`). Cả hai màn `tree` và `add` không mount cùng lúc nên dùng thống nhất `data-testid="studio-open-pattern-library"` và `data-testid="studio-open-media-picker"`. |
| **D4** | **Nút `Reset` & 4 nhánh `StudioSavePolicy`** | **Footer sidebar** hỗ trợ đủ 4 nhánh `StudioSavePolicy` (Mode 1: `Lưu thay đổi`; Mode 2 `immediate`: `Lưu & Cập nhật`; Mode 2 `draft`: `Lưu bản nháp` + `Xuất bản`; Mode 2 `published`: `Cập nhật đã xuất bản`), layout wrap gọn ở `320px`. Nút **`Reset mặc định` / `Về văn bản thuần`** (`data-testid="studio-reset-btn"`) đặt cố định tại dòng trạng thái phụ ở đáy vùng ngữ cảnh của `StudioSidebarHeader`, chỉ hiện khi `builderView === 'tree'` và `canReset === true`. |

---

## Critical User Journeys

1. **CUJ-1 — Mở OpenDash Studio (Mode 2 Entry hoặc Mode 1 Template):**
   - **Given** Editor mở `#/studio/entry/page/1` hoặc Admin mở `#/studio` → `Template & Trình dựng trang` → chọn `home`.
   - **When** giao diện Studio nạp xong.
   - **Then** không còn thanh topbar ngang phía trên canvas; sidebar trái (`flex flex-col`) hiển thị duy nhất một `StudioSidebarHeader` 2 vùng (Logo + `OpenDash Studio` + `Thoát` ở trên; nút quay lại có chữ + tiêu đề + badge ở dưới, không có breadcrumb), mặc định hiển thị **Cấu trúc trang (`tree`)** với các container thu gọn/mở rộng được và nút `+ Thêm vào ...` ở cuối từng container; `StudioSidebarFooter` cố định ở đáy có Desktop/Tablet/Mobile + Undo/Redo (ẩn khi không ở builder) + nút Lưu/Xuất bản.
2. **CUJ-2 — Chọn, mở Menu thao tác và Chỉnh thuộc tính (Từ Tree hoặc Canvas):**
   - **Given** trang đang có `Section → Row → Column → Heading`.
   - **When** người dùng bấm icon cài đặt bên trái hàng `Heading` trong cây, HOẶC nhấp đúp `Heading`, HOẶC bấm icon `.od-canvas-gear` trên `Heading` trong iframe canvas.
   - **Then** `Heading` được chọn (`selected`, viền xanh trên canvas & highlight trên cây), các nhánh cha tự mở rộng nếu đang thu gọn, và menu ngữ cảnh hiện ra ngay dưới hàng `Heading` trong cây với các lệnh: **Chỉnh sửa**, **Nhân bản**, **Copy options**, *(**Paste options** nếu tương thích)*, **Di chuyển lên/xuống**, **Xóa**. Khi bấm **Chỉnh sửa**, sidebar chuyển sang màn **Thuộc tính** chỉ chứa các field của `Heading` (không có nút Xóa/Nhân bản); bấm `← Cấu trúc trang` ở `StudioSidebarHeader` quay về cây. Nếu đang ở màn **Thuộc tính** của Node A và click đơn vào Node B trên canvas, màn **Thuộc tính** và tiêu đề `StudioSidebarHeader` tự chuyển sang Node B.
3. **CUJ-3 — Thêm Block hợp lệ theo Container, Row Preset & Chèn Pattern / Media:**
   - **Given** người dùng đang ở màn **Cấu trúc trang**.
   - **When** bấm `+ Thêm vào Column` trên Column B (dù đang chọn 1 ảnh ở Column A), rồi chọn `core/banner` hoặc bấm **"Chèn ảnh từ Thư viện Media…"**.
   - **Then** sidebar chỉ liệt kê các Element hợp lệ (các loại chạm trần như `core/content_slot >= 1` hoặc `core/list >= 8` hiển thị `disabled` kèm chú thích); block hoặc ảnh mới được chèn vào cuối **đúng Column B** (không ghi đè ảnh ở Column A), quay về `view = 'tree'`, chọn node mới và mở rộng Column B. Khi bấm `+ Thêm vào Section`, có thể chọn nhanh **Row 1 cột / 2 cột / 3 cột** (tạo nguyên tử trong 1 bước Undo kèm `responsive.desktop.columns_per_row` cho 2/3 cột).
4. **CUJ-4 — Nhân bản (`Duplicate`) & Copy/Paste Options có bảo vệ bất biến:**
   - **Given** người dùng mở menu trên một `Section` hoặc `core/card` hợp lệ (không chứa block bị khóa).
   - **When** bấm **Nhân bản**.
   - **Then** toàn bộ nhánh con được nhân bản ngay bên dưới node gốc với ID mới duy nhất (`duplicateNode`), bị vô hiệu hóa nếu chứa `isProtectedNode`, chứa `core/content_slot`, dịch chuyển locked sibling (`!preservesAnchors`), vượt quá 8 `core/list`, 16 distinct `core/pattern_ref`, 512 node hoặc 256 KiB. Khi bấm **Copy options** rồi mở menu trên node cùng loại (không khóa) và bấm **Paste options**, các thuộc tính cấu hình hợp lệ được dán sạch vào node đích (giữ sparse `responsive` và `binding` của đích) và ghi nhận 1 bước Undo.
5. **CUJ-5 — Bảo vệ dữ liệu chưa lưu, `Undo/Redo`, `ProviderInactive` và `401`/`412`:**
   - **Given** trang có thay đổi chưa lưu (`● Chưa lưu` hiện ở header sidebar) và có 1 `plugin/block` thuộc plugin đã tắt (`ProviderInactive`).
   - **When** người dùng mở menu trên block bị khóa, hoặc bấm **Undo** khi đang sửa node vừa tạo trong màn `properties`, hoặc bấm **Thoát** / **Về danh sách trang**, hoặc gặp `401`/`412`.
   - **Then** trên block bị khóa, các lệnh `Chỉnh sửa`, `Nhân bản`, `Copy options`, `Paste options` và `Di chuyển` đều bị vô hiệu hóa (chỉ cho giữ nguyên hoặc `Xóa` có xác nhận theo `preservesAnchors`); khi **Undo** xóa mất node đang mở ở `properties` hoặc container đang mở ở `add`, sidebar tự quay về `tree`; bấm `Thoát` hay `← Về danh sách ...` đều đi qua `onNavigateHash` để `App.tsx:mayLeave` nhắc xác nhận đúng 1 lần khi dirty; lỗi `401`/`412` giữ nguyên bản nháp và xóa clipboard khi đổi user/session.

---

## Technical Plan

### 1. Kiến trúc Shell & Bố cục Sidebar-First (`StudioWorkspace.tsx`)

```text
┌──────────────────────────────────┬───────────────────────────────────────────────┐
│ SIDEBAR (flex flex-col, 320px)   │ CANVAS STAGE (flex-1, full viewport height)   │
│ ┌──────────────────────────────┐ │ ┌───────────────────────────────────────────┐ │
│ │ [O] OpenDash Studio   [Thoát]│ │ │ Status/Conflict Banner (chỉ hiện khi cần) │ │
│ │     Nội dung · Trình dựng    │ │ ├───────────────────────────────────────────┤ │
│ ├──────────────────────────────┤ │ │                                           │ │
│ │ ← Về danh sách trang         │ │ │   BuilderPreviewFrame (h-full)            │ │
│ │ Trang chủ             [Page] │ │ │   - Hover: viền nét đứt xanh              │ │
│ │ ● Chưa lưu    [Về văn bản]   │ │ │   - Selected: viền liền xanh              │ │
│ ├──────────────────────────────┤ │ │   - .od-canvas-gear trên section (TL)     │ │
│ │ WORKSPACE (flex-1 min-h-0    │ │ │     và element non-void (TR)              │ │
│ │   overflow-y-auto)           │ │ │   - KHÔNG có bất kỳ nút + Add nào         │ │
│ │ - Mode 1 Customizer Nav      │ │ │                                           │ │
│ │ - Hoặc Builder 3 views:      │ │ │                                           │ │
│ │   (1) tree (mặc định)        │ │ │                                           │ │
│ │   (2) properties (chỉ fields)│ │ │                                           │ │
│ │   (3) add (catalog theo cha) │ │ │                                           │ │
│ ├──────────────────────────────┤ │ │                                           │ │
│ │ [Desktop | Tablet | Mobile]  │ │ │                                           │ │
│ │ [↶] [↷] [Lưu / Xuất bản]     │ │ └───────────────────────────────────────────┘ │
│ └──────────────────────────────┘ │                                               │
└──────────────────────────────────┴───────────────────────────────────────────────┘
```

- **Xóa `StudioTopBar.tsx` trong Slice S01:** Di chuyển `export type StudioSavePolicy` sang `StudioSidebarFooter.tsx`, gỡ hoàn toàn `<StudioTopBar>` khỏi `StudioWorkspace.tsx`.
- **Flex Layout & Canvas Full Height (`StudioWorkspace.tsx`):**
  - `<aside>` dùng `flex max-h-[50vh] w-full shrink-0 flex-col border-b border-zinc-200 bg-white md:max-h-none md:h-full md:w-[320px] md:border-r md:border-b-0`.
  - Vùng giữa (workspace) dùng `min-h-0 flex-1 overflow-y-auto`.
  - `BuilderPreviewFrame` dùng `frameClassName="h-full min-h-[540px] !border border-zinc-300 rounded-lg"` (bỏ `!h-[calc(100vh-120px)]`) để canvas chiếm trọn 100% chiều cao khung bên phải.
  - Truyền đồng nhất `disabled={loading || saving || !csrfToken}` cho cả Mode 1 (`StudioCustomizerNav`) và Mode 2 (`BuilderEditorCore`).

### 2. Bảng Điều hướng Hợp nhất cho `StudioSidebarHeader` (Single Source of Truth)

**Xóa bỏ hoàn toàn `renderDrawerHeader` và khối header theme trùng lặp (`L175-180`) trong `StudioCustomizerNav.tsx`.** Toàn bộ Mode 1 và Mode 2 dùng chung đúng một `StudioSidebarHeader` tại `StudioWorkspace.tsx` dựa trên controlled state `(panel, selectedTarget, builderView, activeSelectedNode, addTargetParentId)`:

| Màn hình hiện tại | Hàng thương hiệu (`Thoát`) | Nút quay lại (`studio-back-btn`) | Tiêu đề & Badge | Nút `Reset` (`studio-reset-btn`) |
| :--- | :--- | :--- | :--- | :--- |
| **Mode 1 · `root`** | `Thoát` → `#/dashboard` | *(ẩn — đang ở màn gốc Customizer)* | `{activeThemeName ?? activeThemeId ?? 'Theme mặc định'}` · `[Theme]` | Ẩn |
| **Mode 1 · `identity` / `theme-options` / `menus` / `reading` / `templates` (list)** | `Thoát` → `#/dashboard` | `← Tùy biến` (`panel = 'root'`) | Tên panel tương ứng (`Nhận diện Site`, `Dựng Template`…) | Ẩn |
| **Mode 1 · `templates` → `selectedTarget` (warning hoặc `tree`)** | `Thoát` → `#/dashboard` | `← Danh sách Template` (`onSelectTarget(null)`) | `{selectedTarget.label}` · `[Template]` | Hiện khi `canReset && builderView === 'tree'` |
| **Mode 1 · `templates` → `selectedTarget` (`properties` hoặc `add`)** | `Thoát` → `#/dashboard` | `← Cấu trúc trang` (`builderView = 'tree'`) | `nodeTitle` (khi `properties`) hoặc `Thêm Section` / `Thêm vào <Container>` (khi `add`) | Ẩn |
| **Mode 2 · Entry (`tree`)** | `Thoát` → Single Edit (`#/pages/:id/edit`, `#/posts/:id/edit`, `#/cpt/:typeKey/:id/edit`) | `page` → `#/pages` (`← Về danh sách trang`); `post` → `#/posts` (`← Về danh sách bài viết`); CPT → `#/cpt/:typeKey` (`← Về danh sách :typeKey`) | `{entryTitle || 'Nội dung'}` · `[Page]` / `[Post]` / `[:typeKey]` | Hiện khi `canReset && builderView === 'tree'` |
| **Mode 2 · Entry (`properties` hoặc `add`)** | `Thoát` → Single Edit | `← Cấu trúc trang` (`builderView = 'tree'`) | `nodeTitle` (khi `properties`) hoặc `Thêm Section` / `Thêm vào <Container>` (khi `add`) | Ẩn |

- **Bảo vệ rời trang (`mayLeave`):** Cả `studio-exit-btn` và `studio-back-btn` (khi điều hướng hash) đều đi qua `onNavigateHash(...)` để `App.tsx:mayLeave` tự hiển thị hộp thoại xác nhận đúng **1 lần** khi `totalDirty === true`. Không gọi thêm `window.confirm` bên trong `StudioSidebarHeader` hay `handleExit`.
- **Không có dòng breadcrumb trung gian** ở bất kỳ màn nào.

### 3. `StudioSidebarFooter` (`cms/admin/src/studio/StudioSidebarFooter.tsx`)

- Export `StudioSavePolicy` (di chuyển từ `StudioTopBar.tsx`).
- **Hàng 1 — Thiết bị xem trước:** 3 nút `Desktop` / `Tablet` / `Mobile` (giữ `data-testid="studio-viewport-desktop|tablet|mobile"`, `aria-pressed`).
- **Hàng 2 — Undo/Redo + Lưu:** Dùng `flex flex-wrap items-center gap-1.5` để không bị tràn ngang ở chiều rộng `320px`:
  - `Undo` (`data-testid="studio-undo-btn"`) & `Redo` (`data-testid="studio-redo-btn"`): chỉ hiển thị khi `canShowUndo` (`isMode2 || (panel === 'templates' && selectedTarget && templateEditingEnabled)`), đúng theo hành vi cũ của `StudioTopBar.tsx:110`.
  - Giữ đủ 4 nhánh `StudioSavePolicy` và đúng `data-testid`:
    1. `customizer` → `Lưu thay đổi` (`studio-save-btn`)
    2. `entry` + `immediate` → `Lưu & Cập nhật` (`studio-save-btn`)
    3. `entry` + `draft_publish` + `draft` → `Lưu bản nháp` (`studio-save-draft-btn`) + `Xuất bản` (`studio-publish-btn`)
    4. `entry` + `draft_publish` + `published` → `Cập nhật đã xuất bản` (`studio-save-btn`)

### 4. Controlled 3-View State Machine & Isolation trong `StudioWorkspace` + `BuilderEditorCore`

- **Lifted / Controlled State tại `StudioWorkspace.tsx`:**
  - `builderView: StudioBuilderView` (`'tree' | 'properties' | 'add'`, mặc định `'tree'`)
  - `addTargetParentId: string | null`
  - `clipboard: StudioBuilderClipboard | null` — đặt tại `StudioWorkspace.tsx` và **bắt buộc xóa (`setClipboard(null)`) trong `clearAllDrafts()`** để cô lập hoàn toàn khi đổi user, `401` re-auth khác tài khoản, hoặc `resetToken` tăng.
  - `menuRequest: { nodeId: string; mode: 'open' | 'toggle'; seq: number } | null`
  - **Keying `BuilderEditorCore`:**
    - Mode 1: `key={selectedTarget.canonical_target_key}`
    - Mode 2: `key={`${entryTypeKey}:${entryId}`}`
    - Khi đổi `selectedTargetKey`, chuyển panel hoặc `Reset`, reset `builderView = 'tree'`, `addTargetParentId = null`, `menuRequest = null`.
- **Auto-Fallback khi `Undo` / `Redo` / `Reset`:**
  - `useEffect` trong `StudioWorkspace` (hoặc `BuilderEditorCore`): nếu `builderView === 'properties'` mà `!locate(activeDoc, activeSelectedId)` HOẶC `builderView === 'add'` mà `addTargetParentId !== null && !locate(activeDoc, addTargetParentId)` → tự động đặt `builderView = 'tree'`, `addTargetParentId = null`, đóng `openMenuNodeId = null`.
- **Quy tắc chuyển màn xác định (Deterministic Transitions):**
  - Chèn node mới từ `StudioBlockCatalog` (bao gồm `appendNode`, `appendRowWithColumns`, `insertMediaImage`, `insertThemePattern`): luôn quay về `builderView = 'tree'`, chọn node mới tạo (`onSelect(newId)`), và mở rộng container cha trong `collapsedIds`.
  - Click đơn vào node khác trên canvas (`od-studio-pick`): đóng `openMenuNodeId`; nếu đang ở `builderView === 'properties'` thì giữ `properties` và chuyển `selectedId` (cùng tiêu đề `StudioSidebarHeader`) sang node vừa chọn.
  - `insertMediaImage(ids, targetColumnId)`: khi gọi từ `StudioBlockCatalog` (`+ Thêm vào Column`), luôn tạo mới `core/image` vào đúng `targetColumnId` (không ghi đè `selectedId`).
  - `insertThemePattern(sourceDoc, title, appendAtEnd)`: khi gọi từ `+ Thêm Section` ở cấp gốc (`appendAtEnd = true`), chèn vào cuối `sections` và quay về `builderView = 'tree'`.

### 5. Hàm AST Thuần Mới trong `cms/admin/src/builder/document.ts`

```ts
export interface StudioBuilderClipboard {
  kind: NodeKind;
  elementType?: string;
  blockId?: string;
  responsive?: BuilderResponsive;
  props?: Record<string, unknown>;
}
```

1. **`duplicateNode(document: BuilderDocument, id: string, registry: Record<string, Record<string, string>>, blockCatalog?: BlockCatalogItemDto[]): { document: BuilderDocument; id: string } | null`**
   - Từ chối (`return null`) nếu:
     - `!locate(document, id)` hoặc **`isProtectedNode(document, id, registry, blockCatalog)`** (tuyệt đối không nhân bản block bị khóa hoặc container chứa block bị khóa để không phá vỡ `is_preserved` trên Rust server).
     - Subtree chứa `core/content_slot`.
     - Tổng `core/list` sau nhân bản > `8`.
     - Tổng số distinct `pattern_id` của `core/pattern_ref` sau nhân bản > `16`.
     - Tổng số node sau nhân bản > `512` hoặc `JSON.stringify(next).length > 256 * 1024`.
     - **`!preservesAnchors(document, next, registry, undefined, blockCatalog)`** (ngăn chặn dịch chuyển chỉ số sibling chứa locked node nằm phía sau).
   - Clone sâu subtree, cấp lại ID mới duy nhất cho toàn bộ cây con, chèn ngay sau node gốc trong cùng mảng `siblings`.
2. **`appendRowWithColumns(document: BuilderDocument, sectionId: string, columnCount: 1 | 2 | 3): { document: BuilderDocument; id: string } | null`**
   - Hàm nguyên tử tạo `Row` + `columnCount` `Column` rỗng trong 1 lần cập nhật (1 bước Undo). Nếu `columnCount > 1`, gán `responsive: { desktop: { columns_per_row: columnCount } }` để `page.rs` kích hoạt CSS grid nhiều cột ngay lập tức.
3. **`copyNodeOptions(document: BuilderDocument, id: string, registry: Record<string, Record<string, string>>, blockCatalog?: BlockCatalogItemDto[]): StudioBuilderClipboard | null`**
   - Nếu là container: clone `node.responsive` (nếu rỗng trả `responsive: undefined`).
   - Nếu là element (`isElement(loc.node)`):
     - Trả `null` nếu `isLockedElement(loc.node, registry, blockCatalog)` hoặc `loc.node.type === 'core/content_slot'`.
     - Nếu `loc.node.type === 'plugin/block'`: lấy active `BlockSchemaV1` từ `blockCatalog`, chỉ giữ các key trong `props.values` được khai báo trong `schema.fields` (lọc bỏ unknown legacy keys chỉ được preserve cho `element.id` gốc trên server [`builder.rs#L709-L715`](../../cms/crates/opendash-core/src/builder/mod.rs)).
     - Nếu `loc.node.type === 'legacy/block'`: copy `props.values` và `blockId`.
     - Các core elements khác: clone `props`, `delete cloned.binding`.
4. **`canPasteOptions(clipboard: StudioBuilderClipboard | null, target: NodeLocation, locked: boolean): boolean`**
   - Trả `false` nếu `!clipboard || locked || clipboard.kind !== target.kind`.
   - Với `target.kind === 'element'`: narrow bằng `isElement(target.node)` → kiểm tra `target.node.type === clipboard.elementType`, và nếu `plugin/block` hoặc `legacy/block` thì `target.node.props.block_id === clipboard.blockId`.
5. **`pasteNodeOptions(document: BuilderDocument, id: string, clipboard: StudioBuilderClipboard): BuilderDocument | null`**
   - Với container: nếu `clipboard.responsive` có ít nhất 1 key thì gán bản clone sâu, ngược lại **`delete node.responsive`** (giữ sparse JSON theo `document.ts#L157`).
   - Với `plugin/block` và `legacy/block`: chỉ cập nhật `props.values` (giữ nguyên `props.block_id`, tuyệt đối **không** ghi key `binding` vào `PluginBlockProps` có `#[serde(deny_unknown_fields)]`).
   - Với các core elements khác: ghi các props đã copy, giữ nguyên `binding` hiện có trên node đích (nếu có).
6. **`reorderSiblingNode(document: BuilderDocument, sourceId: string, targetId: string, position: 'before' | 'after', registry: Record<string, Record<string, string>>, blockCatalog?: BlockCatalogItemDto[]): BuilderDocument | null`**
   - Dùng cho kéo-thả HTML5 cùng tầng trong `StudioBlockTree.tsx`.
   - Trả `null` nếu `sourceId === targetId`, hoặc `source` và `target` không có cùng `parentId` / cùng `kind`, hoặc `isProtectedNode(document, sourceId, registry, blockCatalog)`.
   - Di chuyển `source` đến vị trí trước (`before`) hoặc sau (`after`) `target` trong cùng mảng `siblings`, sau đó kiểm tra **`preservesAnchors(document, next, registry, undefined, blockCatalog)`** — nếu kéo ngang qua một locked sibling làm dịch chuyển chỉ số của nó thì trả `null` (không cho phép drop).

### 6. `StudioBlockTree.tsx`, `StudioBlockCatalog.tsx` & `BuilderInspector.tsx`

- **`StudioBlockTree.tsx`:**
  - Inline SVG icon (0 dependencies mới) cho `settings-2`, `chevron-right/down`, `plus`, `pencil`, `copy`, `sliders-horizontal`, `clipboard-paste`, `arrow-up`, `arrow-down`, `trash-2`, `lock`, `grip-vertical`.
  - **Kéo-thả HTML5 cùng tầng (`D2`):** Mỗi hàng node không bị `isProtectedNode` có `draggable={!disabled && !protectedNode}` (gắn `onDragStart`, `onDragOver`, `onDrop`, `onDragEnd`). Khi kéo qua một sibling hợp lệ (cùng `parentId` và `reorderSiblingNode(...) !== null`), hiển thị vạch xanh mảnh (`border-t-2 border-blue-500` hoặc `border-b-2 border-blue-500`) theo nửa trên/dưới của hàng đích; khi drop gọi `reorderSiblingNode` qua `applyUpdate` (ghi nhận 1 bước Undo). Song song giữ nguyên phím tắt `Alt+↑/↓` và 2 nút `Di chuyển lên` / `Di chuyển xuống` trong menu thao tác.
  - Mỗi hàng node có 2 `ref` map riêng: `rowRefs` (cho nút tên `treeitem`) và `triggerRefs` (cho nút bánh răng mở menu) để khi bấm `Escape` đóng menu, focus trả về đúng nút trigger của node đó.
  - Menu inline (`role="menu"` / `role="menuitem"`) hỗ trợ phím `↑`/`↓`, `Escape`, và click ra ngoài để đóng.
  - Trên block bị khóa (`isLockedElement`), vô hiệu hóa `Chỉnh sửa`, `Nhân bản`, `Copy options`, `Paste options`, `Di chuyển lên/xuống` và kéo-thả; chỉ cho phép giữ nguyên hoặc `Xóa` (vẫn qua `canDeleteElement` + `preservesAnchors`).
  - **Lối vào nhanh Patterns & Media ở đáy cây (`D3`):** Ngay dưới nút `+ Thêm Section` ở đáy `StudioBlockTree.tsx`, hiển thị hàng 2 nút phụ gọn: **`Patterns`** (`data-testid="studio-open-pattern-library"`, mở `PatternLibraryModal`) và **`Media`** (`data-testid="studio-open-media-picker"`, mở `MediaPicker`).
- **`StudioBlockCatalog.tsx`:**
  - `root`: nút **Section trống** + nút **Chọn từ Thư viện Pattern…** (`data-testid="studio-open-pattern-library"`).
  - `section`: **Row 1 cột**, **Row 2 cột**, **Row 3 cột** (gọi `appendRowWithColumns`).
  - `row`: **Column (Cột)**.
  - `column`: ô tìm kiếm nhanh + lưới 2 cột gồm Khối chuẩn + Declarative Blocks phân nhóm + nút **Chèn ảnh từ Thư viện Media…** (`data-testid="studio-open-media-picker"`). Các block đạt giới hạn (`core/content_slot >= 1` hoặc `core/list >= 8`) hiển thị `disabled` kèm nhãn giới hạn.
- **`BuilderInspector.tsx`:**
  - Thêm prop tùy chọn `hideHeader?: boolean` (ẩn tiêu đề `Thuộc tính` + mã `item.node.id` khi dùng trong Studio vì `StudioSidebarHeader` đã hiển thị tiêu đề block) và `hideDeleteButton?: boolean` (kết hợp với việc `BuilderEditorCore` không truyền `onDeleteNode`). `#/builder` cũ giữ mặc định `false`, không bị ảnh hưởng.

### 7. Mở rộng Preview Canvas Bridge An toàn (`BuilderPreviewFrame.tsx`)

- **Không có bất kỳ nút `+ Add` nào trên canvas.**
- **DOM Injection an toàn & Tránh chồng icon:**
  - CSS trong `preparePreviewMarkup`:
    - `:where([data-od-editable="1"]){position:relative;cursor:pointer;}` (dùng `:where()` để không đè `position:sticky`/`fixed` của header theme).
    - `[data-od-editable="1"][data-od-hover="1"]{outline:1px dashed #2563eb;outline-offset:-1px;}`
    - `[data-od-editable="1"][data-od-selected="1"]{outline:2px solid #2563eb;outline-offset:-1px;}`
    - `.od-canvas-gear` là nút nhỏ (`24×24px`, `margin:0 !important`, `position:absolute;top:4px;z-index:20;`):
      - Trên `section`: `left:4px;z-index:15;`
      - Trên `element`: `right:4px;z-index:20;` (tránh trùng tọa độ với icon `section` khi padding = 0).
      - Nền trắng viền xám mờ (`opacity:0.8`), chuyển nền xanh `#2563eb` chữ trắng (`opacity:1`) khi cha có `[data-od-selected="1"]`.
  - Trong `BRIDGE_SCRIPT`:
    - Chỉ tạo nút `.od-canvas-gear` bằng **pure DOM API** (`document.createElement('button')`, `document.createElementNS('http://www.w3.org/2000/svg', ...)` — tuyệt đối không dùng `innerHTML` chứa `nodeId`) cho các node `[data-od-editable="1"][data-node-id]` có `data-od-kind` là `'section'` hoặc `'element'` và **không phải void/replaced tag** (`IMG`, `HR`, `BR`, `INPUT`, `VIDEO`, `AUDIO`, `IFRAME`, `EMBED`, `SOURCE`), append làm **last child**.
    - **Thứ tự sự kiện capture-phase (`click`):** Bên trong `document.addEventListener("click", ..., true)` hiện có, kiểm tra `var gear = e.target && e.target.closest ? e.target.closest("[data-od-gear]") : null;` **trước tiên**; nếu khớp, gọi `e.preventDefault(); e.stopPropagation();`, lấy `nodeId = gear.getAttribute("data-od-gear")`, áp dụng `data-od-selected`, gửi `postMessage({ type: "od-studio-menu", nonce, nodeId, mode: "toggle" }, "*")` và **`return` ngay** (không gửi thêm `od-studio-pick`).
    - **Sự kiện `dblclick`:** Bỏ qua nếu `e.target.closest("[data-od-gear]")`; nếu trúng `[data-od-editable="1"][data-node-id]`, gọi `e.preventDefault(); e.stopPropagation();`, áp dụng `data-od-selected`, gửi `postMessage({ type: "od-studio-menu", nonce, nodeId, mode: "open" }, "*")` (mở idempotent, không bị flicker).
- **Parent message handler trong `BuilderPreviewFrame.tsx`:**
  - Thêm prop `onOpenNodeMenu?: (id: string, mode: 'open' | 'toggle') => void`.
  - Khi nhận `od-studio-menu`: kiểm tra `nonce` và `locate(doc, nodeId)`, đặt **`lastPickedFromIframeRef.current = nodeId`** (ngăn `sendHighlight()` tự ý smooth-scroll iframe), rồi gọi `onOpenNodeMenuRef.current?.(nodeId, mode)`.
- **Kiểm chứng tự động `BRIDGE_SCRIPT_SHA256`:** Cập nhật chính xác SHA-256 base64 của `BRIDGE_SCRIPT` và xác minh bằng script test tự động `cms/admin/scripts/test-studio-ux.mjs`.

---

## Alternatives Considered

1. **Giữ `StudioTopBar` ngang và chỉ thu gọn `BuilderTree` + `BuilderInspector` thành 2 tab:**
   - *Bị bác bỏ:* Trái trực tiếp với quyết định cuối #2 và #9 của Chủ dự án.
2. **Nén Header sidebar vào 1 hàng gọn duy nhất + thêm dòng breadcrumb:**
   - *Bị bác bỏ:* Chủ dự án đã bác bỏ phiên bản nén 1 hàng và yêu cầu bỏ dòng breadcrumb `Trang chủ / Nội dung trang` (xem `references/owner-sidebar-reference.png` và `START-HERE.md` #6).
3. **Bổ sung thêm thuộc tính CSS/Schema mới trên Rust ngay trong đợt này:**
   - *Bị bác bỏ:* Trộn lẫn việc nâng cấp UX giao diện với việc thay đổi hợp đồng dữ liệu AST/Renderer (`opendash-core` + `page.rs`), tăng rủi ro regression và chưa được Owner duyệt đổi schema.

---

## Non-Functional Requirements

- **Bảo mật & CSP:** Giữ nguyên tuyệt đối `sandbox="allow-scripts"` và `script-src 'sha256-...'` của `BuilderPreviewFrame` (không dùng `'unsafe-inline'` cho script, không dùng `innerHTML` với `nodeId`, không tải icon từ CDN ngoài).
- **Bảo toàn dữ liệu & Hợp đồng lõi:** Không làm thay đổi định dạng `BuilderDocument` / Comment Block `<!-- od:... -->`, bảo toàn 100% `preservesAnchors`, `is_preserved`, `canDeleteElement` (`core/content_slot`), `ProviderInactive`/`Withdrawn` lock, `draft_publish`, và `401`/`412` recovery.
- **0 Dependencies mới:** Không thêm bất kỳ npm package nào vào `cms/admin/package.json`; test script dùng `node:test` + `esbuild` sẵn có từ `vite`.
- **Tương thích ngược Route `#/builder`:** Giữ nguyên `BuilderTree` và `BuilderInspector` mặc định cho `#/builder`.

---

## Detailed Implementation Plan

### Target Files to Create/Modify/Delete

| Phạm vi / Slice | File mục tiêu | Hành động | Vai trò chính |
| :--- | :--- | :---: | :--- |
| **Slice S01** | `cms/admin/src/builder/document.ts` | Modify | Thêm `StudioBuilderClipboard`, `duplicateNode`, `appendRowWithColumns`, `copyNodeOptions`, `canPasteOptions`, `pasteNodeOptions`, `reorderSiblingNode`. |
| **Slice S01** | `cms/admin/src/studio/StudioSidebarHeader.tsx` | Create | Header sidebar 2 vùng chuẩn ảnh tham chiếu (Brand + `Thoát` / Back + Title + Badge + `● Chưa lưu` + `Reset` ở màn gốc `tree`), không breadcrumb. |
| **Slice S01** | `cms/admin/src/studio/StudioSidebarFooter.tsx` | Create | Export `StudioSavePolicy`; Footer cố định chứa Viewport (`Desktop`/`Tablet`/`Mobile`), `Undo`/`Redo` (khi `canShowUndo`), và 4 nhánh nút Lưu/Xuất bản. |
| **Slice S01** | `cms/admin/src/studio/StudioBlockTree.tsx` | Create | Cây cấu trúc `collapse/expand`, kéo-thả HTML5 cùng tầng (`reorderSiblingNode`), icon cài đặt bên trái + nhấp đúp mở menu inline (`role="menu"`), nút `+ Thêm vào ...` theo container, `+ Thêm Section` và 2 nút nhanh `Patterns`/`Media` ở đáy cây. |
| **Slice S01** | `cms/admin/src/studio/StudioBlockCatalog.tsx` | Create | Màn `Thêm` theo container (`root`: Section + Pattern Library; `section`: Row 1/2/3 cột; `row`: Column; `column`: tìm kiếm + lưới Element hợp lệ + Media Picker). |
| **Slice S01** | `cms/admin/src/builder/BuilderInspector.tsx` | Modify | Thêm `hideHeader?: boolean` và `hideDeleteButton?: boolean` (mặc định `false`). |
| **Slice S01** | `cms/admin/src/builder/BuilderWorkspace.tsx` | Modify | Refactor `BuilderEditorCore` sang controlled 3-view (`tree`/`properties`/`add`), bỏ 3 tab & dropdown `Element mới`, refactor `insertMediaImage(ids, targetColumnId)` & `insertThemePattern(..., appendAtEnd)`. |
| **Slice S01** | `cms/admin/src/studio/StudioCustomizerNav.tsx` | Modify | Xóa `renderDrawerHeader` và header theme trùng lặp; truyền controlled builder props xuống `BuilderEditorCore`. |
| **Slice S01** | `cms/admin/src/studio/StudioWorkspace.tsx` | Modify | Gỡ `StudioTopBar`, gắn `StudioSidebarHeader` + `StudioSidebarFooter`, giữ `builderView`/`addTargetParentId`/`clipboard` (reset trong `clearAllDrafts`), key `BuilderEditorCore`, sửa `flex-col` & full-height iframe, đồng bộ `disabled` Mode 1. |
| **Slice S01** | `cms/admin/src/studio/StudioTopBar.tsx` | Delete | Xóa component topbar ngang đã thay thế bởi `StudioSidebarHeader` + `StudioSidebarFooter`. |
| **Slice S01** | `cms/admin/scripts/test-studio-ux.mjs` | Create | Bộ unit test tự động (`node --test`) kiểm chứng `duplicateNode`, `appendRowWithColumns`, `copyNodeOptions`, `canPasteOptions`, `pasteNodeOptions` (gồm `preservesAnchors`, locked nodes, sparse `responsive`, `PluginBlockProps` unknown-key stripping). |
| **Slice S02** | `cms/admin/src/builder/BuilderPreviewFrame.tsx` | Modify | Gắn `.od-canvas-gear` bằng pure DOM API trên `section` (TL) & non-void `element` (TR), xử lý capture-phase click + `dblclick` phát `od-studio-menu` (`mode: 'toggle' | 'open'`), cập nhật `BRIDGE_SCRIPT_SHA256`, đặt `lastPickedFromIframeRef.current = nodeId`, thêm `onOpenNodeMenu`. |
| **Slice S02** | `cms/admin/src/studio/StudioWorkspace.tsx`, `cms/admin/src/studio/StudioCustomizerNav.tsx`, `cms/admin/src/builder/BuilderWorkspace.tsx`, `cms/admin/src/studio/StudioBlockTree.tsx` | Modify | Nối `onOpenNodeMenu` qua `menuRequest` vào `BuilderEditorCore`, tự động mở rộng ancestor + cuộn hàng + quản lý focus bàn phím (`Escape` trả focus về trigger, click ngoài đóng menu, `od-studio-pick` đóng menu). |
| **Slice S02** | `cms/admin/scripts/test-studio-ux.mjs` | Modify | Bổ sung kiểm tra tự động xác nhận `SHA256(BRIDGE_SCRIPT) === BRIDGE_SCRIPT_SHA256` và không có `innerHTML` trong `BRIDGE_SCRIPT`. |

---

### Slice S01: AST Clipboard/Duplicate Primitives, Sidebar-First Studio Shell & 3-View Contextual Sidebar

- **Dependencies:** Đặc tả đạt `Status: Approved` và đã merge vào `main`.
- **File Ownership:**
  - `cms/admin/src/builder/document.ts` — thêm `StudioBuilderClipboard`, `duplicateNode`, `appendRowWithColumns`, `copyNodeOptions`, `canPasteOptions`, `pasteNodeOptions`, `reorderSiblingNode`
  - `cms/admin/src/studio/StudioSidebarHeader.tsx` *(tạo mới)*
  - `cms/admin/src/studio/StudioSidebarFooter.tsx` *(tạo mới — chứa `export type StudioSavePolicy`)*
  - `cms/admin/src/studio/StudioBlockTree.tsx` *(tạo mới)*
  - `cms/admin/src/studio/StudioBlockCatalog.tsx` *(tạo mới)*
  - `cms/admin/src/builder/BuilderInspector.tsx` — thêm `hideHeader?: boolean`, `hideDeleteButton?: boolean`
  - `cms/admin/src/builder/BuilderWorkspace.tsx` — refactor `BuilderEditorCore` sang controlled 3-view (`tree`/`properties`/`add`), cập nhật `insertMediaImage` & `insertThemePattern`
  - `cms/admin/src/studio/StudioCustomizerNav.tsx` — xóa `renderDrawerHeader` & header trùng lặp
  - `cms/admin/src/studio/StudioWorkspace.tsx` — thay `StudioTopBar` bằng `StudioSidebarHeader` + `StudioSidebarFooter`, lift `builderView` / `addTargetParentId` / `clipboard`, cập nhật flex layout & canvas full-height
  - `cms/admin/src/studio/StudioTopBar.tsx` *(xóa)*
  - `cms/admin/scripts/test-studio-ux.mjs` *(tạo mới)*
- **Acceptance Criteria:**
  1. `StudioTopBar.tsx` đã xóa; không còn global toolbar ngang phía trên canvas; Viewport (`Desktop`/`Tablet`/`Mobile`), `Undo`/`Redo` (ẩn khi `!canShowUndo`) và đủ 4 nhánh `StudioSavePolicy` nằm ở `StudioSidebarFooter`.
  2. `StudioSidebarHeader` hiển thị đúng 2 vùng (Brand + `Thoát` ở trên; nút quay lại có chữ + tiêu đề + badge + `● Chưa lưu` + `Reset` ở màn gốc `tree`), không có breadcrumb, không có header trùng lặp trong Mode 1 `StudioCustomizerNav`.
  3. Không còn 3 tab `Blocks / Patterns / Media`; mặc định hiển thị `StudioBlockTree` có thu gọn/mở rộng, **kéo-thả HTML5 cùng tầng** (`reorderSiblingNode`, tôn trọng `isProtectedNode` & `preservesAnchors`), nút `+ Thêm vào ...` ở cuối từng container, `+ Thêm Section` và 2 nút nhanh `Patterns` / `Media` ở đáy cây (`D3`), cùng `StudioBlockCatalog` hỗ trợ Row 1/2/3 cột (nguyên tử + `columns_per_row`), Pattern Library (`studio-open-pattern-library`) và Media Picker (`studio-open-media-picker` chèn đúng `addTargetParentId`).
  4. Menu ngữ cảnh hỗ trợ `Chỉnh sửa`, `Nhân bản`, `Copy options`, `Paste options`, `Di chuyển lên/xuống`, `Xóa`; màn `properties` chỉ chứa field của block đang sửa (không có nút Xóa/Nhân bản).
  5. `node cms/admin/scripts/test-studio-ux.mjs`, `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `check-standards.py`, `check-plans.py` và `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` PASS 100%.
- **Rollback:** Revert các file thuộc Slice S01 trong `cms/admin/`.

---

### Slice S02: Interactive Preview Canvas Gear Icon, Double-Click Bridge & Focus/Keyboard Polish

- **Dependencies:** `Slice S01` đã merge vào `main`.
- **File Ownership:**
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx` — pure DOM `.od-canvas-gear` trên `section` (TL) & non-void `element` (TR), capture-phase click + `dblclick` gửi `od-studio-menu` (`mode: 'toggle' | 'open'`), cập nhật `BRIDGE_SCRIPT_SHA256`, đặt `lastPickedFromIframeRef.current = nodeId`, thêm `onOpenNodeMenu`
  - `cms/admin/src/studio/StudioWorkspace.tsx` & `cms/admin/src/studio/StudioCustomizerNav.tsx` — truyền `onOpenNodeMenu` qua `menuRequest` xuống `BuilderEditorCore` cho cả Mode 1 và Mode 2
  - `cms/admin/src/builder/BuilderWorkspace.tsx` & `cms/admin/src/studio/StudioBlockTree.tsx` — xử lý `menuRequest` (tự mở rộng ancestor, chuyển về `tree`, cuộn vào tầm nhìn, focus menu), đóng menu khi `od-studio-pick` hoặc click ngoài hoặc `Escape` (trả focus về trigger)
  - `cms/admin/scripts/test-studio-ux.mjs` — bổ sung test tự động xác nhận `SHA256(BRIDGE_SCRIPT) === BRIDGE_SCRIPT_SHA256`
- **Acceptance Criteria:**
  1. Trên iframe preview (`interactive`), mỗi `Section` có icon `.od-canvas-gear` góc trên-trái, mỗi non-void `Element` có icon `.od-canvas-gear` góc trên-phải (không chồng tọa độ khi padding = 0), `Row` và `Column` không có icon, và **0 nút `+ Add` trên canvas**.
  2. Bấm `.od-canvas-gear` (toggle) hoặc nhấp đúp block trên canvas (open idempotent) mở đúng menu thao tác trong sidebar, tự mở rộng các nhánh cha đang thu gọn và không làm iframe tự ý cuộn nhảy.
  3. `node cms/admin/scripts/test-studio-ux.mjs` xác nhận `BRIDGE_SCRIPT_SHA256` khớp 100% với `BRIDGE_SCRIPT`; trình duyệt báo `0` lỗi CSP / `0` console error.
  4. `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `check-standards.py`, `check-plans.py` và `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` PASS 100%.
- **Rollback:** Revert các file thuộc Slice S02 trong `cms/admin/`.

---

### Verification Plan

Sau mỗi Slice (và trước khi đóng Spec sang `Verified`), chạy đầy đủ bộ lệnh kiểm chứng từ thư mục gốc repository:

```powershell
node cms/admin/scripts/test-studio-ux.mjs
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
git diff --check
```

Kèm kiểm chứng trình duyệt trực tiếp (`Live Browser E2E`) trên server cục bộ (`http://127.0.0.1:18766/admin/`), đối chiếu thêm với `cms/crates/opendash-server/tests/studio_s02_preview_api.rs`:
1. Mở Mode 2 (`#/studio/entry/page/<id>`) và Mode 1 (`#/studio` → Template `home`): xác nhận không còn topbar ngang, chỉ có duy nhất 1 `StudioSidebarHeader` 2 vùng không có breadcrumb, `StudioSidebarFooter` có Desktop/Tablet/Mobile + Undo/Redo + Lưu/Xuất bản, canvas full-height.
2. Kiểm tra trên canvas: hover có viền nét đứt, chọn có viền xanh liền, có icon `.od-canvas-gear` trên Section (TL) và Element (TR), **0 nút Add trên canvas**, `0` lỗi CSP/console.
3. Thử trọn luồng: bấm icon trên canvas / nhấp đúp → mở menu → **Chỉnh sửa** (chỉ thấy fields, không có nút Xóa) → click đơn node khác trên canvas chuyển đúng màn thuộc tính → quay lại `← Cấu trúc trang` → **+ Thêm vào Column** (chèn ảnh Media vào đúng cột mục tiêu, không ghi đè ảnh đang chọn ở cột khác) → **Row 2 cột** (1 bước Undo) → **Nhân bản** → **Copy options** & **Paste options** → **Undo/Redo** (tự fallback về `tree` nếu node bị xóa) → **Lưu bản nháp** / **Xuất bản** và xác nhận ngoài trang công khai.
4. Kiểm tra các ca bảo vệ: xóa `core/content_slot` bắt buộc bị chặn, block `ProviderInactive` bị khóa sửa/nhân bản/copy/paste/di chuyển nhưng vẫn giữ nguyên vị trí khi lưu node khác, bấm **Thoát** hay **← Về danh sách trang** khi chưa lưu chỉ hiện đúng 1 hộp thoại xác nhận từ `App.tsx:mayLeave`.
