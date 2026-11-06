# OpenDash — Yêu cầu sản phẩm bản đầu

**Trạng thái:** Chủ dự án đã xác nhận phạm vi sau vòng rà soát yêu cầu. Tài liệu này là đầu vào cho đánh giá tech stack và có ưu tiên cao hơn các đề xuất thiết kế trước đó.

## Đối tượng và ưu tiên

OpenDash là CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress, với hệ theme và plugin riêng. Ưu tiên dễ sử dụng, dễ bảo trì, ra sản phẩm nhanh và cài đặt nhanh.

## Cài đặt, vận hành và phân phối

- Dùng Rust, SQLite và phân phối CMS bằng một file thực thi. Mỗi bản cài quản lý một website.
- Developer hoặc người quản trị máy chủ cài trên VPS Linux. Chủ website làm việc qua trình duyệt. Windows phục vụ phát triển và kiểm thử.
- Admin cho sao lưu và phục hồi thủ công, bao gồm nội dung, ảnh, cấu hình, theme và plugin.
- Mỗi website phục vụ một ngôn ngữ nội dung. Giao diện Admin ưu tiên tiếng Việt.
- Công khai mã nguồn. Giấy phép và mô hình thu phí còn mở.

## Nội dung, lưu và phân quyền

- Quản trị viên tạo loại nội dung tùy chỉnh (CPT), custom fields, danh mục có cha–con và thẻ qua giao diện. Theme/plugin có thể định nghĩa sẵn các thành phần này.
- Các field bản đầu gồm chữ ngắn/dài, trình soạn thảo nội dung, số, bật/tắt, ngày, lựa chọn, ảnh và thư viện ảnh.
- Người nhập liệu có thể làm việc qua form. Template lấy dữ liệu của nội dung tương ứng để hiển thị.
- Các thay đổi chưa lưu hiển thị trong màn hình chỉnh sửa hoặc preview. Bấm **Lưu** cập nhật website công khai.
- Hai vai trò là Quản trị viên và Biên tập viên. Quản trị viên quản lý hệ thống, cấu hình, theme và plugin. Biên tập viên quản lý và cập nhật nội dung công khai.

## Builder và theme

- Builder dùng trên máy tính, có cây Section → Row → Column → Element và bảng thuộc tính trong sidebar, bên cạnh là preview. Người dùng chọn phần tử từ cây hoặc preview.
- Dựng và chỉnh được page, header, footer, archive và single. Hỗ trợ bố cục và preview cho desktop, tablet và mobile.
- Template dùng chung có thể liên kết thuộc tính của block với field của nội dung. Block danh sách cho chọn CPT, danh mục, thứ tự, số lượng và phân trang.
- Developer thêm block, template và Theme Options trong gói ZIP cài trực tiếp, không cần biên dịch lại CMS. CMS tạo màn hình setting từ khai báo và cung cấp giá trị cho block/template. Phân biệt setting chung của website và tùy chỉnh riêng của theme.
- Gói theme có thể chứa trang, menu, hình ảnh và cấu hình mẫu. Người dùng chủ động chọn nhập mẫu, quá trình nhập không ghi đè nội dung hiện có.
- Đổi hoặc cập nhật theme giữ nội dung và bố cục người dùng đã chỉnh. Người dùng chủ động chọn thay thế bằng thiết kế mặc định mới.

## Plugin

- Cài plugin ZIP không cần biên dịch lại CMS. Plugin có thể cung cấp CPT, fields, block, template và setting.
- Bản đầu hỗ trợ logic server qua sự kiện, thao tác và API, kể cả gọi dịch vụ ngoài trong phạm vi quyền. Plugin tạo màn hình Admin riêng bằng form, bảng, danh sách, tab và nút do CMS cung cấp.
- CMS hiển thị các quyền plugin yêu cầu và tự áp dụng quyền hợp lệ đã khai báo. Quản trị viên xem thông báo, không có bước duyệt quyền. CMS chặn thao tác ngoài quyền khai báo.
- CMS có thể dừng xử lý plugin lỗi hoặc chạy quá lâu, ghi lỗi và cho phép vô hiệu hóa plugin. Những phần độc lập của website và đường phục hồi trong Admin tiếp tục hoạt động.
- Đổi theme hoặc tắt plugin giữ các CPT, fields và nội dung đã nhập để tiếp tục quản lý. Xóa dữ liệu là thao tác riêng. CMS thông báo template hoặc block thiếu để người dùng xử lý.

## Tính năng có sẵn

- Tạo form liên hệ với các trường cơ bản và đặt form qua builder. Lưu thông tin gửi trong Admin, thông báo qua SMTP do quản trị viên cấu hình.
- SEO có URL tùy chỉnh, tiêu đề, mô tả, ảnh chia sẻ, canonical, tùy chọn lập chỉ mục và sitemap tự động.

## Để sau bản đầu

Bán hàng, AI dựng trang từ ảnh, luồng lưu nháp và xuất bản riêng, lịch sử và khôi phục phiên bản, repeater, quan hệ giữa nội dung, khối đồng bộ tùy ý, lọc nâng cao cho khách truy cập, giao diện plugin tùy ý, tác vụ định kỳ, sao lưu theo lịch, quản lý nội dung đa ngữ và chuyển ngôn ngữ Admin.

## Quyết định kỹ thuật còn mở

Đánh giá framework server, thư viện quản trị và builder, template engine, runtime plugin và cơ chế đóng gói/cài đặt theo yêu cầu trên. Xem [đánh giá tech stack](../architecture/TECH_STACK.md), [kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) và [đề xuất CMS-first](../architecture/CMS_PROPOSAL.md).
