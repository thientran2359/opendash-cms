# Build và kiểm thử

`build.ps1` cài dependency theo hai lockfile npm, build Admin, plugin AssemblyScript mẫu, sau đó chạy Cargo với `--locked`. Dùng `-Release` cho binary phân phối. `check-health.ps1` kiểm tra SQLite qua server đang chạy.

Xem [lệnh chạy các bộ kiểm thử](../../README.md#kiểm-chứng-tại-workspace). Smoke test phân phối thử lỗi build trong workspace tạm; fixture lịch sử được sao chép trước khi nâng cấp.

`content_types_smoke.py` chạy được bằng Python 3 trên macOS/Windows sau khi build binary. Tại root: `python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash` (đổi đường dẫn binary tương ứng trên Windows). Script tạo website/database tạm, không dùng SQLite thực của dự án.

`content_entries_smoke.py` kiểm chứng M3.3 trên HTTP/SQLite thực (entry CRUD, field type validation, Editor/CSRF, slug, phiên bản If-Match, provider inactive và upgrade fixture). Tại root: `python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash`. Cũng dùng SQLite tạm.
