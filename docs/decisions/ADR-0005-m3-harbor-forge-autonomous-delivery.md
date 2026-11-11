# ADR-0005 — M3 thực hiện tự chủ bởi Harbor và Forge

> **Cập nhật phân quyền:** [ADR-0010](ADR-0010-agent-project-and-merge-authority.md) là nguồn hiện hành về vai trò, tự chủ triển khai, review và merge. Các quy định cũ dưới đây được giữ làm lịch sử quyết định.

- **Trạng thái:** **AGREED — owner direct, 2026-11-11**, áp dụng sau khi tài liệu này merge vào `main`.
- **Phạm vi:** chỉ **M3 — Core CMS & WordPress-like Admin** và các PR/tác vụ M3 phát hành sau quyết định này; không sửa hồi tố hồ sơ, không ủy quyền thay đổi các dự án/phase khác.
- **Nguồn:** owner trực tiếp yêu cầu Harbor (CoS macOS) và Forge (CoS Windows) chọn và triển khai **toàn bộ M3 từng task từ đầu đến cuối**, **không gọi thêm agent khác review**, đồng thời đồng ý quy trình Harbor đề xuất: agent tự lập G2/G3 phù hợp, triển khai, kiểm thử, tạo PR và **tự merge khi gate PASS**. Xác nhận trong Chat On Steroids ngày 2026-11-11; không tạo URL chat không tồn tại.
- **Baseline:** `main@35fd12f657095602ab43f9a0ca3506d0f97e728c`, [ADR-0004 Post/Page](ADR-0004-m3-post-page-publication-boundary.md), [M3 TASK_PLAN](../plans/milestones/M3/TASK_PLAN.md), [Project #3](https://github.com/users/thientran2359/projects/3). Tác giả ban đầu: **Harbor (CoS macOS)**.
- **Ngoại lệ:** ghi nhận thay đổi **riêng M3** so với [ADR-0003](ADR-0003-agent-consensus-governance.md) và các quy trình/skill trước đó yêu cầu Atlas/Nova, coordinator/reviewer độc lập và cấm self-merge. Không giả là có peer review. Mọi phạm vi ngoài M3 vẫn theo governance chung.

## Phân quyền và cách phối hợp

1. **Hai agent duy nhất:** Harbor và Forge có quyền điều phối M3, kiểm tra gap/dependency, tự chốt G2 trong giới hạn G0/PRD đã duyệt, chuẩn bị và promote G3, nhận/task reassignment và triển khai trên nhánh/task của chính mình. Không chờ Atlas/Nova/Orion, không bắt buộc review chéo hay owner xác nhận lại mỗi task. Khi cần thiết kế mới, agent phải ghi căn cứ kỹ thuật, phương án, tác động và tests trước khi triển khai; không thể tự thay đổi mục tiêu sản phẩm vượt G0 M3.
2. **Một task — một writer — một phạm vi:** chỉ pickup khi Issue đủ nguồn G3 đã merge, `source_sha/base_sha`, dependency, danh sách file cụ thể và GitHub Project #3 `Ready` được **xác minh**. Agent tự chọn task phù hợp theo dependency/priority và không giẫm lên file agent kia. Chỉ nhận tối đa một task đang triển khai trên mỗi agent; phối hợp tại Issue/PR. Task chưa đủ gate giữ Backlog/Blocked, không tự phát hiện một candidate rồi coi là Ready.
3. **Branch riêng cho từng task M3**, ví dụ `task/harbor-m3-s20-t01` và `task/forge-m3-s21-t01`, bắt đầu từ `origin/main` mới nhất; agent có thể sử dụng worktree riêng của mình. Không rewrite/reset/force-push nhánh `agent/forge` hiện lệch main hoặc PR #7 đang mở, không làm thay đổi PR #9, watcher hay Issue #5. Một checkout/branch cũ có PR khác **không** bị tái dùng để lách ownership.
4. **Tự merge được ủy quyền** cho chính tác giả M3 **chỉ sau** PR tới `main`, head/base SHA được re-fetch, required checks trên **đúng head** thành công, mergeable, mọi tests và negative acceptance của task PASS, không còn high-severity security/data blocker. Dùng `expected_head_sha`; không cần approval trên GitHub từ agent khác. Không bypass ruleset, disable CI, push trực tiếp main, force-push hay tự nhận kết quả test chưa chạy. Nếu API merge timeout, fetch lại PR/main trước khi quyết định retry.
5. **G5 là riêng:** mỗi task chỉ `VERIFIED` khi đã có test thực tế, lệnh, result, fixture, SHA và OS đúng acceptance; main/PR merged không tự là Done. Project #3 là nguồn trạng thái, `TASK_INDEX.md` là snapshot đồng bộ sau khi Issue thực được phát hành hoặc G5+Project Done. Harbor kiểm nghiệm macOS; Forge kiểm nghiệm Windows; không kế thừa PASS giữa hai hệ điều hành. Không tạo watcher/cron/remote-input hoặc tự động gọi Forge khi không có phiên đã xác minh.

## Gate an toàn không được miễn

- Giữ bắt buộc: auth/RBAC, CSRF/Origin, SQL/HTML escaping, XSS/CSP, media ACL và draft privacy, data preservation/backup, atomic migration/rollback plan trên **DB copy**, legacy Builder `/`, Rust/build/browser tests thích hợp, GitHub CI và branch protection.
- Với Post/Page theo [ADR-0004](ADR-0004-m3-post-page-publication-boundary.md): tuyệt đối không merge/deploy seed-only làm lộ draft/media; nếu không chứng minh được giai đoạn fail-closed A/B, gộp A+B+C thành một PR/release an toàn. Không sửa migration 0001–0007, không tự đổi dữ liệu thật để chữa key collision.
- **STOP/BLOCKED** khi thiếu quyền Project hoặc branch, acceptance không đạt, cần đổi G0/PRD, có rủi ro security HIGH/mất dữ liệu chưa có giải pháp hoặc thiếu bằng chứng required. Quyền không chờ peer review **không phải quyền self-risk-accept**. Có thể trình owner vấn đề sản phẩm hoặc quyền bất thường nếu thực sự cần, thay vì tuyên bố đã xong.

## Quy trình cho mỗi task M3

**VERIFY** main/Issue/Project/dependency/ownership → **PICK/CLAIM** một task hợp lệ → **IMPLEMENT** toàn bộ scope trên task branch → **TEST** và ghi evidence đúng SHA/OS → **PR** và đối chiếu CI/security → **SELF-MERGE** bằng expected SHA khi đủ gate → **VERIFY G5**, cập nhật Issue/Project, sync `TASK_INDEX.md` (một writer), sau đó xét task kế tiếp.

Khởi đầu song song không trùng file:

- **Harbor:** [M3-S20-T01 — Post/Page publication backend an toàn](../plans/milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md) (M3-P07/P08/P16; Rust/SQLite/public routes).
- **Forge:** [M3-S21-T01 — AdminShell Tailwind](../plans/milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md) (M3-P09; Admin frontend; không đổi API/DB).

Hai task chỉ thực sự pickup khi đặc tả G3 trên `main`, Issue và **Project #3 Ready** tồn tại; không giả Forge Windows đã nhận việc chỉ vì Issue hoặc label được tạo trên GitHub. Các task M3 tiếp theo được chọn từng task có dependency; đích nghiệm thu M3 vẫn cần P10–P19 và G5 macOS/Windows, không đóng mốc sớm.
