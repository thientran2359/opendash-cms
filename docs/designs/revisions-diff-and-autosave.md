# Lịch sử Phiên bản (Revisions & Diff) và Tự động lưu (Autosave) cho Nội dung & Studio Mode 2

* **Author:** Orion (Elephant / Antigravity)
* **Status:** Approved
* **Approved By:** Owner
* **Approval Evidence:** Owner phê duyệt trực tiếp ("duyệt") ngày 2026-11-27 cho PRD delta (`docs/product/PRODUCT_REQUIREMENTS.md`) và Design Spec này.
* **Date:** 2026-11-26 *(synced onto `origin/main@ecbea04` on 2026-11-27)*
* **Baseline:** `ecbea04` (`origin/main`, đã hợp nhất PR #615 Gettext `.pot`/`.po`/`.mo`, PR #616 UFE S04 `0056`, PR #619 Manual Redirects & 404 Log `0057` nâng nền DB lên `229` objects `sqlite_schema`, PR #620 Build/Test Quick Wins, PR #621 Test Routing & PR #622 UFE Drift R1)
* **Goldfish Review:** 3-agent fresh-context verification on `origin/main@43c19c2` · 2026-11-27 · `egm-implementation-readiness` `GO` (`0 Blocker / 0 Major / 0 Minor`), `egm-codebase-details` (`0 Blocker / 0 Major / 0 Minor`), `egm-design` `APPROVED` (`0 Blocker / 0 Major / 0 Minor`) · pre-stamp SHA-256 `54c56b1cdcb6206fbaffa22f489235e2f97da8bf408569c43e8ace6cff9e840d` · đồng bộ sau duyệt sang `origin/main@ecbea04` (`0058` nối sau `0057`, UFE S04 + Drift R1, Gettext `.pot`/`.po`).
* **Related Contracts & Specs:**
  * [Yêu cầu Sản phẩm OpenDash](../product/PRODUCT_REQUIREMENTS.md)
  * [Core Foundation & Quy chuẩn Code](../contracts/CORE_FOUNDATION.md)
  * [Plugin ABI v1.1](../contracts/PLUGIN_ABI.md)
  * [Unified Field Schema V1](../contracts/UNIFIED_FIELD_SCHEMA.md)
  * [M8 Writer & Recovery Inventory](../contracts/M8_WRITER_INVENTORY.md)
  * [OpenDash Studio (Customizer + UX Builder) & Comment-Block Entry Content](opendash-studio-customizer-and-ux-builder.md)
  * [Entry Lifecycle, Bulk Actions & Quick Edit](entry-lifecycle-bulk-quick-edit.md)
  * [Unified Field Engine V1](unified-field-engine.md)
  * [Test Suite Performance & Reliability](test-suite-performance-and-reliability.md)
  * [Design Sitemap](SITEMAP.md)

---

## Overview

Đặc tả này bổ sung **Lịch sử phiên bản có so sánh khác biệt (Revisions & Diff)** và **Tự động lưu trên server theo từng người dùng (Server-Side Per-User Autosave)** cho toàn bộ **Content Entries** (`post`, `page` và mọi Custom Post Type tùy chỉnh — bao gồm cả **Single Edit** và **Landing Page trên OpenDash Studio Mode 2**) cùng **Pattern CPT** riêng tư (`type_key = "pattern"`, chỉ dành cho `Admin`).

Hai cơ chế hợp thành "lưới an toàn" chuẩn WordPress tương thích trọn vẹn với kiến trúc Rust + SQLite một file thực thi của OpenDash:
1. **Revisions & Diff (`entry_revisions`):** Chụp snapshot bất biến mỗi khi nội dung thay đổi qua đường lưu chính thức (`create`, `save`, `builder_save`, `builder_reset`, `quick_edit`, `seo_save`, `pattern_save`, `plugin_save`, `restore`, `import`), chống trùng lặp bằng `content_hash` (SHA-256), giữ tối đa **25 revisions mới nhất trên mỗi entry**, hỗ trợ **so sánh 3 lớp** (Word/Line Text Diff, **Block-Aware Structural Diff** trên `BuilderDocument` của Landing Page/Pattern, và Field/Taxonomy/Template/SEO Diff) và **khôi phục an toàn** tạo thành một revision mới có thể hoàn tác.
2. **Server Autosave (`entry_autosaves`):** Lưu bản nháp tự động định kỳ theo khóa `(entry_id, user_id)` tách biệt hoàn toàn khỏi bản ghi chính trong `content_entries` — **không bao giờ ghi đè bản `draft` đã lưu, bài `published` hay CPT cấu hình `immediate`**, không làm tăng `content_entries.version`, không chạy plugin hooks và không làm lộ dữ liệu chưa lưu ra trang công khai.

---

## The Problem

1. **Rủi ro mất dữ liệu & thiếu đường hoàn tác trên bài viết/landing page:**
   - Hiện tại [`EditorDraftStore.ts`](../../cms/admin/src/features/editor/EditorDraftStore.ts) chỉ giữ thay đổi chưa lưu trong RAM trình duyệt (`Map`) để phục hồi sau lỗi `401`/`412`; nếu người viết đóng nhầm tab, trình duyệt crash hoặc mất kết nối, toàn bộ chỉnh sửa chưa bấm Lưu sẽ mất sạch.
   - Sau khi đã bấm Lưu hoặc Cập nhật trên bài viết dài ([`SingleEditWorkspace.tsx`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx)) hay landing page có nhiều tầng block ([`StudioWorkspace.tsx`](../../cms/admin/src/studio/StudioWorkspace.tsx)), [`content_entries`](../../cms/migrations/0005_content_entries.sql) chỉ giữ đúng trạng thái cuối cùng (`version` là bộ đếm CAS, không lưu lịch sử). Không có cách xem lại thay đổi so với bản trước hay quay lại bố cục trước khi hỏng.
2. **Chiến lược lưu phân tán giữa `pattern` và entry thường:**
   - Các entry thường (`post`, `page`, CPT) dùng Comment-Block `<!-- od:... -->` nằm ngay trong `content_entries.values_json["body"]` ([`storage/builder/mod.rs`](../../cms/crates/opendash-server/src/storage/builder/mod.rs)), còn `pattern` CPT dùng bảng `entry_builder_documents.document_json` (`values_json = '{}'`). Nếu chỉ chụp `content_entries`, thay đổi bố cục trên Pattern sẽ bị bỏ sót; nếu chỉ diff chuỗi raw của Comment-Block, người dùng không đọc được block nào vừa thêm/xóa/di chuyển.
3. **Ràng buộc an toàn Media & Lifecycle từ nền tảng hiện tại:**
   - [`media_refs.rs::collect_pins_in_tx`](../../cms/crates/opendash-server/src/storage/media_refs.rs#L90) và [`media_is_referenced.rs::media_is_referenced`](../../cms/crates/opendash-server/src/storage/public_content/taxonomy_queries/media_is_referenced.rs#L6) kiểm soát chặt quyền xóa và quyền truy cập media công khai. Nếu revisions cũ ghim chặt media đã thay thế, người dùng sẽ bị kẹt không thể dọn thư viện media.
   - Quy trình Restore phải bảo toàn permalink (`permalinks::sync_claims_in_tx`), trạng thái `draft`/`published`, phân quyền 3 trục (`min_view_role`, `min_edit_role`, `public_exposure`) và xử lý an toàn khi ảnh, video, tệp đính kèm, term, entry tham chiếu, form, block plugin hoặc pattern tham chiếu trong bản lưu cũ không còn tồn tại.

---

## Goals

* **G1 — Lịch sử phiên bản bất biến cho mọi Content Entry & Pattern:** Mọi `content_entries` (`post`, `page`, custom CPT) và `pattern` CPT đều tự động ghi nhận tối đa `MAX_REVISIONS_PER_ENTRY = 25` bản snapshot bất biến trong bảng `entry_revisions`. Tự động bỏ qua khi snapshot trùng `content_hash` với bản mới nhất (sau khi chuẩn hóa SEO toàn rỗng); tự động chụp `baseline` cho dữ liệu tiền-nâng cấp ở lần chỉnh sửa đầu tiên.
* **G2 — Diff Engine thuần Rust 3 lớp có chặn tài nguyên:** Xây dựng module feature-first `opendash_core::revisions` (0 dependency mới, mọi file `<= 600 LOC`) cung cấp: (a) Word-level & Line-level Text/RichText diff với ngân sách cộng dồn `MAX_TOTAL_WORD_DP_CELLS = 500_000` và fallback `O(N)`; (b) **Block-Aware Structural Diff** phân tích cây 4 tầng `Section → Row → Column → Element` từ `BuilderDocument`, chỉ ra rõ block `added`, `removed`, `moved` (kèm thay đổi props nếu có), `modified` theo khóa `(node_kind, block_type, node_id, occurrence_index)` và fallback an toàn khi vượt ngưỡng; (c) Diff từng trường cho Title, Custom Fields (loại trừ `body`, lọc theo quyền xem `min_view_role`), Taxonomies (kèm tên term), Page Layout / Custom Template và Entry SEO.
* **G3 — Khôi phục an toàn, giữ nguyên định danh/URL/trạng thái & hoàn tác được:** `POST /api/admin/content-types/{type_key}/entries/{id}/revisions/{rev_id}/restore` (trong đó `{rev_id}` là khóa chính `entry_revisions.id`) tạo một revision mới (`source = 'restore'`, ghi `restored_from_revision_number`), giữ nguyên `slug`, `custom_path`, `publication_status`, `published_at`, `locale`, `translation_group_id` và `created_at` hiện tại (không làm gãy permalink hay vô tình xuất bản nháp), **áp dụng bảo toàn các trường `!editable` trước khi kiểm tra `missing_refs`**, giữ nguyên cặp `(values["body"], content_mode)` và bỏ qua `sync_entry_body_refs_in_conn` nếu `body` bị khóa sửa, lọc output qua `entries::filter_entry_values_for_role`, đồng bộ `permalinks::sync_claims_in_tx`, kiểm CAS `If-Match` (`412`) trước `409 entry_trashed`, và chạy đầy đủ 4 pha (Preflight → `ContentBeforeSave` ngoài writer lock → Writer tx → `SiteCacheEpoch::global().bump()` + `recompute_projections_nonfatal` + hooks hậu-commit).
* **G4 — Không ghim Media/Term/Pattern/Entry cũ; dọn an toàn khi khôi phục:** Revisions và Autosaves **không** chặn xóa media trong `media_refs.rs` và **không** cấp quyền truy cập media công khai trong `media_is_referenced.rs`. API đọc/so sánh trả về `missing_refs` (`missing_media`, `missing_term_ids` + `TermRef.taxonomy_from_field` kèm `site_locales.deleted_at IS NULL`, `missing_entry_ids`, `missing_pattern_ids`, `missing_contact_form_ids`, `missing_dynamic_options` đủ cả 8 `SelectOptionsSource` gồm `CORE_CONTEXT_FIELDS`, `missing_list_refs`, `missing_block_types`, `cleared_binding_nodes` theo `visible_builder_field` bao gồm `RouteContext` tùy chỉnh & `Field` trên `binding.type_key`, `missing_templates` kiểm `allowed_types`, `dropped_field_keys`/`dropped_block_field_keys` và `non_editable_fields`). Restore chưa xác nhận trả `409 revision_missing_refs`; khi client gửi `acknowledge_missing_refs: true`, server chạy `sanitize_snapshot_for_restore` trên phạm vi editable (tự dọn trường ẩn/tùy chọn và loại bỏ item/block hỏng với ràng buộc không thể để trống gồm `Repeater` `min_items > 0`, trường `required` và fallback prop sau tháo binding) trước khi validate `422` JSON chuẩn `ContentBeforeSaveVeto`.
* **G5 — Autosave server-side theo từng user, cô lập hoàn toàn khỏi bản công khai:** Lưu tự động vào `entry_autosaves` theo khóa `(entry_id, user_id)` mỗi 15 giây debounce (max-wait 60 giây) cho entry đã có ID trên cả Single Edit và Studio Mode 2 (kể cả `pattern` cho Admin), dùng `PatchField<T>` (`Omitted` / `Clear` / `Set(T)`) cho các trường nullable có cờ `*_touched` và `cleared_fields_json`, chống ghi đè ngược thứ tự bằng `AbortController` + `client_seq` đơn điệu. Autosave cho phép `title` rỗng (`""`) khi đang gõ dở (nhưng từ chối `"title": null` với `400`), không ghi đè `content_entries`, không tăng `version`, không kích hoạt hook/projection/cache, tự xóa bản autosave của actor sau khi Save/Restore/Discard thành công và xóa toàn bộ autosaves của entry khi chuyển vào Thùng rác.
* **G6 — Giao diện hợp nhất trên Single Edit & Studio Mode 2 (tuân thủ ratchet LOC):** Cung cấp `AutosaveRecoveryBanner` (quy tắc ưu tiên dứt khoát nhường `EditorDraftStore` RAM draft sau re-auth `401` gồm cả sửa `page_layout_key`/`custom_template_key`, khôi phục vào editor / xem so sánh `current ↔ autosave` / bỏ qua, cảnh báo rõ và tạm dừng timer khi `bannerPendingDecision === true`) và `RevisionHistoryModal` trên cả `SingleEditWorkspace.tsx` (tách `PublishBoxPanel.tsx`, `SingleEditBuilderPromptDialog.tsx`, `SingleEditTranslationModal.tsx`, `useSingleEditRevisions.ts`, `SingleEditRevisionsOverlay.tsx`) lẫn `StudioWorkspace.tsx` Mode 2 (tách `ContentWorkspaces.tsx` từ `App.tsx`, `useStudioMode2Revisions.ts` kèm `handleSaveMode2`, `StudioMode2RevisionsOverlay.tsx` bao gồm banner khôi phục), dùng chuỗi chuẩn Gettext `__()` / `_n()` qua [`i18n/index.tsx`](../../cms/admin/src/i18n/index.tsx) & `cms/l10n/{opendash.pot,vi.po}` + `node cms/scripts/check-i18n.mjs` và giữ mọi file `<= 600` LOC hoặc đúng baseline trong [`loc-baseline.json`](../../cms/scripts/loc-baseline.json).
* **G7 — An toàn nâng cấp, Backup/Restore & Writer Lock:** Migration `0058_entry_revisions_and_autosaves.sql` nối tiếp trực tiếp `0057_manual_redirects_and_404_log.sql` trên `origin/main@ecbea04` (không tạo khoảng trống trên `_sqlx_migrations`), chỉ tăng `+6` đối tượng `sqlite_schema` (nâng tổng số SQLite objects tính cả `sqlite_sequence` và `_sqlx_migrations` trong [`restore.rs:130`](../../cms/crates/opendash-server/src/storage/restore.rs#L130) từ `229` lên `235 / 256`, an toàn dưới ngưỡng `MAX_SCHEMA_OBJECTS = 256`). Tuân thủ `SiteCoordinator` / `AdmittedMutation`, ghi đầy đủ vào [M8_WRITER_INVENTORY.md](../contracts/M8_WRITER_INVENTORY.md) §2–§3, dọn `entry_autosaves` (và giữ `entry_revisions`) khi `restore::validate_and_rebase_copy`.

---

## Non-Goals

* **Studio Mode 1 (`template_overrides`):** Chưa hỗ trợ Revisions & Autosave cho shared templates (`header`, `footer`, `home`, `single`, `archive`), Theme Options, Site Options, Menus và Trang chủ & Bài viết (Reading Settings) trong đợt này.
* **Thực thể ngoài `content_entries`:** Không áp dụng cho Taxonomy Terms, Media Assets, Contact Forms hoặc Plugin Settings.
* **Cộng tác thời gian thực & khóa bài viết (CRDT / Post Locking):** Không triển khai WebSocket/CRDT đồng thời hoặc khóa cứng bài không cho người khác mở; xung đột ghi đồng thời tiếp tục xử lý bằng Optimistic CAS `If-Match` (`412`) hiện hành.
* **Server autosave cho bài mới chưa lưu (`entry_id == null`):** Bài chưa có ID trên server vẫn dùng RAM `EditorDraftStore` như hiện nay; server autosave kích hoạt ngay sau lần Lưu đầu tiên có `entry_id`.
* **Mở rộng Plugin ABI / Host Ops:** Không thêm Host Op mới cho `entry_revisions` / `entry_autosaves` trong `PLUGIN_ABI.md` v1.1 đợt này.
* **UI cấu hình số lượng retention:** Giới hạn `25` revisions/entry là hằng số server trong đợt này, không thêm trang cài đặt riêng.

---

## Critical User Journeys (CUJs)

### CUJ 1: Xem Diff & Khôi phục Phiên bản trên Blog Single Edit (và Hoàn tác Lệnh Khôi phục)
* **Given:** Một Editor đã xuất bản bài viết (`post` #12, `v3`) có tiêu đề, nội dung RichText, ảnh đại diện (`values_json["featured_image"]`), chuyên mục và thẻ SEO, tương ứng với `rev #2` (`revision_number = 2`, `entry_version = 3`). Sau đó Editor sửa sai thân bài và bấm **Cập nhật** lên `v4` — thao tác này tự động sinh `rev #3` (`revision_number = 3`, `entry_version = 4`).
* **When:** Editor mở màn hình sửa bài viết, bấm nút **Lịch sử phiên bản** (trên `publish_box`), chọn `rev #2` trên timeline để so sánh với `Hiện tại` (`v4` / `rev #3`), xem phần thêm/xóa từng từ trong thân bài và thay đổi ở custom fields/taxonomies, rồi bấm **Khôi phục phiên bản #2**.
* **Then:** Hệ thống lưu `v5` và tạo **`rev #4`** (`revision_number = 4`, `source = 'restore'`, `restored_from_revision_number = 2`). Nội dung thân bài, ảnh đại diện, custom fields được phép sửa, taxonomies (kèm đồng bộ permalink) và SEO quay về đúng mốc `rev #2`, trong khi `slug`, `custom_path`, `publication_status = 'published'`, `published_at` và mọi trường chỉ Admin mới được sửa giữ nguyên không đổi. Nếu muốn hoàn tác lệnh khôi phục đó (`v5` quay về lại trạng thái `v4`), Editor có thể mở **Lịch sử phiên bản** và bấm khôi phục **`rev #3`** (tạo `v6` / `rev #5`).

### CUJ 2: Block-Aware Diff & Khôi phục Landing Page trong OpenDash Studio Mode 2
* **Given:** Một Landing Page (`page` #8, `content_mode = 'builder'`) đang ở trạng thái `published`, đã trải qua nhiều lần chỉnh sửa bố cục `Section → Row → Column → Element` (kể cả `PluginBlock` hợp nhất dùng `BlockSchemaV1` với `media`, `group`, `repeater`, `entry_ref`, `term_ref`, `options_source`) trong `#/studio/entry/page/8`.
* **When:** Người chỉnh sửa vô tình xóa một Section Hero và đổi tiêu đề Heading trong một cột khác rồi bấm Lưu; sau đó mở **Lịch sử phiên bản** từ nút overflow `...` trên `StudioSidebarHeader`, xem tab **Thay đổi Block** và khôi phục bản trước đó.
* **Then:** Tab **Thay đổi Block** hiển thị rõ section vừa bị xóa (`removed`), heading vừa sửa (`modified: props.text`) và số block không đổi được gom gọn; khi khôi phục, `StudioWorkspace` gọi `builderApi.getEntryBuilder` nạp lại `EntryBuilderOpen` mới (`vN+1`), cập nhật ngay `entryTitle`, `entryDoc`, `entrySavedDoc`, `entryDiagnostics`, `entryHadExistingDoc`, `entryHadFreeformWrap`, `entryHadRecovery`, xóa undo stack (`resetHistory()`), làm mới cây block + iframe preview và xóa autosave cũ của user đó mà không chuyển trang hoặc làm hỏng slug trang.

### CUJ 3: Lịch sử Phiên bản & Autosave cho Private Pattern CPT (Admin-only)
* **Given:** Một `Admin` đang thiết kế Reusable Pattern (`pattern` #5) trong `#/studio/entry/pattern/5` (dữ liệu AST lưu ở `entry_builder_documents.document_json`) và một `Editor` khác đang đăng nhập.
* **When:** `Admin` lưu thay đổi Pattern, chỉnh sửa thêm chưa bấm Lưu để Autosave chạy sau 15 giây, reload trang và khôi phục Autosave rồi kiểm tra lịch sử phiên bản (kể cả khôi phục về mốc `builder_reset` có `content_mode = 'editor'`); đồng thời `Editor` gọi thử các API revisions/autosave của `type_key = 'pattern'`.
* **Then:** `Admin` thấy đúng `entry_revisions` (chứa `builder_document_json`), diff block chi tiết và khôi phục được Pattern (cả AST lẫn trạng thái reset `entry_builder_documents` về `EMPTY_DOCUMENT` khi `builder_document_json IS NULL`) mà không cấp URL công khai; toàn bộ request `GET`/`PUT`/`POST`/`DELETE` trên `pattern` từ `Editor` bị từ chối với `403 Forbidden`.

### CUJ 4: Autosave Recovery không làm ảnh hưởng bài đã Xuất bản & Cách ly theo User
* **Given:** Một `post` đang `published` (`v5`). User A sửa tiêu đề (thậm chí xóa tạm tiêu đề rỗng để gõ lại) và nội dung trong Single Edit (hoặc sửa block trong Studio Mode 2), đợi Autosave lưu thành công vào `entry_autosaves`, rồi đóng tab mà không bấm **Cập nhật**. Trong lúc đó, User B đăng nhập mở cùng bài viết hoặc một khách truy cập xem URL công khai.
* **When:** Khách truy cập tải URL công khai; User B mở trang sửa bài; sau đó User A quay lại mở đúng bài viết này (và tình huống thứ hai là User B đã lưu lên `v6` trước khi User A quay lại).
* **Then:** Khách truy cập và User B chỉ nhìn thấy đúng bản `v5` đã lưu chính thức (không bao giờ thấy bản autosave của User A). Khi User A quay lại (và không có RAM draft `401` mới hơn trong `EditorDraftStore`), `AutosaveRecoveryBanner` hiện ra mời **Khôi phục bản tự động lưu**, **Xem so sánh** hoặc **Bỏ qua**; nếu bài đã lên `v6` (`base_version = 5 < 6`), banner hiện cảnh báo màu hổ phách và tạm dừng auto-timer (`bannerPendingDecision = true`) cho đến khi User A chọn Khôi phục hoặc Bỏ qua (không ghi thẳng lên DB cho đến khi bấm Lưu/Cập nhật).

### CUJ 5: Khôi phục Revision cũ khi Media, Term, EntryRef, Contact Form hoặc Pattern đã bị Xóa
* **Given:** `rev #1` của một Page từng gắn ảnh đại diện `media_A`, gán tag `T1`, chứa block `core/image` (`media_A`), `core/contact_form` (`form_4`), `core/pattern_ref` (`pattern_9`) và một `PluginBlock` có `entry_ref` / `term_ref` / `media` (`media_B`). Sau đó quản trị viên xóa `media_A` & `media_B` khỏi Media Library (không bị khóa bởi revision), xóa tag `T1`, xóa form `form_4`, trash bài `entry_ref` và force-delete `pattern_9`.
* **When:** User mở so sánh/chi tiết `rev #1` và bấm **Khôi phục phiên bản #1**.
* **Then:** API so sánh và chi tiết đánh dấu rõ các tham chiếu bị thiếu trong `missing_refs` (`missing_media`, `missing_term_ids`, `missing_entry_ids`, `missing_contact_form_ids`, `missing_pattern_ids`, v.v.). Lần gọi `POST .../restore` đầu tiên với `acknowledge_missing_refs = false` trả về `409 revision_missing_refs` (JSON) liệt kê rõ từng mục đã mất để modal yêu cầu xác nhận. Khi user xác nhận (`acknowledge_missing_refs = true`), server tự động dọn an toàn các trường tùy chọn, gỡ item repeater/xóa các block hỏng khỏi `BuilderDocument`, bỏ `T1`, và lưu thành công revision khôi phục mới (`422` JSON chuẩn `ContentBeforeSaveVeto` chỉ xảy ra nếu một custom field cấp top-level được phép sửa có `required = true` bị mất ảnh và không thể để trống).

### CUJ 6: Bảo toàn Phân quyền 3 trục, Trash/Purge, Xóa User & Backup/Restore
* **Given:** Một CPT có trường `internal_notes` (`min_view_role = admin`, `min_edit_role = admin`) và trường `editorial_flag` (`min_view_role = editor`, `min_edit_role = admin`); một bài viết từng được User X (`Editor`) sửa để lại cả revisions lẫn 1 bản autosave.
* **When:** (a) Editor gọi `GET .../revisions/{rev_id}`, `compare`, `GET/PUT .../autosave`, và `POST .../restore`; (b) Bài viết bị chuyển vào Thùng rác rồi Purge vĩnh viễn; (c) Admin xóa User X khỏi hệ thống; (d) Hệ thống sao lưu và phục hồi sang site root mới qua Safe Mode.
* **Then:** (a) Mọi response đọc/diff đều **ẩn hoàn toàn tên và giá trị `internal_notes`** khỏi Editor (chỉ trả cờ boolean `has_hidden_admin_fields` / `has_hidden_admin_changes` mà không lộ tên/số lượng); cho phép Editor xem `editorial_flag` dưới dạng read-only (`editable: false`) và liệt kê trong `missing_refs.non_editable_fields` nếu có khác biệt với DB; và khi Editor restore hoặc autosave, cả `internal_notes` lẫn `editorial_flag` trên DB được giữ nguyên bất biến (**không bao giờ bị `409 missing_refs` chặn bởi ref chết trong field `!editable`**); (b) Trash xóa sạch `entry_autosaves` của entry và chặn toàn bộ endpoint revisions/autosave với `409 entry_trashed`, còn `Purge` xóa sạch cả `entry_revisions` qua `ON DELETE CASCADE`; (c) Xóa User X cascade xóa `entry_autosaves` của User X nhưng giữ `entry_revisions` (`author_user_id = NULL`, giữ nguyên `author_username`); (d) Backup/Restore vượt qua `validate_restored_db_read_only` (`235 <= 256` schema objects) và `validate_and_rebase_copy` dọn sạch `entry_autosaves` lỗi thời nhưng giữ nguyên lịch sử `entry_revisions`.

---

## Technical Plan (Architecture)

### 1. Schema SQLite, Chiến lược Baseline `ecbea04` & Kiểm toán `restore.rs` (`0058_entry_revisions_and_autosaves.sql`)

- **Chiến lược đánh số Migration & Hợp nhất với `origin/main`:**
  - Tại baseline `origin/main@ecbea04`, `cms/migrations/` gồm 56 file `0001`–`0057` (kết thúc ở [`0057_manual_redirects_and_404_log.sql`](../../cms/migrations/0057_manual_redirects_and_404_log.sql) với `+6` net schema objects, tổng thực tế `229` object tính cả `sqlite_sequence` và `_sqlx_migrations`). [`restore.rs:157-179`](../../cms/crates/opendash-server/src/storage/restore.rs#L157-L179) kiểm tra `_sqlx_migrations` tuần tự theo chỉ số tăng dần và [`cms/migrations/README.md:41`](../../cms/migrations/README.md#L41) cấm nhảy cóc số thứ tự migration.
  - Vì `unified-field-engine.md` (`0056` + PR #622) và `manual-redirects-and-404-log.md` (`0057`) đã hợp nhất lên `origin/main@ecbea04`, đặc tả này cấp phát **`0058_entry_revisions_and_autosaves.sql`** (tổng `57` file `0001`–`0058`) và đọc thẳng nền tảng UFE hiện hành:
    - **Custom Content Fields & Studio Blocks (`PluginBlock`):** cả hai cùng dùng thống nhất **`FieldSpec`** (`opendash_core::field_schema::{FieldSpec, FieldType, collect_media_ids, collect_relational_refs}`), truy xuất quyền qua `definition.effective_acl()` (`(min_view_role, min_edit_role, public_exposure)` tại [`field_spec.rs:215-224`](../../cms/crates/opendash-core/src/field_schema/spec/field_spec.rs#L215-L224)), `FieldType::Richtext` cho trường `body`, [`collect_effective_media_ids`](../../cms/crates/opendash-server/src/storage/media/refs.rs#L23) bọc `collect_media_ids`, và [`sync_entry_field_media_refs_in_conn`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/field_media_refs.rs#L12) (`source_kind = 'field'`).
- **Kiểm toán giới hạn `sqlite_schema` ([`restore.rs:14,125-145`](../../cms/crates/opendash-server/src/storage/restore.rs#L125-L145)):**
  - `restore::validate_restored_db_read_only` đếm `SELECT COUNT(*) AS cnt FROM sqlite_schema` **bao gồm cả `sqlite_sequence` và `_sqlx_migrations`**. Trên `ecbea04`, DB khởi tạo thực tế có `229` object (`227` từ migrations `0001`–`0057` tính cả `sqlite_sequence` + `2` từ bảng `_sqlx_migrations` của SQLx; kiểm chứng trực tiếp bằng test thực thi trên `0058` trong S01).
  - Migration `0058` bổ sung đúng **6 đối tượng** trong `sqlite_schema`:
    1. Table `entry_revisions`
    2. Autoindex `sqlite_autoindex_entry_revisions_1` (từ `UNIQUE (entry_id, revision_number)`)
    3. Index `idx_entry_revisions_author`
    4. Table `entry_autosaves`
    5. Autoindex `sqlite_autoindex_entry_autosaves_1` (từ `PRIMARY KEY (entry_id, user_id)`)
    6. Index `idx_entry_autosaves_user`
  - Tổng sau `0058`: `229 + 6 = 235` đối tượng (`234` không tính `sqlite_sequence`) — nằm dưới xa ngưỡng **`MAX_SCHEMA_OBJECTS = 256`**. Không thêm `TRIGGER` hay bảng phụ mới trong `0058`.

```sql
-- 0058_entry_revisions_and_autosaves.sql
-- Additive revision history and per-user server autosaves for content_entries (including pattern CPT).
-- +6 sqlite_schema objects total (2 tables, 2 unique autoindexes, 2 secondary indexes; 0 triggers).

CREATE TABLE entry_revisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    content_type_key TEXT NOT NULL,
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    source TEXT NOT NULL CHECK (
        source IN (
            'create',
            'save',
            'builder_save',
            'builder_reset',
            'quick_edit',
            'seo_save',
            'pattern_save',
            'plugin_save',
            'restore',
            'import',
            'baseline'
        )
    ),
    restored_from_revision_number INTEGER DEFAULT NULL CHECK (
        restored_from_revision_number IS NULL OR restored_from_revision_number >= 1
    ),
    author_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    author_username TEXT NOT NULL DEFAULT 'system',
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    slug TEXT NOT NULL CHECK (length(slug) BETWEEN 1 AND 64),
    content_mode TEXT NOT NULL DEFAULT 'editor' CHECK (content_mode IN ('editor', 'builder')),
    values_json TEXT NOT NULL CHECK (json_valid(values_json) AND json_type(values_json) = 'object'),
    builder_document_json TEXT DEFAULT NULL CHECK (
        builder_document_json IS NULL OR (json_valid(builder_document_json) AND json_type(builder_document_json) = 'object')
    ),
    term_ids_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(term_ids_json) AND json_type(term_ids_json) = 'array'),
    primary_category_term_id INTEGER DEFAULT NULL,
    page_layout_key TEXT DEFAULT NULL,
    custom_template_key TEXT DEFAULT NULL,
    seo_json TEXT DEFAULT NULL CHECK (
        seo_json IS NULL OR (json_valid(seo_json) AND json_type(seo_json) = 'object')
    ),
    entry_version INTEGER NOT NULL CHECK (entry_version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (entry_id, revision_number)
);

CREATE INDEX idx_entry_revisions_author ON entry_revisions (author_user_id, id DESC);

CREATE TABLE entry_autosaves (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content_type_key TEXT NOT NULL,
    source_surface TEXT NOT NULL CHECK (source_surface IN ('editor', 'studio')),
    base_version INTEGER NOT NULL CHECK (base_version >= 1),
    client_seq INTEGER NOT NULL DEFAULT 1 CHECK (client_seq >= 1),
    title TEXT NOT NULL DEFAULT '' CHECK (length(title) <= 200),
    content_mode TEXT NOT NULL DEFAULT 'editor' CHECK (content_mode IN ('editor', 'builder')),
    values_json TEXT NOT NULL CHECK (json_valid(values_json) AND json_type(values_json) = 'object'),
    cleared_fields_json TEXT NOT NULL DEFAULT '[]' CHECK (
        json_valid(cleared_fields_json) AND json_type(cleared_fields_json) = 'array'
    ),
    builder_document_json TEXT DEFAULT NULL CHECK (
        builder_document_json IS NULL OR (json_valid(builder_document_json) AND json_type(builder_document_json) = 'object')
    ),
    term_ids_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(term_ids_json) AND json_type(term_ids_json) = 'array'),
    primary_category_touched INTEGER NOT NULL DEFAULT 0 CHECK (primary_category_touched IN (0, 1)),
    primary_category_term_id INTEGER DEFAULT NULL,
    page_layout_touched INTEGER NOT NULL DEFAULT 0 CHECK (page_layout_touched IN (0, 1)),
    page_layout_key TEXT DEFAULT NULL,
    custom_template_touched INTEGER NOT NULL DEFAULT 0 CHECK (custom_template_touched IN (0, 1)),
    custom_template_key TEXT DEFAULT NULL,
    seo_touched INTEGER NOT NULL DEFAULT 0 CHECK (seo_touched IN (0, 1)),
    seo_json TEXT DEFAULT NULL CHECK (
        seo_json IS NULL OR (json_valid(seo_json) AND json_type(seo_json) = 'object')
    ),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (entry_id, user_id)
);

CREATE INDEX idx_entry_autosaves_user ON entry_autosaves (user_id, updated_at DESC);
```

- **Quy chuẩn đại diện dữ liệu giữa `pattern` và non-pattern & Quy tắc gộp `entry_autosaves`:**
  - **Non-pattern (`post`, `page`, custom CPT):** Toàn bộ nội dung Editor HTML lẫn Comment-Block `<!-- od:... -->` nằm trong `values_json["body"]`, và **`featured_image` (ảnh đại diện)** nằm tại `values_json["featured_image"]` ([`storage/content/builtins/`](../../cms/crates/opendash-server/src/storage/content/builtins/mod.rs)); `builder_document_json = NULL`. Khi dựng Diff, Hydrate DTO đọc/autosave (`RevisionDetailDto.builder_document`, `AutosaveSnapshotDto.builder_document`) hoặc Restore cho entry có `content_mode = 'builder'` (hoặc `values_json["body"]` khác rỗng chứa `<!-- od:` hay đang mở ở Studio), hệ thống phân tích `BuilderDocument` bằng `opendash_core::builder_blocks::parse_blocks_lenient_or_recover(body).document`.
  - **`pattern` CPT (`content_type_key = 'pattern'`):** `content_entries.values_json` luôn là `{}`; cây `BuilderDocument` nằm tại `entry_builder_documents.document_json` ([`entry_builder_snapshot.rs`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/entry_builder_snapshot.rs)). Vì vậy `entry_revisions.builder_document_json` và `entry_autosaves.builder_document_json` lưu chuỗi JSON chuẩn của `BuilderDocument` (`NULL` khi `pattern` ở trạng thái `builder_reset` với `content_mode = 'editor'`, và luôn `NULL` với non-pattern).
  - **Quy tắc gộp `entry_autosaves` theo bề mặt & lọc ACL:**
    - Bảng `entry_autosaves` có `PRIMARY KEY (entry_id, user_id)`:
    - **Nhánh `type_key == "pattern"` (`PUT .../autosave`):** Chỉ chấp nhận `source_surface == "studio"` (trả `400` nếu `"editor"` hoặc có `values`/`cleared_fields`/`term_ids`/`seo`/`page_layout_key`/`custom_template_key`/`primary_category_term_id` khác rỗng/Omitted). Yêu cầu `builder_document: Some(doc)` (`doc` serialized `<= MAX_BUILDER_JSON_BYTES`), lưu vào `builder_document_json`, đặt `content_mode = 'builder'`, `values_json = '{}'`, `cleared_fields_json = '[]'`; nếu `title` là `PatchField::Omitted` thì giữ nguyên `entry_autosaves.title` hiện có hoặc `content_entries.title` trên DB.
    - **Nhánh non-pattern với `source_surface == "studio"`:** Server: (1) yêu cầu `builder_document: Some(doc)` và chuyển đổi thành Comment-Block `serialize_builder_document_to_blocks(&doc)` (`<= MAX_BUILDER_BLOCK_BODY_BYTES`); (2) nạp trạng thái nền từ **hàng `entry_autosaves` hiện có (nếu có) hoặc từ DB chính (`content_entries`, `entry_terms`, `entry_seo`)** rồi **chạy ngay bộ lọc ACL chỉ giữ khóa `effective_fields` mà `caller_role >= min_edit_role`** (không bao giờ ghi dữ liệu ẩn/read-only từ DB vào `entry_autosaves`); (3) **cập nhật `values_json["body"]`**, **loại bỏ `"body"` khỏi `cleared_fields_json`** (tránh Studio vừa sửa block bị `cleared_fields` cũ từ Single Edit xóa mất!), đặt `content_mode = 'builder'`, `source_surface = 'studio'`, giữ nguyên `title` (nếu `PatchField::Omitted`), các custom fields khác trong `values_json`, `term_ids_json`, `*_touched` và `seo_json` đang có.
    - **Nhánh non-pattern với `source_surface == "editor"`:** Lưu toàn bộ trường editable của Single Edit gồm cả `cleared_fields_json` (đã lọc theo `effective_fields` & `editable`, tối đa `128` phần tử). Sử dụng `PatchField<T>` ([`content/entry_input.rs:26`](../../cms/crates/opendash-core/src/content/entry_input.rs#L26): `Omitted`, `Clear`, `Set(T)`) cho `title`, `primary_category_term_id`, `page_layout_key`, `custom_template_key`, `seo`:
      - Nếu `body` là `!editable`, luôn khóa `entry_autosaves.content_mode = db.content_mode` và lọc bỏ `"body"` khỏi `values`/`cleared_fields`.
      - Khi Single Edit gửi `PatchField::Omitted` cho `primary_category_term_id` / `page_layout_key` / `custom_template_key` / `seo`, đặt `*_touched = 0` và `col = NULL`; khi gửi `PatchField::Clear` hoặc **`PatchField::Set(v)`**, đặt `*_touched = 1` và lưu giá trị tương ứng (`NULL` hoặc `v`).
  - **Không dùng FK tới `taxonomy_terms` hoặc `media_assets`:** Cho phép xóa media/term tự do không bị revision khóa. Không backfill dữ liệu cũ trong migration; dùng **Lazy Baseline Capture**.

---

### 2. Thuật toán Snapshot, Hashing, Baseline Capture, Dedup & Pruning

Tất cả logic SQL và hash đặt trong `cms/crates/opendash-server/src/storage/revisions/`; mọi kiểu/hàm chuẩn hóa JSON thuần nằm trong `cms/crates/opendash-core/src/revisions/` (0 SQL trong `routes/`).

1. **Canonical Snapshot & `content_hash` (`sha2::Sha256`):**
   - Trong `opendash_core::revisions`, định nghĩa `CanonicalRevisionPayload` (`title`, `content_mode`, `values: BTreeMap<String, Value>`, `builder_document_json: Option<String>`, `term_ids: Vec<i64>` đã sort & dedup, `primary_category_term_id`, `page_layout_key`, `custom_template_key`, `seo: Option<CanonicalRevisionSeo>`) cùng `canonical_json_value(&Value) -> Value` và `canonical_revision_json(&CanonicalRevisionPayload) -> String`. Hàm `canonical_json_value` sắp xếp đệ quy khóa object, parse `builder_document_json` qua `canonical_json_value` nếu `Some`, đồng thời chuẩn hóa `-0.0 -> 0` và số thực nguyên dạng `f64` nằm chặt trong biên số nguyên an toàn `-(1i64 << 53) as f64..=(1i64 << 53) as f64` (`1.0 -> 1`, tránh méo bão hòa `i64::MAX` khi ép `as i64`) để tránh lệch `content_hash` giả khi round-trip giữa JS `JSON.stringify` và Rust `serde_json::Value`.
   - **Chuẩn hóa `CanonicalRevisionSeo`:** `SingleEditWorkspace.tsx:605-611` gửi row rỗng `{ title: null, description: null, og_image_id: null, canonical_url: null, noindex: false }` ngay khi bấm Lưu lần đầu, trong khi `create_entry` chưa tạo hàng trong `entry_seo`. Vì vậy định nghĩa `CanonicalRevisionSeo::into_normalized(self) -> Option<CanonicalRevisionSeo>`: trim các chuỗi rỗng về `None`, và nếu cả `title`, `description`, `og_image_id`, `canonical_url` đều `None` đồng thời `noindex == false` thì **chuẩn hóa về `None`**. Áp dụng nhất quán trong `CanonicalRevisionPayload`, `compute_revision_content_hash`, `seo_diff`, và `upsert_autosave` — tránh tạo revision trùng và `seo_diff` giả.
   - `slug`, `publication_status`, `published_at`, `created_at` và `custom_path` **KHÔNG** tham gia `content_hash` và **KHÔNG** bị ghi đè khi Restore (cột `entry_revisions.slug` chỉ ghi tham khảo).
   - Trong `opendash_server::storage::revisions`, hàm `compute_revision_content_hash(payload: &CanonicalRevisionPayload) -> String` gọi `canonical_revision_json(payload)` và băm bằng `sha2::Sha256` (đã pin trong `cms/Cargo.toml` và chỉ dùng ở `opendash-server` theo [`check-standards.py`](../../cms/scripts/check-standards.py#L649-L657)) để sinh chuỗi 64-hex.

2. **Lazy Baseline Capture (`capture_baseline_if_missing_in_tx`):**
   - Tham số bắt buộc là **`conn: &mut sqlx::SqliteConnection`** (dùng chung cho cả `&mut *tx` và `&mut *conn` ở `storage/seo/entry_routes.rs` / `storage/import/read/execute_sample_import_with_cutpoint.rs`).
   - Ngay trong writer transaction của lần sửa đầu tiên cho entry chưa có revision — **trước** khi cập nhật `content_entries` / `entry_builder_documents` / `entry_terms` / `entry_seo` — gọi `capture_baseline_if_missing_in_tx(&mut *conn, entry_id)`:
     - Kiểm tra `SELECT EXISTS(SELECT 1 FROM entry_revisions WHERE entry_id = ?)`.
     - Nếu `0`: đọc trạng thái hiện có trên DB (kèm `author_user_id = NULL`, `author_username = 'baseline'`) và chèn `revision_number = 1, source = 'baseline'`.

3. **Chụp Revision sau khi Cập nhật & Pruning (`capture_entry_revision_in_tx`):**
   - Nhận `conn: &mut sqlx::SqliteConnection`, thực thi **bên trong cùng SQLite writer transaction** sau khi ghi xong:
     1. Đọc trạng thái mới vừa ghi từ DB (gồm cả hậu `filter:content/prepare_save`, `replace_term_ids`, `sync_entry_body_refs_in_conn`, `apply_entry_seo_patch_in_tx`).
     2. Chuẩn hóa & tính `content_hash`. Đọc `(revision_number, content_hash)` mới nhất (`ORDER BY revision_number DESC LIMIT 1`).
     3. **Dedup:** Nếu `latest_hash == Some(content_hash)` → bỏ qua việc chèn revision mới.
     4. Cấp `next_rev = latest_rev.unwrap_or(0) + 1`, chèn bản ghi mới. Với `pattern` (`0044`), ghi `content_mode` đồng bộ với `content_entries.content_mode`.
     5. **Pruning (`MAX_REVISIONS_PER_ENTRY = 25`):**
        ```sql
        DELETE FROM entry_revisions
         WHERE entry_id = ?1
           AND id NOT IN (
               SELECT id FROM entry_revisions
                WHERE entry_id = ?1
                ORDER BY revision_number DESC
                LIMIT 25
           )
        ```

4. **Ma trận Đấu nối Toàn bộ 11 Điểm Ghi `content_entries` & Vòng đời:**

| # | Đường ghi hiện hành | File / Hàm trong `cms/crates/opendash-server/src/` | Hành vi Baseline / Revision | Hành vi `entry_autosaves` |
|---|---|---|---|---|
| 1 | Tạo mới Entry đơn / Fence / Schema Fence | `storage/entries/` (`create_entry*`, `create_entry_with_fence`, `create_entry_with_schema_fence`) | `create_entry_with_fence` khi `fence.is_some()` chụp `source = 'plugin_save'` (tác giả `"plugin:<id>"` hoặc `"theme:<id>"` theo `OwnerFence`); còn lại (gồm `create_entry_with_schema_fence` của HTTP route) chụp `'create'` | Không có (mới tạo) |
| 2 | Cập nhật Entry đơn / Fence / Schema Fence | `storage/entries/` (`update_entry*`, `update_entry_with_fence`, `update_entry_with_schema_fence`) | Baseline (nếu thiếu) → cập nhật → chụp `'save'` (với `update_entry` / `update_entry_with_schema_fence` / `fence.is_none()`) hoặc `'plugin_save'` (với `fence.is_some()`, tác giả theo `OwnerFence`) | Nếu `fence.is_none()` hoặc `*_with_schema_fence`, xóa autosave của `actor`; nếu `fence.is_some()` (plugin/theme `functions.js` chạy nền), **giữ nguyên** autosave của user |
| 3 | Tạo / Cập nhật Composite (Single Edit) | [`storage/entries/body_and_refs/commit_composite_entry_save.rs`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/commit_composite_entry_save.rs#L3) | Baseline (nếu `entry_id.is_some()` & thiếu) → cập nhật `content_entries` + `entry_terms` + `entry_seo` → chụp `'create'` hoặc `'save'` | Nếu `entry_id.is_some()`, xóa autosave của `actor` |
| 4 | Tạo bản dịch (Copy-on-create) | [`commit_entry_translation`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/commit_composite_entry_save.rs#L293) | Chụp `source = 'create'` cho bản dịch mới `new_id` (tác giả `actor`) | Không có |
| 5 | Studio Mode 2 Builder Save / Reset | [`mutate_entry_builder.rs`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/mutate_entry_builder.rs#L3) | Baseline (nếu thiếu) → lưu AST/Comment-Block (`pattern` hoặc entry) → chụp `'builder_save'` hoặc `'builder_reset'` | Xóa autosave của `actor` |
| 6 | Pattern Metadata Create / Update | `storage/patterns.rs::{create_for_actor, update_for_actor}` (giữ wrapper `create`/`update` `actor: None` → `"system"`); [`routes/patterns.rs`](../../cms/crates/opendash-server/src/routes/patterns.rs#L79) truyền `Some(&actor)` | Create chụp `'pattern_save'`; Update chạy Baseline (nếu thiếu) → chụp `'pattern_save'` | **Giữ nguyên** autosave AST Studio Mode 2 của `actor` khi chỉ đổi `title`/`slug` trên `PatternsPage` (giống Quick Edit, tăng `version` để banner cảnh báo `is_stale_base`) |
| 7 | Quick Edit / Bulk Metadata Patch | [`commit_metadata_patch_inner`](../../cms/crates/opendash-server/src/storage/entry_lifecycle/validation_and_fences.rs#L196) | Baseline (nếu thiếu) → cập nhật title/terms/primary_category → chụp `'quick_edit'` (nếu `content_hash` đổi) | **Không bao giờ xóa `entry_autosaves`**: giữ nguyên autosave nháp (banner tự cảnh báo `base_version < current_version`) |
| 8 | Entry SEO Put | [`storage/seo/entry_routes.rs`](../../cms/crates/opendash-server/src/storage/seo/entry_routes.rs#L23) (`put_entry_seo_for_actor`, wrapper `put_entry_seo` `actor: None` → `"system"`); [`routes/seo.rs`](../../cms/crates/opendash-server/src/routes/seo.rs#L292) truyền `Some(&actor)` và **chặn `type_key == "pattern"` (`404`) trên `put_entry_route`, `get_entry_seo` & `put_entry_seo`** (giữ `get_entry_route` trả `200 { public_url: null }` cho Admin View URL preview) | Baseline (nếu thiếu) → cập nhật `entry_seo` & `version` → chụp `'seo_save'` | Giữ nguyên autosave (banner tự báo lệch version nếu đang có) |
| 9 | Theme Sample Content Import | [`execute_sample_import_with_cutpoint.rs`](../../cms/crates/opendash-server/src/storage/import/read/execute_sample_import_with_cutpoint.rs#L3) | Chụp `'import'` (`author_user_id = NULL`, `author_username = 'import'`) cho từng entry mới tạo | Không có |
| 10 | Publication & Custom Route | [`set_publication_status.rs`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/set_publication_status.rs#L4) ([`routes/content.rs:94`](../../cms/crates/opendash-server/src/routes/content.rs#L94)), [`put_entry_route`](../../cms/crates/opendash-server/src/storage/seo/route_validation/is_reserved_route_root.rs#L392) ([`routes/seo.rs:76`](../../cms/crates/opendash-server/src/routes/seo.rs#L76)) | Không tạo revision thừa (vì `publication_status` và `custom_path` không thuộc snapshot revision) | Giữ autosave khi chỉ bật/tắt `draft`/`published` |
| 11 | Trash / Purge / Delete User / Restore Backup | [`apply_lifecycle_in_tx.rs`](../../cms/crates/opendash-server/src/storage/entry_lifecycle/types/apply_lifecycle_in_tx.rs#L10) (`Trash`), `physical_delete_cleanup_in_tx` (`Purge`), `patterns.rs::delete`, [`storage/auth.rs:326`](../../cms/crates/opendash-server/src/storage/auth.rs#L326), [`storage/restore.rs:227`](../../cms/crates/opendash-server/src/storage/restore.rs#L227) | **Trash:** giữ `entry_revisions`. **Purge / Pattern Delete / Locale Orphan Purge:** xóa qua `ON DELETE CASCADE`. **User Delete:** giữ revisions (`author_user_id = NULL`). **Backup Restore:** giữ `entry_revisions` | **Trash:** `DELETE FROM entry_autosaves WHERE entry_id = ?`. **Purge / Pattern Delete / User Delete:** `ON DELETE CASCADE`. **Backup Restore:** `DELETE FROM entry_autosaves` trong `validate_and_rebase_copy` |

- **Ghi chú:** [`migrate_demo_entry_builder_docs_on_startup`](../../cms/crates/opendash-server/src/storage/builder/mutations_and_save.rs#L256) chỉ chuẩn hóa lúc khởi động, không tạo revision.

---

### 3. Pure Rust Diff Engine (`opendash_core::revisions`)

Tuân thủ [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md) §4: `opendash-core` thuần Rust (`serde`, `serde_json`, `url`), **không** thêm crate ngoài mới, **không** I/O.

1. **Giới hạn Tài nguyên & Ngân sách Cộng dồn (Chống DoS / OOM):**
   - `pub const MAX_DIFF_LINES: usize = 2_000;`
   - `pub const MAX_DIFF_TOKENS_PER_LINE: usize = 512;`
   - `pub const MAX_DP_CELLS: usize = 500_000;` (trần tế bào cho Line Diff; dùng thuật toán **Hirschberg `O(min(N, M))` bộ nhớ hoặc bảng traceback giới hạn chặt `<= MAX_DP_CELLS`**)
   - `pub const MAX_TOTAL_WORD_DP_CELLS: usize = 500_000;` (**ngân sách cộng dồn toàn request** cho Word-Diff trên mọi dòng ghép cặp; chặn kịch bản `1_000 dòng × 512×512`)
   - `pub const MAX_BLOCK_DIFF_NODES: usize = 1_024;` (khớp tối đa `2 × MAX_BUILDER_NODES (512)` trong [`builder/types.rs:30`](../../cms/crates/opendash-core/src/builder/types.rs#L30))
   - **Cơ chế Fallback `O(N)`:**
     - Với Line Diff: nếu `lines_a.len().saturating_mul(lines_b.len()) > MAX_DP_CELLS` (hoặc số dòng `> MAX_DIFF_LINES`), cắt chung tiền tố/hậu tố `O(N)` rồi phát hunk tóm tắt kèm `truncated_diff: true`.
     - Với Word Diff trên mỗi cặp dòng `Modified`: nếu `tokens_a.len() > MAX_DIFF_TOKENS_PER_LINE || tokens_b.len() > MAX_DIFF_TOKENS_PER_LINE` hoặc `remaining_word_budget < tokens_a.len().saturating_mul(tokens_b.len())`, đặt `truncated_diff = true` và phát thẳng 2 dòng `Removed` + `Added` không chia word span.
     - Với Block Diff: nếu tổng node hai bên vượt `MAX_BLOCK_DIFF_NODES`, đặt `BlockDiffSummary.truncated = true`, chỉ đối chiếu tối đa `MAX_BLOCK_DIFF_NODES / 2` node đầu mỗi cây và giữ nguyên `body_text_diff` (không bao giờ trả `422` khi xem diff).

2. **Lớp 1 — Word-level & Line-level Text/RichText Diff (`compute_line_word_diff`):**
   - Chuẩn hóa ngắt dòng theo block HTML (`</p>`, `</h1..6>`, `</li>`, `<br>`, `\n`), tách token trên biên **Unicode scalar** (an toàn 100% cho tiếng Việt có dấu) và giữ nguyên mỗi thẻ HTML `<[^>]+>` như một token nguyên tử (không cắt xuyên bên trong thẻ/thuộc tính), xuất `TextDiffResult { truncated_diff, hunks, unchanged_collapsed_count }` với `context_lines = 3`.

3. **Lớp 2 — Block-Aware Structural Diff trên `BuilderDocument` (`compute_builder_block_diff`):**
   - Điều kiện chạy: một trong hai mốc có `content_mode == Builder` hoặc body chứa `<!-- od:` hoặc `type_key == "pattern"`, **VÀ** (`type_key == "pattern"` hoặc `caller_can_view_body == true`). Nếu `caller_role < body.min_view_role`, **cấm** tính hay trả về `body_block_diff` lẫn `body_text_diff` (chỉ bật `has_hidden_admin_changes = true` nếu `body` có khác biệt).
   - Duyệt DFS tạo `FlatBlockSummary` với khóa định danh **`(node_kind, block_type, node_id, occurrence_index)`**, kèm `label`, `parent_id`, `path`, `order_index`, và `props_map: BTreeMap<String, Value>` (gộp `props`, `style`, `responsive`, `meta`, `binding`, **và `values` của [`PluginBlockProps`](../../cms/crates/opendash-core/src/builder/validation/types.rs#L164)**).
   - Một block `moved` đồng thời đổi thuộc tính xuất **1 `BlockDiffItem` duy nhất** với `kind: moved`, `before_path`/`after_path`, `before_order`/`after_order`, và `prop_changes: Vec<BlockPropChange>`. Nếu một bên là `Editor` và bên kia là `Builder`, bật `mode_transition`.

4. **Lớp 3 — Field-by-Field & Metadata Diff (Phân quyền 3 trục & Dựng Snapshot `autosave`):**
   - Lấy toàn bộ trường từ `effective.effective_fields` (**kể cả `Withdrawn` / `ProviderInactive`**).
   - **Quy tắc phân quyền 3 trục** (trên `EffectiveFieldEntry` của [`editor_projection/types.rs`](../../cms/crates/opendash-core/src/content/editor_projection/types.rs), `definition` là `FieldSpec` có `definition.effective_acl() = (min_view_role, min_edit_role, public_exposure)` tại [`field_spec.rs:215-224`](../../cms/crates/opendash-core/src/field_schema/spec/field_spec.rs#L215-L224); quyền sửa dẫn xuất `is_field_editable(f, caller_role) = f.effective_status == EffectiveFieldStatus::Active && f.definition.effective_acl().0 <= caller_role && f.definition.effective_acl().1 <= caller_role` — chặt hơn [`editor_panels.rs:659-668`](../../cms/crates/opendash-core/src/content/roles/editor_panels.rs#L659-L668) ở điều kiện `Active`, đồng bộ với [`SingleEditWorkspace.tsx:1265-1266`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx#L1265-L1266)):
     - `caller_role < f.definition.effective_acl().0`: Ẩn hoàn toàn khóa, nhãn và giá trị (**kể cả ép `builder_document = None` trong `RevisionDetailDto`/`AutosaveSnapshotDto` nếu `body` ẩn**). Chỉ trả `has_hidden_admin_fields: bool` / `has_hidden_admin_changes: bool`.
     - `caller_role >= f.definition.effective_acl().0 && !is_field_editable(f, caller_role)` (ký hiệu `!editable`): Hiển thị trong `fields` (`editable: false`) và `field_diffs`; đưa vào `missing_refs.non_editable_fields` nếu khác DB hiện hành.
     - `field_diffs` loại trừ `"body"`.
   - **Quy tắc Materialize `CompareTarget::Autosave` (`materialize_autosave_payload_in_conn`):** Khi dựng `CanonicalRevisionPayload` từ `entry_autosaves` để chạy `revisions_compare`:
     1. Khởi tạo từ dữ liệu **DB hiện hành** cho mọi trường có `min_edit_role > caller_role` (gồm trường ẩn) và nếu `body` là `!editable`, giữ `(db.values["body"], db.content_mode)` và bỏ qua `body` trong `entry_autosaves`.
     2. Với các trường editable: áp dụng `entry_autosaves.values_json` và xóa mọi key trong `entry_autosaves.cleared_fields_json`.
     3. Với các cột có cờ `*_touched`: chỉ ghi đè khi `*_touched == 1`; nếu `*_touched == 0`, kế thừa giá trị DB hiện hành (`content_entries.{primary_category_term_id, page_layout_key, custom_template_key}` và `entry_seo`).
     4. Trong `autosave_get`: (a) re-filter `values`, `cleared_fields` theo `f.definition.effective_acl().0 <= caller_role`; (b) với `type_key != "pattern"`, nếu caller được xem `body` và `content_mode == Builder` (hoặc `values["body"]` chứa `<!-- od:` / không rỗng ở Studio), **hydrate `builder_document = Some(parse_blocks_lenient_or_recover(body).document)`** giống [`entry_builder_snapshot`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/entry_builder_snapshot.rs) (nếu không xem được `body`, ép `builder_document = None` và bật `has_hidden_admin_fields = true`). Tương tự trong `get_revision_filtered` cho `RevisionDetailDto.builder_document`.

---

### 4. Phát hiện Tham chiếu Thiếu & Dọn An toàn khi Khôi phục (`missing_refs`)

1. **Nguyên tắc Không Ghim Media / Term / Pattern / EntryRef:** [`media_refs.rs::collect_pins_in_tx`](../../cms/crates/opendash-server/src/storage/media_refs.rs#L90) và [`media_is_referenced.rs`](../../cms/crates/opendash-server/src/storage/public_content/taxonomy_queries/media_is_referenced.rs#L6) **tuyệt đối không** truy vấn `entry_revisions` hoặc `entry_autosaves`.

2. **Phạm vi kiểm tra `MissingRefsReport` (`opendash_core::revisions::MissingRefsReport`):**
   - Trong `revision_get`, `revisions_compare`, `autosave_get`: kiểm tra tất cả trường caller có `f.definition.effective_acl().0 <= caller_role`.
   - Trong `revision_restore` (**Pha 1 & Pha 3**): **trước hết áp dụng bước bảo toàn DB cho mọi trường `!editable` (tức `!is_field_editable(f, caller_role)`, gồm `(values["body"], content_mode)` nếu `body` `!editable`)**, rồi **CHỈ chạy kiểm tra `missing_refs.has_blocking_refs()` và `sanitize_snapshot_for_restore` trên tập trường và AST mà caller có quyền sửa (`is_field_editable(f, caller_role)`)**. Nhờ đó Editor không bao giờ bị `409` kẹt vĩnh viễn bởi ref chết trong trường read-only của Admin!
   - Các danh mục kiểm tra (tương thích 100% Unified Field Engine S04 trên cả Custom Fields & `BlockSchemaV1`):
     - **`missing_media`:**
       1. Custom fields (`FieldSpec` kiểu `Media` | `Gallery` | lồng `Group`/`Repeater`, kể cả `values["featured_image"]`) qua [`collect_effective_media_ids`](../../cms/crates/opendash-server/src/storage/media/refs.rs#L23) (`opendash_core::field_schema::collect_media_ids`).
       2. `BuilderDocument` qua [`extract_document_media_refs`](../../cms/crates/opendash-server/src/storage/builder/patterns_and_refs.rs#L3) (cùng kiểm loại `image` / `video` / `any` / **`file_ids`** trong `DocumentMediaRefs`): `section.style.background_media_id`, `KnownElement::Image` (trừ `BindingTarget::MediaId`), `KnownElement::Video.{media_id, poster_image_id}`, và `KnownElement::PluginBlock` qua `entry.descriptor.schema.collect_media_ids(&props.values)` ([`block_schema.rs:136`](../../cms/crates/opendash-core/src/block_schema.rs#L136), [`validate_field_value.rs:193-252`](../../cms/crates/opendash-core/src/field_schema/spec/validate_field_value.rs#L193-L252) duyệt đệ quy `Media`, `Gallery`, `Group`, `Repeater`).
       3. `seo.og_image_id` (`image`).
       → Đối chiếu `media_assets WHERE id = ? AND lifecycle_state = 'active'` (+ `kind` khi là `image`/`video`; `any`/`file` chấp nhận khớp [`patterns_and_refs.rs`](../../cms/crates/opendash-server/src/storage/builder/patterns_and_refs.rs)).
     - **`missing_term_ids`:** kiểm tra `term_ids` + `primary_category_term_id` (đối chiếu `taxonomy_terms` + active taxonomy definitions cho `(type_key, locale)`, kèm `type_key == 'post'` & `kind == 'category'` cho `primary_category_term_id`) **và** các `FieldType::TermRef` bên trong custom fields & `PluginBlock` qua [`collect_relational_refs`](../../cms/crates/opendash-core/src/field_schema/spec/validate_field_value.rs#L346-L420) đối chiếu [`validate_relational_refs_in_conn`](../../cms/crates/opendash-server/src/storage/fields/read/load_term_ref_hydration_map_in_conn.rs#L301-L320) (active taxonomy, **`site_locales.deleted_at IS NULL`**, và kiểm tra `taxonomy_from_field` phụ thuộc field `Select` nguồn nếu nguồn bị thiếu hoặc rỗng tại `#L301-306`).
     - **`missing_entry_ids`:** kiểm tra các `FieldType::EntryRef` bên trong custom fields & `PluginBlock` qua `collect_relational_refs` (`entry_ids`) đối chiếu với [`validate_relational_refs_in_conn`](../../cms/crates/opendash-server/src/storage/fields/read/load_term_ref_hydration_map_in_conn.rs#L262-L270) (`trashed_at IS NULL`, active CPT, **`site_locales.deleted_at IS NULL`**, hợp `target_types`, không phải `pattern`/`section`).
     - **`missing_pattern_ids`:** kiểm tra hợp của `pattern_ids_with_schemas(&doc, &schemas)` (cả `KnownElement::PatternRef` lẫn `PluginBlock` field `SelectOptionsSource::Patterns` qua [`block_schema.rs:144`](../../cms/crates/opendash-core/src/block_schema.rs#L144)) với `content_entries WHERE id = ? AND content_type_key = 'pattern' AND trashed_at IS NULL`.
     - **`missing_contact_form_ids`:** tách rạch ròi theo validator thật: `KnownElement::ContactForm(form_id)` đối chiếu `contact_forms WHERE id = ?` (chấp nhận `status = 'disabled'`, khớp [`storage/contact.rs:150-163`](../../cms/crates/opendash-server/src/storage/contact.rs#L150-L163)); còn `PluginBlock` field `SelectOptionsSource::ContactForms` (duyệt theo phạm vi `Select` + `Group` của [`write.rs:101-154`](../../cms/crates/opendash-server/src/storage/fields/write.rs#L101-L154), tính cả `values.get(spec.key).or(spec.default.as_ref())` tại `#L119`, bỏ qua `Repeater` tại `_ => {}` `#L151`) bắt buộc đối chiếu `contact_forms WHERE id = ? AND status = 'active'` (khớp [`storage/fields/types.rs:200-208`](../../cms/crates/opendash-server/src/storage/fields/types.rs#L200-L208)).
     - **`missing_dynamic_options`:** kiểm tra 6 biến thể `SelectOptionsSource` còn lại trong [`storage/fields/types.rs:156-284`](../../cms/crates/opendash-server/src/storage/fields/types.rs#L156-L284) trên `PluginBlock` đúng theo phạm vi duyệt `FieldType::Select` và `FieldType::Group` của [`write.rs:101-154`](../../cms/crates/opendash-server/src/storage/fields/write.rs#L101-L154) (tính cả `values.get(spec.key).or(spec.default.as_ref())` tại `#L119`, không duyệt `FieldType::Repeater` khớp `_ => {}` tại `#L151`; trừ block `is_preserved` trùng hệt DB hiện hành): `Menus` (`site_menus`), `MenuSlots` (slots khai báo ở active theme manifest), `ContentTypes` (`content_types` active & `!= 'pattern'`), `Taxonomies` (`taxonomy_definitions` active+declared), **`ContextFields`** (**luôn chấp nhận `CORE_CONTEXT_FIELDS = &["title", "slug", "url", "cover_url"]`** khớp [`fields/types.rs:248-260`](../../cms/crates/opendash-server/src/storage/fields/types.rs#L248-L260) & [`list_dynamic_options.rs:134-156`](../../cms/crates/opendash-server/src/storage/fields/read/list_dynamic_options.rs#L134-L156), OR: nhánh `ContentMode::Builder` hoặc `type_key == "pattern"` kiểm trong `effective_fields` theo [`visible_builder_field`](../../cms/crates/opendash-server/src/storage/builder/mutations_and_save.rs#L182-L192) `Active && Public && min_view_role <= caller_role`; nhánh `ContentMode::Editor` comment-blocks kiểm theo [`forbid_generic_pattern_write.rs:78-87`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/forbid_generic_pattern_write.rs#L78-L87) `Active && min_view_role <= caller_role`), và `TemplateParts` (theme manifest `parts` hoặc `template_overrides`).
     - **`missing_list_refs`:** `KnownElement::List`: active CPT `type_key` và tồn tại `category_id` / `tag_id` / `term_id` trong `taxonomy_terms`.
     - **`missing_block_types`:** `KnownElement::PluginBlock(PluginBlockProps { block_id, .. })` không active/declared hoặc `KnownElement::Legacy`, trừ khi element thỏa `opendash_core::builder::is_preserved` so với bản hiện hành ([`builder/document.rs:173`](../../cms/crates/opendash-core/src/builder/document.rs#L173)).
     - **`cleared_binding_nodes`:** kiểm từng `ElementBinding` trên `Heading`/`Text`/`Image`/`Button` khớp đúng 3 biến thể của [`BindingSource`](../../cms/crates/opendash-core/src/builder/validation/types.rs#L308-L314), [`document.rs:211-225`](../../cms/crates/opendash-core/src/builder/document.rs#L211-L225), [`mutations_and_save.rs:182-192`](../../cms/crates/opendash-server/src/storage/builder/mutations_and_save.rs#L182-L192) và [`builderrendercontext.rs:402-509`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/builderrendercontext.rs#L402-L509):
       - `BindingSource::Title`: **không bao giờ** coi là thiếu/cần xóa.
       - `BindingSource::RouteContext { field_key }`: miễn trừ `"title" | "slug" | "url"` (`document.rs:217-223`); với `field_key == "cover_url"`, tìm phần tử đầu tiên `FieldType::Media` (cover/image) `&& PublicExposure::Public` (`f.effective_acl().2 == PublicExposure::Public`) trong `current.schema.base.descriptor.fields` và chỉ gắn cờ nếu tìm thấy mà `!visible_builder_field(&current.schema, &f.key, caller_role)` (`builderrendercontext.rs:472-485`); với mọi custom `field_key` khác, gắn cờ nếu không tồn tại hoặc không thỏa `visible_builder_field(&current.schema, field_key, caller_role)` (`Private`, `ProviderInactive`, rút khỏi schema, hoặc `min_view_role > caller_role`).
       - `BindingSource::Field { key }`: tra cứu effective schema của **`binding.type_key`** (`load_effective_content_schema_in_tx`, hỗ trợ exact-entry bindings trỏ sang CPT khác) và gắn cờ nếu `target_schema.effective_fields.contains_key(key) && !visible_builder_field(target_schema, key, caller_role)` (`builderrendercontext.rs:486-507`, trường rút khỏi `effective_fields` hoặc CPT vắng mặt tiếp tục dùng cơ chế fallback bình thường không bị `403`).
     - **`missing_templates`:** chỉ kiểm tra `page_layout_key` / `custom_template_key` của revision khi **khác với giá trị DB hiện hành** mà không có trong active theme v3 manifest (đối với `custom_template_key`, bắt buộc thỏa thêm `tpl.kind == TemplateCandidateKind::SingleCustom && tpl.allowed_types.contains(type_key)` khớp [`validate_filter_in_tx.rs:273-282`](../../cms/crates/opendash-server/src/storage/entries/mutations_and_status/validate_filter_in_tx.rs#L273-L282)).
     - **`dropped_field_keys` & `dropped_block_field_keys`:** khóa trường top-level đã rút khỏi `effective.effective_fields` hoặc tắt `settings.supports`, cùng khóa field thừa bên trong `PluginBlock.props.values` (gồm lồng `Group` và item `Repeater`) không còn tồn tại trong active `BlockSchemaV1.fields` (và không trùng hệt `previous_values` trên DB, khớp [`validate_values.rs:42-53`](../../cms/crates/opendash-core/src/field_schema/spec/validate_values.rs#L42-L53) / [`validate_non_responsive.rs:375,426`](../../cms/crates/opendash-core/src/field_schema/constants/validate_non_responsive.rs#L375)) — luôn tự dọn gọn khi restore và **không** làm `has_blocking_refs()` trả `true`.
     - **`non_editable_fields`:** khóa `f.definition.effective_acl().0 <= caller_role && !is_field_editable(f, caller_role)` khác giá trị DB (thông báo, không chặn `409`).

3. **Hành vi `has_blocking_refs()` & `sanitize_snapshot_for_restore`:**
   - `MissingRefsReport::has_blocking_refs()` trả `true` khi bất kỳ mục nào trong 10 trường blocking (`missing_media`, `missing_term_ids`, `missing_entry_ids`, `missing_pattern_ids`, `missing_contact_form_ids`, `missing_dynamic_options`, `missing_list_refs`, `missing_block_types`, `cleared_binding_nodes`, `missing_templates`) khác rỗng (`dropped_field_keys`, `dropped_block_field_keys` và `non_editable_fields` KHÔNG chặn).
   - Khi `!acknowledge_missing_refs` và `has_blocking_refs()`: trả `409 Conflict` JSON `{ "code": "revision_missing_refs", "error": "...", "missing_refs": ... }`.
   - Khi `acknowledge_missing_refs == true`: chạy `sanitize_snapshot_for_restore` trên phạm vi editable:
     - Xóa sạch mọi `dropped_field_keys` khỏi `values`, và tước bỏ đệ quy mọi `dropped_block_field_keys` khỏi `PluginBlock.props.values` (gồm `Group` & từng item `Repeater`).
     - Custom field `Image` / `seo.og_image_id` mất → xóa key / `None`; `Gallery` → lọc bỏ ID mất. Nếu một custom field `Image` top-level được phép sửa có `required == true` mất ảnh, validator Phase 1 trả `422` JSON chuẩn (`ContentBeforeSaveVeto`) chỉ rõ `field_errors[key]`.
     - `BuilderDocument`:
       - `section.style.background_media_id` → `None`; `Video.poster_image_id` → `None`.
       - Với `ElementNode.binding` thuộc `cleared_binding_nodes`: đặt `binding = None`, sau đó kiểm lại prop fallback theo [`checks.rs:33-88`](../../cms/crates/opendash-core/src/builder/validation/checks.rs#L33-L88) (`Heading`: `text` rỗng, `Image`: `media_id` rỗng hoặc thuộc `missing_media`, `Button`: `label`/`url` rỗng hoặc URL không hợp lệ theo binding tương ứng) — **nếu fallback prop rỗng/hỏng hoặc media fallback đã chết thì xóa luôn `ElementNode` đó khỏi cột**.
       - `KnownElement::{Image, Video(media_id), PatternRef, ContactForm, List, Legacy}` hoặc `PluginBlock` thuộc `missing_block_types` → **xóa `ElementNode` đó** khỏi cột.
       - `KnownElement::PluginBlock` active (`FieldSpec` trong `BlockSchemaV1`, [`validate_values.rs:83-94`](../../cms/crates/opendash-core/src/field_schema/spec/validate_values.rs#L83-L94), [`validate_single_spec.rs:81-93`](../../cms/crates/opendash-core/src/field_schema/constants/validate_single_spec.rs#L81-L93), [`validate_non_responsive.rs:119-204,333-438`](../../cms/crates/opendash-core/src/field_schema/constants/validate_non_responsive.rs#L119-L204)):
         - Mọi field trong nhóm bị ảnh hưởng (`Media`, `Gallery`, `EntryRef`, `TermRef`, `Repeater`, `Select(options_source)`) **cấm** `responsive: true` theo `validate_single_spec.rs:81-93`, nên không có nhánh map `responsive`.
         - Dọn sạch từng giá trị hỏng: `Media` / đơn `Select(options_source)` → `""` (ghi đè `""` tường minh cả khi `spec.default` lỗi để không tái vấp `#L119`); đơn `EntryRef` / `TermRef` (gồm `TermRef` mất `taxonomy_from_field`) → xóa key; mảng `Gallery` / `EntryRef` / `TermRef` / nhiều `Select(options_source)` → lọc bỏ phần tử hỏng (`options_source` tuân thủ phạm vi `Select` + `Group` của `write.rs:101-154`); đồng thời duyệt đệ quy vào `Group` và từng item `Repeater`.
         - **Quy tắc `un_emptiable` hợp nhất** (khi `visible && !upgrade_exempt && (spec.effective_required() || (spec.field_type == FieldType::Repeater && spec.min_items.is_some_and(|m| m > 0)))`, khớp `validate_values.rs:83-94` & `validate_non_responsive.rs:123-126,177-179,196-199,403-406`):
           - Nếu `!un_emptiable`: với mảng (`Gallery` / `EntryRef` / `TermRef`) có `0 < arr.len() < min_items`, thu về `[]` để tránh lỗi `400`; scalar giữ rỗng / xóa key.
           - Nếu **`un_emptiable`** mà sau khi dọn bị: (i) scalar rỗng `""` hoặc mất key (`Media`, đơn `Select`, đơn `EntryRef`/`TermRef`), (ii) mảng `arr.is_empty()` (`len == 0` với `Gallery`, nhiều `EntryRef`/`TermRef`/`Select`, `Repeater`), hoặc (iii) mảng `arr.len() < min_items` (gồm cả `Repeater` có `min_items > 0` dù `effective_required() == false`): nếu nằm bên trong một item `Repeater` → **loại bỏ item `Repeater` hỏng đó** rồi kiểm tra lại đệ quy lên chính `Repeater` cha theo quy tắc `un_emptiable`; nếu ở top-level `PluginBlock` hoặc thoát khỏi `Group` lồng nhau → **xóa toàn bộ `ElementNode` block hỏng** khỏi cột.
     - `term_ids` & `primary_category_term_id`: lọc `missing_term_ids`; nếu `type_key != "post"` hoặc `primary_category_term_id` không nằm trong `term_ids` sau lọc hoặc không phải `kind = 'category'` → đặt `None` (khớp [`validate_filter_in_tx.rs:309-342`](../../cms/crates/opendash-server/src/storage/entries/mutations_and_status/validate_filter_in_tx.rs#L309-L342)).
     - `page_layout_key` / `custom_template_key` nằm trong `missing_templates`: giữ nguyên giá trị DB hiện hành.

---

### 5. HTTP API Surface, Wire DTOs, Bảo mật & Luồng Restore 4 Pha

Đăng ký module `routes/revisions/` (`mod.rs`, `revisions_handlers.rs`, `autosave_handlers.rs`) trong [`app.rs`](../../cms/crates/opendash-server/src/app.rs). Toàn bộ 7 endpoints dùng chung namespace `/api/admin/content-types/{type_key}/entries/{id}/...` (hỗ trợ cả non-pattern lẫn `type_key == "pattern"` riêng cho `Admin`, khớp nhóm route tại [`app.rs:632-704`](../../cms/crates/opendash-server/src/app.rs#L632-L704)).

**Thứ tự handler, Router Layer & Axum Extractors:**
1. Route tĩnh `/api/admin/content-types/{type_key}/entries/{id}/revisions/compare` **bắt buộc** đăng ký ngay **trước** `/revisions/{rev_id}`.
2. Router gắn `DefaultBodyLimit::max(MAX_CONTENT_ENTRY_JSON_BYTES)` (`768 KiB`) trên `.../autosave` (`PUT`) và `DefaultBodyLimit::max(4096)` trên `.../revisions/{rev_id}/restore` (`POST`).
3. Mọi handler dùng **`Path((type_key, raw_id, raw_rev_id)): Path<(String, String, String)>`** (hoặc 2 tham số) và **`bytes: Bytes`**, gọi **duy nhất 1 lần** `require_session` / `require_session_with_csrf` ở **top-level ngay đầu thân hàm** (pass `direct_csrf_guard` tại [`check-standards.py:228-241`](../../cms/scripts/check-standards.py#L228-L241)) rồi kiểm `if type_key == "pattern" && actor.user.role != Role::Admin { return Err(ApiError::forbidden(...)); }` (tránh query auth DB 2 lần như `require_admin_with_csrf`), và tái sử dụng `routes::entries::expected_version` khi parse `If-Match`.
4. **Định dạng HTTP Error:** Các lỗi `ApiError` (`400`, `401`, `403`, `404`, `412`, `428`, `409 provider_inactive / locale_deleted / locale_parked / permalink_collision / entry_trashed`) trả `text/plain; charset=utf-8` ([`error.rs:62-70`](../../cms/crates/opendash-server/src/error.rs#L62-L70)); `409 revision_missing_refs` và **mọi** `422` (`required_field_missing`, `builder_disabled`, `restore_validation_failed`, hoặc `ContentBeforeSave` hook veto) trả JSON `Response` thống nhất (`Cache-Control: no-store`).

**7 Endpoints (`app.rs` & `routes/revisions/`):**

| Method & Full Path | Handler (`routes::revisions`) | Auth / CSRF / Actor | Limits & Validation | Wire Output |
|---|---|---|---|---|
| `GET /api/admin/content-types/{type_key}/entries/{id}/revisions` | `revisions_list` | `require_session`; `pattern` → `Role::Admin` | `?limit` (`1..=25`, default `25`), `?offset` (`0..=1000`); ngoài biên → `400` | `200 OK` `RevisionsListResponse` + `ETag: "vN"`, `Cache-Control: no-store` |
| `GET /api/admin/content-types/{type_key}/entries/{id}/revisions/compare` | `revisions_compare` | `require_session`; `pattern` → `Role::Admin` | `?from=<target>&to=<target>` với `<target>` ∈ `{ <rev_id >= 1>, current, autosave }` | `200 OK` `RevisionCompareResponse` + `Cache-Control: no-store` |
| `GET /api/admin/content-types/{type_key}/entries/{id}/revisions/{rev_id}` | `revision_get` | `require_session`; `pattern` → `Role::Admin` | `{rev_id}` = `entry_revisions.id` (`i64 >= 1`) | `200 OK` `RevisionDetailDto` + `Cache-Control: no-store` |
| `POST /api/admin/content-types/{type_key}/entries/{id}/revisions/{rev_id}/restore` | `revision_restore` | `require_session_with_csrf`; `pattern` → `Role::Admin` (`403`) | `AdmittedMutation`; body `<= 4096`; `routes::entries::expected_version` (`If-Match: "vN"`, `428`/`412`) | `200 OK` `RestoreRevisionOutcome` (`ETag: "v(N+1)"`, `Cache-Control: no-store`) |
| `GET /api/admin/content-types/{type_key}/entries/{id}/autosave` | `autosave_get` | `require_session`; `pattern` → `Role::Admin` | Read-only | `200 OK` `AutosaveSnapshotResponse` + `Cache-Control: no-store` |
| `PUT /api/admin/content-types/{type_key}/entries/{id}/autosave` | `autosave_put` | `require_session_with_csrf`; `pattern` → `Role::Admin` (`403`) | `AdmittedMutation`; layer limit `768 KiB`; nếu `pattern`: `bytes.len() <= MAX_BUILDER_JSON_BYTES + 16*1024` & `doc <= MAX_BUILDER_JSON_BYTES` (`256 KiB`) | `200 OK` `AutosaveUpsertResponse` + `Cache-Control: no-store` |
| `DELETE /api/admin/content-types/{type_key}/entries/{id}/autosave` | `autosave_delete` | `require_session_with_csrf`; `pattern` → `Role::Admin` (`403`) | `AdmittedMutation` | `204 No Content` (idempotent) + `Cache-Control: no-store` |

**Thứ tự kiểm lỗi thực thi khả thi trên mọi handler:**
0. **Axum Extractor Layer (chạy trước thân hàm handler, khớp `routes/entries/crud.rs`):** `DefaultBodyLimit` (`413 Payload Too Large` khi vượt `768 KiB` / `4096` bytes) và `AdmittedMutation` (`503 Service Unavailable` khi đang bật bảo trì).
1. **Auth & Path-level ACL (`401` → `403`):** `require_session*` (`401`); tiếp theo `type_key == "pattern"` & `!Admin` → `403`, `type_key == "section"` → `403`.
2. **Cú pháp & Headers (`400` / `415` / `428`):** `!valid_content_type_key(&type_key)` → **`400`** (nhất quán với `routes/entries` [`valid_type_key`](../../cms/crates/opendash-server/src/routes/entries/lifecycle_and_translations/is_builtin.rs#L243) & `routes/builder.rs`; ghi chú riêng `routes/seo.rs` & `routes/content.rs` trả `404` cho cú pháp `type_key`); `id`/`rev_id` sai cú pháp hoặc `< 1`, `limit`/`offset`/`from`/`to` sai → `400`; `Content-Type` không phải `application/json` (trên `PUT`/`POST`) → `415`; `serde_json` lỗi cú pháp, `"title": null` (`PatchField::Clear` trên `autosave_put`), hoặc `title` (nếu `PatchField::Set`) có ký tự điều khiển / `> 200` Unicode scalar chars (`length(title)` / `.chars().count()`), `cleared_fields.len() > 128`, `term_ids.len() > 96`, `seo > 4096 bytes`, `base_version < 1`, `client_seq < 1` → `400`; thiếu/sai `If-Match` trên `revision_restore` → `428`.
3. **DB Lookup (`404`):** CPT không tồn tại; entry `(id, content_type_key)` không tồn tại; revision không thuộc `(entry_id, content_type_key)`; hoặc `from`/`to=autosave` mà `(entry_id, user_id)` không có hàng trong `entry_autosaves`.
4. **Kiểm tra trạng thái CPT & Locale (`409` trên write) trước Body-ACL (`403`) và DB-Bounds (`400`):**
   - `provider_inactive` / `locale_deleted` / `locale_parked`: trả **`409`** cho `revision_restore` và `autosave_put` **trước** nhánh kiểm `Builder`/ACL để không báo nhầm `403` khi provider đang tắt (`GET` và `autosave_delete` vẫn cho phép nếu chưa trash).
   - Tiếp theo mới kiểm: trên `autosave_put` khi `type_key != "pattern"` & `source_surface == "studio"` mà `!is_field_editable(body, actor_role)` (không có field `"body"` kiểu `RichText` được quyền sửa hoặc bị khóa `min_view_role`/`min_edit_role`) hoặc CPT tắt `ContentSupport::Builder` → `403`; `base_version > current_version` → `400`; `type_key == "pattern"` gửi `source_surface == "editor"` hoặc kèm custom `values`/`seo`/`term_ids`/doc `> 256 KiB` → `400`.
5. **CAS (`412`):** `If-Match` != `entry.version` trên `revision_restore` (kiểm **trước** `entry_trashed`).
6. **Xung đột Dữ liệu (`409`):** `entry_trashed` (áp dụng trên **cả 7 endpoints**); `permalink_collision` (nếu `sync_claims_in_tx` đụng đường dẫn khi restore `post` đã xuất bản); `revision_missing_refs` (JSON trên `revision_restore`).
7. **Validation / Hook (`422` JSON chuẩn `ContentBeforeSaveVeto`):**
   Mọi lỗi `422` từ `POST .../restore` đều serialize struct [`ContentBeforeSaveVeto`](../../cms/crates/opendash-server/src/hooks/types/definitions.rs#L114) `{ code, message, field_errors: BTreeMap<String, String> }`:
   - Custom field `required = true` bị mất media sau sanitize → `code: "required_field_missing"`, `field_errors[field_key] = field_label`.
   - CPT tắt `ContentSupport::Builder` khi khôi phục `builder` editable → `code: "builder_disabled"`, `field_errors["body"] = ...`.
   - Lỗi schema/giá trị khác từ validation Pha 1 (`validate_against_effective`, `verify_supported_values`, `validate_entry_builder_document`, `render_entry_builder_preview`, giới hạn `MAX_BUILDER_BLOCK_BODY_BYTES`) hoặc concurrency TX → `code: "restore_validation_failed"`, kèm `field_errors` tương ứng.
   - Veto từ hook `ContentBeforeSave` → pass-through `ContentBeforeSaveVeto`.

**Hợp đồng Rust DTO & `serde` Chính xác (`opendash_core::revisions` & `storage::revisions`):**

```rust
// --- opendash_core::revisions (thuần Rust, dùng chung Core/Server) ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionSource {
    Create, Save, BuilderSave, BuilderReset, QuickEdit,
    SeoSave, PatternSave, PluginSave, Restore, Import, Baseline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalRevisionSeo {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_image_id: Option<String>,
    pub canonical_url: Option<String>,
    pub noindex: bool,
} // into_normalized(self) -> Option<Self>

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalRevisionPayload {
    pub title: String,
    pub content_mode: ContentMode,
    pub values: BTreeMap<String, Value>,
    pub builder_document_json: Option<String>,
    pub term_ids: Vec<i64>,
    pub primary_category_term_id: Option<i64>,
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub seo: Option<CanonicalRevisionSeo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOpKind { Equal, Added, Removed, Modified }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordSpan { pub kind: DiffOpKind, pub text: String }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffOpKind,
    pub line_no_before: Option<usize>,
    pub line_no_after: Option<usize>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_spans: Vec<WordSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiffHunk { pub header: String, pub lines: Vec<DiffLine> }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiffResult {
    pub truncated_diff: bool,
    pub hunks: Vec<TextDiffHunk>,
    pub unchanged_collapsed_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockNodeKind { Section, Row, Column, Element }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockDiffKind { Added, Removed, Moved, Modified }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockPropChange { pub prop_path: String, pub before: Value, pub after: Value }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDiffItem {
    pub kind: BlockDiffKind,
    pub node_id: String,
    pub node_kind: BlockNodeKind,
    pub block_type: String,
    pub label: String,
    pub before_path: Option<String>,
    pub after_path: Option<String>,
    pub before_order: Option<usize>,
    pub after_order: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prop_changes: Vec<BlockPropChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDiffSummary {
    pub total_nodes_before: usize,
    pub total_nodes_after: usize,
    pub truncated: bool,
    pub unchanged_count: usize,
    pub items: Vec<BlockDiffItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeTransition { pub from: ContentMode, pub to: ContentMode }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDiffItem {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub editable: bool,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionTermRef { pub id: i64, pub name: String, pub taxonomy_key: Option<String>, pub missing: bool }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyDiffSummary {
    pub added_terms: Vec<RevisionTermRef>,
    pub removed_terms: Vec<RevisionTermRef>,
    pub primary_category_before: Option<RevisionTermRef>,
    pub primary_category_after: Option<RevisionTermRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScalarChange<T> { pub before: T, pub after: T }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionDiffPayload {
    pub title_diff: TextDiffResult,
    pub mode_transition: Option<ModeTransition>,
    pub body_text_diff: Option<TextDiffResult>,
    pub body_block_diff: Option<BlockDiffSummary>,
    pub field_diffs: Vec<FieldDiffItem>,
    pub taxonomy_diff: TaxonomyDiffSummary,
    pub layout_diff: Option<ScalarChange<Option<String>>>,
    pub template_diff: Option<ScalarChange<Option<String>>>,
    pub seo_diff: Option<Vec<FieldDiffItem>>,
    pub has_hidden_admin_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingMediaRef { pub media_id: String, pub location: String, pub expected_kind: String, pub required: bool }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingListRef { pub node_id: String, pub reason: String }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingBlockTypeRef { pub node_id: String, pub block_id: String, pub reason: String }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingTemplateRef { pub kind: String, pub key: String }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingDynamicOptionRef { pub source: String, pub value: String, pub field_path: String }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedBlockFieldKey { pub node_id: String, pub block_type: String, pub field_path: String }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearedBindingNode { pub node_id: String, pub field_key: String }

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingRefsReport {
    pub missing_media: Vec<MissingMediaRef>,
    pub missing_term_ids: Vec<i64>,
    pub missing_entry_ids: Vec<i64>,
    pub missing_pattern_ids: Vec<i64>,
    pub missing_contact_form_ids: Vec<i64>,
    pub missing_dynamic_options: Vec<MissingDynamicOptionRef>,
    pub missing_list_refs: Vec<MissingListRef>,
    pub missing_block_types: Vec<MissingBlockTypeRef>,
    pub cleared_binding_nodes: Vec<ClearedBindingNode>,
    pub missing_templates: Vec<MissingTemplateRef>,
    pub dropped_field_keys: Vec<String>,
    pub dropped_block_field_keys: Vec<DroppedBlockFieldKey>,
    pub non_editable_fields: Vec<String>,
}

// --- storage::revisions (Server DTOs) ---
pub enum RevisionActor<'a> { User(&'a SessionContext), Plugin(&'a str), Theme(&'a str), Import, System }

#[derive(Debug, Clone, Serialize)]
pub struct RevisionSummaryDto {
    pub id: i64, pub revision_number: i64, pub content_hash: String, pub source: RevisionSource,
    pub restored_from_revision_number: Option<i64>, pub author_user_id: Option<i64>,
    pub author_username: String, pub title: String, pub content_mode: ContentMode,
    pub entry_version: i64, pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionsListResponse {
    pub items: Vec<RevisionSummaryDto>, pub total: i64, pub limit: i64, pub offset: i64,
    pub current_version: i64, pub has_autosave: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionFieldView {
    pub key: String, pub label: String, pub field_type: String, pub value: Value, pub editable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionDetailDto {
    pub revision: RevisionSummaryDto, pub current_version: i64, pub slug: String, pub title: String,
    pub content_mode: ContentMode, pub fields: Vec<RevisionFieldView>,
    pub builder_document: Option<BuilderDocument>, pub term_ids: Vec<i64>, pub terms: Vec<RevisionTermRef>,
    pub primary_category_term_id: Option<i64>, pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>, pub seo: Option<CanonicalRevisionSeo>,
    pub has_hidden_admin_fields: bool, pub missing_refs: MissingRefsReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompareTarget { Revision { rev_id: i64 }, Current, Autosave }

#[derive(Debug, Clone, Serialize)]
pub struct CompareTargetMeta {
    pub target: CompareTarget, pub revision_number: Option<i64>,
    pub label: String, pub updated_at: Option<String>, pub entry_version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionCompareResponse {
    pub entry_id: i64, pub content_type_key: String, pub current_version: i64,
    pub from_meta: CompareTargetMeta, pub to_meta: CompareTargetMeta,
    pub diff: RevisionDiffPayload, pub from_missing_refs: MissingRefsReport, pub to_missing_refs: MissingRefsReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutosaveSurface { Editor, Studio }

#[derive(Debug, Clone, Serialize)]
pub struct AutosaveSnapshotDto {
    pub source_surface: AutosaveSurface, pub base_version: i64, pub is_stale_base: bool,
    pub updated_at: String, pub client_seq: i64, pub title: String, pub content_mode: ContentMode,
    pub values: BTreeMap<String, Value>, pub cleared_fields: Vec<String>,
    pub primary_category_touched: bool, pub primary_category_term_id: Option<i64>,
    pub page_layout_touched: bool, pub page_layout_key: Option<String>,
    pub custom_template_touched: bool, pub custom_template_key: Option<String>,
    pub seo_touched: bool, pub seo: Option<CanonicalRevisionSeo>,
    pub builder_document: Option<BuilderDocument>, pub term_ids: Vec<i64>,
    pub has_hidden_admin_fields: bool, pub missing_refs: MissingRefsReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutosaveSnapshotResponse { pub current_version: i64, pub autosave: Option<AutosaveSnapshotDto> }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutosaveUpsertInput {
    pub source_surface: AutosaveSurface,
    pub base_version: i64,
    pub client_seq: i64,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub title: PatchField<String>,
    pub content_mode: Option<ContentMode>,
    pub values: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub cleared_fields: Vec<String>,
    pub builder_document: Option<BuilderDocument>,
    pub term_ids: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub primary_category_term_id: PatchField<i64>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub page_layout_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_template_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub seo: PatchField<SeoMetadataInput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutosaveUpsertResponse {
    pub applied: bool, pub saved_at: String, pub base_version: i64,
    pub current_version: i64, pub is_stale_base: bool, pub client_seq: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionRestoreRequest { #[serde(default)] pub acknowledge_missing_refs: bool }

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RestoreRevisionOutcome {
    Entry {
        entry: ContentEntryRecord, custom_path: Option<String>,
        seo: Option<crate::seo::EntrySeoSnapshot>, schema_token: String, revision: RevisionSummaryDto,
    },
    Pattern {
        entry_id: i64, type_key: String, slug: String, title: String, content_mode: ContentMode,
        version: i64, updated_at: String, document: Option<BuilderDocument>,
        publication_status: String, publication_policy: ContentPublication,
        diagnostics: Vec<opendash_core::builder_blocks::BlockDiagnostic>,
        had_freeform_wrap: bool, had_recovery: bool, revision: RevisionSummaryDto,
    },
}
```

**Luồng Restore 4 Pha (An toàn Writer Lock & Nhất quán Side Effects):**
Định nghĩa internal error enum **`RestoreError { MissingRefs(MissingRefsReport), ValidationVeto(ContentBeforeSaveVeto), Api(ApiError) }`** để ánh xạ thống nhất mọi lỗi `409 missing_refs` và **`422` JSON chuẩn `ContentBeforeSaveVeto`** (bên cạnh các lỗi `401`/`403`/`404`/`409 state`/`412` chuẩn `ApiError`):
- **Pha 1 (Read-only Preflight Transaction):** Thực hiện bước 1–5 trên read tx.
  1. Giữ nguyên mọi trường `!is_field_editable(f, actor_role)` hoặc bị ẩn (`f.definition.effective_acl().0 > actor_role`) từ bản DB hiện hành; với non-pattern, tính **`body_editable = effective.effective_fields.get("body").is_some_and(|f| f.definition.field_type == opendash_core::field_schema::FieldType::Richtext && is_field_editable(f, actor_role))`** (gồm kiểm `f.effective_status == EffectiveFieldStatus::Active` nhất quán với `#L302`, `#L320`, `#L392` và [`mutations_and_save.rs:152-172`](../../cms/crates/opendash-server/src/storage/builder/mutations_and_save.rs#L152-L172); không fallback `true`, các custom CPT không khai báo `"body"` hoặc có `"body"` đang `Withdrawn`/`ProviderInactive` đều cho `false`); nếu `!body_editable`, giữ nguyên cả cặp **`(values["body"], content_mode)`** của DB (không chạy validator builder hay comment-block bằng quyền `actor_role`). Nếu `body_editable` và `target.content_mode == ContentMode::Builder`, kiểm tra CPT có `ContentSupport::Builder` (else `RestoreError::ValidationVeto` `code: "builder_disabled"`, `field_errors["body"]`).
  2. Bảo toàn `slug`, `custom_path`, `publication_status`, `published_at`, `locale`, `translation_group_id`, `created_at` hiện tại trên DB.
  3. Tính `missing_refs` trên phạm vi **trường caller được phép sửa (`is_field_editable(f, actor_role)`)**; nếu `missing_refs.has_blocking_refs() && !acknowledge_missing_refs` → trả `RestoreError::MissingRefs(missing_refs)` (`409 revision_missing_refs`).
  4. Chạy `sanitize_snapshot_for_restore` (nếu `body_editable` & `BuilderDocument` có sửa đổi, re-serialize về `sanitized_values["body"]` qua `serialize_builder_document_to_blocks` và kiểm tra `<= MAX_BUILDER_BLOCK_BODY_BYTES`, nếu vượt giới hạn → `RestoreError::ValidationVeto` `code: "restore_validation_failed"` `field_errors["body"]`).
  5. Validate sạch (và chuẩn hóa toàn bộ lỗi validation 400/422 về `RestoreError::ValidationVeto` — giữ nguyên `403` nếu là lỗi quyền thực sự ngoài phạm vi `cleared_binding_nodes`/`!body_editable`):
     - Với `pattern`: nếu `target.builder_document_json.is_some()`, đọc `current` (`entry_builder_snapshot`) để dựng `is_preserved`, chạy [`validate_entry_builder_document`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/builderrendercontext.rs#L385) + [`render_entry_builder_preview(..., false)`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/render_entry_builder_preview.rs#L3) (ánh xạ `ApiError(400|422, msg)` → `ValidationVeto { code: "restore_validation_failed", message: msg, field_errors: { "body": msg } }`); nếu `None` (`builder_reset`) xác nhận `content_mode == 'editor'`.
     - Với non-pattern: duyệt `effective.effective_fields` thu thập `field_errors` cho các trường `f.effective_required() && f.definition.effective_acl().0 <= actor_role && f.definition.effective_acl().1 <= actor_role && support_allows_field(...) && is_missing` (trả `ValidationVeto { code: "required_field_missing", ... }`), chạy tiếp `ContentEntryInput::validate_against_effective` + **`entries::verify_supported_values(type_key, &sanitized_values, &effective.base.descriptor.settings)`** (**3 tham số** tại [`forbid_generic_pattern_write.rs:15`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/forbid_generic_pattern_write.rs#L15)), **`entries::validate_term_ids_for_locale`** (kiểm giới hạn 32 category / 64 tags mỗi taxonomy sau khi dọn `missing_term_ids`, [`fences_and_terms.rs:111-156`](../../cms/crates/opendash-server/src/storage/entries/fences_and_terms.rs#L111-L156)), kiểm chuẩn `primary_category_term_id` (khớp [`validate_filter_in_tx.rs:309-342`](../../cms/crates/opendash-server/src/storage/entries/mutations_and_status/validate_filter_in_tx.rs#L309-L342)) và **`crate::seo::validate_seo_input`** ([`entry_routes.rs:342`](../../cms/crates/opendash-server/src/storage/seo/entry_routes.rs#L342)), rồi phân nhánh validate body nếu `body_editable`:
       - Nếu `target.content_mode == ContentMode::Builder`: gọi [`validate_entry_builder_document`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/builderrendercontext.rs#L385) **+ [`render_entry_builder_preview(..., false)`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/render_entry_builder_preview.rs#L3)** (khớp [`mutate_entry_builder.rs:145-150`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/mutate_entry_builder.rs#L145-L150) để chặn snapshot vượt giới hạn renderer trước khi ghi DB).
       - Nếu `target.content_mode == ContentMode::Editor && opendash_core::builder_blocks::contains_blocks(&sanitized_body)`: gọi **`entries::validated_entry_block_refs_in_conn`** (`pub(crate)` tại `validated_entry_block_refs_in_conn.rs#L6`) **+ `fields::validate_comment_block_refs_in_conn`** (`pub` tại `fields/write.rs#L229`, khớp [`prepare_composite_entry_save.rs:217-245`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/prepare_composite_entry_save.rs#L217-L245), **không** gọi `entry_builder_snapshot` hay đòi `ContentSupport::Builder`).
       Mọi lỗi 400/422 từ các bước này ánh xạ về `RestoreError::ValidationVeto { code: "restore_validation_failed", message, field_errors }`. Đóng read tx.
- **Pha 2 (Plugin Hook Ngoài Writer Lock):** Chỉ chạy `hooks.dispatch_content_before_save_for_actor(type_key, actor_role, candidate)` ngoài writer lock. *(Quyết định thiết kế: bỏ qua `filter:content/prepare_save` để plugin chuẩn hóa không biến dạng snapshot lịch sử đang khôi phục.)*
- **Pha 3 (Single Reserved SQLite Writer Transaction):** Gọi `entries::begin_entry_write(db, actor)` → re-verify `schema_token`, `If-Match`, `trashed_at IS NULL` và re-compute `missing_refs` dưới writer lock: nếu phát sinh ref thiếu *mới* ngoài tập đã xác nhận ở Pha 1 (hoặc chưa từng xác nhận) → rollback & trả `RestoreError::MissingRefs(missing_refs)` (`409 revision_missing_refs`). Nếu sạch → `capture_baseline_if_missing_in_tx(&mut *tx, entry_id)`:
  - **Nhánh `pattern`:** Cập nhật `content_entries` (`title`, `content_mode`, `version = version + 1`, `updated_at`); nếu `target.builder_document_json` là `Some(doc)` → ghi `entry_builder_documents` + `replace_entry_builder_refs(&mut tx, entry_id, Some(&doc))`; nếu `None` (`builder_reset`) → **`DELETE FROM entry_builder_documents WHERE entry_id = ?`** + `replace_entry_builder_refs(&mut tx, entry_id, None)`, khớp [`mutate_entry_builder.rs`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/mutate_entry_builder.rs).
  - **Nhánh non-pattern:** Đọc `previous_body = current.values.get("body").and_then(Value::as_str).map(str::to_owned)`; cập nhật `content_entries` (`title`, `values_json`, `content_mode`, `page_layout_key`, `custom_template_key`, `primary_category_term_id`, `version = version + 1`, `updated_at`); chạy `DELETE FROM entry_builder_documents WHERE entry_id = ?`; cập nhật `entry_terms` (`entries::replace_term_ids`); **nếu `body_editable` gọi `entries::sync_entry_body_refs_in_conn(&mut *tx, entry_id, &effective, &sanitized_values, previous_body.as_deref(), entries::actor_field_role(actor.user.role))`** (**6 tham số** tại [`forbid_generic_pattern_write.rs:41-48`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/forbid_generic_pattern_write.rs#L41-L48), tự gọi `sync_entry_field_media_refs_in_conn` bên trong; nếu **`!body_editable`** thì bỏ qua `sync_entry_body_refs_in_conn` để tránh lỗi media cũ trên `body` khóa sửa nhưng **vẫn gọi `entries::sync_entry_field_media_refs_in_conn(&mut *tx, entry_id, &effective, &sanitized_values)`** ([`field_media_refs.rs:12`](../../cms/crates/opendash-server/src/storage/entries/body_and_refs/field_media_refs.rs#L12)) để đồng bộ media trên custom fields được phép sửa; tuyệt đối không gọi `replace_entry_builder_refs`); gọi **`permalinks::sync_claims_in_tx(&mut tx)`** ([`claims_sync.rs:32`](../../cms/crates/opendash-server/src/storage/permalinks/claims_sync.rs#L32)); cập nhật SEO qua [`apply_entry_seo_patch_in_tx`](../../cms/crates/opendash-server/src/storage/seo/entry_routes.rs#L296) (**`target.seo == None` ánh xạ sang `PatchField::Clear`**, tự đồng bộ/xóa `seo_public_media_refs` bên trong; nếu phát sinh `ApiError::unprocessable` bất ngờ dưới concurrency TX thì ánh xạ sang `RestoreError::ValidationVeto`). Trước khi trả `RestoreRevisionOutcome::Entry { entry, ... }`, bắt buộc gọi **`entries::filter_entry_values_for_role(&mut record, &effective, entries::actor_field_role(actor.user.role))`** ([`validate_filter_in_tx.rs:79-91`](../../cms/crates/opendash-server/src/storage/entries/mutations_and_status/validate_filter_in_tx.rs#L79-L91)) để không bao giờ rò rỉ trường `min_view_role = Admin` cho `Editor`.
  - **Cả hai nhánh:** `capture_entry_revision_in_tx(&mut *tx, entry_id, RevisionSource::Restore, Some(target.revision_number), RevisionActor::User(actor))` + `clear_actor_autosave_in_tx(&mut *tx, entry_id, actor.user.id)` → `commit()`.
- **Pha 4 (Post-Commit Non-Fatal Side Effects — dưới lease `AdmittedMutation`):** `SiteCacheEpoch::global().bump()`, `recompute_projections_nonfatal(db, hooks, entry_id, was_published)` (`was_published = (publication_status == "published")`), dựng đầy đủ `AsyncHookJob { event: HookEvent::ContentAfterSave, principal: actor_principal, locale: Some(locale), type_key: Some(type_key), payload, dispatch_depth: 0, lease }` (`lease.clone()` khi phát thêm `HookEvent::ContentAfterPublish` với `was_published == true` — phát có chủ đích bất cứ khi nào bài khôi phục đang `published`, khác với [`routes/entries/crud.rs:521`](../../cms/crates/opendash-server/src/routes/entries/crud.rs#L521) chỉ phát lúc chuyển `draft → published`, khớp shape dispatch tại `#L527-561`) và gọi `hooks.enqueue_async(...)`.

---

### 6. Kiến trúc Frontend & Ràng buộc Ratchet LOC (`cms/admin/src/`)

1. **API Layer (`cms/admin/src/api/revisions.ts`, re-export ở `api/index.ts`, cập nhật `client.ts`):**
   - Toàn bộ request gọi tới `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${id}/...` qua [`client.ts`](../../cms/admin/src/api/client.ts); bổ sung `keepalive?: boolean` vào `ApiRequestOptions`.
   - `deleteEntryAutosave` bắt buộc dùng **`requestText`** (`204 No Content`).
   - Trên `ApiClientError` ([`client.ts:120-158`](../../cms/admin/src/api/client.ts#L120-L158)): bổ sung `readonly bodyCode?: string;`, `readonly missingRefs?: MissingRefsReport;`, `readonly fieldErrors?: Record<string, string>;` (giữ `errorCode = res.headers.get('X-OpenDash-Error-Code') ?? bodyCode` tương thích ngược):
     - Parse `missingRefs` **trực tiếp** theo `res.status === 409 && errObj?.code === 'revision_missing_refs' && errObj?.missing_refs && typeof errObj.missing_refs === 'object'` (không phụ thuộc `X-OpenDash-Error-Code` để không bị `"server.error"` che mất).
     - Parse `fieldErrors` **trực tiếp** theo `res.status === 422 && errObj?.field_errors && typeof errObj.field_errors === 'object'` (không phụ thuộc `"hook.before_save_veto"`).
     - Bắt `409 entry_trashed` bằng `err.errorCode === 'entry.trashed' || err.bodyCode === 'entry_trashed' || err.message.includes('entry_trashed')` (khớp `SingleEditWorkspace.tsx:724`).

2. **Luật Ưu tiên RAM `EditorDraftStore` vs Server `entry_autosaves`, `client_seq` & Chuyển Màn hình:**
   - **Ưu tiên dứt khoát sau `401` / `412`:**
     - `hasNewerLocalDraft`: `true` nếu phiên hiện tại giữ RAM draft hợp lệ (`Boolean(restored?.dirty_fields.length)` sau khi `EditorDraftStore.restoreAuthorizedDraft` chạy xong trên Single Edit, hoặc `retainingEntryDraft` trong `StudioWorkspace`). Khi `hasNewerLocalDraft === true`, ẩn nút khôi phục của `AutosaveRecoveryBanner` (chỉ hiện ghi chú *"Đang giữ bản nháp cục bộ mới nhất trên trình duyệt"*) và đặt **`bannerPendingDecision = Boolean(serverAutosave && !hasNewerLocalDraft)`**.
     - Mở rộng `EditorDraftPayload`, `diffEditorPayload` và `restoreAuthorizedDraft` trong [`EditorDraftStore.ts:67-128,281-360`](../../cms/admin/src/features/editor/EditorDraftStore.ts#L67-L128) thêm **`page_layout_key?: string | null`** và **`custom_template_key?: string | null`**. Trong [`SingleEditWorkspace.tsx`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx), bắt buộc: (a) điền `page_layout_key` / `custom_template_key` vào cả `baseline` (`L422-451`) lẫn `saved` (`L623-636`); (b) khôi phục `selectedThemeKey` / `selectedThemeKeyRef` / `themeKeyDirty` / `themeKeyDirtyRef` bên trong `applyPayload` (`L270-284`); và (c) gọi `persistDraftPatch` trong `onChange` của `#entry-theme-selector` (`L819-828`) — giúp thay đổi layout/template đơn lẻ sống sót qua `401`/`412` (không bị `L295` xóa khỏi RAM) và cập nhật đúng `draftRef` / `hasNewerLocalDraft`.
   - **Đồng bộ `sessionEpoch`, `onApiError`, `client_seq` & Chống Race:**
     - Bổ sung `editorDraftStore.subscribe(listener)` và truyền props `siteInstanceId`, `authGeneration` từ [`App.tsx:684-697`](../../cms/admin/src/App.tsx#L684-L697) sang **`<StudioWorkspace />`** (`<SingleEditWorkspace />` đã nhận sẵn 2 props này tại `App.tsx:809-849`). Khi cùng user đăng nhập lại (`authGeneration` đổi & `csrfToken != null`), đồng bộ lại `activeEpoch`.
     - Khi `PUT .../autosave` gặp `401`: gọi trực tiếp **`onApiError(err, ...)`** (vì [`App.tsx:366-371`](../../cms/admin/src/App.tsx#L366-L371) tự gọi `editorDraftStore.quarantineAll()`). Khi gặp `409 entry_trashed`: gọi `onTrashedConflict(err.message)` để khóa editor và dừng timer.
     - Bộ đếm `clientSeq` = `Math.max(Date.now() * 1000, serverLastClientSeq + 1) + (++localSeq)`; nếu `upsert_autosave` trả `{ applied: false }`, không cập nhật `'saved'`.
     - **Chuyển màn hình, hủy In-flight Mutation & `keepalive` không race:**
       - Truyền `enabled: isMode2 && active && ...` vào `useEntryAutosave` của `StudioWorkspace` (tắt hẳn timer + blur + `pagehide` khi `!active` và `abortAndClearInflight()` lúc unmount).
       - Trong [`App.tsx:242-271`](../../cms/admin/src/App.tsx#L242-L271) (`mayLeave`): khi `approved && leavingStudio` (user xác nhận bỏ Studio changes để rời sang route ngoài Mode 2 qua sidebar hoặc Back), đặt `lastStudioRouteRef.current = null`, tăng `studioResetEpoch` (gắn vào `key` hoặc reset prop của `<StudioWorkspace />` để không lưu giữ `entryDoc`/`entryDirty` ở chế độ nền `active={false}`) và gọi `markStudioDirty(false)`.
       - Gọi **`abortAndClearInflight()`** (kèm tăng request epoch) **trước** mọi thao tác: `handleSave` (Single Edit), `handleSaveMode2` (Studio Mode 2), `restoreEntryRevision`, `await trashContentEntry` (sau `window.confirm` trong [`SingleEditWorkspace.tsx:972`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx#L972), tuyệt đối **không** gọi `DELETE .../autosave`, chỉ xóa `serverAutosave = null` sau khi Trash thành công), `builder-prompt-discard` ([`SingleEditWorkspace.tsx:1219-1225`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx#L1219-L1225)), `handleReloadMode2` ([`StudioWorkspace.tsx:995`](../../cms/admin/src/studio/StudioWorkspace.tsx#L995)), và `handleExit` ([`StudioWorkspace.tsx:1024-1037`](../../cms/admin/src/studio/StudioWorkspace.tsx#L1024-L1037)).
       - `StudioWorkspace::handleReloadMode2` (nếu có dirty edits và user xác nhận `window.confirm`) và `StudioWorkspace::handleExit` (nếu `entryDirty`, tự gọi **1 lần** `window.confirm(...)` → Hủy thì `return`): gọi `await abortAndClearInflight()`, sau đó bọc **`await deleteEntryAutosave(...)`** trong `try/catch` với `onApiError` (nếu gặp `401` thì **dừng reload/exit** để `retainingEntryDraft` sống sót qua re-auth, và xử lý `409 entry_trashed` / offline an toàn không văng unhandled rejection), rồi mới nạp lại server hoặc reset `setEntryDoc(entrySavedDoc)` + `onDirtyChange(false)` → `onNavigateHash(target)`. Nếu `handleExit` khi `!entryDirty`, chỉ `abortAndClearInflight()` và giữ nguyên server autosave.
       - `pagehide` listener chỉ gửi `PUT .../autosave` với `keepalive: true` **nếu `enabled && !bannerPendingDecision && payloadBytes <= 56 * 1024`** và **không truyền `AbortController.signal` cleanup**.

3. **Shared Hooks & Tách Module UI tuân thủ Ratchet `loc-baseline.json`:**
   - Shared UI (`cms/admin/src/features/revisions/`): `types.ts`, `useEntryAutosave.ts`, `AutosaveRecoveryBanner.tsx`, `RevisionDiffView.tsx`, `RevisionHistoryModal.tsx`.
   - **4 khối tách mã hiện hữu bắt buộc để không chạm trần `0` headroom của `loc-baseline.json`:**
     - [`PanelRegionRenderer.tsx`](../../cms/admin/src/features/editor/PanelRegionRenderer.tsx) (**754 / 781**): tách toàn bộ nhánh `case 'publish_box':` (`L453-648`, ~195 dòng) sang **`features/editor/PublishBoxPanel.tsx`** (`<= 240 LOC`) kèm `PublishBoxRevisionsControl` (`Revisions: N` + nút mở lịch sử + trạng thái autosave) → `PanelRegionRenderer.tsx` giảm xuống `~580 LOC`.
     - [`SingleEditWorkspace.tsx`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx) (**1354 / 1354**): tách modal `<dialog ref={builderDialogRef} ...>` (`L1201-1261`, ~61 dòng) sang **`features/editor/SingleEditBuilderPromptDialog.tsx`** (`<= 90 LOC`) **và** tách modal dịch đa ngôn ngữ (`L1293-1351`, ~59 dòng) sang **`features/editor/SingleEditTranslationModal.tsx`** (`<= 90 LOC`), đóng gói state/logic vào **`features/revisions/useSingleEditRevisions.ts`** (`<= 280 LOC`) + JSX banner/modal vào **`features/revisions/SingleEditRevisionsOverlay.tsx`** (`<= 120 LOC`) → `SingleEditWorkspace.tsx` giảm xuống `~1260 LOC` (dưới xa trần `1354`).
     - [`App.tsx`](../../cms/admin/src/App.tsx) (**879 / 879**): tách 3 khối JSX `contentContent` / `postsContent` / `pagesContent` (`L801-859`, ~59 dòng) sang **`shell/ContentWorkspaces.tsx`** (`<= 120 LOC`) → `App.tsx` giảm xuống `~830 LOC`, đủ chỗ truyền `siteInstanceId`/`authGeneration`/`studioResetEpoch` & reset `mayLeave`.
     - [`StudioWorkspace.tsx`](../../cms/admin/src/studio/StudioWorkspace.tsx) (**1405 / 1405**): tách **`handleSaveMode2` (`L916-993`, ~78 dòng)** cùng **`handleReloadMode2` + `handleExit` (`L995-1037`, ~43 dòng)** và state Mode 2 sang **`features/revisions/useStudioMode2Revisions.ts`** (`<= 420 LOC`), đồng thời render `<AutosaveRecoveryBanner />` **bên trong** **`features/revisions/StudioMode2RevisionsOverlay.tsx`** (`<= 140 LOC`) → `StudioWorkspace.tsx` giảm sạch xuống `~1320 LOC` (`<= 1350 <= 1405`).

4. **Chi tiết Tích hợp 2 Bề mặt Chính:**
   - **Single Edit:**
     - Nạp song song `getRevisions(typeKey, entryId, { limit: 1 })` (`revisionCount = total`) và `getEntryAutosave(typeKey, entryId)` qua `useSingleEditRevisions`. Kiểm sơ bộ độ dài title bằng Unicode scalar `[...title].length <= 200` (khớp SQLite `length(title)` / Rust `.chars().count()`).
     - Khi bấm **Khôi phục bản tự động lưu**: merge các field `!editable` / `*_touched == 0` từ `baselineRef.current`, **giữ nguyên cả `baselineRef.current.version` LẪN `draftRef.current.version` (`snapshot.version`) ở `current_version`** (tránh [`SingleEditWorkspace.tsx:583`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx#L583) gửi `expected_version` cũ gây `412`), nếu `primary_category_touched === false` mà `primary_category_term_id` không còn trong `term_ids` mới thì ép về `null` (khớp `EditorDraftStore.ts:340`), cập nhật **`selectedThemeKeyRef`, `setSelectedThemeKey`, `themeKeyDirtyRef`, `setThemeKeyDirty` TRƯỚC `persistDraftPatch`**.
     - Khi `handleSave` thành công: xóa `serverAutosave = null`, `hasNewerLocalDraft = false`, cập nhật `revisionCount`.
     - Khi Restore thành công trên `SingleEditWorkspace`: cập nhật `baselineRef`, `draftRef` (`applyPayload`, `clearedFields = []`, và **`res.seo === null` gán thành `emptySeo`** thay vì giữ `snapshot.seo`), `setEntryTimestamps`, `setCustomCreatedAt(null)`, `setCustomUpdatedAt(null)`, `setSelectedThemeKey`, `savedThemeKeyRef.current`, `selectedThemeKeyRef.current`, `themeKeyDirtyRef.current = false`, `setThemeKeyDirty(false)`, `setFieldErrors({})`, `getEntryRoute` làm mới `confirmedPublicLink`, `schemaTokenRef`, `revisionCount`, xóa `editorDraftStore`, tắt `dirty`.
   - **OpenDash Studio Mode 2:**
     - `<StudioMode2RevisionsOverlay />` gắn `<AutosaveRecoveryBanner />` trên đầu `<aside>` sidebar trong Mode 2. Nếu `autosave.source_surface === 'editor'` hoặc `autosave.builder_document === null`, banner báo rõ bản nháp từ Single Edit và ưu tiên nút chuyển sang `#/${entryTypeKey}/${entryId}/edit` (còn nếu `builder_document !== null` cho phép áp dụng AST kèm ghi chú không bao gồm custom fields/SEO).
     - Trong `StudioSidebarHeader.tsx`: thêm `onOpenRevisions` vào `hasOverflowActions` + mục **"Lịch sử phiên bản"** khi `mode === 'entry'`. Trong `StudioSidebarFooter.tsx`: truyền props `autosaveStatus`/`autosaveSavedAt` độc lập với `canSave`.
     - Trong `handleSaveMode2` (chuyển sang `useStudioMode2Revisions.ts` từ [`StudioWorkspace.tsx:916-993`](../../cms/admin/src/studio/StudioWorkspace.tsx#L916-L993)): **CHỈ xóa `serverAutosave = null` & làm mới `revisionCount` khi nhánh `saveEntryBuilder` thực sự chạy** (không xóa `serverAutosave` khi `!entryDirty` chỉ gọi `changeContentEntryPublication` bật `published`, khớp Ma trận §2 dòng 10).
     - Khi `POST .../restore` thành công trong Studio Mode 2:
       - **Non-pattern:** gọi **`await builderApi.getEntryBuilder(entryTypeKey, entryId)`** để đồng bộ `entryDoc`, `entrySavedDoc`, `entryDiagnostics`, `entryHadFreeformWrap`, `entryHadRecovery`, `entryHadExistingDoc = opened.document !== null`, `entryVersion`, `entryTitle` (từ response restore), reset chọn/panel (`conflict = false`, `builderView = 'tree'`, `addTargetParentId = null`, `entrySelectedNodeId = null`), và **`resetHistory()`**. Nếu `restoreResp.entry.content_mode === 'editor'`, giữ ở Studio kèm notice *"Phiên bản này ở chế độ Trình soạn thảo chuẩn (`editor`)"* + CTA `#/${entryTypeKey}/${entryId}/edit` (`entryDirty = false`).
       - **`pattern`:** cập nhật trực tiếp từ response restore (`document ?? EMPTY_DOCUMENT`, `entryHadExistingDoc = document !== null`, `entryTitle`, `entryVersion`, `entryDiagnostics: Vec<BlockDiagnostic>`, `resetHistory()`).

5. **Bản địa hóa Admin UI & API Error Chuẩn Gettext (`cms/l10n/` + `cms/admin/src/i18n/index.tsx`):**
   - Tại `9e17cca` (hậu PR #615), server [`i18n.rs`](../../cms/crates/opendash-server/src/i18n.rs) đã rút gọn còn **`422 LOC`** (không cần tách `i18n_http.rs` hay `i18n_revisions.rs`) và đọc từ điển lỗi từ [`cms/l10n/error_codes.json`](../../cms/l10n/error_codes.json), còn Admin UI dùng trực tiếp `__(msgid, params)` / `_n(singular, plural, count, params)` từ [`cms/admin/src/i18n/index.tsx`](../../cms/admin/src/i18n/index.tsx).
   - Toàn bộ lỗi API mới đăng ký ở `cms/l10n/error_codes.json` + cập nhật [`localize_json_error`](../../cms/crates/opendash-server/src/i18n.rs#L120-L165) cho `"revision_missing_refs"` & `422` veto revisions; toàn bộ chuỗi UI `features/revisions/` viết bằng English `msgid` qua `__()`/`_n()` (0 tiếng Việt hardcode trong `cms/admin/src/`) và dịch đầy đủ trong `cms/l10n/{opendash.pot,vi.po}`, kiểm chứng bởi `cargo test ... i18n::tests` và `node cms/scripts/check-i18n.mjs`.

---

### Alternatives Considered

| Phương án thay thế | Lý do cân nhắc | Lý do bác bỏ |
|---|---|---|
| **1. Gộp chung `entry_revisions` / `entry_autosaves` vào `0056`/`0057` hoặc nhảy số migration** | Giảm số lượng file migration. | `0057_manual_redirects_and_404_log.sql` đã hợp nhất trên `origin/main@ecbea04`; [`cms/migrations/README.md:41`](../../cms/migrations/README.md#L41) và [`restore.rs:157-179`](../../cms/crates/opendash-server/src/storage/restore.rs#L157-L179) bắt buộc đánh số tuần tự nên `0058_entry_revisions_and_autosaves.sql` nối tiếp chuẩn xác (`229 + 6 = 235 <= 256`). |
| **2. Cho Revisions & Autosaves khóa xóa Media (`collect_pins_in_tx`)** | Đảm bảo 100% bản lưu cũ còn nguyên ảnh sau nhiều tháng. | Khóa cứng thư viện Media suốt 25 chu kỳ sửa bài. Phát hiện `missing_refs` + dọn an toàn giải quyết triệt để cả hai phía. |
| **3. Lưu Autosave bằng `localStorage` / IndexedDB** | Không tốn ghi DB server và chạy offline. | Mất nháp khi đổi thiết bị/trình duyệt, dễ rò rỉ dữ liệu private/admin nếu dùng chung máy, và không dùng chung được Rust Diff Engine. |
| **4. Lưu Revision ngay trong `content_entries` (`post_type = 'revision'`)** | Quen thuộc từ WordPress. | Làm ô nhiễm hàng trăm truy vấn `content_entries`, `UNIQUE` constraints và triggers `0008`/`0044`/`0047`. Tách `entry_revisions` và `entry_autosaves` (`+6` → `235 / 256`) an toàn tuyệt đối. |
| **5. Khôi phục cả `slug`, `custom_path` và `publication_status`** | Phản ánh nguyên xi mọi cột cũ. | Có thể âm thầm hạ bài chạy traffic về `draft`, đụng `UNIQUE (content_type_key, locale, slug)` và gãy SEO redirect. |

---

## Non-Functional Requirements

1. **Bảo mật & Phân quyền:**
   - Endpoint kiểm `require_session` / `require_session_with_csrf` ngay top-level handler; `type_key == "pattern"` kiểm `actor.user.role != Role::Admin -> 403`.
   - Nghiêm ngặt 3 trục ACL (`min_view_role > caller_role` ẩn cả key/value/`builder_document` và lọc qua `filter_entry_values_for_role` khi restore; `!editable` giữ nguyên DB, bỏ qua `sync_entry_body_refs_in_conn` khi `!body_editable` nhưng vẫn gọi `sync_entry_field_media_refs_in_conn`, và không chặn `409 missing_refs`).
   - Không endpoint công khai nào đọc `entry_revisions` hay `entry_autosaves`; `Cache-Control: no-store`.
   - Đạt 100% [`check-standards.py`](../../cms/scripts/check-standards.py): `MAX_SOURCE_LOC = 600` (cùng ratchet `loc-baseline.json` cho file cũ, không chạm quá trần `app.rs`/`App.tsx`/`SingleEditWorkspace.tsx`/`PanelRegionRenderer.tsx`/`StudioWorkspace.tsx`), không dùng tên chung chung kiểu `part_*`, 100% SQL trong `storage/`, 0 dep mới trong `opendash-core`, top-level CSRF guard.
2. **Hiệu năng & Giới hạn Tài nguyên:**
   - `MAX_DIFF_LINES = 2_000`, `MAX_DIFF_TOKENS_PER_LINE = 512`, `MAX_DP_CELLS = 500_000`, `MAX_TOTAL_WORD_DP_CELLS = 500_000`, `MAX_BLOCK_DIFF_NODES = 1_024` (`< 20ms` trên `768 KiB`).
   - `POST .../restore` chạy Preflight & `ContentBeforeSave` ngoài `BEGIN IMMEDIATE` writer lock.
3. **Bảo toàn Dữ liệu, Nâng cấp & Rollback:**
   - Migration `0058` additive thuần túy (`+6` → `235 / 256`), tương thích với `restore::validate_restored_db_read_only` / `validate_and_rebase_copy`.

---

## Detailed Implementation Plan

### Slice S01: Migration `0058`, Core Diff Engine (`opendash_core::revisions/`), Storage Persistence & Test Runner

- **Dependencies:** Baseline `origin/main@ecbea04` (đã có `0057_manual_redirects_and_404_log.sql`, Gettext `.pot`/`.po`/`.mo`, `field_schema::{collect_media_ids, collect_relational_refs}` & `sync_entry_field_media_refs_in_conn`).
- **File Ownership:**
  - `cms/migrations/0058_entry_revisions_and_autosaves.sql` (new)
  - `cms/migrations/README.md` (modify: cập nhật tổng `57` file `0001`–`0058` và mô tả `entry_revisions` / `entry_autosaves`)
  - `cms/crates/opendash-core/src/lib.rs` (modify: `pub mod revisions;`)
  - `cms/crates/opendash-core/src/builder/document.rs` (modify: mở `pub fn is_preserved`, đã re-export qua `builder::*`)
  - `cms/crates/opendash-core/src/revisions/{mod.rs,types.rs,canonical.rs,text_diff.rs,block_diff.rs,tests.rs}` (new feature-first submodules, mỗi file `<= 600 LOC`)
  - `cms/crates/opendash-server/src/storage/mod.rs` (modify: `pub mod revisions;`)
  - `cms/crates/opendash-server/src/storage/revisions/{mod.rs,types.rs,capture.rs}` (new feature-first submodules, mỗi file `<= 600 LOC`, gồm helper `record_entry_revision_and_clear_autosave_in_tx` & `capture_import_revisions_in_conn` + unit test nội bộ chạy qua `--bin opendash storage::revisions`)
  - `cms/crates/opendash-server/src/storage/entries/body_and_refs/{commit_composite_entry_save.rs,forbid_generic_pattern_write.rs}` (modify: đấu nối baseline/capture/clear autosave vào `commit_composite_entry_save`, `commit_entry_translation`, `create_entry_with_*`, `update_entry_with_*`; vì `fences_and_terms.rs:252-282` đã ủy quyền thẳng sang `forbid_generic_pattern_write.rs` (`508 / 600 LOC` trên `9e17cca`), không cần sửa `fences_and_terms.rs`, và dùng 1-call helper `record_entry_revision_and_clear_autosave_in_tx` cho 4 điểm write để giữ `<= 600 LOC`)
  - `cms/crates/opendash-server/src/storage/builder/read_and_preview/mutate_entry_builder.rs` (modify: đấu nối `'builder_save'`/`'builder_reset'` & clear autosave)
  - `cms/crates/opendash-server/src/storage/patterns.rs` & `cms/crates/opendash-server/src/routes/patterns.rs` (modify: `create_for_actor`, `update_for_actor` giữ autosave khi đổi metadata, wrapper `actor: None` → `"system"`, routes truyền `Some(&actor)`)
  - `cms/crates/opendash-server/src/storage/entry_lifecycle/` (`validation_and_fences.rs`, `types/apply_lifecycle_in_tx.rs` — modify: `commit_metadata_patch_inner` chụp `'quick_edit'` và giữ autosave; `Trash` xóa toàn bộ `entry_autosaves`)
  - `cms/crates/opendash-server/src/storage/seo/entry_routes.rs` & `cms/crates/opendash-server/src/routes/seo.rs` (modify: `put_entry_seo_for_actor`, wrapper cũ `actor: None`, routes truyền `Some(&actor)` và chặn `type_key == "pattern"` trả `404` trên `put_entry_route`, `get_entry_seo` & `put_entry_seo`, giữ `get_entry_route` trả `200` với `public_url: null`)
  - `cms/crates/opendash-server/src/storage/import/read/execute_sample_import_with_cutpoint.rs` (modify: `576 / 600 LOC` — chèn tối đa 3 dòng gọi `capture_import_revisions_in_conn` chụp `'import'` để giữ `<= 600 LOC`)
  - `cms/crates/opendash-server/src/storage/restore.rs` (modify: `DELETE FROM entry_autosaves` trong `validate_and_rebase_copy`)
  - `cms/scripts/run-tests.py` (modify trong S01: thêm `"storage::revisions"` vào `unit_filters` của `"content"` & `"builder"` trong `GROUPS` tuple; đặt nhánh prefix **trước** `filename in core_routes` / `sub in server_routes`: `cms/crates/opendash-core/src/revisions/` → `{"builder", "content"}`, `storage/revisions/` → `{"builder", "content"}`; cập nhật **`keyword_map` thêm `"revision"`, `"autosave"` vào CẢ hai tuple `"content"` lẫn `"builder"`** và thêm test case vào `RunTestsSelfTest`)
  - `cms/crates/opendash-server/tests/{content.rs,content/revisions_storage.rs}` (new domain test submodule `<= 600 LOC`: spawn `CARGO_BIN_EXE_opendash`, kiểm chứng HTTP & `SqlitePool` trên `entry_revisions` / `entry_autosaves`, còn unit test trực tiếp `storage::revisions` nằm trong `src/storage/revisions/` và chạy bằng `--bin opendash storage::revisions`)
- **Acceptance Criteria:**
  - [ ] Migration `0058` chạy sạch trên DB trống và DB có dữ liệu sẵn; `SELECT COUNT(*) FROM sqlite_schema` thực tế `<= 256` (`235`).
  - [ ] Mọi file mới trong `revisions/` và các file đã chỉnh (`forbid_generic_pattern_write.rs` từ `508 LOC`, `execute_sample_import_with_cutpoint.rs` từ `576 LOC`) có `<= 600 LOC`, không thêm dependency mới; `opendash_core::revisions` và `storage::revisions` PASS 100%.
  - [ ] Đủ 11 điểm Ma trận §2 tự động tạo baseline, dedup `content_hash`, prune giữ `25` bản, và dọn `entry_autosaves` đúng quy tắc.
  - [ ] `media_refs` và `public_content::media_is_referenced` không ghim và không cấp ACL công khai cho `entry_revisions` / `entry_autosaves`.
  - [ ] `restore::validate_restored_db_read_only`, `validate_and_rebase_copy`, `run-tests.py --list`, `run-tests.py --self-test` và `content::revisions_storage` PASS 100%.
- **Rollback:** Revert commit S01; `0058` chỉ thêm 2 bảng mới.

### Slice S02: Restore Preflight, Missing-Ref Sanitization, HTTP Revision & Autosave APIs

- **Dependencies:** Slice S01 hoàn tất.
- **File Ownership:**
  - `cms/crates/opendash-server/src/storage/revisions/{mod.rs,types.rs,read_and_diff.rs,missing_refs.rs,restore.rs,autosave.rs}` (create/modify submodules `<= 600 LOC`: `list_revisions`, `get_revision_filtered`, `compare_revisions`, `materialize_autosave_payload_in_conn`, `prepare_revision_restore`, `commit_revision_restore`, `filter_entry_values_for_role`, `get_autosave`, `upsert_autosave`, `delete_autosave`, `detect_missing_refs_in_conn`, `sanitize_snapshot_for_restore`; gọi `entries::validated_entry_block_refs_in_conn`, `fields::validate_comment_block_refs_in_conn`, và `entries::sync_entry_field_media_refs_in_conn`)
  - `cms/l10n/{error_codes.json,opendash.pot,vi.po}` & `cms/crates/opendash-server/src/i18n.rs` (modify: nạp trọn chuỗi lỗi routes/restore/autosave vào `error_codes.json`/`.pot`/`.po` & hỗ trợ `"revision_missing_refs"` / `"required_field_missing"` / `"builder_disabled"` / `"restore_validation_failed"` trong `localize_json_error`)
  - `cms/crates/opendash-server/src/routes/mod.rs` (modify: `pub mod revisions;`)
  - `cms/crates/opendash-server/src/routes/revisions/{mod.rs,revisions_handlers.rs,autosave_handlers.rs}` (new submodules `<= 600 LOC`, **`routes/revisions/mod.rs` bắt buộc dùng glob re-export `pub use revisions_handlers::*; pub use autosave_handlers::*;`** khớp [`check-standards.py:199`](../../cms/scripts/check-standards.py#L199) để kiểm chứng `mutation_handlers`, kèm unit test handler nội bộ chạy qua `--bin opendash routes::revisions`)
  - `cms/crates/opendash-server/src/{main.rs,http_middleware.rs,app.rs}` (`opendash-server` là crate thuần binary chỉ có `main.rs`, không có `lib.rs`; khai báo `mod http_middleware;` trong `main.rs`, tách riêng **2 khối non-route middleware `app.rs:782-864` (Host/Origin) và `app.rs:867-880` (SafeMode)** sang `http_middleware.rs <= 140 LOC` **giữ nguyên `.layer(localize_error_response)` và `.fallback(public_fallback)` tại `app.rs`**, kèm re-export `pub(crate) use crate::http_middleware::HostOriginRejected;` trong `app.rs` cho `i18n.rs`, đăng ký 7 routes `/api/admin/content-types/{type_key}/entries/{id}/...` giữ `app.rs <= 883` LOC theo `loc-baseline.json`)
  - `cms/scripts/run-tests.py` (modify: thêm `"routes::revisions"` vào `unit_filters` của `"content"` & `"builder"`; đặt nhánh prefix `cms/crates/opendash-server/src/routes/revisions/` → `{"builder", "content", "ops"}` **trước** `sub in server_routes`; xử lý tường minh `if sub == "http_middleware.rs": return set(ALL_GROUP_NAMES), set(), True, False` **trước/ngoài** `server_routes` để giữ **`escalate_all = True`** chạy toàn workspace và bổ sung kiểm chứng trong `RunTestsSelfTest`)
  - `cms/crates/opendash-server/tests/{content.rs,builder.rs,ops/writer_barrier.rs,content/revisions_api.rs,builder/revisions_builder.rs}` (new integration test submodules `<= 600 LOC` + bổ sung `("revisions.rs", &["revision_restore", "autosave_put", "autosave_delete"])` vào `required_handlers` trong `writer_barrier.rs`)
- **Acceptance Criteria:**
  - [ ] 7 HTTP endpoints dùng `AdmittedMutation` + `DefaultBodyLimit`, đặt top-level `require_session` / `require_session_with_csrf` + kiểm `type_key == "pattern" && !Admin -> 403`, giữ thứ tự lỗi thực thi §5 (`413 → 401/403 path → 400 syntax/type_key / "title": null / 415 / 428 → 404 DB → 409 state → 403 studio/ACL / 400 DB → 412 → 409 data → 422`), trả `ContentBeforeSaveVeto` JSON thống nhất cho mọi `422`; `writer_barrier`, `i18n::tests` và `check-standards.py` (`app.rs <= 883`, mọi file mới `<= 600 LOC`) PASS 100%.
  - [ ] `min_view_role > caller_role` ẩn hoàn toàn (kể cả `builder_document = None` nếu `body` ẩn và lọc `entries::filter_entry_values_for_role(..., entries::actor_field_role(actor.user.role))` khi trả `RestoreRevisionOutcome::Entry`); non-pattern `builder_document` được hydrate qua `parse_blocks_lenient_or_recover` khi xem được; `!editable` (`!is_field_editable`) giữ nguyên DB, bỏ qua `sync_entry_body_refs_in_conn` nhưng vẫn gọi `sync_entry_field_media_refs_in_conn` khi `!body_editable` và bỏ qua khỏi blocking `missing_refs`.
  - [ ] `POST .../restore` giữ URL/status/locale/created_at, kiểm đủ 8 `SelectOptionsSource` (tuân thủ `Select` + `Group` của `write.rs:101-154`, chấp nhận `CORE_CONTEXT_FIELDS` & phân biệt `ContextFields` theo `Builder`/`Editor`, kèm `site_locales.deleted_at IS NULL` & `TermRef.taxonomy_from_field`), unknown `BlockSchemaV1` keys, quy tắc `un_emptiable` (`Repeater` `min_items > 0` + scalar/array required), `ElementBinding` (`Title`, `RouteContext`, `Field` trên `binding.type_key` theo `visible_builder_field` + kiểm fallback `checks.rs`), `allowed_types` của `SingleCustom` template; Phase 1 phân nhánh `ContentMode::Builder` (`validate_entry_builder_document` + `render_entry_builder_preview(..., false)`) vs `ContentMode::Editor && contains_blocks` (`validated_entry_block_refs_in_conn` + `validate_comment_block_refs_in_conn`), gọi `validate_term_ids_for_locale` + `seo::validate_seo_input` + `verify_supported_values` (3 arg), Phase 3 gọi `sync_entry_body_refs_in_conn` (6 arg) hoặc `sync_entry_field_media_refs_in_conn`, `replace_entry_builder_refs` (`pattern`), `permalinks::sync_claims_in_tx`, `apply_entry_seo_patch_in_tx` (`PatchField::Clear`), và re-verify ref mới ở Pha 3.
  - [ ] `PUT .../autosave` xử lý `title = ""` (từ chối `"title": null -> 400`), `PatchField::{Omitted,Clear,Set}` / `*_touched` / `cleared_fields_json` (xóa `"body"` khi `studio`), nhánh riêng `pattern` (envelope `256 KiB`), `client_seq` (`applied: bool`), và `materialize_autosave_payload_in_conn` cho `compare`; `DELETE .../autosave` idempotent `204`.
  - [ ] `content::revisions_api`, `builder::revisions_builder`, `--bin opendash routes::revisions`, `ops::writer_barrier`, `run-tests.py --list`, `run-tests.py --self-test` PASS 100%.
- **Rollback:** Gỡ route trong `app.rs` & `routes/revisions/`.

### Slice S03: Admin Single Edit Integration & Gettext Localization (`cms/l10n/`)

- **Dependencies:** Slice S02 hoàn tất.
- **File Ownership:**
  - `cms/admin/src/api/client.ts` (modify: `keepalive?: boolean`, thêm `bodyCode?: string`, parse trực tiếp `missingRefs` trên `409 revision_missing_refs` & `fieldErrors` trên `422` bất kể `X-OpenDash-Error-Code`)
  - `cms/admin/src/api/revisions.ts` (new, `<= 600 LOC`)
  - `cms/admin/src/api/index.ts` (modify: re-export `./revisions.ts`)
  - `cms/admin/src/features/editor/EditorDraftStore.ts` (modify: thêm `subscribe(listener)` và `page_layout_key`/`custom_template_key` vào `EditorDraftPayload`, `diffEditorPayload`, `restoreAuthorizedDraft`)
  - `cms/admin/src/features/editor/PublishBoxPanel.tsx` (new `<= 240 LOC`: tách `case 'publish_box'` khỏi `PanelRegionRenderer.tsx:453-648` kèm `PublishBoxRevisionsControl`)
  - `cms/admin/src/features/editor/SingleEditBuilderPromptDialog.tsx` (new `<= 90 LOC`: tách `<dialog ref={builderDialogRef} ...>` khỏi `SingleEditWorkspace.tsx:1201-1261`)
  - `cms/admin/src/features/editor/SingleEditTranslationModal.tsx` (new `<= 90 LOC`: tách modal dịch đa ngôn ngữ khỏi `SingleEditWorkspace.tsx:1293-1351` tạo đệm LOC an toàn)
  - `cms/admin/src/features/revisions/{types.ts,useEntryAutosave.ts,AutosaveRecoveryBanner.tsx,RevisionDiffView.tsx,RevisionHistoryModal.tsx,PublishBoxRevisionsControl.tsx,useSingleEditRevisions.ts,SingleEditRevisionsOverlay.tsx}` (new submodules, mỗi file `<= 600 LOC`, dùng `__()`/`_n()` từ `cms/admin/src/i18n/index.tsx`)
  - `cms/l10n/{opendash.pot,vi.po}` (modify: trích xuất và dịch đầy đủ `msgid` tiếng Việt cho UI Shared/Single Edit revisions)
  - `cms/admin/src/features/editor/{SingleEditWorkspace.tsx,PanelRegionRenderer.tsx}` (modify: dùng `PublishBoxPanel.tsx`, `SingleEditBuilderPromptDialog.tsx` & `SingleEditTranslationModal.tsx`, gắn `useSingleEditRevisions` & `SingleEditRevisionsOverlay`, điền `page_layout_key`/`custom_template_key` ở `baseline` (`L422-451`) & `saved` (`L623-636`), đồng bộ `selectedThemeKey`/`Ref`/`Dirty` trong `applyPayload` (`L270-284`) và gọi `persistDraftPatch` trong `#entry-theme-selector` `onChange` (`L819-828`), gọi `abortAndClearInflight()` trước `handleSave`/`restore`/`handleDelete` (sau confirm) & `builder-prompt-discard`, giữ cả `baselineRef.current.version` & `draftRef.current.version`, ép `primary_category_term_id = null` nếu không còn trong `term_ids` khi `touched == false`, đồng bộ `selectedThemeKeyRef`/`themeKeyDirtyRef` trước `persistDraftPatch`, set `emptySeo` khi `res.seo === null`, giữ `SingleEditWorkspace.tsx <= 1354` (`~1260 LOC`) & `PanelRegionRenderer.tsx <= 781` LOC)
- **Acceptance Criteria:**
  - [ ] `useEntryAutosave` debounce 15s (max-wait 60s), `AbortController` + `abortAndClearInflight()` trước mọi save/restore/trash/discard, `keepalive` ở `pagehide` khi `enabled && !bannerPendingDecision && payloadBytes <= 56 * 1024`, gọi `onApiError` trên `401` (không gọi đúp `quarantineAll()`), cập nhật epoch khi re-login, bắt `409 entry.trashed` / `entry_trashed`.
  - [ ] `EditorDraftStore` RAM draft sau `401` (gồm cả sửa đơn lẻ `page_layout_key`/`custom_template_key` qua `onChange`/`applyPayload`) ưu tiên tuyệt đối (`bannerPendingDecision = Boolean(serverAutosave && !hasNewerLocalDraft)`).
  - [ ] `handleDelete` không gọi `DELETE .../autosave`; khi Save thành công, xóa `serverAutosave` / `hasNewerLocalDraft` & cập nhật `revisionCount`; sau Restore mọi state phụ đồng bộ sạch.
  - [ ] `check-standards.py` (ratchet LOC), `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, và `node cms/scripts/check-i18n.mjs` PASS 100%.
- **Rollback:** Gỡ tích hợp trong `SingleEditWorkspace.tsx` & `PanelRegionRenderer.tsx`.

### Slice S04: Studio Mode 2 Integration, `App.tsx`, Studio Gettext i18n, M8 Inventory & Verification

- **Dependencies:** Slice S03 hoàn tất.
- **File Ownership:**
  - `cms/admin/src/shell/ContentWorkspaces.tsx` (new `<= 120 LOC`: tách 3 wrapper `contentContent` / `postsContent` / `pagesContent` khỏi `App.tsx:801-859`)
  - `cms/admin/src/App.tsx` (modify: dùng `ContentWorkspaces.tsx` giữ `<= 879 LOC`; truyền `siteInstanceId`, `authGeneration`, `studioResetEpoch` vào `<StudioWorkspace />`; khi `mayLeave` duyệt rời `leavingStudio`, xóa `lastStudioRouteRef.current`, tăng `studioResetEpoch` và gọi `markStudioDirty(false)`)
  - `cms/admin/src/features/revisions/{useStudioMode2Revisions.ts,StudioMode2RevisionsOverlay.tsx}` (new `<= 600 LOC`: **`useStudioMode2Revisions.ts` (`<= 420 LOC`)** tách cả `handleSaveMode2` (`StudioWorkspace.tsx:916-993`), `handleReloadMode2` + `handleExit` (`L995-1037`), bật `useEntryAutosave` `enabled: isMode2 && active`, bọc `deleteEntryAutosave` trong `try/catch` + `onApiError` (hủy reload/exit khi `401`) và **`StudioMode2RevisionsOverlay.tsx` (`<= 140 LOC`)** render `<AutosaveRecoveryBanner />` + `<RevisionHistoryModal />`, giữ `StudioWorkspace.tsx <= 1350 <= 1405` LOC)
  - `cms/admin/src/studio/StudioWorkspace.tsx` (modify: gắn `useStudioMode2Revisions` & `StudioMode2RevisionsOverlay` trên đầu `<aside>`, `abortAndClearInflight()` trước save/reload/exit/restore, trong `handleSaveMode2` chỉ xóa `serverAutosave` khi `saveEntryBuilder` thực sự chạy, gọi `builderApi.getEntryBuilder` sau non-pattern restore + `resetHistory()`)
  - `cms/admin/src/studio/StudioSidebarHeader.tsx` (modify: thêm `onOpenRevisions` vào `hasOverflowActions` + mục **"Lịch sử phiên bản"** khi `mode === 'entry'`)
  - `cms/admin/src/studio/StudioSidebarFooter.tsx` (modify: hiển thị trạng thái Autosave độc lập với `canSave` khi ở Mode 2)
  - `cms/l10n/{opendash.pot,vi.po}` (modify: bổ sung bản dịch Gettext cho chuỗi Studio Mode 2 & kiểm chứng biên dịch `.mo`)
  - `docs/contracts/M8_WRITER_INVENTORY.md` (modify: ghi nhận 3 mutators `routes::revisions::*` tại §2 và phân loại bảng/ngoại lệ Trash/Purge/Restore tại §3)
  - `cms/scripts/revisions_autosave_e2e.py` (new: Playwright E2E cho CUJ 1–6 trên macOS/Linux/Windows)
- **Acceptance Criteria:**
  - [ ] Studio Mode 2 hỗ trợ đầy đủ Autosave, Recovery Banner, Block-Aware Diff tab và Restore cho cả Landing Page lẫn `pattern` CPT (bao gồm `builder_reset` về `EMPTY_DOCUMENT`), rời Mode 2 qua sidebar/Back không kẹt `studioDirty` nền, hủy reload/exit an toàn khi `DELETE .../autosave` gặp `401`, không đụng Mode 1 (`customizer`); `PatternsPage.tsx` giữ nguyên lối vào `#/studio/entry/pattern/:id`.
  - [ ] `check-standards.py` (0 vượt `600 LOC` & ratchet `loc-baseline.json`), `typecheck`, `build`, `check-i18n.mjs` PASS 100%.
  - [ ] `M8_WRITER_INVENTORY.md` §2–§3 đồng bộ đầy đủ mutators và vòng đời `storage::revisions`.
  - [ ] Toàn bộ test Rust (`--group all --release`) và Playwright E2E (`revisions_autosave_e2e.py`) PASS 100%.
- **Rollback:** Gỡ tích hợp trong `App.tsx`, `StudioWorkspace.tsx`, `StudioSidebar*`.

---

### Target Files to Create/Modify

*(Danh sách khớp 1-1 với `File Ownership` của Slice S01–S04; mọi file nguồn mới `<= 600 LOC`, không đặt tên `part_*`):*

1. **Database Migration:**
   - `cms/migrations/0058_entry_revisions_and_autosaves.sql` — DDL tạo `entry_revisions`, `entry_autosaves` (`cleared_fields_json`, `*_touched`, `client_seq`) và 2 secondary indexes (`+6` `sqlite_schema` objects).
   - `cms/migrations/README.md` — Cập nhật số lượng migration (`57` files `0001`–`0058`) và mô tả 2 bảng mới.
2. **Core Crate (`cms/crates/opendash-core/src/`):**
   - `lib.rs` — Khai báo `pub mod revisions;`.
   - `builder/document.rs` — Mở `pub fn is_preserved(...) -> bool` (đã re-export qua `builder::*`).
   - `revisions/{mod.rs,types.rs,canonical.rs,text_diff.rs,block_diff.rs,tests.rs}` — Hằng số `MAX_REVISIONS_PER_ENTRY = 25`, `MAX_DIFF_LINES = 2_000`, `MAX_DIFF_TOKENS_PER_LINE = 512`, `MAX_DP_CELLS = 500_000`, `MAX_TOTAL_WORD_DP_CELLS = 500_000`, `MAX_BLOCK_DIFF_NODES = 1_024`; toàn bộ structs/enums `#[derive(Serialize, Deserialize)]` (§5); hàm `canonical_json_value(&Value) -> Value` (chuẩn hóa `builder_document_json`, `-0.0 -> 0` và số nguyên `f64` trong `[-(2^53), 2^53]`), `canonical_revision_json(&CanonicalRevisionPayload) -> String`, `compute_line_word_diff(before: &str, after: &str) -> TextDiffResult`, `compute_builder_block_diff(before: Option<&BuilderDocument>, after: Option<&BuilderDocument>) -> BlockDiffSummary`.
3. **Server Crate (`cms/crates/opendash-server/src/`):**
   - `main.rs` — Khai báo `mod http_middleware;`.
   - `i18n.rs` — Cập nhật `localize_json_error` cho `"revision_missing_refs"` & `422` veto revisions (`422 LOC` trên `9e17cca`, nạp chuỗi từ `cms/l10n/error_codes.json`).
   - `storage/mod.rs` — Khai báo `pub mod revisions;`.
   - `storage/revisions/{mod.rs,types.rs,capture.rs,read_and_diff.rs,missing_refs.rs,restore.rs,autosave.rs}` — DTOs & Inputs (`RevisionActor<'a>`, `RevisionSummaryDto`, `RevisionDetailDto`, `RevisionsListResponse`, `CompareTarget`, `RevisionCompareResponse`, `AutosaveSnapshotDto`, `AutosaveSnapshotResponse`, `AutosaveUpsertInput`, `AutosaveUpsertResponse`, `RevisionRestoreRequest`, `RestoreRevisionOutcome`, internal `RestoreError`); tx helpers (`compute_revision_content_hash(&CanonicalRevisionPayload) -> String`, `capture_baseline_if_missing_in_tx(conn, ...)`, `capture_entry_revision_in_tx(conn, ...)`, `record_entry_revision_and_clear_autosave_in_tx(...)`, `capture_import_revisions_in_conn(...)`, `clear_actor_autosave_in_tx(conn, ...)`, `clear_all_entry_autosaves_in_tx(conn, ...)`, `materialize_autosave_payload_in_conn(...)`, `sanitize_snapshot_for_restore(...)`); public storage APIs (`list_revisions`, `get_revision_filtered`, `compare_revisions`, `prepare_revision_restore`, `commit_revision_restore`, `get_autosave`, `upsert_autosave`, `delete_autosave`, `detect_missing_refs_in_conn`).
   - `storage/entries/body_and_refs/{commit_composite_entry_save.rs,forbid_generic_pattern_write.rs}` — Đấu nối `capture_baseline_if_missing_in_tx`, `capture_entry_revision_in_tx`, `clear_actor_autosave_in_tx` / `record_entry_revision_and_clear_autosave_in_tx` tại `create_entry_with_*`, `update_entry_with_*` (`forbid_generic_pattern_write.rs` `508 -> <= 600 LOC`), `commit_composite_entry_save`, `commit_entry_translation`.
   - `storage/builder/read_and_preview/mutate_entry_builder.rs` — Đấu nối `'builder_save'` / `'builder_reset'` và xóa autosave.
   - `storage/patterns.rs` & `routes/patterns.rs` — Thêm `create_for_actor` / `update_for_actor` (giữ autosave, wrapper `actor: None` → `"system"`), routes truyền `Some(&actor)`.
   - `storage/entry_lifecycle/{validation_and_fences.rs,types/apply_lifecycle_in_tx.rs}` — `commit_metadata_patch_inner` chụp `'quick_edit'` (giữ autosave); `apply_lifecycle_in_tx` (`Trash`) xóa `entry_autosaves`.
   - `storage/seo/entry_routes.rs` & `routes/seo.rs` — Thêm `put_entry_seo_for_actor` (giữ wrapper cũ), routes truyền `Some(&actor)` và chặn `type_key == "pattern"` (`404`) trên `*_entry_seo` & `*_entry_route`.
   - `storage/import/read/execute_sample_import_with_cutpoint.rs` — Gọi helper `capture_import_revisions_in_conn` chụp `'import'` giữ `<= 600 LOC`.
   - `storage/restore.rs` — `validate_and_rebase_copy` chạy `DELETE FROM entry_autosaves`.
   - `routes/mod.rs`, `routes/revisions/{mod.rs,revisions_handlers.rs,autosave_handlers.rs}`, `http_middleware.rs`, `app.rs` — Tách 2 khối `app.rs:782-864` & `867-880` sang `http_middleware.rs` (`<= 140 LOC`) + re-export `HostOriginRejected` giữ `.layer`/`.fallback` tại `app.rs` (`<= 883` LOC); `routes/revisions/mod.rs` dùng `pub use *` cho 2 submodule; đăng ký 7 HTTP handlers dưới `/api/admin/content-types/{type_key}/entries/{id}/...` với top-level CSRF guard, `AdmittedMutation`, `DefaultBodyLimit` và `Bytes`/`Path<(String, String, String)>` sau auth.
4. **Admin Frontend (`cms/admin/src/`):**
   - `api/client.ts` — Thêm `keepalive?: boolean`, `bodyCode?: string`, parse trực tiếp `missingRefs?: MissingRefsReport` (`409`) và `fieldErrors?: Record<string, string>` (`422`).
   - `api/revisions.ts` & `api/index.ts` — 7 hàm API (dùng `requestText` cho `DELETE .../autosave`).
   - `features/editor/EditorDraftStore.ts` — Thêm `subscribe(listener)` cho `sessionEpoch` và `page_layout_key`/`custom_template_key` vào payload/diff/restore.
   - `features/editor/{PublishBoxPanel.tsx,SingleEditBuilderPromptDialog.tsx,SingleEditTranslationModal.tsx,SingleEditWorkspace.tsx,PanelRegionRenderer.tsx}` & `shell/ContentWorkspaces.tsx` — Tách JSX hiện có giữ nguyên giới hạn `loc-baseline.json`, nối `baseline`/`saved`/`applyPayload`/`onChange` cho `page_layout_key`/`custom_template_key` và gắn `useSingleEditRevisions` & `PublishBoxRevisionsControl`.
   - `features/revisions/{types.ts,useEntryAutosave.ts,AutosaveRecoveryBanner.tsx,RevisionDiffView.tsx,RevisionHistoryModal.tsx,PublishBoxRevisionsControl.tsx,useSingleEditRevisions.ts,SingleEditRevisionsOverlay.tsx,useStudioMode2Revisions.ts,StudioMode2RevisionsOverlay.tsx}` — Components & hooks `<= 600 LOC` (`useStudioMode2Revisions.ts <= 420`, `StudioMode2RevisionsOverlay.tsx <= 140`).
   - `App.tsx`, `studio/{StudioWorkspace.tsx,StudioSidebarHeader.tsx,StudioSidebarFooter.tsx}` — Truyền `siteInstanceId`/`authGeneration`/`studioResetEpoch`, reset `mayLeave` khi bỏ Studio Mode 2, gắn `useStudioMode2Revisions` & `<StudioMode2RevisionsOverlay />` (`<AutosaveRecoveryBanner />` trên đầu `<aside>`), `onOpenRevisions` trong `hasOverflowActions`, `getEntryBuilder` + `resetHistory()`, dọn autosave an toàn `try/catch` trong `handleReloadMode2`/`handleExit`.
5. **Localization, Tooling, Contracts & Tests:**
   - `cms/l10n/{error_codes.json,opendash.pot,vi.po}` — Đăng ký lỗi backend (S02) & chuỗi Gettext UI (S03–S04).
   - `cms/scripts/run-tests.py` — Cập nhật `GROUPS` (`unit_filters` `storage::revisions`, `routes::revisions`), `classify_changed_file` (`revisions/` prefix trước dict lookups, explicit `escalate_all = True` cho `http_middleware.rs`), `keyword_map` (`"revision"`, `"autosave"` ở cả `"content"` & `"builder"`), và `RunTestsSelfTest`.
   - `docs/contracts/M8_WRITER_INVENTORY.md` — Bổ sung `routes::revisions::*` (§2) và phân loại bảng/ngoại lệ Trash/Purge/Restore (§3).
   - `cms/crates/opendash-server/tests/{content.rs,builder.rs,ops/writer_barrier.rs,content/revisions_storage.rs,content/revisions_api.rs,builder/revisions_builder.rs}` & `cms/scripts/revisions_autosave_e2e.py`.

---

### Verification Plan

Bắt buộc chạy **đúng thứ tự** theo từng Slice (build `cms/admin` **trước** khi chạy test Rust `opendash-server` để `rust-embed` có `cms/admin/dist/index.html`):

1. **Cổng kiểm tra tĩnh chung (từ gốc repository — mọi Slice S01–S04):**
   ```bash
   python3 cms/scripts/check-standards.py
   python3 cms/scripts/check-standards.py --self-test
   python3 cms/scripts/check-plans.py
   python3 cms/scripts/check-plans.py --self-test
   python3 .agents/scripts/check-agent-artifacts.py
   python3 cms/scripts/run-tests.py --list
   python3 cms/scripts/run-tests.py --self-test
   cargo fmt --manifest-path cms/Cargo.toml --all --check
   git diff --check
   ```
2. **Ma trận kiểm chứng tăng tiến theo Slice:**
   - **Sau S01:**
     ```bash
     npm --prefix cms/admin run build
     python3 cms/scripts/run-tests.py --changed
     cargo test --manifest-path cms/Cargo.toml -p opendash-core revisions
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash storage::revisions
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --test content revisions_storage
     ```
   - **Sau S02:**
     ```bash
     npm --prefix cms/admin run build
     python3 cms/scripts/run-tests.py --changed
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash routes::revisions
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash i18n
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --test content revisions_api
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --test builder revisions_builder
     cargo test --manifest-path cms/Cargo.toml -p opendash-server --test ops writer_barrier
     ```
   - **Sau S03:**
     ```bash
     python3 cms/scripts/check-standards.py
     npm --prefix cms/admin run typecheck
     npm --prefix cms/admin run build
     node cms/scripts/check-i18n.mjs
     ```
   - **Sau S04 (đóng Spec & kiểm chứng Tầng 2):**
     ```bash
     python3 cms/scripts/check-standards.py
     npm --prefix cms/admin run typecheck
     npm --prefix cms/admin run build
     node cms/scripts/check-i18n.mjs
     python3 cms/scripts/run-tests.py --group all --release
     ```
3. **Kiểm chứng Live Browser E2E (Playwright) sau S04 cho CUJ 1–6:**
   - Biên dịch binary server nhúng `cms/admin/dist/` mới nhất, khởi động instance cô lập và chạy `python3 cms/scripts/revisions_autosave_e2e.py` kiểm chứng trọn vẹn:
     - Single Edit autosave 15s/manual trigger & `title = ""` tạm thời → reload trang → phục hồi từ `AutosaveRecoveryBanner` (gồm `page_layout_key` / `cleared_fields` đơn lẻ giữ nguyên qua `401`/`412`) → không rò rỉ ra URL công khai.
     - Single Edit diff (`word` + `field` + `taxonomy`), restore và hoàn tác về bản trước đó, giữ nguyên `slug` + `published`, đồng bộ permalink `%category%`, Trash entry không dính `409` khi xóa.
     - Studio Mode 2 Landing Page & Pattern CPT: Block-Aware Diff (`added`/`removed`/`moved`+`prop_changes`/`modified`, gồm `PluginBlock` trên `BlockSchemaV1`), autosave merge không làm mất metadata, restore (kể cả `builder_reset` và `content_mode == 'editor'`) cập nhật `entryTitle` + live preview, rời Mode 2 qua sidebar/Back sạch sẽ không kẹt confirm/dirty nền.
     - RBAC & ACL 3 trục: Editor bị chặn `403` trên `pattern` và `source_surface = "studio"` khi `body` khóa; ẩn hoàn toàn trường `admin`-only (cả `builder_document`, Text/Block Diff lẫn `RestoreRevisionOutcome`); không chặn `409 missing_refs` và bỏ qua `sync_entry_body_refs_in_conn` trên trường `!editable`; media/form/entry/pattern/options xóa/tắt thành công không bị revision ghim, restore báo `409 revision_missing_refs` và dọn sạch khi xác nhận.
     - Backup & Restore qua Safe Mode: pass fingerprint (`235 <= 256` objects), giữ nguyên `entry_revisions` và dọn `entry_autosaves`.
4. **Đóng Spec theo EGM:** Chạy `/egm-drift-report`, cập nhật `Status` trong [SITEMAP.md](SITEMAP.md) sang `**Verified**` kèm biên bản tại `docs/verification/` và chạy `node cms/scripts/codegraph-index-docs.mjs`.
