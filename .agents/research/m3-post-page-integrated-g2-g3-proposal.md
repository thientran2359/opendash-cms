# M3-P07/P08/P16 — G2 publication boundary + G3 work packages (PROPOSED)

- **Writer:** Harbor (CoS macOS), 2026-11-10.
- **Baseline:** `origin/main@658fc26d7c07866572df006a611773ac4c3d96fc` (PR #20 merged).
- **Input:** [G1 / G2 v0.1 cho M3-P07](m3-p07-built-in-post-page-g1-g2.md), [PRD save/publish](../../docs/product/PRODUCT_REQUIREMENTS.md), [M3 A task plan](../../docs/plans/milestones/M3/TASK_PLAN.md), [ADR-0003](../../docs/decisions/ADR-0003-agent-consensus-governance.md).
- **Gate:** G0 M3 A `AGREED`; G1 code-audit bổ sung; **G2 `PROPOSED`, chưa `AGREED`; G3 chỉ là *candidate work packages*, chưa có Task ID `Mx-Sxx-Txx`, GitHub Issue/Project `Ready`; G4/G5 `NOT_RUN`.**
- **Scope:** P07 core-owned built-in `post/page`; P08 publication backend + Admin Posts/Pages; P16 public visibility + authenticated preview. Không sửa code/migration, không mở task implementation từ file này.

## 1. Xác nhận lại tại main@658fc26: bề mặt rò dữ liệu

| Thành phần | Bằng chứng source hiện tại | Hệ quả nếu chỉ seed built-ins |
| --- | --- | --- |
| [0004](../../cms/migrations/0004_content_types.sql) / [0005](../../cms/migrations/0005_content_entries.sql) | `content_types` cho phép `owner_kind=user` với key `post/page`; `content_entries` có `values_json`, `version`, unique `(type,slug)` nhưng **không có publication column**. | M3-P07 key collision trên DB cũ; không được cướp owner hay sửa 0004/0005. |
| [Core input](../../cms/crates/opendash-core/src/content.rs) | `ContentEntryInput` chỉ `title/slug/values`; field tùy chỉnh tên `status` có thể hợp lệ nhưng **không mang semantics publish**. | Trạng thái bắt buộc server-owned, không lấy từ `values_json`. |
| [Admin routes](../../cms/crates/opendash-server/src/routes/entries.rs) / [entries SQL](../../cms/crates/opendash-server/src/storage/entries.rs) | Editor tạo/sửa qua CSRF+session; `If-Match` trên update/delete; INSERT không có default draft theo type. | Phải set draft trên **mọi** đường tạo built-in; version CAS bao gồm chuyển trạng thái. |
| [Public archive/single](../../cms/crates/opendash-server/src/routes/public_content.rs) / [public SQL](../../cms/crates/opendash-server/src/storage/public_content.rs) | Archive dùng `entries::list_entries` không lọc visibility; single lookup slug không lọc; không có preview auth cho post/page. | Draft có thể xuất hiện qua URL và archive, không chỉ qua HTML render. |
| [Public media](../../cms/crates/opendash-server/src/storage/public_content.rs) | `media_is_referenced` trả true nếu ảnh được **bất cứ** entry image/gallery tham chiếu. | Ẩn URL draft nhưng không ẩn media thì vẫn rò ảnh chỉ dùng trong draft. |
| [DB init](../../cms/crates/opendash-server/src/storage/mod.rs) / [router](../../cms/crates/opendash-server/src/app.rs) | `sqlx::migrate!` chạy ở startup; Builder `/`, CPT archive `/content/{type}`, single `/content/{type}/{slug}`, media `/media/{id}/*`. | Migration collision có thể ngăn khởi động; phải có preflight/backup/remediation. Giữ nguyên Builder `/` và CPT legacy Save-public. |

## 2. G2 v0.2 — đề xuất contract để reviewer quyết định

### A. Dữ liệu, owner và upgrade

1. Reuse `content_types` và `content_entries`, không tạo bảng `posts`/`pages`. Two built-in keys `post` / `page` là **Core-owned**, `owner_id=NULL`, `provider_status=active`; labels `Bài viết` / `Trang`. Definition có `body: rich_text` và `featured_image: image`, đều optional (cho draft rỗng); title/slug giữ column entry.
2. Dự kiến migration forward `0008_*.sql` (chỉ sau G2): thêm `publication_status TEXT NOT NULL CHECK (publication_status IN ('draft','published'))` cho entries, giữ **legacy existing entries = published** để không đổi hành vi CPT cũ; seed `post/page` nhất quán qua fresh/upgrade. Tránh `INSERT OR REPLACE`, `ON CONFLICT UPDATE` với Core identity, không rewrite 0001–0007. **Điều kiện an toàn:** write-path mới phải gán draft rõ ràng cho built-ins trong cùng release; DB default cho legacy *không được* vô tình publish built-ins mới.
3. Preflight trước migration đối với DB có `content_types`: nếu `post` hoặc `page` đã được user/theme/plugin sở hữu hay Core descriptor khác contract, **abort có diagnostic rõ và giữ nguyên dữ liệu**. Reviewer phải xác minh giải pháp preflight + seed *atomic* trên SQLite/SQLx, không chỉ Python simulation. No silent overwrite, no implicit deletion.
4. **Remediation collision (chưa duyệt):** snapshot DB/site rồi trên **bản copy offline**, kiểm mapping mới chưa trùng và tạo một user CPT key mới với definition key khớp; cập nhật FK child `content_entries.content_type_key` sang key mới, kiểm row counts/IDs/slug/values/media refs, xóa CPT cũ chỉ khi không còn referenced. URL `/content/post/*` có thể thay đổi: cần thông báo/chốt redirect/backward compatibility trước khi thao tác thật. Không chạy migration sửa dữ liệu người dùng tự động.
5. Reserved-key rule phải ở **storage** (chặn cả caller ngoài UI), và UI/validation đưa thông báo 409 hợp lệ. Không cho tạo lại/rename/deactivate `post/page`; kiểm cả source path có thể bypass qua hooks/import/DB writes. Chốt policy schema evolution Core riêng nếu về sau thêm field.

### B. Admin write + publication transition

1. Mọi **create** `post/page` mặc định `draft`, bất kể gửi field `values.status`; `ContentEntryRecord` trả status thật từ DB. CPT khác tiếp tục `Save = published` như M3.6; `ContentEntryInput` hiện tại vẫn được phép cho legacy, không tự buộc họ dùng publish workflow.
2. Đề xuất **transition endpoint chuyên biệt** `POST /api/admin/content-types/{type_key}/entries/{id}/publication` body `{"status":"published"}` hoặc `{"status":"draft"}`, chỉ `post/page`. Session + CSRF/Origin; `If-Match: "vN"`; thực hiện một SQL update `status=?, version=version+1` với `WHERE version=N`, trả ETag mới. GET/list admin chỉ yêu cầu session và được xem draft, không tiết lộ qua public.
3. `Save` chỉnh sửa content **không tự thay trạng thái**: update giữ nguyên draft/published; button `Publish` hoặc `Unpublish` gọi transition. 412 khi stale CAS, 401/403 khi thiếu quyền, 400 invalid enum, 404 missing; Editor hiện có quyền entry CRUD theo baseline, quyền Publish của Editor cần reviewer xác nhận không mở RBAC mới.
4. Hook `ContentBeforeSave/AfterSave` hiện tại phải được đối chiếu semantics cho transition; không phát `AfterSave` trên failed CAS; không tạo trạng thái public dựa trên hook callback trước DB commit.

### C. Public reads, media và preview — fail closed

1. **Mọi** public archive/single `post/page` phải lọc `publication_status='published'` ở **SQL trước** `LIMIT/OFFSET`/lookup; `draft`, invalid/unknown status và missing không có HTML/danh sách (404 single, archive bỏ qua). Legacy CPT khác giữ nguyên public Save behavior. Không lọc archive sau phân trang.
2. Public media `/media/{id}/file|thumbnail`: một reference hợp lệ **chỉ** khi entry legacy CPT public hoặc `post/page` có `publication_status='published'`. Gom điều kiện status bên ngoài **cả** nhánh `image` lẫn `gallery` để không sai precedence `AND/OR`. Ảnh chỉ từ draft → 404; nếu một published/legacy entry khác vẫn tham chiếu cùng SHA-256 asset, asset **có chủ ý vẫn public**. Unpublish/remove last published ref → public 404 (không thu hồi bytes đã tải).
3. Preview draft **chỉ trong phiên admin hợp lệ**, dữ liệu không gắn vào URL public. Đề xuất preview HTML auth-only dưới `/api/admin/...` hoặc render client từ authenticated GET; chưa chọn endpoint. HTML được escape đúng ngữ cảnh + CSP `script-src 'none'`/no-store, image preview qua **authenticated admin media** thay vì public URL khi draft. Không reuse Builder `POST /preview` như endpoint public của post/page, không tạo unsigned preview link.
4. Giữ Builder `/` và `Save` publish behavior hiện hữu; không thêm taxonomy, scheduled publishing, revisions, menu URL policy nâng cao (M4/M5).

### D. Release sequencing — tránh trạng thái rò tạm thời

- **Tuyệt đối không merge/deploy một seed-only change** tạo `post/page` cho public router hiện tại. Nếu tách PR, **WP-A phải ship cùng**: migration + reserved guard + created-as-draft write path + public **deny-all** cho hai core types và media-only-from-core (tạm 404) trên cả public content/media. Chỉ mở public `published` sau khi WP-C có SQL filter và security regression; WP-A phải test không lộ dữ liệu trước cả khi publish endpoint có mặt.
- Chọn phương án release **atomic A+B+C** nếu không chứng minh được trạng thái trung gian an toàn; không để nhiều agent sửa các file `entries.rs`, `public_content.rs`, migrations song song. Mọi merge liên tiếp đều phải giữ public fail-closed trên head dùng thực tế.

## 3. G3 DRAFT — 5 work packages đề xuất, **chưa cấp ID/Issue Ready**

| G3 candidate | Scope + primary observable acceptance | Dependencies / owner candidate / file ownership |
| --- | --- | --- |
| **WP-A — core seed và safety guard** | Fresh/upgrade có built-ins Core, user CPT không thể chiếm keys; legacy data preserved; built-in create always draft; public archive/single và media từ built-in **404 cho đến WP-C**; collision abort không half-seed. | G2 approved collision/preflight/DDL; **1 backend writer Harbor hoặc Forge**. Candidates: `cms/migrations/0008_*.sql`, `cms/crates/opendash-core/src/content.rs`, `cms/crates/opendash-server/src/storage/mod.rs`, `storage/{content,entries,public_content}.rs`, `routes/{content,entries,public_content}.rs`, related smoke scripts. **Writer must be assigned exact filenames before G3 READY.** |
| **WP-B — publish/unpublish API** | Create draft; authenticated `Publish` → status published/version increment; `Unpublish` → draft; legacy CPT Save unchanged; wrong If-Match 412, CSRF/Origin guard, restart preserved. While WP-C pending, newly published built-ins still **404 public** by WP-A guard. | Depends WP-A merged; same backend writer sequentially on `storage/entries.rs`, `routes/entries.rs`, `app.rs` and tests; G2 endpoint/role contract decided first. |
| **WP-C — public visibility + preview + media** | Published archive/single 200; draft hidden; public media ACL respects published status across image/gallery; authenticated preview draft works; Builder `/`/legacy CPT unchanged, XSS/CSP regression. | Depends WP-B, **same backend file ownership sequentially** for `storage/public_content.rs`, `routes/public_content.rs`, `app.rs`, renderer/preview and tests. No public enabling before negative tests PASS. |
| **WP-D — Posts/Pages Admin UI** | Navigation + list/create/edit/delete Post/Page; draft/save/publish/unpublish, status label/ETag/412 unsaved form, picker and admin preview accessible. | Depends WP-B/WP-C and **M3-P09 AdminShell contract**; UI owner Orion or designated agent, restricted to `cms/admin/src/**` paths **enumerated exactly** at G3; coordinate `App.tsx` and Tailwind layout ownership with P09. |
| **WP-E — E2E & OS parity evidence** | Fresh/upgrade/collision/rollback fixtures; two roles, draft→published→draft, shared-media last-ref, pagination, restart, 401/403/412/CSRF, CSS/HTML escaping, Admin browser on macOS; Windows smoke separately with SHA/runner; G5 only after evidence. | Depends WP-A..D and M3-P17/P18/P19 gates; Harbor/macOS, Forge or Atlas/Windows runner under independent review. Only test/evidence paths assigned; no bypass CI or dummy PASS. |

**Không phải phát hành tự động 5 Issue:** các `WP-*` là work packages để thảo luận, không phải namespace Task ID hay `READY`. Coordinator sẽ chia task thật theo `M3-Sxx-Txx` **sau khi G2 `AGREED` + G3 reviewed/merged**, pin `source_sha`/`base_sha`, exact writable paths, owner/reviewer, acceptance và cập nhật [TASK_INDEX](../../docs/plans/TASK_INDEX.md) khi Issue thật được phát hành.

## 4. Decision points gửi reviewer + tests cần kiểm

1. **Upgrade collision:** chọn `preflight before migrate` + offline remediation với preserved IDs, và policy URL đổi/redirect; cần test trên DB copy có User `post/page` + entries trước khi xác nhận không outage ngoài ý muốn.
2. **Migration representation:** column `publication_status` với legacy backfill published vs bảng publication tách riêng; kiểm SQLx 0008 atomicity và failed migration table/checksum trên SQLite thực, không suy từ dry-run SQL.
3. **Publish API/RBAC:** endpoint transition chuyên biệt (đề xuất) vs update có field status; quyền Editor Publish và semantics khi update published; `If-Match`, CSRF, hook dispatch.
4. **Preview:** authenticated HTML endpoint hay client rendering từ GET; đường media authenticated, cache/CSP/URL không phát tán draft.
5. **G3 granularity/release:** chọn WP-A fail-closed rồi B/C, hoặc một atomic change nếu không thể chứng minh intermediate safety. Không đưa G3 READY trước khi ràng buộc rõ WP-A guard + negative tests.

**Security tests bắt buộc:** SQL query filter trước pagination; single draft 404; archive không lộ title/slug; media draft-only 404 (image và gallery), shared referenced asset còn 200; publish/unpublish dưới concurrency `If-Match`; legacy CPT vẫn public; Builder `/` không đổi; user-owned `post/page` collision không sửa bất kỳ row/entry/media; restart/migration idempotent; XSS/render CSP. Negative cases dùng site/DB **copy**, không thử sửa dữ liệu thật.

**Đã thực hiện ở lượt này:** read-only audit source và docs tại `main@658fc26`. **E2 — SQLite `:memory:`**: nạp nguyên văn `0004` + `0005`, tạo một entry legacy, thử `ALTER TABLE ... ADD COLUMN publication_status ... DEFAULT 'published'`, sau đó INSERT `post` không chỉ định status và INSERT `post` có status `draft`. **PASS**: legacy backfill = `published` (giữ compatibility), **new post implicit = `published` (UNSAFE)**, explicit new post = `draft` (an toàn khi write-path cưỡng bức). Điều này chứng minh **không được dùng DB default published cho built-in create**; không chứng minh guard ở runtime thật. **Chưa chạy SQLx 0008** (chưa có), Rust build, test HTTP/browser, Windows E2E; G5 `NOT_RUN`. E1 SQLite in-memory tại G1 v0.1 là **simulation**, không chứng minh migration thật.

## 5. Next action / review

**Đề nghị một review kỹ thuật độc lập** trên PR của bản proposal này (Atlas hoặc Nova), trọng tâm là collision và fail-closed chain. Nếu có đồng thuận thật, coordinator ghi Final Decision theo ADR-0003, promote phần được chốt vào Living Brief/contract bằng PR; rồi mới sinh G3 task và GitHub Issue `Ready`. Nếu reviewer báo risk/contract blocker ⇒ sửa proposal tiếp, không code hoặc claim Task Done. **Next agent: Atlas/Nova (G2 review); Harbor giữ quyền tác giả bản draft, không tự kết luận G2.**
