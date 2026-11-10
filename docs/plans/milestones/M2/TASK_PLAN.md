# M2 — Phân rã task: Plugin Sandbox Baseline

Từng hàng `M2-Pxx` chỉ là **candidate đối chiếu baseline**, không phải task G3 Ready hoặc một nguồn Project Status. [Evidence Windows M1/M2](../../../verification/2026-11-07-m1-m2-import.md) là điểm bắt đầu.

| Candidate | Slice / việc cần đạt | Module chính | Dependency và gate | Acceptance / bằng chứng |
| --- | --- | --- | --- | --- |
| M2-P01 | Đọc/kiểm tra ZIP plugin, manifest/ABI tối thiểu | PLG-01 | M1 SitePaths/Auth; ZIP path/size checks (Full-Track nếu đổi ABI) | ZIP hợp lệ cài được, invalid path/manifest không đọc/ghi ngoài site |
| M2-P02 | Wasmi worker riêng, bounded resources/timeout | PLG-01 | M2-P01; sandbox/ABI | Plugin độc hại, quá hạn hoặc crash bị cô lập; CMS/Admin có đường phục hồi |
| M2-P03 | Permission guard, log và disable/re-enable cơ bản | PLG-01, FND-02 | M2-P02; Auth/permission contract | Thao tác thiếu quyền bị chặn, log sự cố, website độc lập không bị treo |
| M2-P04 | Persistence plugin qua restart và Admin package actions | PLG-01, FND-03 | M2-P01..P03 | Danh sách package và trạng thái vẫn nhất quán sau restart; dữ liệu site không bị xóa tự động |
| M2-P05 | Windows plugin smoke + regression baseline | PLG-01 | M2-P01..P04; runner Windows | Thử plugin ZIP và lỗi quyền/timeout/recovery trên exact binary, SHA, OS |

**Không thuộc M2:** PLG-02 event/external API (M2.5/M6), PLG-03 declarative CPT/field registration (M3, mở rộng M6), PLG-04 Admin UI plugin (M6). Xác nhận trước khi giao mới để không làm lại baseline đã có.
