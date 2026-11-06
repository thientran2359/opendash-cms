# SitePaths trên Windows · 07/11/2026

Phạm vi gồm [bộ chọn đường dẫn](../../cms/crates/opendash-server/src/paths.rs) và việc [server mở SQLite](../../cms/crates/opendash-server/src/main.rs). Tiêu chí nghiệm thu là bản cài mới tạo SQLite cạnh binary khi khởi động từ thư mục khác và các biến cấu hình vẫn chọn được site hoặc tệp database riêng.

| Lệnh chạy từ gốc workspace | Kết quả |
|---|---|
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | PASS, 4 test core và 5 test module paths |
| `cargo build --manifest-path cms/Cargo.toml --locked -p opendash-server` | PASS, binary debug trên Windows |

**Smoke qua tiến trình thật.** Chép binary vừa build vào thư mục tạm `install/` mới, khởi động từ `other/` có `cms/Cargo.toml` để kiểm tra cả trường hợp thư mục gọi trông giống workspace. Mỗi ca dùng cổng localhost riêng. Cả ba ca trả HTTP 200 cho `/api/health`, mở lại được SQLite ở vị trí mong đợi với `PRAGMA quick_check = ok`, và giữ thư mục gọi không có database.

| Cấu hình | Tệp thực tế trong vùng tạm |
|---|---|
| Bỏ cả hai biến cấu hình | `install/database/opendash.sqlite` |
| Đặt `OPENDASH_SITE_ROOT` tới `site/` | `site/database/opendash.sqlite` |
| Đặt cả gốc site và `OPENDASH_DB_PATH` tuyệt đối | `external/custom.sqlite` theo đường dẫn ghi đè |

Unit test kiểm tra quy ước build trong workspace, đường dẫn các thư mục website và lọc thư mục gói. Cài ZIP, upload, backup/restore, hoán đổi staging và chạy trên Linux nằm ngoài phạm vi.
