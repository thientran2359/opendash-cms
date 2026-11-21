# Generic CPT Content Builder, Pattern CPT (`core/pattern_ref`) & Pattern Library

* **Author:** Nova (tổng hợp theo quyết định sản phẩm ngày 19–20/11/2026 của chủ dự án)
* **Status:** Approved
* **Date:** 2026-11-20
* **Baseline:** `main@7f29bcf` (xây tiếp sau [Design Spec 1: Theme Format v3 & Template Resolver](theme-format-and-template-resolver.md), migration `0042`)
* **Approved By:** Owner
* **Approval Evidence:** Owner chat 2026-11-20 (Pattern CPT `core/pattern_ref` depth=1, Admin-only write, `post` mặc định tắt Builder, Pattern Library Gutenberg) · PR #424 (`e47d5d3`)
* **Goldfish Review:** 5-agent independent swarm (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, `egm-comprehension`, `egm-zoom-out`) · 2026-11-20 · reviewed `main@7f29bcf` (all Blocker/Major resolved)

---

## Overview

Đặc tả này hoàn thiện nửa thứ hai của kiến trúc Visual Builder trên OpenDash CMS:
1. **Generic CPT Content Builder:** Mở rộng Visual Builder cho **mọi loại nội dung (built-in `page`, `post` và mọi Custom Post Type)** khi đăng ký `ContentSupport::Builder`. Giữ độc lập hoàn toàn nguồn dữ liệu Editor thường (`content_entries.values_json["body"]`) và nguồn Builder AST (`entry_builder_documents`), không bao giờ tự chuyển đổi text ↔ AST.
2. **Pattern CPT & Reusable Pattern Block (`core/pattern_ref`):** Chuẩn hóa private CPT `section` cũ sang **Pattern CPT (`pattern`)** dùng chung bảng `entry_builder_documents` và `entry_builder_media_refs`. Khi chỉnh sửa nhiều trang (Home, Contact, About…), người dùng chèn **1 block `core/pattern_ref`** (`KnownElement::PatternRef(PatternRefProps)`) và chọn `pattern_id` từ danh sách Pattern trong Admin CPT — sửa Pattern 1 lần trong Admin CPT sẽ tự cập nhật đồng bộ ở mọi trang đang gọi. **Giới hạn độ sâu = 1** (Pattern không được lồng `core/pattern_ref` khác) để triệt tiêu hoàn toàn vòng lặp và đệ quy.
3. **Pattern Library (`Explore all patterns` kiểu Gutenberg):** Mở ngăn kéo/modal lưới xem trước ngay trong Visual Builder, liệt kê tất cả mẫu từ thư mục `patterns/*.json` của active theme v3 (DTO giữ trường `source: "theme"` để sẵn sàng mở rộng nguồn Cloud sau này). Bấm chọn mẫu sẽ **COPY / import AST** (sinh mới toàn bộ `node_id`) vào trang đang chỉnh sửa để tùy biến tự do.

> **Phạm vi thay thế hợp đồng cũ:** Sau khi triển khai và kiểm chứng hoàn tất, đặc tả này mở rộng Page-scoped Builder và thay thế tên gọi/mô hình private Section trong [`ADMIN_REVIEW_2026_11.md`](../contracts/ADMIN_REVIEW_2026_11.md), mở rộng `ContentSupport` trong [`CONTENT_FOUNDATION_2026_11.md`](../contracts/CONTENT_FOUNDATION_2026_11.md), và cập nhật ghi chú synced Pattern block (`core/pattern_ref`) trong [`PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md).

---

## The Problem

1. **Builder nội dung hiện đang gắn cứng cho `page`:** [`CONTENT_FOUNDATION_2026_11.md`](../contracts/CONTENT_FOUNDATION_2026_11.md) (`opendash_core::content::ContentSupport`) chỉ có `[Editor, FeaturedImage]`, chưa có `Builder` để CPT tùy chỉnh (như `project`, `landing_page`) hoặc `post` bật hỗ trợ Builder content độc lập với template chung.
2. **Mô hình `Section` cũ chưa hỗ trợ gọi đồng bộ qua 1 block chọn Pattern:** Người dùng muốn tạo một Pattern CTA trong Admin CPT rồi nhúng vào Home, Contact, About bằng 1 block chọn Pattern (sửa 1 lần cập nhật mọi nơi), tách biệt rõ ràng với kho mẫu sẵn để import/copy nhanh từ theme hoặc cloud.
3. **Trùng lặp hạ tầng lưu trữ & thiếu giao diện Pattern Library:** Lưu `section_documents` tách biệt khỏi entry Builder sẽ nhân đôi logic lưu trữ, `media_refs` và backup/restore; đồng thời `patterns/*.json` của Theme v3 (Spec 1) cần giao diện duyệt theo danh mục kiểu Gutenberg (`Explore all patterns`).

---

## Goals

1. **Đăng ký `ContentSupport::Builder` tường minh cho mọi CPT:** Built-in `page` mặc định bật `[Editor, FeaturedImage, Builder]`; built-in `post` mặc định **tắt** (`[Editor, FeaturedImage]`, Admin có thể bật khi cần); CPT tùy chỉnh khai báo rõ trong `ContentTypeSettings.supports`. UI và backend cùng kiểm tra khả năng này.
2. **Thống nhất 1 cặp bảng lưu trữ AST (`entry_builder_documents` + `entry_builder_media_refs`):** Dùng chung cho cả bài viết CPT lẫn `pattern` CPT, khóa theo `content_entries.id` và dùng chung CAS `content_entries.version`. Tách biệt tuyệt đối với `content_entries.values_json["body"]`.
3. **Pattern CPT (`pattern`) + Block `core/pattern_ref` (depth = 1, Admin-only write):** Đổi tên private CPT `section` → `pattern` (cập nhật đủ mọi guard bảo mật); bổ sung `KnownElement::PatternRef(PatternRefProps)` cho phép nhúng đồng bộ vào mọi trang, giới hạn **depth = 1** (cấm lồng `core/pattern_ref` bên trong Pattern), bảo vệ xóa Pattern đang dùng (`409 Conflict` trừ khi `?force=true`) và cập nhật public media ACL qua bảng chỉ mục `entry_pattern_refs`.
4. **Pattern Library (`Explore all patterns`):** Cung cấp API `GET /api/admin/patterns/library` và `GET /api/admin/patterns/library/theme/{key}` đọc từ `patterns/*.json` của active theme v3 (ảnh dùng `/theme/<theme_id>/assets/...` hiện hành), bấm chèn sẽ COPY AST với `node_id` mới vào canvas hiện tại.

---

## Non-Goals

- Không có cờ `synced`/`unsynced` trên Pattern CPT: **Pattern CPT (`core/pattern_ref`)** luôn là tham chiếu đồng bộ qua block (depth = 1); **Pattern Library (`patterns/` & Cloud)** luôn là thao tác COPY / import AST độc lập.
- Không cho phép lồng Pattern trong Pattern (depth > 1).
- Không triển khai máy chủ Cloud Pattern thực tế hay trait trừu tượng thừa (`PatternCatalogProvider`) ở đợt này — chỉ giữ trường `"source": "theme"` trong JSON DTO để tương thích tiến cho UI.
- Không triển khai Custom Builder Block authoring/SDK (đã chốt thảo luận riêng sau) và không thay đổi Template Resolver / Theme v3 / Reading Settings (đã thuộc [Design Spec 1](theme-format-and-template-resolver.md)).

---

## Critical User Journeys (CUJs)

### CUJ 1: Bật Builder cho một CPT tùy chỉnh và sửa 1 bản ghi độc lập
* **Given:** Admin tạo CPT `project` với `supports: ["editor", "featured_image", "builder"]` và có 2 bản ghi `Project A`, `Project B`.
* **When:** Editor mở `Project A`, chuyển `content_mode = "builder"`, dựng layout trong Visual Builder, bấm Preview và bấm Save (bản nháp).
* **Then:** API kiểm tra quyền Editor + `ContentSupport::Builder` hiệu lực của `project`; chỉ `Project A` được lưu AST với CAS `version` chung. `Project B`, template `single-project.html`, dữ liệu `values_json["body"]` của `Project A` và trạng thái public không bị thay đổi. Nếu CPT không bật `Builder` (ví dụ `post` mặc định), UI ẩn nút mở Builder và backend trả `403 Forbidden`.

### CUJ 2: Tắt hỗ trợ Builder hoặc đổi chế độ Editor ↔ Builder không mất dữ liệu
* **Given:** `Project A` đã có cả văn bản trong Editor (`values_json["body"]`) lẫn AST trong Builder (`entry_builder_documents`).
* **When:** Editor chuyển `content_mode` từ `builder` về `editor` (hoặc Admin tắt `Builder` khỏi `project.supports`).
* **Then:** Cả `values_json["body"]` lẫn `entry_builder_documents.document_json` đều được giữ nguyên trong SQLite. Khi `Builder` bị tắt trên CPT, nút mở Builder bị khóa có giải thích và public render fallback an toàn sang nội dung `editor` (đồng thời thu hồi public media ACL của các media chỉ nằm trong Builder AST).

### CUJ 3: Tạo Pattern CTA trong Pattern CPT và nhúng vào Home, Contact, About
* **Given:** Admin tạo một Pattern tên `"CTA Liên hệ"` trong menu **Patterns** (private CPT `pattern`, không có URL public; Save cập nhật ngay bản Pattern đang dùng).
* **When:** Người dùng mở Builder của 3 trang `Home`, `Contact`, `About`, chèn block **`Reusable Pattern` (`core/pattern_ref`)** và chọn `"CTA Liên hệ"` từ dropdown, rồi lưu cả 3 trang. Sau đó Admin sửa tiêu đề/nút của `"CTA Liên hệ"` trong Pattern CPT.
* **Then:** Cả 3 trang `Home`, `Contact`, `About` tự động hiển thị nội dung CTA mới nhất mà không cần sửa lại từng trang. Nếu Admin cố chèn `core/pattern_ref` vào bên trong một Pattern khác, backend từ chối `400 Bad Request` (depth = 1). Nếu Admin bấm xóa Pattern đang gắn ở 3 trang, API trả `409 Conflict` kèm `{ referencing_count: 3, referencing_entries: [...] }` và chỉ xóa khi xác nhận `?force=true`; sau khi xóa, 3 trang vẫn render bình thường (bỏ qua khối thiếu ở public, hiện khung cảnh báo ở Admin Builder).

### CUJ 4: Mở Pattern Library (`Explore all patterns`) để import/copy mẫu từ Theme
* **Given:** Active theme v3 khai báo 6 mẫu trong `patterns/*.json` thuộc các danh mục `call-to-action`, `contact`, `featured`, `headers`, `pages`.
* **When:** Người dùng đang edit một Page trong Builder, mở tab **Patterns** hoặc bấm **"Explore all patterns"**, lọc theo danh mục `call-to-action` và bấm chọn mẫu `"Hero CTA"`.
* **Then:** Builder gọi `GET /api/admin/patterns/library/theme/{key}`, nhận `BuilderDocument`, sinh mới toàn bộ `node_id` và chèn COPY vào vị trí đang chọn. Mọi chỉnh sửa sau đó chỉ thuộc về Page đang edit, không làm thay đổi file theme bất biến.

---

## Technical Plan (Architecture)

### A. Đăng ký `ContentSupport::Builder`, `content_mode` & Unified Builder Storage

1. **`opendash_core::content::ContentSupport`:**
   - Bổ sung variant `Builder` (`"builder"`).
   - `ContentTypeSettings::default_for_type_key(type_key)`:
     - `"page"` → `public: true, has_archive: false, publication: DraftPublish, supports: [Editor, FeaturedImage, Builder]`.
     - `"post"` → `public: true, has_archive: true, publication: DraftPublish, supports: [Editor, FeaturedImage]` (**mặc định tắt `Builder`**).
     - `"pattern"` (ở Slice S02, thay `"section"`) → `private_pattern()` (`public: false, has_archive: false, publication: Immediate, supports: [Builder]`). *(Dùng `Immediate` phản ánh đúng hành vi Save cập nhật đồng bộ ngay lập tức cho các trang đang gọi, trong khi `public: false` và `has_archive: false` đảm bảo không bao giờ lộ ra route public.)*
     - Custom CPT → mặc định `[Editor, FeaturedImage]`, cho phép khai báo thêm `Builder`.
2. **API Entry Builder thống nhất (đăng ký trong `app.rs`, tái sử dụng `routes::settings::{parse_if_match, no_store_headers}`):**
   - `content_mode` (`"editor" | "builder"`) lưu trên `content_entries` và được cập nhật qua `PUT /api/admin/entries/{type_key}/{id}` (`CompositeEntrySaveInput`, trường `content_mode: PatchField<ContentMode>`). Nếu đặt `"builder"` trên CPT chưa bật `ContentSupport::Builder`, trả `422 Unprocessable Entity`.
   - `GET /api/admin/entries/{type_key}/{id}/builder` (`require_session`): kiểm tra `(type_key, id)` tồn tại và CPT có `ContentSupport::Builder` (trả `403` nếu tắt) → trả `200` + `ETag: "vN"` + `{ entry_id, type_key, content_mode, version, document: Option<BuilderDocument> }`.
   - `PUT /api/admin/entries/{type_key}/{id}/builder` (`require_session_with_csrf` + `AdmittedMutation` + `If-Match: "vN"` + `Content-Type: application/json`, body ≤ `256 KiB`): kiểm tra quyền (riêng `type_key == "pattern"` yêu cầu **`require_admin_with_csrf`**), kiểm tra `ContentSupport::Builder` (`403`), cập nhật `entry_builder_documents`, `entry_builder_media_refs`, `entry_pattern_refs`, tăng `content_entries.version = version + 1` và `updated_at = datetime('now')`, phát `HookEvent::ContentAfterSave` sau commit → trả `200` + `ETag: "vN+1"` + `{ entry_id, version, updated_at }`. Lỗi: `428` (thiếu `If-Match`), `412` (stale), `415` (sai Content-Type), `400` (AST/depth không hợp lệ).
   - `POST /api/admin/entries/{type_key}/{id}/builder/preview` (`require_session_with_csrf`, body `BuilderDocument`): render preview HTML với cùng giới hạn bảo mật (`16` pattern, `1024` node, `512 KiB`), **không ghi DB**, **không cấp quyền media public** → trả `200` `{ html, has_active_contact_form }`.
   - **Render vào `{{ od_content() }}` (Spec 1):** Nếu `entry.content_mode == "builder"` **và** CPT đang bật `ContentSupport::Builder` **và** có row trong `entry_builder_documents`, render Builder AST (truyền cờ `has_active_contact_form` ra CSP response); ngược lại fallback an toàn về `values_json["body"]`.

### B. Pattern CPT (`pattern`) & Block `core/pattern_ref` (Depth = 1)

1. **Block lõi `KnownElement::PatternRef(PatternRefProps)` (`opendash_core::builder`):**
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub struct PatternRefProps {
       pub pattern_id: i64,
   }
   ```
   - Nằm trong enum `KnownElement` với `#[serde(tag = "type", content = "props", deny_unknown_fields)]`, JSON chính xác: `{ "type": "core/pattern_ref", "props": { "pattern_id": 42 } }`.
   - **Cấu trúc render trong 4 tầng AST:** Trong `page.rs`, khi render một `KnownElement::PatternRef` bên trong `Column`, core render các `Section` của Pattern đó bên trong wrapper `<div class="od-pattern-ref">` (cùng quy tắc sanitize & responsive hiện hành), truyền cờ `has_active_contact_form` lên trang cha.
   - **Giới hạn độ sâu = 1 & chống DoS:**
     - Khi lưu `entry_builder_documents` cho `type_key == "pattern"`: từ chối `400 Bad Request` nếu AST chứa bất kỳ `KnownElement::PatternRef` hoặc `KnownElement::ContentList` nào (vừa chặn tuyệt đối vòng lặp/đệ quy, vừa tránh xung đột `list_page_<node_id>` khi nhúng 1 Pattern nhiều lần trên cùng trang).
     - Khi lưu entry thường chứa `PatternRef`: xác nhận mọi `pattern_id` đều có `content_type_key = 'pattern'` trong `content_entries` (chống tham chiếu chéo sang CPT khác) và ghi vào bảng `entry_pattern_refs(source_entry_id, target_pattern_id)`.
     - Khi render trang: nạp batch tối đa `16` Pattern duy nhất trong 1 snapshot đọc SQLite, giới hạn tổng số node sau mở rộng `<= 1024` và output `<= 512 KiB`. Nếu `pattern_id` không tồn tại (đã bị xóa): public bỏ qua khối đó an toàn; Admin Builder hiển thị khung cảnh báo thiếu Pattern.
2. **RBAC & API Pattern CPT (`routes/patterns.rs` + `app.rs`):**
   - `GET /api/admin/patterns` (`require_session`) → `200` `{ items: [{ id, title, slug, version, referencing_count, created_at, updated_at }] }`.
   - `GET /api/admin/patterns/{id}` (`require_session`) → `200` + `ETag: "vN"` + metadata Pattern.
   - `POST /api/admin/patterns` (`require_admin_with_csrf` + `AdmittedMutation`) → Body `{ title, slug }` → `201` + `ETag: "v1"`.
   - `PUT /api/admin/patterns/{id}` (`require_admin_with_csrf` + `AdmittedMutation` + `If-Match: "vN"`) → Body `{ title, slug }` → `200` + `ETag: "vN+1"`.
   - `DELETE /api/admin/patterns/{id}` (`require_admin_with_csrf` + `AdmittedMutation` + `If-Match: "vN"`) → Nếu `entry_pattern_refs` có `>= 1` bản ghi trỏ tới `id` và không có `?force=true`, trả `409 Conflict` `{ error, referencing_count, referencing_entries: [{ id, type_key, title }] }`; nếu không có tham chiếu hoặc có `?force=true`, xóa Pattern và trả `204 No Content`.
   - **Builder AST của Pattern** dùng trực tiếp API chung `/api/admin/entries/pattern/{id}/builder` (yêu cầu `require_admin_with_csrf`), không nhân đôi code storage/route!
3. **Bảo vệ Media & Public Media ACL (1-hop join cực nhanh nhờ depth = 1):**
   - `storage/media_refs.rs::collect_pins_in_tx` quét `entry_builder_media_refs` (bao phủ cả bài viết lẫn Pattern, gỡ bỏ wrapper `media_delete_section_aware` thừa ở `app.rs:563`).
   - `storage/public_content.rs::media_is_referenced`: một media là public nếu nằm trong `entry_builder_media_refs` của entry thỏa `publication_status = 'published' AND content_mode = 'builder' AND CPT active/public/supports(builder)`, **HOẶC** nằm trong `entry_builder_media_refs` của một `pattern` được tham chiếu qua `entry_pattern_refs` bởi một entry thỏa đúng điều kiện public đó (chỉ đúng 1-hop join!).
   - Cập nhật toàn bộ 7 file backend có guard `!= 'section'` (`storage/content.rs` `preflight_builtin_key_collisions`, `permalinks.rs`, `public_content.rs`, `sitemap.rs`, `seo.rs`, `entries.rs`, `routes/content.rs`) sang `'pattern'`.

### C. Pattern Library (`Explore all patterns`)

- **Endpoints (`routes/patterns.rs` + `app.rs`, `require_session`):**
  - `GET /api/admin/patterns/library?category=&q=` → `200` `{ categories: ["all", ...], items: [{ source: "theme", id: "theme:<theme_id>:<key>", key, title, category, description, preview_url: Option<String> }] }`. Trả `items: []` nếu chưa có theme active. *(Đọc trực tiếp từ `ThemeV3Manifest.patterns` của active theme via hàm `list_theme_patterns()`, không tạo trait thừa; `preview_url` dùng đường dẫn `/theme/<theme_id>/<rel_path>` hiện hành.)*
  - `GET /api/admin/patterns/library/theme/{key}` (`key` kiểm qua `valid_snake_key`) → `200` `{ source: "theme", key, title, document: BuilderDocument }` (`404` nếu không tồn tại hoặc không có active theme).
- **Copy AST & ảnh tĩnh trong Theme Pattern:**
  - Khi người dùng bấm chọn mẫu trong `PatternLibraryModal.tsx`, helper `cloneWithFreshNodeIds(doc)` tái tạo toàn bộ `node_id` trước khi chèn vào canvas.
  - Ảnh minh họa tĩnh của Theme Pattern dùng URL `/theme/<theme_id>/<rel_path>` (đã có trong allowlist của `template_sec.rs`, không tạo media rác vào `media_assets`; nếu đổi theme sau này, ảnh hiển thị fallback an toàn mà không làm hỏng AST).
- **Giao diện Visual Builder (`BuilderWorkspace.tsx`, `BuilderInspector.tsx`, `PatternLibraryModal.tsx`):**
  - Sidebar trái có 3 tab (`Blocks` | `Patterns` | `Media`) giống Gutenberg:
    - Tab **Blocks**: có block `Reusable Pattern` (`core/pattern_ref`); Inspector hiển thị `<SelectField>` chọn 1 Pattern từ `GET /api/admin/patterns`.
    - Tab **Patterns**: liệt kê danh mục + nút **`Explore all patterns`** mở `PatternLibraryModal.tsx` (ô tìm kiếm, cột danh mục bên trái, lưới preview bên phải; bấm chọn là COPY AST vào canvas).

### D. SQLite Migrations (Tách 2 migration để từng Slice compile & test độc lập)

**Migration `0043_cpt_content_builder.sql` (Slice S01):**
```sql
ALTER TABLE content_entries
    ADD COLUMN content_mode TEXT NOT NULL DEFAULT 'editor'
    CHECK (content_mode IN ('editor', 'builder'));

CREATE TABLE entry_builder_documents (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    document_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE entry_builder_media_refs (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL,
    ref_role TEXT NOT NULL,
    PRIMARY KEY (entry_id, media_id, ref_role)
);
CREATE INDEX idx_entry_builder_media_refs_media ON entry_builder_media_refs(media_id);
```

**Migration `0044_pattern_cpt_and_refs.sql` (Slice S02):**
```sql
-- Chuyển private CPT 'section' -> 'pattern' và hợp nhất vào entry_builder_documents
UPDATE content_types SET key = 'pattern', label = 'Patterns' WHERE key = 'section';
UPDATE content_entries SET content_type_key = 'pattern', content_mode = 'builder' WHERE content_type_key = 'section';

INSERT OR REPLACE INTO entry_builder_documents (entry_id, document_json, updated_at)
SELECT entry_id, document_json, datetime('now') FROM section_documents;

INSERT OR IGNORE INTO entry_builder_media_refs (entry_id, media_id, ref_role)
SELECT entry_id, media_id, ref_role FROM section_document_media_refs;

DROP TABLE IF EXISTS section_document_media_refs;
DROP TABLE IF EXISTS section_documents;

CREATE TABLE entry_pattern_refs (
    source_entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    target_pattern_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    PRIMARY KEY (source_entry_id, target_pattern_id)
);
CREATE INDEX idx_entry_pattern_refs_target ON entry_pattern_refs(target_pattern_id);
```

---

## Alternatives Considered

| Phương án thay thế | Cơ chế & Đánh đổi | Lý do bác bỏ |
|---|---|---|
| Cho phép lồng `core/pattern_ref` depth = 2 | Hỗ trợ Pattern gọi Pattern khác nhưng đòi hỏi kiểm tra đồ thị cycle có write lock và 2-hop transitive join trong public media ACL. | Quá phức tạp so với nhu cầu thực tế; giới hạn depth = 1 triệt tiêu 100% rủi ro vòng lặp/đệ quy và giữ media ACL 1-hop đơn giản, nhanh. |
| Tạo bảng `pattern_documents` / `pattern_document_media_refs` riêng | Tách bảng Pattern khỏi bài viết. | Nhân đôi storage, `media_refs`, `media_lifecycle` và backup/restore trong khi `pattern` đã là một `content_entries` với `supports: [Builder]`. |
| Phân biệt `unsynced`/`synced` trên cùng Pattern CPT | Giống WP 6.3+ nhưng người dùng phải quản lý cờ đồng bộ và thao tác Detach. | Chủ dự án xác nhận mô hình **Pattern CPT (`core/pattern_ref`) = đồng bộ** + **Pattern Library (`patterns/`) = COPY AST** rõ ràng hơn nhiều. |
| Tạo URI scheme mới `theme-asset://` và trait `PatternCatalogProvider` | Trừu tượng hóa sớm cho Cloud và tài sản theme. | YAGNI và phát sinh parser/rewriter mới; tái sử dụng `/theme/...` và giữ trường `"source": "theme"` trong DTO là đủ gọn và mở rộng được. |

---

## Non-Functional Requirements

- **Phân tầng & Base APIs ([CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md)):** `opendash-core` thuần Rust; 100% SQLite queries trong `storage/`; mọi mutation dùng `require_session_with_csrf` / `require_admin_with_csrf` + `AdmittedMutation` ([M8_WRITER_INVENTORY.md](../contracts/M8_WRITER_INVENTORY.md)); mọi `fetch` React đi qua `cms/admin/src/api/client.ts`.
- **An toàn Media & Publication:** Media chỉ dùng trong `entry_builder_documents` của bản `draft` (hoặc bài `content_mode = 'editor'`, hoặc CPT tắt `Builder`, hoặc Pattern chưa gắn vào bài `published`) **không bao giờ** trả `200` ở route public; `collect_pins_in_tx` bảo vệ toàn bộ `entry_builder_media_refs`.
- **Chống DoS:** Tối đa 16 `core/pattern_ref` duy nhất / trang, depth = 1, tổng số node sau mở rộng `<= 1024`, HTML render `<= 512 KiB`, áp dụng giống hệt trên cả public lẫn `/builder/preview`.
- **Bảo toàn dữ liệu & Backup/Restore:** `entry_builder_documents`, `entry_builder_media_refs`, `entry_pattern_refs`, `content_mode` và `pattern` CPT entries được bảo toàn qua restart, đổi theme và `age` backup/restore.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | Reason |
|---|---|---|
| `cms/migrations/0043_cpt_content_builder.sql` | Create | Thêm `content_entries.content_mode`, `entry_builder_documents`, `entry_builder_media_refs`. |
| `cms/migrations/0044_pattern_cpt_and_refs.sql` | Create | Đổi `section` → `pattern`, hợp nhất `section_documents*` vào `entry_builder_*`, tạo `entry_pattern_refs`. |
| `cms/crates/opendash-core/src/content.rs` | Modify | Thêm `ContentSupport::Builder` (`page` bật, `post` tắt); ở S02 đổi `private_section()` → `private_pattern()`. |
| `cms/crates/opendash-core/src/builder.rs` | Modify | Thêm `KnownElement::PatternRef(PatternRefProps)` (`#[serde(tag = "type", content = "props")]`) và validator cấm `PatternRef`/`ContentList` bên trong Pattern. |
| `cms/crates/opendash-server/src/storage/{entries,builder,media_refs,public_content}.rs` | Modify | Lưu/đọc `content_mode`, CRUD & preview `entry_builder_documents` (CAS `content_entries.version`), `collect_pins_in_tx` và `media_is_referenced` (1-hop `entry_pattern_refs`). |
| `cms/crates/opendash-server/src/storage/patterns.rs` (thay `storage/sections.rs`) & `storage/mod.rs` | Create / Modify | Quản lý Pattern CPT (`pattern`), đếm tham chiếu `entry_pattern_refs` khi xóa, và `list_theme_patterns()` đọc `patterns/*.json` của active theme v3. |
| `cms/crates/opendash-server/src/storage/{content,permalinks,sitemap,seo}.rs` | Modify | Đổi toàn bộ guard `!= 'section'` và `preflight_builtin_key_collisions` sang `'pattern'`. |
| `cms/crates/opendash-server/src/page.rs` | Modify | Giải quyết `KnownElement::PatternRef` (batch tối đa 16 Pattern, depth = 1, node cap 1024, fallback an toàn khi thiếu, truyền cờ CSP). |
| `cms/crates/opendash-server/src/routes/{builder,patterns,content,themes,public,public_content,mod}.rs` & `app.rs` | Modify / Create | Đăng ký `/api/admin/entries/{type_key}/{id}/builder*`, `/api/admin/patterns*`, `/api/admin/patterns/library*` trong `app.rs`; gỡ `routes/sections.rs` & `media_delete_section_aware`; nối render vào `{{ od_content() }}`. |
| `docs/contracts/M8_WRITER_INVENTORY.md` & `docs/product/PRODUCT_REQUIREMENTS.md` | Modify | Cập nhật danh sách writer endpoints mới và ghi chú cho `core/pattern_ref`. |
| `cms/admin/src/api/{builder,patterns,content}.ts`, `App.tsx`, `shell/router.ts`, `shell/AdminShell.tsx` | Modify / Create | Typed API clients (thay `api/sections.ts`), cập nhật route/nav `'patterns'` (thay `'sections'`). |
| `cms/admin/src/features/content/{ContentTypeSchemaSettings,ContentTypeForm}.tsx`, `features/editor/SingleEditWorkspace.tsx`, `features/library/TemplateLibraryPage.tsx` | Modify | Checkbox bật/tắt `Builder` cho CPT; chuyển đổi `Editor ↔ Visual Builder` bảo toàn cả 2 nguồn; bỏ phụ thuộc `api/sections.ts`. |
| `cms/admin/src/features/patterns/PatternsPage.tsx` (thay `features/sections/SectionsPage.tsx`) | Create / Replace | Màn quản lý **Patterns** CPT (hiển thị số trang đang gọi, hộp thoại xác nhận `?force=true` khi gặp `409`). |
| `cms/admin/src/builder/{BuilderWorkspace,BuilderInspector,PatternLibraryModal}.tsx` & `builder/document.ts` | Modify / Create | Block `Reusable Pattern` (`core/pattern_ref` + select Pattern CPT), tab `Patterns` ở sidebar trái, modal `Explore all patterns` và helper `cloneWithFreshNodeIds`. |

---

### Slice S01: Generic CPT `ContentSupport::Builder`, `content_mode` & Unified Entry Builder (Migration `0043`)

* **Dependencies:** Spec 1 (`S01`–`S03` đã merge để có slot `{{ od_content() }}` & `core/content_slot`).
* **File Ownership:** `cms/migrations/0043_cpt_content_builder.sql`, `cms/crates/opendash-core/src/content.rs`, `cms/crates/opendash-server/src/storage/{entries,builder,media_refs,public_content}.rs`, `cms/crates/opendash-server/src/routes/{builder,public,public_content}.rs`, `cms/crates/opendash-server/src/app.rs`, `docs/contracts/M8_WRITER_INVENTORY.md`, `cms/admin/src/api/{builder,content}.ts`, `cms/admin/src/features/content/{ContentTypeSchemaSettings,ContentTypeForm}.tsx`, `cms/admin/src/features/editor/SingleEditWorkspace.tsx`.
* **Công việc chính:**
  1. Áp dụng Migration `0043` và thêm `ContentSupport::Builder` (`page` mặc định bật, `post` mặc định tắt; giữ nguyên `"section"` ở slice này để không gãy compile/test).
  2. Triển khai CRUD & preview cho `/api/admin/entries/{type_key}/{id}/builder` (CAS `content_entries.version`, `updated_at`, `ContentAfterSave`, kiểm `ContentSupport::Builder` → `403`, tái sử dụng `routes::settings::{parse_if_match, no_store_headers}`).
  3. Cập nhật `collect_pins_in_tx`, `media_is_referenced` và nối render vào `{{ od_content() }}` (Spec 1) với fallback về `values_json["body"]` + truyền cờ CSP.
  4. Cập nhật UI cấu hình `ContentSupport::Builder` và chuyển chế độ `Editor ↔ Visual Builder` trên `SingleEditWorkspace.tsx`.
* **Acceptance Criteria:**
  - [ ] `page` mặc định bật `Builder`, `post` mặc định tắt (`403` trên API + ẩn nút UI cho tới khi Admin bật); CPT tùy chỉnh bật/tắt đúng; sửa A không đổi B hay shared template.
  - [ ] Đổi `editor ↔ builder` và tắt `Builder` giữ nguyên cả `values_json["body"]` lẫn `entry_builder_documents`; draft & media private không lộ qua public/preview.
  - [ ] `npm run build`, `check-standards.py`, `cargo test --workspace --release --locked` PASS.
* **Rollback:** `git revert` commit của Slice S01 và khôi phục DB fixture trước Migration `0043`.

### Slice S02: Pattern CPT (`pattern`), Block `core/pattern_ref` (Depth = 1) & Guard Migration (`0044`)

* **Dependencies:** Spec 2 Slice `S01`.
* **File Ownership:** `cms/migrations/0044_pattern_cpt_and_refs.sql`, `cms/crates/opendash-core/src/{content,builder}.rs`, `cms/crates/opendash-server/src/storage/{patterns,mod,entries,content,permalinks,sitemap,seo,media_refs,public_content}.rs`, `cms/crates/opendash-server/src/page.rs`, `cms/crates/opendash-server/src/routes/{patterns,mod,content,themes}.rs`, `cms/crates/opendash-server/src/app.rs`, `docs/contracts/M8_WRITER_INVENTORY.md`, `docs/product/PRODUCT_REQUIREMENTS.md`, `cms/admin/src/api/patterns.ts`, `cms/admin/src/App.tsx`, `cms/admin/src/shell/{router.ts,AdminShell.tsx}`, `cms/admin/src/features/patterns/PatternsPage.tsx`, `cms/admin/src/features/library/TemplateLibraryPage.tsx`, `cms/admin/src/builder/{BuilderInspector.tsx,document.ts}`.
* **Công việc chính:**
  1. Áp dụng Migration `0044` hợp nhất `section_documents*` vào `entry_builder_*`, đổi `'section'` → `'pattern'` trên `content.rs` và toàn bộ 7 file backend có guard `!= 'section'`, gỡ `storage/sections.rs`, `routes/sections.rs` và `media_delete_section_aware`.
  2. Thêm `KnownElement::PatternRef(PatternRefProps)` trong `opendash_core::builder`, chặn `PatternRef`/`ContentList` bên trong Pattern (depth = 1), cài đặt batch resolver trong `page.rs` (tối đa 16 Pattern, 1024 node, fallback an toàn khi thiếu) và 1-hop `entry_pattern_refs` trong `media_is_referenced`.
  3. Triển khai `/api/admin/patterns` (`GET` session, `POST/PUT/DELETE` `require_admin_with_csrf`, `409 Conflict` kèm `referencing_count` trừ khi `?force=true`; Builder AST dùng lại `/api/admin/entries/pattern/{id}/builder`).
  4. Xây dựng `PatternsPage.tsx` và block `Reusable Pattern` (`<SelectField>` chọn Pattern CPT) trong `BuilderInspector.tsx`.
* **Acceptance Criteria:**
  - [ ] Nhúng cùng 1 Pattern CTA vào 3 trang (`Home`, `Contact`, `About`), Admin sửa Pattern 1 lần → cả 3 trang hiển thị nội dung mới; Editor nhận `403` khi gọi `POST/PUT/DELETE /api/admin/patterns*`.
  - [ ] Từ chối `400` khi nhúng `PatternRef` hoặc `ContentList` bên trong Pattern; `DELETE` Pattern đang dùng trả `409` kèm danh sách trang và chỉ xóa khi `?force=true`; trang tham chiếu Pattern đã xóa không crash.
  - [ ] Public media ACL cấp quyền 1-hop cho media trong Pattern gắn trên trang `published` và thu hồi ngay khi trang về `draft`/đổi mode; `npm run build` & `cargo test --workspace --release --locked` PASS.
* **Rollback:** `git revert` commit của Slice S02 và khôi phục DB fixture trước Migration `0044`.

### Slice S03: Pattern Library (`Explore all patterns` — Theme `patterns/` & Gutenberg Modal)

* **Dependencies:** Spec 1 Slice `S02` (Theme v3 `patterns`) và Spec 2 Slice `S02` (`storage/patterns.rs`, `routes/patterns.rs`).
* **File Ownership:** `cms/crates/opendash-server/src/storage/patterns.rs`, `cms/crates/opendash-server/src/routes/patterns.rs`, `cms/crates/opendash-server/src/app.rs`, `cms/admin/src/api/patterns.ts`, `cms/admin/src/builder/{BuilderWorkspace.tsx,PatternLibraryModal.tsx,document.ts}`.
* **Công việc chính:**
  1. Triển khai `list_theme_patterns()` và 2 endpoint `GET /api/admin/patterns/library` (`?category=&q=`) & `GET /api/admin/patterns/library/theme/{key}` đọc từ `ThemeV3Manifest.patterns` của active theme v3 (ảnh dùng `/theme/<theme_id>/<rel_path>`).
  2. Cài đặt `cloneWithFreshNodeIds(doc)` trong `cms/admin/src/builder/document.ts`.
  3. Thêm tab `Patterns` ở sidebar trái của `BuilderWorkspace.tsx` và modal `PatternLibraryModal.tsx` (`Explore all patterns` với tìm kiếm, cột danh mục, lưới xem trước và chèn COPY AST vào canvas).
* **Acceptance Criteria:**
  - [ ] `GET /api/admin/patterns/library` lọc đúng theo `category` và `q` từ `patterns/*.json` của active theme v3 (trả `[]` khi chưa có active theme).
  - [ ] Bấm chèn mẫu từ `PatternLibraryModal.tsx` tạo bản COPY AST với toàn bộ `node_id` mới; sửa trên trang không ghi ngược file theme bất biến.
  - [ ] `npm run build`, `check-standards.py`, `cargo test --workspace --release --locked` PASS.
* **Rollback:** `git revert` commit của Slice S03 (không thay đổi schema SQLite).

---

### Verification Plan

1. **Lệnh kiểm chứng chuẩn (bắt buộc PASS độc lập ở từng Slice S01, S02, S03):**
   ```powershell
   python cms/scripts/check-standards.py
   python cms/scripts/check-standards.py --self-test
   python cms/scripts/check-plans.py
   python cms/scripts/check-plans.py --self-test
   cargo fmt --manifest-path cms/Cargo.toml --all --check
   cargo build --manifest-path cms/Cargo.toml --locked
   cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
   git diff --check
   ```
2. **Ma trận kiểm thử hành vi bắt buộc:**
   - **CPT Builder Capability:** `page` mặc định bật, `post` mặc định tắt (bị chặn cả UI & API `403` cho tới khi Admin bật), CPT tùy chỉnh bật/tắt hoạt động đúng; sửa bản ghi A không đổi bản ghi B hay shared template.
   - **Bảo toàn dữ liệu & Publication:** Đổi `editor ↔ builder`, tắt `Builder` trên CPT, đổi theme và restart giữ nguyên cả `values_json["body"]` lẫn `entry_builder_documents`; bản `draft` (kể cả Homepage Page) và media private không lộ qua public hay preview; `401`/`412` giữ nguyên bản nháp.
   - **Pattern CPT & `core/pattern_ref` (depth = 1):** Nhúng cùng 1 Pattern CTA vào 3 trang (`Home`, `Contact`, `About`), Admin sửa Pattern 1 lần → cả 3 trang hiển thị nội dung mới; Editor bị chặn `403` khi gọi `POST/PUT/DELETE /api/admin/patterns*`; từ chối `400` khi nhúng `PatternRef` hoặc `ContentList` bên trong Pattern; `DELETE` Pattern đang dùng trả `409` kèm danh sách trang và chỉ xóa khi `?force=true`; trang tham chiếu Pattern đã xóa không crash; public media ACL cấp quyền 1-hop cho media trong Pattern gắn trên trang `published` và thu hồi ngay khi trang về `draft`/đổi mode.
   - **Pattern Library:** Lọc theo danh mục/từ khóa trả đúng mẫu từ `patterns/*.json` của active theme v3; bấm chèn tạo bản COPY AST với `node_id` mới hoàn toàn, sửa trên trang không ghi ngược file theme bất biến; restart và `age` backup/restore bảo toàn đầy đủ.


## D02 approved coordination amendment

See [S02 target-only Page Builder and provider selections](s02-target-only-page-builder.md). This Approved amendment supersedes conflicting creation/order details in the slices above; implementation authority requires its merge and pinned handoff. It brings the Page-only entry document foundation into Theme S02/0042; later CPT S01/0043 extends those tables rather than recreating them. Owner clarified that no previous release or historical-data compatibility is required. Generic CPT, Patterns and Reading Settings remain in their existing later slices.
