# OpenDash — Hợp đồng Hệ kiểu Trường Thống nhất (`UNIFIED_FIELD_SCHEMA` V1)

- **Phiên bản:** `1.0.0` (`Clean-Slate V1` — hướng tới phát hành V1.0 đầu tiên; mọi mã nguồn/schema tiền-V1 cũ đều bị loại bỏ hoàn toàn thay vì đánh nhánh phiên bản kế thừa; đã gộp đầy đủ `spacing.property`, `select.options_source`, `term_ref.taxonomy_from_field`, `entry_ref.target_types: ["*"]` và bỏ `bindable` / `storage/builder_binding.rs` theo quyết định D1=1B của Luồng A)
- **Trạng thái:** `AGREED` *(Chủ dự án phê duyệt 2026-11-26)* · `not_yet_implemented` *(hợp đồng giao diện bắt buộc trước khi triển khai mã nguồn ở các Design Spec EGM liên quan)*
- **Tài liệu liên quan:** [Yêu cầu sản phẩm V1](../product/PRODUCT_REQUIREMENTS.md) · [Core Foundation](CORE_FOUNDATION.md) · [Plugin ABI](PLUGIN_ABI.md) · [M8 Writer Inventory](M8_WRITER_INVENTORY.md) · [Design Sitemap](../designs/SITEMAP.md)

---

## 1. Mục đích & Nguyên tắc Clean-Slate V1

Tài liệu này là **Nguồn chân lý duy nhất (Single Source of Truth)** cho toàn bộ hệ thống kiểu trường dữ liệu (`FieldSpec` / `FieldType`), điều kiện ẩn/hiện (`conditions`), quy ước lưu trữ JSON, quy ước hydrate dữ liệu ra template MiniJinja (`C-SEC`) và mô hình cấu trúc nội dung 3 trụ cột kiểu ACF trên **OpenDash CMS V1**.

**Nguyên tắc bắt buộc (Clean-Slate V1 — Zero Legacy Shim):**

1. **Một hệ kiểu trường duy nhất dùng chung cho cả 4 bề mặt:**
   - `Content` — Custom Fields của CPT Entries (qua Field Groups).
   - `Term` — Custom Fields của mọi Taxonomy Term (`category`, `tag` và mọi Custom Taxonomy, qua Field Groups).
   - `Options` — Global Site Options, Custom Options Pages và Theme Options (`theme.json`).
   - `OwnerSettings` — Cài đặt của Plugin (`plugin.json -> settings_schema`) và Theme Functions (`theme.json -> functions.settings_schema`).
   - `StudioBlock` — Khai báo thuộc tính block của OpenDash Studio (`blocks/<slug>/block.json`).
2. **Không mang nợ tương thích ngược với các schema nháp trước V1:** Vì OpenDash chưa phát hành bản CMS hay Theme V1 công khai, **cấm** duy trì alias tên kiểu cũ (`short_text`, `long_text`, `rich_text`, `string`, `image`, `image_ref`, `choices`), dual parser, wrapper giả lập `BlockSchemaV1`, hay phân hệ field tách rời (`ContentFieldType`, `ThemeOptionType`, `PluginSettingFieldType`, `TermFieldType`). Mọi tài nguyên nội bộ trong `cms/` (`builtin_blocks`, `opendash-starter`, `examples`, `fixtures`, tests) được cắt chuyển đồng bộ về chuẩn V1 này.
3. **Một lõi Rust + một Form Engine React:**
   - Lõi domain thuần Rust (không I/O, không SQL, không HTTP) đặt tại `opendash_core::field_schema`.
   - Giao diện nhập liệu hợp nhất đặt tại `cms/admin/src/fields/` (`<UnifiedFieldForm>` + registry controls) thay cho `FieldValueInput.tsx`, `AdminTermPanels` và wrapper `OwnerOptionsForm.tsx` cũ.
4. **Phân biệt rõ `group` (cấu trúc dữ liệu) và `tab`/`accordion`/`message` (bố cục giao diện):**
   - `group` **luôn lưu dưới dạng nested JSON object** `{ ... }` thống nhất trên mọi bề mặt (kể cả `StudioBlock`).
   - `tab`, `accordion`, `message` là **UI-only layout primitives**: không lưu dữ liệu vào JSON, dùng để chia tab/khối gập/ghi chú mà vẫn giữ khóa phẳng cho các field bên trong.
5. **Bỏ biến hậu tố phẳng trong template:** Không còn cơ chế phát sinh biến hậu tố `<key>_url`, `<key>_alt`, `<key>_target`, `<key>_classes` và bảng cấm hậu tố. Mọi bề mặt dùng **cấu trúc object đã hydrate đồng nhất** (xem §6).

---

## 2. Danh mục Kiểu Trường Chuẩn (25 Kiểu Dữ liệu + 3 Layout UI-Only)

### 2.1. Bảng kiểu chuẩn & Ma trận khả dụng theo Context (`FieldContextCaps`)

Mỗi `FieldType` có đúng **1 tên `snake_case` chuẩn duy nhất**:

| Nhóm | `type` chuẩn | Giá trị lưu trong JSON | `Content` & `Term` | `Options` | `OwnerSettings` | `StudioBlock` | Cấu hình đặc thù |
|---|---|---|---|---|---|---|---|
| **1. Basic** | `text` | `string` | ✅ | ✅ | ✅ | ✅ | `max_chars` (mặc định `512`, tối đa `2000`), `placeholder`, `prepend`, `append` |
| | `textarea` | `string` | ✅ | ✅ | ✅ | ✅ | `rows` (`2..=16`), `max_chars` (mặc định `4000`, tối đa `100_000`), `placeholder` |
| | `number` | `number` (hữu hạn) | ✅ | ✅ | ✅ | ✅ *(resp, css, unit)* | `min`, `max`, `step`, `prepend`, `append`, `unit` *(Block)* |
| | `range` | `number` (hữu hạn) | ✅ | ✅ | ✅ | ✅ *(resp, css, unit)* | `min` *(bắt buộc)*, `max` *(bắt buộc)*, `step`, `unit` *(Block)* |
| | `email` | `string` | ✅ | ✅ | ✅ | ✅ | Validate cú pháp email an toàn, `max_chars <= 254`, `placeholder` |
| | `url` | `string` | ✅ | ✅ | ✅ | ✅ | Validate URL an toàn (`http(s)://`, `/path`, `#anchor`, `mailto:`, `tel:`, tối đa `2048` bytes), `placeholder` |
| | `secret` | Write-only `SecretStore` ref | ❌ | ❌ | ✅ | ❌ | Không cho `default` khác `null`, không bao giờ trả plaintext ra API/template |
| **2. Content & Media** | `richtext` | Sanitized HTML (`string`) | ✅ | ✅ | ❌ | ✅ | `max_bytes` (tối đa `32_768` qua `sanitize_richtext`; riêng core `body` hỗ trợ comment-block payload), `toolbar: "minimal" \| "standard"` |
| | `media` | `string` (SHA-256 64-hex `media_id`) | ✅ | ✅ | ✅ | ✅ *(css khi `image`)* | `media_kind: "image" \| "video" \| "file" \| "any"` (mặc định `"image"`), tự động pin `media_refs` |
| | `gallery` | `string[]` (mảng `media_id` duy nhất) | ✅ | ✅ | ❌ | ✅ | `media_kind: "image" \| "video" \| "any"` (mặc định `"image"`), `min_items`, `max_items` (`<= 50`) |
| | `link` | `{ url, label, new_tab, nofollow }` | ✅ | ✅ | ❌ | ✅ | `url` (validated), `label` (`<= 160` ký tự), `new_tab: bool`, `nofollow: bool`, `placeholder` |
| **3. Choice** | `select` | `string` hoặc `string[]` (`multiple: true`) | ✅ | ✅ | ✅ | ✅ *(resp, css khi single tĩnh; `options_source`)* | `options: SelectOption[]` (`1..=100`, hoặc bỏ trống khi dùng `options_source` ở `StudioBlock`), `multiple: bool`, `allow_null: bool`, `display: "dropdown" \| "buttons"` |
| | `checkbox` | `string[]` (giá trị duy nhất) | ✅ | ✅ | ✅ | ✅ | `options: SelectOption[]` (`1..=100`), `min_items`, `max_items`, `layout: "vertical" \| "horizontal"` |
| | `radio_buttons` | `string` | ✅ | ✅ | ✅ | ✅ *(resp, css)* | `options: SelectOption[]` (`1..=12`, hỗ trợ `icon`), `layout: "buttons" \| "radio"` |
| | `boolean` | `bool` | ✅ | ✅ | ✅ | ✅ | Render `ToggleSwitchControl`, tùy chọn `on_label`, `off_label` |
| **4. Visual & Date/Time** | `color` | `string` | ✅ | ✅ | ❌ | ✅ *(css)* | `#RGB`/`#RRGGBB`/`#RRGGBBAA`, `transparent`, `currentColor`, hoặc 6 semantic tokens `--od-token-*` |
| | `icon` | `string` (kebab icon key) | ✅ | ✅ | ❌ | ✅ | Chỉ nhận khóa hợp lệ trong curated registry `opendash_core::icons::CORE_ICONS` |
| | `date` | `string` (`YYYY-MM-DD`) | ✅ | ✅ | ❌ | ✅ | ISO-8601 date, `min_date`, `max_date` |
| | `datetime` | `string` (ISO-8601 UTC) | ✅ | ✅ | ❌ | ✅ | Lưu UTC chuẩn (`YYYY-MM-DDTHH:MM:SSZ`), hiển thị theo Site Timezone (`jiff`) |
| | `time` | `string` (`HH:MM` hoặc `HH:MM:SS`) | ✅ | ✅ | ❌ | ✅ | Giờ chuẩn 24h |
| | `spacing` | `{ top?, right?, bottom?, left? }` | ❌ | ✅ | ❌ | ✅ *(resp, css, unit)* | `property?: "padding" \| "margin"` (mặc định `"padding"`), `min` (`0..=256`, mặc định `0` khi `"padding"`; cho phép giá trị âm `-256..=256`, mặc định `-128`, khi `"margin"`), `max` (`<= 256`, mặc định `256`), `unit` (`CssUnit`, mặc định `px` + Tailwind scale sinh `p-*`/`m-*`/`-m-*`) |
| **5. Relational** | `entry_ref` | `i64` hoặc `i64[]` (`multiple: true`) | ✅ | ✅ | ❌ | ✅ | `target_types: string[]` (`1..=16`, hoặc `["*"]` duy nhất ở `StudioBlock`), `multiple: bool`, `min_items`, `max_items` (`<= 50`), lọc theo `locale` |
| | `term_ref` | `i64` hoặc `i64[]` (`multiple: true`) | ✅ | ✅ | ❌ | ✅ | `taxonomy?: string` (hoặc `taxonomy_from_field?: string` duy nhất ở `StudioBlock`, loại trừ lẫn nhau với `taxonomy`), `multiple: bool`, `picker_style`, `sync_entry_terms: bool` *(chỉ `Content`)* |
| **6. Structural** | `group` | Nested JSON Object `{ [sub_key]: Value }` | ✅ | ✅ | ✅ | ✅ | `fields: FieldSpec[]` (không rỗng), lưu lồng dưới `key` của group |
| | `repeater` | `Array<Record<string, Value>>` | ✅ | ✅ | ❌ | ✅ | `fields: FieldSpec[]` (không rỗng), `min_items`, `max_items`, `item_label_field`, `layout: "accordion" \| "table" \| "block"` |
| **7. UI-Only Layout** | `tab` | *(Không lưu dữ liệu)* | ✅ | ✅ | ✅ | ✅ | Mở một tab giao diện mới cho các field kế tiếp cùng cấp (`placement: "top" \| "left"`). Dùng trong Studio để chia `Nội dung` / `Giao diện` / `Nâng cao` |
| | `accordion` | *(Không lưu dữ liệu)* | ✅ | ✅ | ✅ | ✅ | Mở nhóm gập/mở giao diện (`open_by_default: bool`, `multi_expand: bool`); kết thúc bởi `accordion`/`tab` tiếp theo hoặc `endpoint: true` |
| | `message` | *(Không lưu dữ liệu)* | ✅ | ✅ | ✅ | ✅ | Hộp văn bản hướng dẫn/cảnh báo tĩnh (`body: string`, `tone: "info" \| "warning" \| "neutral"`), không chứa HTML |

---

## 3. Cấu trúc JSON Chuẩn của Một `FieldSpec`

Mọi `FieldSpec` (dù viết tay trong `block.json` / `theme.json` / `plugin.json` hay tạo qua Field Group Builder UI) đều dùng chung **1 cấu trúc phẳng gọn**, được Admin UI chia thành **4 tab chuẩn ACF** (`General` · `Validation` · `Presentation` · `Conditional Logic`):

```json
{
  "key": "cta_button",
  "label": "Nút kêu gọi hành động",
  "type": "link",
  "required": false,
  "default": { "url": "", "label": "Tìm hiểu thêm", "new_tab": false, "nofollow": false },
  "instructions": "Đường dẫn nội bộ (/lien-he) hoặc URL https:// đầy đủ.",
  "placeholder": "/lien-he",
  "width": 50,
  "conditions": [
    [
      { "field": "show_cta", "op": "eq", "value": true }
    ]
  ]
}
```

### 3.1. Nhóm thuộc tính dùng chung (mọi Context)

- **Định danh & Kiểu (General):**
  - `key`: `snake_case` (`^[a-z][a-z0-9_]{0,63}$`, `1..=64` ký tự; UI-only `tab`/`accordion`/`message` cũng dùng khóa duy nhất trong scope để React keying ổn định).
  - `label`: chuỗi không rỗng, tối đa `100` ký tự, không ký tự điều khiển.
  - `type`: một trong 28 giá trị ở §2.1.
  - `default`: tùy chọn, phải hợp lệ với kiểu dữ liệu (cấm khác `null` với `secret`).
- **Ràng buộc (Validation):**
  - `required: bool` (mặc định `false`).
  - Các giới hạn theo kiểu: `min`, `max`, `step`, `min_chars`, `max_chars`, `max_bytes`, `min_items`, `max_items`, `min_date`, `max_date`.
- **Hiển thị Form (Presentation):**
  - `instructions?: string` (tối đa `400` ký tự, văn bản thuần).
  - `placeholder?: string` (tối đa `120` ký tự).
  - `prepend?: string`, `append?: string` (tối đa `32` ký tự, áp dụng cho `text`, `number`, `email`, `url`).
  - `width?: 25 | 33 | 50 | 66 | 75 | 100` (lưới cột trên Admin form; mặc định `100`; trong sidebar hẹp của Studio tự động xếp 100% trừ khi container đủ rộng).
- **Điều kiện ẩn/hiện (`conditions` — DNF: OR của các nhóm AND):**
  - Kiểu: `Vec<Vec<ConditionRule>>`. Mảng ngoài là `OR`, mảng trong là `AND`.
  - Mỗi `ConditionRule`: `{ "field": "<key>", "op": "<op>", "value"?: <scalar | array> }`.
  - Danh sách toán tử `op`: `eq`, `neq`, `empty`, `not_empty`, `contains`, `gt`, `lt`, `in`.
  - Quy tắc phạm vi: `field` chỉ được tham chiếu field dữ liệu cùng scope (hoặc top-level khi đang ở trong `repeater`/`group`); cấm tự tham chiếu, cấm chu trình, cấm tham chiếu `secret`, `spacing`, `repeater`, `group` hoặc field có `responsive: true`.
  - **Hành vi khi bị ẩn:** Field đang ẩn theo `conditions` sẽ **không** bị bắt buộc `required` lúc validate lưu, và **không bị xóa giá trị cũ** trong DB (bảo toàn dữ liệu khi người dùng bật/tắt điều kiện).

### 3.2. Thuộc tính mở rộng theo từng Context (`deny_unknown_fields` theo Context)

| Context | Thuộc tính được phép thêm trên `FieldSpec` | Ghi chú kiểm biên |
|---|---|---|
| **`Content` & `Term`** | `acl?: { min_view_role?: "editor" \| "admin", min_edit_role?: "editor" \| "admin", public_exposure?: "public" \| "private" }`, `translatable?: bool`, `show_in_list?: bool`, `declaration_state?: "declared" \| "withdrawn"` | Mặc định `min_view_role = editor`, `min_edit_role = editor`, `public_exposure = public` cho User Field Group (và `private` mặc định cho Theme/Plugin extension trừ khi khai báo tường minh); cấm `min_edit_role < min_view_role`; cấm `public_exposure == public` khi `min_view_role == admin`. |
| **`Options`** | `public?: bool` (mặc định `false`), `translatable?: bool` (mặc định `false`) | `translatable: true` được phép trên mọi kiểu nội dung (`text`, `textarea`, `richtext`, `url`, `link`, `media`, `gallery`, `group`, `repeater`…). `public: true` cho phép C-SEC đọc giá trị ra public template & `ThemeUrlPolicy`. |
| **`OwnerSettings`** | `secret` field type (không cho `public`, `responsive`, `css_var`, `acl`) | Giá trị `secret` đi qua `SecretStore` write-only, chỉ đọc được khi manifest khai báo `permissions.plugin_secrets_read`. |
| **`StudioBlock`** | `responsive?: bool`, `css_var?: string` (`^--od-[a-z0-9-]{1,48}$`), `unit?: "px" \| "rem" \| "em" \| "%" \| "vw" \| "vh"`, `options_source?: "menus" \| "menu_slots" \| "contact_forms" \| "content_types" \| "taxonomies" \| "context_fields" \| "template_parts" \| "patterns"` *(chỉ trên `select`)*, `taxonomy_from_field?: string` *(chỉ trên `term_ref`)*, `target_types: ["*"]` *(wildcard duy nhất trên `entry_ref`)* | `responsive` chỉ bật ở top-level cho `number`, `range`, `select` (single tĩnh), `radio_buttons`, `spacing`; lưu `{ desktop?, tablet?, mobile? }`, kế thừa `desktop → tablet → mobile`. `css_var` cấm đặt trong `repeater` và không trùng tên trong cùng 1 block. Với `select` dùng `options_source`: **loại trừ lẫn nhau** với `options` tĩnh, **cấm** kết hợp `responsive: true` / `css_var` / đặt trong `repeater`, cho phép `null`/rỗng khi chưa có tài nguyên, và server kiểm tra hợp lệ khi lưu. Với `term_ref` dùng `taxonomy_from_field`: **loại trừ lẫn nhau** với `taxonomy` tĩnh, cấm đặt trong `repeater`, phải tham chiếu một field `select` đơn không-responsive cùng scope/top-level (ví dụ `select` có `options_source: "taxonomies"`). Với `entry_ref` dùng `target_types: ["*"]`: chỉ hợp lệ ở `StudioBlock` với mảng đúng 1 phần tử `"*"` (cấm trộn cùng CPT cụ thể), cho phép chọn mọi CPT `active` khác `pattern`. |

---

## 4. Giới hạn Bảo mật, Quy mô & Bảo toàn Dữ liệu

Mọi bề mặt đều tuân thủ các giới hạn an toàn bắt buộc của OpenDash:

1. **Độ sâu lồng cấu trúc:**
   - `repeater`: hỗ trợ độ sâu **1 cấp** (bên trong `repeater` được chứa các kiểu dữ liệu nguyên thủy và tối đa 1 `group` phẳng; **cấm** lồng `repeater` trong `repeater` ở V1).
   - `group`: độ sâu tối đa **2 cấp** (`group` trong `group` hoặc `group` trong `repeater`).
   - Giới hạn phần tử: `repeater` có `max_items <= 50` trên `Content`/`Term`/`Options` và `<= 32` trên `StudioBlock` (tổng `max_items` của các `repeater` trên 1 block `<= 64`).
   - Giới hạn trường: tối đa `64` field dữ liệu mỗi Field Group / Options schema; mỗi `StudioBlock` tối đa `32` top-level data fields và `48` leaf fields (kể cả bên trong `group`/`repeater`), dung lượng `props.values` tối đa `64 KiB`.
2. **Bảo vệ Media & ACL (`media_refs`):**
   - Mọi `media_id` nằm trong `media` và `gallery` ở bất kỳ độ sâu nào (`group`, `repeater`) trên `content_entries.values_json`, `taxonomy_terms.values_json`, `theme_option_values`, `site_global_option_values`, `custom_option_values`, `template_overrides` và `entry_builder_documents` đều được pin chống xóa trong `storage/media_refs.rs`.
   - Public Media ACL **chỉ** cấp quyền tải công khai cho `media_id` thuộc field có `public_exposure == public` (hoặc option `public == true`) của bản ghi đã `published` thuộc CPT/Taxonomy `public == true` và `locale` đang bật.
3. **An toàn Relational (`entry_ref` / `term_ref`):**
   - Trên đường public, mọi tham chiếu đều được batch-load Zero-N+1 (`json_each`), lọc cứng `publication_status = 'published'`, `public = 1`, `provider_status = 'active'` và `locale` đang bật. Tuyệt đối không rò bản nháp, CPT riêng tư hay locale đã tắt.
4. **An toàn chuyển đổi Schema (`COMPAT-1..4`):**
   - Không cho đổi `type` phá dữ liệu hoặc xóa cứng field khi đã có bản ghi dùng dữ liệu đó — chuyển sang `declaration_state = withdrawn` để bảo toàn 100% giá trị và tham chiếu media.

---

## 5. Hợp đồng Content Structure (Mô hình ACF 3 Trụ cột)

OpenDash áp dụng cấu trúc 3 thực thể ngang hàng dưới `Hệ thống → Cấu trúc nội dung`:

1. **Content Types (`content_types`):**
   - Định nghĩa thực thể & hành vi: `key`, `label` (plural), `singular_label`, `description`, `public`, `has_archive`, `publication` (`draft_publish | immediate`), `hierarchical` (`bool`, tối đa 8 cấp), `supports` (`editor`, `featured_image`, `builder`, `excerpt`, `menu_order`, `custom_template`), `menu_name`, `menu_icon` (từ `CORE_ICONS`), `menu_position`, `show_in_menu`, `list_columns`, `archive_per_page`, `default_sort`, `exclude_from_sitemap`, `rewrite_base` (đồng bộ nguyên tử qua `storage/permalinks.rs`).
   - **Không nhúng `Base Fields` bên trong CPT** và **bỏ `base_field_panels` (`layout:*`)**: các trường nội dung tích hợp (`body`, `featured_image`, `excerpt`) đi theo `supports`; mọi trường tùy chỉnh khác đều quản lý tập trung qua **Field Groups**.
   - User CPT hỗ trợ đủ `POST`, `PUT`, và `DELETE` (khi không còn entry).
2. **Taxonomies (`taxonomy_definitions` & `taxonomy_terms`):**
   - Definition: `key`, `label`, `singular_label`, `description`, `hierarchical`, `public`, `selection_mode` (`multiple | single`), `picker_style` (`auto | checklist | tags | select | radio`), `required_on_publish`, `default_term_id`, `editor_can_create_terms`, `show_in_menu`, `show_admin_column`, `rewrite_base`.
   - Term Entity: `name`, `slug`, `parent_id`, `description`, `sort_order`, `featured_image_media_id`, `values_json` (Term Custom Fields từ Field Groups khớp `taxonomy_term`), và `term_seo` — nhập được ở cả tạo mới lẫn chỉnh sửa.
3. **Field Groups (`content_field_groups`) & Location Rules (DNF):**
   - Một Field Group mang `group_id` (`ScopedResourceId`), `title`, `description`, `active`, `presentation` (`region`, `order`, `style: "card" | "seamless"`, `collapsible`, `collapsed_by_default`, `label_placement: "top" | "left"`), `location: Vec<Vec<LocationRule>>` (OR của các nhóm AND), và `fields: Vec<FieldSpec>`.
   - Các `param` hỗ trợ trong `LocationRule`:
     - *Schema-level:* `content_type`, `taxonomy_term`, `options_page` (`site_global`, `theme:<id>`, `custom:<key>`).
     - *Entry/Actor-context:* `page_layout`, `custom_template`, `special_page` (`homepage | posts_page`), `entry_taxonomy`, `entry_id`, `user_role`.
   - Khi nhiều Field Group có khả năng cùng khớp một `content_type` hoặc `taxonomy_term`, hệ thống cưỡng chế tính duy nhất của `field.key` (Rule `COMPAT-1`). Nhóm ẩn do ngữ cảnh entry không bắt buộc `required` và không xóa dữ liệu đã lưu.

---

## 6. Hợp đồng Template Hydration (MiniJinja `C-SEC`)

Mọi bề mặt dùng chung **1 quy tắc hydrate chuẩn (Zero-N+1 batching)**:

| `FieldType` | Dạng biến xuất vào MiniJinja (`C-SEC`) | Ví dụ sử dụng trong `render.html` / Theme |
|---|---|---|
| `text`, `textarea`, `email`, `url`, `color`, `date`, `datetime`, `time`, `select` *(single)*, `radio_buttons` | `string` (hoặc `null`) | `{{ title }}`, `{{ published_date \| date_format("%d/%m/%Y") }}` |
| `number`, `range` | `number` | `{{ price \| format_number }}` |
| `boolean` | `bool` | `{% if show_badge %}...{% endif %}` |
| `richtext` | Safe HTML hậu-render qua `RichTextNonceTable` (không dùng `\|safe`) | `<div class="od-prose">{{ content }}</div>` |
| `media` | `{ id, url, thumbnail_url, alt, width, height, kind }` *(hoặc `null`)* | `<img src="{{ image.url }}" alt="{{ image.alt }}" width="{{ image.width }}" height="{{ image.height }}">` |
| `gallery` | `Array<{ id, url, thumbnail_url, alt, width, height, kind }>` | `{% for img in slides %}<img src="{{ img.url }}" alt="{{ img.alt }}">{% endfor %}` |
| `link` | `{ url, label, target, rel, new_tab, nofollow }` | `<a href="{{ cta.url }}"{% if cta.target %} target="{{ cta.target }}"{% endif %}{% if cta.rel %} rel="{{ cta.rel }}"{% endif %}>{{ cta.label }}</a>` |
| `icon` | `{ name }` *(hoặc `null`)* — dùng với `od_icon()` | `{% if icon %}{{ od_icon(icon.name, "w-5 h-5") }}{% endif %}` |
| `select` *(multiple)*, `checkbox` | `string[]` | `{% for feature in features %}{{ feature }}{% endfor %}` |
| `spacing` | `{ top, right, bottom, left, css, classes }` | `<div class="{{ padding.classes }} {{ margin.classes }}">` *(sinh `p-*` khi `property: "padding"`, `m-*`/`-m-*` khi `property: "margin"`)* |
| `entry_ref` | `{ id, type_key, title, slug, url, cover_url, excerpt }` *(hoặc `Array<...>` khi `multiple`)* | `{% for post in featured_posts %}<a href="{{ post.url }}">{{ post.title }}</a>{% endfor %}` |
| `term_ref` | `{ id, kind, name, slug, url, featured_image }` *(hoặc `Array<...>` khi `multiple`)* | `{% for cat in categories %}<a href="{{ cat.url }}">{{ cat.name }}</a>{% endfor %}` |
| `group` | Object lồng với mọi sub-field đã hydrate | `{{ hero.heading }}`, `<img src="{{ hero.banner.url }}">` |
| `repeater` | Mảng object với mọi sub-field đã hydrate | `{% for faq in faqs %}<h3>{{ faq.question }}</h3><div>{{ faq.answer }}</div>{% endfor %}` |

**Vị trí gắn biến trên từng loại template:**

- **Studio Block (`blocks/<slug>/render.html`):** Các khóa top-level của block nằm trực tiếp ở root context (`{{ heading }}`, `{{ image.url }}`, `{{ cta.url }}`, `{{ hero.title }}`).
- **Entry Template (`single*.html`, `archive*.html`, `page.html`):** `entry.fields.<key>` chứa toàn bộ custom field đã hydrate (chỉ `public_exposure == public`); `entry.field_list` là mảng có thứ tự phục vụ fallback template chung.
- **Taxonomy Template (`taxonomy*.html`):** `term.description`, `term.featured_image` (`{ id, url, thumbnail_url, alt, width, height, kind }`), và `term.fields.<key>`.
- **Theme & Global/Custom Options:** `options.<key>` (Theme Options `public: true`), `site_options.<key>` (Global Site Options), và `custom_options.<page_key>.<key>` (Custom Options Pages).
- **C-SEC URL Provenance:** `template_sec.rs` kiểm tra tĩnh nguồn gốc thuộc tính URL/Media theo đường dẫn đầy đủ trên schema (ví dụ `image.url`, `cta.url`, `options.site_logo.url`, `entry.fields.cover.url`), tiếp tục chặn nội suy URL từ trường `text` tùy ý.
- **Template Blocks theo ngữ cảnh (không còn `ElementBinding`):** Theo quyết định D1=1B của Luồng A, cơ chế `ElementBinding` & `storage/builder_binding.rs` cũ được loại bỏ ở Spec A1 và thay bằng các Template Block độc lập theo `allowed_contexts` dùng chung bộ `FieldSpec` và hydration chuẩn ở mục này.

---

## 7. Phân chia Ranh giới Song song giữa các Luồng Thiết kế & Triển khai

Để các luồng **Builder Blocks** và **Content Structure (ACF)** làm việc độc lập mà không xung đột:

| Luồng / Giai đoạn | File Ownership chính | Điều kiện phối hợp |
|---|---|---|
| **Đợt 1 — Unified Field Engine *(Nền chung — merge trước)*** | `opendash-core/src/field_schema.rs`, `block_schema.rs`, `opendash-server/src/{page.rs,template_sec.rs,content_render.rs,storage/{settings,media_refs}.rs}`, `cms/admin/src/fields/`, cập nhật `7` builtin blocks + `opendash-starter` + examples/fixtures | Hiện thực hóa §2, §3, §4, §6; xóa bỏ 4 hệ field cũ. |
| **Luồng A — Builder Blocks *(Độc lập sau Đợt 1; thiết kế song song ngay)*** | `builtin_blocks/<new-block>/{block.json,render.html}`, theme blocks/patterns, Studio Block Catalog & Canvas UX | Mọi `block.json` & `render.html` mới tuân thủ đúng §2, §3, §6 của hợp đồng này. |
| **Luồng B — Content Structure ACF *(Đợt 2 & Đợt 3 — Độc lập với Luồng A)*** | `opendash-core/src/content.rs`, `migrations/0057+*`, `storage/{content,taxonomy,entries,permalinks}.rs`, `routes/{content,taxonomy}.rs`, `cms/admin/src/features/{content,taxonomy,editor}/` | Hiện thực hóa §5 (CPT v2, Taxonomies v2, Field Groups + Location Rules, Term Meta, Relational `entry_ref`/`term_ref`, Custom Options Pages, Hierarchical CPT). |
