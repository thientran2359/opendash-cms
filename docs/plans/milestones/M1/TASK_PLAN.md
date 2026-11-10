# M1 — Phân rã task: CMS Baseline

Nguồn: [module mapping](../COVERAGE_MATRIX.md) · [Roadmap](../../ROADMAP_V1.md) · [verification M1/M2](../../../verification/2026-11-07-m1-m2-import.md). **Loại:** đối chiếu hồi cứu các công việc M1 theo scope; đã có evidence baseline Windows nhưng **chưa xác minh từng GitHub Issue/Project Done**. Không tạo lại task hoặc issue cho code đã có.

| Candidate (không phải G3 ID) | Slice / việc cần đạt | Module chính | Dependency và gate | Acceptance / bằng chứng cần đối chiếu |
| --- | --- | --- | --- | --- |
| M1-P01 | Bootstrap SQLite, migration, SitePaths mỗi site | FND-03 | Schema/site-root isolation; G2 storage nếu thay contract | Lần đầu/restart/upgrade không lẫn dữ liệu giữa hai site; DB và assets đúng root |
| M1-P02 | Tạo Admin, session và RBAC Admin/Editor, CSRF/Origin | FND-02 | M1-P01, Auth contract | Đăng nhập/401/403, Editor không thao tác Admin; form không tự mất khi hết session |
| M1-P03 | Admin shell và primitives tối thiểu để quản trị | FND-05 | M1-P02 | Navigation, API client, lỗi và UI chạy trên Chromium; không tuyên bố mọi màn V1 đã có |
| M1-P04 | Theme ZIP và Builder một block, Preview trước Save | THM-01, BLD-01 | M1-P01..P03; ZIP/CSP/sandbox contract | Preview không thay public trước Save; Save/restart giữ page, theme ZIP hợp lệ/sai được xử lý an toàn |
| M1-P05 | Windows executable độc lập, health và assets đi kèm | FND-04 | M1-P01..P04 | Release binary, Admin assets, health, site root override và 404/MIME smoke trên Windows |
| M1-P06 | Regression baseline M1 và kiểm chứng có thể tái hiện | FND-02/03/04, BLD-01 | M1-P01..P05; runner Windows | Test Auth/CSRF/Builder/Theme/SQLite và build exit PASS theo **SHA/OS trong báo cáo** |

**Đường đóng M1:** đối chiếu [bằng chứng M1/M2](../../../verification/2026-11-07-m1-m2-import.md), chỉ đóng baseline scope nếu được chấp thuận. Công việc Builder đa tầng thuộc M4, theme/template/import thuộc M5, Linux release thuộc M8. Mọi sửa code mới sau này cần task/gate hiện hành riêng, không tái cấp các candidate này.
