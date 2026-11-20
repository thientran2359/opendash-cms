# Admin Shell & Router (`cms/admin/src/shell/`)

Khung điều hướng chính, xác thực phiên và định tuyến hash URL của React Admin theo thiết kế 3 nhóm tại [Admin Navigation & Edit Screen](../../../../docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

- `AdminShell.tsx`: Thanh điều hướng 3 nhóm (`Nội dung`, `Giao diện`, `Tùy chỉnh`), sidebar thu gọn trên desktop, drawer trên mobile, breadcrumb và kiểm soát hiển thị theo vai trò (`Admin`/`Editor`).
- `router.ts`: Phân tích và chuẩn hóa hash URL (`#/dashboard`, `#/posts`, `#/pages`, `#/sections`, `#/cpt/:typeKey`, `#/appearance/*`, `#/taxonomy/*`, `#/settings/*`, `#/tools/*`, `#/builder`), ánh xạ các legacy alias và `isRouteAllowed`.
- `AuthPanel.tsx`: Màn hình thiết lập Admin ban đầu (bootstrap) và đăng nhập/đăng nhập lại khi hết phiên (`401`).
- `MediaRoute.tsx`: Tích hợp tuyến Media Library vào khung `AdminShell`.
