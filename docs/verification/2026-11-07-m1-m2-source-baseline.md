# Kiểm chứng nguồn M1/M2 trước khi nhập mã

Ngày 07/11/2026, Windows. Workspace đích có mốc scaffold `0ec91ec`. Nguồn do chủ dự án chỉ định là thư mục `opendash/` cùng cấp với `opendash-cms/`.

## Phiên bản đã chạy

- Nhánh nguồn `main`, HEAD `7a519d031f25b8307180b8e9d776473ada042e98`. Tại thời điểm đọc, nhánh đi trước ref local `origin/main` 4 commit.
- Index trống thay đổi staged. Có 35 file tracked sửa hoặc xóa và 53 file untracked. Cây làm việc đã tách Rust sang hai crate và chia lại module Admin.
- Chạy trên bản sao của **cây làm việc**, gồm các file còn tồn tại từ `git ls-files -co --exclude-standard`. Loại đường dẫn trùng, bỏ build output và file bị ignore. Bản sao có SHA-256 `4a6efb825a11b6dcb5d6b505c8a556a0aa53bf6c9f5a187d41e298ee73e8b091`, tính lần lượt theo tên đường dẫn Git tăng dần, mỗi mục gồm tên UTF-8, byte NUL và 32 byte SHA-256 của nội dung.
- Có ba migration `0001_builder_slice.sql`, `0002_auth.sql`, `0003_plugins.sql`. Fixture `fixtures/baseline_0001.sqlite` có SHA-256 `49d8b9dfaea544e68513d70bd0e39039b69ffa6709caffee01ab98ed38a38a45`.

## Lệnh đã chạy từ gốc bản sao

| Lệnh | Kết quả |
|---|---|
| `cargo fmt --all --check` | PASS |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1` | PASS, build React Admin, plugin AssemblyScript mẫu và binary release với Cargo lockfile |
| `cargo test --release --workspace --locked` | PASS, 2 test core và 26 test server |
| `python scripts/windows_smoke.py --binary target/release/opendash.exe` | PASS |
| `python scripts/windows_auth_smoke.py --binary target/release/opendash.exe` | PASS |
| `python scripts/windows_distribution_smoke.py --binary target/release/opendash.exe` | PASS, gồm kiểm tra nâng cấp fixture và binary độc lập |
| `python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe` | PASS |

Kết quả áp dụng cho bản sao nguồn trên Windows. Khi [nhập mã](../plans/README.md), đối chiếu với cây làm việc có fingerprint trên, giữ chuỗi migrations và fixture, tích hợp quy ước `SitePaths` của workspace đích, rồi chạy lại bộ kiểm chứng tại đích. Linux và kiểm tra giao diện thủ công nằm ngoài lượt này.
