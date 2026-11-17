# OpenDash — Yêu cầu sản phẩm bản đầu

## Delivery stages — Usable Beta (cập nhật 17/11/2026)

Theo [quyết định #292](https://github.com/thientran2359/opendash-cms/issues/292#issuecomment-6917897791)
và [ADR-0017](../decisions/ADR-0017-usable-beta-staging.md), ưu tiên **Usable Beta nội bộ
macOS → common RC → V1 public**. Giữ nguyên Rust/SQLite/single executable, Linux VPS,
Builder/plugin server logic và phạm vi V1. Nội dung đã xác nhận bên dưới vẫn áp dụng.

Theo chỉ đạo mới của chủ dự án: Aster tự review/chốt kỹ thuật, Harbor triển khai
các sửa lỗi Beta và technical retest; chỉ tham gia thảo luận liên-agent khi
chủ dự án tạo thảo luận. Xem cập nhật ownership tại ADR-0017.

Beta phải chứng minh site trống → bootstrap/login → Post/Page/CPT + field groups →
media/taxonomy/menu → theme ZIP/sample import không ghi đè → Builder homepage/header/
footer/single/archive → restart/provider changes giữ dữ liệu/layout → backup/restore
root mới → operator kích hoạt → re-edit. Desktop editing và responsive preview/public
không tự chứng nhận mobile authoring hoặc thiết bị thật. Semantics Lưu/Xuất bản giữ nguyên:
draft không public, preview chưa lưu không đổi public, Save Builder cập nhật public ngay.

Biên tập thường ngày qua Admin, không SQL/API thủ công/edit file. Operator được chuẩn bị
binary/môi trường và kích hoạt restored root theo ADR-0015; không hot-swap. Beta dùng
dữ liệu thử/site cô lập; public routes của instance nội bộ không là deploy Internet.
Pin source/binary/fixture hashes; demo Mộc Studio hoặc capability tests không thay
cold walkthrough. Ít nhất một người thử không viết code thực hiện kịch bản, ghi thời gian
và trợ giúp, tách khỏi agent/automated QA. Aster điều phối người thử, không mặc định owner.
Thiếu người thử không chặn audit/sửa lỗi nhưng chưa đạt human usability readiness.

Beta exit cần tất cả hành trình bắt buộc đạt, không blocker security/data/recovery hoặc
không hoàn thành công việc; lỗi cosmetic có Issue/owner/giới hạn, không dùng để waive
security/data. SEO/contact/video và các capability khác vẫn giữ acceptance V1.
Beta PASS không là M8/V1 VERIFIED: native Windows/Linux/macOS, common RC, deferred
milestones, notices/license và owner public-release authorization vẫn bắt buộc.
[G3 checkpoint](../plans/milestones/M7/slices/2026-11-16-usable-beta-g3.md) quy định
case matrix/ownership/sequence; yêu cầu này chưa là evidence nghiệm thu.

**Trạng thái:** Chủ dự án đã xác nhận phạm vi sau vòng rà soát yêu cầu. Tài liệu này là đầu vào cho đánh giá tech stack và có ưu tiên cao hơn các đề xuất thiết kế trước đó.

## Đối tượng và ưu tiên

OpenDash là CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress, với hệ theme và plugin riêng. Ưu tiên dễ sử dụng, dễ bảo trì, ra sản phẩm nhanh và cài đặt nhanh.

## Cài đặt, vận hành và phân phối

- Dùng Rust, SQLite và phân phối CMS bằng một file thực thi. Mỗi bản cài quản lý một website.
- Developer hoặc người quản trị máy chủ cài trên VPS Linux. Chủ website làm việc qua trình duyệt. macOS là môi trường phát triển và thử Usable Beta ưu tiên; Windows tiếp tục được hỗ trợ và kiểm chứng ở common RC/V1.
- Admin cho sao lưu và phục hồi thủ công, bao gồm nội dung, ảnh, cấu hình, theme và plugin.
- Mỗi website phục vụ một ngôn ngữ nội dung. Giao diện Admin ưu tiên tiếng Việt.
- Công khai mã nguồn. Giấy phép và mô hình thu phí còn mở.

## Nội dung, lưu và phân quyền

- Quản trị viên tạo loại nội dung tùy chỉnh (CPT), custom fields, danh mục có cha–con và thẻ qua giao diện. Theme/plugin có thể định nghĩa sẵn các thành phần này.
- Các field bản đầu gồm chữ ngắn/dài, trình soạn thảo nội dung, số, bật/tắt, ngày, lựa chọn, ảnh và thư viện ảnh.
- Người nhập liệu có thể làm việc qua form. Template lấy dữ liệu của nội dung tương ứng để hiển thị.
- Các thay đổi chưa lưu hiển thị trong màn hình chỉnh sửa hoặc preview. **Phân biệt Lưu và Xuất bản:** với `post`/`page` đang ở `draft`, **Lưu** chỉ ghi bản nháp vào DB, không làm nội dung xuất hiện trên URL công khai, archive, sitemap hoặc media public qua tham chiếu draft. Chỉ chuyển trạng thái sang `published` bằng thao tác **Xuất bản** mới công khai nội dung; **Lưu** sửa một `post`/`page` đã `published` cập nhật nội dung công khai. Preview draft phải yêu cầu phiên đăng nhập/quyền hợp lệ. Với **Builder/homepage** hiện hữu, **Lưu** tiếp tục cập nhật trang công khai ngay; không tự thêm workflow draft cho Builder hoặc CPT khác. Phần trạng thái/endpoint/schema cụ thể cần G2 trước triển khai.
- Hai vai trò là Quản trị viên và Biên tập viên. Quản trị viên quản lý hệ thống, cấu hình, theme và plugin. Biên tập viên quản lý và cập nhật nội dung công khai.

## Quyết định bổ sung M3 — Core CMS usable (10/11/2026)

Chủ dự án chốt **phương án A** cho milestone M3: sản phẩm phải sử dụng được trong Admin **gần WordPress**, giao diện xây bằng **Tailwind CSS**, có Dashboard; Posts/Pages với `post` và `page` **built-in**; Media Library ảnh + picker; Users Admin/Editor (list/create/edit/role/password), login/logout; General Settings tối thiểu (site title/tagline/timezone); Appearance→Themes (ZIP install/list/activate cơ bản); Plugins (ZIP install/list/enable/disable). Giữ hoạt động Builder/preview và bảo vệ session/RBAC/CSRF/media; không coi M3 hoàn tất khi chỉ có backend hoặc API.

**Ngoại lệ nhỏ so với giới hạn V1 cũ:** riêng `post`/`page` M3 có trạng thái **draft/published tối thiểu** theo quy tắc Lưu/Xuất bản bên trên (không có approval workflow, scheduled publishing, revision history). Quy trình biên tập đa bước vẫn để sau V1. Taxonomy, plugin CPT/field bridge, video/media delete/reconcile, Builder AST, template/Theme Options, SEO/contact và backup/restore **vẫn nằm trong V1**, nhưng không phải điều kiện đóng M3 phương án A; xem [roadmap](../plans/ROADMAP_V1.md) để biết mốc chính.

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

Bán hàng, AI dựng trang từ ảnh, luồng **duyệt/phê duyệt xuất bản nhiều bước** (ngoài trạng thái draft/published tối thiểu M3), lịch sử và khôi phục phiên bản, repeater, quan hệ giữa nội dung, khối đồng bộ tùy ý, lọc nâng cao cho khách truy cập, giao diện plugin tùy ý, tác vụ định kỳ, sao lưu theo lịch, quản lý nội dung đa ngữ và chuyển ngôn ngữ Admin.

## Bổ sung yêu cầu Admin — 15/11/2026

Chủ dự án xác nhận ưu tiên hoàn thiện **Admin và điều hướng gần với cách sử dụng WordPress**: menu/submenu theo công việc, màn hình chỉnh sửa một nội dung có các vùng bố cục cố định và cơ chế đăng ký panel/nhóm custom fields vào vị trí mong muốn. Chi tiết yêu cầu tại [Admin Navigation & Edit Screen](ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

- Mỗi màn hình có địa chỉ riêng; menu cha/submenu, breadcrumb, tiêu đề và hành động nhất quán, hiển thị theo quyền và bảo toàn thay đổi chưa lưu khi điều hướng.
- Màn hình sửa Post/Page/CPT có vùng nội dung chính và sidebar. Core quản lý thanh hành động Lưu/Xuất bản; các tính năng đăng ký panel vào vùng được hỗ trợ, với thứ tự và điều kiện hiển thị.
- Người quản trị chọn các panel áp dụng cho CPT, chọn vị trí cho nhóm custom fields và thứ tự field trong nhóm. Developer sử dụng cùng cơ chế khai báo mở rộng; plugin tuân theo giới hạn UI khai báo và quyền hiện có.
- Field thuộc nội dung tham gia chung vòng đời nhập liệu, validation, dirty state và nút Lưu của nội dung. Hành động độc lập phải có nút và phản hồi riêng.

Đây là yêu cầu sản phẩm đã xác nhận, **chưa phải contract API/schema, G3 hoặc bằng chứng đã triển khai**. Chưa phân bổ milestone/task; không tự thay phạm vi nghiệm thu hay trạng thái M8 đang thực hiện.

## Quyết định kỹ thuật còn mở

Đánh giá framework server, thư viện quản trị và builder, template engine, runtime plugin và cơ chế đóng gói/cài đặt theo yêu cầu trên. Xem [đánh giá tech stack](../architecture/TECH_STACK.md), [kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) và [đề xuất CMS-first](../architecture/CMS_PROPOSAL.md).

## Owner review follow-up — 17/11/2026

Chủ dự án chốt notes01–07: Admin grouped navigation; one screen per menu destination;
WordPress-like content list/filter/actions with controlled theme/plugin contributions;
one taxonomy panel and explicit draft/publish/update/view actions; separate category/tag
list/edit routes; exact Page Builder; visible image/video uploads; private Section CPT
and local template library. See [ADR-0018](../decisions/ADR-0018-owner-admin-review.md)
and [bounded contract](../contracts/ADMIN_REVIEW_2026_11.md). Page has no core-default
category/tag, while existing links and explicit provider/user eligibility are preserved.
Section insertion copies AST; synced propagation remains deferred. Cloud library and
note08 full Builder redesign stay deferred. No Beta PASS follows from this decision.
