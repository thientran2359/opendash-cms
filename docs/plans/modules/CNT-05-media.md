# CNT-05 — Media Library · Living Module Brief

**Scope:** PRD V1 `AGREED` / ranh giới module đã thống nhất · **Milestone dự kiến:** M3 / `M3-S04` · **PRD:** [Content/media](../../product/PRODUCT_REQUIREMENTS.md) · **Track:** `Full-Track` vì upload, quyền, filesystem/SQLite và public serving.

**Trạng thái hiện tại:** Research `NOT_STARTED` · Design `NOT_STARTED` · Implementation `NOT_STARTED` · Verification `NOT_RUN`; các capability và task dưới đây mới là dự kiến.

Living Module Brief G0–G3 giữ một nguồn chi tiết từ scope đến task. V1 cần ảnh/video upload và image/gallery picker. **Opaque `media_id` thay URL/path trong field là ứng viên G1**, chưa chốt kiểu ID, schema metadata, public route hay cơ chế dọn file. Xem [planning consensus](../../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md).

## G0 — Năng lực cần có

| ID | Capability | Primary acceptance slice dự kiến | Nghiệm thu dự kiến |
| --- | --- | --- | --- |
| `CNT-05.1` | Upload và quản lý media bằng Admin | `M3-S04` | File hợp lệ được lưu, metadata còn sau restart, quyền upload được kiểm tra |
| `CNT-05.2` | Chọn media cho image/gallery fields và Builder | `M3-S04` (picker/field); Builder reuse ở M4 | Field dùng reference ổn định; chọn một/nhiều ảnh, hiển thị đúng |
| `CNT-05.3` | Phục vụ media public có kiểm soát | `M3-S04` | Không đọc tệp ngoài site root, URL/media type đúng, xử lý file thiếu |
| `CNT-05.4` | Tích hợp media với backup/restore website | `M8` (OPS-01 integration) | Metadata/files vẫn nhất quán sau restore, không mất content refs |

## G0 — Dependency và phạm vi

- **Cần trước:** FND-02 (Auth/RBAC), FND-03 (site paths/SQLite), G2 reference contract giữa CNT-02 và CNT-05. M3-S02 xác định schema image/gallery trước khi nối picker.
- **Cung cấp cho:** CNT-02 image/gallery fields, SITE-02 OG image, THM-03 sample import và OPS-01 backup.
- **Ngoài phạm vi:** chỉnh sửa media nâng cao, xử lý video background, CDN/transform service, media search/filter nâng cao. Không tự chọn thư viện xử lý ảnh hoặc mở rộng V1 nếu chưa có nhu cầu rõ.

## G1 — Research phải trả lời (chưa bắt đầu)

1. **Mô hình:** file nằm dưới `uploads/` như thiết kế site root; so sánh opaque `media_id` với URL/path, lựa chọn định danh (UUID/ULID/key) theo convention thật và khôi phục sau đổi domain/path.
2. **Upload security:** các định dạng và kích thước cho phép; MIME/magic bytes, safe filename, path traversal, symlink, collision, giới hạn dung lượng, quyền và CSRF.
3. **Consistency:** ghi SQLite và filesystem không có transaction chung. Cách staging, finalize/cleanup và phục hồi từ lỗi giữa chừng là gì?
4. **Reference:** media bị xóa/thay, bị thiếu file, trùng nội dung, hoặc được dùng ở nhiều field thì API phản hồi sao?
5. **Public delivery:** URL generation, Content-Type, cache headers, access và HTTP range cho video có cần ngay ở V1?
6. **Backup/restore:** snapshot file/media metadata đồng bộ thế nào, có bảo toàn binary và link khi đổi đường dẫn install?
7. **Integration:** CNT-02 định nghĩa image/gallery schema và Admin picker sử dụng Media API nào; UX cho nhiều ảnh và video ra sao?

**Đầu ra G1:** sơ đồ file + metadata + API, tradeoff reference, threat model upload, test lỗi cần mô phỏng, dependency với CNT-02/OPS-01. Ghi tóm tắt tại đây; chỉ tách `docs/research/` khi có đánh giá lớn.

## G2 — Điểm chốt thiết kế (hiện `NOT_STARTED`)

- Reference contract, ID format và migration; field `image`/`gallery` lưu ID hay cấu trúc khác; tách file bytes khỏi metadata.
- Upload limits, format support, validation, chính sách xóa và rollback.
- URL public và quyền thao tác Admin.
- Ownership của metadata và cách backup/restore nhất quán.
- File staging/finalize, cleanup/reconciliation khi SQLite transaction và filesystem không atomic; hành vi khi upload dở dang, file đã xóa hoặc orphan.
- An toàn migration: test trên DB copy, snapshot trước nâng cấp, forward-fix sau khi migration được phát hành; Git revert không rollback dữ liệu đã nâng cấp.

## G3 — Breakdown task dự kiến (chưa sẵn sàng giao code)

| Task ID giữ từ bản nháp | Đầu ra khi qua G3 | Phụ thuộc và điều kiện |
| --- | --- | --- |
| `M3-S04-T03` | Upload/serve endpoints, storage metadata/file lifecycle, auth/limits | Full-Track G2 đã duyệt, FND-02/FND-03 |
| `M3-S04-T04` | Media library UI + picker image/gallery | T03 và schema field CNT-02 từ M3-S02 |
| `M3-S04-T05` | Test API/file consistency, restart, missing file, media reference, recovery | T03/T04; cô lập site root và DB |

**Ánh xạ lịch sử ID:** `M3-S04-T01` (Research) và `M3-S04-T02` (Design G2) là nhãn task trong discovery draft cũ; nay được chuyển thành các mục G1/G2, **không tái cấp phát hai ID đó**. Các `T03..T05` mới chỉ là candidates, chưa `READY`.

Sau khi G2 được chốt, dùng [mẫu handoff](../templates/SLICE_TASK.md) trong `.agents/sync/task-for-ag.md`, không tạo task files riêng. Không triển khai Media M3 theo bản nháp này.
