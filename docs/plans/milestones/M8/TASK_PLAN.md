# M8 — Task planning: Operations & Release V1

Các `M8-Pxx` là **candidate** có dependency, chưa thành Task G3/Project Ready. [Scope V1](../../../product/PRODUCT_REQUIREMENTS.md) · [Roadmap release DoD](../../ROADMAP_V1.md) · [Module coverage](../COVERAGE_MATRIX.md).

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance dự kiến |
| --- | --- | --- | --- | --- |
| M8-P01 | Canonical backup manifest và snapshot consistency | OPS-01 (FND-03, CNT-05, THM-01, PLG-01) | M3 DB/media safety, M5 theme, M6 plugin; Full-Track G2 snapshot/write lock | Lấy nhất quán DB, uploads, config, theme/plugin kể cả `OPENDASH_DB_PATH` ngoài site root; không mất/đứt references |
| M8-P02 | Backup thủ công từ Admin và kiểm tra artifact | OPS-01 (FND-02/05) | P01, file/stream limit và RBAC | Admin tạo/xuất backup, checksum/manifest/size đúng; không lộ paths/secrets; Editor không có quyền |
| M8-P03 | Restore nguyên tử, upgrade/migration và recovery | OPS-01 (FND-03/04) | P01/P02, forward migration, G2 recovery | Restore vào site root mới rồi khởi động, sửa page/CPT/media/theme/plugin được; failpoint/partial restore không hủy site đang chạy |
| M8-P04 | Linux VPS single-binary packaging/install/upgrade | OPS-02 (FND-04) | M1 Windows build baseline, CMS code M3–M7 ready, runner Linux | Release Linux có Admin assets, permissions/path, service/start/stop/restart, logs, upgrade 0001→migrations latest và health |
| M8-P05 | Security/reliability/performance smoke trên Linux | OPS-02 (FND-02/03, CNT-05, PLG-01) | P04 + M3..M7 release gates | Browser/HTTP, RBAC/CSRF/Origin, upload/ref ACL, ZIP traversal, plugin timeout/isolation, concurrency, crash/restart trên Linux thực |
| M8-P06 | Full V1 E2E và backup→restore→re-edit | OPS-01/02 (toàn module V1) | P01..P05, M3..M7 G5 đúng scope | Tạo CPT/taxonomy/media→dựng page/theme/plugin→SEO/form→backup→restore site riêng→edit/save/public OK, không đứt dữ liệu |
| M8-P07 | Hướng dẫn vận hành và release acceptance | OPS-02 (FND-04) | P01..P06, G5 logs, decision về license ngoài phạm vi agent | Tài liệu cài đặt Linux, backup/restore, upgrade, quyền, recovery và giới hạn có thể làm theo; CI/reviewer độc lập và SHA rõ |

**Điều kiện đóng V1:** toàn bộ capability bắt buộc có primary acceptance/evidence, không chỉ đếm pass; Linux thật `VERIFIED`, backup/restore chạy site mới và các quyền/sandbox an toàn. Nếu runner hoặc chứng cứ thiếu thì `NOT_RUN/PARTIAL`, không tuyên bố V1 DONE. Không đưa vào V1 scheduled backup, auto-publishing workflow hoặc đa ngữ.
