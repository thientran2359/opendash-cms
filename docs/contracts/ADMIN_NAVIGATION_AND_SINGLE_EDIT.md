# Contract: Admin Navigation, CPT Schema Builder, Panel Registry & Composite Single Edit (G2 AGREED)

> [!NOTE]
> **Trạng thái hiện hành (2026-11-20):** **Implemented** (migration `0034_field_groups_and_editor_layout.sql`; xem [`docs/designs/SITEMAP.md`](../designs/SITEMAP.md)). Header `NOT_STARTED / NOT_RUN` bên dưới là snapshot lịch sử lúc chốt G2.
> **Điểm đã được mở rộng/thay thế:** Bảng điều hướng & hash URL ở Mục 5.1 đã được cập nhật thành bố cục 3 nhóm (`Nội dung`, `Giao diện`, `Tùy chỉnh`, chuyển `Loại nội dung & Trường` sang `#/appearance/content-types`, `Liên hệ`/`SMTP` sang `#/settings/contact|smtp`) theo [`ADMIN_REVIEW_2026_11.md`](ADMIN_REVIEW_2026_11.md) và [`CONTENT_FOUNDATION_2026_11.md`](CONTENT_FOUNDATION_2026_11.md) (xem tổng hợp tại [`docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`](../product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md)).

**Owner:** Nova (Author) · **Reviewer / Coordinator:** Atlas · **Date:** 2026-11-16
**Design status:** **G2 AGREED (`CONCLUDED_BY_AGENTS`)** — chốt theo Final Decision của Atlas tại [PR #270 receipt (`#issuecomment-6903576673`)](https://github.com/thientran2359/opendash-cms/pull/270#issuecomment-6903576673) trên head `6c0150690a8148ceb2e668546a7d5e6a8edc4824` (đã giải quyết đầy đủ `R1`–`R5` tại [Issue #268](https://github.com/thientran2359/opendash-cms/issues/268) và `270-R1`–`270-R9` tại [PR #270](https://github.com/thientran2359/opendash-cms/pull/270)); có hiệu lực contract kỹ thuật đầy đủ khi PR được merge theo [ADR-0010](../decisions/ADR-0010-agent-project-and-merge-authority.md).
**Decision authority:** [ADR-0016](../decisions/ADR-0016-admin-navigation-and-single-edit.md) · **Baseline:** `main@a52607c` (migration cao nhất hiện tại: `0033_media_lifecycle.sql`).
**Implementation / Verification:** `NOT_STARTED / NOT_RUN`. Đặc tả G3 (`M7-S09-T01`–`M7-S13-T02`) được quy định tại [G3 Admin Navigation & Single Edit](../archive/plans/milestones/M7/slices/2026-11-16-admin-navigation-and-single-edit-g3.md); chưa phát hành GitHub Issue hay cấp `Project Ready` từ tài liệu này.

> [!IMPORTANT]
> Mã [ADR-0016](../decisions/ADR-0016-admin-navigation-and-single-edit.md) và tên migration dự kiến `0034_field_groups_and_editor_layout.sql` được kiểm tra tại baseline `main@a52607c`, **không phải số cứng cấp trước cho runtime**. Khi lập G3 và mở PR triển khai runtime, agent thực hiện phải kiểm tra lại số thứ tự kế tiếp trên `main`.

---

## 1. Định danh có cấu trúc (`ScopedResourceId`), Ba trục quyền Field (`R2`, `270-R7`), Quyền Media Public (`270-R2`) và Vòng đời Provider (`R1`, `R4`, `270-R1`)

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
  - `ResourceOwner::Core` → `core:<local_key>` (chỉ dành cho panel lõi của hệ thống: `core:header_bar`, `core:title_slug`, `core:body_editor`, `core:publish_box`, `core:seo`, `core:featured_image`, `core:taxonomies`, `core:page_attributes`, `core:base_fields`). Không một API hay manifest gói nào được đăng ký tiền tố `core:`.
  - `ResourceOwner::User` → `user:<local_key>` (do Admin tạo qua UI/API quản trị).
  - `ResourceOwner::Theme(pkg_id)` → `theme:<pkg_id>:<local_key>` (`pkg_id` theo `valid_package_id`, `local_key` theo `valid_snake_key`).
  - `ResourceOwner::Plugin(pkg_id)` → `plugin:<pkg_id>:<local_key>` (`pkg_id` theo `valid_package_id`, `local_key` theo `valid_snake_key`).
  - **Định danh cục bộ cấp Layout (`layout:<slice_key>` — `270-R6`, `270-R9`):** Trong `EditorLayoutConfigV1`, các panel chia nhỏ từ base CPT fields (`BaseFieldSlicePanelConfig`) dùng định danh cục bộ theo từng `type_key` có dạng `layout:<slice_key>` (`slice_key` theo `valid_snake_key`). Đây là namespace dành riêng cho cấu hình bố cục cấp site; **không một User Field Group, Theme manifest hay Plugin manifest nào được phép khai báo hay chiếm dụng tiền tố `layout:`** (`422 reserved_panel_namespace`).

### 1.2. Ba trục quyền độc lập của Custom Field (`FieldAccessPolicy` — `R2`, `270-R7`) & Hợp các nguồn Media Public (`270-R2`)

Mỗi custom field trong base CPT (`content_types.definition_json`) hoặc nhóm trường mở rộng (`content_field_groups.fields_json`) có 3 thuộc tính kiểm soát truy cập độc lập:

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
2. **Tách Input DTO và Chuẩn hóa theo Ngữ cảnh (`FieldScopeKind`) để bảo toàn `missing` vs `explicit` (`270-R7`):**
   - Để không bao giờ xảy ra lỗi deserialize trường mở rộng thiếu `public_exposure` thành `Public` (hoặc ngược lại ghi đè trường mở rộng khai báo `"public_exposure": "public"` thành `Private`), tầng nhận dữ liệu (cả HTTP API của User lẫn bộ đọc Manifest của Theme/Plugin) **bắt buộc dùng kiểu đầu vào `FieldDefinitionInput` có `public_exposure: Option<PublicExposure>`** trước khi chuyển sang kiểu lưu trữ chuẩn `ContentFieldDefinition` (xem code chuẩn hóa tại Mục `2.3`):
     - **Trường hợp 1 — Base CPT fields (`FieldScopeKind::BaseCptField`, thuộc `content_types.definition_json.fields`):**
       - Nếu input không gửi `public_exposure` (`None`) → chuẩn hóa thành **`PublicExposure::Public`**.
       - Nếu input gửi tường minh `Some(Public)` hoặc `Some(Private)` → giữ nguyên giá trị tường minh đó.
       - Các dòng `content_types.definition_json` đã lưu từ trước migration (bao gồm `body` và `featured_image` của `builtin_definition("post" | "page")`) khi đọc từ DB nếu thiếu khóa `public_exposure` cũng mặc định là `PublicExposure::Public`, bảo toàn 100% dữ liệu đang hiển thị và `preflight_builtin_key_collisions`.
     - **Trường hợp 2 — Extension Group fields (`FieldScopeKind::ExtensionGroupField`, thuộc `content_field_groups.fields_json` qua User API hoặc Theme/Plugin manifest):**
       - Nếu input không gửi `public_exposure` (`None`) → chuẩn hóa thành **`PublicExposure::Private`**.
       - Nếu input gửi tường minh `"public_exposure": "public"` (`Some(Public)`) → giữ nguyên **`PublicExposure::Public`**.
       - Nếu input gửi tường minh `"public_exposure": "private"` (`Some(Private)`) → giữ nguyên **`PublicExposure::Private`**.
     - **Ghi xuống SQLite (Canonical Persistence):** Sau bước chuẩn hóa `into_canonical(scope)`, cấu trúc `ContentFieldDefinition` lưu xuống `content_types.definition_json` và `content_field_groups.fields_json` **luôn ghi tường minh trường `"public_exposure": "public"` hoặc `"private"`** để khi khởi động lại server (round-trip restart) hoặc truy vấn SQL không bao giờ phụ thuộc vào phỏng đoán ngữ cảnh.
3. **Thực thi trên đường đọc Public, Template Binding và Preview (`R2`):**
   - **Public Routes & MiniJinja View Models** (`/`, `/content/{type_key}`, `/content/{type_key}/{slug}`, `/taxonomy/...`): chỉ nạp vào `entry.fields.<key>` các trường có `public_exposure == Public` của entry có `publication_status == "published"`. Các trường `public_exposure == Private` bị loại bỏ hoàn toàn trước khi truyền vào template engine.
   - **Builder Field Binding (`FixedEntry` và `RouteContext` theo ADR-0009 / M5):** chỉ cho phép bind tới các trường core được phép hoặc custom field có `public_exposure == Public`. Nếu một trường đổi từ `Public` sang `Private`, binding hiện hữu không render giá trị ra public HTML (trả chuỗi rỗng + cảnh báo trong Admin Preview).
   - **Admin Preview:** khi hiển thị bản xem trước giao diện public (iframe preview), view model áp dụng đúng bộ lọc `public_exposure == Public` như trang public thật.
4. **Quy tắc Ủy quyền Media Public (`storage::public_content::media_is_referenced`) & Độc lập với Kiểm kê Lưu trữ (`270-R2`):**
   - Tại baseline (`cms/crates/opendash-server/src/storage/public_content.rs:236-333`), `media_is_referenced(db, id)` là phép hợp (`OR`) của 7 nhánh nguồn tham chiếu công khai. Contract này **giữ nguyên toàn bộ 7 nhánh hợp lệ hiện có**, và chỉ bổ sung điều kiện `public_exposure = 'public'` cùng bảng `content_field_groups` vào **Nhánh 1 (custom fields của entry)**:
     - **Nhánh 1a (Base CPT `image`/`gallery` fields):** `content_entries e` (`e.publication_status = 'published'`) kết nối `content_types t ON t.key = e.content_type_key` và `json_each(t.definition_json, '$.fields') f` với `json_extract(f.value, '$.type') IN ('image', 'gallery')` **VÀ** `COALESCE(json_extract(f.value, '$.public_exposure'), 'public') = 'public'` (bao gồm cả field có `declaration_state` là `'declared'` hoặc `'withdrawn'`, và bao gồm `featured_image` của `post`/`page`).
     - **Nhánh 1b (Extension Field Group `image`/`gallery` fields — mới):** `content_entries e` (`e.publication_status = 'published'`) kết nối `content_field_group_targets gt ON gt.type_key = e.content_type_key` (`gt.association_state IN ('declared', 'withdrawn')`), `content_field_groups g ON g.group_id = gt.group_id` (`g.status IN ('active', 'provider_inactive', 'withdrawn')`), và `json_each(g.fields_json) f` với `json_extract(f.value, '$.type') IN ('image', 'gallery')` **VÀ** `COALESCE(json_extract(f.value, '$.public_exposure'), 'private') = 'public'`.
     - **Nhánh 2 (`builder_homepage_media` — giữ nguyên baseline):** `bm.media_id = ? AND b.document_json IS NOT NULL`.
     - **Nhánh 3 (`theme_option_media_refs` — giữ nguyên baseline):** `tom.media_id = ?` thuộc revision đang active trong `theme_active_selection` có `json_extract(opt.value, '$.public') = 1`.
     - **Nhánh 4 (`seo_public_media_refs` cho `entry_seo` — giữ nguyên baseline):** `sm.source_kind = 'entry_seo' AND sm.media_id = ?` gắn với `e.publication_status = 'published' AND t.provider_status = 'active'`.
     - **Nhánh 5 (`seo_public_media_refs` cho `home_seo` — giữ nguyên baseline):** `sm.source_kind = 'home_seo' AND sm.source_id = 1 AND sm.media_id = ?`.
     - **Nhánh 6 (`builder_homepage_video_refs` — giữ nguyên baseline):** `bv.media_id = ? AND b.document_json IS NOT NULL`.
     - **Nhánh 7 (`template_override_media_refs` — giữ nguyên baseline):** `tom.media_id = ?` thuộc `template_overrides` cấp global (`scope_type_key IS NULL AND scope_entry_id IS NULL`), cấp CPT active (`scope_type_key IS NOT NULL AND scope_entry_id IS NULL`), hoặc cấp entry published (`scope_entry_id IS NOT NULL`).
   - **Quy tắc Dual-Use (Dùng đồng thời ở nguồn Private và nguồn Public):** Một `media_id` bị gắn với trường `public_exposure == Private` (hoặc entry `draft`) chỉ trả `404 Not Found` trên `GET /media/...` **khi và chỉ khi `media_id` đó KHÔNG xuất hiện ở bất kỳ nhánh nào từ Nhánh 1a đến Nhánh 7 ở trên**. Nếu cùng một ảnh vừa được gán vào một trường `Private` vừa được dùng làm logo theme (`theme_option_media_refs`), ảnh OG trang chủ (`home_seo`), ảnh trong `template_overrides`, hoặc trường `Public` của một bài `published`, thì `GET /media/...` vẫn trả `200 OK` bình thường.
   - **Độc lập hoàn toàn với Kiểm kê Bảo vệ Media (`storage::media_refs::collect_pins_in_tx`):** Hàm `collect_pins_in_tx` (`cms/crates/opendash-server/src/storage/media_refs.rs:88-107`) quét trực tiếp toàn bộ `content_entries.values_json` (bất kể `publication_status`, `public_exposure`, `provider_status` hay `declaration_state`) cùng tất cả các bảng khác. Do đó media chỉ gắn với trường `Private` hoặc bài `draft` luôn có `total_pins > 0` và không bao giờ bị xóa khi đối soát/dọn dẹp media.

### 1.3. Vòng đời Provider (`active` / `provider_inactive` / `withdrawn`) & Bảo toàn quản lý dữ liệu (`R1`, `270-R1`)

Tuân thủ yêu cầu cốt lõi của [Product Requirements](../product/PRODUCT_REQUIREMENTS.md) (*"Đổi theme hoặc tắt plugin giữ các CPT, fields và nội dung đã nhập để tiếp tục quản lý"*) và [M6 Contract](M6_PLUGIN_EXTENSIONS.md), hệ thống tách bạch 3 trục độc lập khi Theme bị đổi hoặc Plugin bị tắt/gỡ/rút khai báo:

| Trục hành vi | Khi Provider `active` & `declared` | Khi Provider `provider_inactive` (đổi Theme / tắt Plugin) | Khi Group / Target / Field `withdrawn` (rút group, bỏ target khỏi group, hoặc rút field) |
| --- | --- | --- | --- |
| **1. Quản lý dữ liệu trong Admin & API (`EffectiveContentSchema`)** | Hoạt động đầy đủ theo `min_view_role` / `min_edit_role`. | **Giữ nguyên 100% khả năng xem/thêm/sửa/xóa entry và giá trị của cả base CPT fields lẫn extension fields** thông qua **Core Standard Field Fallback Form** (vì mọi `ContentFieldType` đều là kiểu chuẩn của Core). Không loại bỏ base fields hay extension fields khỏi `EffectiveContentSchema`. | **Giữ nguyên 100% khả năng xem/sửa các giá trị đã lưu** qua Core Standard Field Fallback Form và giữ nguyên kiểm tra validation/reference (`media::require_existing_references`); gắn nhãn `(Đã rút khai báo)` trên UI quản trị. |
| **2. Kiểm kê tham chiếu Media (`collect_pins_in_tx` / `media_is_referenced`) & Khóa tên (`field_key`)** | Theo dõi đầy đủ mọi trường `Image`/`Gallery` và giữ khóa `field_key`. | **Giữ nguyên 100%** kiểm kê media (media không bao giờ bị coi là orphan khi tắt plugin/đổi theme) và giữ khóa `field_key`. | **Giữ nguyên 100%** định nghĩa field trong `EffectiveContentSchema`, kiểm kê media (cả khi group bị rút, khi `type_key` bị bỏ khỏi `target_types`, hoặc khi một field bị rút khỏi group — `270-R1`) và giữ khóa `field_key`. |
| **3. Thực thi mã của Provider (`plugin_action`, Wasm Hooks, Package Template)** | Chạy Wasm hooks (`content/before_save`, `content/after_save`), nút `PanelKind::Action` và template của gói đang active. | **Dừng thực thi mã của provider đã tắt:** Wasm hooks của plugin tắt không chạy; nút `PanelKind::Action` hiển thị trạng thái vô hiệu hóa (`provider_inactive`) và bị chặn ở server (`409 provider_inactive`); template của gói tắt chuyển về template của theme đang active hoặc core safe fallback. | **Dừng thực thi mã của khai báo đã rút:** không chạy action/hook của khai báo đã rút; dữ liệu nội dung vẫn render bình thường qua template/fallback hiện hành nếu `public_exposure == Public`. |

---

## 2. Đối chiếu Baseline (`main@a52607c`) → Proposed Delta (`270-R4`), Schema Lưu trữ (`0034`), Bảo toàn Association (`270-R1`) và Kiểm tra tương thích (`R4`, `270-R8`)

### 2.1. Bảng ánh xạ chính xác từ Baseline (`main@a52607c`) sang Contract đề xuất (`270-R4`)

Không dùng tên bảng/kiểu dữ liệu giả định. Mọi thành phần trong contract này ánh xạ trực tiếp tới mã nguồn tại `main@a52607ccb693eefb8ac8473bc513b59e37991bed`:

| Thành phần | Trạng thái tại Baseline (`main@a52607c`) | Thay đổi trong Contract này (`Migration 0034` / Code Delta) |
| --- | --- | --- |
| **Bảng `content_types`** (`0004_content_types.sql`, `0020`, `0023`) | **ĐÃ CÓ:** cột `key, definition_json, owner_kind, owner_id, provider_status, declaration_state, created_at, updated_at`. Các base fields của CPT lưu trong mảng JSON `definition_json.fields` (không có bảng `content_fields` riêng). | **ALTER (`0034`):** Thêm cột `version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)` để phục vụ CAS (`ETag: "v<version>"`) khi Admin cập nhật CPT qua `PUT /api/admin/content-types/{type_key}`. |
| **Builtin `post` & `page`** (`storage/content.rs:17-78`) | **ĐÃ CÓ:** `builtin_definition("post" \| "page")` chứa 2 field `body` (`RichText`) và `featured_image` (`Image`); hàm `preflight_builtin_key_collisions` kiểm tra `serde_json::from_str::<ContentTypeDefinition>(&raw) == builtin_definition(&key)`. | **GIỮ NGUYÊN 100%:** Các thuộc tính mở rộng thêm vào `ContentFieldDefinition` (`min_view_role`, `min_edit_role`, `public_exposure`, `default_value`, `declaration_state`) khi đọc các dòng builtin legacy trong `content_types` có `#[serde(default)]` khớp đúng giá trị của `builtin_definition` (`Editor`, `Editor`, `Public`, `None`, `Declared`), đảm bảo `preflight_builtin_key_collisions` luôn PASS. |
| **Bảng `content_entries`** (`0005_content_entries.sql`, `0008_post_page_publication.sql`) | **ĐÃ CÓ:** cột `id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at`. Dùng chung cho cả `post`, `page` và mọi CPT (không có bảng `posts`/`pages` riêng). Với `post`/`page`, `body` và `featured_image` nằm trong `values_json`. | **GIỮ NGUYÊN 100%:** Giữ duy nhất vị trí chuẩn của `body` và `featured_image` bên trong `content_entries.values_json`. `CompositeEntrySaveInput` **không** tạo trường `body` hay `cover_image` trùng lặp ở cấp ngoài. |
| **Bảng `entry_terms`** (`0014_entry_terms.sql`) | **ĐÃ CÓ:** cột `(entry_id, term_id)`. | **GIỮ NGUYÊN:** Cập nhật nguyên tử cùng `content_entries` trong giao dịch `CompositeEntrySaveInput`. |
| **Bảng `entry_public_paths` & `entry_redirects`** (`0027_seo_routes.sql`) | **ĐÃ CÓ:** lưu custom path (`entry_public_paths(entry_id, path)`) và lịch sử redirect (`entry_redirects(source_path, entry_id, created_at)`). | **GIỮ NGUYÊN:** Tái sử dụng `validate_route_claim_in_tx` và logic cập nhật `entry_public_paths` / `entry_redirects` bên trong giao dịch `CompositeEntrySaveInput`. |
| **Bảng `entry_seo` & `seo_public_media_refs`** (`0027_seo_routes.sql`) | **ĐÃ CÓ:** lưu SEO cấp entry (`entry_seo(entry_id, title, description, og_image_id, canonical_url, noindex)`) và tham chiếu ảnh OG công khai (`seo_public_media_refs`). | **GIỮ NGUYÊN:** Tái sử dụng `validate_seo_input`, `verify_og_media_exists_in_tx` và `sync_entry_seo_media_ref_in_tx` bên trong giao dịch `CompositeEntrySaveInput`. |
| **Kiểm kê tham chiếu Media** (`storage/media_refs.rs`, `storage/media.rs`) | **ĐÃ CÓ:** `collect_pins_in_tx` quét trực tiếp `content_entries.values_json` và các bảng sidecar; `require_existing_references_in_tx` kiểm tra media tồn tại và `lifecycle_state = 'active'` khi ghi. | **MỞ RỘNG HÀM KIỂM TRA:** Mở rộng `collect_image_ids` / `require_existing_references_in_tx` để kiểm tra cả `ContentFieldType::Image` / `Gallery` thuộc `EffectiveContentSchema` (base fields + attached field groups). Không tạo bảng `media_references` mới. |
| **Bảng `content_field_groups` & `content_field_group_targets`** | **CHƯA CÓ** tại baseline. | **TẠO MỚI (`0034`):** Lưu định nghĩa nhóm custom fields (`content_field_groups`) và bảng liên kết chuẩn hóa từng `type_key` kèm trạng thái tombstone (`content_field_group_targets.association_state IN ('declared', 'withdrawn')` — `270-R1`). |
| **Bảng `content_editor_layouts`** | **CHƯA CÓ** tại baseline. | **TẠO MỚI (`0034`):** Lưu cấu hình bố cục `EditorLayoutConfigV1` (bao gồm cả `base_field_panels` — `270-R6`) theo từng `type_key`. |
| **Namespace HTTP API** (`app.rs:373-410`) | **ĐÃ CÓ:** `/api/admin/content-types` và `/api/admin/content-types/{type_key}/entries/{id}...`. | **ĐỒNG NHẤT TIỀN TỐ:** Giữ nguyên toàn bộ route hiện có dưới `/api/admin/content-types/...`; đặt các route mới dưới cùng tiền tố `/api/admin/content-types/{type_key}/...` và `/api/admin/content-field-groups`. |

### 2.2. SQLite DDL dự kiến (`0034_field_groups_and_editor_layout.sql`)

```sql
-- 1. Bổ sung cột version vào bảng content_types hiện có để hỗ trợ CAS ETag ("v<version>")
ALTER TABLE content_types
    ADD COLUMN version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1);

-- 2. Bảng lưu định nghĩa nhóm custom fields (mở rộng cho post, page, và mọi CPT).
CREATE TABLE IF NOT EXISTS content_field_groups (
    group_id        TEXT PRIMARY KEY NOT NULL COLLATE BINARY, -- Canonical ScopedResourceId: 'user:project_meta', 'theme:moc-studio:hero_fields', 'plugin:seo-pro:extra'
    owner_kind      TEXT NOT NULL CHECK (owner_kind IN ('user', 'theme', 'plugin')),
    owner_id        TEXT,                                     -- NULL khi owner_kind = 'user'; PackageId khi 'theme' hoặc 'plugin'
    local_key       TEXT NOT NULL CHECK (length(local_key) BETWEEN 1 AND 64),
    title           TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    description     TEXT NOT NULL DEFAULT '',
    fields_json     TEXT NOT NULL
                    CHECK (json_valid(fields_json) AND json_type(fields_json) = 'array'),
    default_region  TEXT NOT NULL DEFAULT 'main_bottom'
                    CHECK (default_region IN ('main_top', 'main_primary', 'main_bottom', 'sidebar')),
    default_order   INTEGER NOT NULL DEFAULT 50,
    collapsible      INTEGER NOT NULL DEFAULT 1 CHECK (collapsible IN (0, 1)),
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active', 'provider_inactive', 'withdrawn')),
    version         INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (owner_kind = 'user' AND owner_id IS NULL)
        OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_content_field_groups_owner_local
    ON content_field_groups(owner_kind, COALESCE(owner_id, ''), local_key);

-- 3. Bảng liên kết giữa nhóm field và từng type_key (bảo toàn association tombstone khi gỡ target — 270-R1).
CREATE TABLE IF NOT EXISTS content_field_group_targets (
    group_id          TEXT NOT NULL REFERENCES content_field_groups(group_id) ON DELETE RESTRICT,
    type_key          TEXT NOT NULL REFERENCES content_types(key) ON DELETE RESTRICT,
    association_state TEXT NOT NULL DEFAULT 'declared'
                      CHECK (association_state IN ('declared', 'withdrawn')),
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (group_id, type_key)
);

CREATE INDEX IF NOT EXISTS idx_content_field_group_targets_type
    ON content_field_group_targets(type_key, association_state);

-- 4. Bảng lưu cấu hình bố cục màn hình chỉnh sửa (EditorLayoutConfigV1) cấp website cho từng type_key.
CREATE TABLE IF NOT EXISTS content_editor_layouts (
    type_key        TEXT PRIMARY KEY NOT NULL REFERENCES content_types(key) ON DELETE RESTRICT,
    layout_json     TEXT NOT NULL
                    CHECK (json_valid(layout_json) AND json_type(layout_json) = 'object'),
    version         INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1), -- layout_token = 'v<version>'
    updated_by      INTEGER REFERENCES users(id) ON DELETE SET NULL,
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
```

### 2.3. Tách Input DTO (`FieldDefinitionInput`) và Persisted `ContentFieldDefinition` theo Ngữ cảnh (`270-R7`)

Để giải quyết triệt để xung đột giữa mặc định `Public` của base CPT fields và mặc định `Private` của extension field groups mà vẫn giữ được khai báo `"public_exposure": "public"` tường minh trên extension field groups (`270-R7`), `opendash_core::content` tách rõ **Input Wire DTO (`FieldDefinitionInput`)** và **Canonical Persisted Struct (`ContentFieldDefinition`)**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldScopeKind {
    /// Trường thuộc content_types.definition_json.fields (mặc định khi thiếu public_exposure -> Public)
    BaseCptField,
    /// Trường thuộc content_field_groups.fields_json qua User API hoặc Theme/Plugin manifest (mặc định khi thiếu public_exposure -> Private)
    ExtensionGroupField,
}

/// Input DTO dùng cho mọi API nhận định nghĩa field (POST/PUT content-types, POST/PUT content-field-groups)
/// và manifest gói (theme.json / plugin.json). Giữ nguyên Option để phân biệt missing (None) với explicit (Some).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinitionInput {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ContentFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub min_view_role: Option<Role>,
    #[serde(default)]
    pub min_edit_role: Option<Role>,
    /// None khi JSON không gửi "public_exposure"; Some(Public | Private) khi JSON khai báo tường minh.
    #[serde(default)]
    pub public_exposure: Option<PublicExposure>,
    #[serde(default)]
    pub declaration_state: Option<DeclarationState>,
}

impl FieldDefinitionInput {
    /// Chuẩn hóa theo ngữ cảnh (BaseCptField vs ExtensionGroupField) trước khi kiểm tra hợp lệ và lưu DB.
    pub fn into_canonical(self, scope: FieldScopeKind) -> Result<ContentFieldDefinition, &'static str> {
        let min_view_role = self.min_view_role.unwrap_or(Role::Editor);
        let min_edit_role = self.min_edit_role.unwrap_or(Role::Editor);
        if min_edit_role < min_view_role {
            return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        let public_exposure = match (self.public_exposure, scope) {
            (Some(explicit), _) => explicit,
            (None, FieldScopeKind::BaseCptField) => PublicExposure::Public,
            (None, FieldScopeKind::ExtensionGroupField) => PublicExposure::Private,
        };
        if public_exposure == PublicExposure::Public && min_view_role != Role::Editor {
            return Err("invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.");
        }
        let def = ContentFieldDefinition {
            key: self.key,
            label: self.label,
            field_type: self.field_type,
            required: self.required,
            options: self.options,
            default_value: self.default_value,
            min_view_role,
            min_edit_role,
            public_exposure,
            declaration_state: self.declaration_state.unwrap_or(DeclarationState::Declared),
        };
        def.validate()?;
        Ok(def)
    }
}

/// Kiểu chuẩn đã chuẩn hóa dùng để lưu trữ trong DB (luôn serialize tường minh public_exposure, min_view_role, min_edit_role, declaration_state).
/// Các #[serde(default = ...)] chỉ phục vụ đọc tương thích ngược các dòng content_types.definition_json cũ đã lưu trước migration 0034.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFieldDefinition {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ContentFieldType, // ShortText | LongText | RichText | Number | Boolean | Date | Select | Image | Gallery
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(default = "Role::default_editor")]
    pub min_view_role: Role,
    #[serde(default = "Role::default_editor")]
    pub min_edit_role: Role,
    #[serde(default = "PublicExposure::default_public")]
    pub public_exposure: PublicExposure,
    #[serde(default = "DeclarationState::default_declared")]
    pub declaration_state: DeclarationState, // Declared | Withdrawn (270-R1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveFieldStatus {
    Active,
    ProviderInactive,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveFieldEntry {
    pub definition: ContentFieldDefinition,
    pub owner: ResourceOwner,
    /// "base:<type_key>" hoặc ScopedResourceId của content_field_groups
    pub source_id: String,
    pub effective_status: EffectiveFieldStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveContentSchema {
    pub base: OwnedDescriptor<ContentTypeDefinition>,
    pub base_version: i64,
    pub attached_groups: Vec<AttachedFieldGroupSnapshot>, // Gồm cả association_state = Declared và Withdrawn
    pub effective_fields: BTreeMap<String, EffectiveFieldEntry>, // Hợp nhất toàn bộ base fields + attached groups
    pub schema_token: String, // "sha256:<64-hex>"
}
```

### 2.4. Quy tắc Bảo toàn Association khi Gỡ Target (`remove-target`), Rút Field (`remove-field`) hoặc Xóa Group (`R4`, `270-R1`)

Để ngăn chặn hoàn toàn kịch bản mất định nghĩa field/media reference/key reservation khi một nhóm field bị sửa `target_types` hoặc bị rút bớt một field (`270-R1`), hệ thống áp dụng cơ chế bảo vệ ở **cả 3 cấp độ (Group, Target Association, và Field)**:

1. **Cấp 1 — Xóa / Rút cả Nhóm Field (`DELETE /api/admin/content-field-groups/{group_id}` hoặc Package rút group):**
   - Trong `BEGIN IMMEDIATE`, chuyển `content_field_groups.status = 'withdrawn'`, chuyển toàn bộ dòng của group đó trong `content_field_group_targets` sang `association_state = 'withdrawn'`, và tăng `version = version + 1`. Không xóa dòng khỏi `content_field_groups` hay `content_field_group_targets`.
2. **Cấp 2 — Bỏ một `type_key` khỏi `target_types` của Nhóm Field (`remove-target` — `270-R1`):**
   - Khi cập nhật nhóm field của User (`PUT /api/admin/content-field-groups/{group_id}`) hoặc khi đồng bộ lại manifest của Theme/Plugin (`sync_*_cnt_declarations`) mà danh sách `target_types` mới **bỏ đi một `type_key` đã từng gắn với `group_id`**:
     - Server kiểm tra trong `BEGIN IMMEDIATE` xem có bất kỳ bản ghi nào của `type_key` trong `content_entries` đang lưu ít nhất một `field.key` thuộc `group_id` trong `values_json` hay không (`has_persisted_group_values_for_type(tx, type_key, group_id)`):
       - **Đối với `PUT` của User:** Nếu `has_persisted_group_values_for_type == true`, **không xóa dòng `(group_id, type_key)`** — dòng đó trong `content_field_group_targets` được chuyển sang **`association_state = 'withdrawn'`** (tombstone association). Chỉ khi `type_key` có **0** entry chứa bất kỳ `field.key` nào của `group_id` và người dùng chưa từng lưu layout tham chiếu tới group đó trên `type_key` thì dòng liên kết chưa dùng mới được phép gỡ (hoặc vẫn chuyển sang `'withdrawn'`).
       - **Đối với Re-declaration của Theme/Plugin:** Mọi `type_key` bị rút khỏi `target_types` trong manifest mới đều tự động chuyển sang **`content_field_group_targets.association_state = 'withdrawn'`** (không bao giờ `DELETE` dòng trong `content_field_group_targets`).
3. **Cấp 3 — Xóa / Rút một `field` ra khỏi `fields_json` hoặc `definition_json` (`remove-field` — `270-R1`):**
   - **Đối với `PUT` của User trên Base CPT (`PUT /api/admin/content-types/{type_key}`) hoặc trên User Field Group (`PUT /api/admin/content-field-groups/{group_id}`):**
     - Nếu request **bỏ hẳn một `field.key`** đã tồn tại khỏi mảng `fields`:
       - Server kiểm tra xem trên `type_key` (hoặc trên bất kỳ `type_key` nào đang liên kết với `group_id` trong `content_field_group_targets`) đã có entry trong `content_entries` hay chưa (`has_existing_entries`).
       - Nếu `has_existing_entries == true`, **từ chối ngay lập tức với `409 Conflict` (`code: "cannot_remove_field_with_data"`)** — khớp với `storage::content::check_content_type_compatibility`.
       - Để ngừng sử dụng một trường khi đã có dữ liệu mà không làm mất schema/media inventory, Admin gửi trường đó với `"declaration_state": "withdrawn"` (giữ nguyên `key`, `type`, `options`, `min_view_role`, `min_edit_role`, `public_exposure`).
   - **Đối với Re-declaration của Theme/Plugin:**
     - Khi bản cập nhật của Theme hoặc Plugin bỏ một `field.key` đã từng khai báo trong CPT hoặc trong `field_group` của chính gói đó, hàm đồng bộ trong `BEGIN IMMEDIATE` **tự động giữ lại định nghĩa cũ của `field.key` đó trong `definition_json` / `fields_json` với `declaration_state = 'withdrawn'`** (không bao giờ làm biến mất định nghĩa trường khỏi DB).
4. **Quy tắc tính `effective_status` và Hiệu lực của trạng thái `Withdrawn`:**
   - Với mỗi trường trong `EffectiveContentSchema.effective_fields` của `type_key`:
     - Nếu `base.declaration_state == Withdrawn` HOẶC `group.status == Withdrawn` HOẶC `target.association_state == Withdrawn` HOẶC `field.declaration_state == Withdrawn` → `effective_status = EffectiveFieldStatus::Withdrawn`.
     - Ngược lại, nếu `base.status == ProviderInactive` HOẶC `group.status == ProviderInactive` → `effective_status = EffectiveFieldStatus::ProviderInactive`.
     - Ngược lại → `effective_status = EffectiveFieldStatus::Active`.
   - Trường có `effective_status == EffectiveFieldStatus::Withdrawn`:
     - **Giữ nguyên trong `EffectiveContentSchema`:** khóa `field.key` vẫn được bảo lưu trên `type_key` (không nhóm nào khác được phép tạo trùng tên khóa với kiểu dữ liệu khác).
     - **Giữ nguyên kiểm kê và thẩm quyền Media:** các trường `Image` / `Gallery` ở trạng thái `Withdrawn` vẫn được quét trong `public_content::media_is_referenced`, `media::require_existing_references_in_tx`, và `media_refs::collect_pins_in_tx`.
     - **Giữ nguyên quản lý qua Core Standard Field Fallback Form:** các entry đang có dữ liệu của trường `Withdrawn` vẫn hiển thị và cho phép người có quyền (`min_view_role` / `min_edit_role`) xem/sửa/xóa giá trị bình thường kèm nhãn `(Đã rút khai báo)` trên UI; đối với trường `Withdrawn`, cờ bắt buộc hiệu lực (`effective_required`) bằng `false` cho các entry chưa có giá trị của trường đó.
   - Trường có `effective_status == EffectiveFieldStatus::ProviderInactive` hoặc `EffectiveFieldStatus::Active` giữ nguyên `effective_required = field.definition.required`.

### 2.5. Kiểm tra tương thích & Bất biến Khả năng Tạo mới ở MỌI Transition (`R4`, `270-R1`, `270-R8`)

Hàm kiểm tra `verify_effective_schema_transition(old_effective, new_effective, current_layout, has_existing_entries)` phải được gọi bên trong giao dịch `BEGIN IMMEDIATE` ở **mọi đường thay đổi schema hoặc layout**:
1. Tạo / cập nhật Base CPT của User (`POST /api/admin/content-types`, `PUT /api/admin/content-types/{type_key}`).
2. Tạo mới / cập nhật / rút Field Group (`POST /api/admin/content-field-groups`, `PUT /api/admin/content-field-groups/{group_id}`, `DELETE /api/admin/content-field-groups/{group_id}`).
3. Lưu cấu hình bố cục (`PUT /api/admin/content-types/{type_key}/editor-layout`).
4. Kích hoạt revision của Theme hoặc bật/cập nhật Plugin (`activate_theme_revision`, `enable_plugin_revision`).

**Các bất biến bắt buộc trong `verify_effective_schema_transition`:**
- **Rule COMPAT-1 (Chống đụng độ `field_key` toàn cục trên mỗi `type_key`):**
  - Tập khóa hệ thống bị cấm đặt trùng cho custom field (trừ `body` và `featured_image` vốn thuộc `builtin_definition("post" | "page")`):
    `id`, `content_type_key`, `type_key`, `slug`, `title`, `publication_status`, `status`, `version`, `created_at`, `updated_at`, `term_ids`, `custom_path`, `seo`.
  - Trên mỗi `type_key`, toàn bộ `base.descriptor.fields` (cả `declared` và `withdrawn`) và toàn bộ `fields` của mọi `content_field_groups` liên kết qua `content_field_group_targets` (cả `declared` và `withdrawn`, thuộc mọi `status IN ('active', 'provider_inactive', 'withdrawn')`) phải có `field.key` **duy nhất tuyệt đối**. Mọi xung đột `field.key` bị từ chối với `409 Conflict` (`code: "field_key_collision"`).
- **Rule COMPAT-2 (Bảo toàn kiểu và miền giá trị của dữ liệu đã lưu):**
  - Không được đổi `ContentFieldType` của một `field.key` đã tồn tại; không được xóa phương án `SelectOption` khi `has_existing_entries == true`; không được xóa hẳn `field.key` khi `has_existing_entries == true` (`409 Conflict`, `code: "incompatible_schema_change"` / `"cannot_remove_field_with_data"`).
- **Rule COMPAT-3 (Quy tắc `default_value` & Cấm dùng `default_value` để lách Compatibility trên dữ liệu cũ — `R4`):**
  - **Kiểm tra hợp lệ của `default_value` ngay khi đăng ký schema:** Nếu một field khai báo `default_value: Some(val)`, `val` phải vượt qua `valid_field_value(field, val)` trong `opendash_core::content`. Các kiểu tham chiếu media (**`ContentFieldType::Image`**, **`ContentFieldType::Gallery`**) **bị cấm khai báo `default_value` tĩnh** trong schema (`422 Unprocessable Entity`, `code: "invalid_field_default"`).
  - **Cấm biến field sang `required: true` khi đã có entry:** Khi `has_existing_entries == true` trên `type_key`, mọi thao tác thêm mới một field có `effective_required == true` hoặc đổi một field đang `effective_required == false` sang `effective_required == true` đều **bị từ chối với `409 Conflict` (`code: "incompatible_schema_change"`) ngay cả khi có khai báo `default_value`** (không tự ý backfill ngầm lên các bản ghi cũ).
- **Rule COMPAT-4 (Bất biến Khả năng Tạo mới của `Editor` trên MỌI Transition, độc lập với `has_existing_entries` — `R4`, `270-R8`):**
  - Vì `Editor` luôn có quyền tạo mới entry trên mọi `type_key`, nên ở **mọi trạng thái sau transition (bất kể `has_existing_entries` là `true` hay `false`)**, với mọi trường `f` trong `new_effective.effective_fields` có `effective_required == true` (tức `f.definition.required == true` VÀ `f.effective_status != EffectiveFieldStatus::Withdrawn`, bao gồm cả `Active` lẫn `ProviderInactive`):
    1. `f.definition.min_edit_role >= f.definition.min_view_role` luôn đúng (`Rule ACL-1`, vi phạm trả `422 invalid_field_acl`).
    2. **Trường hợp A (`f.definition.min_edit_role == Role::Editor`):** Người biên tập (`Editor`) bắt buộc phải nhìn thấy và nhập được trường này trên màn hình Single Edit. Do đó, panel chứa trường `f` trong `current_layout` (dù là `BaseFieldsSlice`, `core:base_fields` hay `PanelKind::ContentFields`):
       - **Không được phép** có `placement.enabled == false`,
       - **Không được phép** có `panel.min_view_role == Role::Admin` hoặc `panel.min_edit_role == Role::Admin`.
       - Mọi thao tác sửa schema/group, re-declaration gói hoặc sửa layout vi phạm điều kiện này bị từ chối với `422 Unprocessable Entity` (`code: "required_field_hidden_or_inaccessible"`).
    3. **Trường hợp B (`f.definition.min_edit_role == Role::Admin` — kiểm tra vô điều kiện cả khi `has_existing_entries == false` LẪN khi `has_existing_entries == true` — `270-R8`):**
       - Vì `Editor` không được phép gửi trường `f` khi tạo mới entry (`403 field_edit_forbidden`), trường `f` **bắt buộc phải có `f.definition.default_value == Some(val)` hợp lệ** để server tự động điền khi `Editor` gọi `POST` tạo mới entry!
       - Do đó, bất kỳ transition nào dẫn tới trạng thái `(effective_required == true && min_edit_role == Role::Admin && default_value.is_none())` — bao gồm:
         a) Tạo mới một trường `required: true, min_edit_role: Admin` mà không có `default_value`,
         b) **Đổi `min_edit_role` từ `Editor` sang `Admin`** trên một trường `required: true` hiện hữu (dù `has_existing_entries == true` hay `false`) mà không đồng thời cung cấp `default_value` hợp lệ,
         c) **Xóa `default_value` (`Some -> None`)** trên một trường `required: true, min_edit_role: Admin` hiện hữu (dù `has_existing_entries == true` hay `false`, qua User `PUT` hay Package re-declaration)
       - đều bị **từ chối ngay lập tức với `422 Unprocessable Entity` (`code: "admin_required_field_missing_default"`)**!

---

## 3. Panel Registry (`PanelDefinition` — `270-R9`), Chia nhóm Base Fields (`base_field_panels` — `270-R6`), `EditorLayoutConfigV1` và Đăng ký Declarative

### 3.1. Các vùng hiển thị (`EditorRegion`) & Thứ tự đọc thống nhất trên Mobile

Màn hình Single Edit có 5 vùng cố định:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorRegion {
    HeaderBar,   // Dành riêng cho core:header_bar (Breadcrumb, Tiêu đề màn hình, Xem trước, Lưu/Xuất bản)
    MainTop,     // Phía trên nội dung chính (cột trái desktop)
    MainPrimary, // Vùng nội dung chính: core:title_slug, core:body_editor / nút mở Builder, core:base_fields (fallback)
    MainBottom,  // Phía dưới nội dung chính (cột trái desktop: base_field_panels, field groups, core:seo, ...)
    Sidebar,     // Cột phải desktop: core:publish_box, core:page_attributes, core:featured_image, core:taxonomies, sidebar panels, action panels
}
```

- **Thứ tự DOM & Thứ tự đọc một cột trên Mobile (`< 1024px`):**
  - Thống nhất tuyệt đối theo đúng ngữ nghĩa tên vùng (`main_top` nằm phía trên `main_primary`):
    **`header_bar → main_top → main_primary → main_bottom → sidebar`**.

### 3.2. Định nghĩa `PanelDefinition` đầy đủ (`270-R9`), Chia nhóm Base Fields (`BaseFieldSlicePanelConfig` — `270-R6`) và `EditorLayoutConfigV1`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelKind {
    /// Panel lõi của hệ thống (header_bar, title_slug, body_editor, publish_box, page_attributes, featured_image, taxonomies, seo, base_fields)
    CoreBuiltIn {
        core_key: SnakeKey,
    },
    /// Panel chia nhóm trình bày cục bộ trong layout cho một tập con các base fields thuộc content_types.definition_json.fields (270-R6)
    BaseFieldsSlice {
        slice_key: SnakeKey, // panel_id tương ứng: "layout:<slice_key>"
        field_keys: Vec<SnakeKey>,
    },
    /// Panel hiển thị và chỉnh sửa một nhóm custom fields thuộc bảng content_field_groups
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

/// Cấu trúc PanelDefinition đầy đủ do Server tổng hợp và quản lý (270-R9).
/// Các trường owner, panel_id, required_core và effective_status là server-owned, không bao giờ tin cậy trực tiếp từ client/manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelDefinition {
    pub panel_id: String,                   // "core:<key>", "layout:<slice_key>", "user:<key>", "theme:<id>:<key>", "plugin:<id>:<key>"
    pub owner: ResourceOwner,               // Server-owned: Core | User | Theme(id) | Plugin(id)
    pub title: String,
    pub kind: PanelKind,
    pub applies_to: Vec<String>,            // Danh sách type_key áp dụng ('post', 'page', hoặc CPT key)
    pub default_region: EditorRegion,       // Phải nằm trong allowed_regions (HeaderBar chỉ dành cho core:header_bar)
    pub allowed_regions: Vec<EditorRegion>, // Tập vùng hợp lệ mà panel này được phép đặt vào
    pub default_order: i32,                 // [-1000..=1000]
    pub min_view_role: Role,
    pub min_edit_role: Role,                // Bắt buộc min_edit_role >= min_view_role
    pub collapsible: bool,
    pub required_core: bool,                // Server-owned: chỉ true đối với core:header_bar, core:title_slug, core:publish_box
    pub effective_status: EffectiveFieldStatus, // Active | ProviderInactive | Withdrawn
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutFieldSubgroup {
    pub subgroup_id: SnakeKey,
    pub title: String,
    pub field_keys: Vec<SnakeKey>,
}

/// Cấu hình một panel chia nhóm từ base CPT fields (layout-only, không thay đổi định nghĩa hay khóa dữ liệu — 270-R6)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseFieldSlicePanelConfig {
    /// Định dạng bắt buộc: "layout:<slice_key>" (ví dụ "layout:project_info", "layout:project_status")
    pub panel_id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub collapsible: bool,
    /// Danh sách các field.key thuộc content_types.definition_json.fields của chính type_key này (1..=64 keys)
    pub field_keys: Vec<SnakeKey>,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelPlacementConfig {
    pub panel_id: String, // "core:<key>", "layout:<slice_key>", "user:<key>", "theme:<id>:<key>", "plugin:<id>:<key>"
    pub enabled: bool,
    pub order: i32,
    #[serde(default)]
    pub collapsed_by_default: bool,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>, // Chỉ áp dụng cho PanelKind::ContentFields, BaseFieldsSlice hoặc core:base_fields
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRegionsConfig {
    pub main_top: Vec<PanelPlacementConfig>,
    pub main_primary: Vec<PanelPlacementConfig>,
    pub main_bottom: Vec<PanelPlacementConfig>,
    pub sidebar: Vec<PanelPlacementConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorLayoutConfigV1 {
    pub schema_version: u32, // Cố định bằng 1 ở V1
    pub type_key: String,
    pub version: u64,        // layout_token = format!("v{}", version)
    /// Danh sách các nhóm panel chia từ base fields của CPT (mặc định rỗng nếu dùng chung 1 panel core:base_fields)
    #[serde(default)]
    pub base_field_panels: Vec<BaseFieldSlicePanelConfig>,
    pub regions: EditorRegionsConfig,
}
```

### 3.3. Bảng Ma trận Allowlist `Owner × PanelKind × Namespace × AllowedRegions` & Quy tắc Kiểm tra / Merge Layout (`270-R6`, `270-R9`)

1. **Ma trận Allowlist theo Nguồn Đăng ký (`270-R9`):**
   Không một thực thể nào được đăng ký vượt ra ngoài dòng của mình trong bảng dưới đây (vi phạm trả `422 Unprocessable Entity`):

   | Nguồn đăng ký (`Owner`) | Tiền tố `panel_id` bắt buộc | `PanelKind` được phép | `allowed_regions` tối đa được phép | `required_core` |
   | --- | --- | --- | --- | --- |
   | **Core System** (`ResourceOwner::Core`) | `core:<core_key>` | `PanelKind::CoreBuiltIn` | `HeaderBar` (chỉ `core:header_bar`); `[MainTop, MainPrimary, MainBottom, Sidebar]` cho các panel core khác | `true` với `header_bar`, `title_slug`, `publish_box`; `false` với panel core khác |
   | **Layout-Local Base Slice** (`EditorLayoutConfigV1.base_field_panels`) | `layout:<slice_key>` (cục bộ trong `type_key`) | `PanelKind::BaseFieldsSlice` | `[MainTop, MainPrimary, MainBottom, Sidebar]` (CẤM `HeaderBar`) | Luôn `false` (nhưng bị khóa bởi `Rule COMPAT-4` nếu chứa field `required: true`) |
   | **User Field Group** (`ResourceOwner::User`) | `user:<local_key>` | `PanelKind::ContentFields` | `[MainTop, MainPrimary, MainBottom, Sidebar]` (CẤM `HeaderBar`) | Luôn `false` |
   | **Theme Package** (`ResourceOwner::Theme(theme_id)`) | `theme:<theme_id>:<local_key>` | **Chỉ `PanelKind::ContentFields`** (CẤM `CoreBuiltIn`, `BaseFieldsSlice`, `Action`) | `[MainTop, MainPrimary, MainBottom, Sidebar]` (CẤM `HeaderBar`) | Luôn `false` |
   | **Plugin Package** (`ResourceOwner::Plugin(plugin_id)`) | `plugin:<plugin_id>:<local_key>` | `PanelKind::ContentFields` (từ `field_groups`) **HOẶC** `PanelKind::Action` (từ `editor_panels`) (CẤM `CoreBuiltIn`, `BaseFieldsSlice`) | Với `ContentFields`: `[MainTop, MainPrimary, MainBottom, Sidebar]`; Với `Action`: **`[MainTop, MainBottom, Sidebar]`** (CẤM `HeaderBar` và `MainPrimary`) | Luôn `false` |

2. **Quy tắc kiểm tra hợp lệ (`validate_editor_layout_config` — `270-R6`, `270-R9`):**
   Khi Admin lưu `PUT /api/admin/content-types/{type_key}/editor-layout` hoặc khi hệ thống dựng `EffectiveEditorConfigProjection`:
   - **Rule LAY-1 (`schema_version` & Quotas):**
     - `schema_version` phải bằng `1` (`422 unsupported_layout_schema_version`).
     - Hạn mức trên mỗi `type_key`: tối đa `16` `base_field_panels`, tối đa `64` `PanelPlacementConfig` tổng cộng trên 4 vùng, tối đa `12` `subgroups` trong một panel, và tối đa `64` `field_keys` trong một panel (`422 layout_quota_exceeded`).
   - **Rule LAY-2 (Phân hoạch Base CPT Fields `base_field_panels` + Fallback `core:base_fields` — `270-R6`):**
     - Gọi `eligible_base_fields` là tập các trường trong `content_types.definition_json.fields` của `type_key` (trừ `body` và `featured_image` của `post`/`page` vốn luôn gắn với `core:body_editor` và `core:featured_image`).
     - Với mỗi `slice` trong `base_field_panels`:
       - `slice.panel_id` phải có dạng `"layout:<slice_key>"` với `<slice_key>` hợp lệ theo `valid_snake_key` và không trùng lặp (`422 invalid_or_duplicate_slice_panel_id`).
       - `slice.field_keys` phải gồm `1..=64` khóa thuộc `eligible_base_fields` (`422 unknown_base_field_key`), và mỗi base `field_key` chỉ được xuất hiện tối đa **1 lần** trên toàn bộ `base_field_panels` (`422 duplicate_base_field_placement`).
       - Mỗi `slice` tự động sinh ra một `PanelDefinition` tương ứng có `panel_id = slice.panel_id`, `kind = PanelKind::BaseFieldsSlice`, `allowed_regions = [MainTop, MainPrimary, MainBottom, Sidebar]`, `min_view_role = min(field.min_view_role)`, `min_edit_role = min(field.min_edit_role)`.
       - Mỗi `slice.panel_id` bắt buộc phải có đúng **1** `PanelPlacementConfig` trong `regions` (`422 unplaced_base_field_slice`).
     - **Fallback tự động `core:base_fields`:** Gọi `unassigned_base_fields` là các trường trong `eligible_base_fields` chưa được liệt kê trong bất kỳ `base_field_panels` nào. Nếu `unassigned_base_fields` không rỗng, panel `core:base_fields` tự động chứa đúng `unassigned_base_fields` theo thứ tự khai báo; nếu `unassigned_base_fields` rỗng thì `core:base_fields` tự động ẩn khi render.
   - **Rule LAY-3 (Kiểm tra tham chiếu `panel_id`, Chống trùng lặp & Guard `allowed_regions` — `270-R9`):**
     - Mỗi `panel_id` trong `EditorRegionsConfig` phải tham chiếu tới một `PanelDefinition` đang áp dụng cho `type_key` (`applies_to.contains(type_key)`) (`422 unknown_or_inapplicable_panel_id`).
     - Mỗi `panel_id` chỉ được xuất hiện tối đa **1 lần** trên toàn bộ 4 danh sách vùng trong `EditorRegionsConfig` (`422 duplicate_panel_placement`).
     - Vùng `region` đặt `panel_id` **bắt buộc phải nằm trong `panel_def.allowed_regions`** và `default_region` của mọi `PanelDefinition` cũng phải nằm trong `allowed_regions` (`422 invalid_panel_region`).
   - **Rule LAY-4 (Bảo vệ Panel lõi & Required Fields `Rule COMPAT-4`):**
     - Vùng `header_bar` luôn cố định chứa `core:header_bar`.
     - Các panel có `required_core == true` (`core:title_slug`, `core:publish_box`) không được đặt `enabled: false` (`422 required_core_panel_disabled`).
     - Áp dụng `Rule COMPAT-4` (Mục `2.5`): không được đặt `enabled: false` cho bất kỳ panel nào chứa trường `effective_required == true` và `min_edit_role == Editor` (`422 required_field_hidden_or_inaccessible`).
   - **Rule LAY-5 (Kiểm tra `subgroups` & Tự động nối trường mới vào Panel):**
     - Chỉ các panel thuộc `PanelKind::ContentFields`, `PanelKind::BaseFieldsSlice` hoặc `core:base_fields` mới được có `subgroups` không rỗng (`422 subgroups_not_allowed_on_panel_kind`).
     - Mọi `field_key` trong `subgroups` phải thuộc tập field của chính panel đó (`422 unknown_field_in_subgroup`) và không được lặp lại ở 2 subgroup (`422 duplicate_field_in_subgroups`). Các trường của panel chưa liệt kê trong `subgroups` được tự động nối vào cuối panel theo thứ tự khai báo.
   - **Rule LAY-6 (Tự động chèn Panel mới đăng ký chưa có trong `content_editor_layouts` & Sắp xếp tất định — `270-R9`):**
     - Khi dựng cấu hình bố cục cho `type_key` (dù `content_editor_layouts` chưa có dòng nào cho `type_key` hay **đã có dòng lưu từ trước nhưng sau đó Admin tạo thêm User Field Group mới hoặc cài/nâng cấp Theme/Plugin có thêm `field_group` / `editor_panel` mới áp dụng cho `type_key`**):
       1. Giữ nguyên 100% vị trí (`region`, `order`, `enabled`, `collapsed_by_default`, `subgroups`) của toàn bộ các `panel_id` đã có trong `content_editor_layouts`.
       2. Với mỗi `panel_def` áp dụng cho `type_key` mà `panel_def.panel_id` **chưa xuất hiện** trong `EditorRegionsConfig` đã lưu: hệ thống **tự động nối (auto-append)** một `PanelPlacementConfig` mặc định vào vùng `panel_def.default_region` với `enabled = true`, `order = panel_def.default_order`, `collapsed_by_default = false`, `subgroups = []`.
       3. Trong mỗi vùng (`main_top`, `main_primary`, `main_bottom`, `sidebar`), sắp xếp tất định các panel theo khóa tăng dần:
          `(placement.order ASC, owner_rank(panel_def.owner) ASC, panel_def.panel_id.as_str() ASC)`
          với `owner_rank`: `Core = 0`, `LayoutSlice = 1`, `User = 2`, `Theme = 3`, `Plugin = 4`.

### 3.4. Wire Shape Đăng ký Declarative trong Manifest của Theme (`theme.json` v2) và Plugin (`plugin.json` `manifest_schema: 2`) (`270-R7`, `270-R9`)

Theme và Plugin đăng ký nhóm field và panel bằng cấu trúc JSON tĩnh được kiểm tra nghiêm ngặt bằng `#[serde(deny_unknown_fields)]` (không nạp JS/React/HTML tùy ý, không cho phép khai báo `owner`, `panel_id` có tiền tố `core:`/`layout:`, hay `PanelKind::CoreBuiltIn`/`BaseFieldsSlice`):

```rust
/// Wire DTO cho mục `field_groups` trong cả `theme.json` (v2) và `plugin.json` (manifest_schema: 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFieldGroupInput {
    /// local_key theo valid_snake_key. Server tự sinh group_id = panel_id = "<theme|plugin>:<pkg_id>:<local_key>"
    pub local_key: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// 1..=16 valid_content_type_key ('post', 'page', hoặc CPT key)
    pub target_types: Vec<String>,
    #[serde(default = "EditorRegion::default_main_bottom")]
    pub default_region: EditorRegion, // Phải thuộc [MainTop, MainPrimary, MainBottom, Sidebar]; CẤM HeaderBar
    #[serde(default = "default_panel_order_50")]
    pub default_order: i32,           // [-1000..=1000]
    #[serde(default = "default_true")]
    pub collapsible: bool,
    /// 1..=64 FieldDefinitionInput; mỗi field được chuẩn hóa qua into_canonical(FieldScopeKind::ExtensionGroupField) (270-R7)
    pub fields: Vec<FieldDefinitionInput>,
}

/// Wire DTO cho mục `editor_panels` (Action Panels) — CHỈ hợp lệ trong `plugin.json` (manifest_schema: 2).
/// Nếu `theme.json` chứa `editor_panels` hoặc khai báo kind khác Action -> từ chối 422 ngay khi validate manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestActionPanelInput {
    /// local_key theo valid_snake_key. Server tự sinh panel_id = "plugin:<plugin_id>:<local_key>"
    pub local_key: String,
    pub title: String,
    /// Phải khớp với một `id` trong mảng `actions` của cùng `plugin.json`
    pub action_id: String,
    pub button_label: String,
    #[serde(default)]
    pub confirm_message: Option<String>,
    #[serde(default)]
    pub public_effect: bool,
    /// 1..=16 valid_content_type_key
    pub applies_to: Vec<String>,
    #[serde(default = "EditorRegion::default_sidebar")]
    pub default_region: EditorRegion,       // Phải nằm trong allowed_regions
    #[serde(default = "EditorRegion::default_action_allowed_regions")]
    pub allowed_regions: Vec<EditorRegion>, // Tập con không rỗng của [MainTop, MainBottom, Sidebar]; CẤM HeaderBar và MainPrimary
    #[serde(default = "default_panel_order_80")]
    pub default_order: i32,                 // [-1000..=1000]
    #[serde(default)]
    pub min_view_role: Option<Role>,
    #[serde(default)]
    pub min_edit_role: Option<Role>,
    #[serde(default = "default_true")]
    pub collapsible: bool,
}
```

- **Quy tắc kiểm tra Manifest khi cài đặt / kích hoạt gói (`270-R7`, `270-R9`):**
  1. Hạn mức mỗi gói: tối đa `16` `field_groups`, tối đa `64` `fields` trong một group, tối đa `16` `editor_panels` (đối với plugin).
  2. Không được trùng `local_key` giữa các `field_groups` và `editor_panels` trong cùng một gói (`422 duplicate_panel_id`).
  3. Với mỗi `ManifestFieldGroupInput`:
     - Chuẩn hóa từng phần tử trong `fields` bằng `field_input.into_canonical(FieldScopeKind::ExtensionGroupField)` (`270-R7`: thiếu `public_exposure` → `Private`; khai báo tường minh `"public"` → `Public`).
     - Tự động tạo `PanelDefinition` tương ứng có `panel_id = group_id`, `owner = Theme(id) | Plugin(id)`, `kind = PanelKind::ContentFields { field_group_id }`, `applies_to = target_types`, `default_region` (thuộc `[MainTop, MainPrimary, MainBottom, Sidebar]`), `allowed_regions = vec![MainTop, MainPrimary, MainBottom, Sidebar]`, `min_view_role = min(fields.min_view_role)`, `min_edit_role = min(fields.min_edit_role)`, `required_core = false`.
  4. Với mỗi `ManifestActionPanelInput` (chỉ trong `plugin.json` khi `manifest_schema == 2`):
     - Kiểm tra `action_id` tồn tại trong `manifest.actions`.
     - Kiểm tra `allowed_regions` không rỗng, chỉ chứa các phần tử thuộc `[MainTop, MainBottom, Sidebar]` (nếu chứa `HeaderBar` hoặc `MainPrimary` → từ chối `422 invalid_panel_region`), và `default_region` nằm trong `allowed_regions`.
     - Kiểm tra `min_edit_role >= min_view_role` và `min_edit_role >= action_decl.required_role` (`422 invalid_panel_acl`).

---

## 4. Ma trận Endpoint (`/api/admin/content-types/...`), `schema_token` (`R3`), `PatchField<T>` 3 trạng thái (`270-R5`) và Quyền Private của Plugin (`270-R3`)

### 4.1. Ma trận Endpoint & Quy tắc Precondition (`R3`, `270-R4`)

Toàn bộ API mới sử dụng tiền tố chuẩn `/api/admin/content-types` và `/api/admin/content-field-groups` khớp với `cms/crates/opendash-server/src/app.rs:373-410`. Các endpoint entry rời rạc hiện có (`/entries`, `/entries/{id}`, `/publication`, `/preview`, `/route`, `/seo`) được giữ nguyên để tương thích ngược và đều áp dụng hàng rào kiểm tra snapshot ở Pha 3:

| Phương thức & Đường dẫn | Trạng thái | Quyền tối thiểu | CSRF / Origin | Precondition (`If-Match` / Token) | Mục đích & Hành vi chính |
| --- | --- | --- | --- | --- | --- |
| `GET /api/admin/content-types` | Đã có (mở rộng) | `Editor` | Không (GET) | Trả `version` (`ETag: "v<ver>"`) và `schema_token` cho từng CPT | Liệt kê CPT kèm `owner`, `status`, `declaration_state`, `version`, `schema_token`. |
| `POST /api/admin/content-types` | Đã có | `Admin` | Bắt buộc | **Không yêu cầu `If-Match`** (tạo mới; trùng key trả `409`) | Tạo CPT mới thuộc `ResourceOwner::User` với `version = 1` (chuẩn hóa fields qua `FieldScopeKind::BaseCptField` và kiểm tra `verify_effective_schema_transition`). |
| `PUT /api/admin/content-types/{type_key}` | **Mới** | `Admin` | Bắt buộc | `If-Match: "v<cpt_version>"` (`428` nếu thiếu, `412` nếu lệch) | Cập nhật CPT thuộc `ResourceOwner::User` trong `BEGIN IMMEDIATE` (chuẩn hóa fields qua `FieldScopeKind::BaseCptField`, chạy `verify_effective_schema_transition`, tăng `content_types.version = version + 1`). Từ chối `403` nếu CPT thuộc `Core`/`Theme`/`Plugin`. |
| `GET /api/admin/content-field-groups` | **Mới** | `Admin` | Không (GET) | Trả `version` từng group | Liệt kê toàn bộ `content_field_groups` kèm `targets` (`declared`/`withdrawn`). |
| `POST /api/admin/content-field-groups` | **Mới** | `Admin` | Bắt buộc | **Không yêu cầu `If-Match`** (tạo mới; trùng ID/field_key trả `409`) | Tạo nhóm custom fields thuộc `ResourceOwner::User` với `version = 1` (chuẩn hóa fields qua `FieldScopeKind::ExtensionGroupField` — `270-R7`). |
| `PUT /api/admin/content-field-groups/{group_id}` | **Mới** | `Admin` | Bắt buộc | `If-Match: "v<group_version>"` (`428` / `412`) | Cập nhật nhóm field thuộc `ResourceOwner::User` (chuẩn hóa fields qua `FieldScopeKind::ExtensionGroupField`, áp dụng bảo toàn association `270-R1` và `verify_effective_schema_transition` `270-R8`). |
| `DELETE /api/admin/content-field-groups/{group_id}` | **Mới** | `Admin` | Bắt buộc | `If-Match: "v<group_version>"` (`428` / `412`) | **Rút nhóm field (`status = 'withdrawn'` và `association_state = 'withdrawn'`)**, giữ nguyên `fields_json`, khóa `field_key` và kiểm kê media (`R4`, `270-R1`). |
| `GET /api/admin/content-types/{type_key}/editor-layout` | **Mới** | `Admin` | Không (GET) | Trả `ETag: "v<layout_version>"` (`"v0"` nếu chưa có override) | Đọc cấu hình raw `EditorLayoutConfigV1` và danh sách `PanelDefinition` (đã auto-append panel mới theo `Rule LAY-6` — `270-R9`) cho màn hình quản trị bố cục. |
| `PUT /api/admin/content-types/{type_key}/editor-layout` | **Mới** | `Admin` | Bắt buộc | `If-Match: "v<layout_version>"` (`"v0"` khi lưu override đầu tiên; `428` / `412`) | Lưu `EditorLayoutConfigV1` (kiểm tra `validate_editor_layout_config`, `base_field_panels` `270-R6`, `allowed_regions` `270-R9` và `Rule COMPAT-4` `270-R8`). Tăng `layout_version`, giữ nguyên `schema_token`. |
| **`GET /api/admin/content-types/{type_key}/effective-editor-config`** | **Mới** | **`Editor` + `Admin`** | Không (GET) | Trả `schema_token` (`sha256:<64-hex>`) và `layout_token` (`v<ver>`) | **Projection cấu hình hiệu lực theo phiên của người gọi (`R3`):** chỉ trả các field và panel có `caller.role >= min_view_role`, gắn cờ `editable: caller.role >= min_edit_role` và `effective_status`. Dùng để dựng Single Edit và kiểm quyền khôi phục nháp (`R5`). |
| `POST /api/admin/content-types/{type_key}/entries/composite` | **Mới** | `Editor` | Bắt buộc | Body bắt buộc có `expected_schema_token` (`428` nếu thiếu/rỗng, `409` nếu lệch); `expected_version` phải là `None` (`400` nếu gửi `Some`) | Tạo mới nguyên tử (`content_entries` + `entry_terms` + `entry_public_paths` + `entry_seo`) trong 1 giao dịch `BEGIN IMMEDIATE`. |
| `PUT /api/admin/content-types/{type_key}/entries/{id}/composite` | **Mới** | `Editor` | Bắt buộc | Body bắt buộc có `expected_version` (`428` nếu thiếu, `412` nếu lệch) **và** `expected_schema_token` (`428` nếu thiếu/rỗng, `409` nếu lệch) | Cập nhật nguyên tử (patch/merge 3 trạng thái theo quyền + 1 lần tăng `version + 1`). |

### 4.2. Định nghĩa tất định của `schema_token` (`sha256:<64-hex>` — `R3`)

- `schema_token` có dạng `"sha256:<64-lowercase-hex>"`, tính bằng SHA-256 trên chuỗi JSON chuẩn hóa (RFC 8785 JCS / canonical `BTreeMap` ordering) của `SchemaTokenInput`:
  1. `base_type`: `(key, owner, provider_status, declaration_state, version, taxonomy_eligibility_categories_tags, ordered_base_fields)` — trong đó mỗi field gồm đầy đủ `(key, label, field_type, required, options, default_value, min_view_role, min_edit_role, public_exposure, declaration_state)`.
  2. `attached_groups`: danh sách tất cả các nhóm trong `content_field_group_targets` gắn với `type_key` (cả `association_state = 'declared'` và `'withdrawn'`), sắp xếp theo `group_id ASC`, mỗi nhóm gồm `(group_id, owner, group_status, association_state, group_version, ordered_fields_with_full_acl_default_exposure_and_declaration_state)`.
  3. `provider_generations`: `(active_theme_revision_id, enabled_plugins_for_type: Vec<(plugin_id, revision_generation)>)`.
- Khi `PUT /api/admin/content-types/{type_key}`, `POST/PUT/DELETE /api/admin/content-field-groups`, hoặc kích hoạt/tắt Theme/Plugin commit thành công trong SQLite, `schema_token` tự động thay đổi ngay lập tức theo dữ liệu đã commit.

### 4.3. Kiểu `PatchField<T>` 3 trạng thái, DTO `CompositeEntrySaveInput` & Ngữ nghĩa `Omitted = Preserve` (`270-R4`, `270-R5`)

Vì `Option<Option<T>>` mặc định của `serde` không phân biệt được trường bị bỏ qua (missing) với trường có giá trị JSON `null` (`270-R5`), và vì nếu để `expected_schema_token: String` bắt buộc ở tầng serde thì khi thiếu trường đó Axum sẽ trả `400/422` thay vì `428 Precondition Required`, contract quy định kiểu `PatchField<T>` có custom deserializer tường minh:

```rust
use serde::{Deserialize, Deserializer};

/// Kiểu 3 trạng thái phân biệt chính xác: không gửi key (Omitted), gửi JSON null (Clear), và gửi giá trị (Set).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PatchField<T> {
    #[default]
    Omitted,
    Clear,
    Set(T),
}

pub fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<PatchField<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        None => Ok(PatchField::Clear),
        Some(val) => Ok(PatchField::Set(val)),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeEntrySaveInput {
    /// None khi POST create; Some(v) bắt buộc khi PUT update (nếu thiếu khi PUT -> trả 428 missing_expected_version)
    #[serde(default)]
    pub expected_version: Option<i64>,
    /// Dùng Option<String> ở tầng serde để khi client không gửi hoặc gửi chuỗi rỗng,
    /// handler chủ động trả đúng mã 428 Precondition Required (code: "missing_schema_token").
    #[serde(default)]
    pub expected_schema_token: Option<String>,
    pub title: String,
    pub slug: String,
    /// Chỉ áp dụng cho post/page (hoặc giữ nguyên publication_status hiện có nếu None).
    #[serde(default)]
    pub publication_status: Option<String>,
    /// Bản đồ giá trị các trường trong content_entries.values_json (bao gồm cả "body" và "featured_image" của post/page).
    /// - Khóa KHÔNG xuất hiện trong map `values` -> Omitted (giữ nguyên giá trị trong DB khi update; áp dụng default_value khi create).
    /// - Khóa xuất hiện với giá trị `serde_json::Value::Null` ("k": null) -> Clear (xóa khóa "k" khỏi BTreeMap values_json).
    /// - Khóa xuất hiện với giá trị khác Null ("k": val) -> Set(val) (cập nhật khóa "k" trong values_json).
    #[serde(default)]
    pub values: BTreeMap<String, serde_json::Value>,
    /// - None (không gửi key `term_ids`) -> giữ nguyên entry_terms hiện có khi update (hoặc [] khi create).
    /// - Some([]) ("term_ids": []) -> xóa toàn bộ liên kết entry_terms của entry.
    /// - Some([1, 2]) -> thay thế liên kết entry_terms bằng danh sách ID hợp lệ.
    #[serde(default)]
    pub term_ids: Option<Vec<i64>>,
    /// 3 trạng thái nhờ #[serde(default, deserialize_with = "deserialize_patch_field")]:
    /// - Omitted (không gửi key `custom_path`) -> giữ nguyên dòng trong entry_public_paths.
    /// - Clear ("custom_path": null hoặc "") -> xóa dòng trong entry_public_paths (và ghi entry_redirects nếu bài đang published).
    /// - Set("/duong-dan") -> kiểm tra validate_route_claim_in_tx và upsert vào entry_public_paths.
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_path: PatchField<String>,
    /// 3 trạng thái nhờ #[serde(default, deserialize_with = "deserialize_patch_field")]:
    /// - Omitted (không gửi key `seo`) -> giữ nguyên dòng trong entry_seo và seo_public_media_refs.
    /// - Clear ("seo": null) -> xóa dòng trong entry_seo và xóa seo_public_media_refs của entry_id.
    /// - Set(SeoMetadataInput) -> kiểm tra validate_seo_input + verify_og_media_exists_in_tx, upsert entry_seo và sync_entry_seo_media_ref_in_tx.
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub seo: PatchField<SeoMetadataInput>,
}
```

- **Ví dụ JSON Wire Payload cụ thể (`270-R5`):**
  1. **Giữ nguyên `custom_path`, `seo` và các trường không sửa (`Omitted = Preserve`):**
     ```json
     {
       "expected_version": 4,
       "expected_schema_token": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
       "title": "Tiêu đề mới",
       "slug": "tieu-de-moi",
       "values": {
         "body": "<p>Nội dung cập nhật</p>"
       }
     }
     ```
     *(Ở đây `featured_image`, các custom fields khác trong `values`, `term_ids`, `custom_path`, và `seo` đều vắng mặt → tất cả được giữ nguyên 100% như trong DB).*
  2. **Xóa `custom_path`, xóa một trường tùy chọn `client_name`, và xóa `seo` (`Clear`):**
     ```json
     {
       "expected_version": 5,
       "expected_schema_token": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
       "title": "Tiêu đề mới",
       "slug": "tieu-de-moi",
       "values": {
         "client_name": null
       },
       "custom_path": null,
       "seo": null
     }
     ```
     *(Ở đây `"client_name": null` xóa hẳn khóa `"client_name"` khỏi `content_entries.values_json`; `"custom_path": null` xóa dòng trong `entry_public_paths`; `"seo": null` xóa dòng trong `entry_seo` và `seo_public_media_refs`).*

### 4.4. Quyền Private của Plugin (`ScopedFieldGrant`), Tách `action.required_role` với `actor.user.role` & Thực thi trên mọi đường Host (`270-R3`)

1. **Delta Manifest & Kiểu dữ liệu `PluginPermissions` (`opendash_core::plugin` — `270-R3`):**
   - Giữ nguyên `#[serde(deny_unknown_fields)]` trên `PluginPermissions` và `PluginActionDeclaration`.
   - Bổ sung hai danh sách cấp quyền có phạm vi hẹp (`ScopedFieldGrant`), **chỉ hợp lệ khi `manifest_schema == 2` và có khai báo `min_host_version` thỏa mãn host hiện tại** (nếu khai báo trong `manifest_schema == 1`, `PluginManifest::validate()` từ chối ngay khi cài/bật với `422`):
     ```rust
     #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
     #[serde(deny_unknown_fields)]
     pub struct ScopedFieldGrant {
         /// Phải là valid_content_type_key cụ thể; CẤM dùng wildcard "*".
         pub content_type: String,
         /// Danh sách 1..=64 valid_snake_key cụ thể; CẤM dùng wildcard "*".
         pub field_keys: Vec<String>,
     }

     #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
     #[serde(deny_unknown_fields)]
     pub struct PluginPermissions {
         #[serde(default)]
         pub sqlite: Vec<String>,
         #[serde(default)]
         pub http: Vec<String>,
         #[serde(default)]
         pub network: Vec<NetworkScopeGrant>,
         #[serde(default)]
         pub content_read: Vec<String>,
         #[serde(default)]
         pub content_write: Vec<String>,
         /// Mặc định rỗng (default deny). Chỉ cho phép đọc trường Private của chủ sở hữu khác
         /// khi được liệt kê tường minh (content_type, field_key) tại đây.
         #[serde(default)]
         pub content_read_private: Vec<ScopedFieldGrant>,
         /// Mặc định rỗng (default deny). Chỉ cho phép ghi trường Private của chủ sở hữu khác
         /// khi được liệt kê tường minh (content_type, field_key) tại đây.
         #[serde(default)]
         pub content_write_private: Vec<ScopedFieldGrant>,
     }
     ```
   - **Quy tắc kiểm tra trong `PluginManifest::validate()`:**
     - Nếu `(!self.permissions.content_read_private.is_empty() || !self.permissions.content_write_private.is_empty()) && (self.manifest_schema != 2 || self.min_host_version.is_none())` → trả lỗi validate manifest.
     - Mọi `grant` trong `content_read_private` phải có `valid_content_type_key(&grant.content_type)` (không chấp nhận `"*"`), `self.can_read_content_type(&grant.content_type) == true`, và `grant.field_keys` gồm `1..=64` khóa hợp lệ theo `valid_snake_key` không trùng lặp (không chấp nhận `"*"`).
     - Mọi `grant` trong `content_write_private` phải có `valid_content_type_key(&grant.content_type)`, `self.can_write_content_type(&grant.content_type) == true`, và `grant.field_keys` gồm `1..=64` khóa hợp lệ theo `valid_snake_key` không trùng lặp.
     - Mở rộng allowlist `PluginActionDeclaration.capabilities` (`plugin.rs:253-256`) thành: `"kv_read" | "content_read" | "content_read_private" | "http" | "kv_write" | "content_write" | "content_write_private"`. Nếu action khai báo `"content_read_private"`, action đó phải có `"content_read"` và `manifest.permissions.content_read_private` không rỗng; nếu khai báo `"content_write_private"`, action đó phải có `"content_write"` và `manifest.permissions.content_write_private` không rỗng. Quy tắc V1 cấm trộn `kv_write` và `content_write` trong cùng một action được giữ nguyên 100%.

2. **Tách biệt Ngưỡng gọi Action (`action.required_role`) và Quyền Thực tế của Actor (`actor.user.role` — `270-R3`):**
   - **`action.required_role` là ngưỡng kiểm tra được phép gọi (Admission Threshold):**
     - Khi bắt đầu chạy action (`plugin/manager.rs:181`) VÀ bên trong giao dịch ghi `BEGIN IMMEDIATE` (`entries.rs::verify_plugin_action_fence`), hệ thống kiểm tra:
       1. `plugin.enabled == true` và `plugin.revision_generation == expected_generation`,
       2. Phiên của `actor.user.id` còn hiệu lực và vai trò hiện hành của người gọi trong DB thỏa mãn `actor.current_role >= action.required_role` (nếu `required_role == "editor"` thì cả `Editor` và `Admin` đều thỏa mãn; nếu `required_role == "admin"` thì chỉ `Admin` thỏa mãn).
   - **Quyền đọc/ghi Field tính theo `actor.current_role` thực tế (Không hạ quyền của Admin):**
     - Sau khi vượt qua ngưỡng admission, vai trò dùng để đối chiếu với `field.min_view_role` và `field.min_edit_role` là **`actor.current_role` thực tế của người gọi** (được xác minh lại trong DB lúc commit), **KHÔNG dùng `min(actor.role, action.required_role)`**.
     - Nhờ vậy, khi một `Admin` bấm chạy một action có `required_role: "editor"`, `Admin` giữ nguyên quyền `Admin` (tương thích 100% với hành vi M6 hiện có); ngược lại khi một `Editor` bấm chạy chính action đó, `actor.current_role` là `Editor` nên action không thể đọc trường `min_view_role == Admin` hay ghi trường `min_edit_role == Admin`.
     - Đồng thời, bản thân plugin luôn bị giới hạn bởi **trần capability của chính plugin đó** (`PluginPermissions` ∩ `action.capabilities`) theo hai hàm quyết định dưới đây.

3. **Hai hàm quyết định quyền Field của Plugin & Thực thi trên MỌI đường dữ liệu (`270-R3`):**
   - **Hàm kiểm tra quyền đọc trường (`plugin_can_read_field`):**
     Một plugin `manifest` (đang chạy ở chế độ hook khi `action_caps == None`, hoặc đang chạy một action với `action_caps == Some(&action.capabilities)`) chỉ được phép nhìn thấy giá trị của trường `field_entry: &EffectiveFieldEntry` trên `type_key` khi thỏa mãn **cả 3 điều kiện**:
     1. `manifest.can_read_content_type(type_key) == true` VÀ (nếu `action_caps` là `Some(caps)` thì `caps.contains("content_read")`).
     2. `actor.current_role >= field_entry.definition.min_view_role`.
     3. Một trong ba điều kiện sau đúng:
        - `field_entry.definition.public_exposure == PublicExposure::Public`, **HOẶC**
        - `field_entry.owner == ResourceOwner::Plugin(manifest.id.clone())` (plugin là chủ sở hữu của trường/nhóm trường đó), **HOẶC**
        - `manifest.has_private_read_grant(type_key, &field_entry.definition.key) == true` VÀ (nếu `action_caps` là `Some(caps)` thì `caps.contains("content_read_private")`).
   - **Hàm kiểm tra quyền ghi trường (`plugin_can_write_field`):**
     Khi plugin thực hiện `HostOperationRequest::ContentSave` trong một action có `action_caps`, plugin chỉ được phép gửi tường minh khóa `field_key` của `field_entry` trong `cmd.values` khi thỏa mãn **cả 3 điều kiện**:
     1. `manifest.can_write_content_type(type_key) == true` VÀ `action_caps.contains("content_write")`.
     2. `actor.current_role >= field_entry.definition.min_edit_role`.
     3. Một trong ba điều kiện sau đúng:
        - `field_entry.definition.public_exposure == PublicExposure::Public`, **HOẶC**
        - `field_entry.owner == ResourceOwner::Plugin(manifest.id.clone())`, **HOẶC**
        - `manifest.has_private_write_grant(type_key, &field_entry.definition.key) == true` VÀ `action_caps.contains("content_write_private")`.
   - **Thực thi trên toàn bộ 4 đường giao tiếp Host ↔ Plugin:**
     - **Đường 1 — `content/before_save` & `content/after_save` (`hooks.rs`):** Trước khi gọi từng plugin đăng ký hook, lọc `merged_values` qua `plugin_can_read_field(manifest, None, actor.current_role, type_key, field_entry)` để tạo `filtered_candidate_values` riêng cho plugin đó. Giữ nguyên ABI trả về `Result<(), ContentBeforeSaveVeto>` (`Allow` | `Veto`).
     - **Đường 2 — Input/Context truyền vào Action & Mọi Host Content-Read:** Tại baseline (`host_ops.rs:65-93`), `HostOperationRequest` chỉ có `kv_get`, `kv_set`, `content_save`, `http_request`. Khi host truyền dữ liệu entry vào `input` của một `PanelKind::Action` hoặc nếu bổ sung bất kỳ host op đọc nội dung nào, toàn bộ bản đồ `values` của entry **bắt buộc** được lọc qua `plugin_can_read_field(manifest, Some(&action.capabilities), actor.current_role, type_key, field_entry)` trước khi truyền vào Wasm guest.
     - **Đường 3 — Staged Content Write (`HostOperationRequest::ContentSave` trong `plugin/manager.rs`):** Kiểm tra mọi khóa gửi tường minh trong `cmd.values` bằng `plugin_can_write_field` ở cả bước preflight (trước khi gọi `dispatch_content_before_save`) VÀ bên trong giao dịch `BEGIN IMMEDIATE` (sau khi `verify_plugin_action_fence` xác nhận lại `actor.current_role` và `revision_generation`). Nếu bất kỳ khóa nào trong `cmd.values` không thỏa `plugin_can_write_field`, trả ngay `403 Forbidden` (`code: "field_edit_forbidden"`) mà **không đọc hay so sánh với giá trị trong DB**. Khi `cmd.entry_id` là `Some(id)` (update), các trường không xuất hiện trong `cmd.values` (`Omitted`) giữ nguyên giá trị đang lưu trong `content_entries.values_json`.
     - **Đường 4 — Action Output & Logs:** Không ghi `values` hay `candidate_values` vào bảng log lỗi (`record_hook_failure`, `record_plugin_execution_result`).

### 4.5. Quy trình thực thi 4 Pha của `CompositeEntrySaveInput` (`R2`, `R3`, `R4`, `270-R4`, `270-R5`)

1. **Pha 1 — Read, Preflight, Kiểm tra Precondition `428` & Hợp nhất `Omitted = Preserve` (ngoài giao dịch ghi):**
   - Xác thực phiên đăng nhập, CSRF/Origin, tải `EffectiveContentSchema` hiện tại và ghi nhận `preflight_snapshot = (current_schema_token, actor.user.id, actor.user.role)`.
   - **Kiểm tra `expected_schema_token` (`270-R5`):**
     - Lấy `client_token = input.expected_schema_token.as_deref().map(str::trim).filter(|s| !s.is_empty())`.
     - Nếu `client_token` là `None` → trả **`428 Precondition Required` (`code: "missing_schema_token"`)**.
     - Nếu `client_token != Some(current_schema_token.as_str())` → trả **`409 Conflict` (`code: "schema_version_conflict"`, kèm `current_schema_token` trong `details`)**.
   - **Kiểm tra `expected_version`:**
     - Với `POST .../entries/composite` (create): nếu `input.expected_version.is_some()` → trả `400 Bad Request` (`code: "unexpected_version_on_create"`).
     - Với `PUT .../entries/{id}/composite` (update): nếu `input.expected_version.is_none()` → trả **`428 Precondition Required` (`code: "missing_expected_version"`)**; đọc `existing_entry` từ `content_entries`, nếu `existing_entry.version != expected_version` → trả **`412 Precondition Failed`**.
   - **Kiểm tra quyền Field tuyệt đối không có Value-Comparison Oracle (`R2`):**
     - Duyệt từng khóa `(field_key, raw_val)` xuất hiện tường minh trong `input.values`:
       - Nếu `field_key` không tồn tại trong `effective_schema.effective_fields` → trả `422 Unprocessable Entity` (`code: "unknown_field_key"`).
       - Nếu `actor.user.role < field_entry.definition.min_edit_role` (bao gồm cả trường hợp `actor.user.role < field_entry.definition.min_view_role`): **từ chối ngay lập tức với `403 Forbidden` (`code: "field_edit_forbidden"`) mà KHÔNG đọc hay so sánh với giá trị trong `existing_entry.values`**.
   - **Hợp nhất `merged_values` với `Omitted = Preserve` tuyệt đối khi Update (`270-R5`):**
     - Khi `create` (`POST`): khởi tạo `merged_values = BTreeMap::new()`. Với mỗi `(field_key, field_entry)` trong `effective_schema.effective_fields`:
       - Nếu `input.values` không chứa `field_key` (`Omitted`): chỉ khi `field_entry.effective_status != EffectiveFieldStatus::Withdrawn` và `field_entry.definition.default_value` là `Some(def)`, mới chèn `(field_key, def.clone())` vào `merged_values`.
       - Nếu `input.values` chứa `(field_key, Value::Null)` (`Clear`): không chèn vào `merged_values`.
       - Nếu `input.values` chứa `(field_key, val)` (`Set(val)`): chèn `(field_key, val)` vào `merged_values`.
     - Khi `update` (`PUT`): khởi tạo `merged_values = existing_entry.values.clone()`. Với mỗi `(field_key, raw_val)` trong `input.values`:
       - Nếu `raw_val.is_null()` (`Clear`): gọi `merged_values.remove(&field_key)` (**xóa hẳn khóa khỏi `BTreeMap`, không lưu literal `null`**).
       - Nếu `!raw_val.is_null()` (`Set(raw_val)`): gọi `merged_values.insert(field_key, raw_val)`.
       - **Mọi khóa không xuất hiện trong `input.values` (`Omitted`) được giữ nguyên 100% như trong `existing_entry.values` — TUYỆT ĐỐI KHÔNG tự động chèn `default_value` khi `update` bất kể vai trò của `actor`** (`270-R5`).
   - Kiểm tra toàn bộ `merged_values` với `effective_schema`: mọi trường có `effective_required == true` (tức `required == true` và `effective_status != EffectiveFieldStatus::Withdrawn`) phải tồn tại trong `merged_values` và vượt qua `valid_field_value`; kiểm tra media bằng `require_existing_references` trên `effective_schema`, kiểm tra cú pháp `term_ids`, `custom_path`, và `seo`.

2. **Pha 2 — Chạy Wasm Hook `content/before_save` ngoài giao dịch SQLite (`R2`, `270-R3`):**
   - Gọi `hooks.dispatch_content_before_save` với `filtered_candidate_values` được lọc riêng cho từng plugin theo `plugin_can_read_field` (Mục `4.4`). Không giữ khóa SQLite trong suốt Pha 2.

3. **Pha 3 — Giao dịch ghi nguyên tử `BEGIN IMMEDIATE` & Đối chiếu lại Snapshot Pha 1 (`R3`, `270-R4`, `270-R5`):**
   - Mở một giao dịch SQLite `BEGIN IMMEDIATE` duy nhất (`begin_entry_write`).
   - **Kiểm tra lại Snapshot Pha 1 (áp dụng cho cả Composite và các endpoint ghi nội dung hiện có):**
     1. Kiểm tra lại phiên đăng nhập đang hoạt động của `actor.user.id` và xác nhận `current_actor_role == preflight_snapshot.actor_role` (nếu phiên hết hạn trả `401`; nếu bị hạ quyền trả `403`).
     2. Tính lại `current_schema_token` trong giao dịch: nếu `current_schema_token != preflight_snapshot.schema_token` → abort `409 Conflict` (`code: "schema_version_conflict"`).
     3. Nếu là `update`, kiểm tra lại `content_entries.version == expected_version` (`412 Precondition Failed` nếu lệch).
     4. Kiểm tra lại trong giao dịch: tính duy nhất của `(content_type_key, slug)` và `check_default_slug_claim_in_tx`, sự tồn tại và trạng thái `lifecycle_state = 'active'` của toàn bộ ảnh trong `merged_values` (`require_existing_references_in_tx`), tính hợp lệ và eligibility của `term_ids` (`validate_term_ids`).
   - **Thực thi ghi nguyên tử:**
     1. Insert hoặc Update dòng trong `content_entries` (`title`, `slug`, `values_json = serde_json::to_string(&merged_values)`, `publication_status`, và tăng `version = version + 1` **đúng 1 lần** khi update).
     2. Nếu `input.term_ids` là `Some(ids)`: cập nhật bảng `entry_terms` cho `entry_id` (nếu `None` khi update: giữ nguyên `entry_terms`).
     3. Xử lý `input.custom_path` (`PatchField<String>`):
        - `PatchField::Omitted`: giữ nguyên `entry_public_paths`.
        - `PatchField::Clear` (hoặc chuỗi rỗng `""`): nếu `entry_public_paths` đang có đường dẫn cũ và entry đang `published`, lưu redirect vào `entry_redirects` rồi xóa dòng trong `entry_public_paths` (khớp đúng logic của `storage::seo::put_entry_route`).
        - `PatchField::Set(raw_path)`: gọi `validate_route_claim_in_tx(&mut tx, &raw_path, Some(entry_id))`, cập nhật `entry_public_paths` và `entry_redirects`.
     4. Xử lý `input.seo` (`PatchField<SeoMetadataInput>`):
        - `PatchField::Omitted`: giữ nguyên `entry_seo` và `seo_public_media_refs`.
        - `PatchField::Clear`: xóa dòng của `entry_id` khỏi `entry_seo` và `seo_public_media_refs`.
        - `PatchField::Set(seo_in)`: gọi `validate_seo_input(&seo_in)`, `verify_og_media_exists_in_tx(&mut tx, seo_in.og_image_id.as_deref())`, upsert vào `entry_seo` và gọi `sync_entry_seo_media_ref_in_tx(&mut tx, entry_id)`.
   - `COMMIT` giao dịch và trả về bản ghi tổng hợp (`entry` với `version` mới + `custom_path` + `seo`).

4. **Pha 4 — Chạy `content/after_save` sau khi Commit:**
   - Phát sự kiện `content/after_save` với `filtered_candidate_values` đã lọc theo `plugin_can_read_field` cho từng plugin.

---

## 5. Bảng định tuyến Hash URL chuẩn, Bảo vệ Deep-Link RBAC & Quarantine `EditorDraftStore` (`R5`)

### 5.1. Bảng định tuyến Hash URL chuẩn & Chuyển hướng tương thích ngược

| Hash Route chuẩn | Màn hình / Chức năng | Quyền tối thiểu | Hành vi khi không đủ quyền / Chuyển hướng legacy |
| --- | --- | --- | --- |
| `#/dashboard` | Tổng quan (Dashboard) | `Editor` | — |
| `#/posts` · `#/posts/new` · `#/posts/:id/edit` | Danh sách / Thêm mới / Chỉnh sửa Bài viết | `Editor` | — |
| `#/posts/categories` · `#/posts/tags` | Chuyên mục / Thẻ dùng chung | `Editor` | — |
| `#/pages` · `#/pages/new` · `#/pages/:id/edit` | Danh sách / Thêm mới / Chỉnh sửa Trang | `Editor` | — |
| `#/cpt/:typeKey` · `#/cpt/:typeKey/new` · `#/cpt/:typeKey/:id/edit` | Danh sách / Thêm mới / Chỉnh sửa Entry của CPT `:typeKey` | `Editor` | Nếu `:typeKey` không tồn tại → màn hình `404` nội bộ. Vẫn mở và sửa bình thường khi provider của `:typeKey` đang `provider_inactive` hoặc `withdrawn` (`R1`). |
| `#/media` · `#/media/upload` | Thư viện Media / Tải lên | `Editor` | — |
| `#/appearance/themes` · `#/appearance/customize` · `#/appearance/menus` · `#/appearance/templates` | Quản lý Giao diện / Menus / Templates toàn site | `Admin` | `Editor` truy cập URL trực tiếp → hiển thị `403 Không có quyền truy cập` (không gọi API ghi). |
| `#/builder?template=:role&type=:typeKey&entry=:entryId` | Visual Builder | Theo phạm vi template | Nếu `:role` là template dùng chung toàn site (`header`, `footer`, `archive`, `taxonomy_archive`, hoặc `single`/`page` không gắn `entryId`) → **chỉ `Admin`** (`Editor` bị chặn `403` cả trên UI và server API). `Editor` chỉ được mở/lưu Builder override khi có `entryId` cụ thể mà `Editor` có quyền sửa. |
| `#/contact/forms` · `#/contact/submissions` · `#/contact/smtp` | Form liên hệ / Tin nhắn / Cấu hình SMTP | `Editor` xem tin nhắn; `Admin` cấu hình form & SMTP | — |
| `#/plugins` · `#/plugins/install` | Plugins đã cài / Cài từ ZIP | `Admin` | `Editor` → `403`. |
| `#/users` · `#/users/new` | Tất cả người dùng / Thêm người dùng | `Admin` | `Editor` → `403`. |
| `#/users/profile` | Hồ sơ của tôi (`GET/PUT /api/admin/profile`) | `Editor` + `Admin` | Cho phép sửa `display_name` và đổi mật khẩu của chính mình (đổi mật khẩu **bắt buộc gửi kèm `current_password`**, sai trả `403 invalid_current_password`; không được tự đổi `role`). |
| `#/tools/backup` · `#/tools/restore` · `#/tools/media-reconcile` · `#/tools/health` | Sao lưu / Phục hồi / Đối soát media / Tình trạng hệ thống | `Admin` | `Editor` → `403`. |
| `#/settings/general` · `#/settings/permalinks` · `#/settings/seo` | Cài đặt Tổng quát / Đường dẫn / SEO website | `Admin` | `Editor` → `403`. |
| **`#/settings/content-types`** · **`#/settings/content-types/:typeKey`** | **Cài đặt → Loại nội dung & Trường** (Tạo/sửa CPT, quản lý Field Groups và sắp xếp `EditorLayoutConfigV1` / `base_field_panels`) | **`Admin`-only** | **Tuyệt đối không chứa bảng danh sách hay form nhập entry (`F1`).** Luôn hiển thị trên menu `Cài đặt` kể cả khi có `0` CPT. Route cũ `#/content` tự động redirect: `Admin` → `#/settings/content-types`, `Editor` → `#/posts`. |

### 5.2. Quản lý bản nháp `EditorDraftStore`, Quarantine khi mất phiên và Kiểm quyền lại trước khi khôi phục (`R5`)

1. **Khóa định danh & Provenance của bản nháp trong RAM (`EditorDraftStore`):**
   - Mỗi bản nháp chỉnh sửa đang mở được lưu trong RAM (không ghi ra `localStorage`/`sessionStorage`) với metadata:
     `(site_instance_id, user_id, auth_generation, draft_key, status: 'active' | 'quarantined', schema_token_at_edit, dirty_fields, payload)`.
   - Với màn hình tạo mới (`id === null`), `draft_key` sinh theo `${type_key}:new:${draft_uuid}` riêng cho từng tab tạo mới.
2. **Chặn phản hồi bất đồng bộ trễ bằng `session_epoch` (`R5`):**
   - Biến đếm đơn điệu `session_epoch` tăng lên mỗi khi nhận mã `401 Unauthorized`, bấm Đăng xuất, đổi tài khoản, hoặc phát hiện `site_instance_id` thay đổi. Mọi response bất đồng bộ có `request_epoch !== current_session_epoch` đều bị hủy bỏ hoàn toàn trước khi chạm vào store hay UI state.
3. **Quarantine ngay khi gặp `401 Unauthorized` & Xóa khi Logout/Đổi tài khoản (`R5`):**
   - Khi gặp `401`, toàn bộ nháp trong `EditorDraftStore` chuyển sang `status = 'quarantined'` và cây DOM của form chỉnh sửa bị unmount/ẩn đằng sau màn hình đăng nhập lại.
   - Khi Logout hoặc đăng nhập lại với `user_id` khác hoặc `site_instance_id` khác (ví dụ sau khi restore site), gọi `EditorDraftStore.clear()` xóa sạch 100% nháp trong RAM.
4. **Kiểm quyền lại với `effective-editor-config` trước khi khôi phục nháp (`scrubQuarantinedDraft` — `R5`):**
   - Khi đăng nhập lại cùng `(site_instance_id, user_id)`, trước khi hiển thị bản nháp `quarantined`, client bắt buộc gọi mới `GET /api/admin/content-types/{type_key}/effective-editor-config`:
     - Mọi `field_key` không còn trong tập trường được phép xem (`caller.role < min_view_role`, ví dụ tài khoản vừa bị hạ quyền `Admin → Editor`) bị **xóa vĩnh viễn khỏi `draft.payload.values` và `draft.dirty_fields` trước khi render**.
     - Mọi `field_key` chuyển sang chỉ đọc (`editable == false`, tức `caller.role < min_edit_role`) bị hủy bỏ phần sửa nháp và đặt lại theo giá trị hiện hành trên server.

---

## 6. Ma trận Nghiệm thu Tổng thể & Phân bổ theo Slice (`AC-01`–`AC-11`)

Bảng dưới đây là **bộ tiêu chí nghiệm thu phủ toàn bộ phạm vi tính năng (Feature-Wide Coverage Matrix)**. Khi chia thành các slice G3 để triển khai, **mỗi slice chỉ chạy các case thuộc phạm vi của slice đó**:

| Mã AC | Slice mục tiêu | Nội dung kiểm chứng & Tiêu chí đạt (Pass Criteria) | Liên kết Review |
| --- | --- | --- | --- |
| **AC-01** | `S-SCHEMA` & `S-SAVE` | **Bảo toàn quản lý dữ liệu & media khi Provider Inactive / Group Withdrawn / Remove-Target / Remove-Field:** (a) CPT và Page có trường `Image` do Theme/Plugin khai báo → tắt plugin / đổi theme → `Editor`/`Admin` vẫn xem và sửa được giá trị trường `Image` cùng nội dung entry qua Core Standard Field Fallback Form; bật lại provider giữ nguyên dữ liệu và vị trí panel. (b) Nhóm field của User và của Package gắn `["page", "portfolio"]`, Page có lưu giá trị ảnh → `PUT` nhóm field của User đổi target thành `["portfolio"]` (hoặc thử xóa field ảnh khi đã có entry → bị chặn `409 cannot_remove_field_with_data`, phải dùng `declaration_state = 'withdrawn'`) và nâng cấp Package rút `"page"` / rút field ảnh → association/field trên Page chuyển sang `withdrawn`, chạy đối soát/purge media (`collect_pins_in_tx`) xác nhận ảnh trên Page **không bị coi là orphan hay bị xóa**, và mở lại Page vẫn xem/sửa được trường đó. | `R1`, `R4`, `270-R1` |
| **AC-02** | `S-SCHEMA` | **Kiểm tra tương thích & chống đụng độ `field_key` đối xứng trên 3 hướng:** Thử tạo/sửa base CPT (`PUT /api/admin/content-types/{key}` kèm `If-Match: "v<ver>"`), tạo/sửa field group, và kích hoạt gói có `field_key` trùng với nhóm đang `active`/`provider_inactive`/`withdrawn` (hoặc target `withdrawn`) → cả 3 hướng đều bị chặn `409 field_key_collision` trong `BEGIN IMMEDIATE`. Thử đổi field optional sang `required: true` (kể cả có `default_value`) khi đã có entry → bị chặn `409 incompatible_schema_change`. Kiểm tra `preflight_builtin_key_collisions` vẫn PASS 100% với `post`/`page`. | `R4`, `270-R1`, `270-R4` |
| **AC-03** | `S-SCHEMA` & `S-LAYOUT` | **Kiểm tra bất biến `Rule COMPAT-4` (`N3`) & `default_value` ở MỌI Transition (kể cả khi đã có entry — `270-R8`):** (a) Khai báo `min_edit_role = Editor, min_view_role = Admin` → bị chặn `422 invalid_field_acl`; (b) Khai báo `default_value` cho trường `Image`/`Gallery` hoặc giá trị sai kiểu → bị chặn `422 invalid_field_default`; (c) Ẩn panel (`enabled: false` trên `base_field_panels`, `core:base_fields` hoặc `ContentFields`) hoặc nâng quyền panel lên `Admin` khi chứa trường `effective_required == true, min_edit_role = Editor` → bị chặn `422 required_field_hidden_or_inaccessible`; (d) Trường `required: true, min_edit_role = Admin` khi tạo mới thiếu `default_value` → bị chặn `422 admin_required_field_missing_default`; **(e) CPT đã có entry (`has_existing_entries == true`), một trường đang `required: true, min_edit_role = Editor, default_value = None` → Admin gọi `PUT` (hoặc package re-declare) đổi `min_edit_role` sang `Admin` mà không kèm `default_value` hợp lệ → bị chặn `422 admin_required_field_missing_default`; khi cung cấp kèm `default_value` hợp lệ thì cập nhật thành công và `Editor` vẫn tạo được entry mới; (f) CPT đã có entry và trường đang `required: true, min_edit_role = Admin, default_value = Some("x")` → thử xóa `default_value` (`None`) → bị chặn `422 admin_required_field_missing_default`.** | `R2`, `R4`, `270-R6`, `270-R8` |
| **AC-04** | `S-LAYOUT` | **Chia nhóm Base Fields (`base_field_panels`), Auto-append Panel mới khi nâng cấp gói, Guard `allowed_regions`/Kind & Thứ tự Mobile (`270-R6`, `270-R9`):** (a) Dùng CPT có sẵn base fields trước migration trong `content_types.definition_json.fields` → chia thành ít nhất 2 nhóm (`layout:project_info` ở `main_bottom`, `layout:project_status` ở `sidebar`) → reload và Lưu entry giữ nguyên khóa `definition_json`, `values_json` và template binding; base field chưa gán tự động vào `core:base_fields`; **(b) Khi `content_editor_layouts` đã có cấu hình lưu cho CPT, nâng cấp Theme/Plugin (hoặc tạo User Field Group mới) bổ sung thêm 1 `field_group` / `editor_panel` mới → panel mới tự động được nối (auto-append) vào `default_region` mà không làm mất vị trí đã tùy chỉnh của các panel cũ; (c) Manifest Theme/Plugin thử đăng ký `CoreBuiltIn`, `BaseFieldsSlice`, tiền tố `core:`/`layout:`, Theme thử đăng ký `Action`, hoặc đặt panel vào `HeaderBar` / vùng ngoài `allowed_regions` → bị từ chối `422`;** (d) Trên mobile (`< 1024px`) thứ tự DOM đúng `header_bar → main_top → main_primary → main_bottom → sidebar`. | `270-R6`, `270-R9` |
| **AC-05** | `S-SAVE` & `S-SCHEMA` | **Chuẩn hóa `public_exposure` theo Ngữ cảnh (`270-R7`), Round-trip Restart & Bảo toàn 7 nhánh Media Public (`270-R2`):** **(a) Kiểm증 3 trường hợp chuẩn hóa `FieldDefinitionInput` (cả qua User API và Manifest) kèm khởi động lại server (restart round-trip) và kiểm tra ảnh `Image`: (1) Base CPT field không gửi `public_exposure` → lưu DB là `public`, `GET /media/...` trả `200 OK` cả trước và sau restart; (2) Extension field group không gửi `public_exposure` → lưu DB là `private`, trang public không lộ trường và `GET /media/...` trả `404` cả trước và sau restart (nhưng `collect_pins_in_tx` vẫn giữ `total_pins > 0`); (3) Extension field group khai báo tường minh `"public_exposure": "public"` → lưu DB là `public`, trang public hiển thị và `GET /media/...` trả `200 OK` cả trước và sau restart;** (b) Ảnh dùng đồng thời ở cả trường `Private` và nguồn `Public` → `GET /media/...` trả `200 OK`; (c) Ảnh dùng trong `theme_option_media_refs` (`public = 1`), `seo_public_media_refs` (`home_seo` và `entry_seo` published), `builder_homepage_media`, và `template_override_media_refs` (shared override) đều tiếp tục trả `200 OK`. | `R2`, `270-R2`, `270-R7` |
| **AC-06** | `S-SAVE` | **Quyền Private của Plugin (`ScopedFieldGrant`), Tách `required_role` / `actor.role` & Chống Oracle:** (a) Plugin `manifest_schema: 1` khai báo `content_read_private` hoặc dùng wildcard `"*"` trong `ScopedFieldGrant` bị từ chối khi cài/bật; (b) Plugin chỉ có `content_read` cơ bản không nhận được trường `Private` của chủ sở hữu khác trong `content/before_save`, `content/after_save` và action context, trong khi plugin có `ScopedFieldGrant` hợp lệ nhận được đúng trường được cấp khi `actor.role >= min_view_role`; (c) Action có `required_role = "editor"` khi được `Admin` gọi vẫn giữ quyền `Admin` trên các trường `min_edit_role = Admin`, còn khi được `Editor` gọi thì bị chặn `403 field_edit_forbidden`; (d) Khi `Editor` gửi tường minh một `field_key` có `min_edit_role = Admin` trong `values` (dù giá trị trùng hay khác DB, hoặc `null`), server luôn trả `403 field_edit_forbidden` trước khi so sánh DB. | `R2`, `270-R3` |
| **AC-07** | `S-SAVE` | **Giao dịch Lưu nguyên tử `CompositeEntrySaveInput`, `PatchField<T>` 3 trạng thái & `Omitted = Preserve`:** (a) Một lần bấm Lưu cập nhật `content_entries` (`values_json` chứa `body`/`featured_image`) + `entry_terms` + `entry_public_paths` + `entry_seo` trong một giao dịch `BEGIN IMMEDIATE` và chỉ tăng `version` thêm `+1`; gửi top-level `body` hoặc `cover_image` bị từ chối `400`; (b) Gửi `"custom_path": null` xóa dòng trong `entry_public_paths`; không gửi `custom_path` (`Omitted`) giữ nguyên `entry_public_paths`; (c) `Admin` xóa (`"field": null`) một trường tùy chọn có `default_value` → sau đó `Editor` cập nhật `title` và không gửi trường đó (`Omitted`) → trường đó vẫn vắng mặt trong `values_json` (không bị tự động chèn lại `default_value`). | `F4`, `R4`, `270-R4`, `270-R5` |
| **AC-08** | `S-SAVE` | **Bắt buộc `expected_schema_token` (`428`) & Hàng rào Snapshot ở Pha 3:** (a) Gửi composite save không có trường `expected_schema_token` hoặc gửi chuỗi rỗng → trả đúng `428 Precondition Required` (`code: "missing_schema_token"`); gửi token cũ → `409 schema_version_conflict`; (b) Giả lập Admin đổi schema hoặc hạ quyền `Editor` đúng vào lúc Pha 2 (`content/before_save`) đang chạy → Pha 3 (`BEGIN IMMEDIATE`) phát hiện lệch snapshot và chặn ghi (`409` / `403`) trên cả composite lẫn legacy entry endpoint. | `R3`, `270-R5` |
| **AC-09** | `S-NAV-UI` | **Menu `Cài đặt → Loại nội dung & Trường`, Deep-Link RBAC & Hồ sơ cá nhân:** (a) Khi có `0` CPT, `Admin` vẫn thấy `Cài đặt → Loại nội dung & Trường` (`#/settings/content-types`) và màn hình này không có bảng/form nhập entry; mỗi CPT mới tạo có menu nhập liệu riêng (`#/cpt/:typeKey`); (b) `Editor` gọi `GET /api/admin/content-types/{type_key}/effective-editor-config` thành công để dựng Single Edit nhưng bị chặn `403` ở các API/route quản trị raw schema/layout và Builder template toàn site; (c) Đổi mật khẩu tại `#/users/profile` bắt buộc `current_password`. | `F1`, `F7`, `R3` |
| **AC-10** | `S-NAV-UI` & `S-SAVE` | **Tách bạch `PanelKind::Action` khỏi Lưu nội dung & Mở Builder an toàn:** Bấm Lưu nội dung không kích hoạt `plugin_action`; bấm nút trong Action Panel không tự lưu dữ liệu form chưa lưu. Trên màn hình Thêm mới CPT (`id === null`), nút mở Builder yêu cầu lưu/xuất bản trước chứ không tự động xuất bản ngầm. | `F6`, `F7`, `270-R9` |
| **AC-11** | `S-RECOVERY` | **Quarantine `EditorDraftStore` khi `401` & Negative Case hạ quyền `Admin → Editor` cùng User:** User đang đăng nhập là `Admin`, sửa nháp chứa cả trường `Editor` và trường `Admin`-only → phiên hết hạn (`401`) khiến nháp chuyển sang `quarantined` và ẩn khỏi DOM → trong lúc đó tài khoản này bị hạ quyền xuống `Editor` (hoặc trường đó bị đổi sang `Admin`-only) → cùng user đăng nhập lại → hệ thống tải `effective-editor-config` mới, **xóa bỏ trường `Admin`-only khỏi bản nháp** trước khi khôi phục các trường `Editor` hợp lệ. | `R5` |
