# Tái cấu trúc Thanh điều hướng Sidebar Admin & Định tuyến IA

* **Status:** Approved
* **Approved By:** Owner
* **Approval Evidence:** Owner chat approval 2026-11-28 ("duyệt")
* **Date:** 2026-11-28
* **Baseline:** `main@b3fba26`
* **Migrations:** Không có (0 SQL migration; giữ nguyên 100% backend Rust & DB schema)
* **Worktree / Branch:** `feat/admin-sidebar-ia`
* **Goldfish Review:** `egm-codebase-details` + `egm-implementation-readiness` · 2026-11-28 · `main@b3fba26` (2 Blocker, 5 Major, 6 Minor/Nit từ 2 reviewer đã được xử lý triệt để: bảo vệ `user === null` cho footer profile, giữ `subLabels.library` cho `#/media`, thêm guard `route.sub === 'install'` + `contactContent` + `aria-controls` ID trong `AdminShell`, bỏ yêu cầu `mayLeave` giả cho `contact`, đồng bộ `ContactPanel` `initialTab` + link hai chiều `SmtpSettingsPanel`, giữ nhánh `selectedNavHash` cho custom taxonomy, xử lý chuyển mode `ThemesPanel`, chuẩn hóa lệnh kiểm chứng `.pot`/`.po`, cùng các edge case `router.ts` & CSS `:focus-visible`).

---

## Overview

Đặc tả này điều chỉnh toàn diện **Kiến trúc Thông tin (Information Architecture — IA)**, **thứ tự & nhãn hiển thị**, **định tuyến chuẩn hóa** và **hành vi tương tác** của thanh điều hướng Admin (`AdminShell`, `AdminShellMenu`, `router`, `AuthPanel`) cùng các liên kết điều hướng liên quan trong `cms/admin/src/`.

Mục tiêu là khắc phục triệt để các bất hợp lý đã kiểm chứng trực tiếp trên ứng dụng đang chạy (`http://127.0.0.1:18766/admin/#/dashboard`) và trong mã nguồn nền `main@b3fba26`, đồng thời giữ tương thích ngược hoàn toàn với mọi URL/bookmark hiện có và đồng bộ tuyến `Content structure` với nhánh song song `feat/acf-content-structure`.

---

## The Problem

Qua kiểm tra trực tiếp thanh sidebar Admin (`admin (Admin)`, locale `en`) và đối chiếu với [AdminShellMenu.tsx](../../cms/admin/src/shell/AdminShellMenu.tsx), [AdminShell.tsx](../../cms/admin/src/shell/AdminShell.tsx), [router.ts](../../cms/admin/src/shell/router.ts), [App.tsx](../../cms/admin/src/App.tsx) và các màn hình đích, tồn tại **8 bất hợp lý** ở 3 mức độ:

1. **Sai lệch hoàn toàn giữa nhãn menu và màn hình thực tế (Bug IA):**
   - `Themes → Install theme` trỏ tới `#/appearance/library` ([AdminShellMenu.tsx#L121-L124](../../cms/admin/src/shell/AdminShellMenu.tsx#L121-L124)), nhưng [TemplateLibraryPage.tsx](../../cms/admin/src/features/library/TemplateLibraryPage.tsx#L98-L110) thực chất là **Local template library** (`Synced patterns`, `Theme templates`, `Site overrides`).
   - Ô upload ZIP cài theme (`Install theme from ZIP`) lại nằm chung trong `Installed themes` ([ThemesPanel.tsx#L225-L229](../../cms/admin/src/features/themes/ThemesPanel.tsx#L225-L229)), khiến người dùng bấm nhầm khi muốn cài theme, và không tìm được nơi reset `Site overrides` dưới tên thật của nó.
2. **Thiếu lối vào trực tiếp `Menus` dưới nhóm `APPEARANCE`:**
   - [router.ts#L206-L214](../../cms/admin/src/shell/router.ts#L206-L214) đã hỗ trợ `#/appearance/menus` chuyển hướng vào `#/studio?panel=menus` ([StudioCustomizerNav.tsx#L193](../../cms/admin/src/studio/StudioCustomizerNav.tsx#L193)), nhưng sidebar `APPEARANCE` không có mục `Menus`, buộc người dùng phải tự đoán mở toàn màn hình `Customize appearance`.
3. **`Contact` nằm ở `CONTENT` nhưng URL mang tiền tố `#/settings/contact` và tách rời khỏi SMTP:**
   - `Contact` mang nhãn nội dung nhưng đặt hash `#/settings/contact` ([AdminShellMenu.tsx#L99-L105](../../cms/admin/src/shell/AdminShellMenu.tsx#L99-L105)), thiếu liên thông với `Settings → Email notifications (SMTP)` ([SmtpSettingsPanel.tsx#L101-L102](../../cms/admin/src/features/contact/SmtpSettingsPanel.tsx#L101-L102)).
4. **`Content structure` ở `SYSTEM` nhưng URL vẫn là `#/appearance/content-types` và `#/taxonomy/definitions`:**
   - Prefix `#/appearance/` là tàn dư thiết kế cũ, lệch với breadcrumb `Administration / System / Content structure` và khác chuẩn `#/content-structure/*` của thiết kế `feat/acf-content-structure`.
5. **`Users → My profile` đặt trong nhóm `SYSTEM` gây bất hợp lý cho Editor và thiếu lối vào ở Topbar/Footer:**
   - Với `Editor`, 4 mục Admin-only trong `SYSTEM` bị ẩn hết, khiến cả tiêu đề nhóm `SYSTEM` chỉ còn đúng `Users → My profile`.
   - Trong khi đó, `#user-info` trên topbar ([AuthPanel.tsx#L94-L96](../../cms/admin/src/shell/AuthPanel.tsx#L94-L96)) là văn bản tĩnh (lại bị ẩn ở `<= 719px` trong [styles.css#L448](../../cms/admin/src/styles.css#L448)), còn `.od-sidebar-bottom` ([AdminShell.tsx#L307](../../cms/admin/src/shell/AdminShell.tsx#L307)) chỉ là chữ tĩnh `OpenDash · Admin`.
6. **Thứ tự theo luồng làm việc và khả năng khám phá chưa hợp lý:**
   - Trong `APPEARANCE`: `Customize appearance` đứng trước `Themes` (ngược trình tự chọn theme trước rồi mới tùy biến).
   - Trong `SYSTEM → Settings`: `Multilingual` chen giữa `General` và các cài đặt cốt lõi `Homepage & posts` / `Permalinks`; nhãn `SEO` gây hiểu nhầm cho màn hình thực chất chỉ cấu hình `Homepage SEO` ([SeoPanel.tsx#L109](../../cms/admin/src/features/seo/SeoPanel.tsx#L109)); thiếu liên kết chéo từ `Homepage SEO → Redirects & 404` và từ `Media library → Media reconciliation`.
   - Trong `SYSTEM → Tools`: `Redirects & 404` nên đứng đầu vì tần suất dùng cao hơn thao tác khẩn cấp/bảo trì (`Backup & restore`, `Media reconciliation`, `System health`).
7. **Bất tiện về UX tương tác trên Sidebar & Layout:**
   - Toàn bộ 13 mục cha tích hợp sẵn + các CPT động (14 mục trên site mẫu hiện có 1 CPT `Dự án`) dùng `<button>` ([AdminShell.tsx#L256-L280](../../cms/admin/src/shell/AdminShell.tsx#L256-L280)), không hỗ trợ `Cmd/Ctrl + Click` mở tab mới với các mục lá.
   - Bấm mục cha có submenu luôn gọi `navigate(item.hash)` ngay lập tức, không cho phép mở xem submenu mà giữ nguyên màn hình hiện tại (thậm chí kích hoạt `window.confirm` rời trang khi đang soạn dở).
   - Mục cha đang active (`isActiveParent`) bị ép luôn mở (`openMenuId === item.id || isActiveParent`) nên không thu gọn lại được.
   - Thẻ `<a href>` của submenu gọi `e.preventDefault()` vô điều kiện ([AdminShell.tsx#L289](../../cms/admin/src/shell/AdminShell.tsx#L289)), chặn `Cmd/Ctrl + Click`.
   - `.od-nav` thiếu `min-height: 0` trong flex column nên có nguy cơ ép cắt `.od-sidebar-bottom` ở một số kích thước viewport.
8. **Thiếu đồng nhất về nhãn, icon và số ít/số nhiều:**
   - [App.tsx#L632](../../cms/admin/src/App.tsx#L632) gán cứng `_x("Category", "app.category")` và `__("Tag")` (số ít) cho CPT tùy chỉnh, trong khi `Posts` dùng `Categories` / `Tags` (số nhiều).
   - `Patterns` dùng chung icon `content` với CPT động.
   - `subLabels.install` ở [AdminShell.tsx#L166](../../cms/admin/src/shell/AdminShell.tsx#L166) gán nhầm chuỗi `__("Settings")`.
   - Ngoài ra, nút `Taxonomy definitions` trong [TaxonomyPage.tsx#L166-L167](../../cms/admin/src/features/taxonomy/TaxonomyPage.tsx#L166-L167) không kiểm tra `admin &&`, khiến `Editor` bấm vào bị dẫn tới màn `403`.

---

## Goals

- Sửa trung thực toàn bộ nhãn, route và màn hình tương ứng: tách riêng màn hình `Install theme` (`#/appearance/themes/install`) giống mô hình `Plugins`, và đặt `Template library` (`#/appearance/library`) về đúng vị trí dưới `Patterns`.
- Bổ sung mục `Menus` (`#/studio?panel=menus`) trong `APPEARANCE` và sắp xếp lại thứ tự tự nhiên trong `APPEARANCE`, `Tools`, `Settings`.
- Chuyển `My profile` (`#/users/profile`) thành lối vào trực tiếp trên Topbar (`#user-info`) và Footer Sidebar (`.od-sidebar-bottom` — hoạt động cả trên mobile `<= 719px`), gỡ `My profile` khỏi `Users` để `Editor` không bao giờ thấy nhóm `SYSTEM` thừa.
- Chuẩn hóa route `Contact` về `#/contact` và route `Content structure` về `#/content-structure/content-types` & `#/content-structure/taxonomies` (giữ toàn bộ alias cũ).
- Cải thiện tương tác Sidebar: **Pure Accordion** cho mục cha có submenu (chỉ đóng/mở, không chuyển trang, thu gọn được cả mục đang active), thẻ `<a href>` chuẩn với kiểm tra `isPlainLeftClick` cho mục lá và mục con, khắc phục clipping ở `.od-nav`.
- Chuẩn hóa số nhiều `Categories`/`Tags` cho CPT, bổ sung icon riêng cho `Menus` & `Patterns`, sửa `subLabels.install`, ẩn nút `Taxonomy definitions` với `Editor`, và bổ sung liên kết chéo hai chiều `Contact ↔ SMTP`, `Homepage SEO → Redirects & 404`, `Media library → Media reconciliation`.

---

## Non-Goals

- **Không thêm SQL migration và không sửa code Rust backend** (`cms/crates/**`): giữ nguyên bảo vệ dữ liệu cá nhân (`require_admin` / `require_admin_with_csrf` trên [routes/contact.rs](../../cms/crates/opendash-server/src/routes/contact.rs)) và toàn bộ test API hiện có.
- **Không xóa hoặc làm hỏng các URL / deep link cũ:** mọi đường dẫn `#/settings/contact`, `#/contact/submissions`, `#/appearance/menus`, `#/appearance/options`, `#/appearance/content-types*`, `#/settings/content-types*`, `#/taxonomy/definitions`, `#/content`, `#/sections*`, `#/tools/404-log`, `#/tools/restore`, `#/themes`, `#/users/profile` đều tiếp tục hoạt động.
- **Không đụng vùng AST/Builder nội bộ của Studio** (`cms/admin/src/studio/`, `cms/admin/src/builder/`) và không thêm màn hình `Field Groups` / `Options Pages` của nhánh song song `feat/acf-content-structure` (chỉ đồng bộ trước chuẩn `#/content-structure/*` để merge sạch).
- **Không thêm dependency mới** vào `cms/admin/package.json` hay `cms/Cargo.toml`.

---

## Open Questions

Không còn câu hỏi mở (toàn bộ phạm vi kỹ thuật, quyền truy cập, URL alias và tương thích nhánh song song đã được khóa theo mã nguồn nền `main@b3fba26`).

---

## Critical User Journeys (CUJs)

### CUJ 1: Cài đặt Theme vs Theme đã cài & Thư viện mẫu (`Patterns`)

- **Given:** Admin đăng nhập ở `#/dashboard` và mở nhóm `APPEARANCE`.
- **When:** Admin bấm vào mục cha `Themes` (chỉ mở submenu, không chuyển trang), chọn `Install theme` (`#/appearance/themes/install`), sau đó chuyển sang `Installed themes` (`#/appearance/themes`) và cuối cùng mở `Patterns → Template library` (`#/appearance/library`).
- **Then:**
  - Bấm `Themes` chỉ mở danh sách con, giữ nguyên trang `#/dashboard`.
  - `#/appearance/themes/install` hiển thị màn hình cài theme (`Install theme from ZIP` với `data-testid="themes-upload"`) và link quay lại `Installed themes`; sau khi upload thành công vẫn ở lại màn hình này để xem thông báo kết quả trước khi bấm sang `Installed themes` (giữ `selectedRevisionId` của bản vừa upload để kích hoạt thuận tiện).
  - `#/appearance/themes` hiển thị danh sách revision v3 đã cài, nút `Install theme` dẫn sang `#/appearance/themes/install`, cùng Sample Data Import & Theme Functions (không lặp lại ô upload ZIP).
  - `#/appearance/library` hiển thị `Local template library`, mục cha `Patterns` được highlight active và breadcrumb là `Administration / Appearance / Patterns / Template library`.

### CUJ 2: Lối vào trực tiếp `Menus` & Liên kết chéo (`Contact ↔ SMTP`, `SEO → Redirects`, `Media → Reconcile`)

- **Given:** Admin đang ở bất kỳ màn hình Admin nào.
- **When:** Admin bấm `APPEARANCE → Menus` (`#/studio?panel=menus`), hoặc truy cập `CONTENT → Contact` (`#/contact` và alias `#/settings/contact`), `Settings → Homepage SEO` (`#/settings/seo`), `Settings → Email notifications (SMTP)` (`#/settings/smtp`), `Media library` (`#/media`).
- **Then:**
  - `Menus` là thẻ `<a href="#/studio?panel=menus">` mở thẳng vào OpenDash Studio tại panel `Menus and locations` (đồng bộ hành vi với `#/appearance/menus`).
  - `Contact` mở tại `#/contact` (`#/settings/contact` tự điều hướng về `#/contact`; `#/contact/inbox` & `#/contact/submissions` mở sẵn tab `Inbox` kể cả khi chuyển từ tab `Forms`) kèm liên kết tới `Email notifications (SMTP)` (`#/settings/smtp`), và màn `SMTP` có liên kết ngược lại `Contact forms & inbox` (`#/contact`).
  - `Homepage SEO` hiển thị tiêu đề rõ ràng `Homepage SEO` trên sidebar & trang, kèm liên kết `Redirects & 404` (`#/tools/redirects`).
  - `Media library` hiển thị liên kết `Media reconciliation` (`#/tools/media-reconcile`) chỉ cho vai trò `Admin` (`Editor` không thấy).

### CUJ 3: Pure Accordion, Modifier-Click (`Cmd/Ctrl + Click`) & Giữ trạng thái bản nháp

- **Given:** Người dùng đang soạn bài viết có thay đổi chưa lưu (`hasUnsavedChanges = true`).
- **When:** Người dùng bấm mở/thu gọn mục cha `Posts`, `Pages` hoặc `Settings`, sau đó dùng `Cmd/Ctrl + Click` trên một mục con/mục lá và cuối cùng bấm chuột trái bình thường trên một mục lá.
- **Then:**
  - Bấm vào bất kỳ mục cha có submenu nào đều chỉ đóng/mở submenu đó (cho phép thu gọn cả mục cha đang active), **không** điều hướng và **không** bật hộp thoại xác nhận rời trang.
  - Bấm `Cmd/Ctrl + Click` (hoặc Middle/Shift Click) trên mục lá hoặc mục con không gọi `e.preventDefault()`, cho phép trình duyệt mở tab mới tự nhiên.
  - Bấm chuột trái thông thường gọi `e.preventDefault()` và đi qua `navigate()` / `mayLeave()` bảo vệ bản nháp như hiện hành.

### CUJ 4: `My profile` trên Topbar + Sidebar Footer & Sidebar sạch cho `Editor`

- **Given:** Hệ thống có cả tài khoản `Admin` và `Editor`.
- **When:** `Editor` đăng nhập trên desktop (`1440×900`) và mobile (`390×844`).
- **Then:**
  - Thanh sidebar của `Editor` **không** hiển thị tiêu đề nhóm `SYSTEM` hay mục `Users` (chỉ còn `Dashboard` và nhóm `CONTENT`).
  - Ở desktop, bấm vào tên người dùng `#user-info` trên topbar hoặc thẻ hồ sơ ở `.od-sidebar-bottom` đều mở `#/users/profile` với breadcrumb `Administration / My profile` (đi qua `mayLeave` nếu đang sửa dở).
  - Ở mobile (`<= 719px`, khi `.od-topbar #user-info` ẩn), mở drawer sidebar và bấm vào thẻ hồ sơ ở `.od-sidebar-bottom` sẽ đóng drawer khi điều hướng (`navigate('#/users/profile')`) và mở `#/users/profile`.
  - Khi chưa đăng nhập hoặc mất phiên `401` (`user === null`), `.od-sidebar-bottom` ẩn/không render liên kết hồ sơ để tránh truy cập `user.username` gây lỗi runtime hay rò thẻ điều hướng trong `.od-layout.hidden`.
  - Khi `Editor` thử truy cập trực tiếp `#/contact`, `#/appearance/themes/install`, `#/appearance/library`, `#/content-structure/content-types`, `#/content-structure/taxonomies`, `#/users`, `#/tools/*`, `#/settings/*`, `#/studio`, màn hình vẫn chặn `403 — Access denied` (`routeForbidden`).

### CUJ 5: Đồng bộ `#/content-structure/*`, Số nhiều CPT & Chuẩn Gettext `.pot`/`.po`

- **Given:** Website có CPT tùy chỉnh (ví dụ `project`) gắn `category`, `tag` và taxonomy tùy chỉnh; CMS chuyển đổi qua lại giữa `en` và `vi`.
- **When:** Admin mở `System → Content structure` (`Content types & fields` và `Custom taxonomies`), đồng thời kiểm tra các alias cũ (`#/appearance/content-types`, `#/settings/content-types/project`, `#/taxonomy/definitions`, `#/content`).
- **Then:**
  - Sidebar hiển thị `Categories` / `Tags` (số nhiều) cho taxonomy tích hợp của CPT.
  - `#/content-structure/content-types` và `#/content-structure/taxonomies` hoạt động đúng draft retention (kể cả trong [TaxonomyDefinitionsPage.tsx#L100](../../cms/admin/src/features/taxonomy/TaxonomyDefinitionsPage.tsx#L100) khi gặp `401`) và mọi alias cũ đều hoạt động nguyên vẹn.
  - Không còn chuỗi tiếng Việt hardcoded trong `cms/admin/src/` (`node cms/scripts/check-i18n.mjs`), đồng thời `--l10n-extract` / `--l10n-compile --pot` và suite `i18n` xác nhận 100% khóa mới được khai báo và dịch đầy đủ trong `cms/l10n/opendash.pot` và `cms/l10n/vi.po`.

---

## Technical Plan (Architecture)

### 1. Bản đồ Menu Sidebar, Topbar & Footer mục tiêu

#### Topbar & Sidebar Footer (mọi vai trò đã đăng nhập)

| Vị trí | Element & `data-testid` | Nhãn EN / VI | Đích (`href`) | Quyền |
| --- | --- | --- | --- | --- |
| Topbar phải | `<a id="user-info" data-testid="topbar-profile-link">` | `<username> (Admin\|Editor)` | `#/users/profile` | Admin, Editor |
| Sidebar đáy | `<a data-testid="sidebar-profile-link">` trong `.od-sidebar-bottom` | `<username> · Admin\|Editor · My profile` (`Hồ sơ của tôi`) | `#/users/profile` | Admin, Editor |

#### Cấu trúc điều hướng chính (từ trên xuống dưới)

| # | Nhóm | Mục cha (`data-menu-id`) & Loại DOM | Icon | Submenu & Canonical Hash | Quyền |
| --- | --- | --- | --- | --- | --- |
| 1 | *(Không nhóm)* | **Dashboard** (`dashboard`) — `<a>` lá | `dashboard` | `#/dashboard` | Admin, Editor |
| 2 | **CONTENT** (`Nội dung`) | **Posts** (`posts`) — `<button>` accordion | `posts` | • `All posts` (`#/posts`)<br>• `Add new post` (`#/posts/new`)<br>• `Categories` (`#/posts/categories`)<br>• `Tags` (`#/posts/tags`) | Admin, Editor |
| 3 | **CONTENT** | **Pages** (`pages`) — `<button>` accordion | `pages` | • `All pages` (`#/pages`)<br>• `Add new page` (`#/pages/new`) | Admin, Editor |
| 4 | **CONTENT** | **[CPT động]** (`cpt:<key>`) — `<button>` accordion | `content` | • `All {{label}}` (`#/cpt/:key`)<br>• `Add new` (`#/cpt/:key/new`)<br>• Taxonomy gắn CPT (built-in hiển thị `Categories` / `Tags`) | Admin, Editor |
| 5 | **CONTENT** | **Media library** (`media`) — `<a>` lá | `media` | `#/media` | Admin, Editor |
| 6 | **CONTENT** | **Contact** (`contact`) — `<a>` lá | `mail` | `#/contact` *(alias: `#/settings/contact`, `#/contact/submissions`)* | Admin only |
| 7 | **APPEARANCE** (`Giao diện`) | **Themes** (`themes`) — `<button>` accordion | `themes` | • `Installed themes` (`#/appearance/themes`)<br>• `Install theme` (`#/appearance/themes/install`) | Admin only |
| 8 | **APPEARANCE** | **Customize appearance** (`studio`) — `<a>` lá | `builder` | `#/studio` *(alias: `#/builder`, `#/appearance/options`)* | Admin only |
| 9 | **APPEARANCE** | **Menus** (`menus`) — `<a>` lá | `menus` *(mới)* | `#/studio?panel=menus` *(alias: `#/appearance/menus`)* | Admin only |
| 10 | **APPEARANCE** | **Patterns** (`patterns`) — `<button>` accordion | `patterns` *(mới)* | • `All patterns` (`#/patterns`)<br>• `Add pattern` (`#/patterns/new`)<br>• `Template library` (`#/appearance/library`) | Admin only |
| 11 | **SYSTEM** (`Hệ thống`) | **Content structure** (`schema`) — `<button>` accordion | `schema` | • `Content types & fields` (`#/content-structure/content-types`)<br>• `Custom taxonomies` (`#/content-structure/taxonomies`) | Admin only |
| 12 | **SYSTEM** | **Plugins** (`plugins`) — `<button>` accordion | `plugins` | • `Installed plugins` (`#/plugins`)<br>• `Install plugin` (`#/plugins/install`) | Admin only |
| 13 | **SYSTEM** | **Users** (`users`) — `<button>` accordion | `users` | • `All users` (`#/users`)<br>• `Add user` (`#/users/new`) | Admin only |
| 14 | **SYSTEM** | **Tools** (`tools`) — `<button>` accordion | `tools` | • `Redirects & 404` (`#/tools/redirects`)<br>• `Backup & restore` (`#/tools/backup`)<br>• `Media reconciliation` (`#/tools/media-reconcile`)<br>• `System health` (`#/tools/health`) | Admin only |
| 15 | **SYSTEM** | **Settings** (`settings`) — `<button>` accordion | `settings` | • `General` (`#/settings/general`)<br>• `Homepage & posts` (`#/settings/reading`)<br>• `Permalinks` (`#/settings/permalinks`)<br>• `Homepage SEO` (`#/settings/seo`)<br>• `Multilingual` (`#/settings/multilingual`)<br>• `Email notifications (SMTP)` (`#/settings/smtp`) | Admin only |

> **Lưu ý RBAC cho `Editor`:** Khi `user.role !== 'admin'`, toàn bộ mục trong nhóm `SYSTEM` đều có `adminOnly: true`, nên `menuGroups` tự động loại bỏ nhóm `SYSTEM` khỏi danh sách hiển thị (với `Editor`, sidebar chỉ hiển thị `Dashboard` và nhóm `CONTENT`).

---

### 2. Hợp đồng Định tuyến & RBAC (`cms/admin/src/shell/router.ts`)

1. **Thêm `'contact'` vào `AdminView`:**
   - `#/contact`, `#/contact/forms`, hoặc `#/contact/<khác>` không hợp lệ → `{ view: 'contact', sub: 'forms', rawHash: '#/contact' }`
   - `#/contact/inbox`, `#/contact/submissions` → `{ view: 'contact', sub: 'inbox', rawHash: `#/contact/${segs[1]}` }`
   - `#/settings/contact` (và `#/settings/contact/inbox|submissions`) → `{ view: 'contact', sub: segs[2] === 'inbox' || segs[2] === 'submissions' ? 'inbox' : 'forms', rawHash: ..., redirectHash: segs[2] === 'inbox' || segs[2] === 'submissions' ? '#/contact/inbox' : '#/contact' }` *(tự chuẩn hóa URL trên trình duyệt về `#/contact*`)*.
2. **Tách `install` khỏi `library` trong `themes`:**
   - Kiểm tra rõ `segs[2] === 'install'` khi `first === 'appearance' && segs[1] === 'themes'`, và `segs[1] === 'install'` khi `first === 'themes'`:
     - `#/appearance/themes` (và `#/themes`) → `{ view: 'themes', sub: 'themes', rawHash: '#/appearance/themes' }`.
     - `#/appearance/themes/install` → `{ view: 'themes', sub: 'install', rawHash: '#/appearance/themes/install' }`.
     - `#/themes/install` → `{ view: 'themes', sub: 'install', rawHash: '#/themes/install', redirectHash: '#/appearance/themes/install' }`.
     - `#/appearance/library` → giữ nguyên `{ view: 'themes', sub: 'library', rawHash: '#/appearance/library' }` (giữ `routeOwnsMain` và RBAC Admin-only trong `AdminShell`).
3. **Chuẩn hóa `Content structure` & giữ toàn bộ alias:**
   - `#/content-structure/content-types[/key]` → `{ view: 'settings', sub: 'content-types', typeKey, rawHash: typeKey ? `#/content-structure/content-types/${encodeURIComponent(typeKey)}` : '#/content-structure/content-types' }`.
   - `#/content-structure/taxonomies` → `{ view: 'taxonomy', sub: 'definitions', taxonomyMode: 'definitions', rawHash: '#/content-structure/taxonomies' }`.
   - `#/content-structure` (trống `segs[1]` hoặc `segs[1]` không nhận diện) → `{ view: 'settings', sub: 'content-types', rawHash: '#/content-structure/content-types', redirectHash: '#/content-structure/content-types' }`.
   - Các đường dẫn cũ `#/appearance/content-types[/key]`, `#/settings/content-types[/key]` và `#/taxonomy/definitions` **giữ nguyên parse** (không ép `redirectHash`, đảm bảo không phá vỡ link/test cũ như `entry_lifecycle_trash_e2e.py#L303`). Khi người dùng chọn một CPT khác trong `ContentTypeSchemaSettings` (`App.tsx#L677`), URL chuyển sang `#/content-structure/content-types/${encodeURIComponent(key)}` (lưu ý: mở bare `#/appearance/content-types` hoặc `#/settings/content-types` tự động chọn CPT đầu tiên qua `ContentTypeSchemaSettings.tsx#L219-L221` nên sẽ chuyển sang `#/content-structure/content-types/<firstKey>`, còn deep-link có sẵn `:key` giữ nguyên URL cho tới khi đổi CPT).
   - `#/content` tiếp tục trả `isLegacyContentRedirect: true`; `App.tsx#L303` chuyển hướng Admin tới `#/content-structure/content-types`.
4. **Cập nhật RBAC (`isRouteAllowed` & `isViewAllowed`):**
   - Chặn `route.view === 'contact'` (cũng như `view === 'contact'`) khi `role !== 'admin'`.

---

### 3. Hợp đồng Trạng thái & Tương tác Sidebar (`AdminShell.tsx`, `AuthPanel.tsx`, `styles.css`)

1. **Quản lý trạng thái mở Accordion độc lập (`expandedMenus`):**
   - Thay `openMenuId: string | null` bằng `expandedMenus: Record<string, boolean>`, khởi tạo `{ [activeParentId]: true }`.
   - Khi `activeParentId` đổi (do điều hướng route hoặc deep-link), `useEffect` kiểm tra `if (!activeParent?.children?.length) return;` (tránh ghi các key mục lá vào state) rồi mở bổ sung `prev[activeParentId] ? prev : { ...prev, [activeParentId]: true }` nhưng không tự động đóng các mục khác đang mở, và vẫn cho phép người dùng bấm thu gọn lại chính mục đang active.
2. **Phân biệt Mục lá (`<a>`) vs Mục cha Accordion (`<button>`):**
   - Hàm dùng chung `isPlainLeftClick(event: React.MouseEvent): boolean` (`!event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey && event.button === 0`).
   - **Mục cha không có `visibleChildren` (Dashboard, Media library, Contact, Customize appearance, Menus):** render `<a href={item.hash} data-menu-id={item.id} aria-current={isActiveParent ? 'page' : undefined} className={'od-nav-link w-full justify-between no-underline ' + (isActiveParent ? 'od-nav-link-active' : '')}>`; chỉ gọi `e.preventDefault(); navigate(item.hash)` khi `isPlainLeftClick(e)` là `true`.
   - **Mục cha có `visibleChildren`:** render `<button type="button" data-menu-id={item.id} aria-expanded={isExpanded} aria-controls={\`od-nav-sub-${item.id}\`}>` chỉ toggle `expandedMenus[item.id]`, **không bao giờ gọi `navigate()`**. Container danh sách con (`AdminShell.tsx#L282`) gắn đúng `id={\`od-nav-sub-${item.id}\`}`.
   - **Mục con `<a>`:** áp dụng cùng `isPlainLeftClick(e)` trước khi `e.preventDefault(); navigate(sub.hash)` để hỗ trợ mở tab mới bằng `Cmd/Ctrl + Click`. Lưu ý `navigate()` gọi `onNavigateHash(hash); setMobileOpen(false);` trực tiếp (hành vi đóng mobile drawer ngay khi người dùng chạm link là nhất quán với thiết kế mobile hiện hành).
3. **Đồng bộ `activeParentId`, `selectedNavHash`, `breadcrumb` & Slots trong `AdminShell.tsx`:**
   - Thêm `contactContent: React.ReactNode` vào `AdminShellProps` và mount `{user?.role === 'admin' && route.view === 'contact' && <div className="od-admin-card min-w-0">{contactContent}</div>}` trong `#admin-main` (đồng thời bỏ nhánh `route.sub === 'contact'` khỏi `settingsContent` trong `App.tsx#L729-L730`).
   - Cập nhật vùng render `themesContent` tại `AdminShell.tsx#L423`: `{user?.role === 'admin' && route.view === 'themes' && (route.sub === 'themes' || route.sub === 'install') && <div className="od-admin-card min-w-0">{themesContent}</div>}` để `#/appearance/themes/install` hiển thị màn hình cài theme.
   - `activeParentId`:
     - `route.view === 'contact'` → `'contact'`
     - `route.view === 'themes' && route.sub === 'library'` → `'patterns'`
     - `route.view === 'themes'` → `'themes'`
     - `(route.view === 'studio' || route.view === 'studio-entry') && route.studioPanel === 'menus'` → `'menus'`
     - `route.view === 'studio' || route.view === 'studio-entry'` → `'studio'`
     - `route.view === 'profile'` → `'profile'` *(không gán `'users'` để breadcrumb hiển thị `Administration / My profile`)*
     - `route.view === 'settings' && route.sub === 'content-types'` → `'schema'`
     - `route.view === 'taxonomy'` → giữ logic hiện hành phân biệt `cpt:<key>`, `'schema'` và `'posts'`
   - `selectedNavHash`:
     - `route.view === 'themes'` → `route.sub === 'install' ? '#/appearance/themes/install' : route.sub === 'library' ? '#/appearance/library' : '#/appearance/themes'`
     - `route.view === 'contact'` → `'#/contact'`
     - `route.view === 'studio'` → `route.studioPanel === 'menus' ? '#/studio?panel=menus' : '#/studio'`
     - `route.view === 'settings'` → `route.sub === 'content-types' ? '#/content-structure/content-types' : \`#/settings/${route.sub}\``
     - `route.view === 'taxonomy'` → giữ đủ cả 2 nhánh `route.taxonomyMode === 'definitions' || (route.taxonomyKind !== 'category' && route.taxonomyKind !== 'tag')` trả về `'#/content-structure/taxonomies'` (không làm mất highlight ở `#/taxonomy/<custom_key>/terms`).
   - `subLabels`: sửa `'install': __("Install")`, **giữ nguyên** `'library': __("Library")` (vì `#/media` có `sub: 'library'` và không có `activeChild`, trong khi `#/appearance/library` đã lấy nhãn `"Template library"` từ `activeChild.label`), và bổ sung `'seo': __("Homepage SEO")`, `'forms': __("Contact")`, `'inbox': __("Contact")`.
4. **Topbar & Sidebar Footer Profile + CSS Layout (`AuthPanel.tsx`, `AdminShell.tsx`, `styles.css`):**
   - `AuthPanel.tsx`: nhận thêm `profileHref?: string` (mặc định `'#/users/profile'`), `isProfileActive?: boolean`, `onNavigateProfile?: () => void`; biến `#user-info` thành `<a id="user-info" href={profileHref} data-testid="topbar-profile-link" aria-current={isProfileActive ? 'page' : undefined}>` dùng `isPlainLeftClick`. `AdminShell.tsx` truyền `profileHref="#/users/profile"`, `isProfileActive={route.view === 'profile'}`, `onNavigateProfile={() => navigate('#/users/profile')}` để kích hoạt đồng nhất `mayLeave`.
   - `AdminShell.tsx`: ở `.od-sidebar-bottom`, **bắt buộc guard `user ? (...) : null`** (vì `.od-layout` vẫn mount ở DOM với `.hidden` khi `user === null` lúc khởi động, đăng xuất hoặc `401` quarantine). Khi `user !== null`, render `<a href="#/users/profile" data-testid="sidebar-profile-link" className="od-sidebar-profile ..." aria-current={route.view === 'profile' ? 'page' : undefined}>` kèm avatar chữ cái đầu `(user.username.trim().charAt(0) || 'U').toUpperCase()`, `user.username`, vai trò và nhãn `My profile` (plain left click gọi `navigate('#/users/profile')`).
   - `styles.css`: thêm `min-height: 0;` cho `.od-nav`, `text-decoration: none` cho `.od-nav-link` / `.od-sidebar-profile` / `#user-info`, và mở rộng rule `:focus-visible` (`styles.css#L427-L430`) cho `.od-nav a`, `.od-sidebar-profile`, `#user-info`.

---

### 4. Split Chế độ `ThemesPanel`, Liên kết Chéo & Đồng bộ Màn hình Liên quan

- **`ThemesPanel.tsx` (`mode?: 'list' | 'install'`, mặc định `'list'`):**
  - `mode === 'install'`: hiển thị tiêu đề `Install theme`, hướng dẫn gói Theme v3, ô upload ZIP (`data-testid="themes-upload"`), thông báo thành công/lỗi kèm thẻ `<a href="#/appearance/themes">` quay lại `Installed themes`. Sau khi upload thành công, giữ nguyên người dùng ở trang `Install theme` để đọc xác nhận và bảo toàn `selectedRevisionId` của revision vừa tải lên khi người dùng tự chuyển sang `Installed themes` để kích hoạt.
  - `mode === 'list'`: hiển thị tiêu đề `Installed themes` + nút liên kết `<a href="#/appearance/themes/install">` (`Install theme`), danh sách revision đã cài, nút kích hoạt, shortcut Studio, Sample Data Import và Theme Functions (gỡ bỏ ô upload ZIP trùng lặp khỏi `mode === 'list'`, đồng nhất 100% với `PluginsPanel.tsx`).
  - Thêm `useEffect(() => { setNotice(''); setError(''); }, [mode]);` để không làm rò banner thông báo khi chuyển giữa hai màn hình. Dùng `<a href>` chuẩn để hỗ trợ `Cmd/Ctrl + Click` và đồng nhất với `ThemesPanel.tsx#L284` mà không cần thêm prop `onNavigateHash`.
- **Liên kết chéo giữa các màn hình liên quan (dùng `<a href="#/...">` chuẩn — kích hoạt `hashchange`/`mayLeave` và hỗ trợ `Cmd/Ctrl + Click` không cần sửa `MediaRoute.tsx`):**
  - `ContactPanel.tsx` (hiện 478 LOC): nhận `initialTab?: 'forms' | 'inbox'` (mặc định `'forms'`), đồng bộ qua `useEffect(() => { setActiveTab(initialTab); }, [initialTab]);` (chỉ chuyển tab qua click vẫn giữ state cục bộ), và bổ sung link ở header tới `Email notifications (SMTP)` (`<a href="#/settings/smtp">`).
  - `SmtpSettingsPanel.tsx` (hiện 219 LOC): bổ sung link ở header quay về `Contact forms & inbox` (`<a href="#/contact">`).
  - `SeoPanel.tsx` (hiện 188 LOC): bổ sung link tới `Redirects & 404` (`<a href="#/tools/redirects">`).
  - `MediaLibraryPage.tsx` (hiện 285 LOC): bổ sung link Admin-only `Media reconciliation` (`<a href="#/tools/media-reconcile">`) ở header cạnh nút `Reload` / `+ Upload files`.
  - `App.tsx` (hiện 797 LOC / trần baseline 879 LOC): truyền `contactContent={<ContactPanel csrfToken={csrfToken} initialTab={route.sub === 'inbox' ? 'inbox' : 'forms'} onApiError={(error) => handleApiError(error, __("Contact form management failed."))} />}` (bỏ `route.sub === 'contact'` khỏi `settingsContent`), truyền `mode={route.view === 'themes' && route.sub === 'install' ? 'install' : 'list'}` cho `ThemesPanel`, đổi `cptTaxonomies` `'category'` → `__("Categories")` *(tái sử dụng key đã có sẵn trong `opendash.pot#L2039`)* & `'tag'` → `__("Tags")`, cập nhật `#/appearance/content-types` sang `#/content-structure/content-types` tại `L303` & `L677`.
  - `TaxonomyDefinitionsPage.tsx` (hiện 561 LOC): giữ draft tại `L100` khi hash là `#/content-structure/taxonomies` HOẶC `#/taxonomy/definitions`.
  - `TaxonomyPage.tsx` (hiện 323 LOC): ẩn nút `Taxonomy definitions` với `Editor` (`admin &&`) và trỏ sang `#/content-structure/taxonomies` tại `L166-L167`.

---

## Alternatives Considered

1. **Split-row trên mục cha (vùng nhãn `<a>` điều hướng + nút `▾` riêng để mở submenu) vs Pure Accordion `<button>` + Mục lá `<a>`:**
   - *Bác bỏ split-row:* Trên sidebar hẹp `244px`, vùng bấm chevron nhỏ dễ bấm nhầm gây chuyển trang ngoài ý muốn (mất ngữ cảnh khi đang sửa bài viết), và mục cha như `Tools` / `Settings` không đại diện cho 1 trang duy nhất bằng các mục con của nó.
2. **Di chuyển `Content structure` sang nhóm `CONTENT` vs Giữ ở `SYSTEM` với `#/content-structure/*`:**
   - *Chọn giữ ở `SYSTEM` với `#/content-structure/*`:* Giữ nhóm `CONTENT` tập trung hoàn toàn vào tác nghiệp nội dung hàng ngày, tách biệt rạch ròi với cấu hình schema Admin-only, và khớp 100% với đặc tả đã duyệt của `feat/acf-content-structure` (`Hệ thống → Cấu trúc nội dung`, `#/content-structure/content-types`).
3. **Mở quyền `Contact Inbox` cho `Editor` vs Giữ `Admin only`:**
   - *Chọn giữ `Admin only`:* Mọi API `/api/admin/contact/**` đều khóa cứng `require_admin` / `require_admin_with_csrf` để bảo vệ PII người gửi form và được kiểm chứng chặt trong `contact_forms.rs`; mở cho Editor đòi hỏi thay đổi backend Rust và chính sách bảo mật ngoài phạm vi IA.
4. **Xóa `TemplateLibraryPage` (`#/appearance/library`) vs Di chuyển sang `Patterns → Template library`:**
   - *Chọn giữ và đặt đúng tên/vị trí dưới `Patterns`:* Đây là UI duy nhất cho phép xem/reset `Site overrides` trong DB và đối chiếu các template của theme đang kích hoạt.

---

## Non-Functional Requirements

- **Bảo mật & RBAC:** Không thay đổi hoặc nới lỏng bất kỳ guard `isRouteAllowed` / `isViewAllowed` nào; mọi route `contact`, `themes` (`themes`, `install`, `library`), `studio`, `patterns`, `users`, `tools`, `settings` và `taxonomy/definitions` vẫn trả `403` với `Editor` nếu truy cập trực tiếp qua URL.
- **Khả năng truy cập (Accessibility):** Đủ thuộc tính `aria-expanded`, `aria-controls`, `aria-current="page"`, focus ring `:focus-visible`, và hỗ trợ mở tab mới (`Cmd/Ctrl + Click`) trên toàn bộ thẻ `<a href>`.
- **Responsive (`<= 719px` và `<= 1023px`):** Sửa lỗi flex overflow `.od-nav { min-height: 0; }` để `.od-sidebar-bottom` luôn hiển thị trọn vẹn và cung cấp lối vào `My profile` ngay cả khi `#user-info` trên topbar bị ẩn ở mobile.
- **Giới hạn LOC & Clean Code:** Mọi file mới `<= 600` LOC; mọi file trong `cms/scripts/loc-baseline.json` (như `App.tsx` trần 879 LOC) không vượt baseline; không trùng lặp logic click handler nhờ helper `isPlainLeftClick` dùng chung.
- **Đa ngôn ngữ (i18n):** 100% chuỗi mới dùng `__()` / `_x()`, không còn chuỗi tiếng Việt hardcoded (`node cms/scripts/check-i18n.mjs`), đủ cặp bản dịch Anh–Việt trong `cms/l10n/opendash.pot` và `cms/l10n/vi.po` được kiểm chứng qua `--l10n-extract` / `--l10n-compile --pot` và bộ test `i18n`.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | LOC / Cap | Summary |
| --- | --- | --- | --- |
| `cms/admin/src/shell/router.ts` | Modify | `<= 450 / 600` | Thêm `view: 'contact'`, `themes` `sub: 'install'`, canonical `#/content-structure/{content-types,taxonomies}`, giữ toàn bộ alias cũ và RBAC guard. |
| `cms/admin/src/shell/AdminShellMenu.tsx` | Modify | `<= 260 / 600` | Sắp xếp lại cây menu 3 nhóm (`Themes` trước `Customize appearance`, thêm `Menus`, chuyển `Template library` sang `Patterns`, tách `Install theme`, sắp xếp `Tools`/`Settings`, thêm icon `menus` & `patterns`). |
| `cms/admin/scripts/test-admin-navigation.mjs` | Create | `<= 300 / 600` | Unit tests (`node:test`) cho `parseAdminRoute`, `isRouteAllowed`, `isViewAllowed`, `buildMenuTree`, alias cũ và cấu trúc menu theo vai trò `admin`/`editor`. |
| `cms/admin/package.json` | Modify | `<= 35` | Thêm script `"test:nav": "node ./scripts/test-admin-navigation.mjs"`. |
| `cms/admin/src/shell/AdminShell.tsx` | Modify | `<= 520 / 600` | Pure accordion `expandedMenus` (guard leaf), `isPlainLeftClick`, leaf `<a>`, `id` cho submenu, `activeParentId`/`selectedNavHash`/`breadcrumb` mới, `contactContent` + `route.sub === 'install'`, `.od-sidebar-bottom` profile link (guard `user`), giữ `subLabels.library` và sửa `subLabels.install`. |
| `cms/admin/src/shell/AuthPanel.tsx` | Modify | `<= 140 / 600` | Biến `#user-info` thành liên kết `#/users/profile` hỗ trợ modifier-click. |
| `cms/admin/src/styles.css` | Modify | `<= 500` | Thêm `min-height: 0` cho `.od-nav`, `text-decoration: none` + `:focus-visible` cho `.od-nav a`, `.od-sidebar-profile`, `#user-info`. |
| `cms/admin/src/App.tsx` | Modify | `<= 835 / 879` | Nối `contactContent`, truyền `mode` cho `ThemesPanel`, plural `Categories`/`Tags` ở `cptTaxonomies`, `#/content-structure/content-types` tại `L303` & `L677`. |
| `cms/admin/src/features/themes/ThemesPanel.tsx` | Modify | `<= 510 / 600` | Hỗ trợ `mode: 'list' \| 'install'`, reset `notice`/`error` khi đổi mode và `<a href>` chuyển qua lại giữa `Installed themes` và `Install theme`. |
| `cms/admin/src/features/contact/ContactPanel.tsx` | Modify | `<= 495 / 600` | Hỗ trợ `initialTab` (`forms` \| `inbox`) đồng bộ theo `useEffect` và `<a href="#/settings/smtp">` sang SMTP. |
| `cms/admin/src/features/contact/SmtpSettingsPanel.tsx` | Modify | `<= 235 / 600` | Bổ sung liên kết `<a href="#/contact">` quay lại `Contact forms & inbox`. |
| `cms/admin/src/features/seo/SeoPanel.tsx` | Modify | `<= 205 / 600` | Bổ sung liên kết chéo `<a href="#/tools/redirects">` (`Redirects & 404`). |
| `cms/admin/src/features/media/MediaLibraryPage.tsx` | Modify | `<= 300 / 600` | Bổ sung liên kết Admin-only `<a href="#/tools/media-reconcile">`. |
| `cms/admin/src/features/taxonomy/TaxonomyDefinitionsPage.tsx` | Modify | `<= 565 / 600` | Giữ draft tại `L100` cho cả `#/content-structure/taxonomies` và `#/taxonomy/definitions`. |
| `cms/admin/src/features/taxonomy/TaxonomyPage.tsx` | Modify | `<= 325 / 600` | Giới hạn nút `Taxonomy definitions` cho `admin &&` và trỏ về `#/content-structure/taxonomies`. |
| `cms/l10n/opendash.pot` & `cms/l10n/vi.po` | Modify | — | Bổ sung và đối soát bản dịch Việt–Anh cho toàn bộ nhãn/chuỗi mới. |
| `cms/scripts/revisions_autosave_e2e.py` | Modify | `<= 900` | Cập nhật bước click `Pages` theo hành vi pure accordion (`click button[data-menu-id="pages"]` → `click a[href="#/pages"]`). |
| `cms/scripts/admin_sidebar_ia_e2e.py` | Create | `<= 400` | Playwright Chrome E2E kiểm chứng CUJ 1–5 trên desktop & mobile viewport cho cả `Admin` và `Editor`. |
| `docs/product/{ADMIN_NAVIGATION_AND_EDIT_SCREEN,PRODUCT_REQUIREMENTS}.md`, `cms/admin/src/shell/README.md`, `docs/designs/SITEMAP.md` | Modify | — | Đồng bộ tài liệu sản phẩm (`§3` điều hướng Admin), README module shell và trạng thái trên Sitemap. |

---

### Slice S01: Router Canonical Routes, Aliases, RBAC & Navigation Menu Tree

- **Dependencies:** Không có (nền tảng `main@b3fba26` + Owner duyệt spec).
- **File Ownership:** `cms/admin/src/shell/router.ts`, `cms/admin/src/shell/AdminShellMenu.tsx`, `cms/admin/scripts/test-admin-navigation.mjs`, `cms/admin/package.json`.
- **Acceptance Criteria:**
  - [ ] `router.ts` hỗ trợ `view: 'contact'` (`#/contact*`, redirect `#/settings/contact*` → `#/contact*`), `themes` `sub: 'install'` (`#/appearance/themes/install`, redirect `#/themes/install`), `#/content-structure/content-types[/key]`, `#/content-structure/taxonomies` & bare `#/content-structure` redirect, giữ 100% alias cũ và chặn `Editor` truy cập `contact` trong `isRouteAllowed` / `isViewAllowed` *(lưu ý chuyển tiếp nội bộ `S01 → S02`: giữa `S01` và `S02`, `typecheck` & `test:nav` xanh, riêng render view cho `contact` và `themes/install` được nối ở `S02`)*.
  - [ ] `AdminShellMenu.tsx` xuất `buildMenuTree` đúng thứ tự mới (`Dashboard` + `CONTENT` + `APPEARANCE` với `Themes → Customize appearance → Menus → Patterns` + `SYSTEM` gồm `Content structure`, `Plugins`, `Users` không chứa `My profile`, `Tools`, `Settings`), bổ sung icon `menus` và `patterns`, ẩn toàn bộ `SYSTEM` với `Editor`.
  - [ ] Tạo `cms/admin/scripts/test-admin-navigation.mjs` kiểm chứng đầy đủ bảng định tuyến, alias, RBAC và cấu trúc menu theo vai trò; chạy `npm --prefix cms/admin run test:nav` và `npm --prefix cms/admin run typecheck` đạt `PASS`.
- **Rollback:** `git revert` commit `S01` trên nhánh `feat/admin-sidebar-ia`.

### Slice S02: Sidebar Accordion UX, Topbar/Footer Profile, `ThemesPanel` Install Mode & Cross-Links

- **Dependencies:** `S01`.
- **File Ownership:** `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/shell/AuthPanel.tsx`, `cms/admin/src/styles.css`, `cms/admin/src/App.tsx`, `cms/admin/src/features/themes/ThemesPanel.tsx`, `cms/admin/src/features/contact/{ContactPanel,SmtpSettingsPanel}.tsx`, `cms/admin/src/features/seo/SeoPanel.tsx`, `cms/admin/src/features/media/MediaLibraryPage.tsx`, `cms/admin/src/features/taxonomy/{TaxonomyDefinitionsPage,TaxonomyPage}.tsx`.
- **Acceptance Criteria:**
  - [ ] `AdminShell.tsx` dùng `expandedMenus` (pure accordion, guard `activeParent?.children?.length`, không gọi `navigate` khi bấm mở/đóng), render `<a href>` kèm `isPlainLeftClick` cho mục lá và mục con, gắn `id={\`od-nav-sub-${item.id}\`}`, tính chính xác `activeParentId`/`selectedNavHash`/`breadcrumb` (giữ nhánh custom taxonomy tại `#/content-structure/taxonomies`), render `contactContent` & `themesContent` cho `(route.sub === 'themes' || route.sub === 'install')`, guard `user` trên `.od-sidebar-bottom` profile link, giữ `subLabels.library` và sửa `subLabels.install`.
  - [ ] `AuthPanel.tsx` nâng `#user-info` thành liên kết `#/users/profile` hỗ trợ modifier-click; `styles.css` thêm `min-height: 0` cho `.od-nav`, `no-underline` và `:focus-visible` cho các link mới.
  - [ ] `ThemesPanel.tsx` tách rõ `mode: 'list' | 'install'` kèm reset banner khi chuyển mode; `ContactPanel.tsx` đồng bộ `initialTab`; `ContactPanel.tsx`, `SmtpSettingsPanel.tsx`, `SeoPanel.tsx`, `MediaLibraryPage.tsx` có đủ liên kết chéo đúng quyền; `App.tsx` nối `contactContent`, `ThemesPanel` mode, `__("Categories")` / `__("Tags")` trên `cptTaxonomies`, `#/content-structure/content-types`; `TaxonomyDefinitionsPage.tsx` & `TaxonomyPage.tsx` đồng bộ `#/content-structure/taxonomies` và gate `admin &&`.
  - [ ] `npm --prefix cms/admin run typecheck` và `python3 cms/scripts/run-tests.py --check` xanh, không file nào vượt ngưỡng LOC.
- **Rollback:** `git revert` commit `S02` trên nhánh `feat/admin-sidebar-ia`.

### Slice S03: Gettext L10n (`.pot`/`.po`), E2E Suite & Documentation Sync

- **Dependencies:** `S02`.
- **File Ownership:** `cms/l10n/opendash.pot`, `cms/l10n/vi.po`, `cms/scripts/revisions_autosave_e2e.py`, `cms/scripts/admin_sidebar_ia_e2e.py`, `docs/product/{ADMIN_NAVIGATION_AND_EDIT_SCREEN,PRODUCT_REQUIREMENTS}.md`, `cms/admin/src/shell/README.md`, `docs/designs/SITEMAP.md`.
- **Acceptance Criteria:**
  - [ ] Cập nhật `cms/l10n/opendash.pot` và `cms/l10n/vi.po` đủ 100% khóa UI mới/sửa; `node cms/scripts/check-i18n.mjs`, `--l10n-extract` / `--l10n-compile --pot` và `python3 cms/scripts/run-tests.py --group i18n` đạt `PASS`.
  - [ ] Cập nhật `cms/scripts/revisions_autosave_e2e.py` phù hợp pure accordion; viết và chạy `cms/scripts/admin_sidebar_ia_e2e.py` trên Playwright Chrome kiểm chứng đầy đủ CUJ 1–5 (desktop + mobile drawer, Admin + Editor RBAC, `en` + `vi`, modifier-click & leave-guard).
  - [ ] Đồng bộ `docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`, `docs/product/PRODUCT_REQUIREMENTS.md`, `cms/admin/src/shell/README.md` và `docs/designs/SITEMAP.md`.
- **Rollback:** `git revert` commit `S03` trên nhánh `feat/admin-sidebar-ia`.

---

### Verification Plan

- **Tầng 0 (giữa các Slice trung gian `S01`–`S02` — không build binary đầy đủ, không chạy test Rust nặng):**
  ```bash
  npm --prefix cms/admin run typecheck
  python3 cms/scripts/run-tests.py --check
  ```
- **Tầng 1 (chạy 1 lần duy nhất sau khi hoàn tất `S03`, trước Bước 6 báo cáo Owner):**
  ```bash
  python3 cms/scripts/check-standards.py && python3 cms/scripts/check-standards.py --self-test
  python3 cms/scripts/check-plans.py && python3 cms/scripts/check-plans.py --self-test
  python3 .agents/scripts/check-agent-artifacts.py
  cargo fmt --manifest-path cms/Cargo.toml --all --check
  npm --prefix cms/admin run build
  npm --prefix cms/admin run test:nav
  node cms/admin/scripts/test-studio-ux.mjs
  node cms/scripts/check-i18n.mjs
  cargo run --manifest-path cms/Cargo.toml --bin opendash -- --l10n-extract cms/admin/src --out /tmp/admin-sidebar-ia.pot && cargo run --manifest-path cms/Cargo.toml --bin opendash -- --l10n-compile cms/l10n/vi.po --out /tmp/admin-sidebar-ia-vi.mo --pot cms/l10n/opendash.pot
  python3 cms/scripts/run-tests.py --group i18n
  python3 cms/scripts/run-tests.py --changed
  python3 cms/scripts/admin_sidebar_ia_e2e.py
  python3 cms/scripts/manual_redirects_404_e2e.py
  python3 cms/scripts/revisions_autosave_e2e.py
  git diff --check
  ```
- **Kiểm tra Drift trước Bước 6:** Chạy 1 lượt `/egm-drift-report` gọn ngay trên worktree `feat/admin-sidebar-ia` đối chiếu đặc tả với diff thực tế.
- **Tầng 2 (theo đợt release):** Chạy `python3 cms/scripts/run-tests.py --group all --release` ở đợt gộp tính năng khi có yêu cầu.
