# M3-S21-T01 — AdminShell Tailwind, kiểm chứng Windows

- **Issue:** [#25](https://github.com/thientran2359/opendash-cms/issues/25) · module FND-05 / FND-05.1.
- **Trạng thái:** Frontend `IMPLEMENTED`, Windows acceptance `VERIFIED` theo các kiểm tra bên dưới. **Không** xác nhận toàn bộ milestone M3 đã đạt G5.
- **Agent:** Forge (CoS Windows), branch `task/forge-m3-s21-t01`.
- **G3 đã chốt:** `d6aac6dcd69b84dc73f9d3a72cb3b63c2137ff29`; base thực tế từ `origin/main@922b598c895756738ef143ecbf5eadce3fd43d3b`.
- **Commit mã nguồn đã kiểm chứng:** `b2679086c7741ec8e0cdc685b8b670ad5e9f6c58` (trước đó kiểm chứng `2a1e32f`; task đồng bộ backend mới từ `origin/main@1e0387400c7b0cb87ff23651133f2fe5a12e89d6` qua merge `a43e3578602029fd97cab02efdf7a8915806438d`).
- **Môi trường:** Microsoft Windows 11 Pro, Node.js 22.16.0, npm 10.9.2, Rust/Cargo 1.99.1, Python 3.13.4, Chromium dùng `C:/Program Files/Google/Chrome/Application/chrome.exe`.
- **Địa chỉ kiểm thử:** OpenDash Rust HTTP trên `127.0.0.1` và cổng ngẫu nhiên qua `windows_smoke.running`; mỗi browser smoke sử dụng SQLite/site tạm (`TemporaryDirectory`). Không dùng database hay credentials thật.

## Thay đổi

- AdminShell có sidebar/topbar và nhóm điều hướng Dashboard, Posts, Pages, Content, Media, Themes, Builder, Plugins, Users, Settings; các mục Admin-only bị ẩn với Editor và hash URL đặc quyền bị đưa về Dashboard.
- React/Vite dùng Tailwind 4.1.17 với phiên bản pin trong lockfile; CSS hiện hữu của Builder/Content tiếp tục hoạt động. Vite được cập nhật có kiểm soát từ 6.0.7 lên 6.4.6 để giải quyết cảnh báo kiểm toán dependency tại thời điểm triển khai.
- Trang chưa có chức năng backend (Posts/Pages, Media standalone, Themes, Settings) ghi rõ trạng thái. Users/Plugins/SystemStatus chuyển sang khu vực có quyền phù hợp; Builder vẫn cho phép tải theme ZIP, preview và Save như trước.
- Navigation dùng hash client, giữ đường dẫn cũ `/builder` khởi động thẳng Builder, và **giữ mounted** cả Builder lẫn Content khi đổi mục. Theo dõi form chưa lưu, cảnh báo trước khi rời trang/đăng xuất; chỉ hủy bản nháp Content khi người dùng đồng ý logout. Session 401 giữ giá trị chưa lưu của Builder để khôi phục sau relogin.
- Sidebar responsive, menu mobile khóa cuộn nền và hỗ trợ Escape; focus hiển thị rõ. Không bổ sung endpoint Rust/API/SQL hay thay đổi dữ liệu người dùng.

## Lệnh và kết quả đã chạy (Windows)

| Kiểm tra | Kết quả thực tế |
| --- | --- |
| `cd cms/admin; npm ci` | **PASS**, lockfile cài 84 packages (sau bản vá Vite). |
| `cd cms/admin; npm audit --audit-level=moderate` | **PASS**, 0 vulnerabilities. |
| `cd cms/admin; npm run typecheck` | **PASS**, TypeScript không có lỗi. |
| `cd cms/admin; npm run build` | **PASS**, Vite 6.4.6: 46 modules, CSS/JS vào `dist/assets/`. |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS**, Rust binary debug thực tế nhúng Admin mới. |
| `python cms/scripts/windows_smoke.py --binary cms/target/debug/opendash.exe` | **PASS**, Builder/homepage regression trên site tạm. |
| `python cms/scripts/windows_auth_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite` | **PASS**, auth/RBAC/CSRF/Origin/expiry, concurrent setup, fixture migration. |
| `python cms/scripts/check-plans.py` | **PASS**, links/IDs/evidence consistency. |
| `python cms/scripts/check-plans.py --self-test` | **PASS**, 12 fixtures. |
| `python .agents/scripts/check-agent-artifacts.py` | **PASS**. |
| `git diff --check` | **PASS** sau khi bỏ dòng trống cuối file `AdminShell.tsx`. |

**Browser QA thực tế:** Python Playwright với Chrome desktop trên localhost, chạy cùng backend Rust nhúng `dist` và SQLite tạm; các scripts được chạy từ stdin trong terminal (không thêm script repo). Đã thấy:

1. Thiết lập Admin/đăng nhập, Enter để submit; Dashboard hiển thị. Menu có đủ vùng tương ứng role.
2. Builder nạp theme ZIP mẫu, nhập dữ liệu chưa lưu và preview trên iframe; URL công khai không thay đổi trước Save; chuyển Builder → Content → Builder giữ nguyên bản nháp và preview; Save cập nhật homepage công khai.
3. Xóa hiệu lực session trong **SQLite test tạm**, thực hiện Save gây HTTP 401, đăng nhập lại cùng tài khoản và tiếp tục bản nháp Builder rồi Save thành công. Kiểm thử bổ sung trên `b267908`: đăng nhập lại bằng **tài khoản khác** sau 401 xóa bản nháp Builder/preview của tài khoản cũ, tránh lộ dữ liệu nội bộ.
4. Tạo CPT và bản nháp entry, chuyển Content → Pages/Dashboard → Content không mất dữ liệu. Giao diện Pages cho biết đang phát triển, không tạo dữ liệu giả.
5. Đăng xuất khi Content dirty: chọn hủy xác nhận thì vẫn đăng nhập, xác nhận tiếp tục thì mới đăng xuất.
6. Admin tạo tài khoản Editor trên site thử; Editor không nhìn thấy nav Themes/Plugins/Users/Settings; nhập `/admin/#/plugins` thủ công trả về `#/dashboard`. API RBAC vẫn do Rust quyết định.
7. Viewport 390px: menu mở, Escape đóng, scroll lock được khôi phục, focus button có thể dùng bàn phím. Desktop 1440px và mobile 390px đều có `document.documentElement.scrollWidth === innerWidth`. Không có JavaScript `pageerror` được ghi nhận.

**Regression sau khi Harbor merge Posts/Pages backend:** `cargo build --manifest-path cms/Cargo.toml --locked`, `python cms/scripts/windows_smoke.py --binary cms/target/debug/opendash.exe`, `python cms/scripts/windows_auth_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite`, `npm run typecheck`, `npm run build`, `npm audit --audit-level=moderate`, plan checkers, agent artifacts checker và `git diff --check` đều **PASS**. Browser Playwright với backend mới (SQLite test tạm) **PASS**: login, Builder URL cũ, preview/Save, 401 giữ draft cùng tài khoản, Content CPT/entry nháp, placeholder, Editor không truy cập route đặc quyền, mobile drawer đóng/mở và Escape; không có JS pageerror.

## Ảnh kiểm chứng trên Windows

- [Admin Dashboard, desktop 1440px](m3-s21-issue25/admin-dashboard.png).
- [Menu mobile 390px sau khi animation kết thúc](m3-s21-issue25/admin-mobile.png).
- [Builder, desktop 1440px](m3-s21-issue25/builder.png).

## Giới hạn và điều kiện tích hợp

- Các màn Posts/Pages built-in, Media standalone, Theme management/list/activate, Users list/edit, Settings save **chưa** có implementation API đầy đủ trong slice này; UI ghi rõ là đang phát triển, không coi là nghiệm thu các capability đó của M3.
- Test Chrome chạy trên bản **debug embedded Rust** với database tạm, không kiểm toàn bộ binary release/Linux/macOS. Các acceptance môi trường khác của M3 được theo dõi riêng.
- Browser QA được thực hiện qua các script chạy trong terminal thay vì commit một suite Playwright mới, do Issue #25 giới hạn file ownership; các bước/lệnh và hành vi thực được ghi ở đây. Script `windows_admin_browser.py` lịch sử chứa giả định bố cục cũ, chưa được cập nhật bởi task này (ngoài ownership); xem đó là việc tiếp theo nếu cần dùng script lịch sử làm regression cố định.
- Một lần chạy thêm `python cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash.exe` trên backend Harbor mới bị **FAIL tại cleanup** `TemporaryDirectory` với Windows `WinError 32` (file SQLite vẫn bị process giữ), sau các bước API test trước đó. Không coi smoke này PASS và không sửa script ngoài ownership; auth + Content UI đã có kiểm thử riêng PASS. Đây là vấn đề teardown của test harness cần theo dõi riêng khi cải thiện Windows regression.
- Báo cáo này mô tả kiểm chứng trên commit code `b267908`. PR head có thể có commit thêm tài liệu/ảnh, phải đối chiếu **required GitHub checks tại head mới nhất** trước merge. `MERGED` không tự đồng nghĩa `G5 VERIFIED` toàn M3 hoặc `Project Done`.
- Các ảnh chỉ chụp dữ liệu QA ngẫu nhiên trên site tạm; không lưu mật khẩu/token/cookie.
