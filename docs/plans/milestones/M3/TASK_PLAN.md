# M3 — Kế hoạch chuyển phần việc còn mở thành task có thể nghiệm thu

**Loại tài liệu:** kế hoạch tĩnh về **phạm vi, thứ tự, dependency và gate**; **không phải bảng trạng thái Backlog/Ready/In progress/In review/Done**. [GitHub Project #3](https://github.com/users/thientran2359/projects/3) và [GitHub Issues](https://github.com/thientran2359/opendash-cms/issues) giữ trạng thái thực tế, assignee, blocker và acceptance checklist theo từng task; [GitHub PR](https://github.com/thientran2359/opendash-cms/pulls) giữ diff/review. Không suy status GitHub Project chỉ từ một file, commit, test hoặc PR.

**Nguồn lập kế hoạch:** [Issue #15 — Final Decision của Atlas](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410), [Roadmap V1](../../ROADMAP_V1.md), [Module Catalog](../../MODULE_CATALOG_V1.md), [M3 status snapshot](../../M3_STATUS_2026-11-09.md), [Planning Workflow](../../PLANNING_WORKFLOW.md). Baseline source được quyết định đối chiếu: `main@e4a143291cbaaf3a302cd30e974e27ea9ab886f0` (10/11/2026). **Không chạy lại CMS test trong lượt soạn plan này**; evidence M3.1–M3.6 nằm ở từng SHA/OS trong [verification](../../../verification/README.md).

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

## 2. Phần cần làm tiếp theo — candidate, không phải task READY

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
