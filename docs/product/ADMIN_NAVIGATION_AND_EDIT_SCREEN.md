# Admin Navigation và bố cục màn hình chỉnh sửa

**Ngày:** 15/11/2026 · **Owner điều phối:** Atlas.
**Trạng thái:** Yêu cầu sản phẩm được chủ dự án xác nhận trong chat; thiết kế kỹ thuật/API/schema chưa chốt; triển khai và kiểm chứng phần bổ sung này chưa thực hiện.

Nguồn phạm vi: [Product Requirements](PRODUCT_REQUIREMENTS.md). Áp dụng quy trình [Planning Workflow](../plans/PLANNING_WORKFLOW.md), quy chuẩn [Core Foundation](../contracts/CORE_FOUNDATION.md) và [Plugin ABI](../contracts/PLUGIN_ABI.md) khi thiết kế tiếp.

## 1. Mục tiêu

Người quen WordPress có thể tìm đúng công cụ trong Admin, quản lý nội dung và chỉnh từng trang theo bố cục nhất quán. Các tính năng phát triển sau đăng ký thành phần vào vùng có sẵn; không phải thêm các panel nối tiếp tùy ý vào một màn hình lớn.

Ưu tiên đợt này là cấu trúc Admin, menu nhiều cấp và màn hình single edit. Không tự mở rộng sang Gutenberg, editor mới, revisions, autosave, lịch đăng hoặc workflow duyệt bài.

## 2. Menu chính và submenu

| Menu chính | Submenu mục tiêu |
| --- | --- |
| Tổng quan | Dashboard |
| Bài viết | Tất cả bài viết · Thêm mới · Chuyên mục · Thẻ |
| Trang | Tất cả trang · Thêm mới |
| Thư viện | Tất cả media · Tải lên |
| Giao diện | Themes · Tùy chỉnh theme · Menus · Templates · Builder |
| Liên hệ | Form liên hệ · Tin nhắn nhận được · Cấu hình email |
| Plugins | Plugins đã cài · Cài từ ZIP |
| Người dùng | Tất cả người dùng · Thêm mới · Hồ sơ của tôi |
| Công cụ | Sao lưu · Phục hồi · Đối soát media · Tình trạng hệ thống |
| Cài đặt | Tổng quát · Đường dẫn · SEO website |

Đây là sơ đồ sản phẩm mục tiêu, không khẳng định mọi submenu đã có chức năng. Trước G3 phải lập bảng ánh xạ submenu → màn hình/API hiện có → khoảng thiếu; mục mới như hồ sơ cá nhân cần phạm vi và acceptance riêng nếu chưa có.

- CPT như Dự án/Dịch vụ có menu riêng: danh sách, thêm mới và taxonomy gắn với CPT đó. Taxonomy dùng chung giữ cùng dữ liệu, không tạo bản sao vì xuất hiện ở nhiều menu.
- Phần khai báo CPT/schema/fields thuộc khu vực cấu hình dành cho người có quyền; tách khỏi nhập nội dung hằng ngày. Vị trí chính xác trong cây cấu hình được chốt khi thiết kế màn hình.
- Mỗi màn hình có URL riêng, mở trực tiếp hoặc reload vẫn đến đúng màn hình; back/forward hoạt động. Không chốt cơ chế hash/path hoặc router library ở yêu cầu này.
- Menu cha tự mở theo màn hình hiện tại; submenu được đánh dấu rõ. Có breadcrumb, tiêu đề và hành động chính nhất quán.
- Menu theo quyền Admin/Editor; ẩn menu không thay thế kiểm quyền server. URL trực tiếp vẫn phải được bảo vệ.
- Sidebar thu gọn trên desktop, dạng ngăn kéo trên mobile; hỗ trợ bàn phím và trạng thái mở/đóng có nhãn truy cập.
- Giữ bản nháp khi chuyển màn hình, cảnh báo thay đổi chưa lưu đúng mục; logout/đổi tài khoản không làm lộ bản nháp người trước.

## 3. Các vùng của màn hình single edit

Áp dụng cho Post, Page và CPT. Bố cục desktop mục tiêu:

```text
Thanh màn hình: breadcrumb / tiêu đề / Xem trước / Lưu hoặc Xuất bản
┌──────────────────────────────────┬─────────────────────────┐
│ Tiêu đề nội dung, đường dẫn       │ Xuất bản / trạng thái   │
├──────────────────────────────────┼─────────────────────────┤
│ Panel phía trên nội dung         │ Thuộc tính / template   │
│ Nội dung chính / mở Builder      │ Ảnh đại diện / taxonomy │
│ Panel phía dưới nội dung         │ Nhóm field phụ          │
│ Nhóm custom fields / SEO         │ Panel mở rộng           │
└──────────────────────────────────┴─────────────────────────┘
```

- Core sở hữu thanh hành động và quy tắc Lưu/Xuất bản. Không cho panel tùy ý ghi đè hành động core.
- Các vùng đăng ký: phía trên nội dung chính, phía dưới nội dung chính và sidebar. Tên ID kỹ thuật chưa chốt.
- Mobile chuyển sang một cột theo thứ tự đọc được xác định trước; không mất field, lỗi validation hay hành động lưu.
- Phân biệt chỉnh dữ liệu nội dung, chỉnh layout qua Builder và chỉnh template dùng chung; mở Builder phải xác định phạm vi ảnh hưởng và bảo toàn dữ liệu chưa lưu.

## 4. Panel registry và nhóm custom fields

Developer đăng ký panel theo cùng cơ chế mà các tính năng core sử dụng. Yêu cầu khai báo gồm định danh ổn định, chủ sở hữu, tiêu đề, vùng hiển thị, thứ tự, loại nội dung áp dụng, quyền xem/sửa và khả năng thu gọn. Kiểu dữ liệu và API đăng ký sẽ được chốt ở G2.

- Người quản trị chọn các panel có sẵn áp dụng cho CPT: nội dung, ảnh đại diện, taxonomy, thuộc tính, SEO và panel mở rộng tương thích. Panel bắt buộc của core không bị tắt tùy ý.
- Người quản trị đặt **nhóm field** vào vùng mong muốn và sắp xếp field bên trong nhóm. Ví dụ “Thông tin dự án” dưới nội dung, “Trạng thái dự án” ở sidebar.
- Đổi vị trí chỉ thay cách trình bày, không đổi field ID, quyền sở hữu schema, giá trị đã lưu hoặc binding của template.
- Cấu hình vị trí của CPT/nhóm field là cấu hình dùng chung. Tùy chọn bố cục riêng từng người dùng và kéo thả panel trực tiếp chưa nằm trong yêu cầu đã chốt.
- Plugin khai báo panel bằng các thành phần UI được contract hỗ trợ; không mặc định cho phép JavaScript/React/HTML tùy ý. Quyền phải được kiểm cả client và server.
- Registry phải xử lý trùng ID, thứ tự bằng nhau, provider bị tắt/gỡ và vị trí không hợp lệ một cách xác định. Tắt theme/plugin giữ dữ liệu; không render thành phần không còn được hỗ trợ. Không được xóa giá trị vì panel đang ẩn.

## 5. Lưu, validation và hành động riêng

- Các field thuộc cùng nội dung dùng chung draft state, validation, thông báo thay đổi chưa lưu và nút Lưu. Không tạo nhiều form lưu riêng gây mất nhất quán cho cùng record.
- Field lỗi trong panel thu gọn phải được chỉ rõ; đưa người dùng đến field cần sửa. Giữ dữ liệu nhập khi gặp lỗi xác thực, quyền hoặc xung đột phiên bản.
- Giữ semantics hiện có: Lưu draft Post/Page không công khai; Xuất bản mới công khai; Lưu nội dung đã published cập nhật public. Không tự thêm workflow draft cho CPT/Builder.
- Panel có hành động độc lập, như gửi email thử, dùng nút/phản hồi riêng và khai báo quyền/phạm vi tác động; không tự chạy khi bấm Lưu nội dung.
- Thiết kế kỹ thuật phải xác định ranh giới lưu nguyên tử và xử lý xung đột cho dữ liệu trong panel; chưa coi plugin được quyền tham gia transaction tùy ý.

## 6. Đầu ra cần chốt trước triển khai

1. Inventory màn hình/route hiện tại và sơ đồ điều hướng có ánh xạ quyền; phân biệt tái tổ chức UI với chức năng mới.
2. Mockup desktop cho danh sách bài viết, single edit Page/CPT và Settings; trạng thái mobile, lỗi, dirty và quyền hạn.
3. Contract registry: identity/owner, vị trí/thứ tự, field groups, lưu cấu hình, validation, provider lifecycle và tương thích plugin/theme hiện tại.
4. Kế hoạch migration nếu cần, task G3 với ownership/dependency, acceptance và mức tái sử dụng Base APIs. Chưa cấp mã milestone/task hoặc Project Ready từ tài liệu này.

## 7. Tiêu chí đánh giá sản phẩm

- Người dùng tìm được màn hình từ menu/submenu; URL trực tiếp, reload, back/forward và active navigation nhất quán.
- Admin/Editor thấy đúng các công cụ được phép và không vượt quyền qua URL/API.
- Một Page và một CPT có nhóm field đặt ở vùng chính/sidebar, thứ tự ổn định sau reload; sửa vị trí không mất dữ liệu.
- Panel core và panel mở rộng tuân theo cùng quy tắc bố cục; lỗi/trùng ID/provider mất được xử lý rõ ràng.
- Một lần Lưu xử lý các field của nội dung; navigation, 401 và conflict không làm mất draft. Hành động độc lập không chạy nhầm.
- Dùng [site demo #258](https://github.com/thientran2359/opendash-cms/issues/258) để bổ sung case và screenshots theo source SHA; ảnh chụp thành công không tự là visual approval.

Yêu cầu này không đóng hoặc miễn nghiệm thu M8, không thay ownership task Nova đang thực hiện. Các lỗi đã xác nhận tại [#262](https://github.com/thientran2359/opendash-cms/issues/262) vẫn được theo dõi riêng.
