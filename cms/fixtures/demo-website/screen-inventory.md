# Danh mục màn hình Demo Website — Mộc Studio

Tài liệu kiểm kê toàn bộ màn hình công khai, màn hình quản trị và trạng thái lỗi cần xác minh + chụp ảnh cho website mẫu **“Mộc Studio — Thiết kế không gian sống”**.

## A. Public Website (`1440x900`, `768x1024`, `390x844`)

| ID | Route / URL | Vai trò | Viewport | Kiểm chứng chính | Tên file PNG |
| --- | --- | --- | --- | --- | --- |
| `PUB-01` | `/` | Public | `1440x900` | Hero, dịch vụ nổi bật, dự án tiêu biểu, video MP4 (`Range 206`), bài viết mới, form liên hệ, menu header/footer | `pub-01-home-desktop.png` |
| `PUB-02` | `/` | Public | `768x1024` | Không tràn ngang (`scrollWidth <= innerWidth`), ảnh/video co giãn đúng | `pub-02-home-tablet.png` |
| `PUB-03` | `/` | Public | `390x844` | Layout 1 cột sạch, menu wrap hợp lệ, không tràn ngang | `pub-03-home-mobile.png` |
| `PUB-04` | `/dich_vu` | Public | `1440x900` | Danh sách >= 6 dịch vụ, ảnh bìa `naturalWidth > 0` | `pub-04-dich-vu-archive.png` |
| `PUB-05` | `/dich_vu/<slug>` | Public | `1440x900` | Chi tiết dịch vụ đủ các field CPT | `pub-05-dich-vu-single.png` |
| `PUB-06` | `/du_an` | Public | `1440x900` | Danh sách >= 8 dự án | `pub-06-du-an-archive.png` |
| `PUB-07` | `/du_an/<slug>` | Public | `1440x900` | Chi tiết dự án: đủ 9 nhóm field V1 (short/long/rich/number/bool/date/select/image/gallery) | `pub-07-du-an-single.png` |
| `PUB-08` | `/posts` | Public | `1440x900` | Trang 1 bài viết, bài nháp (`draft`) không lộ | `pub-08-post-archive-p1.png` |
| `PUB-09` | `/posts?page=2` | Public | `1440x900` | Trang 2 phân trang thật (từ >= 12 bài published) | `pub-09-post-archive-p2.png` |
| `PUB-10` | `/<slug>` | Public | `1440x900` | Chi tiết bài viết + SEO `<title>`/`<meta>` | `pub-10-post-single.png` |
| `PUB-11` | `/category/<slug>` | Public | `1440x900` | Archive danh mục phân cấp | `pub-11-taxonomy-category.png` |
| `PUB-12` | `/tag/<slug>` | Public | `1440x900` | Archive thẻ (`tag`) | `pub-12-taxonomy-tag.png` |
| `PUB-13` | `/gioi-thieu` | Public | `1440x900` | Trang Giới thiệu công khai | `pub-13-page-gioi-thieu.png` |
| `PUB-14` | `/lien-he` | Public | `1440x900` | Trang Liên hệ + form gửi thực tế | `pub-14-page-lien-he.png` |

## B. Admin & RBAC (`1440x900`)

| ID | Route / URL | Vai trò | Viewport | Kiểm chứng chính | Tên file PNG |
| --- | --- | --- | --- | --- | --- |
| `ADM-01` | `/auth/login` | Guest | `1440x900` | Form đăng nhập Admin sạch | `adm-01-login.png` |
| `ADM-02` | `/admin#/dashboard` | Admin | `1440x900` | Thống kê tổng quan + chẩn đoán | `adm-02-dashboard.png` |
| `ADM-03` | `/admin#/posts` | Admin | `1440x900` | Danh sách bài viết (published + draft) | `adm-03-posts.png` |
| `ADM-04` | `/admin#/pages` | Admin | `1440x900` | Danh sách trang (`gioi-thieu`, `lien-he`) | `adm-04-pages.png` |
| `ADM-05` | `/admin#/appearance/content-types` | Admin | `1440x900` | Cấu trúc CPT `dich_vu`, `du_an` và field groups | `adm-05-content-types.png` |
| `ADM-06` | `/admin#/media` | Admin | `1440x900` | >= 12 ảnh + 1 video MP4 | `adm-06-media.png` |
| `ADM-07` | `/admin#/posts/categories` | Admin | `1440x900` | >= 3 category phân cấp | `adm-07-taxonomy.png` |
| `ADM-08` | `/admin#/appearance/themes` | Admin | `1440x900` | Theme `moc-studio` đang active | `adm-08-themes.png` |
| `ADM-09` | `/admin#/studio` | Admin | `1440x900` | OpenDash Studio (Customizer + Template Builder) | `adm-09-studio.png` |
| `ADM-10` | `/admin#/plugins` | Admin | `1440x900` | `sample-plugin` đã cài, chạy action thành công | `adm-10-plugins.png` |
| `ADM-11` | `/admin#/users` | Admin | `1440x900` | Danh sách `admin` và `editor_moc` | `adm-11-users.png` |
| `ADM-12` | `/admin#/settings/general` | Admin | `1440x900` | Cài đặt tổng quát | `adm-12-settings.png` |
| `ADM-13` | `/admin#/posts` | Editor | `1440x900` | Editor vào được khu nội dung và bị chặn admin-only (`403`) | `adm-13-editor-rbac.png` |

## C. Trạng thái lỗi & Bảo mật (`1440x900`)

| ID | Route / URL | Vai trò | Viewport | Kiểm chứng chính | Tên file PNG |
| --- | --- | --- | --- | --- | --- |
| `ERR-01` | `/duong-dan-khong-ton-tai` | Public | `1440x900` | Trang 404 thân thiện, đúng HTTP 404 | `err-01-public-404.png` |
| `ERR-02` | `/<draft-slug>` | Public | `1440x900` | Bài nháp trả HTTP 404 với khách | `err-02-draft-hidden-404.png` |
