# OpenDash Studio: WordPress Customizer + Flatsome UX Builder Hybrid & Unified Comment-Block Entry Content

- **Author:** Nova (Elephant)
- **Status:** Verified
- **Date:** 2026-11-22
- **Baseline:** `main@e57ad90`
- **Approved By:** Owner
- **Approval Evidence:** Owner chat 2026-11-22 ("Duyệt" — OpenDash Studio Mode 1 Customizer + Template UX Builder, Mode 2 Entry UX Builder & Unified Gutenberg Comment-Block `values_json["body"]`)
- **Goldfish Review:** 4-agent independent review (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, `egm-zoom-out`) · 2026-11-22 · reviewed `main@e57ad90` (all Blocking/Critical/Major resolved)
- **Related:** [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md), [`docs/contracts/CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`docs/contracts/PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md), [`docs/designs/cpt-content-builder-and-pattern-library.md`](cpt-content-builder-and-pattern-library.md), [`docs/designs/admin-design-system-and-ui-polish.md`](admin-design-system-and-ui-polish.md)

---

## Overview

OpenDash hiện đã có Visual Builder 4 tầng (`SectionNode → RowNode → ColumnNode → ElementNode`), hệ thống typed template overrides (`template_overrides`, `TemplateCandidate`), bộ Pattern Library (`pattern` CPT, `PatternRef`), cùng giao diện Admin Style A (Shadcn + Linear). Tuy nhiên trải nghiệm tùy biến giao diện và dựng nội dung đang bị phân mảnh: cài đặt site, tùy chọn theme, menus, cài đặt đọc và `template_overrides` nằm rải rác trong menu Admin; khung preview là iframe tĩnh chưa hỗ trợ chọn trực tiếp trên canvas; và mỗi bài viết/trang (`page`, `post`, CPT) phải chọn một trong hai kho lưu tách biệt (`values_json["body"]` cho Classic Editor **hoặc** `entry_builder_documents.document_json` cho Builder), gây đứt gãy dữ liệu khi chuyển qua lại giữa hai chế độ.

Đặc tả này thiết kế **OpenDash Studio** — mô hình lai giữa **WordPress Customizer** và **Flatsome UX Builder**, đi kèm chuẩn lưu trữ hợp nhất bằng **Gutenberg-style Comment Blocks (`<!-- od:... -->`)**:

1. **Chế độ 1 — Site Customizer + Template UX Builder (`#/studio`):** Không gian toàn màn hình (Admin-only) với cột trái `340px` trượt cấp (`drill-down`) gồm 2 nhóm: *Tùy biến Website* (Nhận diện Site, Tùy chọn Giao diện, Vị trí Menu Slots, Trang chủ & Đọc — hỗ trợ Live Preview in-memory và lưu các mục đã sửa qua đúng các domain API hiện hữu) và *Dựng Template* (dùng đúng `TemplateCandidate` từ `part:header`, `part:footer`, `single`, `archive`, `page`, `page_layout:*` — trượt sang Flatsome UX Builder để chỉnh `template_overrides.ast_json`).
2. **Chế độ 2 — Page / Entry UX Builder (`#/studio/entry/:typeKey/:id`):** Mở từ nút `Sửa với UX Builder` trong màn hình sửa đơn lẻ của `page`, `post` hoặc CPT bật `ContentSupport::Builder`. Chỉ sửa **duy nhất nội dung `values_json["body"]` của Entry đó** trên nền xem trước đầy đủ có Header/Footer ở chế độ chỉ đọc.
3. **Nguồn Sự thật Duy nhất (`values_json["body"]`):** Bỏ kho lưu song song cho nội dung entry thông thường. Cả Classic Editor và UX Builder cùng đọc từ và ghi vào `content_entries.values_json["body"]` dưới dạng Gutenberg Comment Blocks `<!-- od:... -->` khớp 1:1 với `opendash-core::builder::BuilderDocument`. Riêng `pattern` CPT giữ nguyên `entry_builder_documents` đã nghiệm thu ở Spec 2 (`section` CPT đã retired ở migration `0044` và bị cấm tạo mới/truy cập `403`).

---

## The Problem

1. **Phân mảnh kho lưu Editor vs Builder cho Entry (`PRODUCT_REQUIREMENTS.md` & Spec 2 cũ):** Hiện tại `public_content.rs`, `storage/entries.rs`, `storage/overrides.rs` và `SingleEditWorkspace.tsx` tách nhánh theo `entry.content_mode`: `Editor` đọc `values_json["body"]`, còn `Builder` bắt buộc có `entry_builder_documents`. Sửa nội dung bằng Classic Editor không xuất hiện trong UX Builder và ngược lại.
2. **Trải nghiệm Customizer & Template Builder tách rời khỏi Live Preview:** Site Identity (`general`), Theme Options (`theme_options`), Menu Slots (`menu_slots`), Reading (`reading`) và `template_overrides` nằm ở các màn hình riêng rẽ, không có live preview transient trước khi bấm Lưu.
3. **Canvas xem trước chưa hỗ trợ tương tác (`BuilderWorkspace.tsx`):** `<iframe id="canvas" sandbox="" srcDoc={previewHtml} />` hiển thị ảnh tĩnh; người dùng chưa thể rê chuột hiện viền hay bấm trực tiếp phần tử trên trang để mở Inspector như Flatsome UX Builder.
4. **Khoảng trống CPT tự tạo bật `ContentSupport::Builder`:** `EffectiveContentSchema::assemble` trong `cms/crates/opendash-core/src/content.rs` chỉ auto-inject `"featured_image"`, không auto-inject `"body"`; `eligible_base_field_keys` (`content.rs:2001`), `build_core_and_group_panels` (`content.rs:2053`), `verify_supported_values` (`storage/entries.rs:31-52`) và `SingleEditWorkspace.tsx::supportAllowsField` (`L70-85`) chưa mở `"body"` khi CPT chỉ bật `ContentSupport::Builder`.
5. **Ranh giới kích thước giữa `BuilderDocument` (`256 KiB`) và `RichText` (`200,000` chars):** `MAX_BUILDER_JSON_BYTES = 256 * 1024` (`262_144` bytes). Khi serialize thành Comment Blocks `<!-- od:... -->` kèm escaping JSON, chuỗi `body` có thể vượt `200,000` ký tự của `RichText` (`content.rs:1617`) hoặc chạm trần `MAX_CONTENT_ENTRY_JSON_BYTES = 512 * 1024` (`routes/entries.rs:28`).

---

## Goals

- **G1 — Hợp nhất 100% nội dung Entry vào `values_json["body"]`:** Mọi `page`, `post` và CPT bật `ContentSupport::Builder` (ngoài `pattern`) dùng chung duy nhất `values_json["body"]` cho Classic Editor, UX Builder, Admin Preview và Public Rendering.
- **G2 — Bộ mã hóa/giải mã Comment Block thuần Rust (`opendash-core::builder_blocks`):** Khớp 1:1 với các struct `#[serde(deny_unknown_fields)]` hiện có của `BuilderDocument`, chỉ xuất comment delimiter `<!-- od:... -->` sạch sẽ (không chèn `<div class="od-section">` dư thừa vào `values_json["body"]`), hỗ trợ auto-wrap nội dung văn bản cũ (`> 16,384` chars được chia chunk hợp lệ theo ngưỡng `valid_text`), escaping comment boundary an toàn tuyệt đối, và recovery không mất chữ khi comment bị sửa lệch.
- **G3 — Đồng bộ Media/Pattern Refs trên MỌI Writer & Giữ Vững Media ACL:** Tất cả điểm ghi `content_entries` (Composite UI, `create/update_entry_with_schema_fence`, plugin `content_save` via `create/update_entry_with_fence`, `import.rs`, và `save_entry_builder`) đều đồng bộ `entry_builder_media_refs` và `entry_pattern_refs` (chỉ cấp quyền công khai ở cả 2 nhánh trực tiếp và 1-hop pattern khi `type` active, `public = true`, `body` công khai, và entry `published`).
- **G4 — OpenDash Studio Mode 1 (`#/studio`):** Giao diện 340px drill-down Style A ngoài `.od-legacy-scope`, dùng các form drawer gọn riêng trong `StudioCustomizerNav.tsx` (không phá vỡ các trang Admin full-page hiện có), live preview in-memory (read-only, **không** dùng `AdmittedMutation`), lưu các domain đã chỉnh qua API sẵn có, dựng Template theo `TemplateCandidate` và cardinality `core/content_slot` từ `overrides.rs`.
- **G5 — OpenDash Studio Mode 2 (`#/studio/entry/:typeKey/:id`):** Mở thẳng UX Builder cho `values_json["body"]` của Entry (hỗ trợ cả `DraftPublish` và `Immediate`), xem trước đầy đủ có Header/Footer ở chế độ chỉ đọc, giữ `<textarea>` Classic Editor luôn mở để sửa trực tiếp văn bản và bảo toàn `editorDraftStore` & `401`/`412` drafts.
- **G6 — Interactive Canvas an toàn XSS & Đơn giản Hóa Nạp Asset:** Giữ nguyên `script-src 'none'` và bỏ hoàn toàn `data-od-*` khỏi Public routes; nâng cấp iframe Admin sang `sandbox="allow-scripts"` (cấm `allow-same-origin`) + rewrite URL `/theme/...` & `/media/...` sang `${window.location.origin}/...` + CSP `script-src 'sha256-...'` + nonce/scroll `postMessage`.

---

## Non-Goals

- Không thay thế `entry_builder_documents` đối với `pattern` CPT (`pattern` giữ kho lưu Spec 2). Không kích hoạt lại loại `section` đã retired ở migration `0044`.
- Không thay đổi mô hình lưu trữ văn bản thoát HTML (`escape_html`) của `KnownElement::Text(TextProps { text })` và Classic `RichText` trong đợt này (chưa mở trình soạn thảo WYSIWYG raw HTML tự do cho end-user).
- Không bổ sung `logo_media_id` vào bảng/struct `GeneralSettings` (logo website do Theme Options kiểu `image_ref` quản lý theo kiến trúc theme v3 hiện hữu).
- Không tạo thêm bảng lưu trữ lịch sử `legacy_dual_content_archive` hay endpoint mega-transaction mới `PUT /api/admin/studio/customizer` (tuân thủ quyết định `D02` của Spec 2 về không giữ tương thích dữ liệu lịch sử cũ và giữ kiến trúc tối giản theo khuyến nghị của `egm-zoom-out`).

---

## Superseded Contracts & Migration Impact

1. **`docs/product/PRODUCT_REQUIREMENTS.md`:** Thay thế quy định tách kho lưu/nhánh dữ liệu giữa Classic Editor và Visual Builder trên cùng một Entry bằng cơ chế **nguồn duy nhất `values_json["body"]` dạng Comment Block**.
2. **`docs/designs/cpt-content-builder-and-pattern-library.md` (Spec 2):**
   - Giữ nguyên toàn bộ kiến trúc `Pattern` CPT (`entry_builder_documents`), `PatternRef` (depth = 1, max 16 refs/document), `PatternLibraryModal`, DoS caps và bảng `entry_builder_media_refs` / `entry_pattern_refs`.
   - `entry_builder_documents` không còn làm kho chính cho `page` hay CPT khác `pattern`; hàm dọn dẹp nhẹ lúc khởi động (`migrate_demo_entry_builder_docs_on_startup` trong `storage/builder.rs`) chuyển các dòng non-pattern demo còn trong `entry_builder_documents` vào `values_json["body"]` rồi xóa dòng non-pattern khỏi `entry_builder_documents`.
   - `content_entries.content_mode` (`editor` | `builder`) chỉ giữ vai trò ghi nhận chế độ mở gần nhất / UI preference, **không** còn là điều kiện phân nhánh render public (`public_content.rs`), preview (`overrides.rs`), hay guard `422` khi lưu composite entry (`entries.rs`).

---

## Open Questions

*(Không còn câu hỏi mở chặn thiết kế. Tất cả phát hiện từ Goldfish Round 1 và Round 2 đã được khóa dứt khoát ở Technical Plan bên dưới.)*

---

## Critical User Journeys

1. **CUJ-1 — Tùy biến Website với Live Preview (`#/studio`):**
   - Admin vào `Giao diện → OpenDash Studio` (`#/studio`): Sidebar Admin ẩn đi, hiện TopBar + panel trái `340px` ngoài `.od-legacy-scope`.
   - Chọn *Tùy biến Website → Nhận diện Site*: sửa Tiêu đề site (`site_title`), Tagline; xem thay đổi tức thì ở Preview (gọi `POST /api/admin/studio/preview` in-memory với `TransientStudioOverrides`, DB chưa đổi); quay lại vào *Vị trí Menu* gắn menu khác vào slot `header` (hoặc bấm liên kết mở `#/appearance/menus` nếu cần thêm/sửa cây menu chi tiết), kiểm tra ở viewport `Mobile 390px`; bấm `Lưu thay đổi` → Studio gọi các API domain có thay đổi (`saveGeneralSettings`, `saveThemeOptions`, `putMenuSlots`, `saveReadingSettings`) kèm `If-Match` version tương ứng.
2. **CUJ-2 — Dựng Template Theme v3 bằng UX Builder (`#/studio`):**
   - Admin chọn *Dựng Template → Header (`part:header`)* hoặc *Single (`single` / `single:type:dich_vu`)*. Nếu target đang dùng template gốc C-SEC của Theme (chưa có hàng trong `template_overrides`), Studio hiển thị cảnh báo rõ ràng trước khi Admin bấm *"Tạo bản dựng UX Builder"*.
   - Rê chuột trên khung Preview phải thấy viền hover; click trực tiếp vào nút trên Template → bridge script gửi `postMessage` (kèm nonce) → cây bên trái tự chọn đúng node & mở `BuilderInspector`. Thử xóa `core/content_slot` duy nhất trên `Single` bị chặn theo quy tắc `validate_content_slots`; `core/template_part` được thêm/sửa `part_key`/xóa bình thường.
3. **CUJ-3 — Chỉnh sửa Hai Chiều Classic Editor ↔ UX Builder trên 1 Page/Post/CPT (`#/content/...` ↔ `#/studio/entry/...`):**
   - Editor tạo/mở Trang `Giới thiệu` (hoặc CPT bật `ContentSupport::Builder`), nhập văn bản thuần trong Classic Editor, bấm Lưu, rồi bấm `Sửa với UX Builder`.
   - Nếu đang còn thay đổi chưa lưu trong `editorDraftStore`, Classic Editor hiển thị hộp thoại (`Lưu & mở UX Builder` / `Bỏ thay đổi & mở` / `Hủy`) — không bao giờ âm thầm mất chữ.
   - Mode 2 (`#/studio/entry/page/<id>`) mở ra: panel trái chỉ có UX Builder của `body`, Preview phải hiển thị khung trang hoàn chỉnh gồm Header + Template Single/Page + Nội dung Entry đang sửa + Footer (trong đó Header/Footer ở chế độ chỉ đọc, không chọn được). Văn bản cũ tự động bọc thành `SectionNode → RowNode → ColumnNode → Text` (`had_freeform_wrap: true`); Editor thêm `Heading`, `Image`, `Row`, `PatternRef` rồi bấm `Lưu bản nháp` (`draft`) hoặc `Xuất bản` / `Cập nhật đã xuất bản` (hoặc `Lưu & Cập nhật` với CPT `immediate`).
   - Toàn bộ cấu trúc được serialize thành Comment Blocks `<!-- od:... -->` sạch sẽ (không có thẻ `<div>` rác) ghi thẳng vào `values_json["body"]` và xóa cache `editorDraftStore`. Khi quay lại Classic Editor, `<textarea>` hiển thị trực tiếp nội dung `<!-- od:... -->` kèm thanh nhắc nhỏ phía trên + nút `Sửa với UX Builder`, cho phép Editor sửa nhanh câu chữ bên trong `<!-- od:text -->...<!-- /od:text -->` rồi lưu lại và mở UX Builder thấy ngay nội dung mới.
4. **CUJ-4 — Đồng bộ Media/Pattern Refs từ mọi Entry Writer & ACL Draft/Public:**
   - Khi lưu Page ở trạng thái `draft` có `<!-- od:image ... -->` hoặc `<!-- od:pattern_ref ... -->` (qua Mode 2 UX Builder, Classic Editor, **hoặc** API/Plugin `content_save`), server trích ref và cập nhật `entry_builder_media_refs` + `entry_pattern_refs`. Ảnh trong bài `draft` vẫn bị chặn `403/404` trên `/media/:id/file`, bị chặn xóa ở Thư viện Media, và tự động mở công khai khi Entry chuyển sang `published` trên một `content_type` `public = true, active, declared`.

---

## Technical Plan (Architecture)

### 1. Kiến trúc Route & Hai Chế độ Studio (`cms/admin/src/shell/router.ts`, `AdminShell.tsx`)

- Thêm 2 `AdminView` mới trong `cms/admin/src/shell/router.ts`:
  1. `studio`: `#/studio` (query `?panel=root|identity|theme-options|menus|reading|templates&template=<canonical_key>`). Quyền `isViewAllowed` & `isRouteAllowed`: **`admin` only** (`403`/cấm với `editor`).
  2. `studio-entry`: `#/studio/entry/:typeKey/:id`. Quyền `isRouteAllowed`: `admin` và `editor`, với điều kiện `typeKey !== 'pattern' && typeKey !== 'section'`, khớp regex `^[a-z][a-z0-9_]{0,63}$`, `id` là số nguyên dương an toàn (server + page config kiểm tra `ContentSupport::Builder` + quyền sửa trường `body`).
- **Redirect tương thích ngược từ `#/builder`:**
  - `#/builder?template=entry&type=<ty>&entry=<id>` hoặc `#/builder?template=page&type=page&entry=<id>` → redirect về `#/studio/entry/<ty>/<id>`.
  - `#/builder?template=single&type=<ty>&entry=<id>` (`validAdminScopedSingle`) → redirect về `#/studio?panel=templates&template=single:type:<ty>:id:<id>`.
  - `#/builder?template=<role>&type=<ty>` hoặc `#/builder` (trống) → map `<role>` sang `TemplateCandidate::canonical_key()` (`header` → `part:header`, `footer` → `part:footer`, `taxonomy_archive` → `taxonomy`, `archive` → `archive` / `archive:type:<ty>`, `single` → `single` / `single:type:<ty>`) và mở `#/studio?panel=templates&template=...`.
- **Ẩn Chrome Admin (`cms/admin/src/shell/AdminShell.tsx`):** Khi `route.view === 'studio' || route.view === 'studio-entry'`, `AdminShell` ẩn `<aside id="admin-navigation">` và `<header className="od-topbar">`, render full-viewport `StudioWorkspace` ngoài `.od-legacy-scope`. Menu Sidebar mục *Giao diện* cập nhật liên kết chính thành `OpenDash Studio` (`#/studio`).

### 2. Đặc tả Định dạng Comment Block Thuần Rust (`cms/crates/opendash-core/src/builder_blocks.rs`)

Module mới thuần Rust (chỉ dùng `std`, `serde`, `serde_json`, `crate::builder::*`, `crate::id::valid_package_id`; **0** I/O, HTTP, SQL).

#### 2.1. Ánh xạ 1:1 với AST `opendash-core::builder` (`#[serde(deny_unknown_fields)]`)
Mọi comment block bám sát **100%** các trường thực tế của `BuilderDocument` v1 (`SCHEMA_VERSION = 1`), **không** sinh thẻ `<div class="od-section">` dư thừa чтобы nội dung trong `<textarea>` Classic Editor luôn gọn và dễ đọc:

- **3 tầng Container (mở & đóng, chỉ giữ `id` và `responsive` trong JSON):**
  ```html
  <!-- od:section {"id":"sec-1","responsive":...} -->
  <!-- od:row {"id":"row-1","responsive":...} -->
  <!-- od:column {"id":"col-1","responsive":...} -->
  <!-- od:heading {"id":"el-1","level":2,"text":"Giới thiệu"} /-->
  <!-- od:text {"id":"el-2"} -->
  Nội dung văn bản thuần của đoạn...
  <!-- /od:text -->
  <!-- /od:column -->
  <!-- /od:row -->
  <!-- /od:section -->
  ```
- **Khối văn bản (`KnownElement::Text(TextProps { text, binding })`):**
  - Nếu `props.text` **không** chứa chuỗi `<!--`, `-->`, hoặc `--!>`: xuất dạng cặp thẻ `<!-- od:text {"id":"el-1","binding":...} -->\n<TEXT>\n<!-- /od:text -->`. Khi parse chỉ gỡ đúng cặp `\n` delimiter đầu/cuối do serializer chèn, **không** gọi `.trim()` làm mất khoảng trắng/xuống dòng của người dùng; chuẩn hóa `\r\n` và `\r` → `\n`.
  - Nếu `props.text` **có** chứa `<!--`, `-->`, hoặc `--!>`: serializer tự chuyển sang **dạng tự đóng** `<!-- od:text {"id":"el-1","text":"...","binding":...} /-->` (payload nằm trọn trong JSON đã escape), đảm bảo mọi chuỗi UTF-8 đều round-trip lossless 100%.
- **Các Element lá khác (`self-closing /-->` — 1 dòng JSON duy nhất khớp `KnownElement` & `Unresolved`):**
  `od:heading` (`HeadingProps`), `od:image` (`ImageProps`), `od:video` (`VideoProps`), `od:button` (`ButtonProps`), `od:list` (`ListProps`), `od:contact_form` (`ContactFormProps`), `od:content_slot` (`ContentSlotProps`), `od:pattern_ref` (`PatternRefProps`), `od:template_part` (`TemplatePartProps`), `od:plugin_block` (`PluginBlockProps`), `od:legacy` (`LegacyBlockProps` cho `"legacy/block"`), và `od:unresolved` (`BTreeMap<String, Value>` cho `ElementContent::Unresolved`).

#### 2.2. Chống Phá vỡ Comment HTML & Giao thức Mô hình Văn bản An toàn
- **Escape JSON payload:** `encode_comment_json` serialize qua `serde_json::to_string`, sau đó thay thế mọi `--` → `\u002d\u002d` (lặp cho tới khi hết `--` để xử lý chuỗi `---`), `<` → `\u003c`, `>` → `\u003e`, đảm bảo chuỗi JSON kết quả không bao giờ chứa `<!--`, `-->`, `--!>` hay `--`.
- **Mô hình HTML escaping thống nhất:** `KnownElement::Text(TextProps { text })` tiếp tục lưu **văn bản thuần** và được `page.rs:348-350` escape qua `escape_html(&props.text)`, đồng bộ 100% với `content_render.rs:2` (*"Rich-text stays untrusted text and is HTML-escaped"*).

#### 2.3. Giao diện Hàm & Xử lý Auto-Wrap / Recovery
- Hằng số: `pub const MAX_BUILDER_BLOCK_BODY_BYTES: usize = 384 * 1024;` (`393_216` bytes).
- Các hàm công khai:
  1. `pub fn contains_blocks(raw_body: &str) -> bool`
  2. `pub fn extract_visible_text(raw_body: &str) -> String`: trích nội dung đọc được từ `od:text`, `od:heading`, `od:button` (phục vụ `content_render.rs` cho `entry.values.body`/`entry.fields`, `storage/builder_binding.rs`, và `reset_entry_builder`).
  3. `pub fn serialize_builder_document_to_blocks(doc: &BuilderDocument) -> Result<String, BuilderBlockError>`
  4. `pub fn parse_blocks_to_builder_document(raw_body: &str) -> Result<ParsedBlockDocument, BuilderBlockError>` & `pub fn parse_blocks_lenient_or_recover(raw_body: &str) -> ParsedBlockDocument`:
     - **Auto-wrap freeform text:** Khi `raw_body` không có block `<!-- od:` (hoặc có văn bản nằm ngoài `<!-- od:section -->`), mọi đoạn freeform không rỗng được bọc thành `SectionNode → RowNode → ColumnNode → Element(Text)` với ID xác định theo chỉ số (`sec-wrap-1`, `row-wrap-1`, `col-wrap-1`, `el-wrap-1-1`, thỏa `valid_package_id`). Nếu đoạn text vượt quá **`16_384` Unicode scalars** (`builder.rs:569`), chia deterministically theo ranh giới dòng/ký tự thành nhiều `KnownElement::Text` liên tiếp (`<= 16_384` chars/node).
     - **Recovery khi comment bị hỏng (`parse_blocks_lenient_or_recover`):** Đoạn text vùng comment lỗi được bọc thành `KnownElement::Text` (`el-rec-1`, ..., đã lọc control chars & chia chunk `<= 16_384` chars) và ghi nhận `BlockDiagnostic { node_id, code: "malformed_comment_recovered" }`, `had_recovery = true`.
  5. `pub fn extract_block_refs_lenient(raw_body: &str) -> ExtractedBlockRefs`: quét các comment header `od:image`, `od:video`, `od:pattern_ref` hợp lệ để đồng bộ media/pattern pins ngay cả khi sửa từ Classic Editor.

### 3. Hợp nhất Lưu trữ Entry (`content.rs`, `storage/`, `public_content.rs`)

#### 3.1. Ngân sách Kích thước Đồng bộ & Tự động Cấp `body` cho CPT (`opendash-core/src/content.rs`, `routes/entries.rs`)
- Trong `cms/crates/opendash-core/src/content.rs:1617` (`valid_field_value` cho `ContentFieldType::RichText`):
  - Giữ giới hạn `200_000` Unicode scalars đối với văn bản thuần **không** chứa `<!-- od:`.
  - Khi `crate::builder_blocks::contains_blocks(s)` trả `true`, cho phép `s.len() <= MAX_BUILDER_BLOCK_BODY_BYTES` (`384 * 1024` bytes) và `s.chars().count() <= 393_216`.
- Trong `cms/crates/opendash-server/src/routes/entries.rs:28`: nâng `MAX_CONTENT_ENTRY_JSON_BYTES` từ `512 * 1024` lên `768 * 1024` bytes.
- **Hỗ trợ `"body"` cho CPT bật `ContentSupport::Builder` mà không phá vỡ `storage/entries.rs:36-40`:**
  1. Trong `EffectiveContentSchema::assemble` (`content.rs:1049-1073`): khi `!is_builtin && key != "pattern" && key != "section" && settings.supports(ContentSupport::Builder)`, **chỉ khi `!effective_fields.contains_key("body")`** mới tự động chèn `FieldDefinition { key: "body", label: "Nội dung", field_type: RichText, required: false, public_exposure: PublicExposure::Public, min_view_role: FieldRole::Editor, min_edit_role: FieldRole::Editor }`. (Nếu một CPT đã tự định nghĩa custom field `"body"` kiểu khác `RichText`, chỉ từ chối khi bật `ContentSupport::Builder` trên CPT đó với `422` để bảo toàn quy tắc `entries.rs:36-40` cho mọi CPT không dùng Builder).
  2. Cập nhật `eligible_base_field_keys` (`content.rs:2001`), `build_core_and_group_panels` (`content.rs:2053`), và `SingleEditWorkspace.tsx::supportAllowsField` (`L70-85`): tách `"body"` sang panel `core:body_editor` và cho phép hiển thị/sửa `"body"` khi loại nội dung bật `ContentSupport::Editor` **hoặc** `ContentSupport::Builder` (miễn là `effective_fields["body"].field_type == RichText`).

#### 3.2. Cập nhật Entry Storage & Đồng bộ mọi Điểm Ghi (`storage/`)
- **`cms/crates/opendash-server/src/storage/entries.rs`:**
  - `verify_supported_values` (`L31-52`): cho phép trường `"body"` (`RichText`) khi `settings.supports(ContentSupport::Editor) || settings.supports(ContentSupport::Builder)`.
  - `prepare_composite_entry_save` (`L1406`) & `commit_composite_entry_save` (`L1515-1525`): bỏ kiểm tra `SELECT EXISTS(SELECT 1 FROM entry_builder_documents ...)` và bỏ lỗi cấm `entry_id.is_none()` với `content_mode = "builder"`.
  - Tạo hàm dùng chung **`sync_entry_body_refs_in_tx(&mut tx, entry_id, schema, values)`** và gọi tại **TẤT CẢ** điểm ghi `content_entries` (`commit_composite_entry_save`, `create/update_entry_with_schema_fence`, `create/update_entry_with_fence` dùng bởi `plugin/manager.rs::content_save`, và `storage/import.rs`):
    - Nếu `schema.key != "pattern" && schema.key != "section"` và `"body"` là `RichText`: gọi `extract_block_refs_lenient(&body)`, validate các `media_id` và `pattern_id` tồn tại (`422` nếu trỏ pattern/media không tồn tại hoặc tự tham chiếu), và gọi `builder::replace_entry_builder_refs` khi `"body"` có `public_exposure == PublicExposure::Public && min_view_role == FieldRole::Editor` (nếu `"body"` là `Private`/`Admin`-only thì xóa refs công khai ở `entry_builder_media_refs` để không lộ media qua ACL public).
- **`cms/crates/opendash-server/src/storage/builder.rs`:**
  - `type_key == "pattern"`: giữ nguyên đọc/ghi từ `entry_builder_documents`.
  - `read_entry_builder`: với entry khác `pattern`, kiểm tra `min_edit_role` của `"body"` (`403` nếu `Admin`-only mà actor là `Editor`), đọc `values_json["body"]`, parse qua `parse_blocks_lenient_or_recover`. Trả kèm `diagnostics`, `had_freeform_wrap`, `had_recovery`, `publication_status`, `publication_policy`.
  - `mutate_entry_builder` / `save_entry_builder`: kiểm tra `min_edit_role` trên `"body"`, CAS qua `content_entries.version` (`If-Match: "v<N>"` → `412`), pre-commit `render_entry_builder_preview`, serialize `document` → `body_blocks`, gán `candidate_values["body"] = Value::String(body_blocks.clone())` trước `ContentBeforeSave`, ghi `values_json["body"] = body_blocks` + đồng bộ `entry_builder_media_refs` & `entry_pattern_refs` trong `entries::begin_entry_write`, giữ nguyên `publication_status`.
  - `reset_entry_builder` (`confirm == true`): với non-pattern entry, ghi `extract_visible_text(&body)` vào `values_json["body"]`, xóa `entry_builder_media_refs`/`entry_pattern_refs` của entry và tăng `version`.
  - **Dọn dẹp demo startup (`migrate_demo_entry_builder_docs_on_startup`):** Gọi trong `main.rs` dưới khóa `SiteCoordinator` (không tạo bảng SQL mới theo `Spec 2 D02`): chuyển mọi dòng non-pattern còn trong `entry_builder_documents` sang `values_json["body"]`, đồng bộ refs và xóa dòng non-pattern đó khỏi `entry_builder_documents`.
- **`cms/crates/opendash-server/src/storage/media_refs.rs`:**
  - `collect_pins_in_tx` (`L199-219`): deduplicate theo `entry_id` giữa `content_entries.values_json` (`source_kind: "content_entry"`) và `entry_builder_media_refs`.

#### 3.3. Public Rendering & Media ACL (`public_content.rs`, `content_render.rs`, `overrides.rs`, `builder_binding.rs`, `routes/entries.rs`)
- **`cms/crates/opendash-server/src/storage/public_content.rs`:**
  - Render single/page: bất kể `entry.content_mode`, khi entry bật `ContentSupport::Builder` và `values_json["body"]` chứa `<!-- od:` (`contains_blocks(&raw_body)`), parse & render qua `page::render_builder_with_patterns`. Nếu không chứa `<!-- od:`, render qua `content_render::render_single_with_site_and_archive` giữ nguyên escaped text.
  - Sửa **cả 2 nhánh** trong `media_is_referenced` (`L621-646` — nhánh trực tiếp `entry_builder_media_refs` và nhánh 1-hop `entry_pattern_refs`): bỏ `JOIN entry_builder_documents` và `e.content_mode = 'builder'` trên entry cha `e`, **giữ nguyên** toàn bộ điều kiện `e.content_type_key NOT IN ('pattern', 'section') AND e.publication_status = 'published' AND t.provider_status = 'active' AND t.declaration_state = 'declared' AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1 AND EXISTS (SELECT 1 FROM json_each(t.definition_json, '$.settings.supports') WHERE value = 'builder')`.
- **`content_render.rs`, `storage/builder_binding.rs`, `storage/overrides.rs`, `routes/entries.rs`:**
  - `content_render::entry_view_with_url` & `builder_binding.rs`: dùng `extract_visible_text` khi đọc trường `"body"` chứa `<!-- od:` để theme custom hoặc element binding không in ra raw comment JSON.
  - `builder::preview_saved_page_body_in_tx` (gọi bởi `overrides::preview_typed_override` ở `L1438`) và `routes/entries.rs::entry_preview` (`L547-578`): đọc và render unified `values_json["body"]` không phụ thuộc `content_mode`.

---

### 4. Kiến trúc Studio Preview, Mode 1 & Mode 2 (`routes/themes.rs`, `overrides.rs`, `page.rs`, `cms/admin/src/studio/`)

#### 4.1. Hợp đồng API Studio (`cms/crates/opendash-server/src/routes/themes.rs`, `routes/builder.rs`, `app.rs`)

1. **`GET /api/admin/studio/targets` (`Admin` only; `Editor` → `403`, handler trong `routes/themes.rs`)**
   - Kết hợp `storage::theme::load_active_theme_v3`, danh sách `content_types` và `storage::overrides::list_overrides` để trả danh mục `TemplateCandidate` khả dụng (`part:header`, `part:footer`, `page`, `single`, `archive`, `single:type:<ty>`, `archive:type:<ty>`, `page_layout:<key>`) kèm `override_id`, `version`, `effective_source`, `requires_content_slot` (`slots == 1`), `forbids_content_slot` (`slots == 0`).

2. **`POST /api/admin/studio/preview` (`Admin` only; `Editor` → `403`, read-only **không** dùng `AdmittedMutation`, `DefaultBodyLimit::max(512 * 1024)`, handler trong `routes/themes.rs`)**
   - Mở rộng trực tiếp trên pipeline `storage::overrides::preview_typed_override` hiện có (không tự viết lại bộ định tuyến public C-SEC):
     ```rust
     #[derive(Deserialize)]
     #[serde(deny_unknown_fields)]
     pub struct StudioPreviewRequest {
         pub target: TemplateCandidate,
         #[serde(default)]
         pub document: Option<BuilderDocument>, // Nếu None: dùng override đang lưu hoặc C-SEC template của active theme
         #[serde(default)]
         pub preview_entry: Option<overrides::TemplatePreviewEntry>,
         #[serde(default)]
         pub preview_annotations: bool,
         #[serde(default)]
         pub transient: Option<TransientStudioOverrides>,
     }

     #[derive(Deserialize)]
     #[serde(deny_unknown_fields)]
     pub struct TransientStudioOverrides {
         #[serde(default)]
         pub general: Option<GeneralSettingsInput>,
         #[serde(default)]
         pub theme_options: Option<BTreeMap<String, Value>>,
         #[serde(default)]
         pub menu_slots: Option<BTreeMap<String, Option<i64>>>,
     }
     ```
   - Trong `overrides.rs`: nạp `site`, `options`, `menus` in-memory; nếu có `transient`, validate qua domain validators (trả `422` nếu lỗi) và ghi đè lên `theme_context` (`{ site, options, theme_options, menus }`) — riêng các option kiểu `ThemeOptionType::ImageRef` trong preview được map sang `/api/admin/media/{media_id}/file` để ảnh nháp chưa xuất bản vẫn hiển thị được trong `preparePreviewMarkup`!
   - Nếu `target.kind != TemplateCandidateKind::Part`, tự động bọc kết quả với `overrides::resolve_and_render_header_footer` (dùng cùng `site`, `theme_options`, `menus` in-memory và `RenderAnnotationScope::None` cho Header/Footer) để khung Preview luôn hiển thị trang đầy đủ có Header + Body + Footer.

3. **Mở rộng Mode 2 Preview & Open trên `/api/admin/entries/{type_key}/{id}/builder*` (`routes/builder.rs`, `storage/builder.rs`):**
   - `GET/PUT /api/admin/entries/{type_key}/{id}/builder`: giữ nguyên `ETag: "v<N>"` / `If-Match: "v<N>"`, bổ sung các trường additive vào `EntryBuilderOpen`: `publication_status`, `publication_policy`, `diagnostics`, `had_freeform_wrap`, `had_recovery`.
   - `POST /api/admin/entries/{type_key}/{id}/builder/preview` (`Admin` & `Editor`): render `document` của Entry với `RenderAnnotationScope::EntryBodyOnly` (truyền đúng `actor_field_role(actor)` — không bao giờ dùng `FieldRole::Admin` cho `Editor`!), sau đó bọc vào khung template `Page`/`Single` đang kích hoạt + `Header`/`Footer` (`RenderAnnotationScope::None`) để Mode 2 luôn thấy Header/Footer thật ở chế độ chỉ đọc.

#### 4.2. Preview Node Annotations gọn nhẹ (`cms/crates/opendash-server/src/page.rs`)
- Thêm `pub enum RenderAnnotationScope { None, TemplateAll, EntryBodyOnly }`.
- Tái sử dụng `data-node-id="..."` sẵn có trong `page.rs` và chỉ gắn thêm `data-od-editable="1" data-od-kind="section|row|column|element"` trên các node thuộc phạm vi đang sửa (`TemplateAll` hoặc `EntryBodyOnly`).
- Ép `RenderAnnotationScope::None` khi render bên trong `PatternRef`, `TemplatePart`, và `ContentSlot` con, cũng như trên toàn bộ Public routes (`routes/public.rs`, `routes/public_content.rs`).

#### 4.3. Interactive Preview Iframe Đơn giản & An toàn (`cms/admin/src/builder/BuilderPreviewFrame.tsx`)
1. Tách phần preview iframe từ `BuilderWorkspace.tsx` thành `cms/admin/src/builder/BuilderPreviewFrame.tsx`.
2. **Cách thức nạp CSS/Media/Font trong Opaque Sandbox (`sandbox="allow-scripts"`, CẤM `allow-same-origin`):**
   - Trong `preparePreviewMarkup(html, signal)`:
     - Giữ nguyên cơ chế đổi `<img src="/api/admin/media/{id}/file">` sang `data:` URL (vì route `/api/admin/media/...` cần session cookie mà opaque iframe không gửi được).
     - Gỡ bỏ mọi thẻ `<script>` có sẵn trong DOM parse từ server (phòng vệ chiều sâu bên cạnh server normalization).
     - Viết lại các đường dẫn tương đối gốc (`/theme/...` và `/media/...` trên `link[href]`, `img[src]`, `video[src]`, `video[poster]`, `source[src]`) thành URL tuyệt đối `${window.location.origin}/theme/...` và `${window.location.origin}/media/...` (các route public `/theme/{digest}/...` và `/media/...` không cần cookie, đồng thời mọi `url(...)` tương đối bên trong file `.css` của theme sẽ tự động resolve chuẩn theo URL của file `.css` đó mà không cần fetch/inline thủ công!).
     - Chèn thẻ `<meta http-equiv="Content-Security-Policy">` cho phép tải style/img/media/font từ `${window.location.origin}` nhưng **khóa chặt `script-src` vào duy nhất hash SHA-256 của bridge script tĩnh**:
       `default-src 'none'; style-src 'unsafe-inline' ${origin}; img-src data: blob: ${origin}; media-src data: blob: ${origin}; font-src data: ${origin}; script-src 'sha256-<STATIC_BRIDGE_HASH>'; object-src 'none'; base-uri 'none'; form-action 'none';`
3. **Static SHA-256 Bridge + Nonce + Scroll Sync qua `postMessage`:**
   - Gắn `<meta name="od-studio-nonce" content="<RANDOM_UUID>">` và `<meta name="od-studio-scroll" content="<x>,<y>">` vào `<head>`, cùng 1 `<style>` highlight (`[data-od-editable="1"][data-od-hover]`, `[data-od-editable="1"][data-od-selected]`) và 1 `<script>` bridge tĩnh (~40 dòng):
     - Đọc `nonce` từ `meta[name="od-studio-nonce"]`; khôi phục `window.scrollTo(x, y)` từ `meta[name="od-studio-scroll"]` khi load.
     - Bắt `mouseover` / `mouseout` / `click` trên `[data-od-editable="1"][data-node-id]`, gọi `e.preventDefault(); e.stopPropagation();` chặn nhảy trang, gửi `parent.postMessage({ type: 'od-studio-pick' | 'od-studio-hover', nonce, nodeId, kind }, '*')` và báo vị trí cuộn `{ type: 'od-studio-scroll', nonce, x, y }`.
     - Nhận lệnh `{ type: 'od-highlight', nonce, selectedId }` từ parent để tô viền và `scrollIntoView({ block: 'nearest' })`.
   - Parent xác thực `event.source === iframeRef.current?.contentWindow`, `payload.nonce === currentNonceRef.current`, và `nodeId` tồn tại trong `BuilderDocument` hiện tại.

#### 4.4. Cấu trúc UI `OpenDash Studio` (`cms/admin/src/studio/`)
- **Mode 1 Customizer (`StudioCustomizerNav.tsx`):**
  - Không sửa hay nhúng phức tạp các trang full-page `GeneralSettingsPanel.tsx`, `ReadingSettingsPanel.tsx`, `MenusPanel.tsx`.
  - Tạo các form drawer `340px` gọn nhẹ ngay trong `StudioCustomizerNav.tsx` cho *Nhận diện Site* (`site_title`, `tagline`, `timezone`), *Trang chủ & Đọc* (`homepage_page_id`, `posts_page_id`), *Vị trí Menu* (dropdown gán menu vào từng slot của theme + liên kết mở `#/appearance/menus` khi cần thêm/sửa cây menu chi tiết), và tái sử dụng `ThemeOptionsForm.tsx` (thêm optional prop `onValuesChange` để đồng bộ live preview).
  - Khi bấm `Lưu thay đổi`, Studio chỉ gọi các endpoint tương ứng với những nhóm có `dirty == true` (`saveGeneralSettings`, `saveReadingSettings`, `saveThemeOptions`, `putMenuSlots`), giữ nguyên draft khi gặp `401`/`412` và báo rõ trạng thái từng mục.
- **Template UX Builder Safeguards (`document.ts`, `BuilderInspector.tsx`):**
  - Mở khóa chỉnh sửa/thêm/xóa `core/template_part` (`part_key`); chỉ khóa **xóa** `core/content_slot` khi `targetCandidate` yêu cầu `slots === 1` (`Home`, `Page`, `PageLayout`, `Single`, `SingleCustom`, `Singular` theo `overrides.rs:780-794`).
  - Khi target chưa có row trong `template_overrides`, hiển thị thông báo đang dùng template mặc định của theme + nút *"Khởi tạo bản dựng UX Builder"*.
- **Mode 2 UI (`#/studio/entry/:typeKey/:id`) & Classic Editor (`SingleEditWorkspace.tsx`, `PanelRegionRenderer.tsx`, `EditorDraftStore.ts`):**
  - TopBar Mode 2 hiển thị nút theo đúng `publication_policy` (`immediate` → `Lưu & Cập nhật`; `draft_publish` đang `draft` → `Lưu bản nháp` + `Xuất bản`; đang `published` → `Cập nhật đã xuất bản`). Undo/Redo giới hạn 50 bước trên `BuilderDocument` (bỏ qua khi đang focus trong `<input>`/`<textarea>`/`<select>`).
  - Trong `SingleEditWorkspace.tsx`: gỡ bỏ `<select Nguồn nội dung>` cũ; cập nhật `supportAllowsField` cho phép `"body"` khi `supports` có `'editor'` hoặc `'builder'`; nhắc lưu nếu `dirty === true` trước khi chuyển sang `#/studio/entry/...`; gọi `editorDraftStore.clearDraft` sau khi Mode 2 lưu thành công.
  - Trong `PanelRegionRenderer.tsx`: giữ `<textarea>` của `"body"` **luôn hiển thị và sửa trực tiếp được**, kèm thanh công cụ nhỏ phía trên có nút `Sửa với UX Builder` (và badge thông báo khi phát hiện `<!-- od:... -->`).

---

## Alternatives Considered

| Phương án | Đánh giá | Quyết định |
| :--- | :--- | :--- |
| **A. Giữ 2 kho lưu tách biệt (`values_json["body"]` vs `entry_builder_documents`)** | Trái với yêu cầu trực tiếp của Chủ dự án: Editor và UX Builder phải cùng đọc/lưu trên một nội dung duy nhất của bài viết. | **Loại bỏ** (trừ `pattern`). |
| **B. Comment Block `<!-- od:... -->` 1:1 AST `BuilderDocument` (không chèn `<div>` thừa, giữ `<textarea>` mở)** *(Chọn)* | Khớp 1:1 `BuilderDocument` (`#[serde(deny_unknown_fields)]`), không mở lỗ hổng XSS raw HTML, đọc/sửa văn bản trực tiếp ngay trong `<textarea>` Classic Editor. | **Chấp nhận**. |
| **C. Tạo bảng `0045` `legacy_dual_content_archive` và endpoint `PUT /api/admin/studio/customizer`** | Phản biện `egm-zoom-out` chỉ ra `Spec 2 D02` đã chốt không cần tương thích dữ liệu lịch sử cũ và việc gom 4 domain vào 1 endpoint + sửa 4 trang Admin full-page là over-engineered. | **Loại bỏ** — giữ kiến trúc gọn nhẹ. |
| **D. Fetch & inline thủ công toàn bộ CSS/video/font bằng client `DOMParser` allowlist** | Dễ làm vỡ `url(...)` tương đối trong CSS, `.woff2` font và `<fieldset disabled>`. Viết lại URL `/theme/...` & `/media/...` sang `${window.location.origin}/...` kết hợp CSP `script-src 'sha256-...'` đơn giản và bền vững hơn nhiều. | **Loại bỏ** — dùng URL origin rewrite + SHA-256 CSP. |

---

## Non-Functional Requirements

- **Hiệu năng Parse/Serialize:** Parser `builder_blocks.rs` chạy O(N) single-pass trên ngưỡng `<= MAX_BUILDER_BLOCK_BODY_BYTES` (`384 KiB`), không backtracking regex, đảm bảo < 2ms.
- **Bảo mật & Cách ly (Security & Isolation):**
  - `opendash-core::builder_blocks` chứa **0** câu lệnh I/O, HTTP hay SQL.
  - Public routes (`routes/public.rs`, `routes/public_content.rs`) luôn chạy với `RenderAnnotationScope::None` (giữ `script-src 'none'`, **0** thẻ `data-od-*`).
  - `POST /api/admin/studio/preview` là đọc-chỉ (**không** dùng `AdmittedMutation`, giới hạn body `512 KiB`).
  - Các mutation thật (`save_entry_builder`, domain settings/menu/theme-options saves) đi qua `AdmittedMutation` + CSRF + `If-Match` CAS.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Hành động | Slice | Mục tiêu |
| :--- | :--- | :--- | :--- |
| `cms/crates/opendash-core/src/builder_blocks.rs` | Tạo mới | S01 | Parser/serializer/extractor/recovery Comment Block `<!-- od:... -->` thuần Rust + unit tests. |
| `cms/crates/opendash-core/src/lib.rs` | Chỉnh sửa | S01 | Export module `builder_blocks`. |
| `cms/crates/opendash-core/src/content.rs` | Chỉnh sửa | S01 | Auto-inject `"body"` cho CPT `Builder` (`EffectiveContentSchema::assemble`), `eligible_base_field_keys`, panel `core:body_editor`, giới hạn `RichText` block `384 KiB`. |
| `cms/crates/opendash-server/src/main.rs` | Chỉnh sửa | S01 | Gọi `storage::builder::migrate_demo_entry_builder_docs_on_startup(&db)` sau `init_db` dưới khóa `SiteCoordinator`. |
| `cms/crates/opendash-server/src/storage/builder.rs` | Chỉnh sửa | S01, S02 | Đọc/lưu/preview/reset non-pattern entry trên `values_json["body"]`, check `min_edit_role`, CAS `version`, dọn demo rows lúc boot, và bọc Header/Footer + `RenderAnnotationScope` cho preview. |
| `cms/crates/opendash-server/src/storage/entries.rs` | Chỉnh sửa | S01 | Mở `"body"` trong `verify_supported_values`, gỡ guard `entry_builder_documents`, gọi `sync_entry_body_refs_in_tx` trên mọi create/update/composite writers. |
| `cms/crates/opendash-server/src/storage/import.rs` | Chỉnh sửa | S01 | Gọi `sync_entry_body_refs_in_tx` khi import entries chứa `<!-- od:... -->`. |
| `cms/crates/opendash-server/src/storage/media_refs.rs` | Chỉnh sửa | S01 | Deduplicate `content_entries` vs `entry_builder_media_refs` sau hợp nhất. |
| `cms/crates/opendash-server/src/storage/public_content.rs` | Chỉnh sửa | S01 | Render unified `values_json["body"]` không phụ thuộc `content_mode`; sửa cả 2 nhánh `media_is_referenced`. |
| `cms/crates/opendash-server/src/content_render.rs` | Chỉnh sửa | S01 | Dùng `extract_visible_text` cho `FieldView`/`values` của `body` khi chứa `<!-- od:`. |
| `cms/crates/opendash-server/src/storage/builder_binding.rs` | Chỉnh sửa | S01 | Trích `extract_visible_text` nếu element bind `Text` vào field `RichText` có comment block. |
| `cms/crates/opendash-server/src/routes/entries.rs` | Chỉnh sửa | S01 | Nâng `MAX_CONTENT_ENTRY_JSON_BYTES` (`768 * 1024`), hỗ trợ CPT `Builder` trong `entry_preview`. |
| `cms/crates/opendash-server/src/page.rs` | Chỉnh sửa | S02 | Thêm `RenderAnnotationScope` (`None` / `TemplateAll` / `EntryBodyOnly`), xuất `data-od-editable="1" data-od-kind` trong Admin preview. |
| `cms/crates/opendash-server/src/storage/overrides.rs` | Chỉnh sửa | S02 | Đọc unified `body`, mở rộng `preview_typed_override` nhận `TransientStudioOverrides` + `RenderAnnotationScope` + bọc Header/Footer. |
| `cms/crates/opendash-server/src/routes/themes.rs` | Chỉnh sửa | S02 | Thêm handler `studio_targets` (`GET /api/admin/studio/targets`) và `studio_preview` (`POST /api/admin/studio/preview`). |
| `cms/crates/opendash-server/src/routes/builder.rs` | Chỉnh sửa | S02 | Bổ sung diagnostics & publication metadata trên `EntryBuilderOpen` (`/api/admin/entries/{type_key}/{id}/builder`). |
| `cms/crates/opendash-server/src/app.rs` | Chỉnh sửa | S02 | Đăng ký `/api/admin/studio/targets` và `/api/admin/studio/preview` + `DefaultBodyLimit`. |
| `cms/admin/src/api/builder.ts` | Chỉnh sửa | S03 | Cập nhật `EntryBuilderOpen` diagnostics & policy flags. |
| `cms/admin/src/api/studio.ts` | Tạo mới | S03 | Client API typed cho `getStudioTargets` và `postStudioPreview`. |
| `cms/admin/src/shell/router.ts` | Chỉnh sửa | S03 | Thêm `studio`, `studio-entry`, RBAC `admin` vs `editor`, redirect `#/builder` đúng cả `validAdminScopedSingle`. |
| `cms/admin/src/shell/AdminShell.tsx` | Chỉnh sửa | S03 | Ẩn Sidebar/Topbar Admin khi active view là Studio; gắn link menu `OpenDash Studio`. |
| `cms/admin/src/builder/document.ts` | Chỉnh sửa | S03 | Thêm `core/content_slot` & `core/template_part` helpers (`canDeleteElement` theo `TemplateCandidateKind`). |
| `cms/admin/src/builder/BuilderInspector.tsx` | Chỉnh sửa | S03 | Cho chỉnh `part_key` của `core/template_part`, khóa xóa `core/content_slot` khi bắt buộc 1 slot. |
| `cms/admin/src/builder/BuilderPreviewFrame.tsx` | Tạo mới | S03 (shell), S04 (interactive) | Wrapper `preparePreviewMarkup` & `<iframe>` tách riêng khỏi `BuilderWorkspace.tsx`; S04 nâng cấp sang interactive `sandbox="allow-scripts"` + SHA-256 CSP bridge. |
| `cms/admin/src/builder/BuilderWorkspace.tsx` | Chỉnh sửa | S03 | Trích `BuilderEditorCore` dùng lại trong Studio, render `BuilderPreviewFrame` và diagnostics banner. |
| `cms/admin/src/studio/StudioWorkspace.tsx` | Tạo mới | S03 | Khung Studio toàn màn hình Mode 1 & Mode 2, `401`/`412` draft recovery, Publish orchestration Mode 2. |
| `cms/admin/src/studio/StudioTopBar.tsx` | Tạo mới | S03 | TopBar `48px`, Viewport switcher, Undo/Redo stack (`<= 50`), nút Lưu/Xuất bản theo policy. |
| `cms/admin/src/studio/StudioCustomizerNav.tsx` | Tạo mới | S03 | Drill-down menu `340px` với các form drawer gọn cho Site Identity, Reading, Menu Slots, `ThemeOptionsForm` và Template List. |
| `cms/admin/src/features/themes/ThemeOptionsForm.tsx` | Chỉnh sửa | S03 | Thêm optional `onValuesChange` / `hideSubmitButton` prop khi nhúng trong `StudioCustomizerNav`. |
| `cms/admin/src/features/editor/SingleEditWorkspace.tsx` | Chỉnh sửa | S03 | Gỡ select `Nguồn nội dung`, mở `supportAllowsField` cho `builder`, đồng bộ `editorDraftStore` + dialog nhắc lưu trước khi mở Mode 2. |
| `cms/admin/src/features/editor/PanelRegionRenderer.tsx` | Chỉnh sửa | S03 | Giữ `<textarea>` của `"body"` luôn mở, thêm thanh công cụ `Sửa với UX Builder` + badge `<!-- od:... -->`. |
| `cms/admin/src/features/editor/EditorDraftStore.ts` | Chỉnh sửa | S03 | Helper `clearDraft` cho Mode 2 save. |
| `cms/crates/opendash-server/tests/studio_builder_test.rs` | Tạo mới | S04 | Integration tests XSS, preview CSP, `Editor` `403`, round-trip Classic ↔ UX Builder, ACL media 1-hop pattern. |

---

### Slice S01: Pure-Rust Comment Block Codec & Unified Entry `body` Storage

- **Dependencies:** Baseline `main@e57ad90`.
- **File Ownership:**
  - `cms/crates/opendash-core/src/builder_blocks.rs` (new)
  - `cms/crates/opendash-core/src/lib.rs`
  - `cms/crates/opendash-core/src/content.rs`
  - `cms/crates/opendash-server/src/main.rs`
  - `cms/crates/opendash-server/src/storage/builder.rs`
  - `cms/crates/opendash-server/src/storage/entries.rs`
  - `cms/crates/opendash-server/src/storage/import.rs`
  - `cms/crates/opendash-server/src/storage/media_refs.rs`
  - `cms/crates/opendash-server/src/storage/public_content.rs`
  - `cms/crates/opendash-server/src/storage/builder_binding.rs`
  - `cms/crates/opendash-server/src/content_render.rs`
  - `cms/crates/opendash-server/src/routes/entries.rs`
- **Implementation Details:**
  1. Xây `builder_blocks.rs` với grammar 1:1 `BuilderDocument` (không chèn thẻ `<div>` bao quanh; `SectionNode`/`RowNode`/`ColumnNode` chỉ có `{id, responsive}`; `KnownElement::Text` lưu text thuần, fallback tự đóng khi chứa `<!--`/`-->`/`--!>`), escaping JSON (`--` → `\u002d\u002d`, `<` → `\u003c`, `>` → `\u003e`), auto-wrap freeform chunking `<= 16_384` chars, recovery qua `KnownElement::Text`, và `extract_visible_text` / `extract_block_refs_lenient`.
  2. Trong `content.rs`: nâng cap `RichText` khi `contains_blocks(s)` tới `MAX_BUILDER_BLOCK_BODY_BYTES` (`384 KiB`); auto-inject `"body"` (`RichText`) trong `EffectiveContentSchema::assemble` cho CPT khác `pattern`/`section` bật `ContentSupport::Builder` khi chưa có field `"body"`; cập nhật `eligible_base_field_keys` và `build_core_and_group_panels` để tách `"body"` sang `core:body_editor` cho cả `Editor || Builder`.
  3. Trong `storage/` & `routes/entries.rs`:
     - Nâng `MAX_CONTENT_ENTRY_JSON_BYTES = 768 * 1024`; mở `"body"` ở `verify_supported_values`; gỡ check `entry_builder_documents` trong `prepare_composite_entry_save` / `commit_composite_entry_save`.
     - Chạy `sync_entry_body_refs_in_tx` trong tất cả writers (`composite`, `*_with_schema_fence`, `*_with_fence`, `import.rs`, `save_entry_builder`), chỉ cấp pin public khi `"body"` có `PublicExposure::Public` + `min_view_role == Editor`.
     - Trong `storage/builder.rs`: check `min_edit_role` trên `"body"`, dùng CAS `content_entries.version`, truyền `candidate_values["body"]` vào `ContentBeforeSave`, ghi trực tiếp `values_json["body"]` giữ nguyên `publication_status`; implement `reset_entry_builder` thành flatten plain text qua `extract_visible_text`; thêm `migrate_demo_entry_builder_docs_on_startup` gọi từ `main.rs`.
     - Trong `storage/public_content.rs`: render thống nhất theo `values_json["body"]`, cập nhật cả 2 nhánh (`entry_builder_media_refs` trực tiếp và `entry_pattern_refs` 1-hop) của `media_is_referenced` giữ nguyên filter `active`/`declared`/`public = 1`.
     - Trong `content_render.rs` & `builder_binding.rs`: dùng `extract_visible_text` khi đọc `body` có blocks; hỗ trợ CPT `Builder` trong `routes/entries.rs::entry_preview`.
- **Acceptance Criteria:**
  - Unit tests `opendash-core::builder_blocks`: mọi `KnownElement`, `--` / `-->` / `--!>` injection, text >16 KiB CRLF auto-wrap thành nhiều `Text` nodes, exact newline strip quanh `od:text`, recovery comment hỏng thành `Text` không mất chữ.
  - Integration tests `opendash-server`: dọn `entry_builder_documents` demo rows lúc boot, `plugin` `content_save` đồng bộ `entry_builder_media_refs`, 1-hop Pattern ACL ẩn khi parent `draft` / private CPT và mở khi parent `published`, `read_entry_builder`/`save_entry_builder` từ chối `Editor` (`403`) nếu `"body"` đặt `min_edit_role = Admin`, và round-trip Classic ↔ Builder giữ nguyên 100% `values_json["body"]`.
  - `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` đạt 100% pass.
- **Rollback:** Revert PR S01 trước khi triển khai S02–S04.

---

### Slice S02: Preview Annotations, Studio Targets & Full-Page Studio Preview API

- **Dependencies:** Slice S01 merged on `main`.
- **File Ownership:**
  - `cms/crates/opendash-server/src/page.rs`
  - `cms/crates/opendash-server/src/storage/builder.rs` (`RenderAnnotationScope` + full-page Header/Footer wrapper in `preview_entry_builder`)
  - `cms/crates/opendash-server/src/storage/overrides.rs`
  - `cms/crates/opendash-server/src/routes/themes.rs`
  - `cms/crates/opendash-server/src/routes/builder.rs`
  - `cms/crates/opendash-server/src/app.rs`
- **Implementation Details:**
  1. `page.rs` & `overrides.rs`: thêm `RenderAnnotationScope { None, TemplateAll, EntryBodyOnly }`, chỉ gắn `data-od-editable="1" data-od-kind="..."` cạnh `data-node-id` sẵn có khi Admin preview trong phạm vi đang sửa; ép `RenderAnnotationScope::None` bên trong `PatternRef`, `TemplatePart`, và `ContentSlot`.
  2. `overrides.rs`: mở rộng `preview_typed_override` dùng unified `values_json["body"]`, nhận `TransientStudioOverrides` (`general`, `theme_options` với `ImageRef` trỏ `/api/admin/media/{id}/file` cho preview, `menu_slots`), và bọc `resolve_and_render_header_footer` khi target không phải `Part`.
  3. `storage/builder.rs` & `routes/builder.rs`: cập nhật `preview_entry_builder` bọc kết quả render `document` (`RenderAnnotationScope::EntryBodyOnly`, đúng `actor_field_role(actor)`) trong template `Single`/`Page` + `Header`/`Footer` (`RenderAnnotationScope::None`); bổ sung `diagnostics`, `had_freeform_wrap`, `had_recovery`, `publication_status`, `publication_policy` vào `EntryBuilderOpen`.
  4. `routes/themes.rs` & `app.rs`: implement & wire `GET /api/admin/studio/targets` (`Admin` only) và `POST /api/admin/studio/preview` (`Admin` only, read-only **no** `AdmittedMutation`, limit `512 KiB`).
- **Acceptance Criteria:**
  - Public routes (`GET /`, `GET /<slug>`) **tuyệt đối không** chứa `data-od-editable` / `data-od-kind` / `<script>`.
  - `GET /api/admin/studio/targets` và `POST /api/admin/studio/preview` trả `403` với `Editor`; `POST /api/admin/studio/preview` hiển thị đúng `transient.general.site_title` và `transient.theme_options` (kể cả ảnh `ImageRef` chưa public) mà không ghi xuống DB.
  - `POST /api/admin/entries/{type_key}/{id}/builder/preview` trả HTML có Header/Footer chỉ đọc và chỉ các node thuộc `body` của Entry có `data-od-editable="1"`.
- **Rollback:** Revert commit/PR S02; không đổi migration SQL nên rollback tức thì.

---

### Slice S03: OpenDash Studio Shell, Mode 1 Customizer + Template Builder & Mode 2 Entry UI

- **Dependencies:** Slice S02 merged on `main`.
- **File Ownership:**
  - `cms/admin/src/api/builder.ts`
  - `cms/admin/src/api/studio.ts` (new)
  - `cms/admin/src/shell/router.ts`
  - `cms/admin/src/shell/AdminShell.tsx`
  - `cms/admin/src/builder/document.ts`
  - `cms/admin/src/builder/BuilderInspector.tsx`
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx` (new — initial static `sandbox=""` wrapper extracted from `BuilderWorkspace.tsx`)
  - `cms/admin/src/builder/BuilderWorkspace.tsx`
  - `cms/admin/src/studio/StudioWorkspace.tsx` (new)
  - `cms/admin/src/studio/StudioTopBar.tsx` (new)
  - `cms/admin/src/studio/StudioCustomizerNav.tsx` (new)
  - `cms/admin/src/features/themes/ThemeOptionsForm.tsx`
  - `cms/admin/src/features/editor/SingleEditWorkspace.tsx`
  - `cms/admin/src/features/editor/PanelRegionRenderer.tsx`
  - `cms/admin/src/features/editor/EditorDraftStore.ts`
- **Implementation Details:**
  1. `router.ts` & `AdminShell.tsx`: thêm `#/studio` (`Admin`), `#/studio/entry/:typeKey/:id` (`Admin`/`Editor`), cập nhật `isRouteAllowed` & redirect `#/builder` (giữ `validAdminScopedSingle` trỏ Mode 1 Template), ẩn Chrome Sidebar/Topbar khi đang ở Studio.
  2. Trích `BuilderPreviewFrame.tsx` (ở S03 giữ `sandbox=""` an toàn hiện hành, làm điểm ghép sạch cho S04 nâng cấp interactive mà không đụng `BuilderWorkspace.tsx` hay `StudioWorkspace.tsx`).
  3. Studio Mode 1 (`StudioWorkspace.tsx`, `StudioTopBar.tsx`, `StudioCustomizerNav.tsx` + `ThemeOptionsForm.tsx`): panel trái `340px` ngoài `.od-legacy-scope` với các drawer gọn cho Site Identity, Reading, Menu Slots, Theme Options và Template List; live preview debounced 250ms via `postStudioPreview`; lưu các domain `dirty` qua API hiện hữu (`saveGeneralSettings`, `saveReadingSettings`, `saveThemeOptions`, `putMenuSlots`); giữ nguyên draft qua `401` re-auth và `412`.
  4. Template UX Builder (`document.ts`, `BuilderInspector.tsx`): cập nhật `canDeleteElement` khóa xóa `core/content_slot` khi `requires_content_slot`, mở chỉnh `core/template_part` `part_key`, hiện cảnh báo trước khi tạo mới override đè C-SEC theme template.
  5. Mode 2 (`StudioWorkspace.tsx`) & Classic Editor (`SingleEditWorkspace.tsx`, `PanelRegionRenderer.tsx`, `EditorDraftStore.ts`): nút Save/Publish khớp `DraftPublish` & `Immediate`, Undo/Redo cap 50 bước cho AST, gỡ `<select Nguồn nội dung>`, mở `supportAllowsField` cho `builder`, giữ `<textarea>` luôn hiển thị kèm thanh nút `Sửa với UX Builder`, nhắc lưu draft trước khi sang `#/studio/entry/...` và `clearDraft` sau khi Studio save.
- **Acceptance Criteria:**
  - `npm --prefix cms/admin run typecheck` và `npm --prefix cms/admin run build` pass 100%.
  - Smoke E2E trên browser thật: `#/studio` live preview transient + lưu từng domain; Mode 1 tạo/sửa Template `Header` & kiểm tra bảo vệ `core/content_slot` trên `Single`; Mode 2 chỉnh Page `draft`/`published` và CPT bật `Builder`, kiểm tra round-trip Classic `<textarea>` ↔ UX Builder ↔ Classic `<textarea>` đồng bộ `values_json["body"]`.
- **Rollback:** Revert PR S03; giữ nguyên API backend S01–S02 không ảnh hưởng vận hành hiện có.

---

### Slice S04: Interactive XSS-Safe Canvas Bridge & End-to-End Verification

- **Dependencies:** Slice S03 merged on `main`.
- **File Ownership:**
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx`
  - `cms/crates/opendash-server/tests/studio_builder_test.rs` (new)
- **Implementation Details:**
  1. Nâng cấp độc lập bên trong **`BuilderPreviewFrame.tsx`** (không xung đột file với S03):
     - Trong `preparePreviewMarkup`: giữ nguyên chuyển `<img src="/api/admin/media/...">` sang `data:` URL; xóa mọi thẻ `<script>` khỏi DOM; viết lại các URL tương đối gốc `/theme/...` và `/media/...` thành `${window.location.origin}/theme/...` và `${window.location.origin}/media/...`.
     - Đổi iframe sang `sandbox="allow-scripts"` (tuyệt đối không `allow-same-origin`) với CSP `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline' ${origin}; img-src data: blob: ${origin}; media-src data: blob: ${origin}; font-src data: ${origin}; script-src 'sha256-<EXACT_HASH>'; object-src 'none'; base-uri 'none'; form-action 'none';">`.
     - Chèn `<meta name="od-studio-nonce" content="...">`, `<meta name="od-studio-scroll" content="x,y">`, CSS hover/selected highlight, và 1 `<script>` bridge tĩnh match chính xác SHA-256 CSP: gửi `od-studio-hover`, `od-studio-pick`, `od-studio-scroll` kèm `nonce`, chặn điều hướng click link/submit, nhận `od-highlight` tô viền node và cuộn mượt `scrollIntoView` (giữ scroll offset qua mỗi render mà không truy cập DOM cross-origin).
     - Parent kiểm tra `event.source === iframeRef.current?.contentWindow`, khớp `nonce`, và xác thực `nodeId` có trong `BuilderDocument` hiện tại trước khi kích hoạt `onSelectNode`.
  2. Viết `cms/crates/opendash-server/tests/studio_builder_test.rs` bao phủ full matrix: unified `body` round-trip, plugin writer ref sync, 1-hop Pattern ACL, RBAC `Editor` `403`, zero `data-od-editable` trên Public HTML, và payload XSS `<script>`/`<img onerror>`/`<svg onload>`.
- **Acceptance Criteria:**
  - Click trực tiếp phần tử trên Preview chọn đúng node ở Tree & Inspector; ở Mode 2 click vào Header/Footer không chọn được node nào; cuộn Preview giữ nguyên vị trí khi gõ text ở Inspector.
  - Payload XSS `<script>`, `<svg onload>`, `<a href="javascript:...">` bị vô hiệu hoàn toàn bởi server escaping + SHA-256 CSP & `script-src 'none'` công khai.
  - Toàn bộ Verification Plan dưới đây PASS 100% và lập biên bản tại `docs/verification/`.
- **Rollback:** Thêm prop fallback `interactive={false}` trên `BuilderPreviewFrame` hoặc revert PR S04 (đưa iframe về `sandbox=""` tĩnh trong khi S01–S03 tiếp tục vận hành bình thường).

---

### Verification Plan

Sau khi hoàn tất từng Slice (S01–S04) và trước khi chuyển trạng thái Spec sang `Implemented` / `Verified`, bắt buộc chạy đầy đủ:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
git diff --check
```

Và kiểm chứng thực tế trên server `opendash` đang chạy (Windows/macOS):
1. Kiểm tra Mode 1 (`#/studio`): đổi Customizer live preview → Lưu thay đổi; khởi tạo & chỉnh Template `Header` với hover/click-to-select; kiểm tra bảo vệ `core/content_slot` trên `Single`.
2. Kiểm tra Mode 2 (`#/studio/entry/page/<id>` và 1 CPT tự tạo bật `ContentSupport::Builder`): kiểm tra không chỉnh được Header/Footer; vòng tròn Classic Editor `<textarea>` → UX Builder → Classic Editor `<textarea>` → UX Builder bảo toàn 100% `values_json["body"]`, kiểm tra media ACL `draft` vs `published`.
3. Chạy `/egm-drift-report` và `node cms/scripts/codegraph-index-docs.mjs` khi đóng Spec.
