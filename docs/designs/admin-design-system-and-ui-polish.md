# Admin Design System (Style A — Shadcn + Linear) & UI Polish

**Author:** Nova
**Status:** Verified
**Date:** 2026-11-21
**Approved By:** Owner (Style A direction & pure-frontend scope); Nova (delegated technical closure)
**Approval Evidence:** Owner approved Style A (Shadcn + Linear + Light Sidebar on Tailwind CSS v4) and authorized `/egm-draft` → Goldfish review → direct implementation on `agent/nova` in chat (2026-11-21).
**Goldfish Review:** 3-reviewer zero-context loop (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`) — all Blockers/Majors/Minors resolved in this revision (exact `.od-legacy-scope` matrix including `posts`/`pages` edit mode, 100% single `<main>` invariant across Login/`#/sections`/`#/appearance/library`, synchronized `@layer` order in `media-library.css`, scoped `captionId`, `#status` topbar visibility rule, unlayered CSS moved into `@layer components`, explicit `AdminPrimitives` contracts, and full Rust+Admin verification commands).
**Baseline:** `main` `f1979a3` (sau PR #436 / EGM #427)

## Overview

OpenDash Admin (`cms/admin/`) hiện đang vận hành trên 3 lớp CSS chồng chéo (`@layer legacy` dùng tag selector toàn cục trong `styles.css`, Tailwind CSS v4 tắt `preflight`, và file CSS viết tay riêng ở `media-library.css`). Hệ quả là hàng loạt lỗi hiển thị và phân cấp thị giác xuất hiện trên nhiều màn hình: nút sort bảng và nút phụ biến thành nút CTA xanh đậm, dropdown `<select>` vỡ layout, sidebar bị cắt ở `1280×800`, trùng 2–3 thẻ `<main>` trong DOM, và lặp tiêu đề/breadcrumb.

Đặc tả này thiết lập một **Admin Style Guide thống nhất theo Phong cách A (Shadcn Minimal + Linear Precision + Sidebar sáng)** chạy thuần **Tailwind CSS v4** đã cài sẵn (`tailwindcss: 4.1.17`, không thêm dependency), cô lập triệt để CSS legacy để không làm hỏng Visual Builder hay các màn hình đang được triển khai song song, và đánh bóng các màn hình Admin cốt lõi tách biệt hoàn toàn với chuỗi `#427 → #431` (Harbor & Aster).

## Goals

1. **Thiết lập Design Tokens & Base CSS sạch trên Tailwind v4:**
   - Khai báo bộ token màu sáng trung tính (`canvas`, `surface`, `sidebar`, `border`, `text`, `muted`, `accent`, `danger`, `success`) bằng `@theme` trong `cms/admin/src/styles.css`.
   - Cô lập toàn bộ rule tag toàn cục cũ (`button`, `input`, `select`, `textarea`, `h1`, `h2`) trong `@layer legacy` vào phạm vi `.od-legacy-scope` (giữ nguyên biến `:root` cũ để panel legacy không vỡ), đồng thời thiết lập `@layer base` sạch (`button { background: transparent; color: inherit; }`, `a { text-decoration: none; }`, `accent-color` + `color-scheme: light`) và đồng bộ thứ tự `@layer` giữa `styles.css` và `media-library.css`.
2. **Cung cấp bộ UI Primitives tái sử dụng nhẹ (`AdminPrimitives.tsx`):**
   - `AdminButton` (`primary | secondary | ghost | danger`, `sm | md`, mặc định `type="button"`), `AdminBadge` (`neutral | success | warning | danger`), `AdminAlert` (`info | success | warning | error`).
3. **Chuẩn hóa Shell, Sidebar, Accessibility & Auth (Slice S01):**
   - Đảm bảo **duy nhất 1 thẻ `<main>`** tồn tại trong DOM trên **100% route Admin** (kể cả Login `!user`, `#/sections`, `#/appearance/library`, `#/media`).
   - Chuyển Sidebar sang **Phong cách A** (nền sáng `#f8fafc`, viền phải `1px solid #e4e4e7`, mật độ gọn để cả 3 nhóm `Nội dung` / `Giao diện` / `Tùy chỉnh` hiển thị đủ ở `1280×800`, bỏ gạch chân submenu, di chuyển các rule sidebar nằm ngoài layer vào `@layer components` với độ tương phản đạt WCAG AA, khử trùng lặp breadcrumb `Giao diện / Giao diện`).
   - Sửa phân cấp nút và trạng thái thông báo trên `AuthPanel` (`Đăng nhập` là primary, `Thiết lập Admin` là secondary; nhắc đăng nhập dùng màu trung tính thay vì xanh thành công, đồng thời giữ ẩn trạng thái không lỗi trên `.od-topbar`).
   - Bổ sung khối lối tắt tới các CPT tùy chỉnh ngay trên trang Tổng quan (tận dụng `cptTypes` và `navigate` sẵn có trong `AdminShell`, không sửa `App.tsx` hay API).
4. **Đánh bóng 4 màn hình Admin cốt lõi (Slice S02):**
   - `EntryListPage`: Sửa vỡ `<select>`, bỏ tiêu đề lặp và gom gọn bộ lọc/bulk để thấy nhiều dòng bảng ngay above-the-fold ở `1280×800`, sửa nút sort cột theo chuẩn `sortable-table` (`aria-sort` chỉ đặt trên `<th>` đang sort, nút ghost gọn, container `role="region"` có `aria-labelledby` theo `typeKey` cuộn ngang bàn phím được), dùng `AdminBadge` không rớt dòng cho trạng thái.
   - `MediaLibraryPage` + `media-library.css`: Bỏ `<main>` lồng nhau, bổ sung CSS cho `.od-media-primary`, `.od-media-filters`, `.od-media-video-placeholder`, cập nhật toàn bộ nhãn tiếng Việt chính xác cho cả ảnh và video MP4 (`Thư viện media`, `Chi tiết tệp`, `tệp đã tải`, `Mở tệp gốc trong tab mới`), đồng bộ token Style A và panel chi tiết sticky trên desktop.
   - `GeneralSettingsPanel`: Sửa đảo ngược phân cấp nút (`Lưu cài đặt` / `Lưu cài đặt công khai` là primary, `Tải lại từ máy chủ` / `Tải lại cài đặt công khai` là secondary, giữ nguyên nhãn và các nút retry), căn thẳng độ rộng khối `Ví dụ giờ địa phương` (`max-w-xl`) với các ô nhập liệu phía trên.
   - `DashboardPage`: Đồng bộ card thống kê theo Style A, dùng `AdminButton variant="secondary"` cho `Làm mới thống kê` / `Thử lại` và `AdminAlert` cho thông báo lỗi.

## Non-Goals

- Không thay đổi bất kỳ mã nguồn Rust (`cms/crates/`), SQLite migrations, hay HTTP API (`cms/admin/src/api/*`) nào.
- Không thêm dependency npm mới; giữ nguyên `cms/admin/package.json` và `package-lock.json`.
- **Không chạm** các file thuộc phạm vi `#427 → #431` để đảm bảo 0 xung đột merge với Harbor & Aster: `cms/admin/src/builder/*`, `cms/admin/src/features/themes/*`, `cms/admin/src/features/library/*`, `cms/admin/src/features/editor/SingleEditWorkspace.tsx`, `cms/admin/src/features/sections/SectionsPage.tsx`, `cms/admin/src/App.tsx`. Các màn hình chưa migrate tiếp tục được bọc trong `.od-legacy-scope` để giữ nguyên 100% giao diện hiện tại.
- Không thêm Dark Mode toggle hay API tìm kiếm Media toàn thư viện ở phía server trong phạm vi đặc tả này.

## Technical Plan

### 1. Kiến trúc CSS Layer & Cô lập Legacy (`cms/admin/src/styles.css` + `media-library.css`)

```css
@layer theme, base, legacy, components, utilities;
@import "tailwindcss/theme.css" layer(theme);
@import "tailwindcss/utilities.css" layer(utilities);
```

- Đồng bộ dòng `@layer theme, base, legacy, components, utilities;` ở cả `cms/admin/src/styles.css` và `cms/admin/src/features/media/media-library.css` để thứ tự bundler không bao giờ đẩy `base` ra sau `utilities`.
- **`@theme` tokens (Style A — Shadcn + Linear):**
  - `--color-od-canvas: #fafafa` (nền vùng làm việc)
  - `--color-od-surface: #ffffff` (nền card/bảng/control)
  - `--color-od-sidebar: #f8fafc` (nền sidebar sáng)
  - `--color-od-border: #e4e4e7` (viền mảnh `1px`)
  - `--color-od-border-strong: #d4d4d8` (viền control/hover)
  - `--color-od-text: #09090b` (chữ chính)
  - `--color-od-muted: #52525b` (chữ phụ/metadata, đạt contrast ≥ 4.5:1 trên `#fafafa` và `#f8fafc`)
  - `--color-od-accent: #2563eb` và `--color-od-accent-hover: #1d4ed8` (1 màu nhấn duy nhất, tương phản > 4.5:1 trên nền trắng)
  - `--color-od-danger: #dc2626`, `--color-od-success: #16a34a`
- **`@layer base`:**
  - Đặt `*, *::before, *::after { box-sizing: border-box; }`, `body { margin: 0; padding: 0; font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: var(--color-od-canvas); color: var(--color-od-text); }`, `.od-root { accent-color: var(--color-od-accent); color-scheme: light; }` (theo `brand-consistent-forms`).
  - Reset an toàn cho `button { font: inherit; background: transparent; color: inherit; border: 0; cursor: pointer; }`, `button:disabled { cursor: not-allowed; }`, `a { color: inherit; text-decoration: none; }`, `input, select, textarea { font: inherit; color: inherit; }`.
- **`@layer legacy` scoped:**
  - Giữ nguyên `:root { --color-bg: ...; --color-surface: ...; ... }` trong `@layer legacy` để mọi panel legacy vẫn resolve được `var(--color-*)`.
  - Mọi selector tag toàn cục (`input`, `select`, `textarea`, `button`, `h1`, `h2`) và utility legacy (`button.btn-secondary`, `button.btn-danger`, `.panel-row`, `.form-group`) chỉ áp dụng bên trong `.od-legacy-scope` (ví dụ `.od-legacy-scope button { ... }`).
  - Di chuyển toàn bộ rule sidebar ở cuối `styles.css` (`.od-nav-group`, `.od-nav-group-title`, `.od-nav-coming-soon`, `.od-nav-link:disabled`, `.od-nav-sub-disabled`) vào trong `@layer components` và chuyển màu sang tông chữ tối trên nền sáng (`#52525b` / `#71717a`).
  - Gỡ bỏ các override ID cũ `.od-topbar #auth-logout`, `.od-auth-card #auth-login`, `.od-auth-card #auth-setup` khỏi `@layer components` để `AdminButton` quản lý thống nhất kích thước/bo góc/hover; đổi `.od-topbar #status.success { display: none; }` thành `.od-topbar #status:not(.error) { display: none; }`.

### 2. Ma trận `.od-legacy-scope` & Single `<main>` Landmark (`AdminShell.tsx`)

- **Single `<main>` invariant (đúng `1` thẻ `<main>` trên 100% route):**
  - Khi `!user` (Login): `.od-auth-layout` là `<main id="auth-main">`; `.od-page-container` bên trong `.od-layout.hidden` là `<div id="admin-main">` (không phải `<main>`).
  - Khi đã đăng nhập (`user`):
    - Các route `sections` (`SectionsPage.tsx:87`) và `themes` với `sub === 'library'` (`TemplateLibraryPage.tsx:85`) tự render thẻ `<main>` riêng và nằm ngoài File Ownership; với 2 route này (và `media` nếu S01 đứng riêng trước S02), `.od-page-container` render là `<div id="admin-main">`.
    - Mọi route còn lại: `.od-page-container` render là `<main id="admin-main">`, còn toàn bộ wrapper con (`#dashboard-main`, `#builder-main`, `#content-main`, và các thẻ card route) đổi từ `<main>` sang `<div>` giữ nguyên `id` và `className`.
- **Ma trận `.od-legacy-scope` trên `AdminShell.tsx`:**
  - **Không dùng `.od-legacy-scope` (Style A hiện đại):** `dashboard`, `posts` khi `route.sub === 'list'`, `pages` khi `route.sub === 'list'`, `cpt` khi `route.sub === 'list'`, `media` (sau S02), `settings` khi `route.sub === 'general'`.
  - **Bọc `.od-legacy-scope` (giữ nguyên 100% style cũ, 0 hồi quy):**
    - `posts` / `pages` / `cpt` khi `route.sub !== 'list'` (`new` / `edit` — bảo vệ `SingleEditWorkspace.tsx` và `MediaPicker.tsx`).
    - `settings` khi `route.sub !== 'general'` (`permalinks`, `seo`, `contact`, `smtp`, `content-types`).
    - `builder`, `themes` (`themes`, `menus`, `options`, `library`), `taxonomy`, `plugins`, `users`, `profile`, `tools`, `sections`.
- **Breadcrumb deduplication:** `rawParts.filter((part, idx) => idx === 0 || part !== rawParts[idx - 1]).join(' / ')` (rút gọn `Quản trị / Giao diện / Giao diện / Themes` → `Quản trị / Giao diện / Themes`).
- **Sidebar Style A:** Rộng `244px` trên desktop (`>= 1024px`), nền `#f8fafc`, viền phải `1px solid #e4e4e7`, header thương hiệu gọn (`px-4 py-3.5`), item `px-2.5 py-1.5 text-[13px]`, nhóm ngăn cách `mt-4`, submenu không gạch chân (`no-underline`), active pill `bg-zinc-200/80 text-zinc-950 font-medium`, hover `hover:bg-zinc-200/50`.
- **Dashboard CPT Shortcuts:** Trên `route.view === 'dashboard'`, lọc `cptTypes` loại bỏ `post`, `page`, `section` (giống `buildMenuTree`). Nếu danh sách rỗng thì không render; nếu có ≥ 1 CPT tùy chỉnh thì render khối `data-testid="dashboard-cpt-shortcuts"` gồm các card Style A liên kết tới `#/cpt/${key}` (`Danh sách`) và `#/cpt/${key}/new` (`+ Thêm mới`).

### 3. UI Primitives (`cms/admin/src/components/AdminPrimitives.tsx`)

```ts
export type AdminButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';
export type AdminButtonSize = 'sm' | 'md';
export interface AdminButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: AdminButtonVariant; // default: 'secondary'
  size?: AdminButtonSize;       // default: 'md'
}

export type AdminBadgeTone = 'neutral' | 'success' | 'warning' | 'danger';
export interface AdminBadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  tone?: AdminBadgeTone; // default: 'neutral'
}

export type AdminAlertTone = 'info' | 'success' | 'warning' | 'error';
export interface AdminAlertProps extends React.HTMLAttributes<HTMLDivElement> {
  tone?: AdminAlertTone; // default: 'info'; default role: tone === 'error' ? 'alert' : 'status'
}
```

- `AdminButton`: mặc định `type = 'button'` (các nút submit như `#auth-login` và `Lưu cài đặt` truyền tường minh `type="submit"`). Classes:
  - Base: `inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-od-accent disabled:pointer-events-none disabled:opacity-50`
  - `size`: `sm` = `h-8 px-2.5 text-xs`, `md` = `h-9 px-3.5 text-sm`
  - `variant`: `primary` = `bg-od-accent text-white hover:bg-od-accent-hover`, `secondary` = `border border-zinc-300 bg-white text-zinc-800 hover:bg-zinc-50`, `ghost` = `bg-transparent text-zinc-700 hover:bg-zinc-100 hover:text-zinc-950`, `danger` = `border border-rose-200 bg-rose-50 text-rose-700 hover:bg-rose-100`
- `AdminBadge`: `inline-flex items-center whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset` + màu theo `tone` (`neutral`, `success`, `warning`, `danger`).
- `AdminAlert`: `rounded-lg border px-3.5 py-2.5 text-sm` + màu theo `tone` (`info`: sky/blue, `success`: emerald, `warning`: amber, `error`: rose); sử dụng tại `GeneralSettingsPanel`, `EntryListPage`, `DashboardPage`.

### 4. Sortable Table & Compact Filter Bar (`EntryListPage.tsx`)

- Gỡ bỏ khối `<header>` lặp tiêu đề/eyebrow (`EntryListPage.tsx:449–456`); gom các tab trạng thái (`Tất cả`, `Bản nháp`, `Đã xuất bản`) cùng nút `+ Thêm mới` (`AdminButton variant="primary" size="sm"`) và `Tải lại` (`AdminButton variant="secondary" size="sm"`) lên cùng 1 thanh toolbar đầu trang.
- Gom tìm kiếm và các bộ lọc (`Danh mục`, `Thẻ`, `Taxonomy tùy chỉnh`, `Tháng cập nhật`, `Mỗi trang`) vào 1 card trắng gọn (`p-3.5`) với các control `h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900` (xóa hoàn toàn lỗi vỡ caret `<select>`).
- Nút bulk action: `Xuất bản đã chọn` (`secondary`/`sm`), `Chuyển về nháp` (`secondary`/`sm`), `Xóa đã chọn` (`danger`/`sm`).
- Bảng dữ liệu tuân thủ `sortable-table`:
  - `const captionId = \`entry-table-caption-\${typeKey}\`;` gán cho `<caption id={captionId} className="sr-only">`.
  - Container cuộn ngang: `role="region" aria-labelledby={captionId} tabIndex={0} className="... overflow-x-auto"`.
  - `<th>` của `Tiêu đề` và `Ngày cập nhật`: đặt `aria-sort={isSorted ? (order === 'asc' ? 'ascending' : 'descending') : undefined}` trên `<th>` (không đặt trên `<button>`, không đặt `aria-sort="none"`).
  - `<button type="button" data-testid="sort-title" | "sort-updated">` dùng kiểu ghost `inline-flex min-h-6 w-full items-center gap-1 text-left text-xs font-semibold uppercase tracking-wider text-zinc-600 hover:text-zinc-950`, bỏ `aria-label` dư thừa (giữ tên cột làm label tự nhiên, mũi tên `↑`/`↓` trong `<span aria-hidden="true">`).
  - Giữ nguyên toàn bộ `data-testid` / `data-*` hiện có (`entry-list-${typeKey}`, `entry-table`, `entry-table-page-info`, `provider-action-result`, `sort-title`, `sort-updated`, `data-entry-id`, `data-bulk-id`).

## Alternatives Considered

1. **Bật toàn bộ `@import "tailwindcss/preflight.css"` cho cả ứng dụng:** Bị loại vì làm vỡ các panel legacy (`BuilderWorkspace.tsx`, `ThemesPanel.tsx`, `SingleEditWorkspace.tsx`). Cô lập `@layer legacy` vào `.od-legacy-scope` + `@layer base` tối giản an toàn hơn nhiều và cho phép migrate từng phần không hồi quy.
2. **Cài thêm thư viện UI ngoài (Radix UI / Headless UI / Lucide):** Bị loại theo ràng buộc `TECH_STACK.md` (giữ dependency đã pin, không phình bundle).
3. **Đưa luôn `ThemesPanel.tsx` và `BuilderWorkspace.tsx` vào đợt này:** Bị loại để đảm bảo 0 xung đột file với `#427 → #431` của Harbor & Aster.

## Detailed Implementation Plan

### Slice S01: Design Tokens, Legacy CSS Isolation, Shared Primitives & Admin Shell / Auth

- **Dependencies:** None (trực tiếp trên `origin/main` `f1979a3`).
- **File Ownership:**
  - `cms/admin/src/styles.css` (modify)
  - `cms/admin/src/features/media/media-library.css` (modify — đồng bộ dòng 1 `@layer` order ngay từ S01)
  - `cms/admin/src/components/AdminPrimitives.tsx` (create)
  - `cms/admin/src/shell/AdminShell.tsx` (modify)
  - `cms/admin/src/shell/AuthPanel.tsx` (modify)
- **Changes:**
  1. `cms/admin/src/styles.css` & `cms/admin/src/features/media/media-library.css` (L1):
     - Đồng bộ `@layer theme, base, legacy, components, utilities;`.
     - Thêm `@theme` tokens Style A, `@layer base` resets (`accent-color`, `color-scheme: light`, `button`, `a`, `input`, `select`, `textarea`).
     - Scope các tag selector và utility legacy trong `@layer legacy` vào `.od-legacy-scope`, giữ nguyên `:root` vars.
     - Di chuyển L690–716 vào `@layer components`, chuyển toàn bộ Shell/Sidebar/Topbar/Auth/Card sang Style A sáng gọn (`min-height: 64px` topbar, sidebar `244px` `#f8fafc`).
     - Đổi `.od-topbar #status.success { display: none; }` → `.od-topbar #status:not(.error) { display: none; }` và gỡ ID override cũ của `#auth-login`/`#auth-setup`/`#auth-logout`.
  2. `cms/admin/src/components/AdminPrimitives.tsx`:
     - Triển khai `AdminButton`, `AdminBadge`, `AdminAlert` đúng chữ ký ở Technical Plan §3.
  3. `cms/admin/src/shell/AdminShell.tsx`:
     - Áp dụng Single `<main>` invariant (Login + mọi route đã đăng nhập).
     - Áp dụng ma trận `.od-legacy-scope` chính xác (bao gồm `posts`/`pages`/`cpt` khi `route.sub !== 'list'`, `settings` khi `route.sub !== 'general'`).
     - Khử lặp breadcrumb liền kề, cập nhật sidebar sáng Style A (không gạch chân submenu, gọn vừa `1280×800`), thay `.od-welcome` bằng header sáng gọn và thêm khối `data-testid="dashboard-cpt-shortcuts"`.
  4. `cms/admin/src/shell/AuthPanel.tsx`:
     - Thay `.panel-row` bằng Tailwind flex utilities; thêm border/padding Style A cho `TextInput`; dùng `AdminButton` cho `Đăng nhập` (`primary`, `type="submit"`), `Thiết lập Admin` (`secondary`), `Đăng xuất` (`secondary`, `sm`); giữ nguyên toàn bộ `id` và `role="status"`.
- **Acceptance Criteria:**
  - `npm run typecheck` và `npm run build` trong `cms/admin/` xanh.
  - Trên Login (`!user`) và mọi route Admin (`#/dashboard`, `#/posts`, `#/media`, `#/sections`, `#/appearance/themes`, `#/appearance/library`, `#/settings/general`, `#/builder`), `document.querySelectorAll('main').length === 1`.
  - Ở `1280×800`, nhóm `Tùy chỉnh` trong sidebar sáng hiển thị đầy đủ, submenu không gạch chân, breadcrumb `#/appearance/themes` hiển thị `Quản trị / Giao diện / Themes`.
  - Builder (`#/builder`), Themes (`#/appearance/themes`), và Posts/Pages editor (`#/posts/new`) giữ nguyên styling qua `.od-legacy-scope`, 0 lỗi console JS.
- **Rollback:** `git revert` commit của Slice S01.

### Slice S02: Core Admin Screens Polish (`EntryListPage`, `MediaLibraryPage`, `GeneralSettingsPanel`, `DashboardPage`)

- **Dependencies:** Slice S01 (triển khai nối tiếp ngay trên cùng nhánh `agent/nova`).
- **File Ownership:**
  - `cms/admin/src/features/content/EntryListPage.tsx` (modify)
  - `cms/admin/src/features/media/MediaLibraryPage.tsx` (modify)
  - `cms/admin/src/features/media/media-library.css` (modify)
  - `cms/admin/src/features/system/GeneralSettingsPanel.tsx` (modify)
  - `cms/admin/src/features/dashboard/DashboardPage.tsx` (modify)
- **Changes:**
  1. `cms/admin/src/features/content/EntryListPage.tsx`:
     - Gọn hóa toolbar + filter card Style A, `<select>`/`<input>` đồng nhất `h-9` không vỡ caret, `sortable-table` (`aria-sort` trên `<th>`, `captionId` theo `typeKey`, container `role="region"` `tabIndex={0}`), `AdminBadge` trạng thái, `AdminButton` cho hành động/bulk/phân trang, `AdminAlert` cho thông báo lỗi, giữ nguyên mọi `data-testid`/`data-*`.
  2. `cms/admin/src/features/media/MediaLibraryPage.tsx` & `cms/admin/src/features/media/media-library.css`:
     - Đổi `<main className="od-media-library">` → `<div className="od-media-library">` (đồng thời bỏ ngoại lệ `media` ở `AdminShell.tsx` nếu đã bật).
     - Cập nhật toàn bộ nhãn tiếng Việt nhất quán cho cả ảnh và video MP4 (`Thư viện media`, `Chi tiết tệp`, `tệp đã tải`, `Mở tệp gốc trong tab mới`, v.v.).
     - Bổ sung `.od-media-primary`, `.od-media-filters`, `.od-media-video-placeholder` trong `media-library.css`, đồng bộ token Style A và đặt `.od-media-details` `position: sticky; top: 76px;` trên desktop.
  3. `cms/admin/src/features/system/GeneralSettingsPanel.tsx`:
     - Dùng `AdminButton` (`primary` cho `Lưu cài đặt` / `Lưu cài đặt công khai` và các nút retry; `secondary` cho `Tải lại từ máy chủ` / `Tải lại cài đặt công khai`), `AdminAlert` cho error/notice/missing-origin, căn `Ví dụ giờ địa phương` `max-w-xl`, giữ nguyên toàn bộ `data-testid` (`general-settings`, `timezone-preview`, `public-settings`, `missing-origin-prompt`).
  4. `cms/admin/src/features/dashboard/DashboardPage.tsx`:
     - Dùng `AdminButton variant="secondary" size="sm"` cho `Làm mới thống kê` / `Thử lại`, `AdminAlert` cho lỗi, chuẩn hóa card số liệu Style A và giữ nguyên `data-dashboard-metric`.
- **Acceptance Criteria:**
  - `npm run typecheck` và `npm run build` trong `cms/admin/` xanh 100%.
  - Trên `#/posts` ở `1280×800`: `<select>` 1 dòng không vỡ caret; nút sort không tô xanh CTA và có `aria-sort` đúng trên `<th>`; hiển thị ≥ 5 dòng (hoặc toàn bộ dữ liệu hiện có) không cần cuộn; badge trạng thái và hành động không rớt dòng bất thường.
  - Trên `#/media`: đúng 1 thẻ `<main>`, nút lọc đang chọn (`.od-media-primary`) nổi bật rõ, nhãn nhất quán cho cả ảnh và video MP4, panel chi tiết sticky trên desktop.
  - Trên `#/settings/general`: `Lưu cài đặt` nổi bật hơn `Tải lại từ máy chủ`, khối `Ví dụ giờ địa phương` thẳng mép `max-w-xl`.
  - Trên `#/dashboard`: card thống kê + lối tắt CPT hiển thị gọn theo Style A.
- **Rollback:** `git revert` commit của Slice S02.

### Verification Plan

1. **Build, Standards & Rust Workspace Verification (`npm run build` chạy trước `cargo build`/`cargo test` vì `opendash-server` nhúng `cms/admin/dist/`):**
   ```powershell
   Push-Location cms/admin; npm run typecheck; npm run build; Pop-Location
   python cms/scripts/check-standards.py
   python cms/scripts/check-standards.py --self-test
   python cms/scripts/check-plans.py
   python cms/scripts/check-plans.py --self-test
   python .agents/scripts/check-agent-artifacts.py
   cargo fmt --manifest-path cms/Cargo.toml --all --check
   cargo build --manifest-path cms/Cargo.toml --locked
   cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
   git diff --check
   ```
2. **Live Browser Verification (trên `http://127.0.0.1:18766/admin/`):**
   - Kiểm tra trực quan bằng trình duyệt ở `1280×800` và `390×844` trên: Login, Dashboard (`#/dashboard`), Posts (`#/posts`), Media (`#/media`), Settings (`#/settings/general`), Themes (`#/appearance/themes`), Builder (`#/builder`).
   - Xác nhận: 0 lỗi JS console, `document.querySelectorAll('main').length === 1` trên mọi route, sidebar sáng gọn ở `1280×800`, không hồi quy trên Builder/Themes.
