# Development scripts

- `build.ps1`: từ gốc website, chạy `powershell -File cms/scripts/build.ps1`. Script cài frontend theo `cms/admin/package-lock.json`, build Admin rồi chạy `cargo build --locked -p opendash-server` từ bên trong `cms/`. Thêm `-Release` để tạo binary release.
- `check-health.ps1`: kiểm tra endpoint `/api/health` trong lúc server đang chạy trên máy local.

Scripts smoke cho Builder/Auth/Plugin trong tài liệu lịch sử thuộc repository nguồn, chưa được chuyển vào scaffold. Không gọi những script đó trước khi nhập source và fixtures gốc.
