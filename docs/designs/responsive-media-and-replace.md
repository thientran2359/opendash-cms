# Đặc tả Thiết kế: Ảnh Responsive (`srcset` / Multi-Size), Tối ưu WebP & Replace Media (Giữ nguyên ID)

**Status:** Approved  
**Date:** 2026-11-27  
**Author:** Orion / Elephant  
**Owner Decision:** `1A + 2A + 3A + 4A` (chốt ngày 2026-11-26)  
**Approved By:** Owner (thientran2359)  
**Approval Evidence:** Owner chốt phương án `1A + 2A + 3A + 4A` (2026-11-26) và chỉ đạo triển khai trên `origin/main@43c19c2` ("kéo code mới nhất về và triển khai tiếp đi", 2026-11-27)  
**Goldfish Review:** Hoàn tất 2 vòng Goldfish fresh-context 3-persona (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`); toàn bộ phát hiện Blocker/Major/Minor/Nit đã được xử lý trong Revision 3 và đồng bộ theo kiến trúc Feature-First trên `main@43c19c2`.  
**Dependency Status:** Các phần lõi của [`unified-field-engine.md`](unified-field-engine.md) (`0055`, `0056`), `0057` (`manual_redirects`), gettext `.pot`/`.po`/`.mo` và PR #620 đã merge vào `origin/main@45888d6`. Sau khi đồng bộ nhánh với `origin/main`, đặc tả này dùng migration kế tiếp **`0058`** (`cms/migrations/0058_responsive_media_and_replace.sql`). Mọi file nguồn mới tuân thủ giới hạn `<= 600 LOC` và chuẩn Feature-First trong [`AGENTS.md`](../../AGENTS.md).

> [!NOTE]
> Tài liệu này thiết lập đặc tả kỹ thuật đầy đủ theo quy trình **EGM (Elephant-Goldfish Model)** và [`AGENTS.md`](../../AGENTS.md) cho bộ ba năng lực media: **(1) Đa kích cỡ responsive (`srcset` / `sizes`) không upscale**, **(2) Nén WebP lossy có size-gate kèm encode-only security boundary (`webp =0.3.1`)**, và **(3) Replace Media giữ nguyên 64-hex `id` (`If-Match` CAS + `MediaReadGate` + journal `.quarantine/`)**, đồng thời giữ Backup gọn nhẹ (`4A`).

---

## 1. Overview & Goals

### 1.1 Bài toán hiện tại

Hệ thống media của OpenDash hiện lưu mỗi asset theo `id = sha256(bytes)` với tối đa 2 tệp trên đĩa:
- File gốc canonical: `uploads/media/{id[0..2]}/{id}.{ext}` phục vụ qua `/media/{id}/file`.
- Ảnh thu nhỏ đơn lẻ: `uploads/media/{id[0..2]}/{id}.thumb.png` (`<= 320x320`) phục vụ qua `/media/{id}/thumbnail`.

Cách tổ chức này dẫn đến ba giới hạn lớn khi vận hành website thật:
1. **Lãng phí băng thông và LCP chậm:** Người dùng mobile phải tải ảnh gốc tới 6000px vì thiếu `medium` / `large` / `srcset` / `sizes`.
2. **Chưa tối ưu định dạng WebP:** Thư viện `image = "=0.25.10"` / `image-webp = 0.2.4` hiện chỉ mã hóa WebP lossless (VP8L), thường cho dung lượng lớn hơn JPEG ảnh chụp; trang công khai chưa có `<picture><source type="image/webp">` gọn nhẹ.
3. **Không thể thay thế file tại chỗ (Replace Media):** Vì `id` đang đồng nhất chặt với `content_sha256`, việc thay ảnh/video mới bắt buộc tạo `id` mới, làm gãy liên kết cũ trong nội dung, Visual Builder, cấu hình SEO và hệ sinh thái ngoài.

### 1.2 Mục tiêu & Quyết định đã chốt (`1A + 2A + 3A + 4A`)

1. **Tùy chọn `1A` — Nén WebP lossy với `webp = "=0.3.1"` (`default-features = false`) theo ranh giới Encode-Only:**
   - Giữ giải mã ảnh đi vào **100% thuần Rust** qua `image = "=0.25.10"` (dưới `DECODE_SLOTS` và giới hạn `6000x6000`, `20_000_000` pixels).
   - Crate `webp =0.3.1` (`libwebp-sys` static, tương thích MIT/Apache-2.0/BSD-3-Clause) chỉ nhận buffer `RgbImage` / `RgbaImage` tin cậy đã giải mã và resize xong để nén `.webp`, có **size-gate** (`webp.len() < fallback.len()`). Không bao giờ gọi API decode của `libwebp`.
2. **Tùy chọn `2A` — Bộ biến thể cố định không upscale + C-SEC `od_image()` & Builder / Theme / Rich-HTML responsive:**
   - Sinh háo hức lúc upload/replace các biến thể không upscale: `thumbnail` (`<= 320` box, chỉ ghi `thumbnail.webp` nếu nhỏ hơn `.thumb.png` truyền thống), `medium` (`768w` nếu `orig_width > 768`), `large` (`1536w` nếu `orig_width > 1536`), `full` (chỉ sinh `.webp` cho nguồn JPEG/PNG nếu nhỏ hơn file gốc).
   - Cung cấp route `/media/{id}/variant/{spec_ext}` và `/api/admin/media/{id}/variant/{spec_ext}`, công cụ Admin **Regenerate Variants** theo lô hợp tác, helper C-SEC `od_image()` dùng `ImageNonceTable` với provenances không giả mạo được, và nâng cấp `srcset` ở Visual Builder (`page.rs`), template hiển thị nội dung (`content_render.rs`), SEO và Plugin runtime.
3. **Tùy chọn `3A` — Replace Media giữ nguyên `id` cho Admin:**
   - Hỗ trợ thay thế **cả Image và Video (MP4)** giữ nguyên 64-hex `id` và **cùng `kind`** (cấm đổi chéo `image <-> video`), tách `content_sha256` khỏi `id`, dùng `generation` + `version` (`ETag: "gv{gen}-v{ver}"`), bắt buộc `If-Match` CAS, `MediaReadGate` không TOCTOU và không giữ lock DB trong critical section để tránh DoS `503`, cộng journal phục hồi `media_replace_jobs` + `.quarantine/{job_id}/` phẳng tuyệt đối.
4. **Tùy chọn `4A` — Lưu trữ `uploads/variants/` tách biệt, Backup không ôm biến thể, Restore reset sạch:**
   - Lưu biến thể tại `uploads/variants/{shard}/{media_id}/v{version}/{spec}.{ext}` để tái tạo hoàn toàn được.
   - Backup giữ nguyên `BackupManifestV1` và `MAX_BACKUP_PAYLOADS = 20_000`, chỉ sao lưu file gốc + `.thumb.png` (+ delete quarantine phẳng); Restore xóa `uploads/variants/`, dọn `media_variants`, đặt `variants_ready_version = 0` trong `validate_and_rebase_copy` trước khi verify và báo Admin chạy Regenerate.

### 1.3 Non-Goals

- Không hỗ trợ AVIF (`ravif`/`aom`) hay nén video bằng `ffmpeg` runtime (giữ ràng buộc single-binary của [`PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md)).
- Không mở rộng giới hạn kích thước ảnh vượt quá `6000x6000` hay video vượt quá `3840x2160` của `0030_media_video.sql`.
- Không cho phép kích thước cắt tùy biến theo query string động (tránh DoS ổ đĩa/CPU).
- Không giữ lịch sử các phiên bản cũ để rollback trực tiếp trong Admin UI; `.quarantine/{job_id}/` chỉ nhằm mục đích đảm bảo giao dịch nguyên tử lúc thay thế và thời gian ân hạn dọn rác 7 ngày.
- Không cho phép plugin WASM tự ý gọi API Replace Media hoặc ghi trực tiếp xuống filesystem ngoài host capabilities hiện có.

---

## 2. Architecture & Technical Plan

### 2.1 Kiến trúc CSDL & Migration `0058`

Migration `cms/migrations/0058_responsive_media_and_replace.sql` chạy tuần tự sau `0057_manual_redirects_and_404_log.sql` (`227` schema objects trước `0058` trên `main@45888d6`). Migration `0058` sử dụng `STRICT, WITHOUT ROWID` cho 2 bảng mới để không phát sinh thêm `sqlite_autoindex`, bổ sung **+7 net schema objects** (tổng `234 <= 256` giới hạn `MAX_SCHEMA_OBJECTS` của `storage/restore.rs`):

1. **Mở rộng `media_assets` (`ALTER TABLE`, `+0` table objects + `+2` index objects):**
   - Lưu ý: Cột `generation` và `version` **ĐÃ TỒN TẠI** từ `0033_media_lifecycle.sql` (`generation INTEGER NOT NULL DEFAULT 1`, `version INTEGER NOT NULL DEFAULT 1`). Tuyệt đối **không** chạy `ADD COLUMN version` hoặc `ADD COLUMN generation` trong `0058`.
   - Giữ nguyên toàn bộ `CHECK` bounds của `0030_media_video.sql` (`image`: `width/height BETWEEN 1 AND 6000`, `byte_size > 0 AND byte_size <= 8388608`, `thumb_width/height BETWEEN 1 AND 320`, `extension IN ('jpg','png','webp')`; `video`: `width BETWEEN 1 AND 3840`, `height BETWEEN 1 AND 2160`, `byte_size > 0 AND byte_size <= 104857600`, `duration_ms BETWEEN 1 AND 600000`, `video_codec IS NOT NULL`, `extension = 'mp4'`).
   - Chỉ `ALTER TABLE media_assets` thêm đúng **3 cột**:
     - `content_sha256 TEXT CHECK (content_sha256 IS NULL OR (length(content_sha256) = 64 AND content_sha256 GLOB '[0-9a-f]*'))`
     - `variants_ready_version INTEGER NOT NULL DEFAULT 0 CHECK (variants_ready_version >= 0)` (upload ảnh mới đặt `1`; ảnh legacy `0`; video đặt `0`).
     - `updated_at TEXT`
   - Backfill trong `0058`:
     - `UPDATE media_assets SET content_sha256 = id WHERE content_sha256 IS NULL;`
     - `UPDATE media_assets SET updated_at = created_at WHERE updated_at IS NULL;`
   - Thêm 2 index (`+2` objects):
     - `CREATE INDEX idx_media_assets_content_sha256 ON media_assets (content_sha256, lifecycle_state);` (phục vụ dedup O(log N) sau khi tách `content_sha256` khỏi `id`).
     - `CREATE INDEX idx_media_assets_variants_pending ON media_assets (id) WHERE kind = 'image' AND lifecycle_state = 'active' AND variants_ready_version < version;`
   - Trong code Rust (`storage/media/`), mọi `SELECT` vẫn đọc phòng vệ `COALESCE(content_sha256, id) AS content_sha256` và `COALESCE(updated_at, created_at) AS updated_at`.
2. **Bảng `media_variants` (`STRICT, WITHOUT ROWID`, `+1` object, `0` autoindex):**
   - `media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE`
   - `version INTEGER NOT NULL CHECK (version >= 1)`
   - `spec TEXT NOT NULL CHECK (spec IN ('thumbnail', 'medium', 'large', 'full'))`
   - `format TEXT NOT NULL CHECK (format IN ('jpeg', 'png', 'webp'))`
   - `width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 6000)`
   - `height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 6000)`
   - `byte_size INTEGER NOT NULL CHECK (byte_size > 0)` *(đồng bộ tên cột `byte_size` với `media_assets.byte_size`)*
   - `relative_path TEXT NOT NULL`
   - `sha256 TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 GLOB '[0-9a-f]*')`
   - `created_at TEXT NOT NULL DEFAULT (datetime('now'))`
   - `PRIMARY KEY (media_id, version, spec, format)` — Không cần secondary index vì khóa chính `WITHOUT ROWID` phục vụ trực tiếp mọi truy vấn theo `(media_id, version, spec)`.
3. **Bảng `media_replace_jobs` (`STRICT, WITHOUT ROWID`, `+1` table + `+3` index objects = `+4` objects):**
   - Khớp mẫu của `0033_media_lifecycle.sql` (`media_delete_jobs`): lưu `media_id TEXT NOT NULL CHECK (length(media_id) = 64 AND media_id GLOB '[0-9a-f]*')` không gắn FK chặn `DELETE FROM media_assets` sau khi job hoàn tất (kiểm tra active job bằng code/TX khi `begin_delete`).
   - Toàn bộ tên cột & ràng buộc `0030_media_video.sql:26-69` được phản ánh chính xác 1-1 cho bản thay thế:
     - `job_id TEXT PRIMARY KEY CHECK (length(job_id) = 64 AND job_id GLOB '[0-9a-f]*')` *(64-hex sinh từ `auth::generate_random_token()`, nhất quán với `0032` và `0033`)*
     - `media_id TEXT NOT NULL CHECK (length(media_id) = 64 AND media_id GLOB '[0-9a-f]*')`
     - `expected_generation INTEGER NOT NULL CHECK (expected_generation >= 1)`
     - `expected_version INTEGER NOT NULL CHECK (expected_version >= 1)`
     - `new_version INTEGER NOT NULL CHECK (new_version = expected_version + 1)`
     - `old_content_sha256 TEXT NOT NULL CHECK (length(old_content_sha256) = 64 AND old_content_sha256 GLOB '[0-9a-f]*')`
     - `new_content_sha256 TEXT NOT NULL CHECK (length(new_content_sha256) = 64 AND new_content_sha256 GLOB '[0-9a-f]*')`
     - `new_kind TEXT NOT NULL CHECK (new_kind IN ('image', 'video'))`
     - `new_filename TEXT NOT NULL CHECK (length(new_filename) BETWEEN 1 AND 255)`
     - `new_mime_type TEXT NOT NULL`
     - `new_extension TEXT NOT NULL`
     - `new_byte_size INTEGER NOT NULL`
     - `new_width INTEGER NOT NULL`
     - `new_height INTEGER NOT NULL`
     - `new_thumb_width INTEGER`
     - `new_thumb_height INTEGER`
     - `new_duration_ms INTEGER`
     - `new_video_codec TEXT`
     - `new_audio_codec TEXT`
     - `new_variants_json TEXT NOT NULL` — mảng JSON `Vec<QueuedMediaVariant>` (`spec`, `format`, `width`, `height`, `byte_size`, `relative_path`, `sha256`, `staged_rel_path`).
     - `old_canonical_rel_path TEXT NOT NULL`, `old_thumb_rel_path TEXT`
     - `staging_rel_dir TEXT NOT NULL`, `quarantine_rel_dir TEXT NOT NULL`
     - `phase TEXT NOT NULL CHECK (phase IN ('staged', 'moving_old', 'promoting_new', 'completed', 'rolled_back'))`
     - `error_message TEXT`, `created_at TEXT NOT NULL DEFAULT (datetime('now'))`, `updated_at TEXT NOT NULL DEFAULT (datetime('now'))`, `purge_after TEXT`, `purged_at TEXT`
     - Table-level `CHECK` khớp `0030_media_video.sql:41-68`:
       ```sql
       CHECK (
           (
               new_kind = 'image'
               AND new_mime_type IN ('image/jpeg', 'image/png', 'image/webp')
               AND new_extension IN ('jpg', 'png', 'webp')
               AND new_byte_size > 0 AND new_byte_size <= 8388608
               AND new_width BETWEEN 1 AND 6000
               AND new_height BETWEEN 1 AND 6000
               AND new_thumb_width IS NOT NULL AND new_thumb_width BETWEEN 1 AND 320
               AND new_thumb_height IS NOT NULL AND new_thumb_height BETWEEN 1 AND 320
               AND new_duration_ms IS NULL
               AND new_video_codec IS NULL
               AND new_audio_codec IS NULL
           )
           OR
           (
               new_kind = 'video'
               AND new_mime_type = 'video/mp4'
               AND new_extension = 'mp4'
               AND new_byte_size > 0 AND new_byte_size <= 104857600
               AND new_width BETWEEN 1 AND 3840
               AND new_height BETWEEN 1 AND 2160
               AND new_thumb_width IS NULL
               AND new_thumb_height IS NULL
               AND new_duration_ms IS NOT NULL AND new_duration_ms BETWEEN 1 AND 600000
               AND new_video_codec IS NOT NULL
           )
       )
       ```
   - 3 indexes (`+3` objects):
     - `CREATE UNIQUE INDEX idx_media_replace_jobs_single_active ON media_replace_jobs(media_id) WHERE phase IN ('staged', 'moving_old', 'promoting_new');`
     - `CREATE INDEX idx_media_replace_jobs_phase_updated ON media_replace_jobs(phase, updated_at);`
     - `CREATE INDEX idx_media_replace_jobs_purge ON media_replace_jobs(purged_at, purge_after) WHERE phase IN ('completed', 'rolled_back');`
4. **Tái tạo `operation_jobs` (`+0` net objects, khớp 100% `0032_operations_jobs.sql`):**
   - Bảo toàn toàn bộ cột của `0032_operations_jobs.sql` (đặc biệt cột **`state`**, không có cột `status`), chỉ mở rộng `CHECK` cho `kind`:
     `kind TEXT NOT NULL CHECK (kind IN ('backup', 'restore', 'reconcile', 'purge', 'media_delete', 'media_regenerate'))`.
   - `DROP` và tạo lại đúng **3 indexes gốc** của `0032_operations_jobs.sql:19-28`:
     - `idx_operation_jobs_state_requested ON operation_jobs (state, requested_at DESC, id DESC)`
     - `idx_operation_jobs_kind_requested ON operation_jobs (kind, requested_at DESC, id DESC)`
     - `idx_operation_jobs_single_active_heavy ON operation_jobs ((1)) WHERE kind IN ('backup', 'restore', 'reconcile', 'purge') AND state IN ('queued', 'running')`
   - Cập nhật `OperationKind` trong `cms/crates/opendash-server/src/storage/operations.rs:10-46` thêm variant `MediaRegenerate` (`"media_regenerate"`, `is_heavy() == false`) để không bao giờ tranh chấp khóa `single_active_heavy` với `backup`/`restore`/`reconcile`/`purge`.

---

### 2.2 Ma trận Biến thể Ảnh, Nén WebP (`webp =0.3.1`) & Giới hạn Tài nguyên

1. **Crate & Ranh giới An toàn (`1A`):**
   - Thêm `webp = { version = "=0.3.1", default-features = false }` vào `cms/Cargo.toml` và khóa trong `cms/Cargo.lock`.
   - `webp` cung cấp wrapper MIT/Apache-2.0 quanh `libwebp-sys` (BSD-3-Clause); ghi nhận yêu cầu toolchain biên dịch C (`cc`) trong `docs/architecture/TECH_STACK.md`.
   - **Ranh giới Encode-Only:** Toàn bộ giải mã JPEG/PNG/WebP đi vào vẫn thực hiện qua pure-Rust `image = "=0.25.10"`. Chỉ `DynamicImage` hợp lệ đã vượt qua giới hạn `MAX_DIMENSION = 6000` và `MAX_PIXELS = 20_000_000` mới chuyển buffer `RgbImage` / `RgbaImage` cho `webp::Encoder`. Giải phóng `webp::WebPMemory` sang `Vec<u8>` ngay trong `spawn_blocking`.
2. **Kiểm soát đồng thời toàn cục (`DECODE_SLOTS` / `VIDEO_INGEST_SLOTS`):**
   - Di chuyển `static DECODE_SLOTS: Semaphore = Semaphore::const_new(2)` và `static VIDEO_INGEST_SLOTS: Semaphore = Semaphore::const_new(2)` đang nằm cục bộ trong `media_upload` / `media_video_upload` tại `cms/crates/opendash-server/src/routes/media.rs:98,130` lên làm `pub(crate) static` trong `cms/crates/opendash-server/src/media/types.rs`. Mọi luồng upload mới, replace media và batch `media_regenerate` đều dùng chung 2 semaphore này.
3. **Quy tắc Sinh biến thể & Size-Gate Tất định (`2A`):**
   - **Thumbnail (`thumbnail`):** File canonical `.thumb.png` tại `uploads/media/{shard}/{id}.thumb.png` (`<= 320x320`) là ảnh fallback duy nhất cho cỡ thumbnail. **Tuyệt đối không tạo `thumbnail.jpg` hay `thumbnail.png` trong `uploads/variants/`**. Chỉ ghi `('thumbnail', 'webp')` vào `uploads/variants/{shard}/{id}/v{ver}/thumbnail.webp` nếu `webp_bytes.len() < thumb_png_bytes.len()`.
   - **Medium (`medium`) & Large (`large`):** Sinh `medium` nếu `orig_width > 768` (`target_w = 768`) và sinh `large` nếu `orig_width > 1536` (`target_w = 1536`), tính `target_h = ((orig_height as u64 * target_w as u64 + orig_width as u64 / 2) / orig_width as u64).max(1) as u32`. Không bao giờ upscale.
   - **Chọn 1 định dạng non-WebP fallback cho mỗi cỡ `medium`/`large`:**
     - Nguồn `image/jpeg` -> chỉ `jpeg` (quality `82`).
     - Nguồn `image/png`:
       - Nếu có pixel alpha `< 255`: chỉ `png`.
       - Nếu opaque (toàn bộ `alpha == 255`): nén thử `jpeg` (quality `85`) và `png`, **chọn đúng 1 bản có `byte_size` nhỏ hơn nghiêm ngặt** -> ghi đúng 1 hàng non-WebP trong `media_variants`.
     - Nguồn `image/webp`: chỉ sinh `medium.webp` / `large.webp` (nếu vượt ngưỡng `768w` / `1536w`); không sinh bản non-WebP dư thừa.
   - **Full (`full`):**
     - Chỉ áp dụng cho nguồn `image/jpeg` hoặc `image/png`: nén WebP ở nguyên kích thước `(orig_width, orig_height)` và chỉ ghi `('full', 'webp')` khi `webp_bytes.len() < canonical_byte_size`.
     - Với nguồn `image/webp`: **không** tạo hàng `('full', 'webp')`.
   - **Chính sách chất lượng WebP tất định & chặn phình C-heap với Alpha:**
     - Nguồn `image/jpeg` (opaque) và `image/webp` (opaque): `encode(82.0)` (khớp quality `82` của fallback JPEG).
     - Nguồn `image/png` không có alpha: `encode(85.0)` (khớp quality `85` của candidate fallback JPEG cho opaque PNG).
     - Ảnh có alpha thực sự (`alpha < 255`):
       - Nếu `width * height <= 1_000_000` (`<= 1 MP`): thử `encode_lossless()` trước; nếu `lossless.len() >= fallback.len()`, nén lại bằng lossy `encode(85.0)` (vẫn giữ kênh alpha).
       - Nếu `width * height > 1_000_000`: bỏ qua `encode_lossless()` để khóa trần C-heap của `libwebp`, chạy thẳng `encode(85.0)`.
     - Chỉ lưu biến thể WebP vào `uploads/variants/` và `media_variants` khi `webp_bytes.len() < fallback_bytes.len()`.
4. **Quy tắc xây dựng `webp_srcset` & `fallback_srcset` (`opendash-core::media`):**
   - Hàm thuần `build_responsive_srcsets(mime_type: &str, orig_width: u32, file_url: &str, variants: &[MediaVariantView]) -> (Option<String>, Option<String>)` nhận `file_url` **đã gắn sẵn `?v={version}`** (ví dụ `/media/{id}/file?v=2` hoặc `/api/admin/media/{id}/file?v=2`).
   - Danh sách `MediaAssetDetails.variants` chỉ gồm các hàng thực tế trong `media_variants` (không bao gồm `.thumb.png` legacy).
   - **Nguồn JPEG / PNG:**
     - `fallback_srcset`: chỉ trả `Some(...)` khi tồn tại ít nhất `medium` hoặc `large` non-WebP (`medium`, `large`, `{file_url} {orig_width}w`). Nếu không có `medium`/`large`, trả `None`.
     - `webp_srcset`: liệt kê theo thứ tự tăng dần bề rộng mọi hàng WebP trong `{medium, large, full}` (trả `None` nếu rỗng).
   - **Nguồn WebP:**
     - `fallback_srcset`: `None` (`<img src>` dùng `{file_url}`).
     - `webp_srcset`: chỉ trả `Some(...)` khi tồn tại ít nhất `medium` hoặc `large`: gồm `medium.webp` (nếu có) + `large.webp` (nếu có) + `{file_url} {orig_width}w`.
   - **Ghi nguyên tử:** Mọi ghi file vào staging/variants đều dùng helper mới `write_atomic_overwrite(path, bytes)` (tạo temp file cùng thư mục -> `sync_all` -> `rename` đè đích), tuyệt đối không dùng `write_once`.

---

### 2.3 Upload Hợp nhất (`actual_id` Dedup), `MediaReadGate` Zero-TOCTOU & Replace Media (`3A`)

1. **Di chuyển `MediaStore::staging_dir()` sang `uploads/.staging/`:**
   - Hiện tại `MediaStore::new(root)` (`media/service.rs`) đặt `self.root = root.join("media")` và `staging_dir()` trả về `uploads/media/.staging`, gây xung đột 2-hex shard trong `scan_uploads_tree` và `backup_worker/`.
   - Chỉnh `MediaStore` dùng `self.uploads_root()` để `staging_dir()` trả `self.uploads_root().join(".staging")` (đồng bộ với `storage/import/`), và nâng cấp `cleanup_staging()` dùng `remove_dir_all` cho thư mục con (`upload-*`, `replace-*`, `regen-*`) và `remove_file` cho file phẳng còn sót.
2. **Upload mới Hợp nhất cho CẢ Image (`media_upload`) & Video (`media_video_upload`):**
   - Khắc phục lỗ hổng `persist_video_from_staging` (`routes/media.rs:206-210`) ghi file live trước khi kiểm tra dedup khiến video cũ đã bị replace bị gán nhầm thành asset mới:
   - Cả `media_upload` và `media_video_upload` đều xử lý theo 4 bước thống nhất trong `storage/media/::allocate_or_save_with_variants`:
     1. Giải mã (ảnh hoặc MP4) và ghi toàn bộ file gốc + `.thumb.png` (nếu là ảnh) + biến thể vào `uploads/.staging/upload-{uuid}/`, tính `content_sha256`.
     2. Xin `SiteCoordinator::global().try_admit_mutation()` ngắn hạn **sau khi** đã auth (`require_session_with_csrf`) và staging/decode xong, rồi mở `BEGIN IMMEDIATE`:
        - Tìm asset đang hoạt động theo nội dung: `SELECT ... FROM media_assets WHERE COALESCE(content_sha256, id) = ?1 AND lifecycle_state = 'active'` (tận dụng `idx_media_assets_content_sha256`). Nếu có -> xóa `.staging/upload-{uuid}/` và trả về asset hiện hữu.
        - Nếu chưa trùng `content_sha256`: kiểm tra khóa chính `id = content_sha256` đã bị chiếm dụng bởi asset khác (do trước đó từng replace sang SHA mới) hoặc bởi `media_delete_jobs WHERE phase != 'finalized'`. Nếu đụng, sinh `actual_id = sha256(format!("{content_sha256}:{}", generate_random_token()))` đến khi duy nhất.
     3. Di chuyển/đổi tên từ `.staging/upload-{uuid}/` vào vị trí live của `actual_id` (`uploads/media/{shard}/{actual_id}.{ext}`, `.thumb.png`, `uploads/variants/{shard}/{actual_id}/v1/...`), chèn `media_assets` (`version = 1`, `variants_ready_version = 1` cho ảnh / `0` cho video) + `media_variants`, rồi `COMMIT`.
     4. Nếu bất kỳ bước ghi DB hoặc `COMMIT` nào lỗi sau khi đã promote file live, chạy dọn bù trừ xóa các file/thư mục vừa promote cho `actual_id` trước khi trả lỗi.
3. **`MediaReadGate` Không TOCTOU, Fix Lost-Wakeup & Single Lease Acquire:**
   - Fix lỗi lost-wakeup tiềm ẩn trong `media/service.rs`: trong `acquire_read_lease(&self, id: &str)` (chuyển sang `async fn` ở `media/read_gate.rs`), gọi `let notified = self.read_gate.notify.notified(); tokio::pin!(notified); notified.as_mut().enable();` **trước khi thả mutex**, rồi `tokio::time::timeout(remaining, notified)`. Hết `2s` trả `503 Service Unavailable`; `deleting == true` trả ngay `404 Not Found`.
   - Đổi `MediaStore::read` -> `read_with_lease(&self, _lease: &MediaReadLease, ...)`, thêm `read_variant_with_lease`, `metadata_with_lease`, `variant_metadata_with_lease` (ánh xạ `ErrorKind::NotFound` -> `404 Not Found`), và truyền `lease: MediaReadLease` vào `stream_authorized_video` để **không** gọi `acquire_read_lease` hai lần (loại bỏ re-entrancy deadlock `active_readers`).
   - Toàn bộ handler đọc (`/media/{id}/file`, `/thumbnail`, `/variant/{spec_ext}` trong `routes/public_content/assets_and_sitemap/` và `/api/admin/media/{id}/...` trong `routes/media.rs`) gọi `acquire_read_lease(&id).await?` **1 lần TRƯỚC** khi gọi `storage::media::find` / `find_variant`.
   - Đối với route public (`/media/{id}/...`): kiểm tra nhanh `media_is_referenced(db, id)` trước để không gây `503` giả cho URL rác chưa từng tham chiếu, còn với asset có tham chiếu thì giữ `MediaReadLease` bao trọn lần đọc metadata + đĩa (bảo đảm thấy đúng bundle `version` mới khi vừa replace).
   - Cập nhật `stream_authorized_video` (`media/types.rs`) chuyển `ETag` và `If-Range` từ `asset.id` sang `format!("\"{}\"", asset.metadata_etag())` (`"gv{gen}-v{ver}"`) để `Range` request khi tua video không nối nhầm chunk của file cũ sau Replace. Hợp nhất `media_lifecycle::parse_media_if_match` dùng chung `opendash_core::media::parse_media_etag_token`.
4. **Giao thức Replace Media (`PUT /api/admin/media/{id}/file`) — Không SQL trong Critical Section & Đầy đủ Rollback Tx 2:**
   - **Bước 1 — Auth & Preflight sớm (trước khi nhận stream tới 100 MiB):**
     - `require_admin_with_csrf(&headers, &db).await?` trực tiếp trong thân handler.
     - Kiểm tra `If-Match`: thiếu -> `428 Precondition Required`; sai định dạng `"gv{gen}-v{ver}"` -> `412 Precondition Failed`.
     - Tra nhanh asset hiện tại: kiểm tra tồn tại (`404`), khớp `(generation, version)` (`412`), `Content-Type` phù hợp với `asset.kind` (`image/*` cho `image`, `video/mp4` cho `video`; sai trả `422 Unprocessable Entity`), và nếu có `Content-Length`, chặn vượt `8 MiB` / `100 MiB` ngay bằng `413 Payload Too Large` / `400 Bad Request`.
   - **Bước 2 — Streaming Upload, Giải mã & Staging với RAII Guard:**
     - Nhận raw streaming `Body` (`Content-Type` + `X-File-Name`, đồng bộ với `media_upload`/`media_video_upload`; router gắn `DefaultBodyLimit::max(MAX_VIDEO_BYTES)`), ghi vào `uploads/.staging/replace-{job_id}/` được bọc trong `ReplaceStagingGuard` (dọn sạch khi drop).
     - Giải mã & nén toàn bộ `.thumb.png` + `uploads/variants/...` bản `new_version` ngay trong `.staging/replace-{job_id}/` (dưới `DECODE_SLOTS` / `VIDEO_INGEST_SLOTS`). Kiểm tra `new_kind == old_kind` (`422` nếu lệch).
   - **Bước 3 — Kiểm tra Độc giả trước khi bật `replacing`:**
     - Chờ tối đa `500ms` ở `media_read_gate.wait_for_no_active_readers_before_swap(&id)` khi **chưa bật `replacing = true`**. Nếu vẫn còn stream mở -> hủy staging qua `ReplaceStagingGuard`, trả `409 Conflict` (`"readers_active"`).
   - **Bước 4 — Xin `AdmittedMutation` ngắn hạn & Ghi Tx 1 (`phase = 'promoting_new'`) TRƯỚC khi bật `replacing`:**
     - Xin `SiteCoordinator::global().try_admit_mutation()` (nếu đang bảo trì trả `503`).
     - Mở `BEGIN IMMEDIATE` Tx 1: xác nhận lại CAS `(generation, version)`, không có `media_delete_jobs` / `media_replace_jobs` active, chèn `media_replace_jobs` với `phase = 'staged'` rồi cập nhật ngay `phase = 'promoting_new'` trong cùng Tx 1 trước `COMMIT` (hoặc chèn trực tiếp trạng thái chuẩn bị hoán đổi `'moving_old'`/`'promoting_new'` nếu 2-pha? Dùng `'promoting_new'` ngay tại điểm commit chuẩn bị chạm đĩa live bảo đảm crash recovery xử lý dọn cả file cũ lẫn file mới tiềm năng mà **không cần bất kỳ lệnh SQLite nào trong lúc `replacing = true`**!). Giải giáp `ReplaceStagingGuard`.
   - **Bước 5 — Bật `replacing = true`, Hoán đổi Đĩa (~5ms, 0 SQLite I/O) & Tx 2 Commit:**
     - Gọi `media_read_gate.try_begin_replace_swap(&id)`: nếu vừa có reader lọt vào -> cập nhật `media_replace_jobs` sang `'rolled_back'`, xóa staging, trả `409`.
     - Di chuyển file gốc cũ `{id}.{old_ext}` và `{id}.thumb.png` sang thư mục phẳng `uploads/.quarantine/{job_id}/` (**tuyệt đối KHÔNG di chuyển thư mục con `v{old_ver}/` vào `.quarantine/`**; giữ `uploads/variants/{shard}/{id}/v{old_ver}/` tại chỗ trong lúc swap!).
     - `rename` file gốc mới `{id}.{new_ext}`, `.thumb.png` mới và `uploads/variants/{shard}/{id}/v{new_ver}/` vào đích live (cùng mount `uploads/`).
     - Nhả tạm hoặc vẫn giữ `replacing = true` qua Tx 2 siêu ngắn? Thực hiện ngay `BEGIN IMMEDIATE` Tx 2 cập nhật `media_assets` (`filename`, `mime_type`, `extension`, `byte_size`, `width`, `height`, `thumb_width`, `thumb_height`, `duration_ms`, `video_codec`, `audio_codec`, `content_sha256`, `version = new_version`, `variants_ready_version`, `generation = generation + 1`, `updated_at`), xóa `media_variants` cũ / chèn `media_variants` mới, cập nhật `media_replace_jobs` (`phase = 'completed'`, `purge_after = datetime('now', '+7 days')`), `COMMIT`.
     - **Nếu BẤT KỲ bước đĩa nào OR Tx 2 lỗi (kể cả `SQLITE_BUSY` / disk full):** chạy `rollback_replace_disk` — unlink `{id}.{new_ext}` (đặc biệt khi `old_ext != new_ext`), xóa `v{new_ver}/`, chuyển ngược `{id}.{old_ext}` & `{id}.thumb.png` từ `.quarantine/{job_id}/` về live (bản `v{old_ver}/` vẫn còn nguyên vẹn tại chỗ!), nhả `replacing = false` (`notify_waiters()`), cập nhật `phase = 'rolled_back'`, trả `500`.
     - **Chỉ SAU KHI Tx 2 commit & đã tắt `replacing = false` (`notify_waiters()`):** thực hiện best-effort xóa thư mục staging và `uploads/variants/{shard}/{id}/v{old_ver}/` (nếu gián đoạn trước khi kịp xóa, `Reconcile` tự thu dọn như `ReconcileOrphanKind::Variant`).
5. **Thứ tự Khởi động trong `main.rs`:**
   - `(1)` Chạy migrations `0001..0058`.
   - `(2)` Gọi **duy nhất 1 lần** `storage::operations::mark_interrupted_on_startup(&db)` (xóa lời gọi trùng lặp tại `main.rs`).
   - `(3)` `recover_orphaned_import_runs(&db, uploads_root)` -> `recover_pending_delete_jobs_on_startup` -> **`recover_pending_replace_jobs_on_startup`** (nếu job đang ở `'staged'`: đánh dấu `'rolled_back'`; nếu ở `'moving_old'` / `'promoting_new'`: **unlink mọi file đích mới `{id}.{new_extension}`, `.thumb.png` mới nếu đã đè, và `v{new_version}/` trước khi khôi phục file cũ từ `.quarantine/{job_id}/`**, rồi đánh dấu `'rolled_back'`).
   - `(4)` **Chỉ SAU khi các hàm recovery hoàn tất**, mới gọi `media_store.cleanup_staging().await`.

---

### 2.4 Reconcile, `.quarantine/` Phẳng, Backup/Restore (`4A`), Sample Import & Multi-Batch Regenerate

1. **Giữ `.quarantine/` 100% Phẳng & Cập nhật `media_delete` + `media_lifecycle` + `storage/media_refs.rs`:**
   - Vì biến thể đã đánh số phiên bản (`v{ver}/`), Replace Media chỉ đưa file phẳng (`{id}.{old_ext}`, `{id}.thumb.png`) vào `uploads/.quarantine/{job_id}/` -> giữ nguyên bất biến phẳng của `scan_uploads_tree` (`media_lifecycle.rs:519`) và `BackupFileKind::QuarantineObject` (`opendash-core/src/backup.rs:304`).
   - Cập nhật `tombstone_media_in_tx` (`storage/media_lifecycle.rs`, mở từ `BEGIN IMMEDIATE` trong `routes/media.rs:289`): từ chối với `409 Conflict` nếu `media_id` đang có `media_replace_jobs` ở phase `'staged' | 'moving_old' | 'promoting_new'`; và trong `execute_delete_job_inner` (`media_lifecycle.rs`), sau khi finalize delete job, thực hiện best-effort `remove_dir_all(uploads/variants/{shard}/{media_id})`.
   - Cập nhật `media_lifecycle::scan_uploads_tree`: bỏ qua `uploads/.staging/` hợp lệ; duyệt thêm cây `uploads/variants/{2-hex}/{64-hex}/v{n}/{spec}.{ext}` tạo `ReconcileOrphanKind::Variant` tách biệt khỏi `ReconcileOrphanKind::Canonical`. Nâng `MAX_RECONCILE_FILES` từ `20_000` (`media_lifecycle.rs:211`) lên `100_000`.
   - Bổ sung `generation: i64` và `version: i64` vào `MediaInventoryRow`, đồng thời `plan_reconcile` đối chiếu cả bảng `media_variants` đang active `(media_id, version, relative_path)` thay vì chỉ đoán theo thư mục `v{version}/`.
   - Thay toàn bộ truy vấn `list_all_delete_jobs` giới hạn `LIMIT 200` trong `plan_reconcile`, `purge_quarantine` và `collect_operations_diagnostics` bằng hàm tra theo lô **`find_delete_jobs_by_ids` / `find_replace_jobs_by_ids`** dựa trên danh sách `job_id` thực tế quét được trong `uploads/.quarantine/`.
   - Trong `purge_quarantine`: **tuyệt đối bỏ qua** thư mục khớp với `media_replace_jobs` đang ở `'staged' | 'moving_old' | 'promoting_new'` bất kể tuổi `mtime`; chỉ xóa thư mục replace đã hết hạn 7 ngày ở `'completed' | 'rolled_back'` và cập nhật `purged_at = datetime('now')`; đồng thời prune bản ghi `media_replace_jobs` đã dọn quá 7 ngày (`purged_at IS NOT NULL AND updated_at < datetime('now', '-7 days')`).
   - Cập nhật `storage/media_refs.rs::count_reconcile_records_bounded` (`:416-437`) đếm `media_assets` + non-finalized `media_delete_jobs` + non-purged `media_replace_jobs` + `contact_notification_attempts`.
   - Trong `storage/operations.rs::prune_old_receipts`, bổ sung `AND kind != 'backup'` để receipt `media_regenerate` không đẩy mất lịch sử sao lưu.
2. **Backup (`4A`) & Restore Đúng Thứ Tự (`storage/backup.rs`, `backup_worker.rs`, `storage/restore.rs`):**
   - Trong `storage/backup.rs::collect_backup_inventory` (`:113-171`): thêm kiểm tra `SELECT COUNT(*) FROM media_replace_jobs WHERE phase IN ('staged', 'moving_old', 'promoting_new')` -> trả `ApiError::conflict(...)` khi đang có Replace đang chạy; giữ `quarantine_job_ids` chỉ truy vấn `media_delete_jobs` (do đó quarantine delete vẫn được sao lưu, còn quarantine replace chỉ giữ tạm trên máy hiện hành sẽ không phình snapshot).
   - Trong `backup_worker.rs`: bỏ qua thư mục cấp 1 `uploads/variants/` và `uploads/.staging/`, giữ nguyên kiểm tra shard 2-hex nghiêm ngặt cho `uploads/media/{shard}`.
   - Trong `storage/restore.rs::validate_and_rebase_copy` (`:227-296`, chạy **SAU** `compiled_migrator().run()` ở `:245` và **TRƯỚC** `PRAGMA integrity_check` ở `:270` và trước `verify_restored_db_references` ở `restore.rs:268`):
     - Thực thi:
       - `DELETE FROM media_variants;`
       - `UPDATE media_assets SET variants_ready_version = 0;`
       - `UPDATE media_replace_jobs SET phase = 'rolled_back', error_message = COALESCE(error_message, 'restored_cleanup'), purged_at = COALESCE(purged_at, datetime('now')), updated_at = datetime('now') WHERE phase IN ('staged', 'moving_old', 'promoting_new');`
       - `UPDATE media_replace_jobs SET purged_at = COALESCE(purged_at, datetime('now')), updated_at = datetime('now') WHERE purged_at IS NULL;`
     - Đồng thời `restore.rs` đảm bảo thư mục đích khôi phục không giữ `uploads/variants/` dư thừa. Nhờ đặt trước `verify_restored_db_references`, `collect_backup_inventory` đọc bản sao DB hoàn toàn sạch.
3. **Sample Import (`storage/import.rs`):**
   - Giữ `dry_run_sample_import` **100% chỉ đọc** (không ghi `import_runs`).
   - Mở rộng `StagedMediaEntry` (giữ mảng JSON `Vec<StagedMediaEntry>` trong `import_runs.media_manifest_json` tương thích với `recover_orphaned_import_runs` tại `:1561`) thêm `#[serde(default)] content_sha256: String` và `#[serde(default)] variants: Vec<QueuedMediaVariant>`.
   - Trong `execute_sample_import`, giải mã & sinh biến thể cho ảnh mẫu vào staging, giải quyết dedup `content_sha256` / `actual_id` và promote `uploads/variants/{shard}/{actual_id}/v1/`, đồng thời trong `recover_orphaned_import_runs` xóa thêm `uploads/variants/{shard}/{media_id}/` khi `!entry.already_existed && refs == 0`.
4. **Regenerate Variants Hợp tác Đa Batch (`POST /api/admin/media/variants/regenerate`):**
   - Dùng khóa đơn toàn cục `static MEDIA_REGENERATE_ACTIVE: AtomicBool` + RAII guard (`409 Conflict` nếu đang có batch chạy). Không giữ `AdmittedMutation` qua cả `15s` batch; chỉ xin `SiteCoordinator::global().try_admit_mutation()` trong khoảnh khắc ghi từng ảnh (ngừng sớm trả về `next_after_id` nếu hệ thống bắt đầu bảo trì `503`).
   - Hợp đồng `run_id` trên `operation_jobs` (`kind = 'media_regenerate'`):
     - `run_id` bắt buộc là chuỗi **64-hex** (`is_valid_job_id`); nếu bỏ trống khi bắt đầu lượt mới, server cấp qua `auth::generate_random_token()` và chèn dòng `operation_jobs` (`state = 'running'`); nếu sai cú pháp trả `400 Bad Request`.
     - Khi client truyền tiếp `run_id` ở các batch sau: tìm hàng `operation_jobs`; không thấy -> `404 Not Found`; nếu `state != 'running'` (đã `cancelled`, `interrupted`, `succeeded`, `failed`) -> `409 Conflict` (`"regenerate_run_not_active"`).
     - Trong khi `!done`, `operation_jobs.state` giữ `'running'` và cập nhật lũy tiến `summary_json` (`{ total_processed, total_succeeded, total_failed, total_skipped_concurrent, last_after_id }`); khi `done == true`, chuyển sang `'succeeded'` bằng `transition_job_cas`.
   - Cấu trúc `RegenerateVariantsResponse` trả về số đếm **của batch hiện tại** `{ run_id, batch_processed, batch_succeeded, batch_failed, batch_skipped_concurrent, failed_ids, remaining, next_after_id, done }` kèm trường `cumulative` phản ánh tổng từ `summary_json`.
   - Khi đọc từng ảnh gốc trong batch: lấy `MediaReadLease` ngắn để đọc bytes an toàn từ đĩa (nhả lease trước khi chạy CPU encode dưới `DECODE_SLOTS`), nén trong bộ nhớ rồi ghi nguyên tử trực tiếp vào thư mục đích `uploads/variants/{shard}/{id}/v{version}/` qua `write_atomic_overwrite` (temp sibling + sync + rename), rồi trong `BEGIN IMMEDIATE` + `AdmittedMutation` kiểm tra CAS `WHERE id = ? AND generation = ? AND version = ? AND lifecycle_state = 'active'` (`rows_affected() == 0` -> dọn file tạm vừa tạo `remove_paths_best_effort` và tính vào `batch_skipped_concurrent`; lỗi đọc đĩa cá nhân -> đưa vào `failed_ids` và **vẫn tịnh tiến `next_after_id`**).

---

### 2.5 HTTP API DTOs, Headers & Routing Contracts (`/api/admin/media/...`)

1. **DTOs chuẩn hóa (`opendash-core::media` & `storage/media.rs`):**
   - `MediaVariantSpec`: `Thumbnail | Medium | Large | Full`.
   - `MediaVariantFormat`: `Jpeg | Png | Webp`.
   - `MediaVariantView`: `{ spec, format, width, height, byte_size, url }`.
   - `MediaAsset`: thêm `content_sha256: String`, `variants_ready_version: i64`, `updated_at: String` (bên cạnh `generation: i64`, `version: i64` đã có).
   - `MediaAssetDetails`: `#[serde(flatten)] pub asset: MediaAsset`, `variants: Vec<MediaVariantView>`, `webp_srcset: Option<String>`, `fallback_srcset: Option<String>`, `etag: String`.
   - `MediaVariantsStatus`: `{ total_images: i64, ready_images: i64, pending_images: i64, variant_files_bytes: i64, active_run_id: Option<String> }` (`variant_files_bytes` tính O(1) qua `SELECT COALESCE(SUM(byte_size), 0) FROM media_variants`).
   - `RegenerateVariantsRequest`: `{ after_id: Option<String>, batch_size: Option<u32>, force: Option<bool>, run_id: Option<String> }` (`batch_size` kẹp `1..=25`, mặc định `15`).
2. **Routes, Guard RBAC, Body Limit, `ETag` & `Cache-Control` (`routes/media/`, `routes/public_content/`, `app.rs`):**
   - **Public (`routes/public_content/` & `app.rs:132-134`):**
     - `GET|HEAD /media/{id}/file`
     - `GET|HEAD /media/{id}/thumbnail`
     - `GET|HEAD /media/{id}/variant/{spec_ext}`
   - **Admin (`routes/media/` & `app.rs:646-675` — 100% tiền tố `/api/admin/media/...`):**
     - `POST /api/admin/media` (`require_session_with_csrf`) — giữ nguyên quyền Editor + Admin.
     - `POST /api/admin/media/video` (`require_session_with_csrf`) — giữ nguyên quyền Editor + Admin.
     - `GET /api/admin/media` (`require_session`) — trả `Vec<MediaAsset>`.
     - `GET /api/admin/media/{id}` (`require_session`) — cho phép Editor/Admin xem chi tiết `MediaAssetDetails` kèm `variants`/`ETag` (Replace & Delete vẫn khóa chặt `require_admin_with_csrf`).
     - `GET|HEAD /api/admin/media/{id}/file`, `/thumbnail`, `/variant/{spec_ext}` (`require_session`).
     - `PUT /api/admin/media/{id}/file` (`require_admin_with_csrf` trực tiếp trong handler, raw streaming `Body` + `Content-Type` + `X-File-Name`, `DefaultBodyLimit::max(MAX_VIDEO_BYTES)` `100 MiB`).
     - `GET /api/admin/media/variants/status` (`require_admin`).
     - `POST /api/admin/media/variants/regenerate` (`require_admin_with_csrf`).
   - **Phân tích `{spec_ext}` & Mã lỗi:** `^(thumbnail|medium|large|full)\.(jpg|png|webp)$`: sai cú pháp tại Admin -> **`400 Bad Request`** (thiếu row/file -> `404`); sai cú pháp hoặc thiếu tại Public -> **`404 Not Found`**.
   - **Headers, `If-None-Match` (`304`) & `Method::HEAD`:**
     - Video Public `/file`: `Cache-Control: no-store`, `ETag: "gv{generation}-v{version}"`.
     - Video Admin `/file`: `Cache-Control: private, no-store`, `ETag: "gv{generation}-v{version}"`.
     - Ảnh & Variant Public: `Cache-Control: public, no-cache`, `ETag: "gv{generation}-v{version}"` (variant gắn thêm `-{spec}-{format}`).
     - Ảnh & Variant Admin: `Cache-Control: private, no-cache`, `ETag` tương ứng.
     - Toàn bộ ảnh/thumb/variant handler kiểm tra `If-None-Match` trả `304 Not Modified` (trước khi đọc file bytes) và nhánh `*method == Method::HEAD` chỉ gọi `metadata_with_lease` (`fs::metadata`) trả header + thân rỗng.

---

### 2.6 Tích hợp C-SEC `od_image()`, `document.rs` Preview Policy, `content_render.rs` Purity, Builder CSS & Admin UI

1. **C-SEC `od_image()` & Provenance Không Giả Mạo (`template_sec.rs`):**
   - Dựa trên hệ thống kiểu của [`unified-field-engine.md`](unified-field-engine.md), `cms/crates/opendash-server/src/field_hydrate.rs` (crate root) cùng `storage/fields.rs::load_media_hydration_map_in_conn` cấp `HydratedMedia` đầy đủ `version`, `variants`, `webp_srcset`, `fallback_srcset`.
   - **Chữ ký hàm Template & Whitelist AST:**
     `od_image(media, sizes="100vw", class="", alt="", loading="lazy", decoding="async", fetchpriority="")` (hỗ trợ đối số vị trí `media` + named kwargs hoặc literal tùy chọn).
     - Bổ sung `parse_od_image_call` vào AST `Expr::Call` whitelist tại `template_sec.rs:2847-2855` (`validate_stmt_with_icons`), `:3102`, `:4797-4817` (`walk_theme_stmt_with_url_scopes`), và `validate_block_template_csec` / `walk_block_ast_stmt` (cho cả Theme lẫn StudioBlock).
     - Đối số `media` (arg 0) bắt buộc xác minh tĩnh ở pha AST thuộc kiểu `FieldTemplateNode::Media` qua `ThemeUrlPolicy` / `BlockSchemaTemplateContext` / `UrlScopeStack`.
     - Các tham số `sizes`, `class`, `loading`, `decoding`, `fetchpriority` bắt buộc là string literal kiểm tra ngay ở pha compile: `loading ∈ {"lazy", "eager"}`, `decoding ∈ {"async", "sync", "auto"}`, `fetchpriority ∈ {"", "high", "low", "auto"}`, `class` qua `valid_icon_class_attr`, `sizes` ASCII `1..=256` bytes chỉ gồm `[a-zA-Z0-9 ,.():%-]`. `alt` có thể là literal hoặc biểu thức text safe-escaped.
   - **Opaque Provenance Chống Giả Mạo Runtime:**
     - Để ngăn mẫu như `{% set fake = {"id": "..."} %}` lách kiểm tra AST hoặc cấu trúc Map, bọc mỗi hydrated media field được phép trong đối tượng MiniJinja mờ không thể tự dựng từ template (`SafeMediaObject` cài `minijinja::value::Object` downcast trong `od_image()`), đồng thời giữ hiển thị các thuộc tính đọc an toàn khi template truy cập field bình thường. Nếu `SafeMediaObject` rỗng (`None`) hoặc `kind == "video"`, `od_image()` trả về `""`.
   - **Ngân sách & Cài đặt `ImageNonceTable` 1-1 với `IconNonceTable`:**
     - Hằng `IMAGE_NONCE_PREFIX = "OD_IMAGE_"`, cấm xuất hiện chuỗi `OD_IMAGE_` trong source/data; `MAX_IMAGES_PER_REQUEST = 128`; trừ dung lượng HTML đầu ra vào giới hạn `MAX_RENDERED_HTML_BYTES` (`512 KiB`) / `RenderBudget`; cấm gọi bên trong `{% set %}`/`{% set %}...{% endset %}`/filters (parity với `reject_icons_in_set_block`).
     - Pha `finalize_document_images` thay thế mỗi nonce **tối đa 1 lần (`<= 1`)** với trần `MAX_ICON_DOCUMENT_BYTES` (`4 MiB`).
     - Gọi `finalize_document_images` tại đúng **8 điểm** hiện đang gọi `finalize_document_icons`: `routes/public_content.rs` (`:501`, `:778`, `:1109`, `:1454`), `storage/overrides.rs` (`:1846`, `:3299`, `:3530`), và `storage/builder.rs` (`:1483`).
     - **Giữ nguyên** quy tắc nghiêm ngặt cho `srcset` ở `template_sec.rs:5541-5554` (vì `build_responsive_srcsets` trả `None` khi không đủ ứng viên, không cần ngoại lệ `srcset=""`).
2. **`document.rs` Chính sách Preview vs Public (`document.rs`):**
   - Làm rõ: `document.rs:50-75` dùng danh sách chặn `is_forbidden_tag` (`<picture>` và `<source>` vốn không bị chặn). Bổ sung nhận diện `<source>` là void element trong tokenizer/normalizer và thêm bộ kiểm tra thuộc tính URL cho `<source>`/`<img>` trong `normalize_public_document_with_options`, tham số hóa bằng `allow_admin_media_preview: bool`:
     - Public (`false`): mọi URL trong `src` và từng ứng viên `srcset` chỉ chấp nhận `/media/{64-hex}/...`.
     - Admin Preview (`true`, bật từ `storage/overrides.rs:3278,3509`, `routes/content.rs:189`, `routes/entries.rs`, `routes/builder.rs`): cho phép thêm `/api/admin/media/{64-hex}/...` đã được `rewrite_preview_media_urls` sinh ra, tránh lỗi `<main data-doc-diagnostic>` xóa trắng preview.
3. **`content_render.rs` Thuần Túy (0-SQL) & Preview Media Rewriter:**
   - `content_render.rs` là renderer đồng bộ thuần túy theo [`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md): các caller trong `storage/` (`public_content.rs`, `routes/content.rs`, `routes/entries.rs`) nạp trước `HydratedMediaMap` bằng `storage::media::find_many_details_for_ids` rồi truyền `&HydratedMediaMap` vào `content_render`.
   - Làm rõ ranh giới: `opendash-core::richtext::sanitize_richtext` (`richtext.rs:209-345`) tiếp tục chặn `<img>`/`<picture>` trong RichText thông thường; nâng cấp responsive trong `content_render.rs` áp dụng cho `DEFAULT_SINGLE` / `DEFAULT_ARCHIVE`, block/layout HTML và kết hợp với `rewrite_preview_media_urls` (`:856-874`) mở rộng viết lại cả `url`, `thumbnail_url`, `webp_srcset`, `fallback_srcset`, `variants[*].url` sang `/api/admin/media/...`.
4. **Visual Builder `page.rs` & CSS Tĩnh Tất Định:**
   - `storage/builder.rs` và `storage/overrides.rs` nạp `HydratedMediaMap` từ **đủ 4 nguồn**: `extract_document_media_refs` tĩnh, kết quả phân giải `BindingTarget::MediaId`, các nested `patterns`, và giá trị `default` media ID trong schema `PluginBlock`.
   - `page.rs` hoàn toàn **0-SQL**: `KnownElement::Image` đọc `HydratedMediaMap` sinh `<picture>` / `<img>` có `srcset` + `?v={version}`; `KnownElement::Video` gắn `?v={version}` vào `src` và `poster`; `builder_responsive_css` & `serialize_scoped_block_css` chọn phương án tất định gắn `?v={version}` cho URL canonical: `url("/media/{id}/file?v={version}")` (public) hoặc `url("/api/admin/media/{id}/file?v={version}")` (preview), đồng bộ mở rộng `validate_safe_scoped_block_css` (`template_sec.rs:2651-2703`) chấp nhận `(?:\?v=[1-9][0-9]*)?`.
5. **SEO (`storage/seo.rs`), Themes (`cms/crates/opendash-server/src/builtin_themes/opendash-starter/`) & Plugin (`plugin/runtime.rs`, SDK):**
   - Mọi nhánh `project_og_image_url` trong `storage/seo.rs` (site home + post/page/entry) join `version` active xuất `/media/{id}/file?v={version}` (giữ JPEG/PNG gốc cho crawler mạng xã hội).
   - `plugin/runtime.rs::MediaGet` kiểm tra `is_public_scope` ACL (không rò rỉ media chưa publish) và cập nhật `cms/sdk/js/opendash.d.ts` + [`PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md) trả về `version`, `content_sha256`, `generation`, `?v=`, `variants`, `webp_srcset`, `fallback_srcset`.
   - Theme Starter hợp nhất `cms/crates/opendash-server/src/builtin_themes/opendash-starter/` và `cms/crates/opendash-server/src/builtin_blocks/` dùng `od_image()`, được xác nhận qua `theme.rs`.
6. **Admin UI & Sandboxed Preview (`cms/admin/`):**
   - `MediaLibraryPage.tsx`: chỉ giữ lại `selectedId` khi `refreshVersion` thay đổi sau Replace (vẫn reset khi đổi `user?.id`, `offset`, `kindFilter`). Khi chọn item hoặc Replace xong, gọi `GET /api/admin/media/{id}` (`getMedia`) lấy `MediaAssetDetails` (`variants`, `etag`). Khóa nút **Replace File** và **Regenerate Variants** sau `user?.role === 'admin'`; hiển thị thông báo rõ cho `409 readers_active` (đang có người xem/tải), `412 Precondition Failed` (tải lại chi tiết mới) và `422` (sai loại image/video). Cập nhật `mediaThumbnailUrl(id, version)` và thêm `mediaOriginalUrl(id, version)` tại mọi nơi gọi (`MediaLibraryPage.tsx`, `MediaPicker.tsx`).
   - `BuilderPreviewFrame.tsx`: lưu ý cơ chế hiện hành dùng `FileReader.readAsDataURL(blob)` (inline `data:` URL). Cập nhật cả `PRIVATE_MEDIA_PATH` và `CSS_MEDIA_URL` khớp `/api/admin/media/{id}/(file|thumbnail|variant/...)(?:\?v=\d+)?` và `/media/...`; với `<img>` private đã thay bằng `data:` URL, gỡ thuộc tính `srcset` trên `<img>` và `<source srcset="/api/admin/media/...">` lân cận; với ảnh public `/media/...` trong `srcdoc`, mở rộng `urlAttributes` viết lại cả `img[srcset]` và `source[srcset]` sang `${resolvedOrigin}/media/...`.

---

## 3. Alternatives Considered

| Nhóm quyết định | Phương án đã chọn | Phương án thay thế đã cân nhắc & Lý do loại |
| --- | --- | --- |
| **1A vs 1B (WebP Encoder)** | `1A`: `webp =0.3.1` encode-only sau pure-Rust decode | `1B`: Tự viết encoder VP8 trong `image-webp` -> Chất lượng nén kém, rủi ro lỗi bitstream cao; ranh giới encode-only giữ an toàn tuyệt đối cho parser. |
| **2A vs 2B (Variants & `srcset`)** | `2A`: Sinh sẵn `320` / `768` / `1536` + `full` WebP không upscale | `2B`: Cache sinh theo request đầu -> Đòi hỏi khóa đĩa trên luồng đọc, dễ bị DoS resize và phình ổ đĩa khó dự đoán. |
| **3A vs 3B (Replace Media)** | `3A`: Replace giữ 64-hex `id` với `If-Match`, `MediaReadGate`, journal & `.quarantine/` phẳng | `3B`: Tạo `id` mới + `media_redirects` (`301`) -> Gãy toàn bộ đối chiếu ID trong DB/Builder/CSS và phát sinh chuỗi redirect chậm LCP. |
| **4A vs 4B (Backup / Storage)** | `4A`: Bỏ qua `uploads/variants/` khi Backup, giữ nguyên `BackupManifestV1` & giới hạn 20,000 | `4B`: Đóng gói biến thể vào `.zip` backup -> Tăng kích thước snapshot 4–6x dù biến thể tái tạo được 100%. |
| **Phạm vi `0030` vs `8192px`** | Giữ nguyên giới hạn `6000x6000` (ảnh) và `3840x2160` (video) của `0030` | Rebuild `media_assets` nâng lên `8192` -> Tăng đột biến ngưỡng giải mã C-heap/RAM và phát sinh migration rebuild bảng nặng không cần thiết. |
| **Phụ thuộc `unified-field-engine`** | Đồng bộ trực tiếp trên `S02`–`S03` đã merge (`0055`, `field_hydrate.rs`, `storage/fields/`, `opendash-starter/`) và kiến trúc Feature-First `<= 600 LOC` trên `main@43c19c2` | Giữ kiến trúc module đơn tệp cũ -> Vi phạm kiểm tra `loc-baseline.json` và bỏ qua `HydratedMediaRow` / `BuilderRenderContext.hydration_media` có sẵn. |

---

## Detailed Implementation Plan

### 4.1 Hợp đồng Thực thi & Ma trận Ghi M8 (`M8_WRITER_INVENTORY.md` Alignment)

Tuân thủ [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md) và [`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md):
- **Upload mới (`POST /api/admin/media`, `/video`):** `require_session_with_csrf` -> staging + decode ngoài lease -> xin `AdmittedMutation` ngắn hạn -> `BEGIN IMMEDIATE` (`allocate_or_save_with_variants`) -> promote + `COMMIT`.
- **Replace Media (`PUT /api/admin/media/{id}/file`):** `require_admin_with_csrf` -> `If-Match` / `kind` / `Content-Length` preflight -> stream + encode vào `.staging/replace-{job_id}` -> chờ reader <= 500ms (`replacing = false`) -> xin `AdmittedMutation` ngắn hạn -> Tx 1 (`'promoting_new'`) -> `replacing = true` + swap đĩa phẳng + Tx 2 (`'completed'`) -> nhả khóa (hoặc rollback bù trừ tại chỗ nếu lỗi Đĩa/Tx 2).
- **Batch Regenerate (`POST /api/admin/media/variants/regenerate`):** `require_admin_with_csrf` -> `MEDIA_REGENERATE_ACTIVE` (`AtomicBool`) -> đọc gốc với `MediaReadLease` ngắn -> nén ngoài lease -> xin `AdmittedMutation` ngắn hạn cho từng ảnh -> `write_atomic_overwrite` + CAS `(generation, version)`.

### 4.2 Phân chia Slices (`S01`–`S04`, Chuẩn Feature-First `<= 600 LOC`)

#### Slice S01: Nền tảng Core, Migration `0058`, Nén WebP Biến thể, Upload `.staging/` & Route Variant
- **Dependencies:** `main@45888d6` (`0055`–`0057` đã có mặt).
- **File Ownership:**
  - `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-server/Cargo.toml`
  - `docs/architecture/TECH_STACK.md`
  - `cms/migrations/0058_responsive_media_and_replace.sql`
  - `cms/crates/opendash-core/src/lib.rs`, `cms/crates/opendash-core/src/media.rs`
  - `cms/crates/opendash-server/src/media/{mod.rs,types.rs,service.rs,read_gate.rs,variants.rs}`, `cms/crates/opendash-server/src/media_video/`
  - `cms/crates/opendash-server/src/storage/media/{mod.rs,types.rs,queries.rs,variants.rs,refs.rs}`, `cms/crates/opendash-server/src/storage/operations.rs`
  - `cms/crates/opendash-server/src/routes/media.rs`, `cms/crates/opendash-server/src/routes/public_content/assets_and_sitemap/`, `cms/crates/opendash-server/src/app.rs`
  - `cms/crates/opendash-server/tests/site_media_seo.rs`, `cms/crates/opendash-server/tests/site_media_seo/responsive_media_variants.rs`
- **Công việc chi tiết:**
  - `opendash-core::media` (thuần Rust, không I/O): `MediaVariantSpec`, `MediaVariantFormat`, `MediaVariantView`, `QueuedMediaVariant`, `parse_variant_spec_ext`, `parse_media_etag_token`, `format_media_etag`, `build_responsive_srcsets`.
  - `media/`: chuyển `staging_dir()` sang `uploads/.staging`, sửa `cleanup_staging()` hỗ trợ `remove_dir_all`, hoist `DECODE_SLOTS`/`VIDEO_INGEST_SLOTS`, fix lost-wakeup `Notified::enable()` trong `media/read_gate.rs`, refactor `read_with_lease` / `metadata_with_lease` / `read_variant_with_lease` (`NotFound -> 404`), cập nhật `stream_authorized_video` dùng `_lease` + `ETag: "gv{gen}-v{ver}"`; thêm `write_atomic_overwrite` và nén WebP encode-only tại `media/variants.rs`.
  - `storage/media/` (tách feature-first `<= 600 LOC`) & `storage/operations.rs`: cập nhật `MediaAsset`, `MediaAssetDetails`, `allocate_or_save_with_variants` cho cả image & video (giải quyết va chạm `actual_id`), `find_many_details_for_ids`, thêm `OperationKind::MediaRegenerate` (`is_heavy() == false`).
  - `routes/media.rs`, `routes/public_content/assets_and_sitemap/`, `app.rs`: route `/media/{id}/variant/{spec_ext}` (public) và `/api/admin/media/{id}/variant/{spec_ext}` (admin `require_session`), lấy `acquire_read_lease(&id).await?` 1 lần trước DB query, nhánh `Method::HEAD` + `If-None-Match` (`304`); gắn test mới vào domain binary `tests/site_media_seo.rs`.
- **Acceptance Criteria:**
  1. `0058` migrate sạch từ `0057`, giữ nguyên cột/index `0030`/`0032`/`0033` và tổng schema objects là `234 <= 256`.
  2. Upload JPEG / opaque PNG / alpha PNG (`<= 1 MP` & `> 1 MP`) / WebP sinh biến thể đúng quy tắc size-gate, không upscale, không tạo `thumbnail.{jpg,png}` trong `uploads/variants/`.
  3. Re-upload ảnh hoặc MP4 có SHA trùng với bản gốc của asset đã từng replace sang SHA mới tạo `actual_id` mới thành công, không trả nhầm asset mới.
  4. `HEAD` & `If-None-Match` hoạt động đúng trên cả `/media/...` và `/api/admin/media/...`, toàn bộ file nguồn mới tuân thủ `<= 600 LOC` và kiểm thử thuộc domain binary `site_media_seo`.
- **Rollback:** Revert PR của `S01` trước khi merge/triển khai `S02`–`S04`.

#### Slice S02: Replace Media (`3A`), Phục hồi Startup, Reconcile, Backup/Restore (`4A`), Sample Import & Batch Regenerate
- **Dependencies:** `S01`
- **File Ownership:**
  - `cms/crates/opendash-server/src/media/{service.rs,replace.rs}`, `cms/crates/opendash-server/src/media_video/`
  - `cms/crates/opendash-server/src/storage/media/`, `cms/crates/opendash-server/src/routes/media.rs`, `cms/crates/opendash-server/src/app.rs`
  - `cms/crates/opendash-server/src/media_lifecycle/`, `cms/crates/opendash-server/src/storage/media_lifecycle.rs`, `cms/crates/opendash-server/src/storage/media_refs.rs`
  - `cms/crates/opendash-server/src/storage/operations.rs`, `cms/crates/opendash-server/src/routes/operations/`
  - `cms/crates/opendash-server/src/main.rs`
  - `cms/crates/opendash-server/src/backup_worker/`, `cms/crates/opendash-server/src/storage/backup.rs`, `cms/crates/opendash-server/src/restore.rs`, `cms/crates/opendash-server/src/storage/restore.rs`, `cms/crates/opendash-server/src/storage/import/`
  - `cms/crates/opendash-server/tests/ops.rs`, `cms/crates/opendash-server/tests/ops/media_replace_and_lifecycle.rs`
- **Công việc chi tiết:**
  - `PUT /api/admin/media/{id}/file`: `require_admin_with_csrf`, raw streaming `Body` + `DefaultBodyLimit::max(MAX_VIDEO_BYTES)`, preflight sớm (`If-Match` `428`/`412`, same `kind` `422`, `Content-Length`), `ReplaceStagingGuard`, chờ reader `<= 500ms` (`409 readers_active`), `AdmittedMutation` ngắn hạn, Tx 1 `'promoting_new'`, `replacing = true` swap file phẳng `.quarantine/{job_id}/`, Tx 2 `'completed'`, rollback bù trừ đầy đủ khi lỗi Đĩa hoặc Tx 2, dọn `v{old_ver}/` sau khi nhả `replacing`.
  - `main.rs`: dọn trùng lặp `mark_interrupted_on_startup`, gọi `recover_pending_replace_jobs_on_startup` (unlink cả `{id}.{new_extension}` & `v{new_version}/` trước khi hoàn tác `.quarantine/`) **trước** `cleanup_staging()`.
  - `media_lifecycle/` & `storage/{media_lifecycle,media_refs,operations}.rs`: sửa `tombstone_media_in_tx` (`BEGIN IMMEDIATE` + chặn replace active) & `execute_delete_job_inner` (xóa `uploads/variants/{shard}/{id}`), `scan_uploads_tree` (`.staging` + `uploads/variants/`), `MAX_RECONCILE_FILES = 100_000`, `MediaInventoryRow` + kiểm `media_variants`, chunked `find_{delete,replace}_jobs_by_ids` thay `LIMIT 200`, `purge_quarantine` & prune terminal replace jobs 7d, `prune_old_receipts` (`AND kind != 'backup'`).
  - `storage/backup.rs` (`collect_backup_inventory` chặn active replace job), `backup_worker/` (bỏ qua `uploads/variants/` & `uploads/.staging/`), `storage/restore.rs::validate_and_rebase_copy` (reset `media_variants`, `variants_ready_version = 0`, `media_replace_jobs` trước `verify_restored_db_references`).
  - `storage/import/`: giữ `dry_run` read-only, mở rộng `StagedMediaEntry` sinh biến thể & chống đụng `actual_id`, dọn `uploads/variants/` khi recover import lỗi.
  - `GET /api/admin/media/variants/status` (`require_admin`) & `POST /api/admin/media/variants/regenerate` (`require_admin_with_csrf`, `64-hex` `run_id`, `operation_jobs` stateful `'running' -> 'succeeded'`, `MediaReadLease` đọc gốc, `failed_ids` tiến `next_after_id`, CAS DB).
- **Acceptance Criteria:**
  1. Replace ảnh/video giữ nguyên ID, rollback sạch cả khi giả lập fail tại Tx 2 hoặc mất điện ở `promoting_new` (kể cả khi đổi extension PNG -> WebP).
  2. Reconcile xử lý > 200 mục `.quarantine/`, phân tách orphan `Canonical` vs `Variant`; xóa asset không để lại rác trong `uploads/variants/`.
  3. Backup bỏ qua `uploads/variants/`; Restore reset sạch trước `verify_restored_db_references`; Regenerate hợp tác không làm mất receipt `backup`.
- **Rollback:** Revert PR của `S02`.

#### Slice S03: C-SEC `od_image()`, Post-UFE Render, `document/` Preview Policy, Builder, SEO, Theme & Plugin
- **Dependencies:** `S01` (nối trên `S02`–`S03` của `unified-field-engine` đã merge)
- **File Ownership:**
  - `cms/crates/opendash-server/src/field_hydrate.rs`
  - `cms/crates/opendash-server/src/storage/fields/`, `cms/crates/opendash-server/src/storage/builder/`, `cms/crates/opendash-server/src/storage/overrides/`, `cms/crates/opendash-server/src/storage/settings/`, `cms/crates/opendash-server/src/storage/public_content/`, `cms/crates/opendash-server/src/storage/seo/`
  - `cms/crates/opendash-server/src/template_sec/{mod.rs,nonces.rs,css.rs,policy/}`, `cms/crates/opendash-server/src/document/`, `cms/crates/opendash-server/src/theme/`
  - `cms/crates/opendash-server/src/content_render/`, `cms/crates/opendash-server/src/page/`
  - `cms/crates/opendash-server/src/routes/public_content/`, `cms/crates/opendash-server/src/routes/content.rs`, `cms/crates/opendash-server/src/routes/entries/`, `cms/crates/opendash-server/src/routes/builder.rs`, `cms/crates/opendash-server/src/routes/themes/`
  - `cms/crates/opendash-server/src/plugin/runtime/`, `cms/crates/opendash-server/src/plugin/tests/`
  - `cms/sdk/js/opendash.d.ts`, `docs/contracts/PLUGIN_ABI.md`
  - `cms/crates/opendash-server/src/builtin_blocks/`, `cms/crates/opendash-server/src/builtin_themes/opendash-starter/`
  - `cms/crates/opendash-server/tests/theme.rs`, `cms/crates/opendash-server/tests/theme/responsive_media_render.rs`
- **Công việc chi tiết:**
  - `field_hydrate.rs` & `storage/fields/`: mở rộng `HydratedMediaRow` / `hydrate_media_row` đầy đủ `version`, `variants`, `webp_srcset`, `fallback_srcset`.
  - `template_sec/` & `theme/`: cài `parse_od_image_call`, `SafeMediaObject`, `ImageNonceTable` (`OD_IMAGE_`, `MAX_IMAGES_PER_REQUEST`, `RenderBudget`, cấm `Set`/`SetBlock`), gọi `finalize_document_images` tại đủ 8 điểm (`routes/public_content/`, `storage/overrides/`, `storage/builder/`), mở rộng `validate_safe_scoped_block_css` cho `?v=\d+`.
  - `document/`: bổ sung `<source>` void element & kiểm tra `srcset` URL theo cờ `allow_admin_media_preview` (chỉ mở `/api/admin/media/` trong preview).
  - `content_render/`: nhận `&HydratedMediaMap` đồng bộ từ `storage/` callers (0-SQL), render responsive cho `DEFAULT_SINGLE`/`DEFAULT_ARCHIVE` & cập nhật `rewrite_preview_media_urls` đầy đủ `srcset`/`variants`.
  - `page/` + `storage/{builder,overrides}/`: tận dụng & mở rộng `BuilderRenderContext.hydration_media` đủ 4 nguồn (static refs, `BindingTarget::MediaId`, nested `patterns`, `PluginBlock` defaults); `page/` cập nhật `Image`, `Video` (`src` + `poster`) và CSS canonical `url(...?v={version})`.
  - `storage/seo/`, `plugin/runtime/` (`MediaGet` kiểm ACL + `?v=`), SDK & `PLUGIN_ABI.md`, `builtin_blocks/` & `builtin_themes/opendash-starter/`.
- **Acceptance Criteria:**
  1. `od_image()` từ chối map giả tạo từ template (`{% set fake = ... %}`), từ chối chuỗi thô, chống nonce forgery/reuse và vượt ngưỡng kích thước.
  2. Admin preview với `/api/admin/media/...` trong `srcset` vượt qua `document/` bình thường (không bị fallback `<main data-doc-diagnostic>`).
  3. `page/` và `content_render/` giữ 0 SQL/async query; SEO OG URLs & `MediaGet` plugin nhất quán `?v={version}`.
- **Rollback:** Revert PR của `S03`.

#### Slice S04: Admin UI, Replace UX, Multi-Batch Regenerate & Sandboxed Preview Fix
- **Dependencies:** `S01`, `S02`
- **File Ownership:**
  - `cms/admin/src/api/media.ts`, `cms/admin/src/api/operations.ts`
  - `cms/admin/src/features/media/` (`MediaLibraryPage.tsx`, `MediaPicker.tsx`, `library/fileValidation.ts`)
  - `cms/admin/src/features/operations/ReconcilePanel.tsx`
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx`
  - `cms/admin/src/i18n/`
  - `cms/admin/package.json`, `cms/admin/scripts/test-responsive-media-preview.mjs`
- **Công việc chi tiết:**
  - Cập nhật `api/media.ts`, `api/operations.ts`, `mediaThumbnailUrl(id, version)` và `mediaOriginalUrl(id, version)`.
  - `MediaLibraryPage.tsx`: giữ `selectedId` chỉ theo `refreshVersion` (vẫn xóa khi đổi `offset`/`kindFilter`/`user?.id`); gọi `getMedia(id)` lấy `MediaAssetDetails`; khóa Replace & Regenerate theo `user?.role === 'admin'`; xử lý tường minh lỗi `409 readers_active`, `412`, `422`.
  - `ReconcilePanel.tsx` & UI Regenerate: chạy tuần tự theo batch `15` truyền `run_id` & `next_after_id`, hỗ trợ Dừng (cancel job); tách hiển thị orphan `Canonical` vs `Variant`.
  - `BuilderPreviewFrame.tsx`: khớp `PRIVATE_MEDIA_PATH` & `CSS_MEDIA_URL` cho `?v=\d+` và `/variant/{spec_ext}`, xử lý `img[srcset]` / `source[srcset]` (gỡ private `srcset` sau khi resolve `FileReader.readAsDataURL`, viết lại public `/media/` `srcset` sang `${resolvedOrigin}/media/...`).
  - `cms/admin/package.json` thêm `"test:media-preview": "node ./scripts/test-responsive-media-preview.mjs"` (không dùng `vitest`/`jsdom`), đồng bộ i18n `en`/`vi`.
- **Acceptance Criteria:**
  1. Replace File trong UI chỉ hiện với `admin`, giữ nguyên ngăn chi tiết sau khi cập nhật và hiện bảng biến thể + ETag mới.
  2. Preview `BuilderPreviewFrame` dựng đúng cả ảnh private (có `data:` URL & không lỗi 401 `srcset`) lẫn ảnh public `srcset` và CSS `background-image`.
  3. `npm run typecheck`, `npm run build`, `npm run test:fields`, `npm run test:media-preview` và `check-i18n.mjs --strict` pass.
- **Rollback:** Revert PR của `S04`.

---

### 4.3 Boundary Test Matrix

#### Boundary Test Matrix

| Nhóm kiểm thử | Ca biên bắt buộc kiểm chứng | Kỳ vọng hệ thống |
| --- | --- | --- |
| **Biến thể & Size-gate (`S01`)** | Ảnh `<= 320w`, `500w`, `768w`, `769w`, `1536w`, `1537w`, `6000w` (`0030` max); JPEG, opaque PNG (`min(jpg, png)`), RGBA PNG `<= 1 MP` vs `> 1 MP`, WebP nguồn | Không upscale; không ghi `thumbnail.{jpg,png}` vào `variants/`; đúng 1 fallback non-WebP mỗi cỡ; `webp_srcset` nguồn WebP kèm `{file_url} {orig_w}w` khi có `medium`/`large` |
| **Upload & Đụng ID (`S01`)** | File hỏng / > `6000w` / decompression bomb; upload lại file ảnh hoặc video SHA `S1` sau khi Asset A (`id=S1`) đã replace sang `S2` | Lỗi giải mã không tạo rác DB/ổ đĩa; re-upload `S1` (cả ảnh & video) sinh `actual_id != S1` an toàn trong `BEGIN IMMEDIATE` |
| **Replace CAS & Drain (`S02`)** | Thiếu `If-Match`, sai ETag, đổi chéo `image <-> video`, vượt `8 MiB`/`100 MiB`, đang stream > `500ms` | Lần lượt trả `428`, `412`, `422`, `413` trước khi chạm live files, và `409 readers_active` không nghẽn độc giả |
| **TOCTOU, Đĩa Fail & Crash (`S02`)** | Đọc đồng thời trong critical section; giả lập fail Tx 2 sau khi đã đổi tên đĩa; giả lập crash ở `'promoting_new'` khi đổi `png -> webp` | Không bao giờ lộ cặp DB/file lệch version; fail Tx 2 hoàn tác sạch file cũ; startup recovery trước `cleanup_staging()` unlink `.webp` mới & khôi phục `.png` cũ |
| **Reconcile, Delete, Backup, Restore (`S02`)** | Delete asset có nhiều biến thể; > `200` job trong `.quarantine/`; Backup -> Replace -> Restore snapshot có job replace lơ lửng; Import mẫu đụng SHA | `media_delete` xóa sạch `uploads/variants/{shard}/{id}`; Reconcile không sót; Restore dọn trong `validate_and_rebase_copy` trước verify; Import giữ `Vec<StagedMediaEntry>` |
| **Regenerate Hợp tác (`S02`)** | File hỏng giữa batch; 2 batch song song; client gửi tiếp `run_id` đã bị cancel/interrupted; > 200 lượt regenerate vs lịch sử backup | Batch 2 nhận `409`; `failed_ids` tịnh tiến `next_after_id`; `run_id` đã hủy trả `409 regenerate_run_not_active`; receipt `backup` còn nguyên |
| **C-SEC, Preview & UI (`S03`–`S04`)** | `od_image()` với `{% set fake = {"id": ...} %}` hoặc chuỗi 64-hex; nonce reuse / `OD_IMAGE_` literal; preview `/api/admin/media/...` qua `document/`; Editor vs Admin RBAC; `BuilderPreviewFrame` | Từ chối giả mạo `SafeMediaObject` & raw hex; Preview qua `document/` an toàn; Editor dùng upload & `GET /{id}` nhưng UI ẩn Replace/Regenerate; Preview xử lý cả `CSS_MEDIA_URL` & `img/source[srcset]` |

### Verification Plan

```powershell
# Kiểm tra quy chuẩn mã nguồn, kế hoạch & i18n
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
node cms/scripts/check-i18n.mjs --strict

# Format & Frontend typecheck + build + Node unit tests
cargo fmt --manifest-path cms/Cargo.toml --all --check
npm --prefix cms/admin ci
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
npm --prefix cms/admin run test:fields
npm --prefix cms/admin run test:media-preview

# Tầng 1: Chạy các miền test liên quan theo thay đổi (dùng danh sách nhóm cách nhau dấu phẩy)
python3 cms/scripts/run-tests.py --changed
python3 cms/scripts/run-tests.py --group site-media-seo,ops,theme,builder,content,plugin

# Tầng 2: Kiểm chứng đầy đủ bản release trước khi đóng Spec / Release
python3 cms/scripts/run-tests.py --group all --release
git diff --check
```
