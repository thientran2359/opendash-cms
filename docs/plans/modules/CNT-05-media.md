# CNT-05 — Media Library · Living Module Brief

**Scope:** PRD V1 `AGREED` / ranh giới module đã thống nhất · **Milestone dự kiến:** M3 / `M3-S04` · **PRD:** [Content/media](../../product/PRODUCT_REQUIREMENTS.md) · **Track:** `Full-Track` vì upload, quyền, filesystem/SQLite và public serving.

**Trạng thái hiện tại:** Research `REVIEWED` (source/CodeGraph tại `main@2bad893` và phản biện Codex ↔ Antigravity đến Turn 5, 07/11/2026) · Design `NOT_STARTED` · Implementation `NOT_STARTED` · Verification `NOT_RUN`; capability và task dưới đây mới là dự kiến.

Living Module Brief G0–G3 giữ một nguồn chi tiết từ scope đến task. V1 cần ảnh/media upload và image/gallery picker; video là phần cần xác minh mức hỗ trợ/giới hạn khi G2. **Opaque `media_id` thay URL/path trong field là ứng viên G1**, chưa chốt kiểu ID, schema metadata, public route hay cơ chế dọn file. Xem [planning consensus](../../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) và [Pre-M3 readiness](../../discussions/threads/2026-11-07-pre-m3-codegraph-readiness.md).

## G0 — Năng lực cần có

| ID | Capability | Primary acceptance slice dự kiến | Nghiệm thu dự kiến |
| --- | --- | --- | --- |
| `CNT-05.1` | Upload và quản lý media bằng Admin | `M3-S04` | File hợp lệ được lưu, metadata còn sau restart, quyền upload được kiểm tra |
| `CNT-05.2` | Chọn media cho image/gallery fields và Builder | `M3-S04` (picker/field); Builder reuse ở M4 | Field dùng reference ổn định; chọn một/nhiều ảnh, hiển thị đúng |
| `CNT-05.3` | Phục vụ media public có kiểm soát | `M3-S04` | Không đọc tệp ngoài site root, URL/media type đúng, xử lý file thiếu |
| `CNT-05.4` | Tích hợp media với backup/restore website | `M8` (OPS-01 integration) | Metadata/files vẫn nhất quán sau restore, không mất content refs |

## G0 — Dependency và phạm vi

- **Cần trước:** FND-02 (Auth/RBAC), FND-03 (site paths/SQLite), G2 reference contract giữa CNT-02 và CNT-05. M3-S02 xác định schema image/gallery trước khi nối picker.
- **Cung cấp cho:** CNT-02 image/gallery fields, SITE-02 OG image, THM-03 sample import và OPS-01 backup.
- **Ngoài phạm vi:** chỉnh sửa media nâng cao, xử lý video background, CDN/transform service, media search/filter nâng cao. Không tự chọn thư viện xử lý ảnh hoặc mở rộng V1 nếu chưa có nhu cầu rõ.

## G1 — Research đã review (ứng viên cho G2)

1. **Mô hình:** file nằm dưới `uploads/` như thiết kế site root; so sánh opaque `media_id` với URL/path, lựa chọn định danh (UUID/ULID/key) theo convention thật và khôi phục sau đổi domain/path.
2. **Upload security:** các định dạng và kích thước cho phép; MIME/magic bytes, safe filename, path traversal, symlink, collision, giới hạn dung lượng, quyền và CSRF.
3. **Consistency:** ghi SQLite và filesystem không có transaction chung. Cách staging, finalize/cleanup và phục hồi từ lỗi giữa chừng là gì?
4. **Reference:** media bị xóa/thay, bị thiếu file, trùng nội dung, hoặc được dùng ở nhiều field thì API phản hồi sao?
5. **Public delivery:** URL generation, Content-Type, cache headers, access và HTTP range cho video có cần ngay ở V1?
6. **Backup/restore:** snapshot file/media metadata đồng bộ thế nào, có bảo toàn binary và link khi đổi đường dẫn install?
7. **Integration:** CNT-02 định nghĩa image/gallery schema và Admin picker sử dụng Media API nào; UX cho nhiều ảnh và video ra sao?

**Đầu ra G1:** sơ đồ file + metadata + API, tradeoff reference, threat model upload, test lỗi cần mô phỏng, dependency với CNT-02/OPS-01. Ghi tóm tắt tại đây; chỉ tách `docs/research/` khi có đánh giá lớn.

### G1.1 — Kiểm kê source và luồng sử dụng (đã khảo sát và phản biện)

**Cách làm:** dùng CodeGraph `symbol SitePaths`, `symbol build_router`, `symbol rendered`, `symbol init_db`, `callgraph` trên `main.rs:20`, `storage/mod.rs:13`, `app.rs:58`, `routes/public.rs:10`, `routes/builder.rs:101`; sau đó đọc source thật và tìm `uploads_dir`, `DefaultBodyLimit`, `Content-Security-Policy`. Graph phát hiện đúng `main → init_db/build_router`, `public_page → load_home_page/render_block/rendered`, nhưng suy dư các lời gọi method trùng tên như `new/get`; **file:line bên dưới mới là chứng cứ source**.

| Phạm vi / owner | Có sẵn (`OBSERVED IN SOURCE`) | Khoảng trống thực tế |
| --- | --- | --- |
| **FND-03 — Paths** | [`paths.rs:17-90`](../../../cms/crates/opendash-server/src/paths.rs) tạo `SitePaths` từ site root, có `uploads_dir()`, `backups_dir()`, `staging_dir()`; [`main.rs:28-40`](../../../cms/crates/opendash-server/src/main.rs) dùng `db_path()` để mở SQLite. | [`AppState` ở `app.rs:29-51`](../../../cms/crates/opendash-server/src/app.rs) chỉ giữ DB, PluginManager và hooks, chưa truyền `SitePaths` tới route. Chưa có upload service; staging **chưa đồng nghĩa đã có atomic upload**. |
| **FND-03 — SQLite** | [`storage/mod.rs:13-34`](../../../cms/crates/opendash-server/src/storage/mod.rs) bật FK/WAL và SQLx migrations 0001–0003. | Chưa có bảng `media` hay `storage/media.rs`; metadata, reference state, checksum/file reconciliation và backup integration chưa hiện thực. |
| **HTTP upload** | [`app.rs:58-90`](../../../cms/crates/opendash-server/src/app.rs) đặt `DefaultBodyLimit::max(MAX_ZIP_BYTES)` ở router; `MAX_ZIP_BYTES = 64 * 1024` trong [`theme.rs`](../../../cms/crates/opendash-server/src/theme.rs), plugin install/run có override riêng. [`auth.rs:213-263`](../../../cms/crates/opendash-server/src/auth.rs) cung cấp guard Admin/Editor + CSRF. | Chưa có `routes/media.rs`, upload multipart/streaming/body-size policy; mỗi endpoint media cần giới hạn phù hợp và test `413`, không bỏ global guard theo suy đoán. Cần chốt Editor có được upload không dựa trên PRD. |
| **Public media và preview** | [`routes/public.rs:18-30`](../../../cms/crates/opendash-server/src/routes/public.rs) dùng `CSP: sandbox; default-src 'none'`, `nosniff`; [`admin/src/App.tsx:318-350`](../../../cms/admin/src/App.tsx) còn chèn `<meta http-equiv=Content-Security-Policy content=default-src 'none'>` vào preview HTML. | Hiện chưa có endpoint serve bytes, URL media hay chính sách CSP cho ảnh/video. Không chỉ sửa public `img-src`: cần kiểm preview `srcdoc`, sandbox và trusted origins; không nới CSP mặc định cho mọi route trước G2. |
| **FND-01 — ZIP safety** | [`package.rs:10-80`](../../../cms/crates/opendash-server/src/package.rs) tái sử dụng guard ZIP file/entry/bounded read của theme/plugin. | Guard ZIP **không thay cho** upload media: phải kiểm magic bytes/MIME, file path, symlink, size, xử lý định dạng nguy hiểm và stream resource limits riêng. |
| **CNT-02 / BLD-05 / OPS-01** | [PRD](../../product/PRODUCT_REQUIREMENTS.md) yêu cầu image/gallery fields, Builder binding và backup/restore thủ công; [Admin features README](../../../cms/admin/src/features/README.md) đặt Media UI ở M3. | Picker/reference/thumbnail, xóa media còn field tham chiếu, backup đồng bộ file/DB và error UX chưa được code; M8 mới nghiệm thu backup integration theo [Roadmap](../ROADMAP_V1.md). |

### G1.2 — Phương án MediaRef và dữ liệu cần so sánh (**chưa chốt G2**)

| Vấn đề | Phương án A | Phương án B | Nhận định và test cần thiết |
| --- | --- | --- | --- |
| **Field image/gallery lưu gì** | Một **opaque `media_id`** (gallery là danh sách ID); SQLite metadata dẫn tới storage key/file; URL sinh lúc serve. | Lưu URL/path tương đối hoặc storage key trong field (kèm metadata tùy chọn). | A bền vững hơn trước đổi domain/di chuyển uploads và có thể quản lý missing/deleted refs, nhưng thêm bước resolve/DB và chính sách uniqueness; B nhanh hơn nhưng dễ lộ đường dẫn, ràng buộc rename/move/domain. **Khuyến nghị nghiên cứu sâu A, chưa chốt ID format/serialization**. |
| **Lưu file và metadata** | Stage/validate tại `runtime/staging` → rename trong cùng volume → insert metadata DB → cleanup/reconcile nếu lỗi. | Insert metadata `pending` (và storage key) → stage/finalize file → chuyển `active` qua DB; job/recovery scan xử lý pending quá hạn. | A ít trạng thái DB, nhưng crash sau rename trước INSERT tạo orphan; xóa bù có thể fail. B quan sát/retry tốt hơn nhưng cần state machine, cleanup và giao diện ẩn pending; cả hai có crash windows. **Không có atomic transaction chung giữa filesystem và SQLite**. |
| **Xóa/đổi media đang được dùng** | Mark deleted/tombstone, resolve thành placeholder, GC sau khi an toàn. | Hard delete có kiểm tra ref count, từ chối nếu đang tham chiếu, cleanup theo thứ tự. | Cần thống nhất với CNT-02/CNT-04, backup/restore và hành vi khi user thay file. Tránh orphan/DB ref dangling; chưa chọn retention/GC policy. |
| **Đường phục vụ media** | Handler theo ID, kiểm metadata/status và trả bytes + content headers. | URL tới storage key/path an toàn do host sinh sau khi validate; metadata resolve trong Admin. | Cả hai cần chống traversal/symlink, route namespace và quyền public/private. Video HTTP Range/cache/ETag chỉ đưa vào V1 nếu scope & tests xác nhận cần. |

**Turn 2 — ứng viên Hybrid MediaRef + reference projection:** Antigravity đề xuất opaque `media_id` và bảng `content_media_refs(entry_id, field_key, media_id)` với FK `ON DELETE RESTRICT` để thống kê nơi sử dụng và chặn hard-delete nhầm. Nghiên cứu sâu cách **CNT-04 đồng bộ projection trong cùng transaction lưu entry**; nếu chỉ lưu ref trong JSON mà không đồng bộ projection, FK không bảo vệ được. Gallery cần uniqueness/order semantics; cần tính cả Builder blocks, Theme Options và các nơi sử dụng ngoài Entry hoặc xác định rõ giới hạn bảo vệ. Index trên `media_id` cần được kiểm tra bằng query plan; **FK không bảo đảm truy vấn `O(1)`** hay bảo vệ những ref chưa được ghi vào bảng.

**Turn 2 — upload có pending-state:** hướng stage/validate → ghi metadata `pending/staging` → rename → mark `active` được ưu tiên nghiên cứu hơn file-first vì có dấu vết khi crash sau INSERT. **Nhưng crash sau tạo temp file và trước INSERT vẫn để orphan trong `runtime/staging/`**; startup recovery phải quét staging directory theo tuổi/ownership, xử lý bản ghi pending và cả file cuối không có metadata (legacy/repair). Không coi staging journal đảm bảo cleanup 100%, `rename` luôn bền trước power-loss hoặc tính nguyên tử xuyên SQLite ↔ filesystem. Chỉ public/resolve `active` **và xác minh file tồn tại**, retry/recovery phải idempotent.

**Turn 2 — kiểm tra CSP `srcDoc`:** [`BuilderWorkspace.tsx:103-108`](../../../cms/admin/src/builder/BuilderWorkspace.tsx) dùng `iframe sandbox="" srcDoc`; `App.tsx` gắn meta CSP `default-src 'none'`. Lưu ý theo [MDN về iframe srcdoc](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe#srcdoc), URL tương đối trong `about:srcdoc` **mặc định resolve dựa vào URL trang cha**; thiếu `<base>` không mặc nhiên làm `/media/...` vô hiệu. Vấn đề thật cần thử browser là **CSP source matching trên opaque origin, chính sách image/media, sandbox** và origin của media endpoint; chưa được suy `'self'` sẽ hoạt động hay thất bại trong mọi trình duyệt. Giữ `sandbox=""`, không thêm `allow-same-origin`/`allow-scripts` chỉ để hiển thị ảnh.

**G1 REVIEWED — E3 preview/public CSP trong Chromium (`TESTED IN ISOLATED FIXTURE`):** [Turn 4](../../discussions/threads/2026-11-07-m3-g1-content-types-media-research.md) báo Playwright + Chromium trên HTTP server `127.0.0.1` cô lập: `<iframe sandbox="" srcdoc>` có meta CSP `default-src 'none'; img-src 'self'; style-src 'unsafe-inline'` tải được `/media/pixel.png` (`naturalWidth = 1`), chặn ảnh ở host khác và chặn inline script/onload; trang public với CSP `sandbox; default-src 'none'; img-src 'self'; media-src 'self'` cũng tải ảnh và chặn script. Không cần thêm `<base>` cho `/media/...`. **Kết quả này chứng minh fixture trong Chromium, chưa chứng minh route media/preview OpenDash đã hoạt động hay mọi browser tuân cùng quy tắc.** G2 phải giữ sandbox và chạy regression trên Admin/public thực, HTTP headers, source matching và media response trước khi quyết định CSP cuối cùng.

**Turn 2 — định dạng & response security (đề xuất chưa chốt):** ưu tiên chấp nhận ảnh raster (`jpeg/png/webp/gif`), từ chối `svg/html` và polyglot nguy hiểm trong V1; video `mp4/webm` chỉ nếu phạm vi V1 được xác nhận. Kiểm cả MIME, cấu trúc tệp qua decoder/parser có giới hạn và kích thước/tài nguyên; **magic bytes đơn lẻ không đủ bảo đảm an toàn**. `nosniff`, Content-Type chuẩn và chính sách CSP/Content-Disposition theo ngữ cảnh serve là các ứng viên cần test, không khẳng định một CSP cứng thích hợp cho mọi media.

**Dependency bắt buộc:** CNT-05 sở hữu bytes + metadata + MediaRef resolver; CNT-02 sở hữu **field schema/validation** image/gallery và nơi lưu ref, không duplicate media metadata; CNT-04 sở hữu entry và quyền content; BLD-05 render/binding, SITE-02 OG/public URL và OPS-01 backup là downstream. Đưa **hợp đồng kiểu ref** sang CNT-02 trước khi khóa image/gallery schema, nhưng Media picker UI chỉ tích hợp sau khi schema đã sẵn sàng.

### G1.3 — Threat model và failure/recovery tests đề xuất

| Nhóm test | Tình huống/expected invariant | Trạng thái |
| --- | --- | --- |
| **Auth & upload boundary** | Unauthenticated 401, Editor/Admin theo quyết định RBAC, CSRF/Origin fail 403; MIME header giả, magic bytes sai, file/extension nguy hiểm, path traversal, symlink, filename collision, size vượt limit → từ chối, không ghi file/DB | `NOT_RUN` — chưa có endpoint |
| **Payload streaming** | JSON/ZIP/media có limit tách biệt; `413` tại ngưỡng, upload lớn/slow/cancel không giữ memory hay temp vô hạn | `NOT_RUN` — G2 cần xác định extractor/layer và policy size |
| **File ↔ SQLite consistency** | Crash/failpoint ở stage, rename, insert, activate, delete, cleanup; restart/reconcile không lộ pending, không mất media đang tham chiếu, không để tombstone bị serve | `NOT_RUN` — chưa có state machine |
| **MediaRef & move** | Một ID được dùng nhiều field/gallery; URL domain/path đổi không mất reference; missing file, orphan DB/file, ref bị xóa được phản hồi rõ và không execute script | `NOT_RUN` — chưa có ref model |
| **FK/reference projection** | Xóa media còn `content_media_refs` bị RESTRICT; thiếu index phải phát hiện scan; field/gallery update/delete phải đồng bộ bảng refs trong cùng transaction; tính đến refs ngoài Entry | `NOT_RUN` — mới kiểm bằng SQLite in-memory ứng viên |
| **Staging pre-journal crash** | Crash sau ghi file temp nhưng trước INSERT pending vẫn có orphan; startup quét temp cũ theo lease/age, không đụng upload đang hoạt động; stale pending/final file recovery idempotent | `NOT_RUN` — chưa có recovery production |
| **Unsafe formats/preview srcdoc** | Từ chối SVG/HTML/polyglot, MIME giả và resource bomb; preview iframe `sandbox="" srcDoc` hiển thị raster qua CSP phù hợp nhưng không chạy script, kiểm trên browser | E3 **PASS ở Chromium fixture cô lập**; validation upload và regression trên CMS thực `NOT_RUN` |
| **Public & preview** | Ảnh/video load theo CSP hợp lý nhưng scripts nguy hiểm vẫn bị sandbox; đúng `Content-Type`, `nosniff`, cache headers; video byte-range nếu thuộc scope V1 | `NOT_RUN` — chưa có media server |
| **Upgrade & backup** | Thêm migration mới từ **DB copy** `cms/fixtures/baseline_0001.sqlite` và DB có `home_page`/plugins; kiểm FK/integrity, restart, backup/restore nhất quán media bytes + refs | `NOT_RUN` — M3/M8 còn mở |

**An toàn trước G2:** PRD chỉ xác nhận ảnh/gallery field; video public delivery/preview và giới hạn dung lượng cần làm rõ trong G1/G2, không tự xây transcoding/CDN. `SitePaths` là đường chuẩn nhưng G2 còn phải chốt cách truyền vào service/handler, symlink policy và xử lý `OPENDASH_DB_PATH` tùy chỉnh. Nếu DB nằm khác volume site root, không giả định SQLite và file cùng transaction. Không dùng Git revert để rollback dữ liệu đã migrate.

**E2 — thí nghiệm G1 độc lập (07/11/2026):** Python sqlite3 in-memory với `PRAGMA foreign_keys=ON`: `DELETE media_assets` khi có `content_media_refs` bị RESTRICT đúng dự kiến; `EXPLAIN QUERY PLAN` trước index là `SCAN`, sau `CREATE INDEX ... (media_id)` là `SEARCH`. Dùng Python `tempfile.TemporaryDirectory` mô phỏng file staging tồn tại trước metadata cho thấy orphan trước journal. **PASS cho mô hình kiểm chứng cục bộ**, không thay thế Rust/Windows failpoint/crash test.

**E3 — thí nghiệm G1 do Antigravity báo tại Turn 4 (07/11/2026):** Playwright + Chromium với HTTP server/media 1×1 PNG trên loopback và ba tình huống `srcdoc img-src 'self'`, `srcdoc img-src <origin>` và public CSP `sandbox` đều **PASS trên fixture**, có ghi nhận `naturalWidth=1`, ảnh domain khác bị chặn và script/onload không thực thi. Đây là kết quả được ghi trong thread; chưa có fixture/log/screenshot lưu trong repo để replay và **không phải test route CMS/Windows integration**. E3 không thay thế hàng kiểm chứng thực của M3.

**Ưu tiên nghiên cứu cho G2:** opaque `media_id` + resolver/metadata; `content_media_refs` là **projection có điều kiện** cần CNT-04 đồng bộ trong cùng transaction khi save/delete Entry, có index tra `media_id` và FK bảo vệ đúng các nguồn ref đã được projection bao phủ. Media refs trong Builder/Theme Options/SEO cần được bổ sung khi các module liên quan triển khai hoặc công bố ranh giới chưa được bảo vệ. Ưu tiên stage → ghi pending → finalize → active, kèm recovery cho stale DB rows, file tạm **trước khi ghi journal** và file cuối không có metadata; quyết định TTL/lease và cơ chế cleanup khởi động/on-demand ở G2, không mặc định thêm periodic scheduler vào V1.

**Kết luận G1 (`REVIEWED`):** thống nhất ưu tiên opaque MediaRef, reference projection có điều kiện, pending-state upload + filesystem/DB reconciliation và CSP/sandbox đã thử trên fixture cô lập. Đủ cơ sở **mở G2**, nhưng chưa chốt SQL schema, ID/URL contract, endpoint, file format/limits, public/private policy, xóa/GC hay backup semantics; các ca fault injection, thực thi Rust/Windows và CMS browser integration vẫn `NOT_RUN`. [Thread đồng thuận G1](../../discussions/threads/2026-11-07-m3-g1-content-types-media-research.md).

## G2 — Điểm chốt thiết kế (hiện `NOT_STARTED`)

- Reference contract, ID format và migration; field `image`/`gallery` lưu ID hay cấu trúc khác; tách file bytes khỏi metadata.
- Upload limits, format support, validation, chính sách xóa và rollback.
- URL public và quyền thao tác Admin.
- Ownership của metadata và cách backup/restore nhất quán.
- File staging/finalize, cleanup/reconciliation khi SQLite transaction và filesystem không atomic; hành vi khi upload dở dang, file đã xóa hoặc orphan.
- CSP `img-src`/`media-src` của preview/public (giữ `iframe sandbox=""`, script bị chặn), kiểm chứng bằng browser regression trên CMS thật; xác định phạm vi projection refs và thời điểm chạy sweep phù hợp V1.
- An toàn migration: test trên DB copy, snapshot trước nâng cấp, forward-fix sau khi migration được phát hành; Git revert không rollback dữ liệu đã nâng cấp.

## G3 — Breakdown task dự kiến (chưa sẵn sàng giao code)

| Task ID giữ từ bản nháp | Đầu ra khi qua G3 | Phụ thuộc và điều kiện |
| --- | --- | --- |
| `M3-S04-T03` | Upload/serve endpoints, storage metadata/file lifecycle, auth/limits | Full-Track G2 đã duyệt, FND-02/FND-03 |
| `M3-S04-T04` | Media library UI + picker image/gallery | T03 và schema field CNT-02 từ M3-S02 |
| `M3-S04-T05` | Test API/file consistency, restart, missing file, media reference, recovery | T03/T04; cô lập site root và DB |

**Ánh xạ lịch sử ID:** `M3-S04-T01` (Research) và `M3-S04-T02` (Design G2) là nhãn task trong discovery draft cũ; nay được chuyển thành các mục G1/G2, **không tái cấp phát hai ID đó**. Các `T03..T05` mới chỉ là candidates, chưa `READY`.

Sau khi G2 được chốt, dùng [mẫu handoff](../templates/SLICE_TASK.md) trong `.agents/sync/task-for-ag.md`, không tạo task files riêng. Không triển khai Media M3 theo bản nháp này.
