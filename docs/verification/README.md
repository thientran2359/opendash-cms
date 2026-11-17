# Báo cáo kiểm chứng

Lưu kiểm chứng có bằng chứng theo ngày hoặc milestone (`YYYY-MM-DD-<chu-de>.md`, `M1_*.md`, ...).

Mỗi báo cáo ghi rõ: **môi trường**, **đường dẫn mã**, **lệnh/check đã thực hiện**, **kết quả (PASS / FAIL / NOT RUN)**, **giới hạn**, **các lỗi còn mở**. Phân biệt:

- `AGREED`: chủ dự án đã chốt thiết kế/contract.
- `implemented`: source code có mặt và chứa thay đổi tương ứng.
- `verified`: có bằng chứng lệnh, test hoặc kiểm tra thực tế của phiên bản đó.

Không suy ra trạng thái `verified` từ tài liệu chuyển đến hoặc báo cáo của repository khác.

- [Kiểm kê workspace trước khi tạo scaffold (2026-11-06)](2026-11-06-repository-inventory.md).
- [Báo cáo build và smoke test scaffold (2026-11-06)](2026-11-06-scaffold-build.md).
- [Chuyển cấu trúc `cms/`, `database/` và kiểm chứng (2026-11-07)](2026-11-07-layout-migration.md).
- [Chuyển `docs/` ra thư mục gốc (2026-11-07)](2026-11-07-docs-root-move.md).
- [SitePaths và lần khởi động đầu của binary trên Windows (2026-11-07)](2026-11-07-site-paths.md).
- [Nhập M1/M2 và kiểm chứng tại đích trên Windows (2026-11-07)](2026-11-07-m1-m2-import.md).
- [Kiểm chứng nguồn M1/M2 trước khi nhập mã (2026-11-07)](2026-11-07-m1-m2-source-baseline.md).
- [Xây dựng Bộ khung Lõi (Core Kernel) & Quy chuẩn Code M2.5 (2026-11-07)](2026-11-07-m2-5-core-kernel.md).
- [M3.1 — Mô hình CPT và Custom Fields (2026-11-08)](2026-11-08-m3-1-content-domain.md).
- [M3.2 — SQLite và API tạo/liệt kê CPT (2026-11-08)](2026-11-08-m3-2-content-types-backend.md).
- [M3.3 — CRUD entry nội dung và kiểm chứng lưu bền vững (2026-11-08)](2026-11-08-m3-3-content-entry-crud.md).
- [M3.4 — Giao diện Admin quản lý CPT và Content Entry (2026-11-08)](2026-11-08-m3-4-admin-content-ui.md).
- [M3.5 — Media Library, thumbnail, picker và kiểm tra tham chiếu (2026-11-09)](2026-11-09-m3-5-media-library.md).
- [M3.6 — Public CPT Rendering, theme templates và public media ACL (2026-11-09)](2026-11-09-m3-6-public-content-rendering.md).
- [Đối chiếu tài liệu M3, workflow GitHub và CodeGraph (2026-11-09)](2026-11-09-agent-documentation-reconciliation.md) — chỉ xác minh tài liệu/công cụ, không phải kiểm thử lại CMS.
- [G5 GitHub Management (2026-11-09)](2026-11-09-github-governance-g5.md) — Issue Forms, Project, Ruleset, CI và giới hạn của evidence.
- [CodeGraph — thiết lập và truy vấn trên Windows (2026-11-07)](2026-11-07-codegraph-setup.md) — kiểm chứng công cụ/index và MCP, **không phải CMS runtime E2E hoặc kết nối CodeGraph cho mọi agent**.
- [Skill review/merge thủ công của Atlas/Nova (2026-11-10)](2026-11-10-review-merge-skill.md) — kiểm thử quy trình, artifact guard và checker theo báo cáo; **không phải CMS HTTP/browser/Windows–Linux E2E**, không suy đã merge từ trạng thái local ghi trong báo cáo.
- [Adapter Atlas → CoS Windows: W01 ACL và PONG thực tế (2026-11-12)](2026-11-12-cos-dispatch-w01-pong.md) — chứng cứ gửi chat trên Forge và giới hạn token rotation/production Issue pickup.
- [Kiểm chứng M5 G5 trên Windows (2026-11-13)](2026-11-13-m5-g5-windows.md) — build, full workspace tests và cross-module E2E `M5-S08-T01` trên Windows.
- [Kiểm chứng M5 #157 native macOS Intel (thực chạy 2026-11-17)](2026-11-13-m5-g5-macos.md) — M4→M5→M4, Chrome và preservation PASS trong phạm vi ghi nhận; full cross-OS G5 chưa đóng.
- [Windows source-matched M5 #157 / M7 #214 (2026-11-17)](2026-11-17-m5-m7-windows-source-rebaseline.md) — binary/rollback/tests trên `c59d2f6` (M5) và `69d311b` (M7); browser assertion receipts, runner teardown và giới hạn full G5 được phân biệt rõ.
- [G5 native macOS Intel — M8-S10-T01 / #318 (2026-11-17)](2026-11-17-m8-s10-t01-g5-macos.md) — release suite, Safe Mode khi restore/restart, actual old/new/old và negative ACL/CSRF; Windows F08 vẫn là follow-up trước common RC.
- [G5 native macOS Intel — M7-S15-T01 / #319 (2026-11-17)](2026-11-17-m7-s15-t01-g5-macos.md) — real Chrome picker Page/Post/CPT, bounded pagination, 401/403/412, stable IDs, rollback old/new/old và M7 browser regression; Windows F01 chưa revalidate.
- [G5 native macOS Intel — M5-S11-T01 / #320 (2026-11-17)](2026-11-17-m5-s11-t01-g5-macos.md) — real Chrome v2 revision discovery/activation, legacy workflow, sample import, 401/403/412/409, release Rust 5/5, old/new/old data continuity; Windows F02 chưa revalidate.
- [G5 native macOS Intel — M7-S15-T02 / #321 (2026-11-17)](2026-11-17-m7-s15-t02-g5-macos.md) — Chrome draft navigation/Save/412/reload/delayed, store ACL test, Rust release 3/3 và old/new/old; tổ hợp browser 401/downgrade/reauth và Windows F03 vẫn NOT_RUN.

- [M7 planning G1/G2/G3 — 14/11/2026](2026-11-14-m7-planning.md): docs/source validation only, full14-task coverage; M7 runtime/native/browser NOT_RUN.

- [M8 G1/G2/G3 planning — 14/11/2026](2026-11-14-m8-planning.md): all18 tasks/8 candidates specified, source/docs checks only; M8 native/runtime NOT_RUN.
