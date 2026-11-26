# Declarative `block.json` Block Engine & OpenDash Studio Schema-Driven Blocks

- **Author:** Nova (Elephant)
- **Status:** Verified
- **Date:** 2026-11-22
- **Baseline:** `main@a5e0d50`
- **Approved By:** Owner
- **Approval Evidence:** Owner chat 2026-11-22 ("duyệt" — Declarative block.json Block Engine v3, PRD §7 repeater 1-level builder carve-out & OpenDash Studio / Trình dựng trang official naming)
- **Goldfish Review:** Round 1 & Round 2 independent reviews (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`) · 2026-11-22 · reviewed `main@a5e0d50` (all Blockers/Majors resolved in v3)
- **Related:** [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md), [`docs/contracts/CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`docs/contracts/PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md), [`docs/designs/opendash-studio-customizer-and-ux-builder.md`](opendash-studio-customizer-and-ux-builder.md)

---

## Overview

OpenDash đã hoàn thiện **OpenDash Studio** (hai chế độ Tùy biến Website / Template ở `#/studio` và Dựng nội dung Entry ở `#/studio/entry/:typeKey/:id`), AST 4 tầng `BuilderDocument` (`Section → Row → Column → Element`) cùng chuẩn lưu Comment Block `<!-- od:... -->` trên `values_json["body"]`. Tuy nhiên cơ chế đăng ký block hiện tại còn ba khoảng trống lớn:

1. **Core blocks bị hardcode 6 lớp:** Để thêm một khối dựng trang mới (`banner`, `card`, `spacer`, `divider`, `accordion`...), hiện phải sửa thủ công cả `opendash-core/src/builder.rs`, `builder_blocks.rs`, `page.rs`, `document.ts`, `BuilderWorkspace.tsx` và `BuilderInspector.tsx`.
2. **Theme/Plugin blocks quá nghèo nàn:** [`Fields`, `BlockDescriptor`, `Block`](../../cms/crates/opendash-core/src/theme/mod.rs) chỉ hỗ trợ `fields: BTreeMap<String, String>` (tối đa 12 chuỗi phẳng `<= 512` ký tự). Dù [`PluginBlockProps`](../../cms/crates/opendash-core/src/builder/mod.rs) đã dùng `values: BTreeMap<String, serde_json::Value>` và serialize bằng thẻ `<!-- od:plugin_block {...} /-->` ([`builder_blocks.rs`](../../cms/crates/opendash-core/src/builder_blocks/mod.rs)), [`load_block_catalog`](../../cms/crates/opendash-server/src/storage/page.rs#L18-L41) vẫn chỉ nạp `theme_blocks.fields_json` kiểu phẳng và [`BuilderInspector.tsx`](../../cms/admin/src/builder/BuilderInspector.tsx) chỉ vẽ `<input type="text">` đơn.
3. **Tên gọi trên UI chưa thống nhất:** Chuỗi `"UX Builder"` vẫn còn xuất hiện ở 5 file Admin UI (cùng 2 thông báo lỗi ở backend) thay vì tên chính thức **OpenDash Studio / Trình dựng trang**.

Đặc tả này thiết kế **Declarative `block.json` Block Engine** cho phép **Core**, **Theme v3** và **Plugin ZIP** đăng ký block chỉ bằng một cặp file `block.json` + `render.html` (C-SEC), dùng chung một đường kiểm chứng kiểu dữ liệu, bảo mật C-SEC, media ACL và giao diện `<BlockSchemaForm>` tự sinh. Đặc tả này **tách sạch phạm vi** với phần permalink (`storage/permalinks.rs`, `seo.rs`, `routes/seo.rs`) mà Orion đang thực hiện.

---

## The Problem

1. **Giới hạn schema của [`BlockDescriptor`](../../cms/crates/opendash-core/src/theme/mod.rs) & xung đột ngữ pháp ID ([`id.rs`](../../cms/crates/opendash-core/src/id.rs#L22-L64)):**
   - `BlockDescriptor.fields` chỉ là `BTreeMap<String, String>`; [`validate_known`](../../cms/crates/opendash-core/src/builder/mod.rs) chỉ kiểm tra provider active chứ chưa xác thực kiểu giá trị hay `DeclarationState::Withdrawn`, và từ chối luôn `Save` nếu provider là `ProviderInactive`.
   - `validate_package_id` cho phép `id` dài tới 64 ký tự và bắt đầu bằng chữ số (`0-9`), đồng thời **chưa dành riêng `"core"` / `"legacy"`**, khiến Theme/Plugin tên `"core"` có thể chiếm namespace của Core; trong khi `valid_namespaced_block_id` giới hạn mỗi phần tối đa 32 ký tự và phải bắt đầu bằng `[a-z]`.
2. **Khoảng trống C-SEC cho CSS scoped, `url`, `richtext` & `repeater` ([`template_sec.rs`](../../cms/crates/opendash-server/src/template_sec.rs) & [`document.rs`](../../cms/crates/opendash-server/src/document.rs)):**
   - `validate_safe_css` (`#L1046-L1048`) cấm `/api/admin/media/...` nên Preview của block có ảnh nền CSS sẽ lỗi; đồng thời `BuilderPreviewFrame.tsx` hiện chỉ inline `<img src>`/`<video poster>` sang `data:` URL chứ chưa chuyển `url("/api/admin/media/...")` bên trong `<style data-od-block-styles>`.
   - `validate_safe_url` (`#L1065-L1087`) từ chối `#`, `#anchor`, `mailto:`, `tel:`, còn `decode_html_entities_for_url` (`#L1055-L1063`) chưa giải mã đầy đủ named/leading-zero/no-semicolon HTML5 character references (`&sol;`, `&#x002f;`, `&#47`), `validate_tag_attributes` chưa kiểm `srcset`/`ping`, và `parse_attributes` (`#L1502`) ghi đè attribute trùng (HTML5 giữ cái đầu tiên).
   - C-SEC cấm `|safe`, còn `Value::from_safe_string` bỏ qua context HTML attribute; `validate_stmt` (`#L1147-L1170`) cho phép `Set`/`SetBlock`/`WithBlock`/`GetItem`, khiến kiểm tra AST đơn thuần chưa có `HtmlScanState` sẽ bị qua mặt bởi variable shadowing hoặc nhánh `if/else`. Ngoài ra, `validate_html_and_css_context` (`#L1309-L1315`), `document::tokenize` (`#L221-L232`) và `HtmlScanState::Comment` (`#L867`) chưa xử lý đồng bộ đóng comment kiểu HTML5 (`--!>`, `<!-->`, `<!--->`), và `FORBIDDEN_TAGS` thiếu RAWTEXT/RCDATA tags (`title`, `xmp`, `noembed`, `noframes`, `plaintext`).
3. **Thiếu trích xuất Media ACL, trích xuất văn bản & tách biệt Delete Pin với Public ACL:**
   - `entry_builder_media_refs`, `template_override_media_refs`, `builder_homepage_media` và `builder_homepage_video_refs` là **bảng cấp quyền tải công khai `/media/{id}/file`** (đọc trong [`public_content.rs`](../../cms/crates/opendash-server/src/storage/public_content.rs#L630-L753)), còn bảo vệ chống xóa Media được xử lý riêng bởi [`collect_pins_in_tx`](../../cms/crates/opendash-server/src/storage/media_refs.rs#L90-L220) quét JSON thô. Hiện [`extract_block_refs_lenient`](../../cms/crates/opendash-core/src/builder_blocks/mod.rs), [`image_ids` / `video_ids` / `poster_image_ids`](../../cms/crates/opendash-server/src/storage/builder.rs#L406-L458), `replace_entry_builder_refs`, `check_preview_images`, `save_and_render`, [`sync_entry_body_refs_in_conn`](../../cms/crates/opendash-server/src/storage/entries.rs#L57-L189) và [`refresh_override_media_refs_in_tx`](../../cms/crates/opendash-server/src/storage/overrides.rs#L929-L956) chưa xử lý `PluginBlock`, đồng thời `extract_visible_text` cũng bỏ qua `PluginBlock`.
4. **Chi phí parse lặp, xung đột `<head>` CSS & giới hạn Fuel/Size ([`page.rs`](../../cms/crates/opendash-server/src/page.rs), [`app.rs`](../../cms/crates/opendash-server/src/app.rs), [`routes/public_content.rs`](../../cms/crates/opendash-server/src/routes/public_content.rs)):**
   - [`render_plugin_block_csec`](../../cms/crates/opendash-server/src/page.rs#L1106-L1119) re-validate AST ở mọi block, cấp mới `10_000` fuel cho từng block và dùng `?` làm cả trang lỗi nếu 1 block fail; đồng thời `load_entry_render_snapshot` render Entry Body trước khi `routes/public_content.rs#L629` tạo `PublicCompositionBudget` cho Template Override & Header/Footer nên chưa dùng chung một ngân sách `10_000` fuel trên toàn request.
   - `compose_document_with_seo` ([`content_render.rs`](../../cms/crates/opendash-server/src/content_render.rs#L272-L351)) gom toàn bộ `<style>` từ Header, Footer, Template Override, Patterns và Entry vào `<head>`: nếu các tree độc lập dùng trùng `node.id` (ví dụ `el-1`), selector không phân tầng sẽ đè nhau.
5. **Đường dẫn gói Theme v3 & Plugin ZIP chưa hỗ trợ `blocks/<slug>/`:**
   - Theme v3 [`validate_theme_v3_files`](../../cms/crates/opendash-server/src/theme.rs#L170-L287) từ chối mọi file không nằm trong tập `declared` dựng từ `theme.json`, và kiểm tra `patterns/*.json` bằng catalog rỗng.
   - Plugin ZIP [`PluginPackage::from_zip`](../../cms/crates/opendash-server/src/plugin/package.rs#L21-L99) dùng `ensure_safe_flat_entry` (cấm thư mục con) và `MAX_PLUGIN_FILES = 10` đếm theo `archive.len()`; `plugin_revisions` chưa lưu `blocks_json` và `storage/plugin.rs` chưa đưa `blocks/` vào digest & durability.

---

## Goals

- **G1 — Chuẩn `block.json` v1 thống nhất Core / Theme / Plugin:** Định nghĩa `BlockSchemaV1` thuần Rust (`cms/crates/opendash-core/src/block_schema.rs`) gồm 12 kiểu field (`text`, `textarea`, `richtext`, `number`, `range`, `boolean`, `select`, `color`, `media`, `url`, `group` UI-only, `repeater` 1 cấp) + `css_var` + `responsive: true` + `show_if` + nâng cấp tương thích ngược `theme_blocks` cũ (`legacy_mode: true`) giữ nguyên toàn bộ caller qua S01 trước khi mở rộng ở S02/S03.
- **G2 — Tái sử dụng AST `KnownElement::PluginBlock` & Bảo toàn dữ liệu:** Không thêm AST variant dư thừa; dùng thẻ `<!-- od:plugin_block {...} /-->`, kiểm đủ cả `ProviderStatus::Active` và `DeclarationState::Declared`, cho phép `ProviderInactive`/`Withdrawn`/`Unresolved` round-trip qua đúng bất biến `is_preserved`, hỗ trợ schema evolution giữ unknown keys và trích xuất văn bản hiển thị.
- **G3 — Bảo mật C-SEC tuyệt đối & Ngân sách Fuel/HTML toàn trang:**
  - `richtext` sanitize bằng zero-dependency AST tokenizer/serializer (`cms/crates/opendash-core/src/richtext.rs`) + tiêm **nonce 128-bit hậu-render** qua một-pass `HtmlScanState::Body` (cấm `OD_RICHTEXT_` literal, cấm `Value::from_safe_string`, chặn alias/shadowing/filter lên biến `richtext` & `url`, chặn duplicate attributes, đóng comment HTML5 đầy đủ trên `template_sec.rs` & `document.rs`).
  - `color`/`range`/`number`/`select`/`media` gắn `css_var` được Rust serialize thành `<style data-od-block-styles>` scoped theo `[data-od-node-scope="<tree_scope>-<node_id>"]` duy nhất xuyên suốt Entry/Override/Header/Footer/Patterns với đúng breakpoint `@media (width < 1024px)` và `@media (width < 768px)` của `builder_responsive_css`.
  - Pre-validate & cache C-SEC trong `AppState::block_template_cache`; chia sẻ chung một `RenderBudget` (`10_000` fuel, trần `512 KiB` HTML) xuyên suốt Entry Body, Patterns, Template Override và Header/Footer, cô lập lỗi render từng block thành placeholder an toàn.
- **G4 — Tách rạch ròi Delete-Pin vs Public Media ACL:** Giữ `media_refs::collect_pins_in_tx` quét JSON thô để bảo vệ xóa `409 Conflict`; chỉ ghi media của block `Active + Declared` vào `entry_builder_media_refs` / `template_override_media_refs` / `builder_homepage_media` / `builder_homepage_video_refs`, tự động resync các hàng chứa `plugin/block` (giữ nguyên toàn bộ guard privacy CPT `supports('builder')` & `body_allows_public_refs`) mỗi khi đổi theme hoặc cài/nâng cấp/bật/tắt plugin, và giữ `404 Not Found` chuẩn trên `/media/{id}/file` cho draft.
- **G5 — 5 Core Blocks nhúng sẵn, `<BlockSchemaForm>` & Tên gọi Chính thức:** Đóng gói 5 core blocks (`core/banner`, `core/card`, `core/spacer`, `core/divider`, `core/accordion` 0-JS), nâng cấp `<BlockSchemaForm>` + `<RichTextFieldControl>` + `<MediaPicker acceptKind="any">` + `BuilderPreviewFrame` CSS data-URI inliner, và đổi toàn bộ nhãn UI + API copy `"UX Builder"` thành **OpenDash Studio / Trình dựng trang**.

---

## Non-Goals

- Không viết lại 7 content core blocks hiện hữu (`core/heading`, `core/text`, `core/image`, `core/video`, `core/button`, `core/list`, `core/contact_form`) hay 3 structural/template blocks (`core/content_slot`, `core/pattern_ref`, `core/template_part`).
- Không chỉnh sửa bất kỳ file nào thuộc phạm vi permalink mà Orion đang thực hiện (`storage/permalinks.rs`, `seo.rs`, `storage/seo.rs`, `routes/seo.rs`).
- Không bật `repeater` cho CPT/User Custom Fields (`ContentFieldType` giữ nguyên 9 kiểu).
- Không cho phép lồng `repeater` trong `repeater`, không nhận raw `<script>`/`<iframe>`/`<style>`, không thêm crate Rust mới vào `cms/Cargo.toml` hay package npm mới vào `cms/admin/package.json`.

---

## PRD §7 Amendment & Official Naming

Theo [ADR-0010](../decisions/ADR-0010-agent-project-and-merge-authority.md), thay đổi PRD phải được **Chủ dự án (`Owner`)** duyệt khi xác nhận Spec. Đề xuất cập nhật đoạn cuối [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md#L49-L51) tại `## 7. Ngoài phạm vi bản đầu (Sau V1)` (Owner duyệt Spec trước khi triển khai, cập nhật file PRD ở commit đầu của Slice S01):

- **Hiện hành:** `... lịch sử và khôi phục phiên bản (revisions), repeater, quan hệ giữa nội dung (relation), ...`
- **Thay thế:** `... lịch sử và khôi phục phiên bản (revisions), repeater cho custom fields của CPT (riêng Trình dựng trang hỗ trợ trường lặp \`repeater\` tối đa 1 cấp có giới hạn kích thước trong \`block.json\`), quan hệ giữa nội dung (relation), ...`
- **Tên gọi chính thức:** Thống nhất dùng **OpenDash Studio / Trình dựng trang**, thay thế toàn bộ các chuỗi hiển thị `"UX Builder"` tại 5 file TSX của `cms/admin/src/` và 2 thông báo API trong [`cms/crates/opendash-server/src/storage/builder.rs`](../../cms/crates/opendash-server/src/storage/builder.rs#L1106-L1111) (giữ nguyên `data-testid="open-ux-builder-toolbar"` tại [`PanelRegionRenderer.tsx#L318`](../../cms/admin/src/features/editor/PanelRegionRenderer.tsx#L318) để không làm gãy selector test E2E hiện có).

---

## Open Questions

*(Không còn câu hỏi mở. Tất cả phát hiện từ Goldfish Round 1 và Round 2 đã được giải quyết dứt khoát tại Technical Plan và 4 Slices bên dưới.)*

---

## Critical User Journeys

1. **CUJ-1 — Đăng ký block qua `block.json` từ Theme v3 / Plugin ZIP:**
   - **Given** tác giả đặt `blocks/<slug>/block.json` + `blocks/<slug>/render.html` hợp lệ trong Theme v3 hoặc Plugin ZIP (với `package_id` hợp lệ không chiếm `"core"`/`"legacy"`).
   - **When** Admin upload và kích hoạt Theme v3 / cài Plugin ZIP.
   - **Then** hệ thống xác thực `BlockSchemaV1`, kiểm C-SEC `render.html` (kể cả trong `patterns/*.json` của Theme v3 nếu dùng block của theme đó), băm vào `revision_digest` và đồng bộ `builder_block_definitions`; block xuất hiện ngay trong palette của OpenDash Studio kèm `<BlockSchemaForm>` đầy đủ control và CSS scoped đúng viewport.
2. **CUJ-2 — Dựng & xem trước `core/banner` và `core/accordion` trong OpenDash Studio:**
   - **Given** Editor mở Page `#/studio/entry/page/<id>` hoặc Admin mở Template `#/studio`.
   - **When** chèn `core/banner` (chọn `bg_media` từ `<MediaPicker>`, chỉnh `overlay_color`, đặt `min_height` riêng `Desktop` vs `Mobile`, nhập mô tả `richtext`) và `core/accordion` (chỉnh 3 mục `repeater`, đảo thứ tự).
   - **Then** `BuilderPreviewFrame` chuyển cả `url("/api/admin/media/{id}/file")` trong `<style data-od-block-styles>` sang `data:` URI để hiển thị ảnh nền trong iframe `sandbox="allow-scripts"`; đổi viewport `Desktop` (`>=1024px`) ↔ `Tablet` (`<1024px`) ↔ `Mobile` (`<768px`) cập nhật chiều cao chuẩn xác; khi Entry còn `draft`, `/media/{id}/file` công khai trả **`404 Not Found`** và chỉ mở `200 OK` sau khi **Xuất bản**; xóa media đang nằm trong `repeater` bị chặn **`409 Conflict`**.
3. **CUJ-3 — Bảo toàn dữ liệu & Thu hồi ACL khi tắt Plugin / đổi Theme / nâng cấp Schema:**
   - **Given** Page hoặc Template Override đã lưu một `plugin/block` có ảnh `M` thuộc plugin `acme-blocks`, sau đó Admin tắt `acme-blocks` (`ProviderInactive`) hoặc nâng cấp phiên bản theme bỏ bớt field/thêm field mới.
   - **When** tắt plugin / đổi theme hoặc Editor sửa một section khác rồi Lưu (không sửa hay di chuyển node bị tắt).
   - **Then** server tự resync ACL công khai ngay trong transaction đổi trạng thái để ảnh `M` (nếu chỉ thuộc block đã tắt) không còn tải công khai qua `/media/M/file` (`404`), nhưng raw pin trong `collect_pins_in_tx` vẫn giữ nguyên (`409` nếu xóa `M`), Editor vẫn Lưu thành công phần còn lại mà không mất dữ liệu node cũ, và khi render chỉ hiện khối rỗng an toàn kèm `BlockDiagnostic` trong Studio.

---

## Technical Plan

### 1. Kiến trúc Tổng thể, Storage & Vòng đời Registry

```text
Embedded Core (include_str!) ─┐
Theme v3 (themes/revisions/) ─┼─► BlockSchemaV1 + C-SEC Pre-Validate ─► SQLite: builder_block_definitions
Plugin ZIP (+ plugin_revisions)┘                                                    │
Legacy theme_blocks ──────────┘ (upcast legacy_mode: true)                          ▼
                                                                    RegistryCatalog<RegisteredBlockSchema>
                                                                      ├── GET /api/admin/builder/blocks
                                                                      ├── builder.rs validate_for_save_with_schemas
                                                                      ├── Media ACL sync & resync on toggle/install/theme
                                                                      └── page.rs SSR + RenderBudget + BlockTemplateCache
```

- **Tái sử dụng AST & Comment Tag Hiện Hữu:** Giữ nguyên [`KnownElement::PluginBlock(PluginBlockProps)`](../../cms/crates/opendash-core/src/builder/mod.rs) (`"type": "plugin/block"`) và tag Comment Block **`<!-- od:plugin_block {"id":"...","block_id":"...","values":{...}} /-->`** tại [`builder_blocks.rs#L274-L281, #L822-L828`](../../cms/crates/opendash-core/src/builder_blocks/mod.rs). Không thêm variant AST mới.
- **Migration `0046_declarative_block_engine.sql` (cùng `cms/migrations/README.md`):**
  ```sql
  CREATE TABLE builder_block_definitions (
      block_id TEXT PRIMARY KEY NOT NULL,
      owner_kind TEXT NOT NULL CHECK (owner_kind IN ('core', 'theme', 'plugin')),
      owner_id TEXT NOT NULL,
      provider_status TEXT NOT NULL CHECK (provider_status IN ('active', 'provider_inactive')),
      declaration_state TEXT NOT NULL CHECK (declaration_state IN ('declared', 'withdrawn')),
      schema_json TEXT NOT NULL CHECK (json_valid(schema_json)),
      template TEXT NOT NULL,
      revision_digest TEXT NOT NULL,
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  );
  CREATE INDEX idx_builder_block_definitions_owner
      ON builder_block_definitions(owner_kind, owner_id, provider_status, declaration_state);

  ALTER TABLE plugin_revisions
      ADD COLUMN blocks_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(blocks_json));
  ```
  - **Cấu trúc `plugin_revisions.blocks_json` & Disk Revision:** JSON object keyed theo slug: `{ "<slug>": { "schema": <BlockSchemaV1>, "template": "<render.html UTF-8>" } }` (`"{}"` khi plugin không có block nào). Khi plugin không có block nào, **không tạo `plugins/revisions/<digest>/blocks.json` và không thay đổi công thức băm `compute_plugin_revision_digest_with_extensions`** để giữ nguyên 100% digest/durability của mọi plugin pre-`0045`. Khi plugin có >= 1 block, hash bytes chuẩn hóa của `blocks_json` (sorted keys) vào `compute_plugin_revision_digest_with_extensions`, staging thêm `plugins/revisions/<digest>/blocks.json`, và `verify_plugin_revision_durability` kiểm tra tính khớp của `blocks.json` mỗi khi file/metadata hiện diện.
  - **Số lượng object SQLite & Backup/Restore ([`restore.rs`](../../cms/crates/opendash-server/src/storage/restore.rs#L14, #L170-L220), [`backup_worker.rs`](../../cms/crates/opendash-server/src/backup_worker.rs#L780-L799)):** Giữ tổng số `sqlite_schema` xa ngưỡng `MAX_SCHEMA_OBJECTS = 256` (chỉ tăng 1 bảng + 1 index). Lưu `blocks_json` trong `plugin_revisions` + `builder_block_definitions` ngay trong SQLite bảo đảm **100% Backup/Restore** giữ đủ block plugin khi restore sang host sạch dù `backup_worker.rs` chỉ copy DB + `themes/revisions/`. Với bản backup `<= 0044` được restore, `sqlx::migrate!` chạy `0045` lúc khởi động và `ensure_embedded_core_blocks` tự nạp 5 core blocks + sync blocks của active theme từ `themes/revisions/`.
- **Chống Chiếm Namespace & Xung đột Cross-Provider ([`id.rs`](../../cms/crates/opendash-core/src/id.rs#L22-L64)):**
  - Cập nhật `id::validate_package_id`: từ chối rõ ràng `"core"` và `"legacy"` (lưu ý `builder.rs#L527` `check_node_id` cũng gọi `valid_package_id`, đã xác minh không có test nào dùng `"core"`/`"legacy"` làm ID node).
  - Ràng buộc định danh block thống nhất: folder `<slug>` và tên block sau `/` thỏa `valid_kebab_segment_with_limit(slug, 32)`, bắt buộc `block_schema.id == format!("{owner_id}/{dir_slug}")`, và gói có thư mục `blocks/` bắt buộc `owner_id` thỏa `valid_kebab_segment_with_limit(owner_id, 32)` (lỗi `400 Bad Request` nếu `id` gói bắt đầu bằng số hoặc > 32 ký tự mà khai báo `blocks/`).
  - Trước khi ghi `builder_block_definitions`, nếu `block_id` đã thuộc `(owner_kind, owner_id)` khác hoặc trùng `theme_blocks.id` ngoài owner hiện tại → rollback transaction và trả **`409 Conflict`**.
- **Đồng bộ Vòng đời (`storage/page.rs`, `storage/theme.rs`, `storage/plugin.rs`, `plugin/manager.rs`):**
  - *Startup:* `ensure_embedded_core_blocks(&db)` upsert 5 core blocks (`owner_kind='core'`, `owner_id='core'`, `provider_status='active'`, `declaration_state='declared'`).
  - *Theme activate (`storage/theme.rs::activate_revision`):* Dùng `ThemeRevisionRecord.file_digests` (`storage/theme.rs#L70, #L296-L307`) đã được `verify_revision_durability` xác thực: nếu `themes/revisions/<revision_digest>/blocks/` tồn tại, đọc & parse lại `blocks/<slug>/{block.json,render.html}` trong `BEGIN IMMEDIATE`, chạy `sync_owner_blocks_in_tx` (trả `Err(ApiError::conflict(...))` nếu trùng cross-provider, khớp `sync_theme_declarations_in_tx`; chuyển block của theme cũ sang `provider_inactive`; chuyển block bị rút ở revision mới sang `declaration_state='withdrawn', provider_status='provider_inactive'`), rồi gọi `resync_plugin_block_acls_in_tx(&mut conn)`.
  - *Plugin install / upgrade / toggle (`storage/plugin.rs` + [`plugin/manager.rs#L531-L560`](../../cms/crates/opendash-server/src/plugin/manager.rs#L531-L560)):*
    - Trong `install_plugin_revision`: kiểm tra xung đột `409 Conflict` cross-provider trước bước staging đĩa (`storage/plugin.rs#L270`), đồng bộ `sync_owner_blocks_in_tx` trong transaction DB (`enabled = 1` tại `#L295`), ghi `plugin_revisions.blocks_json`, và gọi **`resync_plugin_block_acls_in_tx`** (block bị bỏ ở phiên bản mới chuyển `declaration_state='withdrawn', provider_status='provider_inactive'`).
    - Trong `set_plugin_enabled` & `set_plugin_enabled_cas`: chạy `set_owner_block_status_in_tx` với điều kiện **`AND declaration_state = 'declared'`** (không hồi sinh block `Withdrawn`!), rồi gọi **`resync_plugin_block_acls_in_tx(&mut tx)`**.
- **Hợp đồng `GET /api/admin/builder/blocks` & `GET /registry`:**
  - `GET /api/admin/builder/blocks` (route trong `app.rs`, `require_session`, `Cache-Control: no-store`) trả `{ "items": BlockCatalogItemDto[] }` (**tuyệt đối không trả `template`**):
    ```ts
    export interface BlockCatalogItemDto {
      block_id: string;
      owner_kind: 'core' | 'theme' | 'plugin';
      owner_id: string;
      status: 'active' | 'provider_inactive';
      declaration_state: 'declared' | 'withdrawn';
      schema: BlockSchemaV1; // schema_version, id, title, category, icon, render, fields
    }
    ```
  - `load_unified_block_catalog_in_conn(&mut SqliteConnection)` đọc `builder_block_definitions` cộng các `theme_blocks` legacy chưa có mặt (upcast `legacy_mode: true`), giúp mọi writer/transaction dùng chung không bị deadlock pool; `GET /registry` giữ nguyên `BTreeMap<String, Fields>`; riêng `KnownElement::Legacy` giữ nguyên `theme_blocks` + alias fallback tại `page.rs#L615-L664`.

### 2. Đặc tả Chính xác `block.json` v1 (`cms/crates/opendash-core/src/block_schema.rs`)

Thuần Rust (`serde`, `serde_json`, `url` — 0 crate mới); regex hex/CSS var dùng byte checks thuần.

#### Cấu trúc Serde Chính thức (`#[serde(deny_unknown_fields)]`)
```rust
const fn default_repeater_max() -> usize { 32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockCategory { Layout, Media, Interactive, Content, Marketing, Plugin }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaFieldKind { Image, Video, #[default] Any }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CssUnit { Px, Rem, Em, #[serde(rename = "%")] Percent, Vw, Vh }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption { pub value: String, pub label: String }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShowIfScalar { Bool(bool), Int(i64), String(String), Null }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShowIfRule { pub field: String, pub equals: ShowIfScalar }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockFieldSpec {
    Text { key: String, label: String, #[serde(default)] max_chars: Option<usize>, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] placeholder: Option<String>, #[serde(default)] show_if: Option<ShowIfRule> },
    Textarea { key: String, label: String, #[serde(default)] max_chars: Option<usize>, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] show_if: Option<ShowIfRule> },
    Richtext { key: String, label: String, #[serde(default)] max_bytes: Option<usize>, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] show_if: Option<ShowIfRule> },
    Number { key: String, label: String, #[serde(default)] min: Option<f64>, #[serde(default)] max: Option<f64>, #[serde(default)] step: Option<f64>, #[serde(default)] required: bool, #[serde(default)] default: Option<f64>, #[serde(default)] unit: Option<CssUnit>, #[serde(default)] css_var: Option<String>, #[serde(default)] responsive: bool, #[serde(default)] show_if: Option<ShowIfRule> },
    Range { key: String, label: String, min: f64, max: f64, #[serde(default)] step: Option<f64>, #[serde(default)] default: Option<f64>, #[serde(default)] unit: Option<CssUnit>, #[serde(default)] css_var: Option<String>, #[serde(default)] responsive: bool, #[serde(default)] show_if: Option<ShowIfRule> },
    Boolean { key: String, label: String, #[serde(default)] default: Option<bool>, #[serde(default)] show_if: Option<ShowIfRule> },
    Select { key: String, label: String, options: Vec<SelectOption>, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] css_var: Option<String>, #[serde(default)] responsive: bool, #[serde(default)] show_if: Option<ShowIfRule> },
    Color { key: String, label: String, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] css_var: Option<String>, #[serde(default)] show_if: Option<ShowIfRule> },
    Media { key: String, label: String, #[serde(default)] media_kind: MediaFieldKind, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] css_var: Option<String>, #[serde(default)] show_if: Option<ShowIfRule> },
    Url { key: String, label: String, #[serde(default)] required: bool, #[serde(default)] default: Option<String>, #[serde(default)] placeholder: Option<String>, #[serde(default)] show_if: Option<ShowIfRule> },
    Group { key: String, label: String, fields: Vec<BlockFieldSpec>, #[serde(default)] show_if: Option<ShowIfRule> },
    Repeater { key: String, label: String, #[serde(default)] min_items: usize, #[serde(default = "default_repeater_max")] max_items: usize, #[serde(default)] item_label_field: Option<String>, fields: Vec<BlockFieldSpec>, #[serde(default)] show_if: Option<ShowIfRule> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSchemaV1 {
    pub schema_version: u8,
    pub id: String,
    pub title: String,
    pub category: BlockCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub render: String,
    #[serde(default)]
    pub fields: Vec<BlockFieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredBlockSchema {
    pub schema: BlockSchemaV1,
    pub legacy_mode: bool,
}
```

#### Quy tắc Khóa Cứng ở `BlockSchemaV1::validate()`
- **Định danh & tên dành riêng:** `schema_version == 1`, `render == "render.html"`, `valid_namespaced_block_id(&id)` và `id.starts_with(&format!("{expected_owner_prefix}/"))`. Mọi `field.key` thỏa `valid_snake_key_with_limit(key, 32)`, không trùng `key` trong toàn block (bung phẳng `Group`), và **chống xung đột biến tổng hợp**: field ngoài `Url` không được đặt tên `"url"` hoặc kết thúc bằng `_url`, đồng thời **mọi field cùng scope (kể cả `Url` hoặc `Group`)** không được trùng `<media_key>_url`, `<media_key>_alt`, `<media_key>_width` hay `<media_key>_height` của bất kỳ field `Media` nào cùng scope (cho phép các khóa layout tự nhiên như `min_height` của `core/banner`).
- **DoS Bounds:** Tổng số field thực top-level (không đếm `Group`, cộng cả con bên trong `Group` và 1 cho mỗi `Repeater`) `<= 32`; tổng field lá tính cả trong `Repeater` `<= 48`; `Repeater.fields` **chỉ chứa 10 kiểu nguyên thủy** (`Text | Textarea | Richtext | Number | Range | Boolean | Select | Color | Media | Url`, không `Group`/`Repeater`, `responsive: false`); `1 <= Repeater.max_items <= 32`, `min_items <= max_items`, tổng `max_items` trên 1 block `<= 64`, `item_label_field` trỏ tới sub-field `Text|Select|Number|Url` tồn tại trong `Repeater.fields`.
- **Ràng buộc theo từng kiểu field:**
  - `Text`: `max_chars` trong `1..=2000` (mặc định `256`). `Textarea`: `1..=16_384` (mặc định `4000`).
  - `Richtext`: `max_bytes` sau sanitize `<= 32_768` (mặc định `32_768`), đầu vào thô UTF-8 `<= 65_536` (`64 KiB`).
  - `Number` / `Range`: `min`/`max` nếu có phải `is_finite()`, `min <= max`; `step` nếu có phải `is_finite() && step > 0.0`. `unit` hợp lệ trên `Number`/`Range` **kể cả khi `css_var` là `None`** (dùng làm nhãn đơn vị trên UI); nếu có `css_var` mà `unit` là `None`, CSS xuất số thực/nguyên trần (ví dụ `1.5`).
  - `Select`: `1..=32` `SelectOption` (`value` không rỗng `<= 64`, label `<= 80`). Nếu `Select` có `css_var`, mọi `option.value` phải khớp `^[a-z0-9_-]{1,32}$`.
  - `Color`: Chỉ chấp nhận `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `"transparent"`, `"currentColor"`, hoặc đúng 1 trong 6 token: `["var(--od-token-primary)", "var(--od-token-accent)", "var(--od-token-surface)", "var(--od-token-muted)", "var(--od-token-text)", "var(--od-token-border)"]`.
  - `Media`: Nếu đặt `css_var`, bắt buộc `media_kind == MediaFieldKind::Image`.
  - `Url`: Kiểm tra qua **`valid_block_url`**: cho phép bare **`"#"`**, `#<fragment>` (`#[A-Za-z0-9_-]{1,128}`), `/...` (cấm `//`, `..`, `\`), `http://`, `https://`, `mailto:`, `tel:`, cấm credentials, whitespace/ASCII control bytes (`0x00..=0x1f`, `0x7f`), `javascript:`/`data:`/`vbscript:`, tối đa `2048` bytes.
  - `css_var`: Khớp `^--od-[a-z0-9-]{1,48}$`; không trùng nhau trong 1 block.
  - `ShowIfRule`: `field` trỏ tới flat field tồn tại ở top-level (không phải `Group`, `Repeater`, hoặc field có `responsive: true`).

#### Quy tắc `validate_values`, `legacy_mode` & Schema Evolution (`builder.rs`)
- **`legacy_mode == true` vs `false`:** Với `RegisteredBlockSchema { legacy_mode: true }` (dùng cho `theme_blocks` cũ qua `upcast_legacy_descriptor` trong shim `validate_for_save`), `validate_values` áp dụng nguyên quy tắc tương thích cũ (`exact keys matching descriptor`, string `<= 512` chars, cho phép key legacy `"url"`/`"*_url"`) để giữ nguyên 100% test `legacy_descriptor_matches_m3_block_schema` (`builder.rs#L1247`); đồng thời `KnownElement::Legacy` giữ `Block::validate_against_fields`. Với mọi block `block.json` (`legacy_mode == false`), áp dụng đầy đủ chuẩn `BlockSchemaV1`: `serde_json::to_vec(&props.values)?.len() <= 65_536`, tổng key `<= 64` (giữ lại unknown keys cũ của phiên bản trước).
- **Truy xuất `previous_values` khi nâng cấp schema & `show_if`:**
  - Nếu `show_if` của field đánh giá thành `false` với `values` hiện tại → **bỏ qua** `required` cho field đó (giữ nguyên dữ liệu đang ẩn trong `values`).
  - Tìm `previous_values` bằng cặp **`(element.id, props.block_id)`** ở bất kỳ vị trí nào trong `previous` (cho phép kéo đổi vị trí section/row/col của node Active đã nâng cấp schema mà không bị vướng `required` ở field mới thêm!): nếu key mới của schema vắng trong `previous_values` và vắng trong `values` hiện tại → bỏ qua `required`; với `Repeater`, kiểm tra từng phần tử theo index `i`: nếu `i < prev_items.len()` và sub-key mới vắng trong `prev_items[i]` lẫn `cur_items[i]` → bỏ qua `required`.
- **Truy xuất `sanitize_richtext` hai chốt:** S01 (`validate_values`) gọi `sanitize_richtext(raw)` kiểm bounds UTF-8 `<= 64 KiB` / output `<= max_bytes` mà không biến đổi JSON người dùng nhập; S02 (`page.rs` / render) **bắt buộc** chạy lại `sanitize_richtext(raw)` trước khi tạo nonce `OD_RICHTEXT_<32hex>` thay thế.
- **`ProviderInactive` / `Withdrawn` (`builder.rs#L536-L553, #L629-L642`):** [`validate_known`](../../cms/crates/opendash-core/src/builder/mod.rs) bắt buộc `descriptor.status == ProviderStatus::Active && descriptor.declaration_state == DeclarationState::Declared`. Node thuộc provider ngưng/rút/thiếu chỉ được lưu khi thỏa đúng **`is_preserved(element, section.id, row.id, column.id, previous)`** tại vị trí `(s, r, c, e)` (giống 100% `preservesAnchors` của `document.ts` — cho phép xóa node, không cho sửa payload hay đổi cột).

### 3. Safe RichText AST Tokenizer & C-SEC Hardening (`opendash-core::richtext` + `template_sec.rs` + `document.rs` + `page.rs`)

1. **`opendash-core::richtext::sanitize_richtext(raw: &str) -> Result<String, &'static str>`:**
   - Từ chối ngay nếu `raw.len() > 65_536` (`64 KiB`) hoặc chứa `'\0'`.
   - Pure-Rust tokenizer + stack balancer (depth cap = `16`), tách dùng chung `decode_html5_entities` (giải mã toàn bộ HTML5 named refs như `&sol;`, `&colon;`, `&Tab;`, `&NewLine;` + decimal/hex có leading zero và thiếu `;`).
   - Loại bỏ toàn bộ thẻ + thân của RAWTEXT/RCDATA/foreign elements (`script`, `style`, `noscript`, `title`, `textarea`, `xmp`, `noembed`, `noframes`, `plaintext`, `iframe`, `object`, `embed`, `svg`, `math`), bỏ comments (`-->`, `--!>`, `<!-->`, `<!--->`), CDATA và xử lý đóng thẻ lồng vỡ.
   - Allowlist duy nhất: `p`, `br`, `strong`, `b`, `em`, `i`, `u`, `ul`, `ol`, `li`, `blockquote`, `a`. Thẻ ngoài allowlist như `span`/`div`/`h1` bóc thẻ giữ text; cấm `<a>` lồng `<a>`; chỉ `<a href>` giữ attribute `href` sau khi qua `decode_html5_entities` + `valid_block_url` (cấm ASCII control chars `0x00..=0x1f`, `0x7f`), xuất `<a href="<escaped>" rel="noopener noreferrer">` với toàn bộ `& < > " '` re-escaped; output `<= 32_768` bytes.
2. **Đóng lỗ hổng HTML5 chung trong `template_sec.rs` & `document.rs`:**
   - **Đồng bộ đóng comment HTML5:** Cập nhật `HtmlScanState::Comment` (`#L867` — chuyển trực tiếp sang trạng thái comment sau `<!--` và nhận ngay `-->`, `--!>`, `<!-->`, `<!--->`), `template_sec::validate_html_and_css_context` (`#L1309-L1315`) và [`document::tokenize`](../../cms/crates/opendash-server/src/document.rs#L221-L232) để không bị `<!-- --!>` hoặc `<!-->` vượt rào. Bổ sung `title`, `xmp`, `noembed`, `noframes`, `plaintext` vào `FORBIDDEN_TAGS` / `HtmlScanState`.
   - **Chống Duplicate Attribute & Static URL Decoding (`#L1432`, `#L1440-L1506`):** `parse_attributes` **từ chối ngay khi gặp trùng tên thuộc tính** (thay vì ghi đè last-wins trong `BTreeMap`); `validate_tag_attributes` gọi `decode_html_entities_for_url` cho cả nhánh `!post_render`, từ chối byte điều khiển ASCII `0x00..=0x1f`/`0x7f` sau decode (không xóa ngầm) và kiểm thêm `srcset` và `ping` cùng `validate_safe_url` (chỉ `<a href>` dùng `validate_safe_href_url` cho phép `#`, `#fragment`, `mailto:`, `tel:`).
   - **Joint Jinja-AST + `HtmlScanState` (`validate_block_template_csec`):** Cấm chuỗi literal `"OD_RICHTEXT_"` (mirror `PREVIEW_PART_NONCE_PREFIX`); theo dõi vòng `Stmt::ForLoop`; **cấm `Stmt::Set`, `Stmt::SetBlock`, `Stmt::WithBlock`, `Expr::GetItem`, `Expr::Filter`** lên biến `richtext`, `url`, `media` hoặc biến lặp `ForLoop`; chỉ cho phép `richtext` ở nhánh `IfCond`-merged `HtmlScanState::Body`; và chỉ cho `url` / `<media_key>_url` vào thuộc tính URL tương ứng (`href` vs `src`/`poster`).
   - **Thay thế Nonce `richtext` hậu-render:** Sinh `OD_RICHTEXT_<32hex>` bằng `OsRng`, đưa vào MiniJinja dưới dạng chuỗi thường (**cấm `Value::from_safe_string`**), sau `render_captured` + `validate_html_and_css_context` dùng một-pass `HtmlScanState` thay **duy nhất khi đang ở `HtmlScanState::Body`** (cấm xuất hiện ở attribute/comment/rawtext hoặc > 1 lần).
3. **Scoped CSS, `BlockTemplateCache`, Hierarchical `{scope}` & Shared `RenderBudget` (`app.rs`, `routes/public_content.rs`, `page.rs`, `overrides.rs`, `public_content.rs`, `BuilderPreviewFrame.tsx`):**
   - **Canonical Selector duy nhất & Phân cấp Scope tránh đè `<head>`:** Wrapper `<div class="builder-plugin-block" data-block-id="..." data-od-node-scope="{scope}-{node_id}">`, rule CSS phát dạng **`[data-od-node-scope="{scope}-{node_id}"] { --od-xxx: ...; }`**, sử dụng đúng breakpoint của `builder_responsive_css` (**`@media (width < 1024px)`** tablet và **`@media (width < 768px)`** mobile). Root tree scope truyền tường minh per-call vào `page::render_builder_*`: `"entry"` (Entry Body), `"ov"` (DB Template Override), `"home"` (Homepage), `"hdr"` / `"ftr"` / `"tp_{part_idx}"` (Template Parts) và `"pat_{pattern_id}"` (mỗi Pattern trong `PatternResults`, bảo đảm không bao giờ trùng với Entry/Override/Header/Footer khi `compose_document_with_seo` đưa `<style data-od-block-styles>` lên `<head>`).
   - **An toàn giá trị CSS & Preview Inlining:** `validate_safe_scoped_block_css(css, is_preview)` từ chối `{`, `}`, `;`, `\`, `/*`, `*/`, `\r`, `\n` trong value và cho `url("/api/admin/media/{64hex}/file")` chỉ khi `is_preview == true` (giữ `/media/...` & `/theme/...` cho public; bỏ qua nếu rỗng `""`). Tại `BuilderPreviewFrame.tsx::preparePreviewMarkup`, tải & thay `url("/api/admin/media/{64hex}/file")` thành `url("data:...")` và rewrite `url("/media/...")` trong `style[data-od-block-styles]`.
   - **`BlockTemplateCache` (`app.rs`) & `HomeState` (`storage/builder.rs`):** `AppState` giữ `pub block_template_cache: Arc<BlockTemplateCache>` (bọc `std::sync::RwLock<BTreeMap<(String, String), Arc<ValidatedBlockTemplate>>>`, khi `len() >= 256` thì `.clear()` trước khi chèn; key `(block_id, sha256(schema_json || "\0" || template))`; implements `FromRef<AppState>`), truyền qua routes (`public_content`, `builder`, `themes`) vào `page.rs`. `HomeState` thêm `#[derive(Default)]` và `pub declared_blocks: BTreeMap<String, RegisteredBlockDef>` (`storage/builder.rs`, cập nhật 4 literal ở `page.rs#L1184..#L1312` bằng `..Default::default()`, giữ nguyên `themes` & `register_plugin_block_template`).
   - **`RenderBudget` xuyên suốt Request:** `pub struct RenderBudget { pub fuel_remaining: u64, pub html_bytes_used: usize }` (khởi tạo `10_000`, `0`), nhúng vào `PublicCompositionBudget` / `PreviewCompositionBudget` (`overrides.rs`) và trả kèm trong `PublicEntryRenderSnapshot` (`storage/public_content.rs`) để `routes/public_content.rs::render_entry_response` và `wrap_entry_body_in_preview_shell_in_tx` trừ chung **một ngân sách** từ Entry Body -> Template Override -> Header/Footer. Mỗi `PluginBlock` nhận `min(budget.fuel_remaining, 2_000)` fuel; nếu `fuel_remaining == 0` lúc vào hoặc runtime fail/OOF, **cô lập**: ghi `BlockDiagnostic { node_id, code: "plugin_block_render_error" }` (`"plugin_block_unavailable"` chỉ dùng cho block thiếu/inactive/withdrawn) và trả placeholder rỗng; cộng `rendered.len()` vào `budget.html_bytes_used` và chỉ fail-closed khi toàn trang vượt `MAX_RENDERED_HTML_BYTES` (`512 KiB`).

### 4. Media Refs, ACL Privacy & Trích xuất Văn bản

- **Delete Pin vs Public ACL:** `media_refs::collect_pins_in_tx` giữ nguyên quét JSON thô (`409` delete protection). `builder_blocks::extract_block_refs_with_schemas` (có arm `"plugin_block"`) & `extract_document_media_refs` **chỉ thu thập `strict_media_ids` từ `Active + Declared` `PluginBlock`**, tuyệt đối không đẩy 64-hex từ node `ProviderInactive`/`Withdrawn`/`Unresolved` vào các bảng Public ACL (`entry_builder_media_refs`, `template_override_media_refs`, `builder_homepage_media`, `builder_homepage_video_refs`).
- **`*_in_conn` Transactions & Homepage `media_kind = "any"`:** Dùng `load_unified_block_catalog_in_conn(&mut SqliteConnection)` bên trong `sync_entry_body_refs_in_conn` (`entries.rs`, `422 Unprocessable`), `CompositePreflightPlan`, `validate_media_refs_in_tx` (`builder.rs`, `400 Bad Request`), `check_preview_images` (`builder.rs`), `save_and_render` (nếu `media_assets.kind == "image"` -> ghi `builder_homepage_media`, `"video"` -> `builder_homepage_video_refs(ref_role='video')`), `replace_entry_builder_refs` và `refresh_override_media_refs_in_tx` (`overrides.rs`).
- **`resync_plugin_block_acls_in_tx` Bảo toàn Guard (`storage/page.rs`):** Khi theme/plugin thay đổi trạng thái trong `activate_revision`, `install_plugin_revision`, `set_plugin_enabled*`, chỉ resync bảng ACL cho các dòng chứa `plugin/block` / `od:plugin_block`; riêng `content_entries` **bắt buộc** lọc `content_type_key NOT IN ('pattern', 'section')`, kiểm `supports` có `'builder'` và kiểm **`body_allows_public_refs(&schema)`** (`public_exposure == Public && min_view_role == Editor` — xóa sạch ACL nếu `body` private!), chỉ ghi `entry_builder_media_refs` và **không động vào `entry_pattern_refs`** hay thất bại trên pattern đã xóa!
- **`extract_visible_text` (`builder_blocks.rs#L1254-L1285`):** Bổ sung arm `"plugin_block"` trích văn bản từ `text`, `textarea`, `richtext` (qua `strip_richtext_to_plain`) và sub-field `repeater`.

### 5. 5 Embedded Core Blocks (`builtin_blocks/`) & Package Auto-Discovery

1. **5 Core Blocks nhúng sẵn (`cms/crates/opendash-server/src/builtin_blocks/<slug>/{block.json,render.html}`, tất cả field có đủ `label` + `SelectOption { value, label }`):**
   - **`core/spacer`:** `category: "layout"`, `icon: "move-vertical"`, `title: "Khoảng trống"`, `height` (`label: "Chiều cao"`, `range`: 8–240, step 4, default 32, `unit: "px"`, `responsive: true`, `css_var: "--od-spacer-h"`).
   - **`core/divider`:** `category: "layout"`, `icon: "minus"`, `title: "Đường phân cách"`, `line_style` (`label: "Kiểu đường"`, `select`: `[solid/Nét liền, dashed/Nét đứt, dotted/Chấm]`, default `"solid"`, `css_var: "--od-divider-style"`), `thickness` (`label: "Độ dày"`, `range`: 1–8, step 1, default 1, `unit: "px"`, `css_var: "--od-divider-t"`), `width_pct` (`label: "Chiều rộng"`, `range`: 10–100, step 5, default 100, `unit: "%"`, `responsive: true`, `css_var: "--od-divider-w"`), `color` (`label: "Màu sắc"`, `color`, default `"#e4e4e7"`, `css_var: "--od-divider-c"`).
   - **`core/banner`:** `category: "media"`, `icon: "image"`, `title: "Banner"`, `group("background", "Bố cục & Nền")` (`bg_media` `label: "Ảnh nền"` `media_kind: "image"` `css_var: "--od-banner-bg"`, `overlay_color` `label: "Lớp phủ"` `color` default `"transparent"` `css_var: "--od-banner-overlay"`, `min_height` `label: "Chiều cao tối thiểu"` `range` 160–800 default 360 `unit: "px"` `responsive: true` `css_var: "--od-banner-h"`, `focus_x` `label: "Tâm ngang (%)"` `range` 0–100 default 50 `unit: "%"` `css_var: "--od-banner-fx"`, `focus_y` `label: "Tâm dọc (%)"` `range` 0–100 default 50 `unit: "%"` `css_var: "--od-banner-fy"`); `group("content", "Nội dung & CTA")` (`align` `label: "Căn lề"` `select`: `[left/Trái, center/Giữa, right/Phải]` default `"center"` `responsive: true` `css_var: "--od-banner-align"`, `heading` `label: "Tiêu đề"` `text` `required: true` default `"Tiêu đề Banner"`, `body` `label: "Mô tả"` `richtext`, `show_cta` `label: "Hiện nút CTA"` `boolean` default `true`, `cta_label` `label: "Nhãn nút"` `text` `show_if: { "field": "show_cta", "equals": true }` default `"Khám phá"`, `cta_url` `label: "Liên kết CTA"` `url` `show_if: { "field": "show_cta", "equals": true }` default `"#"`).
   - **`core/card`:** `category: "content"`, `icon: "square"`, `title: "Thẻ nội dung"`, `layout` (`label: "Bố cục"`, `select`: `[vertical/Dọc, horizontal/Ngang]`, default `"vertical"`), `image` (`label: "Ảnh thẻ"`, `media`, `media_kind: "image"`), `title` (`label: "Tiêu đề"`, `text`, `required: true`, default `"Thẻ nội dung"`), `body` (`label: "Nội dung"`, `richtext`), `bg_color` (`label: "Màu nền"`, `color`, default `"#ffffff"`, `css_var: "--od-card-bg"`), `show_cta` (`label: "Hiện nút CTA"`, `boolean`, default `false`), `cta_label` (`label: "Nhãn nút"`, `text`, `show_if: { "field": "show_cta", "equals": true }`, default `"Xem chi tiết"`), `cta_url` (`label: "Liên kết"`, `url`, `show_if: { "field": "show_cta", "equals": true }`, default `"#"`).
   - **`core/accordion`:** `category: "interactive"`, `icon: "chevrons-up-down"`, `title: "Accordion / FAQ"`, `style` (`label: "Kiểu hiển thị"`, `select`: `[bordered/Có viền, minimal/Tối giản]`, default `"bordered"`), `open_first` (`label: "Mở sẵn mục đầu"`, `boolean`, default `true`), `items` (`label: "Danh sách mục"`, `repeater`, `min_items: 1`, `max_items: 24`, `item_label_field: "title"`, các sub-field: `title` `label: "Tiêu đề"` `text` `required: true`, `content` `label: "Nội dung"` `richtext`, `open` `label: "Mở sẵn"` `boolean` default `false`). Render `<details{% if item.open or (open_first and loop.first) %} open{% endif %}>` (**0 KB JS**).
2. **Theme v3 & Plugin ZIP Auto-Discovery:**
   - **Theme v3:** Cho phép `blocks/<slug>/{block.json,render.html}` (`slug <= 32`, `<= 32` block/gói) trong `v3_entry_cap_and_allowlist` & `declared` của `validate_theme_v3_files` (đủ cặp 2 file), nạp catalog (Core + Theme blocks) truyền vào kiểm chứng `patterns/*.json` (`theme.rs#L259`).
   - **Plugin ZIP (`plugin/package.rs` & [`docs/contracts/PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md)):** Tái sử dụng `package::validate_v2_relative_path` + `seen_lower` + `V2_MAX_EXPANSION_RATIO = 100`, bỏ qua `entry.is_dir()` (`size == 0`) với trần thô `archive.len() <= 256` và nâng **`MAX_PLUGIN_FILES = 72` áp dụng cho file thường trích xuất** (đủ 32 block × 2 file + `manifest.json`/`plugin.wasm`/`README.md`, áp dụng `manifest_schema` `1|2`).

### 6. Studio Admin UI (`cms/admin/src/` — 0 dependencies mới)

- **`api/builder.ts` & `types.ts`:** Thêm `BlockCatalogItemDto`, `BlockSchemaV1`, `BlockFieldSpec`, `BlockPropValue`, `getBlockSchemas()`.
- **`features/media/MediaPicker.tsx`:** Widen `acceptKind?: MediaKind | 'any'`; giữ biến state nội bộ `filter: 'all' | 'image' | 'video'` (mặc định `'all'` khi `acceptKind === 'any'`, truyền `filter === 'all' ? undefined : filter` vào `listMedia` để không bao giờ gửi `?kind=any` gây `400` ở `routes/media.rs#L45-L50`), cập nhật `<input accept="image/jpeg,image/png,image/webp,video/mp4">` và tự định tuyến `uploadVideo` (`.mp4`/`video/mp4`) vs `uploadMedia` khi `acceptKind === 'any'`.
- **`builder/document.ts`, `BuilderTree.tsx`, `BuilderCanvas.tsx`, `BuilderInspector.tsx`, `BuilderWorkspace.tsx`, `StudioWorkspace.tsx`, `StudioCustomizerNav.tsx`:**
  - Giữ `'plugin/block'` trong `InsertableElementType` ngoài `ELEMENT_TYPES` (`#L54`, tránh phá `BuilderBindingFields.tsx#L17`); mở rộng `appendNode(document, parentId, kind, elementType, options?: { blockSchema?: BlockSchemaV1 })` tạo `{ block_id, values }` chuẩn (bung default trong `Group`, khởi tạo `min_items` cho `Repeater`, `{ desktop: default }` cho `responsive: true`).
  - Mở rộng `isLockedElement`, `isProtectedNode`, `preservesAnchors`, `nodeTitle` nhận `blockCatalog`: cho phép sửa `'plugin/block'` khi `status === 'active' && declaration_state === 'declared'`, khóa chỉ đọc (chỉ cho xóa/giữ nguyên vị trí) khi `provider_inactive` hoặc `withdrawn`; mở rộng `setNodeField` hỗ trợ `BlockPropValue` trên `props.values`.
  - Tạo **`cms/admin/src/builder/BlockSchemaForm.tsx`** (`12` kiểu field + `<RichTextFieldControl>` + `show_if` + breakpoint toggle `Desktop`/`Tablet`/`Mobile` + `repeater` Thêm/Nhân bản/Xóa/Lên/Xuống).
  - Cập nhật `BuilderPreviewFrame.tsx::preparePreviewMarkup` inline CSS `url("/api/admin/media/{64hex}/file")` trong `style[data-od-block-styles]` sang `data:` URL và rewrite `url("/media/...")`.
  - Nạp `getBlockSchemas()` trong cả `StudioWorkspace` và `BuilderWorkspace` (`BuilderEditorCore` + standalone `#/builder`, tự reload khi `themeRevision` đổi hay toggle plugin).
- **Thống nhất Tên gọi (`OpenDash Studio / Trình dựng trang`):** Thay thế mọi chuỗi hiển thị `"UX Builder"` trong `BuilderWorkspace.tsx` (retired; extracted to [`BuilderEditorCore.tsx`](../../cms/admin/src/builder/BuilderEditorCore.tsx)), [`PanelRegionRenderer.tsx`](../../cms/admin/src/features/editor/PanelRegionRenderer.tsx), [`SingleEditWorkspace.tsx`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx), [`StudioCustomizerNav.tsx`](../../cms/admin/src/studio/StudioCustomizerNav.tsx), [`StudioWorkspace.tsx`](../../cms/admin/src/studio/StudioWorkspace.tsx) và 2 thông báo backend tại [`storage/builder.rs#L1106,#L1111`](../../cms/crates/opendash-server/src/storage/builder.rs#L1106-L1111) (giữ nguyên `data-testid="open-ux-builder-toolbar"`).

---

## Alternatives Considered

1. **Tạo variant AST mới `KnownElement::SchemaBlock` thay cho `KnownElement::PluginBlock`:**
   - *Bị bác bỏ (`egm-zoom-out`):* Nhân đôi logic Comment Block `<!-- od:... -->`, round-trip `Unresolved`/`ProviderInactive` và wire handling tại nhiều file không cần thiết vì `PluginBlock` đã có sẵn `block_id` namespaced và `values: BTreeMap<String, Value>`.
2. **Dùng MiniJinja `Value::from_safe_string` cho `richtext` hoặc mở `{{ ... }}` trong `<style>`/`style="..."`:**
   - *Bị bác bỏ (`egm-design` Blocker):* Tạo lỗ hổng Stored XSS ở HTML attribute và CSS breakout. Nonce 128-bit qua `HtmlScanState::Body` và Rust Scoped CSS Serializer ngăn chặn triệt để 2 vector này.
3. **Di chuyển 7 core blocks cũ sang `block.json` trong v1 hoặc quét 64-hex pin vào bảng Public ACL:**
   - *Bị bác bỏ:* Viết lại core block cũ đụng `ElementBinding`/`ResolvedList`/`ContactForm`; còn đưa token 64-hex từ block `ProviderInactive`/`Unresolved` vào `entry_builder_media_refs` sẽ gây rò rỉ media riêng tư/nháp công khai. Bảo vệ xóa giữ tách biệt ở `media_refs::collect_pins_in_tx`.

---

## Non-Functional Requirements

- **Bảo toàn dữ liệu & Privacy:** Tắt plugin, đổi theme hoặc nâng cấp `block.json` giữ nguyên payload đã lưu, bảo toàn node `ProviderInactive`/`Withdrawn` qua `is_preserved`, bảo toàn delete-pin `409` trong `media_refs.rs`, thu hồi Public ACL ngay lập tức khi provider ngưng và giữ `404` cho media draft.
- **Giới hạn DoS:** `<= 32` field thực/block, `<= 48` field lá, `<= 32` item/repeater, `<= 64` tổng item/block, cấm repeater lồng repeater, `<= 64 KiB` HTML thô & `<= 32 KiB` HTML sạch mỗi `richtext`, `<= 64 KiB` JSON values/node, dùng chung `RenderBudget` (`10_000` fuel, tối đa `2_000`/block, `512 KiB` HTML tích lũy toàn trang).
- **Ranh giới Kiến trúc (`CORE_FOUNDATION.md` & `check-standards.py`):** 0 crate Rust mới trong `cms/Cargo.toml`, 0 npm package mới trong `cms/admin/package.json`; `opendash-core` thuần Rust 100%; SQLite queries đặt trong `storage/`; `GET /api/admin/builder/blocks` dùng `require_session`, giữ nguyên CSRF guard trên mọi mutation route.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| Phạm vi / Slice | File / Thư mục mục tiêu | Vai trò chính |
| :--- | :--- | :--- |
| **Docs & Contracts** | `docs/product/PRODUCT_REQUIREMENTS.md` (đã cập nhật §7 tại PR #470), `cms/migrations/README.md`, `docs/contracts/M8_WRITER_INVENTORY.md`, `docs/contracts/PLUGIN_ABI.md` | Ngoại lệ `repeater` 1 cấp Trình dựng trang, bảng writer `builder_block_definitions`, mở rộng Plugin ZIP `blocks/` (`MAX_PLUGIN_FILES = 72`). |
| **Slice S01 (`opendash-core`)** | `cms/crates/opendash-core/src/{lib.rs,id.rs,richtext.rs*,block_schema.rs*,theme.rs,builder.rs,builder_blocks.rs}` | `BlockSchemaV1` (12 kiểu field), `richtext` sanitizer thuần Rust, reserved `"core"`/`"legacy"`, `validate_for_save_with_schemas`, trích xuất media & văn bản `plugin_block`. |
| **Slice S02 (C-SEC & 5 Core Blocks)** | `cms/migrations/0046_declarative_block_engine.sql*`, `cms/crates/opendash-server/src/{app.rs,main.rs,template_sec.rs,document.rs,page.rs}`, `builtin_blocks/**`, `storage/{page,builder,overrides,public_content,restore}.rs`, `routes/{public_content,builder,themes}.rs` | Migration `0046`, đóng lỗ hổng HTML5 comment/duplicate attr/entity, nonce RichText, Scoped CSS `[data-od-node-scope]`, `BlockTemplateCache`, `RenderBudget` toàn request, 5 core blocks nhúng sẵn. |
| **Slice S03 (Packages & Media ACL)** | `cms/crates/opendash-server/src/{theme.rs,app.rs}`, `plugin/{package.rs,manager.rs}`, `storage/{theme,plugin,builder,entries,overrides,page}.rs`, `routes/builder.rs` | Auto-discovery `blocks/<slug>/` trong Theme v3 & Plugin ZIP, `plugin_revisions.blocks_json`, resync Public ACL khi bật/tắt/đổi theme, `GET /api/admin/builder/blocks`. |
| **Slice S04 (Studio UI)** | `cms/admin/src/{types.ts,api/builder.ts}`, `features/media/MediaPicker.tsx`, `builder/{document.ts,BlockSchemaForm.tsx*,BuilderPreviewFrame.tsx,BuilderTree.tsx,BuilderCanvas.tsx,BuilderInspector.tsx,BuilderWorkspace.tsx}`, `studio/{StudioWorkspace.tsx,StudioCustomizerNav.tsx}`, `features/editor/{PanelRegionRenderer.tsx,SingleEditWorkspace.tsx}`, `storage/builder.rs#L1106,#L1111` | `<BlockSchemaForm>` 12 kiểu field, CSS data-URI Preview inliner, `MediaPicker` `'any'`, khóa `ProviderInactive`, đổi nhãn `"UX Builder"` sang **OpenDash Studio / Trình dựng trang**. |

### Slice S01: Core `BlockSchemaV1`, Reserved Package Grammar, Pure-Rust RichText & `validate_known`

- **Dependencies:** Đặc tả `Status: Approved` và `PRODUCT_REQUIREMENTS.md` §7 đã merge vào `main` (PR #470).
- **File Ownership:**
  - `cms/crates/opendash-core/src/lib.rs` — export `block_schema`, `richtext`
  - `cms/crates/opendash-core/src/id.rs` — dành riêng `"core"` và `"legacy"` trong `validate_package_id`, giữ `valid_namespaced_block_id` chuẩn `<= 32` mỗi phần
  - `cms/crates/opendash-core/src/richtext.rs` *(tạo mới)* — `sanitize_richtext(raw: &str) -> Result<String, &'static str>`, `strip_richtext_to_plain(raw: &str) -> String`, `decode_html5_entities` (dùng chung cho URL checks)
  - `cms/crates/opendash-core/src/block_schema.rs` *(tạo mới)* — `BlockSchemaV1`, `BlockFieldSpec`, `BlockCategory`, `MediaFieldKind`, `CssUnit`, `SelectOption`, `ShowIfScalar`, `ShowIfRule`, `RegisteredBlockSchema`, `valid_block_url`, `valid_block_color`, `BlockSchemaV1::validate`, `BlockSchemaV1::validate_values`, `BlockSchemaV1::collect_media_ids`, `upcast_legacy_descriptor`
  - `cms/crates/opendash-core/src/theme.rs` — giữ `Fields`, `BlockDescriptor`, `Block` (`#L10-L57`) tương thích; thêm adapter chuyển `BlockDescriptor` → `RegisteredBlockSchema { legacy_mode: true }`
  - `cms/crates/opendash-core/src/builder.rs` — cập nhật `validate_known` cho `KnownElement::PluginBlock` kiểm `ProviderStatus::Active && DeclarationState::Declared`, hàm `validate_for_save_with_schemas`, giữ shim `validate_for_save(..., &RegistryCatalog<BlockDescriptor>, ...)` qua `upcast_legacy_descriptor` để toàn workspace biên dịch sạch qua S01, áp dụng `is_preserved` chuẩn cho `ProviderInactive`/`Withdrawn`, lookup `previous_values` theo `(element.id, props.block_id)`
  - `cms/crates/opendash-core/src/builder_blocks.rs` — giữ nguyên comment tag `plugin_block`, bổ sung `extract_block_refs_lenient_with_schemas` (arm `"plugin_block"` chỉ lấy media từ block `Active + Declared` trong `schemas`) và `extract_visible_text_with_schemas`
- **Acceptance Criteria:**
  1. `validate_package_id("core")` và `validate_package_id("legacy")` bị từ chối.
  2. `BlockSchemaV1` kiểm đủ 12 kiểu field, chống shadow `"url"` / `*_url` (ngoài `Url`) và mọi `<media_key>_{url,alt,width,height}` cùng scope, giới hạn `<= 32` field thực, `<= 48` field lá, cấm `repeater` lồng `repeater`, cho phép `unit` UI hint khi không có `css_var`, đúng 6 `--od-token-*`, `valid_block_url` hỗ trợ bare `"#"`, `#fragment`, `/`, `http`, `https`, `mailto:`, `tel:`.
  3. `richtext::sanitize_richtext` loại RAWTEXT/RCDATA/foreign (`script`, `style`, `title`, `xmp`, `noembed`, `noframes`, `plaintext`, `iframe`, `svg`, `math`), giải mã HTML5 entities đầy đủ trước khi kiểm `href`, cấm `<a>` lồng `<a>`, xuất `rel="noopener noreferrer"` với `&<>"'` escaped, chặn input `> 64 KiB` và output `> 32 KiB`.
  4. `validate_for_save` cũ pass hết test; `validate_for_save_with_schemas` hỗ trợ `legacy_mode`, giữ `ProviderInactive`/`Withdrawn` qua `is_preserved`, bảo toàn unknown keys cũ và miễn `required` khi `show_if = false` hoặc field mới trên `(element.id, block_id)` / item `repeater` mới thêm sau nâng cấp.
  5. `opendash-core` unit tests PASS và `python cms/scripts/check-standards.py` PASS (0 crate ngoài).
- **Rollback:** Revert các file thuộc `opendash-core` và khôi phục dòng `§7` PRD.

---

### Slice S02: Migration `0046`, C-SEC Joint Checker & HTML5 Fixes, Scoped CSS, `RenderBudget` & 5 Core Blocks

- **Dependencies:** `Slice S01` merged vào `main`.
- **File Ownership:**
  - `cms/migrations/0046_declarative_block_engine.sql` *(tạo mới)* & `cms/migrations/README.md` — bảng `builder_block_definitions` (`CHECK (json_valid(schema_json))`) + `ALTER TABLE plugin_revisions ADD COLUMN blocks_json`
  - `cms/crates/opendash-server/src/app.rs` — thêm `block_template_cache: Arc<BlockTemplateCache>` (`std::sync::RwLock`, cap 256, `FromRef<AppState>`)
  - `cms/crates/opendash-server/src/template_sec.rs` — đóng comment HTML5 uniform (`-->`, `--!>`, `<!-->`, `<!--->`), thêm `title`/`xmp`/`noembed`/`noframes`/`plaintext` vào `FORBIDDEN_TAGS`, chống duplicate attribute trong `parse_attributes`, dùng `decode_html_entities_for_url` (cấm ASCII control chars sau decode) cả trong `!post_render` & kiểm `srcset`/`ping`, tách `validate_safe_href_url`, `validate_safe_scoped_block_css(css, is_preview)`, `validate_block_template_csec` (joint AST + `HtmlScanState`, cấm `OD_RICHTEXT_` và `Set`/`With`/`GetItem`/`Filter` trên `richtext`/`url`/`media`), và `render_block_csec_with_budget` (post-render single-pass `HtmlScanState::Body`)
  - `cms/crates/opendash-server/src/document.rs` — cập nhật `document::tokenize` (`#L221-L232`) đóng comment HTML5 (`-->`, `--!>`, `<!-->`, `<!--->`) đồng bộ với `template_sec.rs`
  - `cms/crates/opendash-server/src/builtin_blocks/{spacer,divider,banner,card,accordion}/{block.json,render.html}` *(tạo mới)* — 5 core blocks đầy đủ `category`/`icon`/`label` + `--od-banner-fx`/`--od-banner-fy`
  - `cms/crates/opendash-server/src/storage/page.rs` — `RegisteredBlockDef`, `load_unified_block_catalog_in_conn`, `ensure_embedded_core_blocks`, `sync_owner_blocks_in_tx`, `set_owner_block_status_in_tx` (`AND declaration_state = 'declared'`, `409 Conflict` cross-provider)
  - `cms/crates/opendash-server/src/page.rs` — `RenderBudget`, `BlockTemplateCache`, `serialize_scoped_block_css` với `[data-od-node-scope="{scope}-{node_id}"]` + `@media (width < 1024px)` + `@media (width < 768px)`, chạy `sanitize_richtext` trước khi tạo nonce, cô lập lỗi block (`plugin_block_render_error`), inject `<media_key>_url`, cập nhật 4 fixture `HomeState` bằng `..Default::default()`
  - `cms/crates/opendash-server/src/storage/{builder,overrides,public_content,restore}.rs` & `cms/crates/opendash-server/src/routes/{public_content,builder,themes}.rs` & `cms/crates/opendash-server/src/main.rs` — thêm `declared_blocks` vào `HomeState`, khởi tạo `BlockTemplateCache` tại `main.rs`, truyền chung 1 `RenderBudget` + `&BlockTemplateCache` xuyên suốt Entry Body (`"entry"`/`"home"`) -> Patterns (`"pat_{id}"`) -> Override (`"ov"`) -> Parts (`"hdr"`/`"ftr"`/`"tp_{i}"`) qua `PublicEntryRenderSnapshot` và `Public/PreviewCompositionBudget`, kiểm `sqlite_schema` < 256 và rehydrate pre-`0045` restore
  - `docs/contracts/M8_WRITER_INVENTORY.md` — bổ sung các writer của `builder_block_definitions`
- **Acceptance Criteria:**
  1. Khởi động DB tạo bảng `0045` (`sqlite_schema` < `256`), `restore.rs` tests pass, 5 core blocks nạp sạch lúc boot.
  2. `<!-- --!><script>...</script>`, `<!--><script>...</script>`, `<a href="javascript:..." href="https://ok">`, `/&sol;evil.com` (kể cả `!post_render`), `{% set rt = body %}` hoặc `{{ rt }}` trong attribute đều bị từ chối ở C-SEC + `document::tokenize`.
  3. `serialize_scoped_block_css` phát `[data-od-node-scope="{scope}-{node_id}"]` duy nhất qua Entry/Override/Header/Footer/Pattern + `@media (width < 1024px)` + `@media (width < 768px)` qua `validate_safe_scoped_block_css`.
  4. Một `RenderBudget` dùng chung `10_000` fuel & `512 KiB` HTML xuyên suốt cả Entry Body lẫn Template Override + Parts; 1 block lỗi fuel trả placeholder `plugin_block_render_error` không làm HTTP 500.
  5. Integration tests trong `cms/crates/opendash-server/tests/` PASS toàn bộ kịch bản trên và render đủ 5 core blocks.
- **Rollback:** Chưa phát hành migration ra ngoài dev; rollback bằng DB test mới và revert các file server S02.

---

### Slice S03: Package Auto-Discovery (`blocks/<slug>/`), Media ACL Sync/Resync & Admin Block Schema API

- **Dependencies:** `Slice S02` merged vào `main`.
- **File Ownership:**
  - `cms/crates/opendash-server/src/theme.rs` — mở `blocks/<slug>/{block.json,render.html}` (`slug <= 32`, `<= 32` block) trong `v3_entry_cap_and_allowlist` & `declared`, kiểm C-SEC và truyền theme catalog vào validation `patterns/*.json` (`#L259`)
  - `cms/crates/opendash-server/src/plugin/package.rs` & `docs/contracts/PLUGIN_ABI.md` — dùng `validate_v2_relative_path` + `seen_lower` + zip-ratio guard, skip `is_dir()` (`archive.len() <= 256`), `MAX_PLUGIN_FILES = 72` cho file thường (`manifest_schema` `1|2`)
  - `cms/crates/opendash-server/src/plugin/manager.rs` — truyền `blocks` từ `PluginPackage` vào `storage/plugin.rs` khi install/upgrade
  - `cms/crates/opendash-server/src/storage/theme.rs` — đọc & đối chiếu `ThemeRevisionRecord.file_digests` cho `blocks/` từ `themes/revisions/<digest>/` trong `activate_revision`, `Err(ApiError::conflict(...))` nếu trùng cross-provider, gọi `sync_owner_blocks_in_tx` + `resync_plugin_block_acls_in_tx`
  - `cms/crates/opendash-server/src/storage/plugin.rs` — giữ nguyên digest cho plugin 0 block; với plugin >= 1 block đưa `blocks` vào `compute_plugin_revision_digest_with_extensions`, staging `plugins/revisions/<digest>/blocks.json`, `verify_plugin_revision_durability`, ghi `plugin_revisions.blocks_json`; kiểm `409` cross-provider trước disk staging + gọi `sync_owner_blocks_in_tx` + `resync_plugin_block_acls_in_tx` trong `install_plugin_revision`; gọi `set_owner_block_status_in_tx` (`AND declaration_state = 'declared'`) + `resync_plugin_block_acls_in_tx` trong `set_plugin_enabled*`
  - `cms/crates/opendash-server/src/storage/{builder,entries,overrides,page}.rs` — `load_unified_block_catalog_in_conn` cho mọi transaction, trích xuất `strict_media_ids` từ block `Active + Declared` trên `validate_media_refs_in_tx` (`400`), `check_preview_images`, `save_and_render` (map `"any"` -> `builder_homepage_media` vs `builder_homepage_video_refs`), `replace_entry_builder_refs`, `sync_entry_body_refs_in_conn` (`422`) & `CompositePreflightPlan`, `refresh_override_media_refs_in_tx`, và `resync_plugin_block_acls_in_tx` bảo toàn `supports('builder')` + `body_allows_public_refs` (không chạm `entry_pattern_refs`, giữ delete-pin ở `media_refs::collect_pins_in_tx`)
  - `cms/crates/opendash-server/src/routes/builder.rs` & `cms/crates/opendash-server/src/app.rs` — thêm `GET /api/admin/builder/blocks` (`require_session`, `no-store`, DTO `{ items }`, không trả `template`), giữ `GET /registry`
- **Acceptance Criteria:**
  1. Theme v3 & Plugin ZIP chứa `blocks/<slug>/{block.json,render.html}` (`<= 32` blocks, `owner_id` & `slug <= 32`) nạp & đăng ký tự động; chiếm `"core"`/`"legacy"` hoặc `id` > 32 bị từ chối `400`, trùng provider khác trả `409`.
  2. Media trong `PluginBlock` (gồm `repeater` & `media_kind = "any"`): `/media/{id}/file` trả **`404`** khi `draft`, **`200`** khi `published` (nếu `body` là Public), bị chặn **`409 Conflict`** khi xóa ngay cả khi plugin tắt.
  3. Tắt plugin / đổi theme thu hồi công khai tức thì (`404`) via `resync_plugin_block_acls_in_tx`, bật lại phục hồi `200`, entry `body` private không bao giờ rò rỉ ACL khi resync.
  4. `GET /api/admin/builder/blocks` trả chuẩn `BlockCatalogItemDto[]` trong `{ items }` và `check-standards.py` PASS.
- **Rollback:** Revert S03; hệ thống chỉ chạy 5 core blocks từ S02.

---

### Slice S04: Studio `<BlockSchemaForm>`, CSS Data-URI Preview, MediaPicker `'any'` & Official Naming Cutover

- **Dependencies:** `Slice S03` merged vào `main`.
- **File Ownership:**
  - `cms/admin/src/types.ts` & `cms/admin/src/api/builder.ts` — `BlockSchemaV1`, `BlockFieldSpec`, `BlockCatalogItemDto`, `BlockPropValue`, `getBlockSchemas()`
  - `cms/admin/src/features/media/MediaPicker.tsx` — mở rộng `acceptKind?: MediaKind | 'any'`, tab filter nội bộ `'all' | 'image' | 'video'` (mapping `'all'` -> `undefined` ở `listMedia`), `<input accept>` & upload router MP4/Ảnh
  - `cms/admin/src/builder/document.ts` — mở khóa `'plugin/block'` trong `isLockedElement` khi `status === 'active' && declaration_state === 'declared'`, cập nhật `isProtectedNode`, `preservesAnchors`, `nodeTitle`, `appendNode(..., options?: { blockSchema })`, và `setNodeField` hỗ trợ `props.values` + `BlockPropValue`
  - `cms/admin/src/builder/BlockSchemaForm.tsx` *(tạo mới)* — UI 12 kiểu field + `<RichTextFieldControl>` + `show_if` + breakpoint toggle (`desktop`/`tablet`/`mobile`) + `repeater` Thêm/Nhân bản/Xóa/Lên/Xuống
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx` — inline `url("/api/admin/media/{64hex}/file")` thành `data:` URL & rewrite `url("/media/...")` trong `style[data-od-block-styles]`
  - `cms/admin/src/builder/{BuilderTree,BuilderCanvas,BuilderInspector,BuilderWorkspace}.tsx` & `cms/admin/src/studio/{StudioWorkspace,StudioCustomizerNav}.tsx` — nạp `getBlockSchemas()` (Studio + `#/builder`, reload khi `themeRevision`/plugin đổi), palette 6 category, nối `<BlockSchemaForm>`, khóa chỉ đọc khi `ProviderInactive`/`Withdrawn`
  - `cms/admin/src/features/editor/{PanelRegionRenderer,SingleEditWorkspace}.tsx` & `cms/crates/opendash-server/src/storage/builder.rs#L1106,#L1111` — đổi `"UX Builder"` sang **"OpenDash Studio"** / **"Trình dựng trang"** (giữ `data-testid="open-ux-builder-toolbar"`)
- **Acceptance Criteria:**
  1. Studio `#/studio` và `#/studio/entry/page/<id>` chèn đủ 5 core blocks + theme/plugin blocks; `<BlockSchemaForm>` hoạt động đủ 12 kiểu field, `show_if`, `responsive`, `<MediaPicker acceptKind="any">`, và `repeater`.
  2. Live Preview hiển thị đúng ảnh nền CSS (`data:` URI) và chuyển đổi viewport `Desktop`/`Tablet`/`Mobile` khớp `@media (width < 1024px)` / `(width < 768px)`.
  3. Block `ProviderInactive`/`Withdrawn` bị khóa sửa ở Inspector (chỉ đọc hoặc xóa).
  4. Không còn chuỗi `"UX Builder"` hiển thị trong `cms/admin/src/` hay `storage/builder.rs#L1106,#L1111`; `typecheck` & `vite build` PASS.
- **Rollback:** Revert các file `cms/admin/src/` và 2 chuỗi ở `storage/builder.rs`, build lại `cms/admin/dist/`.

---

### Verification Plan

Sau mỗi slice (và trước khi đóng sang `Verified`), chạy đầy đủ các lệnh từ root repository:

```powershell
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

Kèm kiểm chứng trình duyệt trực tiếp trên server cục bộ (`http://127.0.0.1:18766/admin/`): tạo Page mới chứa `core/banner` (`bg_media`, `min_height` khác nhau giữa `Desktop` 1280px và `Mobile` 390px, mô tả `richtext`) và `core/accordion` (`repeater` 3 mục), xác nhận Live Preview trong OpenDash Studio, kiểm `/media/{id}/file` trả **`404`** khi còn `draft` và **`200`** sau khi **Xuất bản**, kiểm xóa media trong `repeater` trả **`409 Conflict`**, tắt plugin kiểm node `ProviderInactive` giữ nguyên được khi lưu mục khác (và media thu hồi về **`404`**), với `0` console error / `0` 4xx-5xx ngoài các case âm tính chủ đích.
