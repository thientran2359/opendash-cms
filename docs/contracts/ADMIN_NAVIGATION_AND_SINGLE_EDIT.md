# Contract: Admin Navigation, CPT Schema Builder, Panel Registry & Composite Single Edit

**Owner:** Nova (Draft Author) · **Reviewer / Coordinator:** Atlas · **Date:** 2026-11-15
**Design status:** `DRAFT / G2 NEEDS_RESPONSE` (cập nhật xử lý `R1`–`R5` theo [Issue #268 Turn 4](https://github.com/thientran2359/opendash-cms/issues/268#issuecomment-6897780974); **chưa `AGREED`** cho tới khi Atlas review head SHA và ghi Final Decision).
**Decision authority:** [ADR-0016](../decisions/ADR-0016-admin-navigation-and-single-edit.md) · **Baseline:** `main@a52607c` (migration cao nhất hiện tại: `0033_media_lifecycle.sql`).
**Implementation / Verification:** `NOT_STARTED / NOT_RUN`. Chưa cấp ownership triển khai runtime hay `Project Ready`; bộ G3 phát hành sau khi contract được chốt và merge.

> [!IMPORTANT]
> Mã [ADR-0016](../decisions/ADR-0016-admin-navigation-and-single-edit.md) và tên migration dự kiến `0034_field_groups_and_editor_layout.sql` được kiểm tra tại baseline `main@a52607c`, **không phải số cứng cấp trước cho runtime**. Khi lập G3 và mở PR triển khai runtime, agent thực hiện phải kiểm tra lại số thứ tự kế tiếp trên `main`.

---

## 1. Định danh có cấu trúc (`ScopedResourceId`), Ba trục quyền Field (`R2`) và Vòng đời Provider (`R1`, `R4`)

### 1.1. Định danh `ScopedResourceId` (`opendash_core::id`)

Không trộn `PackageId` (cho phép dấu gạch ngang `-`) vào `valid_namespaced_snake_key` (`a-z0-9_`). Mọi `field_group_id` và `panel_id` dùng kiểu định danh có cấu trúc:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ScopedResourceId {
    pub owner: ResourceOwner, // Core | User | Theme(PackageId) | Plugin(PackageId)
    pub local_key: SnakeKey,  // valid_snake_key: ^[a-z][a-z0-9_]{0,63}$
}
```

- **Biểu diễn chuỗi chuẩn (Canonical String Format):**
  - `ResourceOwner::Core` → `core:<local_key>` (chỉ dành cho panel lõi của hệ thống, ví dụ `core:title_slug`, `core:publish_box`, `core:seo`, `core:featured_image`, `core:taxonomies`, `core:page_attributes`). Không một API hay manifest gói nào được đăng ký tiền tố `core:`.
  - `ResourceOwner::User` → `user:<local_key>` (do Admin tạo qua UI/API quản trị).
  - `ResourceOwner::Theme(pkg_id)` → `theme:<pkg_id>:<local_key>` (`pkg_id` theo `valid_package_id`, `local_key` theo `valid_snake_key`).
  - `ResourceOwner::Plugin(pkg_id)` → `plugin:<pkg_id>:<local_key>` (`pkg_id` theo `valid_package_id`, `local_key` theo `valid_snake_key`).

### 1.2. Ba trục quyền độc lập của Custom Field (`FieldAccessPolicy` — `R2`)

Mỗi custom field trong base CPT hoặc `FieldGroupDefinition` có 3 thuộc tính kiểm soát truy cập độc lập:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicExposure {
    Public,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldAccessPolicy {
    pub min_view_role: Role,             // Editor | Admin (mặc định: Editor)
    pub min_edit_role: Role,             // Editor | Admin (mặc định: Editor)
    pub public_exposure: PublicExposure, // Public | Private
}
```

1. **Bất biến giữa `min_view_role`, `min_edit_role` và `public_exposure`:**
   - **Rule ACL-1 (`min_edit_role >= min_view_role`):** Với thứ tự `Editor < Admin`, mọi khai báo field có `min_edit_role == Editor` nhưng `min_view_role == Admin` đều bị từ chối ngay khi đăng ký/cập nhật schema với `422 Unprocessable Entity` (`code: "invalid_field_acl"` — người nhập liệu không thể sửa một trường mà họ không có quyền nhìn thấy).
   - **Rule ACL-2 (`public_exposure == Public` đòi hỏi `min_view_role == Editor`):** Một trường giới hạn `min_view_role == Admin` không bao giờ được phép công khai ra ngoài public (`public_exposure == Public` kết hợp `min_view_role == Admin` bị từ chối với `422 invalid_field_acl`).
2. **Giá trị mặc định & Tương thích ngược:**
   - **Base CPT fields hiện hữu** (`content_fields` tạo trước migration này hoặc tạo qua base CPT descriptor không chỉ định `public_exposure`): mặc định `(min_view_role = Editor, min_edit_role = Editor, public_exposure = Public)` để **không làm thay đổi bất kỳ dữ liệu public nào đang hiển thị**.
   - **Extension fields mới trong `content_field_groups`:** mặc định `min_view_role = Editor`, `min_edit_role = Editor`, và **`public_exposure = Private`** trừ khi schema/manifest khai báo tường minh `"public_exposure": "public"`.
3. **Thực thi đồng nhất trên toàn bộ đường đọc Public, Preview và Media (`R2`):**
   - **Public Routes & MiniJinja View Models** (`/`, `/content/{type_key}`, `/content/{type_key}/{slug}`, `/taxonomy/...`): chỉ nạp vào `entry.fields.<key>` các trường có `public_exposure == Public` của entry đã xuất bản (`published`). Các trường `public_exposure == Private` bị loại bỏ hoàn toàn trước khi truyền vào template engine.
   - **Builder Field Binding (`FixedEntry` và `RouteContext` theo ADR-0009 / M5):** chỉ cho phép bind tới các trường core được phép hoặc custom field có `public_exposure == Public`. Nếu một trường đổi từ `Public` sang `Private`, binding hiện hữu không render giá trị ra public HTML (trả chuỗi rỗng + cảnh báo trong Admin Preview).
   - **Admin Preview:** khi hiển thị bản xem trước giao diện public (iframe preview), view model áp dụng đúng bộ lọc `public_exposure == Public` như trang public thật để người biên tập thấy chính xác những gì khách truy cập sẽ thấy.
   - **Media Public Authorization (`GET /media/...` và `M8` D02 Reference Inventory):**
     - Một file media chỉ được phục vụ công khai không cần đăng nhập trên `/media/...` khi được tham chiếu bởi ít nhất một trường `public_exposure == Public` (hoặc `cover_image` / Builder block công khai) của một nội dung đang `published`.
     - Nếu file media chỉ được tham chiếu bởi trường `public_exposure == Private` (hoặc bài viết đang ở trạng thái `draft`), truy cập public `/media/...` trả `404 Not Found`; **tuy nhiên bộ kiểm kê tham chiếu media (`M8` D02 all-saved-data reference inventory) vẫn giữ nguyên tham chiếu này** để không bao giờ xóa nhầm file trong quá trình đối soát/dọn dẹp media.

### 1.3. Vòng đời Provider (`active` / `provider_inactive` / `withdrawn`) & Bảo toàn quản lý dữ liệu (`R1`)

Tuân thủ yêu cầu cốt lõi của [Product Requirements](../product/PRODUCT_REQUIREMENTS.md) (*"Đổi theme hoặc tắt plugin giữ các CPT, fields và nội dung đã nhập để tiếp tục quản lý"*) và [M6 Contract](M6_PLUGIN_EXTENSIONS.md), hệ thống tách bạch 3 trục độc lập khi Theme bị đổi hoặc Plugin bị tắt/gỡ/rút khai báo:

| Trục hành vi | Khi Provider `active` | Khi Provider `provider_inactive` (đổi Theme / tắt Plugin) | Khi Khai báo `withdrawn` (rút field group / CPT trong phiên bản gói mới hoặc Admin rút group) |
| --- | --- | --- | --- |
| **1. Quản lý dữ liệu trong Admin & API (`EffectiveContentSchema`)** | Hoạt động đầy đủ theo `min_view_role` / `min_edit_role`. | **Giữ nguyên 100% khả năng xem/thêm/sửa/xóa entry và giá trị của cả base CPT fields lẫn extension fields** thông qua **Core Standard Field Fallback Form** (vì mọi `FieldType` đều là kiểu chuẩn của Core). Không loại bỏ base fields hay extension fields khỏi `EffectiveContentSchema`. | **Giữ nguyên 100% khả năng xem/sửa các giá trị đã lưu** qua Core Standard Field Fallback Form và giữ nguyên kiểm tra validation/reference; gắn nhãn `(Đã rút khai báo)` trên UI quản trị. |
| **2. Kiểm kê tham chiếu Media (`M8` D02) & Ràng buộc khóa tên (`field_key`)** | Theo dõi đầy đủ mọi trường `FieldType::Image` và giữ khóa `field_key`. | **Giữ nguyên 100%** kiểm kê `media_references` (media không bao giờ bị coi là orphan khi tắt plugin/đổi theme) và giữ khóa `field_key`. | **Giữ nguyên 100%** kiểm kê `media_references` và giữ khóa `field_key` trong `EffectiveContentSchema` (không cho nhóm khác chiếm lại `field_key` cũ gây lệch kiểu dữ liệu). |
| **3. Thực thi mã của Provider (`plugin_action`, Wasm Hooks, Package Template)** | Chạy Wasm hooks (`content/before_save`, `content/after_save`), nút `PanelKind::Action` và template của gói đang active. | **Dừng thực thi mã của provider đã tắt:** Wasm hooks của plugin tắt không chạy; nút `PanelKind::Action` hiển thị trạng thái vô hiệu hóa (`provider_inactive`) và bị chặn ở server (`409 provider_inactive`); template của gói tắt chuyển về template của theme đang active hoặc core safe fallback. | **Dừng thực thi mã của khai báo đã rút:** không chạy action/hook của khai báo đã rút; dữ liệu nội dung vẫn render bình thường qua template/fallback hiện hành nếu `public_exposure == Public`. |

---

## 2. Schema Lưu trữ (`0034_field_groups_and_editor_layout.sql`), Rút nhóm Field (`R4`) và Kiểm tra tương thích đối xứng (`R4`)

### 2.1. SQLite DDL dự kiến (`0034_field_groups_and_editor_layout.sql`)

```sql
-- Bảng lưu định nghĩa nhóm custom fields (mở rộng cho post, page, và mọi CPT).
-- Giữ nguyên bảng content_types và content_fields hiện hữu cho base CPT.
CREATE TABLE IF NOT EXISTS content_field_groups (
    group_id        TEXT PRIMARY KEY,               -- Canonical ScopedResourceId: 'user:project_meta', 'theme:moc-studio:hero_fields', 'plugin:seo-pro:extra'
    owner_kind      TEXT NOT NULL CHECK (owner_kind IN ('user', 'theme', 'plugin')),
    owner_id        TEXT NOT NULL,                  -- 'user' hoặc validated PackageId
    local_key       TEXT NOT NULL,                  -- validated SnakeKey
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    target_types    TEXT NOT NULL,                  -- JSON array các type_key: '["page","portfolio"]'
    fields_json     TEXT NOT NULL,                  -- JSON array của ExtendedFieldDescriptor (có min_view_role, min_edit_role, public_exposure, default_value)
    default_region  TEXT NOT NULL DEFAULT 'main_bottom'
                    CHECK (default_region IN ('main_top', 'main_primary', 'main_bottom', 'sidebar')),
    default_order   INTEGER NOT NULL DEFAULT 50,
    collapsible      INTEGER NOT NULL DEFAULT 1 CHECK (collapsible IN (0, 1)),
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active', 'provider_inactive', 'withdrawn')),
    version         INTEGER NOT NULL DEFAULT 1,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (owner_kind, owner_id, local_key)
);

CREATE INDEX IF NOT EXISTS idx_content_field_groups_owner
    ON content_field_groups(owner_kind, owner_id, status);

-- Bảng lưu cấu hình bố cục màn hình chỉnh sửa (EditorLayoutConfigV1) cấp website cho từng type_key.
CREATE TABLE IF NOT EXISTS content_editor_layouts (
    type_key        TEXT PRIMARY KEY,               -- 'post', 'page', hoặc CPT key
    layout_json     TEXT NOT NULL,                  -- JSON serialized EditorLayoutConfigV1
    version         INTEGER NOT NULL DEFAULT 1,     -- CAS version (layout_token = 'v<version>')
    updated_by      INTEGER REFERENCES users(id) ON DELETE SET NULL,
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
```

### 2.2. Kiểu dữ liệu Rust cho Field Group & Effective Schema (`opendash_core::content`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldGroupStatus {
    Active,
    ProviderInactive,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtendedFieldDescriptor {
    pub id: SnakeKey,
    pub label: String,
    pub field_type: FieldType,
    pub required: bool,
    #[serde(default)]
    pub default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub validation: Option<FieldValidation>,
    #[serde(default = "Role::default_editor")]
    pub min_view_role: Role,
    #[serde(default = "Role::default_editor")]
    pub min_edit_role: Role,
    #[serde(default = "PublicExposure::default_private")]
    pub public_exposure: PublicExposure,
    /// Nguồn sở hữu trường (Base CPT owner hoặc Field Group ScopedResourceId)
    pub source_id: String,
    pub source_status: FieldGroupStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveContentSchema {
    pub base: ContentTypeDescriptor,
    pub base_provider_status: FieldGroupStatus, // Active | ProviderInactive | Withdrawn
    pub attached_groups: Vec<FieldGroupDefinition>, // Bao gồm cả Active, ProviderInactive, Withdrawn
    pub effective_fields: BTreeMap<String, ExtendedFieldDescriptor>, // Hợp nhất không trùng key
    pub schema_token: String, // "sha256:<64-hex>" tính từ toàn bộ snapshot
}
```

### 2.3. Quy tắc Rút nhóm Field (`DELETE` → Tombstone `withdrawn`) & Bảo toàn dữ liệu (`R4`)

1. **Không Hard-Delete nhóm Field đang có dữ liệu hoặc lịch sử:**
   - Khi Admin gọi `DELETE /api/admin/content/field-groups/{group_id}` (kèm `If-Match: "v<version>"`), hoặc khi một bản cập nhật Theme/Plugin rút một `field_group` khỏi manifest:
     - Server thực thi trong `BEGIN IMMEDIATE`: chuyển `status = 'withdrawn'` và tăng `version = version + 1` (tombstone), **không xóa dòng khỏi `content_field_groups`**.
     - Nhóm ở trạng thái `withdrawn` **giữ nguyên**:
       1. Định nghĩa `fields_json` (đặc biệt là các trường `FieldType::Image` để bộ đối soát media `M8` D02 tiếp tục quét và bảo vệ file media đang được tham chiếu bởi các entry cũ).
       2. Quyền sở hữu `ScopedResourceId` và **sự chiếm giữ `field_key`** trên các `target_types` đã gắn (không cho phép nhóm field khác hoặc base CPT sau này khai báo trùng `field_key` với kiểu dữ liệu khác).
       3. Khả năng hiển thị/chỉnh sửa giá trị hiện hữu qua Core Standard Field Fallback Form trong màn hình Single Edit.
   - Việc xóa vĩnh viễn dữ liệu (destructive purge / schema migration xóa cột dữ liệu trong `content_entries.values_json`) **nằm ngoài phạm vi của contract này**.

### 2.4. Kiểm tra tương thích & Chống đụng độ `field_key` đối xứng trên cả 3 hướng (`R4`)

Hàm kiểm tra thuần `verify_effective_schema_transition(old_effective, new_effective, existing_entry_stats)` phải được gọi bên trong giao dịch `BEGIN IMMEDIATE` ở **cả 3 đường thay đổi schema**:
1. Cập nhật Base CPT của User (`PUT /api/admin/content/types/{key}`).
2. Tạo mới / cập nhật / rút Field Group (`POST /api/admin/content/field-groups`, `PUT /api/admin/content/field-groups/{group_id}`, `DELETE /api/admin/content/field-groups/{group_id}`).
3. Kích hoạt revision của Theme hoặc bật/cập nhật Plugin (`activate_theme_revision`, `enable_plugin_revision`).

**Các bất biến bắt buộc trong `verify_effective_schema_transition`:**
- **Rule COMPAT-1 (Chống đụng độ `field_key` toàn cục trên mỗi `type_key`):**
  - Tập khóa dự trữ của một `type_key` gồm:
    - Các khóa hệ thống (`id`, `type_key`, `slug`, `title`, `body`, `status`, `cover_image`, `author_id`, `created_at`, `updated_at`, `published_at`, `version`, `term_ids`, `custom_path`, `seo`).
    - Toàn bộ `base.fields` của `type_key`.
    - Toàn bộ `fields` của mọi `content_field_groups` gắn với `type_key` có `status IN ('active', 'provider_inactive', 'withdrawn')`.
  - Mọi xung đột `field_key` giữa base CPT và field group, hoặc giữa hai field group khác `group_id`, đều bị từ chối với `409 Conflict` (`code: "field_key_collision"`).
- **Rule COMPAT-2 (Bảo toàn kiểu và miền giá trị của dữ liệu đã lưu):**
  - Áp dụng mở rộng từ `check_content_type_compatibility`: không được đổi `FieldType` của một `field_key` đã tồn tại, không được thu hẹp tập `options` của `Select` hoặc hạ trần `max_length`/`max` nếu làm bất kỳ entry hiện hữu nào trong DB trở nên không hợp lệ (`409 Conflict`, `code: "incompatible_schema_change"`).
- **Rule COMPAT-3 (Quy tắc `default_value` & Cấm dùng `default_value` để lách Compatibility trên dữ liệu cũ — `R4`):**
  - **Kiểm tra hợp lệ của `default_value` ngay khi đăng ký schema:** Nếu một field khai báo `default_value: Some(val)`, `val` phải thỏa mãn đầy đủ `FieldType` và `FieldValidation` của chính field đó (`Text` min/max length & regex, `Number` min/max, `Select` nằm trong `options`, `Boolean`, `Date`). Các kiểu tham chiếu thực thể động (**`FieldType::Image`** tham chiếu media ID, hoặc tham chiếu taxonomy term) **bị cấm khai báo `default_value` tĩnh** trong schema (`422 Unprocessable Entity`, `code: "invalid_field_default"`).
  - **Cấm biến field sang `required: true` khi đã có entry thiếu dữ liệu:** Nếu `type_key` đã có entry lưu trong DB mà thiếu `field_key` (khi thêm mới một field `required: true` vào CPT đã có entry, hoặc đổi một field đang `required: false` sang `required: true`), thao tác cập nhật schema **bị từ chối với `409 Conflict` (`code: "incompatible_schema_change"`) ngay cả khi có khai báo `default_value`**, vì phạm vi này không thực hiện backfill hàng loạt lên các bản ghi cũ.
- **Rule COMPAT-4 (Quy tắc thống nhất `N3` giữa `required`, `default_value`, `min_view_role`, `min_edit_role` và `EditorLayoutConfigV1` — `R4`):**
  - Nhắc lại `Rule ACL-1`: `min_edit_role >= min_view_role` luôn đúng (loại trừ hoàn toàn trường hợp `min_edit_role = Editor` nhưng `min_view_role = Admin`).
  - **Trường hợp 1 (`required: true` và `min_edit_role == Editor`):** Người biên tập (`Editor`) bắt buộc phải nhìn thấy và nhập được trường này. Do đó:
    - Panel/nhóm chứa trường này **không được phép** tắt (`enabled: false`) trong `EditorLayoutConfigV1`.
    - Panel chứa trường này **không được phép** có `panel.min_view_role == Admin` hoặc `panel.min_edit_role == Admin`.
    - Mọi cấu hình vi phạm bị từ chối với `422 Unprocessable Entity` (`code: "required_field_hidden_or_inaccessible"`).
  - **Trường hợp 2 (`required: true` và `min_edit_role == Admin` trên loại nội dung mà `Editor` được phép tạo mới):** Vì `Editor` không có quyền sửa trường này khi tạo entry mới, trường đó **bắt buộc phải có `default_value` kiểu vô hướng hợp lệ** trong schema để server tự động điền khi `Editor` tạo entry (`FieldPatch::Omitted`). Nếu thiếu `default_value`, đăng ký/cập nhật schema bị từ chối với `422 Unprocessable Entity` (`code: "admin_required_field_missing_default"`).

---

## 3. Panel Registry, Kiểu dữ liệu `EditorLayoutConfigV1`, Đăng ký Declarative và Thứ tự đọc trên Mobile

### 3.1. Các vùng hiển thị (`EditorRegion`) & Thứ tự đọc thống nhất trên Mobile

Màn hình Single Edit có 5 vùng cố định:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorRegion {
    HeaderBar,   // Dành riêng cho core:header_bar (Breadcrumb, Tiêu đề màn hình, Xem trước, Lưu/Xuất bản)
    MainTop,     // Phía trên nội dung chính (cột trái desktop)
    MainPrimary, // Vùng nội dung chính: core:title_slug, core:body_editor / nút mở Builder, base CPT fields
    MainBottom,  // Phía dưới nội dung chính (cột trái desktop: field groups, core:seo, ...)
    Sidebar,     // Cột phải desktop: core:publish_box, core:page_attributes, core:featured_image, core:taxonomies, sidebar field groups, action panels
}
```

- **Thứ tự DOM & Thứ tự đọc một cột trên Mobile (`< 1024px`):**
  - Thống nhất tuyệt đối theo đúng ngữ nghĩa tên vùng (`main_top` nằm phía trên `main_primary`):
    **`header_bar → main_top → main_primary → main_bottom → sidebar`**.
  - Không ẩn bất kỳ field hay thông báo lỗi validation nào khi chuyển sang bố cục một cột trên mobile; thanh `header_bar` giữ cố định (sticky) ở đầu màn hình.

### 3.2. Định nghĩa `PanelDefinition`, `EditorLayoutConfigV1` và `LayoutFieldSubgroup`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelKind {
    /// Panel lõi của hệ thống (title_slug, body_editor, publish_box, page_attributes, featured_image, taxonomies, seo)
    CoreBuiltIn {
        core_key: SnakeKey,
    },
    /// Panel hiển thị và chỉnh sửa một nhóm custom fields (lưu cùng CompositeEntrySaveInput)
    ContentFields {
        field_group_id: ScopedResourceId,
    },
    /// Panel chứa nút hành động độc lập gọi plugin_action (KHÔNG chạy khi bấm Lưu nội dung)
    Action {
        action_id: SnakeKey,
        button_label: String,
        confirm_message: Option<String>,
        public_effect: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelDefinition {
    pub panel_id: ScopedResourceId,
    pub title: String,
    pub kind: PanelKind,
    pub applies_to: Vec<String>,            // Danh sách type_key áp dụng ('post', 'page', hoặc CPT key)
    pub default_region: EditorRegion,       // Không được là HeaderBar đối với non-core panel
    pub allowed_regions: Vec<EditorRegion>, // Tập con của [MainTop, MainPrimary, MainBottom, Sidebar]
    pub default_order: i32,                 // [-1000..=1000]
    pub min_view_role: Role,
    pub min_edit_role: Role,                // Bắt buộc min_edit_role >= min_view_role
    pub collapsible: bool,
    pub required_core: bool,                // true đối với core:header_bar, core:title_slug, core:publish_box
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutFieldSubgroup {
    pub subgroup_id: SnakeKey,
    pub title: String,
    pub field_keys: Vec<SnakeKey>,          // Tham chiếu tới các field thuộc cùng field_group_id của panel
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelPlacementConfig {
    pub panel_id: String,                   // Canonical ScopedResourceId string
    pub enabled: bool,
    pub order: i32,
    pub collapsed_by_default: bool,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>, // Chỉ áp dụng cho PanelKind::ContentFields
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorRegionsConfig {
    pub main_top: Vec<PanelPlacementConfig>,
    pub main_primary: Vec<PanelPlacementConfig>,
    pub main_bottom: Vec<PanelPlacementConfig>,
    pub sidebar: Vec<PanelPlacementConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorLayoutConfigV1 {
    pub schema_version: u32,                // Cố định bằng 1 ở V1
    pub type_key: String,
    pub version: u64,                       // layout_token = format!("v{}", version)
    pub regions: EditorRegionsConfig,
}
```

### 3.3. Quy tắc kiểm tra hợp lệ (`validate_editor_layout_config`) & Thứ tự quyết định (Tie-Breaking)

Khi Admin lưu `PUT /api/admin/content/types/{key}/editor-layout` hoặc khi hệ thống hợp nhất `EditorLayoutConfigV1` với danh sách `PanelDefinition` hiện hành:
1. **`schema_version`:** Phải bằng `1` (`422 unsupported_layout_schema_version` nếu khác `1`).
2. **Bảo vệ vùng `HeaderBar` & Panel lõi bắt buộc:**
   - Vùng `header_bar` luôn cố định chứa `core:header_bar` và không nhận cấu hình từ `EditorRegionsConfig`.
   - Các panel có `required_core == true` (`core:title_slug`, `core:publish_box`, và `core:base_fields` nếu CPT có base fields) không được phép đặt `enabled: false` (`422 required_core_panel_disabled`).
3. **Chống trùng lặp `panel_id` & Kiểm tra vùng hợp lệ (`allowed_regions`):**
   - Mỗi `panel_id` chỉ được xuất hiện tối đa **1 lần** trên toàn bộ 4 danh sách trong `EditorRegionsConfig` (`422 duplicate_panel_placement`).
   - Vùng đặt `panel_id` phải nằm trong `panel_def.allowed_regions` (`422 invalid_panel_region`).
   - Giới hạn kích thước: tối đa `64` panel placements trên một `type_key`, tối đa `12` `subgroups` trong một panel, và tối đa `64` `field_keys` trong một panel.
4. **Kiểm tra tham chiếu & Chống trùng lặp `field_key` trong `subgroups`:**
   - Chỉ panel có `PanelKind::ContentFields { field_group_id }` (hoặc `core:base_fields`) mới được khai báo `subgroups` không rỗng (`422 subgroups_not_allowed_on_panel_kind`).
   - Mọi `field_key` trong `subgroups` phải tồn tại trong nhóm field tương ứng (`422 unknown_field_in_subgroup`) và không được xuất hiện ở 2 subgroup khác nhau trong cùng một panel (`422 duplicate_field_in_subgroups`).
   - **Bảo toàn trường mới thêm (Auto-append):** Nếu nhóm field được thêm trường mới trong schema mà `subgroups` đã lưu chưa liệt kê `field_key` đó, khi dựng `EffectiveEditorConfigProjection`, server tự động nối các trường chưa liệt kê vào cuối nhóm theo đúng thứ tự khai báo trong schema (không bao giờ làm ẩn mất trường vì cấu hình subgroup cũ).
5. **Quy tắc sắp xếp tất định (Deterministic Ordering):**
   - Trong mỗi vùng (`main_top`, `main_primary`, `main_bottom`, `sidebar`), các panel được sắp xếp theo khóa tăng dần:
     `(placement.order ASC, owner_rank(panel.owner) ASC, panel.panel_id.as_str() ASC)`
     trong đó `owner_rank`: `Core = 0`, `User = 1`, `Theme = 2`, `Plugin = 3`.
   - Các panel mới đăng ký chưa có trong `content_editor_layouts` được tự động chèn vào `panel.default_region` với `order = panel.default_order` và `enabled = true`.
   - Nếu một `panel_id` đã lưu trong `content_editor_layouts` thuộc về một `PanelKind::Action` của plugin đang `provider_inactive`, placement vẫn được giữ nguyên trong JSON lưu trữ để khi bật lại plugin không mất vị trí; trên UI hiển thị trạng thái `provider_inactive` (nút vô hiệu hóa). Nếu là `PanelKind::ContentFields` của provider đang `provider_inactive` hoặc `withdrawn`, panel **vẫn hiển thị bình thường qua Core Standard Field Fallback Form** theo mục `1.3` (`R1`).

### 3.4. Đường đăng ký Declarative qua Manifest của Theme (`theme.json` v2) và Plugin (`plugin.json` `manifest_schema: 2`)

Theme và Plugin **không được phép** nạp JS/React/HTML tùy ý vào Admin để vẽ panel. Các gói đăng ký nhóm field và panel hoàn toàn bằng JSON khai báo tĩnh trong manifest:

```json
{
  "field_groups": [
    {
      "local_key": "project_specs",
      "title": "Thông số dự án",
      "description": "Các thông số kỹ thuật của dự án thiết kế",
      "target_types": ["portfolio", "page"],
      "default_region": "main_bottom",
      "default_order": 30,
      "collapsible": true,
      "fields": [
        {
          "id": "client_name",
          "label": "Khách hàng",
          "field_type": "text",
          "required": false,
          "min_view_role": "editor",
          "min_edit_role": "editor",
          "public_exposure": "public"
        },
        {
          "id": "internal_budget_note",
          "label": "Ghi chú ngân sách nội bộ",
          "field_type": "text",
          "required": false,
          "min_view_role": "admin",
          "min_edit_role": "admin",
          "public_exposure": "private"
        }
      ]
    }
  ],
  "editor_panels": [
    {
      "local_key": "sync_crm_panel",
      "title": "Đồng bộ CRM",
      "kind": {
        "kind": "action",
        "action_id": "ping_crm",
        "button_label": "Gửi dữ liệu thử sang CRM",
        "confirm_message": "Bạn có chắc muốn gửi bản ghi đã lưu sang CRM?",
        "public_effect": false
      },
      "applies_to": ["portfolio"],
      "default_region": "sidebar",
      "allowed_regions": ["sidebar", "main_bottom"],
      "default_order": 80,
      "min_view_role": "admin",
      "min_edit_role": "admin",
      "collapsible": true
    }
  ]
}
```

- **Ràng buộc bảo mật & Hạn mức (Quotas) khi cài/kích hoạt gói:**
  - `owner` được server tự động gán từ danh tính gói đã xác thực (`ResourceOwner::Theme(theme_id)` hoặc `ResourceOwner::Plugin(plugin_id)`). Manifest không được khai báo `owner` khác.
  - Mỗi `field_group` khai báo trong manifest tự động sinh ra một `PanelDefinition` kiểu `PanelKind::ContentFields { field_group_id }` có cùng `ScopedResourceId`.
  - `editor_panels` kiểu `PanelKind::Action` chỉ hợp lệ trong `plugin.json` (Theme không có Wasm action runtime; nếu `theme.json` khai báo `PanelKind::Action` sẽ bị từ chối `422`). `action_id` phải khớp với một action đã khai báo trong `actions` của cùng `plugin.json`.
  - Hạn mức mỗi gói: tối đa `16` `field_groups`, tối đa `64` `fields` trong một group, tối đa `16` `editor_panels`. Vượt hạn mức hoặc vi phạm `verify_effective_schema_transition` làm giao dịch kích hoạt gói fail-closed (`409` / `422`), giữ nguyên revision đang chạy.

---

## 4. Ma trận Endpoint, `schema_token` đầy đủ (`R3`), Giao dịch `CompositeEntrySaveInput` (`R2`, `R3`, `R4`) và Lọc Payload Hook (`R2`)

### 4.1. Ma trận Endpoint & Quy tắc Precondition (Create vs Update, `schema_token` vs `layout_token` — `R3`)

| Phương thức & Đường dẫn | Quyền tối thiểu | CSRF / Origin | Precondition (`If-Match` / Token) | Mục đích & Hành vi chính |
| --- | --- | --- | --- | --- |
| `GET /api/admin/content/types` | `Editor` | Không (GET) | Trả `ETag` danh sách | Liệt kê CPT kèm `provider_status` và `schema_token`. |
| `POST /api/admin/content/types` | `Admin` | Bắt buộc | **Không yêu cầu `If-Match`** (tạo mới; trùng key trả `409`) | Tạo CPT mới thuộc `ResourceOwner::User`. |
| `PUT /api/admin/content/types/{key}` | `Admin` | Bắt buộc | `If-Match: "v<cpt_version>"` (`428` nếu thiếu, `412` nếu lệch) | Cập nhật CPT thuộc `ResourceOwner::User` (chạy `verify_effective_schema_transition` trong `BEGIN IMMEDIATE`). Từ chối `403` nếu CPT thuộc `Theme`/`Plugin` hoặc builtin `post`/`page`. |
| `GET /api/admin/content/field-groups` | `Admin` | Không (GET) | Trả `version` từng group | Liệt kê toàn bộ `content_field_groups` (quản trị raw schema). |
| `POST /api/admin/content/field-groups` | `Admin` | Bắt buộc | **Không yêu cầu `If-Match`** (tạo mới; trùng ID/field_key trả `409`) | Tạo nhóm custom fields thuộc `ResourceOwner::User`. |
| `PUT /api/admin/content/field-groups/{group_id}` | `Admin` | Bắt buộc | `If-Match: "v<group_version>"` (`428` / `412`) | Cập nhật nhóm field thuộc `ResourceOwner::User` (chạy `verify_effective_schema_transition`). |
| `DELETE /api/admin/content/field-groups/{group_id}` | `Admin` | Bắt buộc | `If-Match: "v<group_version>"` (`428` / `412`) | **Rút nhóm field (`status = 'withdrawn'` tombstone)**, giữ nguyên `fields_json`, khóa `field_key` và kiểm kê media (`R4`). |
| `GET /api/admin/content/types/{key}/editor-layout` | `Admin` | Không (GET) | Trả `ETag: "v<layout_version>"` (`"v0"` nếu chưa có override) | Đọc cấu hình raw `EditorLayoutConfigV1` và danh sách toàn bộ `PanelDefinition` cho màn hình quản trị bố cục. |
| `PUT /api/admin/content/types/{key}/editor-layout` | `Admin` | Bắt buộc | `If-Match: "v<layout_version>"` (`"v0"` khi lưu override đầu tiên; `428` / `412`) | Lưu `EditorLayoutConfigV1` (kiểm tra `validate_editor_layout_config` + `N3`). Tăng `layout_version`, giữ nguyên `schema_token`. |
| **`GET /api/admin/content/types/{key}/effective-editor-config`** | **`Editor` + `Admin`** | Không (GET) | Trả `schema_token` (`sha256:<64-hex>`) và `layout_token` (`v<ver>`) | **Projection cấu hình hiệu lực theo phiên của người gọi (`R3`):** chỉ trả các field và panel có `caller.role >= min_view_role`, gắn cờ `editable: caller.role >= min_edit_role` và `provider_status`. Dùng để dựng màn hình Single Edit và kiểm quyền khôi phục nháp (`R5`). |
| `POST /api/admin/content/{type_key}/composite` | `Editor` | Bắt buộc | **Body bắt buộc có `expected_schema_token`** (`428` nếu thiếu, `409` nếu lệch); `expected_version` phải là `None` | Tạo mới nguyên tử (core entry + field values + taxonomy terms + `custom_path` + SEO) trong 1 giao dịch `BEGIN IMMEDIATE`. |
| `PUT /api/admin/content/{type_key}/{id}/composite` | `Editor` | Bắt buộc | **Body bắt buộc có cả `expected_version: u64` (`428`/`412`) và `expected_schema_token: String` (`428`/`409`)** | Cập nhật nguyên tử (patch/merge theo quyền + 1 lần tăng `version + 1`). |

### 4.2. Định nghĩa tất định của `schema_token` (`sha256:<64-hex>` — `R3`)

- `schema_token` **không dùng prefix cắt ngắn** và không chỉ băm các nhóm `active`.
- `schema_token` có dạng `"sha256:<64-lowercase-hex>"`, được tính bằng hàm băm SHA-256 trên chuỗi JSON chuẩn hóa (canonical `BTreeMap` key ordering theo RFC 8785 JCS) của cấu trúc `SchemaTokenInput`:
  1. `base_descriptor`: `(type_key, owner, base_provider_status, version, capabilities, supports_category, supports_tag, ordered_base_fields)` trong đó mỗi field gồm đầy đủ `(id, field_type, required, default_value, validation, min_view_role, min_edit_role, public_exposure)`.
  2. `attached_groups`: danh sách tất cả các nhóm trong `content_field_groups` gắn với `type_key` có `status IN ('active', 'provider_inactive', 'withdrawn')`, sắp xếp theo `group_id ASC`, mỗi nhóm gồm `(group_id, owner, status, version, ordered_fields_with_full_acl_and_defaults)`.
  3. `provider_generations`: `(active_theme_revision_id, enabled_plugins_for_type: Vec<(plugin_id, revision_generation)>)`.
- Bất kỳ thay đổi nào lên định nghĩa trường, `required`, `default_value`, `min_view_role`, `min_edit_role`, `public_exposure`, trạng thái `active`/`provider_inactive`/`withdrawn`, hay thế hệ revision của gói đều tạo ra một `schema_token` mới.

### 4.3. DTO `CompositeEntrySaveInput` & Ngữ nghĩa `FieldPatch` duy nhất (`R2`, `R3`, `R4`)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldPatch {
    /// Trường không xuất hiện trong JSON object `values`: giữ nguyên giá trị đang lưu trong DB (hoặc điền default_value khi tạo mới).
    Omitted,
    /// Trường xuất hiện với giá trị JSON `null`: XÓA khóa `field_key` khỏi BTreeMap `values_json` (không lưu literal `null` vào DB).
    Clear,
    /// Trường xuất hiện với giá trị khác `null`: gán giá trị mới cho `field_key` trong `values_json`.
    Set(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeEntrySaveInput {
    pub expected_version: Option<u64>,     // None khi POST create; Some(v) bắt buộc khi PUT update
    pub expected_schema_token: String,     // Bắt buộc cho cả POST create và PUT update ("sha256:<64-hex>")
    pub title: String,
    pub slug: String,
    pub body: Option<String>,              // Áp dụng cho post / page
    pub status: Option<EntryStatus>,       // Draft | Published (theo capability của type_key)
    pub cover_image: Option<String>,
    pub values: BTreeMap<String, Option<serde_json::Value>>, // None trong map tương ứng FieldPatch::Clear; vắng mặt tương ứng FieldPatch::Omitted
    pub term_ids: Option<Vec<i64>>,
    pub custom_path: Option<Option<String>>, // Some(Some("/duong-dan")) | Some(None) xóa custom_path | None giữ nguyên
    pub seo: Option<EntrySeoInput>,
}
```

### 4.4. Quy trình thực thi 4 Pha của `CompositeEntrySaveInput` (Không giữ khóa DB qua Wasm & Khóa Oracle — `R2`, `R3`, `R4`)

1. **Pha 1 — Read, Preflight & Kiểm quyền Field không có Oracle (ngoài giao dịch ghi):**
   - Xác thực phiên đăng nhập, CSRF/Origin, và ghi nhận `preflight_snapshot = (current_schema_token, actor.id, actor.role, session_generation)`.
   - Kiểm tra `expected_schema_token`:
     - Nếu rỗng/thiếu → trả `428 Precondition Required` (`code: "missing_schema_token"`).
     - Nếu `expected_schema_token != current_schema_token` → trả `409 Conflict` (`code: "schema_version_conflict"`, đính kèm `current_schema_token` trong `details`).
   - Nếu là `PUT` update: đọc entry hiện tại từ DB, kiểm tra `expected_version` (`428` nếu `None`, `412 Precondition Failed` nếu khác `entry.version`).
   - **Kiểm tra quyền Field tuyệt đối không có Value-Comparison Oracle (`R2`):**
     - Duyệt từng khóa `(field_key, patch_opt)` được gửi tường minh trong `input.values`:
       - Nếu `field_key` không tồn tại trong `EffectiveContentSchema.effective_fields` → trả `422 Unprocessable Entity` (`code: "unknown_field_key"`).
       - Nếu `actor.role < field_desc.min_edit_role` (bao gồm cả trường hợp `actor.role < field_desc.min_view_role`): **từ chối ngay lập tức với `403 Forbidden` (`code: "field_edit_forbidden"`) mà KHÔNG đọc hay so sánh với giá trị đang lưu trong DB**.
   - **Hợp nhất `merged_values` (`BTreeMap<String, serde_json::Value>`):**
     - Khởi tạo `merged_values` bằng bản sao `existing_entry.values` (khi update) hoặc `BTreeMap::new()` (khi create).
     - Với mỗi `field_desc` trong `effective_fields`:
       - Nếu `input.values` không chứa `field_desc.id` (`FieldPatch::Omitted`):
         - Khi `create`: nếu `field_desc.default_value` là `Some(def)`, chèn `(field_desc.id, def.clone())` vào `merged_values`.
         - Khi `update`: nếu `merged_values` chưa có `field_desc.id` VÀ `actor.role < field_desc.min_edit_role` VÀ `field_desc.default_value` là `Some(def)`, chèn `(field_desc.id, def.clone())`; ngược lại giữ nguyên giá trị cũ trong `merged_values`.
       - Nếu `input.values` chứa `(field_desc.id, None)` (`FieldPatch::Clear`):
         - Nếu `field_desc.required == true` → trả `422 Unprocessable Entity` (`code: "required_field_missing"`).
         - Ngược lại gọi `merged_values.remove(&field_desc.id)` (**xóa hẳn key khỏi JSON object, không bao giờ lưu giá trị `null`**).
       - Nếu `input.values` chứa `(field_desc.id, Some(val))` (`FieldPatch::Set(val)`):
         - Kiểm tra `val` theo `field_desc.field_type` và `field_desc.validation`; nếu hợp lệ thì gán `merged_values.insert(field_desc.id, val)`.
   - Kiểm tra toàn bộ `merged_values` với `EffectiveContentSchema` (mọi trường `required: true` phải có mặt), kiểm tra sự tồn tại của media (`require_existing_references`), tính hợp lệ của `term_ids`, `custom_path`, và `seo`.

2. **Pha 2 — Chạy Wasm Hook `content/before_save` với Payload đã lọc theo từng Plugin (ngoài giao dịch SQLite — `R2`):**
   - Không giữ bất kỳ kết nối/giao dịch ghi SQLite nào trong suốt thời gian chạy Wasm (`<= 5s`).
   - Giữ nguyên chữ ký kết quả của `hooks.rs::dispatch_content_before_save` trả về `Result<(), ContentBeforeSaveVeto>` (`Allow` hoặc `Veto { code, message, field_errors }`, plugin **không có quyền sửa `merged_values`**).
   - **Ma trận lọc Payload theo từng Plugin (`R2`):**
     - Với mỗi plugin `plugin_id` đăng ký `content/before_save` (hoặc `content/after_save`) cho `type_key`:
       1. Plugin phải đang `enabled = 1` và có capability `content_read` cho `type_key`.
       2. Xây dựng bản chiếu `filtered_candidate_values` dành riêng cho `plugin_id`: một cặp `(field_key, value)` trong `merged_values` chỉ được đưa vào `filtered_candidate_values` khi thỏa mãn **đồng thời**:
          - `actor.role >= field_desc.min_view_role` (nếu người bấm Lưu là `Editor`, không một plugin nào nhận được các trường `min_view_role == Admin` trong hook envelope), **VÀ**
          - Một trong ba điều kiện sau đúng:
            a) `field_desc.public_exposure == PublicExposure::Public`, **HOẶC**
            b) `field_desc.source_id` thuộc quyền sở hữu của chính plugin đó (`ResourceOwner::Plugin(plugin_id)`), **HOẶC**
            c) Plugin khai báo tường minh capability `content_read_private` cho `type_key` trong manifest đã được Admin kích hoạt.
       3. Các trường không thỏa điều kiện trên bị loại bỏ hoàn toàn khỏi JSON envelope gửi vào Wasm worker của `plugin_id` và bị ẩn khỏi mọi dòng log lỗi/chẩn đoán của hook.
     - Quy tắc tương tự áp dụng cho `plugin_action` (`host_call` staged content write trong `plugin/manager.rs`): quyền ghi hiệu lực là `effective_role = min(actor.role, action_decl.required_role)`; các trường không gửi giữ nguyên giá trị cũ (`FieldPatch::Omitted`), và bất kỳ trường nào gửi tường minh mà `effective_role < field_desc.min_edit_role` (hoặc trường `Private` của chủ sở hữu khác khi plugin không có capability ghi tương ứng) đều bị từ chối với `403 field_edit_forbidden` (không so sánh DB).

3. **Pha 3 — Giao dịch ghi nguyên tử `BEGIN IMMEDIATE` & Đối chiếu lại Snapshot Pha 1 (`R3`):**
   - Mở một giao dịch SQLite `BEGIN IMMEDIATE` duy nhất.
   - **Kiểm tra lại Snapshot Pha 1 (bắt buộc cho MỌI endpoint ghi nội dung, kể cả legacy endpoint không có client token — `R3`):**
     1. Đọc lại phiên đăng nhập hiện hành của `actor.id`: nếu phiên đã bị thu hồi/hết hạn → abort `401 Unauthorized`; nếu `current_actor.role != preflight_snapshot.actor_role` hoặc không còn đủ quyền trên các trường đã sửa → abort `403 Forbidden`.
     2. Tính lại `current_schema_token` bên trong giao dịch `BEGIN IMMEDIATE`: nếu `current_schema_token != preflight_snapshot.schema_token` → abort `409 Conflict` (`code: "schema_version_conflict"`).
     3. Nếu là `update`, kiểm tra lại `entry.version == expected_version` (`412 Precondition Failed` nếu lệch).
     4. Kiểm tra lại tính duy nhất của `slug`, xung đột `custom_path`, sự tồn tại của `media_id` và `term_ids`.
   - Ghi cập nhật vào `content_entries` (hoặc `posts`/`pages`), `entry_terms`, `custom_routes`, `seo_meta`, và cập nhật bảng `media_references` (gồm cả các trường `Image` có `public_exposure == Private` và các trường thuộc nhóm `provider_inactive`/`withdrawn`).
   - Tăng `version = version + 1` **đúng một lần** và `COMMIT`.

4. **Pha 4 — Chạy `content/after_save` sau khi Commit:**
   - Phát sự kiện `content/after_save` với `filtered_candidate_values` áp dụng đúng ma trận lọc của Pha 2. Lỗi của `after_save` được ghi log (đã lọc dữ liệu nhạy cảm) và không hoàn tác giao dịch đã commit.

---

## 5. Bảng định tuyến Hash URL chuẩn, Bảo vệ Deep-Link RBAC & Quarantine `EditorDraftStore` (`R5`)

### 5.1. Bảng định tuyến Hash URL chuẩn & Chuyển hướng tương thích ngược

| Hash Route chuẩn | Màn hình / Chức năng | Quyền tối thiểu | Hành vi khi không đủ quyền / Chuyển hướng legacy |
| --- | --- | --- | --- |
| `#/dashboard` | Tổng quan (Dashboard) | `Editor` | — |
| `#/posts` · `#/posts/new` · `#/posts/:id/edit` | Danh sách / Thêm mới / Chỉnh sửa Bài viết | `Editor` | — |
| `#/posts/categories` · `#/posts/tags` | Chuyên mục / Thẻ dùng chung | `Editor` | — |
| `#/pages` · `#/pages/new` · `#/pages/:id/edit` | Danh sách / Thêm mới / Chỉnh sửa Trang | `Editor` | — |
| `#/cpt/:typeKey` · `#/cpt/:typeKey/new` · `#/cpt/:typeKey/:id/edit` | Danh sách / Thêm mới / Chỉnh sửa Entry của CPT `:typeKey` | `Editor` | Nếu `:typeKey` không tồn tại → màn hình `404` nội bộ. Vẫn mở và sửa bình thường khi provider của `:typeKey` đang `provider_inactive` (`R1`). |
| `#/media` · `#/media/upload` | Thư viện Media / Tải lên | `Editor` | — |
| `#/appearance/themes` · `#/appearance/customize` · `#/appearance/menus` · `#/appearance/templates` | Quản lý Giao diện / Menus / Templates toàn site | `Admin` | `Editor` truy cập URL trực tiếp → hiển thị `403 Không có quyền truy cập` (không gọi API ghi). |
| `#/builder?template=:role&type=:typeKey&entry=:entryId` | Visual Builder | Theo phạm vi template | Nếu `:role` là template dùng chung toàn site (`header`, `footer`, `archive`, `taxonomy_archive`, hoặc `single`/`page` không gắn `entryId`) → **chỉ `Admin`** (`Editor` bị chặn `403` cả trên UI và server API). `Editor` chỉ được mở/lưu Builder override khi có `entryId` cụ thể mà `Editor` có quyền sửa. |
| `#/contact/forms` · `#/contact/submissions` · `#/contact/smtp` | Form liên hệ / Tin nhắn / Cấu hình SMTP | `Editor` xem tin nhắn; `Admin` cấu hình form & SMTP | — |
| `#/plugins` · `#/plugins/install` | Plugins đã cài / Cài từ ZIP | `Admin` | `Editor` → `403`. |
| `#/users` · `#/users/new` | Tất cả người dùng / Thêm người dùng | `Admin` | `Editor` → `403`. |
| `#/users/profile` | Hồ sơ của tôi (`GET/PUT /api/admin/profile`) | `Editor` + `Admin` | Cho phép sửa `display_name` và đổi mật khẩu của chính mình (đổi mật khẩu **bắt buộc gửi kèm `current_password`**, sai trả `403 invalid_current_password`; không được tự đổi `role`). |
| `#/tools/backup` · `#/tools/restore` · `#/tools/media-reconcile` · `#/tools/health` | Sao lưu / Phục hồi / Đối soát media / Tình trạng hệ thống | `Admin` | `Editor` → `403`. |
| `#/settings/general` · `#/settings/permalinks` · `#/settings/seo` | Cài đặt Tổng quát / Đường dẫn / SEO website | `Admin` | `Editor` → `403`. |
| **`#/settings/content-types`** · **`#/settings/content-types/:typeKey`** | **Cài đặt → Loại nội dung & Trường** (Tạo/sửa CPT, quản lý Field Groups và sắp xếp `EditorLayoutConfigV1`) | **`Admin`-only** | **Tuyệt đối không chứa bảng danh sách hay form nhập entry (`F1`).** Luôn hiển thị trên menu `Cài đặt` kể cả khi có `0` CPT. Route cũ `#/content` tự động redirect: `Admin` → `#/settings/content-types`, `Editor` → `#/posts`. |

### 5.2. Quản lý bản nháp `EditorDraftStore`, Quarantine khi mất phiên và Kiểm quyền lại trước khi khôi phục (`R5`)

1. **Khóa định danh & Provenance của bản nháp trong RAM (`EditorDraftStore`):**
   - Mỗi bản nháp chỉnh sửa đang mở được lưu trong bộ nhớ RAM của ứng dụng Admin (không ghi ra `localStorage`/`sessionStorage` để tránh lộ dữ liệu nhạy cảm trên máy dùng chung) với khóa và metadata:
     ```ts
     interface DraftEnvelope {
       site_instance_id: string;      // Định danh instance/epoch của site trả về từ /api/admin/auth/me (đổi khi restore site)
       user_id: number;               // ID tài khoản đang đăng nhập
       auth_generation: number;       // Thế hệ phiên xác thực
       draft_key: string;             // `${type_key}:edit:${entry_id}` hoặc `${type_key}:new:${draft_uuid}` (dùng UUID riêng cho từng tab tạo mới)
       status: 'active' | 'quarantined';
       schema_token_at_edit: string;  // schema_token lúc sửa
       dirty_fields: Set<string>;     // Danh sách các field_key người dùng đã chạm vào
       payload: DraftPayload;         // Dữ liệu nháp trên form
     }
     ```
2. **Chặn phản hồi bất đồng bộ trễ bằng `session_epoch` (`R5`):**
   - Frontend duy trì biến đếm đơn điệu `session_epoch` tăng lên mỗi khi xảy ra một trong các sự kiện: nhận mã `401 Unauthorized`, bấm Đăng xuất, đổi tài khoản, hoặc phát hiện `site_instance_id` thay đổi.
   - Mọi request fetch/save bất đồng bộ đều ghi nhận `request_epoch = session_epoch` lúc khởi tạo; khi response trả về, nếu `request_epoch !== current_session_epoch`, kết quả đó **bị hủy bỏ hoàn toàn**, không được ghi vào `EditorDraftStore` hay state của component.
3. **Cơ chế Quarantine ngay khi mất phiên (`401 Unauthorized` — `R5`):**
   - Khi bất kỳ lệnh gọi API nào trả về `401 Unauthorized`:
     1. Tăng `session_epoch`.
     2. Chuyển toàn bộ các mục trong `EditorDraftStore` sang `status = 'quarantined'`.
     3. **Unmount / che toàn bộ cây DOM của form chỉnh sửa đằng sau màn hình yêu cầu đăng nhập lại** (không để các giá trị trường nhạy cảm của phiên cũ tiếp tục hiển thị trên màn hình khi chưa xác thực lại).
   - Khi người dùng chủ động bấm **Đăng xuất (Logout)** hoặc đăng nhập bằng một tài khoản khác (`new_user.id !== draft.user_id`) hoặc kết nối vào một site vừa khôi phục (`new_site_instance_id !== draft.site_instance_id`):
     - Gọi `EditorDraftStore.clear()` xóa sạch 100% các bản nháp trong RAM.
4. **Quy trình kiểm quyền và lọc lại bản nháp sau khi đăng nhập lại (`Post-Reauthentication Gate` — `R5`):**
   - Ngay cả khi đăng nhập lại đúng cùng `(site_instance_id, user_id)`, bản nháp đang `quarantined` **không được phép tự động đổ thẳng ra form**. Quy trình bắt buộc:
     1. Gọi `GET /api/admin/auth/me` lấy `(site_instance_id, user.id, user.role)` mới.
     2. Gọi mới `GET /api/admin/content/types/{type_key}/effective-editor-config` dưới phiên mới để lấy `EffectiveEditorConfigProjection` hiện hành của user đó.
     3. **Đối chiếu và lọc bản nháp (`scrubQuarantinedDraft`):**
        - Với mọi `field_key` trong `draft.payload.values`:
          - Nếu `field_key` **không còn nằm trong tập trường được phép xem** của `EffectiveEditorConfigProjection` mới (ví dụ: tài khoản vừa bị hạ quyền từ `Admin` xuống `Editor`, hoặc Admin khác vừa nâng `min_view_role` của trường đó lên `Admin`) → **xóa vĩnh viễn `field_key` khỏi `draft.payload.values` và `draft.dirty_fields` trước khi render**.
          - Nếu `field_key` vẫn được phép xem (`min_view_role <= caller.role`) nhưng **không còn quyền sửa** (`editable == false`, tức `caller.role < min_edit_role`) → **hủy bỏ giá trị nháp đã sửa của `field_key`**, khôi phục về giá trị hiện hành trên server và hiển thị thông báo phi nhạy cảm rằng một số trường chỉ đọc đã được đặt lại theo quyền hiện tại.
     4. Sau khi lọc xong mới chuyển `status = 'active'` và hiển thị lại lên màn hình Single Edit (không tự động bấm Lưu).

---

## 6. Ma trận Nghiệm thu Tổng thể & Phân bổ theo Slice (`AC-01`–`AC-11`)

Bảng dưới đây là **bộ tiêu chí nghiệm thu phủ toàn bộ phạm vi tính năng (Feature-Wide Coverage Matrix)**. Khi chia thành các slice G3 để triển khai, **mỗi slice chỉ chạy các case thuộc phạm vi của slice đó**, không bắt một slice đơn lẻ chạy toàn bộ 11 case không liên quan:

| Mã AC | Slice mục tiêu | Nội dung kiểm chứng & Tiêu chí đạt (Pass Criteria) | Liên kết Review |
| --- | --- | --- | --- |
| **AC-01** | `S-SCHEMA` & `S-SAVE` | **Bảo toàn quản lý dữ liệu & media khi Provider Inactive / Withdrawn:** CPT và Page có trường `Image` do Theme/Plugin khai báo → tắt plugin / đổi theme (hoặc rút nhóm field sang `withdrawn`) → `Editor`/`Admin` vẫn xem và sửa được giá trị trường `Image` cùng nội dung entry qua Core Standard Field Fallback Form; đối soát media (`M8` D02) không báo orphan và không xóa ảnh → bật lại provider giữ nguyên dữ liệu và vị trí panel trong `EditorLayoutConfigV1`. | `R1`, `R4` |
| **AC-02** | `S-SCHEMA` | **Kiểm tra tương thích & chống đụng độ `field_key` đối xứng trên 3 hướng:** Thử tạo/sửa base CPT, tạo/sửa field group, và kích hoạt gói có `field_key` trùng với nhóm đang `active`/`provider_inactive`/`withdrawn` → cả 3 hướng đều bị chặn `409 field_key_collision` trong `BEGIN IMMEDIATE`. Thử đổi field optional sang `required: true` (kể cả có `default_value`) khi đã có entry thiếu trường đó → bị chặn `409 incompatible_schema_change`. | `R4` |
| **AC-03** | `S-SCHEMA` & `S-LAYOUT` | **Kiểm tra bất biến `N3` & `default_value`:** (a) Khai báo `min_edit_role = Editor, min_view_role = Admin` → bị chặn `422 invalid_field_acl`; (b) Khai báo `default_value` cho trường `Image` hoặc giá trị sai kiểu → bị chặn `422 invalid_field_default`; (c) Ẩn panel chứa trường `required: true, min_edit_role = Editor` → bị chặn `422 required_field_hidden_or_inaccessible`; (d) Trường `required: true, min_edit_role = Admin` thiếu `default_value` → bị chặn `422 admin_required_field_missing_default`. | `R2`, `R4` |
| **AC-04** | `S-LAYOUT` | **Sắp xếp Panel, Subgroup & Thứ tự Mobile:** Đặt các nhóm field vào `main_top`, `main_bottom`, `sidebar` trên Page và CPT; reload giữ nguyên thứ tự ổn định; thêm field mới vào group tự động nối vào cuối panel mà không mất dữ liệu; trên viewport mobile (`< 1024px`) thứ tự DOM đúng `header_bar → main_top → main_primary → main_bottom → sidebar`. | Turn 4 Completeness |
| **AC-05** | `S-SAVE` | **Tách bạch `public_exposure` khỏi `min_view_role`/`min_edit_role`:** Tạo trường mở rộng `min_view_role = Editor, public_exposure = Private` (có gắn ảnh media riêng) và trường `public_exposure = Public` → `Editor` xem/sửa được cả hai trong Admin, nhưng trang public, template binding và `GET /media/...` không đăng nhập chỉ hiển thị trường/ảnh `Public` (ảnh `Private` trả `404` trên public nhưng vẫn được bảo vệ trong `media_references`). | `R2` |
| **AC-06** | `S-SAVE` | **Lọc Payload Hook theo Plugin & Chống Oracle dò giá trị bí mật:** (a) Khi lưu entry có trường `Private` hoặc trường `min_view_role = Admin`, plugin thứ ba chỉ có `content_read` cơ bản không nhận được các trường đó trong `content/before_save` và `content/after_save`; (b) Khi `Editor` gửi tường minh một `field_key` có `min_edit_role = Admin` trong `values` (dù gửi giá trị trùng khớp hay khác với DB, hoặc gửi `null`), server đều trả đúng cùng mã `403 field_edit_forbidden` trước khi so sánh DB. | `R2` |
| **AC-07** | `S-SAVE` | **Giao dịch Lưu nguyên tử `CompositeEntrySaveInput` & `FieldPatch::Clear`:** Một lần bấm Lưu cập nhật core entry + custom fields + taxonomy terms + `custom_path` + SEO trong một giao dịch `BEGIN IMMEDIATE` và chỉ tăng `version` thêm `+1`; gửi `"field_key": null` xóa hẳn khóa khỏi `values_json`; nếu `custom_path` hoặc SEO lỗi thì toàn bộ entry + fields rollback 100%. | `F4`, `R4` |
| **AC-08** | `S-SAVE` | **Bắt buộc `expected_schema_token` & Hàng rào Snapshot ở Pha 3:** (a) Gửi composite save thiếu `expected_schema_token` → `428 missing_schema_token`; gửi token cũ → `409 schema_version_conflict`; (b) Giả lập Admin đổi schema hoặc hạ quyền `Editor` đúng vào lúc Pha 2 (`content/before_save`) đang chạy → Pha 3 (`BEGIN IMMEDIATE`) phát hiện lệch snapshot và chặn ghi (`409` / `403`). | `R3` |
| **AC-09** | `S-NAV-UI` | **Menu `Cài đặt → Loại nội dung & Trường`, Deep-Link RBAC & Hồ sơ cá nhân:** (a) Khi có `0` CPT, `Admin` vẫn thấy `Cài đặt → Loại nội dung & Trường` (`#/settings/content-types`) và màn hình này không có bảng/form nhập entry; mỗi CPT mới tạo có menu nhập liệu riêng (`#/cpt/:typeKey`); (b) `Editor` gọi `GET .../effective-editor-config` thành công để dựng Single Edit nhưng bị chặn `403` ở các API/route quản trị raw schema/layout và Builder template toàn site; (c) Đổi mật khẩu tại `#/users/profile` bắt buộc `current_password`. | `F1`, `F7`, `R3` |
| **AC-10** | `S-NAV-UI` & `S-SAVE` | **Tách bạch `PanelKind::Action` khỏi Lưu nội dung & Mở Builder an toàn:** Bấm Lưu nội dung không kích hoạt `plugin_action`; bấm nút trong Action Panel không tự lưu dữ liệu form chưa lưu. Trên màn hình Thêm mới CPT (`id === null`), nút mở Builder yêu cầu lưu/xuất bản trước chứ không tự động xuất bản ngầm. | `F6`, `F7` |
| **AC-11** | `S-RECOVERY` | **Quarantine `EditorDraftStore` khi `401` & Negative Case hạ quyền `Admin → Editor` cùng User:** User đang đăng nhập là `Admin`, sửa nháp chứa cả trường `Editor` và trường `Admin`-only → phiên hết hạn (`401`) khiến nháp chuyển sang `quarantined` và ẩn khỏi DOM → trong lúc đó tài khoản này bị hạ quyền xuống `Editor` (hoặc trường đó bị đổi sang `Admin`-only) → cùng user đăng nhập lại → hệ thống tải `effective-editor-config` mới, **xóa bỏ trường `Admin`-only khỏi bản nháp** trước khi khôi phục các trường `Editor` hợp lệ. | `R5` |
