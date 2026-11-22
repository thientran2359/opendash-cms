# OpenDash — Yêu cầu Sản phẩm V1

> Tài liệu này xác định phạm vi và hành vi sản phẩm cốt lõi của OpenDash V1. Mọi thay đổi phạm vi sản phẩm cần được chủ dự án phê duyệt thông qua quy trình [EGM](../../AGENTS.md).

## 1. Đối tượng và Ưu tiên

OpenDash là CMS cho blog và website giới thiệu, dành cho người dùng quen WordPress, với hệ theme và plugin riêng. Ưu tiên dễ sử dụng, dễ bảo trì, ra sản phẩm nhanh và cài đặt nhanh.

- **Giai đoạn phát hành:** Usable Beta nội bộ (ưu tiên macOS cho cold walkthrough) → common Release Candidate (RC) → V1 public (Windows, macOS, Linux VPS).
- **Cơ chế phân phối:** Rust + SQLite, đóng gói thành **một file thực thi duy nhất** (`opendash` / `opendash.exe`). Mỗi bản cài quản lý một website.
- **Vận hành:** Developer hoặc quản trị máy chủ triển khai trên VPS Linux (hoặc chạy cục bộ trên macOS/Windows); chủ website làm việc qua trình duyệt. Giao diện Admin ưu tiên tiếng Việt; mỗi website phục vụ một ngôn ngữ nội dung. Công khai mã nguồn (giấy phép và mô hình thu phí do chủ dự án chốt trước khi phát hành công khai).
- **Sao lưu & Phục hồi:** Admin cho sao lưu (mã hóa bằng passphrase) và phục hồi thủ công sang site root mới (Safe Mode trước khi kích hoạt tường minh), bao gồm DB, media, cấu hình, theme, plugin và SMTP secrets.

## 2. Nội dung, Lưu vs Xuất bản và Phân quyền

- **Loại nội dung & Trường tùy chỉnh:** Quản trị viên tạo loại nội dung tùy chỉnh (CPT), nhóm custom fields (9 kiểu: `short_text`, `long_text`, `rich_text`, `number`, `toggle`, `date`, `select`, `image`, `gallery`), custom taxonomy (có thể phân cấp cha–con hoặc phẳng) và gắn quyền truy cập theo 3 trục (`min_view_role`, `min_edit_role`, `public_exposure`).
- **Tài nguyên sẵn có:** Built-in `post`, `page` và private `pattern` CPT (Admin lưu một lần, tham chiếu đồng bộ qua block `core/pattern_ref`, không có URL công khai riêng); built-in `category` (phân cấp) và `tag` (phẳng).
- **Phân biệt Lưu và Xuất bản:**
  - Với `post`, `page` và các CPT cấu hình `publication: "draft_publish"`, **Lưu** bản `draft` chỉ ghi vào DB, không hiển thị trên URL công khai, archive, sitemap hoặc media public; chỉ thao tác **Xuất bản** mới công khai nội dung; **Lưu** sửa một bài đã `published` sẽ cập nhật nội dung công khai. Preview draft yêu cầu phiên đăng nhập/quyền hợp lệ.
  - Với **Visual Builder** (template overrides, Builder của entry CPT có bật hỗ trợ, Pattern Builder) và CPT cấu hình `immediate`, **Lưu** cập nhật trực tiếp ngay (không có workflow draft đa bước).
- **Hai vai trò:** `Admin` (quản lý hệ thống, Pattern, schema CPT/taxonomy, theme, plugin, users, backup/restore) và `Editor` (quản lý nội dung, media, Builder của CPT được bật; không được sửa Pattern hay cấu hình hệ thống).
- **Bảo toàn bản nháp:** Thay đổi chưa lưu trên form được giữ lại khi gặp lỗi `401` (quarantine trước khi đăng nhập lại và kiểm quyền hiệu lực) hoặc `412` (CAS conflict).

## 3. Điều hướng Admin & Màn hình Single Edit

- **Điều hướng 3 nhóm có nhãn** (tham chiếu chi tiết tại [Admin Navigation & Edit Screen](ADMIN_NAVIGATION_AND_EDIT_SCREEN.md)):
  - **Nội dung:** `Tổng quan` (Dashboard), `Bài viết` (danh sách, thêm mới, chuyên mục, thẻ), `Trang`, `Thư viện media` (ảnh & video độc lập), `Patterns` và các CPT đang hoạt động.
  - **Giao diện:** `Giao diện` (Themes, Menus), `Tùy chỉnh giao diện` (Theme Options, Visual Builder), `Loại nội dung & Trường` (tách biệt hoàn toàn khỏi màn nhập liệu entry), `Taxonomy tùy chỉnh`, `Thư viện mẫu`.
  - **Tùy chỉnh:** `Plugins`, `Người dùng / Hồ sơ của tôi`, `Công cụ` (Sao lưu & Phục hồi, Đối soát media, Tình trạng hệ thống), `Cài đặt` (Tổng quát, Đường dẫn tĩnh, SEO website, Liên hệ, Thông báo Email SMTP).
- **Màn hình Single Edit:** Chia vùng cố định (`header_bar` do Core sở hữu, `main_top`, `main_primary`, `main_bottom`, `sidebar`; tự động xếp thành 1 cột trên mobile `< 1024px`). Các field nhóm mở rộng và core tham gia chung một lần Lưu nguyên tử (`CompositeEntrySaveInput`).

## 4. Visual Builder và Theme

- **Visual Builder:** Hoạt động trên máy tính với cây 4 tầng `Section → Row → Column → Element` (`Heading`, `Text`, `Image`, `Button`, `ContentList`, `ContactForm`, `Video`), inspector thuộc tính và iframe preview theo 3 viewport (`desktop`, `tablet`, `mobile`).
- **Phạm vi chỉnh sửa:** Dựng và chỉnh được `header`, `footer`, `archive`, `single`, `taxonomy_archive`, từng entry của CPT có bật Builder; private `pattern` là AST tái sử dụng **đồng bộ** qua `core/pattern_ref` (tối đa 1 cấp, không lồng Pattern hoặc ContentList trong Pattern). Pattern Library của theme cung cấp thao tác sao chép AST độc lập trong slice kế tiếp.
- **Theme ZIP:** Hỗ trợ block, template (`C-SEC` safe subset), Theme Options, khai báo CPT/fields/taxonomy/`rewrite_base` và nhập dữ liệu mẫu chọn lọc (không ghi đè nội dung hiện có). Đổi hoặc cập nhật theme vẫn giữ nguyên CPT, nội dung và bố cục người dùng đã chỉnh.

## 5. Plugin

- **Plugin ZIP & Sandbox:** Cài plugin ZIP không cần biên dịch lại CMS; logic server chạy WebAssembly qua **Wasmi** trong worker cô lập tiến trình, giới hạn tài nguyên (fuel, RAM 16 MiB, deadline 5s) và HTTP egress chống SSRF.
- **Khả năng mở rộng:** Plugin khai báo CPT/fields/taxonomy/`rewrite_base`, hooks (`page/after_save`, `content/before_save`, `content/after_save`), standalone actions, Admin UI khai báo (form, bảng, list, tabs, button) và panel/column mở rộng.
- **Quyền & Phục hồi:** Quyền hợp lệ được áp dụng tự động theo khai báo; chặn thao tác ngoài quyền; tắt/lỗi plugin vẫn giữ nguyên CPT, schema, giá trị đã nhập và tham chiếu media để tiếp tục quản lý trong Admin.

## 6. Tính năng tích hợp sẵn

- **Media:** Upload và quản lý ảnh (JPEG/PNG/WebP) và video (MP4), kiểm soát quyền truy cập media theo trạng thái xuất bản/`public_exposure`, xóa an toàn theo tham chiếu (`media_refs`) và đối soát/quarantine.
- **Liên hệ & SEO:** Tạo contact form qua Admin/Builder, lưu submissions, gửi thông báo qua SMTP TLS/STARTTLS; cấu hình đường dẫn tĩnh (permalinks) với 301 redirect tự động, tiêu đề/mô tả SEO, OG image, canonical, `robots.txt` và `sitemap.xml`.

## 7. Ngoài phạm vi bản đầu (Sau V1)

Bán hàng (e-commerce), AI dựng trang từ ảnh, luồng duyệt/phê duyệt xuất bản nhiều bước (ngoài trạng thái `draft`/`published`), lịch sử và khôi phục phiên bản (revisions), repeater cho custom fields của CPT (riêng Trình dựng trang hỗ trợ trường lặp `repeater` tối đa 1 cấp có giới hạn kích thước trong `block.json`), quan hệ giữa nội dung (relation), khối đồng bộ tự động (synced propagation), lọc nâng cao cho khách truy cập, giao diện plugin JS/HTML tùy ý, tác vụ định kỳ (cron), sao lưu theo lịch, quản lý nội dung đa ngữ và chuyển ngôn ngữ Admin.
