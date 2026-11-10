# M3 — Phân rã đầy đủ task Content & Media

**Loại tài liệu:** kế hoạch tĩnh về **phạm vi, thứ tự, dependency và gate**; **không phải bảng trạng thái Backlog/Ready/In progress/In review/Done**. [GitHub Project #3](https://github.com/users/thientran2359/projects/3) và [GitHub Issues](https://github.com/thientran2359/opendash-cms/issues) giữ trạng thái thực tế, assignee, blocker và acceptance checklist theo từng task; [GitHub PR](https://github.com/thientran2359/opendash-cms/pulls) giữ diff/review. Không suy status GitHub Project chỉ từ một file, commit, test hoặc PR.

**Nguồn lập kế hoạch:** [Module Catalog](../../MODULE_CATALOG_V1.md) → [Roadmap V1](../../ROADMAP_V1.md) → [M3 milestone](README.md) → các task candidate; [ma trận coverage toàn V1](../COVERAGE_MATRIX.md), [M3 status snapshot](../../M3_STATUS_2026-11-09.md), [Planning Workflow](../../PLANNING_WORKFLOW.md). [Issue #15 — Final Decision của Atlas](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410) chỉ chốt **một phần ưu tiên nghiên cứu/kiểm chứng**, không phải danh sách đầy đủ module→milestone→task như chủ dự án yêu cầu. Baseline khảo sát `main@e4a143291cbaaf3a302cd30e974e27ea9ab886f0`, tài liệu kế hoạch đã merge ở `main@5287811a208be26a3007831afc5fed926de08f66`. **Không chạy lại CMS test trong lượt soạn plan này**; evidence M3.1–M3.6 nằm ở từng SHA/OS trong [verification](../../../verification/README.md).

## M3 đầy đủ — mapping module → slice → task candidate

M3 phải bao quát **CNT-01, CNT-02, CNT-03, CNT-04, CNT-05 và phần bridge tối thiểu PLG-03**, với FND/OPS làm prerequisite. Những `M3-Pxx` chỉ là **planning IDs nội bộ** (không phải `M3-Sxx-Txx` G3 IDs, GitHub Issue hay Project Status). Đối với sáu slice có code, cần **kiểm tra closure riêng** chứ không tạo lại implementation task. Thứ tự G1/G2 của #15 được gắn dưới task phụ thuộc, không thay cấu trúc milestone.

| Candidate | Slice/task và phạm vi | Module primary | Dependency/gate | Acceptance dự kiến/nguồn đối chiếu |
| --- | --- | --- | --- | --- |
| M3-P01 | M3.1 — model CPT và 9 loại field | CNT-01/02 | M2.5 core validation | Source và [evidence M3.1](../../../verification/2026-11-08-m3-1-content-domain.md); không mặc định task Done |
| M3-P02 | M3.2 — CPT SQLite và GET/POST Admin | CNT-01 | P01, FND-02/03 | CRUD tạo/liệt kê, RBAC/CSRF, migration/restart theo [evidence M3.2](../../../verification/2026-11-08-m3-2-content-types-backend.md) |
| M3-P03 | M3.3 — Entry CRUD và field value validation | CNT-02/04 | P02, concurrency/If-Match | POST/PUT/DELETE/ETag, transaction, restart theo [evidence M3.3](../../../verification/2026-11-08-m3-3-content-entry-crud.md) |
| M3-P04 | M3.4 — Admin Content form | CNT-01/02/04 | P03, FND-05 | 9 field form, 401/412/draft, browser theo [evidence M3.4](../../../verification/2026-11-08-m3-4-admin-content-ui.md) |
| M3-P05 | M3.5 — Media raster, thumbnail, picker | CNT-05/02 | P03/P04, upload security | JPEG/PNG/WebP upload/refs/picker theo [evidence M3.5](../../../verification/2026-11-09-m3-5-media-library.md), video/xóa còn thiếu |
| M3-P06 | M3.6 — Public CPT archive/single, media ref ACL | CNT-04/05 | P03/P05, CSP/escaping | Public routes, XSS/privacy, theme fallback theo [evidence M3.6](../../../verification/2026-11-09-m3-6-public-content-rendering.md) |
| M3-P07 | CPT schema change, built-in post/page và owner lifecycle còn thiếu | CNT-01 | P02, Full-Track G2 schema/upgrade/backward compatibility | Update/withdraw/restore, namespace conflict; provider disable/restart không xóa entry; giữ M1 `home_page` |
| M3-P08 | Taxonomy CRUD: category cha–con và tag qua Admin | CNT-03 | P02/P03, **B G1/G2** term identity/slug/depth/cycle/ownership | CRUD, RBAC/CSRF, slug collision, no cycles, delete/reparent policy, restart/upgrade bảo toàn entry |
| M3-P09 | Term↔entry links, Admin filter/query taxonomy | CNT-03/04 | P08 và P03; G2 cardinality/concurrency | Gắn nhiều term, lọc entry, remove term không xóa entry, ETag và restart; public taxonomy URL chờ M5 |
| M3-P10 | CPT/field plugin bridge tối thiểu & provider persistence | PLG-03 (CNT-01/02) | P07..P09, **C G1/G2**; code sau B để tránh trùng file | Plugin declare CPT/fields, collision/versioning, disable/restart/reinstall dữ liệu user còn quản lý được; blocks/templates/settings sau M6 |
| M3-P11 | Media video V1: format, serving/metadata/limits | CNT-05 | P05; **D Full-Track G2** codec/MIME/Range/size/privacy | Video hợp lệ phục vụ theo policy, invalid media/Range/permissions bị chặn; không tự lùi phạm vi V1 |
| M3-P12 | Media delete/referenced policy, reconcile, backup boundary | CNT-05 (OPS-01) | P05/P06, **D G2** consistency/GC/restore | No unsafe hard-delete khi còn refs, failpoint/restart/reconcile/idempotency, DB có thể ở ngoài site root; backup engine M8 |
| M3-P13 | Windows SQLite smoke harness deterministic cleanup (**A0**) | OPS-02 (FND-04) | P02/P03/P05/P06 source, test-only Fast-Track nếu không đổi contract | 4 smoke scripts exit 0 trên Windows, connection close và temp cleanup sạch, không bỏ assertions |
| M3-P14 | Windows release parity / Linux smoke riêng (**A1**) | OPS-02 (FND-04/CNT-05) | P13 merge, release binary/runner, negative matrix | Migration 0001→0007, auth/CSRF/ETag, entry/media/restart/plugin/browser; Linux riêng `NOT_RUN` khi thiếu runner |
| M3-P15 | G5 M3 cross-module content E2E và closure | CNT-01–05 (PLG-03) | P07..P14 + G2/G3/CI từng task; M3 evidence theo OS | Content taxonomy/media/provider disable/re-enable, CRUD/persistence/security, upgrade và frontend; không đóng M3 nếu còn V1 scope hoặc parity gap |

**Primary acceptance và tránh trùng:** M3-P01..P06 là **đối chiếu nguồn đã có**, không phát hành sáu task code trùng. P08/P09 nối B, P10 nối C, P11/P12 nối D, P13/P14 nối A0/A1 của #15. Nghiên cứu E (BLD-02/03) thuộc [M4/TASK_PLAN](../M4/TASK_PLAN.md), không tính là M3 implementation/closure.

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

## 2. Bổ sung quyết định ưu tiên #15 cho task candidate — không phải toàn bộ M3

Các ký hiệu **A0/A1/B/C/D/E** là **nhóm công việc trong Final Decision #15**, **không phải Task ID G3**. Chỉ coordinator phát hành task với ID và GitHub Issue sau khi gate/handoff chính thức đủ điều kiện. Không tự tái sử dụng các số `M3-S01-T01/T02`, `M3-S04-T01/T02` đã retired trong discovery cũ; `M3-S01..S05` ở Roadmap là **slice đề xuất lịch sử, không ánh xạ 1:1** tới M3.1–M3.6 đã code.

| Nhóm và ưu tiên | Nội dung/primary acceptance dự kiến | Phụ thuộc chặn và ranh giới | Gate trước task implementation |
| --- | --- | --- | --- |
| **A0 — đầu tiên** | Làm ổn định bốn smoke harness Windows, đóng SQLite connection và dọn site tạm đúng cách kể cả success/failure | Chỉ bốn script Python đã chỉ định; không sửa CMS/storage/API/DB. [Windows teardown evidence](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6749382579) ghi `WinError 32`, không phải PASS | Fast-Track **chỉ khi** không đổi contract, cần G3/handoff + runner Windows |
| **A1 — sau A0** | Windows **release** parity trên SHA mới merge: migration 0001→0007, CPT/entry/restart, private/public media, security negative tests, plugin worker/recovery và Admin browser; Linux smoke riêng | Phải có A0 đã review/merge và binary release/runner đúng OS; thiếu Linux runner ghi `NOT_RUN`, không kết luận parity | Handoff test matrix, exact binary/SHA, regression + evidence G5; không tự làm mọi M3 test thành PASS |
| **B — G1/G2 tiếp theo** | CNT-03 + CNT-04 taxonomy: category cha–con/tag, entry association/filter/restart, RBAC/CSRF và bảo toàn entry cũ | Full-Track, chốt term ID/slug scope/cycle/depth/ownership/delete/reparent/concurrency trước migration/API; public taxonomy URL/SEO chờ SITE-02 | G1 research → **G2 AGREED** độc lập → G3 task+ownership, không code từ roadmap cũ |
| **C — research có thể song song B, code sau B** | CNT-01/02 × PLG-03: plugin đăng ký CPT/fields qua canonical core schema, disable/restart/re-enable/reinstall vẫn quản trị được dữ liệu; từ chối collision/invalid declarations | Full-Track, B và C dùng chung storage/router/migration nên mặc định không code song song; taxonomy bridge sau B. Blocks/templates/settings ngoài slice đầu | Chốt manifest/ABI/namespace/lifecycle/schema evolution/atomic rollback trong G2; G3, review, file ownership |
| **D — G2 sớm, code sau contract** | CNT-05 × OPS-01: referenced-delete/tombstone/GC, missing-file diagnostic, crash/restart reconcile, private/public refs, backup boundary; video V1/M3 cần nghiên cứu định dạng/limits | **Không tự hard-delete/GC** khi còn uncertain references/restore. `OPENDASH_DB_PATH` có thể ở ngoài site root; backup engine thuộc M8, không gộp vào M3 | Full-Track G2 data safety, error/failpoint/recovery và rollback trước implementation |
| **E — nghiên cứu M4, không tính là M3 Done** | BLD-02/03 × BLD-01: AST, Section→Row→Column→Element, descriptors/tree/canvas, legacy one-block, preview chưa Save và theme layout ownership | Chỉ nghiên cứu G1/G2 sớm, không mở M4 code tùy tiện. Binding/query public BLD-05 chờ CNT/SITE-02/THM-02 | G1/G2 song song nếu có writer riêng; triển khai M4 theo milestone/gate riêng |

**Định hướng ownership từ #15:** Atlas điều phối A0/A1 và Windows runner; Nova reviewer độc lập; Harbor là writer ứng viên cho nghiên cứu **B** sau docs #14; Orion là ứng viên nghiên cứu **E** khi branch rảnh. **Đây không phải phân công/handoff đã phát hành.** C/D và Linux runner cần coordinator xác nhận người làm trước khi tạo task. Không giả định Forge rảnh hoặc một agent nhận hai implementation task.

### A0 — điều kiện nhận và nghiệm thu cụ thể

- [ ] Handoff chỉ định chính xác bốn file `cms/scripts/content_types_smoke.py`, `content_entries_smoke.py`, `media_smoke.py`, `public_content_smoke.py` và một báo cáo mới `docs/verification/` có tên xác định; pin `source_sha`/`base_sha`, reviewer và runner Windows.
- [ ] SQLite connection đóng deterministically trên cả luồng thành công/lỗi, không đổi transaction semantics; không swallow exception, bỏ assertion hoặc nới kiểm tra để có exit 0.
- [ ] Chạy lại bốn scripts với CMS binary/SHA tương ứng, ghi process exit code **và** temp cleanup; Rust workspace tests phải có final exit/result, không dùng từng test riêng để kết luận cả workspace.
- [ ] Admin browser M3.4 được lập **bài kiểm tra riêng**; bốn harness không chứng minh browser coverage. Nếu phải sửa CMS/DB/API: **Stop-and-Propose** scope Full-Track, không lách gate.

### A1 — điều kiện nghiệm thu parity riêng theo OS

- [ ] Windows release binary mới build trên SHA đã merge; test nâng cấp 0001→0007 trên **DB/site copy**, CPT/entry schema, validation/ETag 412, restart, media unreferenced 404/referenced 200 và remove last ref 404.
- [ ] Kiểm auth/RBAC, CSRF/Origin, lỗi 401/403/412, plugin package worker/disable/recovery; Admin browser với dữ liệu thực, không dùng debug/unit thay release smoke.
- [ ] Ghi riêng Windows và Linux: runner, binary, SHA, lệnh, fixture, mã thoát, screenshots nếu có và lỗi còn mở; thiếu runner Linux = `NOT_RUN`. Không phát biểu M3 hoặc V1 parity VERIFIED nếu matrix còn thiếu.

### B/C/D — điều kiện nghiệm thu sau khi G2 được duyệt

- **B:** CRUD category/tag, kiểm cycle/depth/slug collision/deletion/reparenting, entry links và filter, quyền, restart/migration bảo toàn; không tự thêm public URL taxonomy trước G2 SITE-02.
- **C:** cùng schema/data core khi plugin enable/disable/restart/reinstall; Editor vẫn quản lý nội dung giữ lại, plugin disable không chạy logic; kiểm collision/invalid schema/versioning và atomic rollback, không copy dữ liệu sang plugin.
- **D:** xóa asset đang tham chiếu theo chính sách đã duyệt, crash/DB/file failpoints, reconcile idempotent, concurrent upload/delete và snapshot DB+uploads khi DB ngoài root; video MIME/codec/Range/size được chốt và test riêng, không hứa backup engine M8 đã xong.

Các acceptance trên **là checklist soạn kế hoạch**, không phải checkbox nghiệm thu đã được tick, G2 đã `AGREED`, task `READY` hay mã nguồn đã chạy test.

## 3. Trước khi coordinator tạo một Issue triển khai

1. **Xác minh chưa làm trùng:** fetch `origin/main`, search cả [open/closed Issues](https://github.com/thientran2359/opendash-cms/issues), PR, [M3 snapshot](../../M3_STATUS_2026-11-09.md) và evidence SHA/OS. Giải quyết khác biệt `M3.1..M3.6` vs `M3-Sxx`; không tái dùng ID lịch sử.
2. **Chốt gate theo ảnh hưởng:** G0 scope, G1 source/research, G2 schema/API/ownership/security/rollback nếu Full-Track; G3 breakdown và task template theo [SLICE_TASK](../../templates/SLICE_TASK.md). A0 chỉ Fast-Track nếu đúng test-only. Tất cả mã task `Mx-Sxx-Txx` chưa được phát hành đều là **unassigned**, không tự tạo ID.
3. **Handoff cụ thể:** một implementer, một independent reviewer, coordinator, primary acceptance slice, task ID **sau khi chốt**, exact writable paths (không dùng glob), dependency, exclusions, tests, `source_sha` và `base_sha`, rollback. Nếu hai task trùng `content.rs`, `storage/content.rs`, `app.rs`, migration hoặc Admin: xếp thứ tự ownership trước khi giao.
4. **Publish qua GitHub khi đủ điều kiện:** G3 `READY` đã review/merge vào main và Issue đáp ứng [GitHub Workflow](../../../../.agents/GITHUB_WORKFLOW.md), coordinator mới tạo/đồng bộ Issue, xác minh [Project #3](https://github.com/users/thientran2359/projects/3) `Ready`. Agent pickup xác minh lại; nếu thiếu Project Status, gate hay owner thì `BLOCKED/NEEDS_VERIFICATION`, **không tự chuyển**.
5. **Kết thúc:** reviewer/CI kiểm PR, sau merge vẫn phải G5 theo acceptance & OS. Chỉ chuyển Project `Done` khi có evidence và chấp thuận đóng; việc viết plan/đổi đường dẫn hay test PASS một slice không đủ.

**Không nằm trong lượt này:** tạo GitHub implementation task, đổi label/Project Status, sửa `docs/plans/modules/`, điều chỉnh G2 lịch sử, chạy runtime hoặc phân công agent/runner. Những việc này cần handoff và gate riêng.
