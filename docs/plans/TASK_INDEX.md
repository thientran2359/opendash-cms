# Danh sách milestone & task — OpenDash V1

**Bản tổng hợp cho người theo dõi công việc · đối chiếu GitHub Project #3 ngày 2026-11-11 · nguồn [ADR-0004 G2](../decisions/ADR-0004-m3-post-page-publication-boundary.md), [ADR-0005 M3 Harbor–Forge](../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md) và [G3 task source](milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md).**
Gồm **9 milestone / 82 planning candidates** theo các TASK_PLAN.md; bốn task M3 (#24/#25/#35/#36) đã Done/G5 đúng phạm vi. Các hàng khác vẫn chỉ là planning candidates, không tự thành Issue Ready; toàn M3 chưa đạt G5.
Danh sách này là **bản tóm tắt dễ đọc**; **[GitHub Issues](https://github.com/thientran2359/opendash-cms/issues) và [Project #3](https://github.com/users/thientran2359/projects/3)** mới là nguồn chính thức cho task thực tế, owner và trạng thái live. Nếu hai nơi khác nhau, **lấy GitHub làm chuẩn và sửa lại bảng**, không sửa GitHub để khớp bảng.

**Đọc trạng thái:** Chưa G3 = chỉ là ứng viên trong plan, chưa phải task thực thi/Issue Ready; G1/G2 — PROPOSED = đang nghiên cứu thiết kế, chưa chốt G2; khi được phát hành chính thức, **bổ sung hàng task ID dạng Mx-Sxx-Txx ngay dưới candidate**, gắn **Issue URL** và sao trạng thái đã kiểm tra từ GitHub Project (Backlog, Ready, In progress, In review, Done). Blocked phải có Issue liên kết giải thích. Chỉ đánh dấu Done sau **G5/acceptance VERIFIED và Project Done**; PR merged hoặc code đã tồn tại **không tự tương đương Done**. M1/M2 có **source baseline lịch sử**, M2.5 đã **VERIFIED theo phạm vi mốc**, và M3.1–M3.6 có **code/evidence theo slice**; các dòng này **không được tự nhận task G3 đã Done**. Nhãn có chữ “chưa G3” chỉ trạng thái **phát hành Issue/task**, không phủ nhận source/evidence đã có.

**Cách duy trì:** (1) coordinator tạo Issue G3 hợp lệ → thêm task ID + link Issue, không đổi ID candidate thành task ID; (2) agent đang làm chỉ cập nhật hàng đúng Issue khi Project Status được kiểm chứng, có thể đề xuất sửa bảng trong task PR nếu file ownership/không xung đột; (3) **khi task hoàn thành**, coordinator/reviewer kiểm Issue, acceptance, G5 và Project Done, rồi cập nhật hàng đó thành Done trong PR docs/handoff hoàn tất, kèm link Issue/PR/evidence; (4) nếu task thay đổi/blocked/reopen, cập nhật lại, không xóa lịch sử GitHub. **Một người sửa file index tại một thời điểm**, không để nhiều agent làm dirty cùng file; index không thay thế G2/G3 hoặc tự chạy watcher.

**Task hạ tầng ngoài 9 milestone CMS** có một mục riêng ở cuối file; Issue thảo luận/process không được tính như implementation task. Không gán hạ tầng vào milestone sản phẩm.

## [M1 — CMS Baseline](https://github.com/thientran2359/opendash-cms/milestone/1) (6)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M1-P01](milestones/M1/TASK_PLAN.md) | Bootstrap SQLite, migration, SitePaths mỗi site | Có source lịch sử (chưa G3) | — |
| [M1-P02](milestones/M1/TASK_PLAN.md) | Tạo Admin, session và RBAC Admin/Editor, CSRF/Origin | Có source lịch sử (chưa G3) | — |
| [M1-P03](milestones/M1/TASK_PLAN.md) | Admin shell và primitives tối thiểu để quản trị | Có source lịch sử (chưa G3) | — |
| [M1-P04](milestones/M1/TASK_PLAN.md) | Theme ZIP và Builder một block, Preview trước Save | Có source lịch sử (chưa G3) | — |
| [M1-P05](milestones/M1/TASK_PLAN.md) | Windows executable độc lập, health và assets đi kèm | Có source lịch sử (chưa G3) | — |
| [M1-P06](milestones/M1/TASK_PLAN.md) | Regression baseline M1 và kiểm chứng có thể tái hiện | Có source lịch sử (chưa G3) | — |

## [M2 — Plugin Sandbox Baseline](https://github.com/thientran2359/opendash-cms/milestone/2) (5)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M2-P01](milestones/M2/TASK_PLAN.md) | Đọc/kiểm tra ZIP plugin, manifest/ABI tối thiểu | Có source lịch sử (chưa G3) | — |
| [M2-P02](milestones/M2/TASK_PLAN.md) | Wasmi worker riêng, bounded resources/timeout | Có source lịch sử (chưa G3) | — |
| [M2-P03](milestones/M2/TASK_PLAN.md) | Permission guard, log và disable/re-enable cơ bản | Có source lịch sử (chưa G3) | — |
| [M2-P04](milestones/M2/TASK_PLAN.md) | Persistence plugin qua restart và Admin package actions | Có source lịch sử (chưa G3) | — |
| [M2-P05](milestones/M2/TASK_PLAN.md) | Windows plugin smoke + regression baseline | Có source lịch sử (chưa G3) | — |

## [M2.5 — Core Foundation](https://github.com/thientran2359/opendash-cms/milestone/3) (8)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M2.5-P01](milestones/M2.5/TASK_PLAN.md) | §4.1 Contract/Base APIs thật | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P02](milestones/M2.5/TASK_PLAN.md) | §4.2 Quy chuẩn tổ chức code | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P03](milestones/M2.5/TASK_PLAN.md) | §4.3 Registry ownership | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P04](milestones/M2.5/TASK_PLAN.md) | §4.4 Hook/Action thực tế | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P05](milestones/M2.5/TASK_PLAN.md) | §4.5 Đưa luồng có sẵn qua core | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P06](milestones/M2.5/TASK_PLAN.md) | §4.6 Khép bất đồng Base APIs | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P07](milestones/M2.5/TASK_PLAN.md) | §4.7 Mẫu mở rộng + checker | Mốc VERIFIED (chưa G3) | — |
| [M2.5-P08](milestones/M2.5/TASK_PLAN.md) | §4.8 Full regression + G5 | Mốc VERIFIED (chưa G3) | — |

## [M3 — Core CMS & WordPress-like Admin](https://github.com/thientran2359/opendash-cms/milestone/4) (19)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M3-P01](milestones/M3/TASK_PLAN.md) | M3.1 — model CPT và 9 loại field | Có code/evidence slice (chưa G3) | — |
| [M3-P02](milestones/M3/TASK_PLAN.md) | M3.2 — CPT SQLite và GET/POST Admin | Có code/evidence slice (chưa G3) | — |
| [M3-P03](milestones/M3/TASK_PLAN.md) | M3.3 — Entry CRUD và field value validation | Có code/evidence slice (chưa G3) | — |
| [M3-P04](milestones/M3/TASK_PLAN.md) | M3.4 — Admin Content form | Có code/evidence slice (chưa G3) | — |
| [M3-P05](milestones/M3/TASK_PLAN.md) | M3.5 — Media raster, thumbnail, picker | Có code/evidence slice (chưa G3) | — |
| [M3-P06](milestones/M3/TASK_PLAN.md) | M3.6 — Public CPT archive/single, media ref ACL | Có code/evidence slice (chưa G3) | — |
| [M3-P07](milestones/M3/TASK_PLAN.md) | Built-in `post`/`page` core-owned và schema/seed an toàn | Backend #24 **task G5 VERIFIED** (macOS + Windows); không suy ra toàn bộ M3 G5 | [ADR-0004](../decisions/ADR-0004-m3-post-page-publication-boundary.md) · [Issue #24](https://github.com/thientran2359/opendash-cms/issues/24) · [Windows evidence](../verification/2026-11-11-m3-s20-t01-post-page-publication-windows.md) |
| [M3-S20-T01](milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md) | Harbor: Post/Page backend, migration, publish, protected preview/public/media (P07/P08/P16) | **Done / task G5 VERIFIED** — Issue closed, Project #3 Done; macOS runtime + Windows HTTP/SQLite regression PASS, PR #29/#33 merged và CI Windows/Ubuntu PASS | [Issue #24](https://github.com/thientran2359/opendash-cms/issues/24) · [PR #29](https://github.com/thientran2359/opendash-cms/pull/29) · [PR #33](https://github.com/thientran2359/opendash-cms/pull/33) · [macOS evidence](../verification/2026-11-11-m3-s20-t01-post-page-publication-macos.md) · [Windows evidence](../verification/2026-11-11-m3-s20-t01-post-page-publication-windows.md) |
| [M3-P08](milestones/M3/TASK_PLAN.md) | Posts/Pages trong Admin: list/create/edit/delete, tiêu đề/slug/body/ảnh đại diện và trạng thái draft/published tối thiểu | **Backend #24 + UI #35 G5 VERIFIED theo task** — Posts/Pages CRUD, draft, explicit Publish/Unpublish và private preview đã được kiểm chứng Windows; không suy ra toàn bộ M3 G5 | [Issue #24 backend](https://github.com/thientran2359/opendash-cms/issues/24) · [Issue #35](https://github.com/thientran2359/opendash-cms/issues/35) · [PR #39](https://github.com/thientran2359/opendash-cms/pull/39) · [Windows G5](../verification/2026-11-11-m3-s22-t01-post-page-admin-windows.md) |
| [M3-S22-T01](milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md) | **Forge:** Posts/Pages Admin UI với CRUD, draft, publish/unpublish, preview bảo vệ và MediaRoute seam | **Done / task G5 VERIFIED** — Issue #35 closed, Project #3 Done; Windows Chrome/SQLite + 59 Rust tests PASS, CI Ubuntu/Windows SUCCESS tại head `3144a8f`, PR #39 merge `cfd8a6a`; hai script legacy có giới hạn ghi trong evidence. Seam đã giao Harbor #36. | [Issue #35](https://github.com/thientran2359/opendash-cms/issues/35) · [PR #39](https://github.com/thientran2359/opendash-cms/pull/39) · [Windows evidence](../verification/2026-11-11-m3-s22-t01-post-page-admin-windows.md) · [WP-D reference](milestones/M3/slices/2026-11-10-post-page-publication-g3.md) |
| [M3-P09](milestones/M3/TASK_PLAN.md) | AdminShell Tailwind CSS với sidebar/topbar/routing/responsive/RBAC | **Implemented, Windows VERIFIED** — #31 merged; không suy ra M3 toàn bộ đạt G5 | [Issue #25](https://github.com/thientran2359/opendash-cms/issues/25) · [PR #31](https://github.com/thientran2359/opendash-cms/pull/31) |
| [M3-S21-T01](milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md) | Forge: AdminShell Tailwind, sidebar/topbar, client navigation và RBAC UI | **Done / task G5 VERIFIED** — Issue closed, Project #3 Done, Windows browser/CI Ubuntu+Windows PASS tại PR head `71825ad`; merge `6cbfd19` | [Issue #25](https://github.com/thientran2359/opendash-cms/issues/25) · [PR #31](https://github.com/thientran2359/opendash-cms/pull/31) · [Windows evidence](../verification/2026-11-11-m3-s21-adminshell-windows.md) |
| [M3-P10](milestones/M3/TASK_PLAN.md) | Dashboard cơ bản + lối tắt quản trị | **Dashboard #50 task G5 VERIFIED trên Windows**, PR #60 đã merge; release M3 #53/#54 còn riêng | [Issue #50](https://github.com/thientran2359/opendash-cms/issues/50) · [Windows G5 evidence](../verification/2026-11-11-m3-s26-t01-dashboard-windows.md) |
| [M3-S26-T01](milestones/M3/slices/2026-11-11-m3-remaining-g3.md) | Forge · Dashboard và thống kê Post/Page | **Done / task G5 VERIFIED** — post-merge Windows 65/65 Rust, Dashboard API/Chrome 390px, auth/Content/Public/Settings regression PASS on `main@51aae56`; CI Ubuntu/Windows SUCCESS on PR head `f556ce7`; full M3 G5 pending | [Issue #50](https://github.com/thientran2359/opendash-cms/issues/50) · [PR #60](https://github.com/thientran2359/opendash-cms/pull/60) · [Windows evidence](../verification/2026-11-11-m3-s26-t01-dashboard-windows.md) |
| [M3-P11](milestones/M3/TASK_PLAN.md) | Media Library thành trang độc lập, dùng lại upload/raster thumbnails/picker M3.5 | **UI task #36 G5 VERIFIED** trên macOS — ảnh, upload, phân trang, Editor, route và MediaPicker; không suy ra toàn bộ M3 G5 | [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36) · [PR #38](https://github.com/thientran2359/opendash-cms/pull/38) · [PR #42](https://github.com/thientran2359/opendash-cms/pull/42) · [macOS evidence](../verification/2026-11-11-m3-s23-t01-media-library-macos.md) |
| M3-S23-T01 | **Harbor:** Standalone Media Library UI, upload/thumbnail/list, responsive, tích hợp vào AdminShell qua Forge seam | **Done / task G5 VERIFIED** — Issue #36 closed, Project #3 Done; macOS embedded Rust/Admin + Chrome/SQLite, Admin/Editor, 44 ảnh phân trang, upload hợp lệ/sai, private media/401, Content/Posts picker PASS; CI Ubuntu/Windows SUCCESS tại head `0b5e82f`; PR #38 merge `92cbfe0`, PR #42 merge `acf1ff9`. | [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36) · [PR #38](https://github.com/thientran2359/opendash-cms/pull/38) · [PR #42](https://github.com/thientran2359/opendash-cms/pull/42) · [macOS G5 evidence](../verification/2026-11-11-m3-s23-t01-media-library-macos.md) |
| [M3-P12](milestones/M3/TASK_PLAN.md) | Users: list/create/edit/role/password, bảo vệ tài khoản Admin cuối | **Users #48 task G5 VERIFIED trên Windows** — hậu kiểm integrated Admin/Auth và pre-0009 snapshot rollback; M3 release #53/#54 còn riêng | [Issue #48](https://github.com/thientran2359/opendash-cms/issues/48) · [Windows G5 evidence](../verification/2026-11-11-m3-s24-t01-users-windows.md) |
| [M3-S24-T01](milestones/M3/slices/2026-11-11-m3-users-settings-g3.md) | Forge · Users, Admin cuối và thu hồi phiên | **Done / task G5 VERIFIED** — integrated Windows 65/65 Rust, Auth/Users Chrome 390px, CAS, Admin cuối, session/race, actual pre-0009 binary rollback PASS; PR #56 Ubuntu/Windows governance CI SUCCESS; fixture harness đồng bộ migration 0010, không đụng runtime | [Issue #48](https://github.com/thientran2359/opendash-cms/issues/48) · [PR #56](https://github.com/thientran2359/opendash-cms/pull/56) · [Windows evidence](../verification/2026-11-11-m3-s24-t01-users-windows.md) |
| [M3-P13](milestones/M3/TASK_PLAN.md) | General Settings tối thiểu: site title/tagline/timezone, lưu DB + màn Admin | **SITE-01.1 task #49 G5 VERIFIED** — macOS real Admin/SQLite/browser và snapshot rollback PASS; không suy ra toàn bộ M3 release G5 | [Issue #49](https://github.com/thientran2359/opendash-cms/issues/49) · [PR #57](https://github.com/thientran2359/opendash-cms/pull/57) · [macOS G5 evidence](../verification/2026-11-11-m3-s25-t01-settings-macos.md) |
| [M3-S25-T01](milestones/M3/slices/2026-11-11-m3-users-settings-g3.md) | Harbor · General Settings và múi giờ | **Done / task G5 VERIFIED** — Issue #49 closed, Project #3 Done; 65/65 Rust tests, Chrome 390px, escaping/CAS, 0009→0010 upgrade + old-binary rollback PASS on macOS; PR #57 head `57481e7`, Ubuntu/Windows governance CI SUCCESS, code merge `08f8d21` | [Issue #49](https://github.com/thientran2359/opendash-cms/issues/49) · [PR #57](https://github.com/thientran2359/opendash-cms/pull/57) · [macOS G5 evidence](../verification/2026-11-11-m3-s25-t01-settings-macos.md) |
| [M3-P14](milestones/M3/TASK_PLAN.md) | Appearance → Themes: upload ZIP, danh sách và activate theme cơ bản | **SITE Themes #51 G5 VERIFIED (macOS task scope)** — 0011, A→B/A→C/C→A, rollback/Chrome 390px PASS; M3 full release G5 riêng | [Issue #51](https://github.com/thientran2359/opendash-cms/issues/51) · [PR #63](https://github.com/thientran2359/opendash-cms/pull/63) · [macOS evidence](../verification/2026-11-11-m3-s27-t01-themes-macos.md) |
| [M3-S27-T01](milestones/M3/slices/2026-11-11-m3-remaining-g3.md) | Harbor · Themes, kích hoạt và giữ homepage | **Done / task G5 VERIFIED** — Issue #51 closed completed, Project #3 Done; 67/67 Rust, ZIP/CAS/auth, Chrome, 0011 upgrade + old-binary rollback PASS on macOS. Ubuntu+Windows Governance CI PASS on head `be4e9e9`; code merge `c602635` | [Issue #51](https://github.com/thientran2359/opendash-cms/issues/51) · [PR #63](https://github.com/thientran2359/opendash-cms/pull/63) · [macOS evidence](../verification/2026-11-11-m3-s27-t01-themes-macos.md) |
| [M3-P15](milestones/M3/TASK_PLAN.md) | Plugins: màn Admin độc lập dùng ZIP install/list/enable/disable baseline | **Plugins #52 task G5 VERIFIED trên Windows** — runtime + Admin API/browser, chưa suy ra M3 release G5 | [Issue #52](https://github.com/thientran2359/opendash-cms/issues/52) · [Windows G5 evidence](../verification/2026-11-11-m3-s28-t01-plugins-windows.md) |
| [M3-S28-T01](milestones/M3/slices/2026-11-11-m3-remaining-g3.md) | Forge · Plugins và phục hồi khi worker lỗi | **Done / task G5 VERIFIED** — post-merge Windows 65/65 Rust, plugin API + Chrome 390px, auth/Admin/Dashboard/Content/Public/Settings regression PASS on `main@51aae56`; CI Ubuntu/Windows SUCCESS on PR head `ae5b86b`; M3-wide release G5 pending | [Issue #52](https://github.com/thientran2359/opendash-cms/issues/52) · [PR #59](https://github.com/thientran2359/opendash-cms/pull/59) · [Windows evidence](../verification/2026-11-11-m3-s28-t01-plugins-windows.md) |
| [M3-P16](milestones/M3/TASK_PLAN.md) | Public Post/Page cơ bản và preview/visibility | Backend/publication #24 **task G5 VERIFIED** (macOS + Windows); giao diện các candidate khác của M3 còn riêng | [Issue #24](https://github.com/thientran2359/opendash-cms/issues/24) · [PR #29](https://github.com/thientran2359/opendash-cms/pull/29) · [Windows evidence](../verification/2026-11-11-m3-s20-t01-post-page-publication-windows.md) · [G3 WP-E](milestones/M3/slices/2026-11-10-post-page-publication-g3.md) |
| [M3-P17](milestones/M3/TASK_PLAN.md) | E2E usable core trên macOS và hồi quy bảo mật dữ liệu | G3 đã merge, có Issue triển khai | [Issue #53](https://github.com/thientran2359/opendash-cms/issues/53) |
| [M3-S29-T01](milestones/M3/slices/2026-11-11-m3-remaining-g3.md) | Harbor · E2E M3 trên macOS | Backlog — chờ release candidate | [Issue #53](https://github.com/thientran2359/opendash-cms/issues/53) · [PR G3 #47](https://github.com/thientran2359/opendash-cms/pull/47) |
| [M3-P18](milestones/M3/TASK_PLAN.md) | Windows SQLite smoke harness deterministic cleanup (A0, lịch sử #15) | Bản sửa và evidence Windows đã merge. Chạy lại trong P19. | [PR #33](https://github.com/thientran2359/opendash-cms/pull/33) · [Issue #54](https://github.com/thientran2359/opendash-cms/issues/54) |
| [M3-P19](milestones/M3/TASK_PLAN.md) | Windows core CMS parity và G5 M3 closure; Linux early smoke riêng (A1) | G3 đã merge, có Issue triển khai | [Issue #54](https://github.com/thientran2359/opendash-cms/issues/54) |
| [M3-S30-T01](milestones/M3/slices/2026-11-11-m3-remaining-g3.md) | Forge · Windows release, P18 regression và điều kiện đóng M3 | Backlog — chờ release candidate và evidence macOS | [Issue #54](https://github.com/thientran2359/opendash-cms/issues/54) · [PR G3 #47](https://github.com/thientran2359/opendash-cms/pull/47) |

## [M4 — Visual Builder](https://github.com/thientran2359/opendash-cms/milestone/5) (10)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M4-P01](milestones/M4/TASK_PLAN.md) | Thiết kế AST node Section/Row/Column/Element và descriptors Heading/Text/Image/Button | Chưa G3 | — |
| [M4-P02](milestones/M4/TASK_PLAN.md) | Renderer và lưu/mở lại AST, fallback provider | Chưa G3 | — |
| [M4-P03](milestones/M4/TASK_PLAN.md) | UI tree/canvas/inspector chọn node từ cây hoặc preview | Chưa G3 | — |
| [M4-P04](milestones/M4/TASK_PLAN.md) | Bố cục responsive desktop/tablet/mobile | Chưa G3 | — |
| [M4-P05](milestones/M4/TASK_PLAN.md) | Bind core element với field/CPT, escaping theo ngữ cảnh | Chưa G3 | — |
| [M4-P06](milestones/M4/TASK_PLAN.md) | Query/list block: CPT/category/sort/limit/pagination | Chưa G3 | — |
| [M4-P07](milestones/M4/TASK_PLAN.md) | Preview chưa lưu, Save mới public, conflict/refresh | Chưa G3 | — |
| [M4-P08](milestones/M4/TASK_PLAN.md) | Regression Builder + taxonomy và handoff Theme M5 | Chưa G3 | — |
| [M4-P09](milestones/M4/TASK_PLAN.md) | Taxonomy B từ #15: category cha–con và tag CRUD Admin | Chưa G3 | — |
| [M4-P10](milestones/M4/TASK_PLAN.md) | Taxonomy entry links, filter/query cho CPT và Builder | Chưa G3 | — |

## [M5 — Theme & Templates](https://github.com/thientran2359/opendash-cms/milestone/6) (10)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M5-P01](milestones/M5/TASK_PLAN.md) | Mở rộng theme registry/activation M3: template ownership, update/version và package lifecycle | Chưa G3 | — |
| [M5-P02](milestones/M5/TASK_PLAN.md) | URL policy nâng cao + RequestContext cho CPT/post/page/taxonomy | Chưa G3 | — |
| [M5-P03](milestones/M5/TASK_PLAN.md) | Template composition page/header/footer/archive/single | Chưa G3 | — |
| [M5-P04](milestones/M5/TASK_PLAN.md) | Theme Options mở rộng Settings core M3 qua declarations | Chưa G3 | — |
| [M5-P05](milestones/M5/TASK_PLAN.md) | Import mẫu theo lựa chọn: page/menu/media/settings | Chưa G3 | — |
| [M5-P06](milestones/M5/TASK_PLAN.md) | Đổi/cập nhật theme bảo toàn layout và CPT/fields/taxonomy/entries | Chưa G3 | — |
| [M5-P07](milestones/M5/TASK_PLAN.md) | Missing-provider fallback và public accessibility/security | Chưa G3 | — |
| [M5-P08](milestones/M5/TASK_PLAN.md) | E2E public delivery, sample menu import và theme preservation | Chưa G3 | — |
| [M5-P09](milestones/M5/TASK_PLAN.md) | SITE-01 menu storage tối thiểu phục vụ import M5: menu/entry refs, ordering, origin và data ownership | Chưa G3 | — |
| [M5-P10](milestones/M5/TASK_PLAN.md) | Theme ZIP declarations CPT/fields/taxonomy qua canonical domain registry (không dùng plugin bridge) | Chưa G3 | — |

## [M6 — Plugin Extensions](https://github.com/thientran2359/opendash-cms/milestone/7) (8)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M6-P01](milestones/M6/TASK_PLAN.md) | Hoàn thiện event/actions ContentBefore/AfterSave, server APIs | Chưa G3 | — |
| [M6-P02](milestones/M6/TASK_PLAN.md) | Khai báo và thực thi external service calls theo quyền | Chưa G3 | — |
| [M6-P03](milestones/M6/TASK_PLAN.md) | Mở rộng declarative registration bridge cho blocks/templates/settings | Chưa G3 | — |
| [M6-P04](milestones/M6/TASK_PLAN.md) | Taxonomy declaration tích hợp với plugin lifecycle | Chưa G3 | — |
| [M6-P05](milestones/M6/TASK_PLAN.md) | Declarative Admin UI từ schema: form/table/list/tabs/button | Chưa G3 | — |
| [M6-P06](milestones/M6/TASK_PLAN.md) | Sandbox, permission và failure isolation regression | Chưa G3 | — |
| [M6-P07](milestones/M6/TASK_PLAN.md) | E2E Plugin ZIP xuyên Content/Theme/Builder | Chưa G3 | — |
| [M6-P08](milestones/M6/TASK_PLAN.md) | C từ #15: plugin CPT/field provider bridge vào canonical core schema | Chưa G3 | — |

## [M7 — Site Features](https://github.com/thientran2359/opendash-cms/milestone/8) (8)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M7-P01](milestones/M7/TASK_PLAN.md) | Mở rộng Site Settings UI/engine core M3, phân quyền | Chưa G3 | — |
| [M7-P02](milestones/M7/TASK_PLAN.md) | Menu/navigation CRUD và binding theme trên model M5, không tạo persistence riêng | Chưa G3 | — |
| [M7-P03](milestones/M7/TASK_PLAN.md) | SEO fields: URL/redirect/canonical/title/description/OG/robots | Chưa G3 | — |
| [M7-P04](milestones/M7/TASK_PLAN.md) | Sitemap tự động và invalidation | Chưa G3 | — |
| [M7-P05](milestones/M7/TASK_PLAN.md) | Builder contact form schema/validation/submission persistence | Chưa G3 | — |
| [M7-P06](milestones/M7/TASK_PLAN.md) | SMTP settings + thông báo/giám sát khi gửi lỗi | Chưa G3 | — |
| [M7-P07](milestones/M7/TASK_PLAN.md) | E2E Site Features + video / security | Chưa G3 | — |
| [M7-P08](milestones/M7/TASK_PLAN.md) | D từ #15 (video): media video V1 có metadata/serving/limits an toàn | Chưa G3 | — |

## [M8 — Operations & Release](https://github.com/thientran2359/opendash-cms/milestone/9) (8)

| Mã candidate / task | Công việc | Trạng thái | GitHub Issue / PR |
| --- | --- | --- | --- |
| [M8-P01](milestones/M8/TASK_PLAN.md) | Canonical backup manifest và snapshot consistency | Chưa G3 | — |
| [M8-P02](milestones/M8/TASK_PLAN.md) | Backup thủ công từ Admin và kiểm tra artifact | Chưa G3 | — |
| [M8-P03](milestones/M8/TASK_PLAN.md) | Restore nguyên tử, upgrade/migration và recovery | Chưa G3 | — |
| [M8-P04](milestones/M8/TASK_PLAN.md) | Linux VPS single-binary packaging/install/upgrade | Chưa G3 | — |
| [M8-P05](milestones/M8/TASK_PLAN.md) | Security/reliability/performance smoke trên Linux | Chưa G3 | — |
| [M8-P06](milestones/M8/TASK_PLAN.md) | Full V1 E2E và backup→restore→re-edit | Chưa G3 | — |
| [M8-P07](milestones/M8/TASK_PLAN.md) | Hướng dẫn vận hành và release acceptance | Chưa G3 | — |
| [M8-P08](milestones/M8/TASK_PLAN.md) | D từ #15 (lifecycle): media delete theo reference policy, reconcile/GC và backup boundary | Chưa G3 | — |

## Task thực tế không thuộc 9 milestone CMS

| GitHub task | Công việc | Trạng thái ghi nhận | GitHub Issue |
| --- | --- | --- | --- |
| P1-W01 | Khảo sát CoS Windows — Local Control API và Project/worktree | Issue **Open**; hồ sơ quy trình ghi **BLOCKED_SECURITY** (chưa xác minh Project Status trong bảng) | [#6](https://github.com/thientran2359/opendash-cms/issues/6) |
