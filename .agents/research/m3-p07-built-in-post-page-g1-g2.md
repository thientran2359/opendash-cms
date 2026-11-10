# Research: M3-P07 — built-in `post` / `page` (G1 + G2 proposal)

- **Agent / ngày:** Harbor (CoS macOS) · 2026-11-10
- **Source baseline:** `main@dfdcae2ebf9f06cd8d94b7ca3141c88d7dcd6005` (PR #18 đã merge); `agent/harbor` fast-forward tới đúng SHA này trước khảo sát
- **Gate:** G0 `AGREED` (phương án A); **G1 source audit thực hiện, chờ peer review**; **G2 `PROPOSED`, chưa `AGREED`**; G3 `NOT_STARTED`, G4/G5 `NOT_RUN`
- **Module/candidate:** `CNT-01/02`, `FND-01/03`, `M3-P07`; phụ thuộc tích hợp `M3-P08/P16`
- **Nguồn:** [PRD V1 — Lưu/Xuất bản](../../docs/product/PRODUCT_REQUIREMENTS.md), [M3 plan](../../docs/plans/milestones/M3/TASK_PLAN.md), [Living Brief CNT-01 (G2 cũ là lịch sử)](../../docs/plans/modules/CNT-01-content-types.md), [Planning Workflow](../../docs/plans/PLANNING_WORKFLOW.md), [ADR-0003](../../docs/decisions/ADR-0003-agent-consensus-governance.md)
- **Phạm vi:** research/design **không đổi code, migration, API/ABI, contract đã chốt, Project Status**, không tạo `M3-Sxx-Txx` hay Issue `Ready`.

## 1. Mục tiêu G0 và invariant

Fresh install và upgrade đều có sẵn `post` và `page` **do Core sở hữu**, editor không cần vào màn tạo CPT. Dùng cùng canonical `content_types` / `content_entries` thay vì dựng một bảng Posts/Pages mới. `post`/`page` hỗ trợ title/slug/body/featured image ở M3; draft/published tối thiểu và public/preview bảo mật do các candidate `P08/P16` hoàn tất. **Lưu draft không công khai, Xuất bản mới công khai; Builder/homepage `/` tiếp tục công khai khi Lưu.** Phải giữ nguyên CPT/entry/media hiện có, M1 `home_page`, không mất dữ liệu trong các lần chạy/migrate lại. Không mở taxonomy (M4), plugin declarations (M6), theme CPT declaration (M5), revision/schedule.

## 2. G1 — Source audit tại main@dfdcae2 (đã đối chiếu file)

| Nơi quan sát | Đã có | Khoảng trống/rủi ro |
| --- | --- | --- |
| [Core content](../../cms/crates/opendash-core/src/content.rs) | `ContentTypeDefinition { key, label, fields }`, validation snake-key 64, 9 field kinds trong đó `rich_text` và `image`; `ContentEntryInput { title, slug, values }`. | Chưa có factory schema chuẩn `post`/`page`; chưa có `publication_status`. Trường select do người dùng đặt tên `status` không được xem là trạng thái xuất bản. |
| [Core registry](../../cms/crates/opendash-core/src/registry.rs) | `ResourceOwner::{Core,User,Theme,Plugin}`, `ProviderStatus` và `RegistryCatalog` ngăn owner conflict trong RAM. | `RegistryCatalog` không tự seed SQLite; không coi in-memory registry là nguồn sự thật cho built-ins. |
| [Migration 0004](../../cms/migrations/0004_content_types.sql) | Bảng `content_types(key PRIMARY KEY, definition_json, owner_kind, owner_id, provider_status)`, Core yêu cầu `owner_id IS NULL`. | Không có row `post`/`page` mặc định; user đã có thể tạo key cùng tên từ M3.2. |
| [Migration 0005](../../cms/migrations/0005_content_entries.sql) | Entry thuộc site, FK `content_type_key` `ON DELETE RESTRICT`, unique `(type, slug)`, `version` dùng optimistic concurrency. | Không có cột publication status; mọi entry hiện được coi public. Không được viết lại migrations cũ. |
| [Migration 0007](../../cms/migrations/0007_content_templates.sql) + [DB init](../../cms/crates/opendash-server/src/storage/mod.rs) | `sqlx::migrate!` dùng lần lượt migration file; mới nhất `0007`. | Forward migration tiếp theo cần kiểm restore/old SQLite copy; SQLite DDL rollback/SQLx test chưa chạy. |
| [CPT storage](../../cms/crates/opendash-server/src/storage/content.rs) + [routes](../../cms/crates/opendash-server/src/routes/content.rs) | `insert_user_content_type` tạo `owner_kind='user'` và trả 409 khi duplicate; GET cần session, POST yêu cầu Admin + CSRF; `list_content_types` hydrate Core owner đúng. | Phải chặn User chiếm reserved `post/page` kể cả khi thiếu seed hoặc bản cũ đang conflict. Không có core seed helper/service hiện hành. |
| [Entry storage/routes](../../cms/crates/opendash-server/src/storage/entries.rs) / [routes/entries.rs](../../cms/crates/opendash-server/src/routes/entries.rs) | CRUD dùng cùng schema CPT, session+CSRF và `If-Match`/ETag; `content_entries` lưu values JSON. | Route create/update không có status argument và mặc định public; P07 đứng riêng phải **fail closed** ở public hoặc chờ P08/P16 gate, tránh lộ nội dung do editor mới tạo. |
| [Public routes](../../cms/crates/opendash-server/src/routes/public_content.rs) / [SQL lookup](../../cms/crates/opendash-server/src/storage/public_content.rs) | `/content/{type_key}`, `/content/{type_key}/{slug}`; archive dùng `list_entries`, detail `get_by_slug`; HTML có CSP/escaping. | Cả archive/detail **không lọc draft**; dữ liệu public ngay sau Save theo baseline M3.6, trái yêu cầu post/page mới nếu không gắn status gate. |
| [Public media ACL](../../cms/crates/opendash-server/src/storage/public_content.rs) | `media_is_referenced` tìm reference image/gallery trong **mọi** entry; media upload/admin file cần auth. | Ảnh **chỉ tham chiếu bởi draft** vẫn có thể public nếu bật draft mà không sửa ACL. Cần filter `published` đúng các type có workflow; ảnh đã được bài khác public tham chiếu thì không thể xem là riêng tư tuyệt đối. |
| [M1 Builder](../../cms/crates/opendash-server/src/storage/page.rs) + [router](../../cms/crates/opendash-server/src/app.rs) | `home_page(id=1)`, `/` render riêng, public CPT dưới `/content/*`; plugin, auth, media routes đã có. | Không đổi root `/` hoặc tự thay `home_page` bằng `page`; M3-P14 theme activation là bài toán khác. |

**G1 kết luận:** dữ liệu và owner schema lõi đã có, nên **reuse `content_types`** cho built-ins. Khoảng trống là **forward seed có guard** + **tính bất biến owner/schema** + **vùng an toàn trước khi P08/P16 bật chức năng publish**, không phải viết lại CPT CRUD M3.1–M3.6.

## 3. G1 — Lựa chọn và thí nghiệm cô lập E1

| Phương án | Ưu | Nhược/rủi ro | Kết luận đề xuất |
| --- | --- | --- | --- |
| **A. Forward migration `0008`** tạo hai definition `owner_kind='core'` + guard collision | Fresh/upgrade và các test chạy `sqlx::migrate!` có cùng dữ liệu; đúng trật tự schema | Migration là lần chạy một lần; khi thay đổi schema built-in sau này cần version/forward migration; key `post/page` của User cũ là blocker | **Ưu tiên A**, cần G2 duyệt guard và migration transaction trước code |
| **B. Startup `ensure_core_types` sau migration** | Có thể reconcile state mỗi restart, linh hoạt default labels | Dễ cập nhật đè schema, race/multiple processes, test DB dùng migrate trực tiếp có thể thiếu seed, lỗi startup sau khi migration xong | Không chọn làm nguồn seed chính; nếu cần repair chỉ chạy qua migration/service có guard độc lập |
| **C. Hai bảng `posts`/`pages` riêng** | CRUD specialized nhanh | Trùng dữ liệu/schema/ACL với `content_entries`, phá hợp đồng owner CNT-01 và future CPT templates | **Loại bỏ** |

**E1, SQLite `:memory:`, 10/11/2026 (không sửa source):** nạp chính xác SQL từ `0004_content_types.sql` + `0005_content_entries.sql`, mô phỏng transaction seed Core với descriptor gồm `body` rich_text và `featured_image` image. **PASS** (a) seed rồi chạy lại không nhân bản, entry `portfolio_item` cũ vẫn tồn tại; (b) User đang sở hữu `page` gây collision thì rollback toàn thao tác, `post` không bị chèn dở và User `page` còn nguyên. **Giới hạn:** đây là *simulation* có logic guard viết riêng trong Python; chưa có migration `0008`, không chứng minh migration SQLx/Rust, boot, concurrency, backup hay cross-OS.

## 4. G2 — Contract ứng viên v0.1, CHƯA ĐƯỢC DUYỆT

| Chủ đề | Thiết kế đề xuất | Review/acceptance bắt buộc |
| --- | --- | --- |
| **Reserved identity/owner** | Key cố định `post`, `page` theo `valid_content_type_key`; mỗi row có `owner_kind='core'`, `owner_id=NULL`, `provider_status='active'`. User/Theme/Plugin không được claim, rename, deactivate hoặc sửa schema Core. Tránh thêm namespace/ID mới ở plugin ABI. | Fresh/upgrade/restart/list trả cùng hai Core types; POST create CPT với reserved keys trả 409, kể cả khi có race; Editor 403 khi tạo CPT. |
| **Default descriptors** | Gợi ý `post: Bài viết`, `page: Trang`, cùng hai fields `body` (`rich_text`, optional để lưu nháp chưa có body), `featured_image` (`image`, optional). Title và slug vẫn là các cột entry vốn có; không nhét title/slug vào `values_json`. | Dùng validator hiện hành; không áp đặt required body làm chặn nháp; media ID validation và ref ACL giữ nguyên contract. Core schema không tùy ý đổi qua Admin. |
| **Seed và data preservation** | Thêm migration forward sau `0007` (ứng viên `0008`, không sửa các file 0001–0007), seed chỉ khi missing và kiểm owner/schema khi existing; toàn bộ preflight+seed trong một transaction; không dùng `INSERT OR REPLACE` hay `ON CONFLICT UPDATE` để cướp key. Trước upgrade site thật phải backup/snapshot; migration lỗi không làm mất bản cũ. | Fixture `baseline_0001`, DB `0007` có CPT/entry/media/Builder, restart và repeat init; checksum dữ liệu cũ không đổi, FK/row count còn đúng. SQLx migrate về cùng result; test failure atomicity. |
| **Collision trên DB đã có User `post/page`** | **Fail closed**, trước mọi mutation phải phát hiện owner != Core hoặc Core descriptor bị thay trái contract; không auto chuyển User→Core, không xóa content. Diagnostic đưa ra cách backup và manual remap có kiểm chứng (cần G2 reviewer chốt kế hoạch xử lý). | DB upgrade có User `page` + entries phải **giữ nguyên cả DB và nội dung**, không seed dở. **Blocker G2:** chọn cơ chế remediation không khiến site cũ bị kẹt không thể upgrade lâu dài. |
| **Publish status là contract tích hợp, không phải field tùy ý** | Nghiên cứu `status` server-owned trên `content_entries` (`draft/published`) thay vì custom field `select`; `post/page` mới tạo mặc định `draft`, chỉ explicit Publish mới public. Entry legacy/CPT khác giữ hành vi Save-public (đề xuất backfill `published`). `If-Match`/version phải cover status transitions. | **Chưa chốt owner migration status giữa P07/P08/P16**. Không cho P07 code tự mở public `post/page` nếu chưa có status + ACL: hoặc phát hành atomic slice cùng P08/P16, hoặc tạm deny public built-ins cho tới khi được publish đúng cách. |
| **Public URL, preview, media** | Giữ `/` Builder và `/content/{type}/{slug}` đã có; P16 lọc published trên archive/detail, preview draft cần auth session; public image ACL chỉ tính entry được phép public. Reuse canonical DB/access layer, không query SQL trong routes. | 404/không xuất hiện archive đối với draft; media chỉ được draft tham chiếu phải 404 public; publish rồi 200; unpublish lại 404 (trừ asset có ref public khác); Editor không bypass auth. CSP/XSS và privacy test. |
| **Error handling/rollback** | Duplicate core key khác owner/invalid descriptor → fail an toàn, log chỉ thông tin không nhạy cảm; transaction không có half-migration, không dùng Git revert như rollback SQLite. Ownership conflict là blocking gate trước G3, không chấp nhận ghi đè dữ liệu. | Negative tests cho collision, SQL migration failure, orphan FK, concurrent startup / initial setup; DB copy và kiểm tra dữ liệu trước-sau. |

**Bảo vệ khi P07 triển khai tách riêng:** API `/api/admin/content-types` hiện cho phép Editor tạo entry thuộc mọi CPT; vì public renderer hiện live ngay sau Save, **chỉ seed Core types mà chưa khóa public là không đủ an toàn**. Đề xuất G3 bắt buộc chỉ rõ **staged fail-closed guard** hoặc gom P07 với capability publication nhỏ nhất của P08/P16. Không phân rã thành hai task viết đồng thời cùng migrations/storage/router.

## 5. Dependency, thử nghiệm và gate

**Thứ tự đề nghị:** (a) Atlas/Nova peer-review G1 và chốt G2 reserved-key collision/remediation + descriptors/migration/visibility; (b) cập nhật Living Brief CNT-01/02 và ADR/contract khi đã có quyết định, review docs PR; (c) coordinator chuẩn hóa `M3-Sxx-Txx` và G3 READY có file ownership; (d) implement P07 / P08 / P16 có sequence để không làm lộ draft; (e) Windows parity và macOS E2E trên source/binary SHA thực.

**Danh sách file ứng viên G3 cần phân quyền, không phải permission hiện hành:** `cms/migrations/0008_*.sql`, `cms/crates/opendash-core/src/content.rs`, `cms/crates/opendash-server/src/storage/{content,entries,public_content}.rs`, `cms/crates/opendash-server/src/routes/{content,entries,public_content}.rs`, `cms/crates/opendash-server/src/app.rs`, `cms/scripts/{content_types_smoke,content_entries_smoke,public_content_smoke}.py`, frontend ContentWorkspace và reports `docs/verification/`. Không hai agent viết migration/route/storage chung song song; `M3-P09` AdminShell làm độc lập khi UI ownership rõ.

**Ma trận nghiệm thu tối thiểu trước G5 (chưa chạy):**

1. Fresh DB: `post/page` có sẵn, `owner=core`, schema fields như G2, Admin/Editor list thấy; user CPT tùy chỉnh vẫn tạo được trừ reserved keys.
2. Upgrade DB 0001/0007 trên **bản sao**: dữ liệu `home_page`, plugin, user CPT, entry, media, templates không mất; sau restart/migrate lại không duplicate built-ins.
3. Collision user-owned `post/page` có entries: fail an toàn + diagnostic/remediation, không cướp owner hoặc xóa một row; rollback không để migration áp dụng nửa vời.
4. Auth/Admin/Editor/CSRF/Origin/invalid JSON/unique key: 401/403/400/409 theo API; core type không bị đổi/xóa; truy cập đồng thời không nhân bản.
5. Trước khi full publish pipeline sẵn sàng: không có entry built-in mới nào vô tình public. Sau P08/P16: `draft` lưu/restart/preview auth, public/archive/media private; `publish` thành public; `Lưu published` cập nhật ngay; Builder Save trên `/` giữ nguyên.
6. Không suy PASS macOS sang Windows/Linux; các smoke/binary E2E phải nêu runner, SHA, exit code và data copy.

**Tests đã chạy trong G1:** static code audit + E1 Python SQLite `:memory:` **PASS simulation**; **Rust workspace/build, actual migration `0008`, HTTP/browser, macOS/Windows/Linux E2E: NOT_RUN** (chưa triển khai). Các verification M3.1–M3.6 thuộc phiên bản/source cũ, không làm chứng G5 cho G0 mới.

## 6. Open questions / yêu cầu phản biện G2

1. **Collision key `post/page` cũ:** fail startup có nguy cơ gây outage; nên cung cấp preflight CLI migration + explicit rename với FK/site links, hay chỉ fail closed + manual migration? Reviewer cần chọn phương án an toàn có thể phục hồi và test trên DB copy.
2. **Lần đầu tạo draft:** status column/migration và write-path thuộc P07 hay P08? **Không cho public builtins trước P16**, yêu cầu thứ tự release/file ownership chặt. Với CPT legacy, có giữ mặc định published để backward compatible?
3. **Field keys/shape:** `body`, `featured_image` optional như proposal hay cần thêm `excerpt`? M3 A không yêu cầu excerpt; tránh scope creep. Schema evolution Core builtins sau V1 áp dụng thế nào?
4. **Media public sharing:** cùng asset ID được refer từ published và draft thì public có chủ ý? Cần xác nhận policy và negative tests cho chỉ-draft, remove-last-published-ref.
5. **Edit/preview URL:** endpoint & auth strategy cho draft, reserved namespace và slug collision với route khác chốt ở P16/SITE-02, không tự suy từ existing URL.

## 7. Decision / promotion reference (only after approval)

**PENDING.** Đề nghị Atlas (coordinator) hoặc Nova (reviewer độc lập) đối chiếu `main@dfdcae2`, trả lời blocker 1–5 và kết luận G2 qua agent consensus / ADR-0003. Sau Final Decision, Harbor hoặc writer được phân quyền mới promote **phần đã duyệt** vào Living Brief/contract qua PR review; chỉ sau đó mới phát hành task `G3 READY`. Không dùng research này như authorization code.
