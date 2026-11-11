# M3 — Task planning: Core CMS & WordPress-like Admin (phương án A)

**Loại tài liệu:** kế hoạch tĩnh về **phạm vi, thứ tự, dependency và gate**; **không phải bảng trạng thái Backlog/Ready/In progress/In review/Done**. [GitHub Project #3](https://github.com/users/thientran2359/projects/3) và [GitHub Issues](https://github.com/thientran2359/opendash-cms/issues) giữ trạng thái thực tế, assignee, blocker và acceptance checklist theo từng task; [GitHub PR](https://github.com/thientran2359/opendash-cms/pulls) giữ diff/review. Không suy status GitHub Project chỉ từ một file, commit, test hoặc PR.

**G0 scope được chủ dự án chốt 10/11/2026:** M3 là **CMS cơ bản dùng được trong Admin**, lấy cảm hứng WordPress nhưng dùng Tailwind CSS; không phải clone WordPress hoặc hoàn thành tất cả năng lực V1. [Module Catalog](../../MODULE_CATALOG_V1.md) → [Roadmap V1](../../ROADMAP_V1.md) → [M3 milestone](README.md) → các candidate; [coverage V1](../COVERAGE_MATRIX.md), [snapshot cũ M3](../../M3_STATUS_2026-11-09.md), [workflow](../../PLANNING_WORKFLOW.md). [Issue #15 Final Decision](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410) là thứ tự nghiên cứu kỹ thuật **lịch sử**, không thay G0 mới. Source khảo sát `agent/harbor@f239752bf90736595a598dadf6dfaf5bb299552d`, không có test runtime mới cho việc đổi scope; chỉ có evidence M3.1–M3.6 theo SHA/OS ở [verification](../../../verification/README.md).

**G2 cập nhật 10/11/2026:** [ADR-0004 Final Decision — G2 AGREED trực tiếp bởi owner](../../../decisions/ADR-0004-m3-post-page-publication-boundary.md) cho **M3-P07/P08/P16** (collision/migration, Admin/Editor Publish+CAS, draft preview authenticated, media ACL, fail-closed release sequencing). [G3 work packages](slices/2026-11-10-post-page-publication-g3.md) là **DRAFT**, chưa cấp Task ID/Issue/Project Ready; không được triển khai seed-only, không coi đây là waiver CI/security hoặc đóng G5/M3.

**Execution update 11/11/2026:** [ADR-0005](../../../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md) cho Harbor/Forge tự chọn và self-merge từng task M3 khi CI/tests/security pass, không cần peer review. [M3-S20-T01 và M3-S21-T01](slices/2026-11-11-m3-autonomous-first-tasks.md) là hai đặc tả G3 mở đầu; các hàng `M3-Pxx` phía dưới vẫn là planning candidates và không tự biến thành GitHub Issue/Project Ready.

**Thiết kế phần M3 còn lại:** [ADR-0006](../../../decisions/ADR-0006-m3-admin-and-release-contracts.md) ghi G2 `AGREED` cho P10, P12–P15 và P17–P19 theo [Issue #45](https://github.com/thientran2359/opendash-cms/issues/45). [G3 đợt Users và General Settings](slices/2026-11-11-m3-users-settings-g3.md) chuẩn bị hai gói đầu. P18 đã có bản sửa và evidence từ PR #33, phần còn lại là regression trong P19.

## M3 phương án A — mapping capability → task candidate

M3 tập trung **CNT-01/02/04/05, FND-02/05, SITE-01 tối thiểu, THM-01 và PLG-01 UI/lifecycle tối thiểu**, với FND/OPS prerequisite. **CNT-03 taxonomy chuyển M4, PLG-03 bridge chuyển M6, media video chuyển M7, xóa/reconcile chuyển M8** (giữ đủ scope V1 trong [coverage](../COVERAGE_MATRIX.md)). `M3-Pxx` là **planning IDs**, không phải `M3-Sxx-Txx` G3, GitHub Issue hay Project Status. Sáu slice có code cần closure riêng, không phát hành lại task code. Không tự coi source hiện có là đã qua acceptance mới.

| Candidate | Slice/task và phạm vi | Module primary | Dependency/gate | Acceptance dự kiến/nguồn đối chiếu |
| --- | --- | --- | --- | --- |
| M3-P01 | M3.1 — model CPT và 9 loại field | CNT-01/02 | M2.5 core validation | Source và [evidence M3.1](../../../verification/2026-11-08-m3-1-content-domain.md); không mặc định task Done |
| M3-P02 | M3.2 — CPT SQLite và GET/POST Admin | CNT-01 | P01, FND-02/03 | CRUD tạo/liệt kê, RBAC/CSRF, migration/restart theo [evidence M3.2](../../../verification/2026-11-08-m3-2-content-types-backend.md) |
| M3-P03 | M3.3 — Entry CRUD và field value validation | CNT-02/04 | P02, concurrency/If-Match | POST/PUT/DELETE/ETag, transaction, restart theo [evidence M3.3](../../../verification/2026-11-08-m3-3-content-entry-crud.md) |
| M3-P04 | M3.4 — Admin Content form | CNT-01/02/04 | P03, FND-05 | 9 field form, 401/412/draft, browser theo [evidence M3.4](../../../verification/2026-11-08-m3-4-admin-content-ui.md) |
| M3-P05 | M3.5 — Media raster, thumbnail, picker | CNT-05/02 | P03/P04, upload security | JPEG/PNG/WebP upload/refs/picker theo [evidence M3.5](../../../verification/2026-11-09-m3-5-media-library.md), video/xóa còn thiếu |
| M3-P06 | M3.6 — Public CPT archive/single, media ref ACL | CNT-04/05 | P03/P05, CSP/escaping | Public routes, XSS/privacy, theme fallback theo [evidence M3.6](../../../verification/2026-11-09-m3-6-public-content-rendering.md) |
| M3-P07 | **Built-in `post`/`page` core-owned** và schema/seed an toàn | CNT-01/02 | P02/P03; Full-Track G2 core namespace, schema/upgrade/rollback | Fresh install + upgrade có `post`/`page` tự có, không trùng key hay nhân bản, không xóa CPT/entry cũ hoặc M1 `home_page`; editor không phải tự tạo CPT |
| M3-P08 | **Posts/Pages** trong Admin: list/create/edit/delete, tiêu đề/slug/body/ảnh đại diện và trạng thái draft/published tối thiểu | CNT-02/04 (FND-05) | P07/P04/P09; G2 status/visibility, concurrency | CRUD/ETag, form/image picker; **Lưu draft chỉ ghi DB, không public; Xuất bản mới công khai; Lưu published cập nhật public**; 401/412 giữ unsaved changes; không approval/revision/schedule |
| M3-P09 | **AdminShell Tailwind CSS** với sidebar/topbar/routing/responsive/RBAC | FND-05 (FND-02) | M1 auth, P04; G2 navigation/compatibility nếu đổi contract | Dashboard, Posts, Pages, Media, Appearance→Themes, Plugins, Users, Settings, Builder; Editor không truy cập chức năng Admin; giữ Builder/preview, CSRF/401/draft |
| M3-P10 | Dashboard cơ bản + lối tắt quản trị | FND-05 (CNT-04) | P09/P07; query theo quyền | Hiện số bài/trang theo đúng trạng thái, hành động nhanh và thông báo; không lộ dữ liệu/secret qua role |
| M3-P11 | **Media Library** thành trang độc lập, dùng lại upload/raster thumbnails/picker M3.5 | CNT-05 (FND-05) | P05/P09; giữ upload ACL | Upload/list/preview/chọn ảnh cho bài/trang, xử lý lỗi/restart; **chưa yêu cầu video, xóa/reconcile** |
| M3-P12 | **Users**: list/create/edit/role/password, bảo vệ tài khoản Admin cuối | FND-02/05 | M1 auth, P09; Full-Track G2 RBAC/session/password lifecycle | Admin quản lý tài khoản Admin/Editor an toàn, Editor bị 403, mật khẩu hash, không tự khóa admin cuối, session cũ bị thu hồi theo policy |
| M3-P13 | **General Settings** tối thiểu: site title/tagline/timezone, lưu DB + màn Admin | SITE-01 (FND-05) | P09, FND-03; Full-Track G2 storage/validation/ownership | Chỉ Admin sửa, đọc lại sau restart, site title/tagline có hiệu lực tại nơi hỗ trợ; không kéo Theme Options/SMTP/SEO lên M3 |
| M3-P14 | **Appearance → Themes**: upload ZIP, danh sách và activate **theme cơ bản** | THM-01 (FND-05) | M1 ZIP/Builder, P09; Full-Track G2 active theme vs active block, compatibility/rollback | Cài/list/activate có trạng thái rõ; đổi theme không xóa post/page/media và không phá homepage; ZIP invalid/traversal bị chặn; không bắt buộc import/template composition/customizer/update |
| M3-P15 | **Plugins**: màn Admin độc lập dùng ZIP install/list/enable/disable baseline | PLG-01 (FND-05) | M2/M2.5 plugin, P09; giữ worker/security contract | Admin cài và bật/tắt plugin ZIP, hiển thị lỗi/quyền/trạng thái; Editor bị chặn, worker lỗi không làm sập Admin; **không bắt buộc bridge đăng ký CPT/fields M3** |
| M3-P16 | Public Post/Page cơ bản và preview/visibility | CNT-04 (SITE-02) | P06/P07/P08; Full-Track G2 slug/status/URL/CSP | Chỉ published có URL ổn định (có thể dùng `/content/post/{slug}`, `/content/page/{slug}`); draft không lộ public/archive/media tham chiếu, preview cần auth; **Lưu published cập nhật public, Lưu Builder giữ hành vi cũ trên `/`** |
| M3-P17 | **E2E usable core trên macOS** và hồi quy bảo mật dữ liệu | CNT-01/04 (FND-02/05) | P07..P16 + G2/G3/CI mỗi task | Fresh install→bootstrap/login→post/page→media→publish/public→users/settings→theme/plugin→logout/login/restart; quyền/CSRF/401/412/XSS/ACL không hồi quy; báo cáo SHA/OS |
| M3-P18 | Windows SQLite smoke harness deterministic cleanup (**A0**, lịch sử #15) | OPS-02 (FND-04) | P02/P03/P05/P06; test-only Fast-Track nếu không đổi contract | 4 smoke scripts exit 0 trên Windows, connection close/temp cleanup đúng, không bỏ assertions |
| M3-P19 | Windows core CMS parity và G5 M3 closure; Linux **early smoke** riêng (**A1**) | OPS-02 (FND-04/FND-05) | P17/P18, Windows runner; G5 independent review | Windows release binary + Admin E2E P17, migration/restart/security; macOS và Windows ghi kết quả riêng; Linux smoke nếu có runner, nếu thiếu = `NOT_RUN` **và V1 Linux release vẫn M8**; không đóng M3 nếu acceptance M3 hoặc parity Windows chưa đạt |

**Primary acceptance và tránh trùng:** M3-P01..P06 là **đối chiếu bằng chứng**, P07..P17 là sản phẩm Core CMS (không mặc định đã triển khai), P18/P19 giữ A0/A1 kiểm chứng Windows. B taxonomy → [M4](../M4/TASK_PLAN.md), C plugin CPT/fields bridge → [M6](../M6/TASK_PLAN.md), D video → [M7](../M7/TASK_PLAN.md) và delete/reconcile → [M8](../M8/TASK_PLAN.md). E Builder AST → M4. Các quyết định G1/G2 #15 vẫn là input cần đánh giá lại, **không tự trở thành gate đã duyệt hay task G3**.

## 1. Việc có code ≠ task đã Done

Sáu [plan M3.1–M3.6](README.md#các-slice-và-nguồn-kiểm-chứng) là hồ sơ thực hiện *theo slice*, có bằng chứng trên macOS trong phạm vi tương ứng. **Chúng không phải sáu GitHub implementation task đã được gán ID G3 và đóng nghiệm thu.** M3 tổng vẫn **PARTIAL**, chưa `CLOSED`; kiểm tra source và acceptance của mỗi năng lực trước khi tạo task mới để không giao lại phần đã làm.

| Lớp | Nguồn cập nhật | Điều kiện được coi là đã hoàn thành |
| --- | --- | --- |
| **Scope/design** | [PRD](../../../product/PRODUCT_REQUIREMENTS.md), brief/ADR đã review/merge | G0/G2 được chốt đúng phạm vi, không suy từ source đã có |
| **Kế hoạch milestone/slice** | [Roadmap](../../ROADMAP_V1.md), [M3 overview](README.md), tài liệu này | Nêu đủ ranh giới và gate; **bản kế hoạch không có trạng thái Done** |
| **Task execution** | [GitHub Project #3](https://github.com/users/thientran2359/projects/3) và Issue task | `Backlog` (chưa G3) → `Ready` (G3 approved) → `In progress` (claimed) → `In review` (PR/CI hoặc chưa G5) → `Done` (**G5 VERIFIED đúng acceptance + chấp thuận đóng**) |
| **Code/review** | PR head/base SHA, reviewer độc lập, CI | `SUBMITTED/REVIEWED/MERGED` không tự tương đương `Done` |
| **Verification** | [docs/verification](../../../verification/README.md) | Đúng binary/SHA, hệ điều hành/runner, lệnh, kết quả, hành vi phủ và giới hạn; Windows/Linux không kế thừa PASS macOS |

`BLOCKED` là nguyên nhân/next action ghi trong Issue, không được che bằng cách chuyển `Ready` hoặc `Done`. Không lập danh sách `pending/doing/done` hoặc mirror YAML/Markdown giữ trạng thái song song.

## 2. Giữ kết quả thảo luận #15 làm input kỹ thuật, không buộc closure M3 mới

**A0/A1/B/C/D/E** là nhóm trong Final Decision #15, **không phải Task ID G3**. Owner đã thay đổi **G0 scope M3 thành phương án A**; A0/A1 vẫn ở M3-P18/P19, còn B/C/D được hoạch định tại M4/M6/M7/M8 theo capability, E ở M4. G1/G2 kỹ thuật đã nghiên cứu không tự động thành phê duyệt mới. Chỉ coordinator phát hành Task ID/GitHub Issue sau gate; không tái sử dụng `M3-S01-T01/T02`, `M3-S04-T01/T02` lịch sử. Bảng sau để bảo toàn rationale, **không phải backlog M3 phải hoàn thành**.

| Nhóm và ưu tiên | Nội dung/primary acceptance dự kiến | Phụ thuộc chặn và ranh giới | Gate trước task implementation |
| --- | --- | --- | --- |
| **A0 — M3-P18** | Làm ổn định bốn smoke harness Windows, đóng SQLite connection và dọn site tạm kể cả success/failure | Chỉ bốn Python scripts; không đổi CMS/storage/API. [Evidence teardown](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6749382579) ghi `WinError 32`, không phải PASS | Fast-Track nếu test-only, G3/handoff + Windows runner |
| **A1 — M3-P19** | Windows release parity trên CMS core M3 mới, security/upgrade/restart/Admin browser; Linux early smoke riêng | Sau A0 merge và core M3 P17 có evidence; thiếu Linux runner ghi `NOT_RUN`, Linux release đầy đủ thuộc M8 | G5 theo binary/SHA/OS/runner; không suy PASS |
| **B — chuyển M4-P09/P10** | CNT-03 + CNT-04 taxonomy: category/tag, entry association/filter/restart/RBAC | Full-Track G2 term ID/slug/cycle/depth/delete policy trước migration; public taxonomy URL phối hợp M5 | G1/G2 độc lập, G3/file ownership khi M4 chuẩn bị triển khai |
| **C — chuyển M6-P08** | CNT-01/02 × PLG-03: plugin đăng ký CPT/fields, bảo toàn data khi disable/restart/reinstall | Full-Track G2 manifest/namespace/lifecycle/rollback; không code song song trên schema/router đang tranh chấp với B nếu chưa phân quyền | G2 mới, G3/review độc lập, không dùng #15 làm phê duyệt |
| **D — chuyển M7-P08/M8-P08** | CNT-05 video an toàn; media delete/tombstone/reconcile/backup boundary | Video và lifecycle là V1 nhưng không chặn usable core M3; không hard-delete referenced media; backup engine M8 | G2 codec/MIME/Range/ACL; G2 atomic data/files/restore/failpoints |
| **E — nghiên cứu M4, không tính là M3 Done** | BLD-02/03 × BLD-01: AST, Section→Row→Column→Element, descriptors/tree/canvas, legacy one-block, preview chưa Save và theme layout ownership | Chỉ nghiên cứu G1/G2 sớm, không mở M4 code tùy tiện. Binding/query public BLD-05 chờ CNT/SITE-02/THM-02 | G1/G2 song song nếu có writer riêng; triển khai M4 theo milestone/gate riêng |

**Định hướng ownership lịch sử từ #15:** Atlas điều phối A0/A1 và Windows runner; Nova reviewer độc lập; Harbor là writer ứng viên nghiên cứu B; Orion ứng viên nghiên cứu E. **Không phải handoff hiện hành**. G0 mới không tự giao ai triển khai; coordinator xác nhận ownership, gate và reviewer trước khi tạo task.

### A0 — điều kiện nhận và nghiệm thu cụ thể

- [ ] Handoff chỉ định chính xác bốn file `cms/scripts/content_types_smoke.py`, `content_entries_smoke.py`, `media_smoke.py`, `public_content_smoke.py` và một báo cáo mới `docs/verification/` có tên xác định; pin `source_sha`/`base_sha`, reviewer và runner Windows.
- [ ] SQLite connection đóng deterministically trên cả luồng thành công/lỗi, không đổi transaction semantics; không swallow exception, bỏ assertion hoặc nới kiểm tra để có exit 0.
- [ ] Chạy lại bốn scripts với CMS binary/SHA tương ứng, ghi process exit code **và** temp cleanup; Rust workspace tests phải có final exit/result, không dùng từng test riêng để kết luận cả workspace.
- [ ] Admin browser M3.4 được lập **bài kiểm tra riêng**; bốn harness không chứng minh browser coverage. Nếu phải sửa CMS/DB/API: **Stop-and-Propose** scope Full-Track, không lách gate.

### A1 — điều kiện nghiệm thu parity riêng theo OS

- [ ] Windows release binary mới build trên SHA đã merge; test nâng cấp migration 0001→latest trên **DB/site copy**, core post/page seed/slug/status/visibility, validation/ETag 412, restart, media unreferenced 404/referenced 200 và remove last ref 404.
- [ ] Kiểm auth/RBAC, CSRF/Origin, lỗi 401/403/412, plugin package worker/disable/recovery; Admin browser với dữ liệu thực, không dùng debug/unit thay release smoke.
- [ ] Ghi riêng macOS/Windows/Linux: runner, binary, SHA, lệnh, fixture, mã thoát, screenshots và lỗi còn mở. Thiếu Linux runner = `NOT_RUN`, **không nói Linux VERIFIED**; M3 closure chỉ theo G5 macOS/Windows core được chốt, Linux release đầy đủ M8.

### B/C/D — điều kiện nghiệm thu sau khi G2 được duyệt

- **B (M4):** CRUD category/tag, kiểm cycle/depth/slug collision/deletion/reparenting, entry links và filter, quyền, restart/migration bảo toàn; không thêm public URL taxonomy trước G2 SITE-02.
- **C (M6):** cùng schema/data core khi plugin enable/disable/restart/reinstall; Editor vẫn quản lý nội dung giữ lại, plugin disable không chạy logic; collision/invalid schema/versioning/atomic rollback, không copy dữ liệu sang plugin.
- **D (M7/M8):** xóa asset theo chính sách đã duyệt, crash/DB/file failpoints, reconcile idempotent, concurrent upload/delete và snapshot DB+uploads khi DB ngoài root; video MIME/codec/Range/size chốt và test riêng; backup engine chỉ M8.

Các acceptance trên **là checklist soạn kế hoạch**, không phải checkbox nghiệm thu đã được tick, G2 đã `AGREED`, task `READY` hay mã nguồn đã chạy test.

## 3. Trước khi coordinator tạo một Issue triển khai

1. **Xác minh chưa làm trùng:** fetch `origin/main`, search cả [open/closed Issues](https://github.com/thientran2359/opendash-cms/issues), PR, [M3 snapshot](../../M3_STATUS_2026-11-09.md) và evidence SHA/OS. Giải quyết khác biệt `M3.1..M3.6` vs `M3-Sxx`; không tái dùng ID lịch sử.
2. **Chốt gate theo ảnh hưởng:** G0 scope, G1 source/research, G2 schema/API/ownership/security/rollback nếu Full-Track; G3 breakdown và task template theo [SLICE_TASK](../../templates/SLICE_TASK.md). A0 chỉ Fast-Track nếu đúng test-only. Tất cả mã task `Mx-Sxx-Txx` chưa được phát hành đều là **unassigned**, không tự tạo ID.
3. **Handoff cụ thể:** một implementer, một independent reviewer, coordinator, primary acceptance slice, task ID **sau khi chốt**, exact writable paths (không dùng glob), dependency, exclusions, tests, `source_sha` và `base_sha`, rollback. Nếu hai task trùng `content.rs`, `storage/content.rs`, `app.rs`, migration hoặc Admin: xếp thứ tự ownership trước khi giao.
4. **Publish qua GitHub khi đủ điều kiện:** G3 `READY` đã review/merge vào main và Issue đáp ứng [GitHub Workflow](../../../../.agents/GITHUB_WORKFLOW.md), coordinator mới tạo/đồng bộ Issue, xác minh [Project #3](https://github.com/users/thientran2359/projects/3) `Ready`. Agent pickup xác minh lại; nếu thiếu Project Status, gate hay owner thì `BLOCKED/NEEDS_VERIFICATION`, **không tự chuyển**.
5. **Kết thúc:** reviewer/CI kiểm PR, sau merge vẫn phải G5 theo acceptance & OS. Chỉ chuyển Project `Done` khi có evidence và chấp thuận đóng; việc viết plan/đổi đường dẫn hay test PASS một slice không đủ.

**Không nằm trong lượt này:** tạo GitHub implementation task, đổi label/Project Status, sửa `docs/plans/modules/`, điều chỉnh G2 lịch sử, chạy runtime hoặc phân công agent/runner. Những việc này cần handoff và gate riêng.
