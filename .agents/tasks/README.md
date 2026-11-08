# Hồ sơ task đang chuẩn bị

`.agents/tasks/` là nơi lưu đề xuất, handoff và báo cáo kỹ thuật **chưa trở
thành nguồn trạng thái độc lập**. Khi task đã `READY`, GitHub Issue giữ ID,
owner, assignee, dependency và trạng thái duy nhất; PR giữ diff và review.
Dùng [quy trình GitHub](../GITHUB_WORKFLOW.md) và
[mẫu Task](../templates/TASK.md).

Không tạo task thực thi khi G2/contract chưa được duyệt. Một agent branch chỉ
có một implementation task active; không cộng dồn nhiều task vào PR.
Commit/push sau test và cập nhật Issue/PR, không tự merge hoặc tuyên bố VERIFIED.
