# Kiểm chứng bổ sung M5/M6 trên Windows (Entry Gate M7 — 2026-11-14)

Báo cáo bổ sung thực hiện theo [ADR-0014](../decisions/ADR-0014-m7-site-features.md#owner-directed-macos-deferral--2026-11-14) và [M7 Contract §9](../contracts/M7_SITE_FEATURES.md#windows-development-gate-and-deferred-macos) để xác nhận Windows development gate trước khi mở khóa triển khai M7 (`#202`–`#213`), trong khi kiểm chứng bản địa macOS đang được hoãn theo chỉ đạo chủ dự án.

## Môi trường & Nguồn kiểm chứng

- **Hệ điều hành:** Windows (`opendash-cms-win-ag`)
- **Nguồn đã pin:** `agent/nova` tại `c59d2f6ca5b2f4b2df0ef38cde4a77253c031af1` (đồng bộ `origin/main@7716aab`)
- **Binary M5/M6 kiểm chứng:** `cms/target/debug/opendash.exe`
  - **SHA256:** `83238A0181B8A3D3B3ACD88B7C1184D2D4AE44BB4768F0C44B173D2996A43CD9`
- **Binary M4 đối chiếu rollback (`a176414bba5b7b85f408137ef0e0b964c2329843`):** `opendash.exe` (build từ worktree `a176414` với `npm ci && npm run build` và `cargo build --locked`)
  - **SHA256:** `8FDE13B21C20F521D005BE55E704DF4CE3CB068461E1CECDFB07D1D8961A7CE7`

## Lệnh/Check đã thực hiện & Kết quả

| Hạng mục | Lệnh / Phạm vi | Kết quả | Chi tiết |
|---|---|---|---|
| Admin UI locked build | `npm --prefix cms/admin run build` | **PASS** | `tsc && vite build` (`1.98s`, `72 modules`, `dist/assets/index---28KVk9.js 337.91 kB`) |
| Rust locked build | `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** | Biên dịch `opendash-server` (`1m 06s`, exit `0`) |
| Toàn bộ workspace tests | `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | **PASS** | Exit `0`, toàn bộ unit tests và integration tests `m5_*`, `m6_s01_t01`..`m6_s08_t01` PASS (`0 failed`) |
| Rollback pre-M5 về binary M4 `a176414` | Script Python dựng `opendash.exe` tại `a176414`, tạo bài viết trên M4, chụp snapshot DB+files, nâng cấp lên M5/M6, ghi bài mới, khôi phục snapshot và chạy lại binary M4 `a176414` | **PASS** | `PRE_M5_SNAPSHOT_TAKEN=true`, `M5_M6_UPGRADE_VERIFIED=true`, `M4_SNAPSHOT_ROLLBACK_LIVE_SITE_VERIFIED=true` — site M4 khôi phục trả HTTP `200` trên `/` và giữ nguyên `M4 Baseline Post` |
| Trình duyệt thật Chrome M5/M6 | Playwright + Google Chrome (`headless=True`) trên `opendash.exe` live | **PASS** | `M5_M6_REAL_BROWSER_VERIFIED=true` (0 `pageerror`): setup/login, `401` unauth, `412` CAS stale `If-Match`, General Settings, Theme ZIP upload/activate, Builder Preview (public `/` không đổi) vs Save (public `/` cập nhật), Media upload, Plugin ZIP v2 install (`browser-suite`, `veto-guard`, `trap-guard`), Declarative UI & action (`browser-action-ok`), `content/before_save` veto (`browser veto hold`) & lưu thành công sau khi tắt veto, cô lập lỗi `unreachable` trap (public `/` vẫn `200`), disable/restart/bảo toàn/re-enable |
| Kiểm chứng bản địa macOS | `M5#157`, `M6#188` trên macOS | **NOT RUN** | Hoãn theo [ADR-0014](../decisions/ADR-0014-m7-site-features.md#owner-directed-macos-deferral--2026-11-14) |

## Kết luận gate

- **Windows development gate (ADR-0014 / M7 §9):** **PASS** trên Windows bản địa; không còn blocker bảo mật/dữ liệu mở trên Windows.
- **Giới hạn:** Kiểm chứng bản địa macOS là `NOT RUN`; `#157` và `#188` tiếp tục giữ mở cho đến khi hoàn tất nghiệm thu macOS trước khi phát hành V1.
