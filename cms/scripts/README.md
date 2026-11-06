# Build và kiểm thử Windows

`build.ps1` cài dependency theo hai lockfile npm, build Admin, plugin AssemblyScript mẫu, sau đó chạy Cargo với `--locked`. Dùng `-Release` cho binary phân phối. `check-health.ps1` kiểm tra SQLite qua server đang chạy.

Xem [lệnh chạy các bộ kiểm thử](../../README.md#kiểm-chứng-tại-workspace). Smoke test phân phối thử lỗi build trong workspace tạm; fixture lịch sử được sao chép trước khi nâng cấp.
