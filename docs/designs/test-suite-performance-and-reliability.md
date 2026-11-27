# Test Suite Performance & Reliability (Startup I/O, Domain Consolidation, Unit Parallelism)

**Status:** Approved  
**Date:** 2026-11-27  
**Author:** Elephant  
**Approved By:** Owner  
**Approval Evidence:** Owner direct approval in conversation 2026-11-27  
**Goldfish Review:** 3-core (egm-design, egm-codebase-details, egm-implementation-readiness) · 2026-11-27 · 0 Blocker  
**Components:** `opendash-server::{storage/mod.rs, storage/page/write.rs, storage/theme/write.rs, plugin/mod.rs, plugin/runtime/store_limits.rs, plugin/review_regressions/suite_02.rs, plugin/tests/suite_01.rs}`, `cms/crates/opendash-server/tests/`, `cms/scripts/run-tests.py`

---

## Overview

Bộ test Rust của OpenDash CMS chậm do ba nút thắt chính: (1) mỗi lần khởi động `opendash` trên `site_root` mới tốn ~`957 ms` vì `32 × F_FULLFSYNC` trên macOS APFS và SQLite WAL `synchronous=FULL`; (2) 99 file integration test phân mảnh buộc `cargo test` chạy tuần tự và link 99 executable; (3) `opendash-server` unit tests (`283` test) mất **`147.09s`** (~67% tổng thời gian `--group all`) do 24 `plugin::*` unit test tuần tự hóa trên một mutex toàn cục `PLUGIN_TEST_SERIAL`. Ngoài ra, nhánh `refactor/feature-first-modularization` còn 1 lỗi hồi quy khi tách thư mục `routes/<name>/` khiến `m8_s01_t02_writer_barrier` không tìm thấy file route.

---

## The Problem

Đo thực tế trên macOS (`cms/`):

1. **Startup I/O (~`957 ms` mỗi lần spawn `opendash`):**
   - `93/99` integration test tự spawn `CARGO_BIN_EXE_opendash` (`241` lần gọi `spawn_server*` / `Command::new`).
   - Thời gian khởi động `site_root` mới: `957 ms` (debug) / `974 ms` (release) so với `50–101 ms` khi restart trên `site_root` đã khởi tạo.
   - Nguyên nhân: `write_files_durably` trong `storage/theme/write.rs` gọi `sync_all()` (`F_FULLFSYNC` trên macOS) cho từng file trong 32 file Starter Theme (~`745 ms`), `storage/mod.rs` dùng SQLite WAL mặc định `synchronous=FULL`, và `ensure_embedded_core_blocks` chạy 14 auto-commit rời rạc.
2. **99 binary integration test phân mảnh:**
   - 80/99 binary chỉ có ≤ 2 test (`cargo test` chạy tuần tự giữa các binary). Nhóm `i18n` (7 binary / 18 test) mất `19.95s` trước khi gộp.
3. **Unit test `opendash-server` tuần tự hóa quá mức (`147.09s`):**
   - `24` test trong `plugin/tests/suite_01.rs` và `plugin/review_regressions/suite_0{1,2}.rs` đều giữ `PLUGIN_TEST_SERIAL.lock().await`, dù phần lớn chỉ dùng `sqlite::memory:` độc lập và không chạm biến toàn cục.
4. **Hồi quy tách module trên `refactor/feature-first-modularization`:**
   - `store_limits.rs:1072` dùng đường dẫn cũ `plugin::tests::tests::test_worker_entry` khiến `suite_02.rs` treo vô hạn 24 phút (đã vá trên nhánh).
   - `m8_s01_t02_writer_barrier::audited_writer_inventory_covers_every_mutating_route_handler` đọc `routes/{entries,operations,plugin,themes}.rs` thay vì các file trong `routes/<name>/*.rs`.

---

## Goals

- Giảm thời gian khởi động `opendash` trên `site_root` mới xuống ≤ `350 ms` (và `< 100 ms` với pre-seeded template ở S03) mà **không** phá vỡ A-DURABILITY.
- Hợp nhất 98 integration test thành 7 domain binary (`theme`, `builder`, `content`, `plugin`, `i18n`, `site_media_seo`, `ops`) + giữ `m6_s04_t01_egress_ssrf` độc lập (cô lập `HTTP_PROXY`), giữ `run-tests.py --changed` chính xác.
- Khôi phục 100% pass cho `--group all` (sửa `m8_s01_t02_writer_barrier` cho cấu trúc `routes/<name>/*.rs`).
- Giảm thời gian chạy `opendash-server` unit tests từ `147.09s` xuống `< 25s` bằng cách thu hẹp `PLUGIN_TEST_SERIAL` về đúng các test chạm trạng thái toàn cục.

## Non-Goals

- Không tắt `fsync` hoặc bỏ barrier `.complete` (`F_FULLFSYNC`) trong production.
- Không nới lỏng hoặc bỏ qua assertion trong bất kỳ test nào (kể cả `AdmittedMutation` audit).
- Không thay đổi kiến trúc 1 file thực thi (`opendash`) hay `Wasmi =2.0.0`.

---

## Critical User Journeys (CUJs)

- **CUJ-1 (Incremental `--changed`):** *Given* developer sửa một file trong `cms/crates/opendash-server/src/i18n.rs` hoặc `tests/i18n/*`, *When* chạy `python3 cms/scripts/run-tests.py --changed`, *Then* chỉ nhóm `i18n` được biên dịch/chạy và hoàn tất trong `< 10s`.
- **CUJ-2 (Full Workspace `--group all`):** *Given* developer hoặc CI chạy `python3 cms/scripts/run-tests.py --group all`, *When* toàn bộ unit + integration tests chạy, *Then* 100% test pass (bao gồm `ops::m8_s01_t02_writer_barrier`), không deadlock, tổng thời gian warm `< 60s`.
- **CUJ-3 (A-DURABILITY & M8 Writer Admission):** *Given* server chạy thực tế trên macOS/Linux/Windows, *When* cài đặt theme v3 hoặc gọi mutating route, *Then* `.complete` vẫn có barrier `sync_all()` và mọi mutating handler vẫn được kiểm tra `AdmittedMutation`.

---

## Technical Plan (Architecture)

### Đã triển khai trên nhánh (Wave 1 & Wave 2)
- **Wave 1 (Startup I/O):**
  - `cms/crates/opendash-server/src/storage/mod.rs`: đặt `.synchronous(SqliteSynchronous::Normal)` trên `SqliteConnectOptions` (khuyến nghị chuẩn của SQLite cho `WAL`).
  - `cms/crates/opendash-server/src/storage/page/write.rs`: bọc `ensure_embedded_core_blocks` trong `BEGIN IMMEDIATE ... COMMIT`.
  - `cms/crates/opendash-server/src/storage/theme/write.rs`: thêm `sync_staged_file` dùng `libc::fsync` trên macOS (`sync_data()` trên non-macOS) cho từng file staging; giữ `f.sync_all()` trên `.complete` trong `storage/theme/read.rs`.
- **Wave 2 (Domain Binary Consolidation):**
  - Gộp 98 file `tests/*.rs` vào 7 thư mục con + 7 entrypoint `tests/<group>.rs` dùng `#[path = "..."]`; giữ `tests/plugin_egress_ssrf.rs` độc lập; cập nhật `cms/scripts/run-tests.py` hỗ trợ nested test routing.

### Các Slice tiếp theo (S01–S03)
- **Slice S01:** Cập nhật `m8_s01_t02_writer_barrier.rs` để đọc cả file đơn `routes/<stem>.rs` lẫn toàn bộ `*.rs` (sắp xếp theo tên) trong thư mục `routes/<stem>/` (cho `entries/`, `operations/`, `plugin/`, `themes/`), đảm bảo không bỏ sót handler nào; ghi nhận bản vá `store_limits.rs:1072` và guard chống treo trong `suite_02.rs`.
- **Slice S02:** Kiểm toán 24 vị trí dùng `PLUGIN_TEST_SERIAL` trong `plugin/tests/suite_01.rs` và `plugin/review_regressions/`; chỉ giữ `PLUGIN_TEST_SERIAL` cho các test chạm biến toàn cục (`TEST_STAGE_WRITE_PAUSE`, `TEST_STAGE_WRITE_BARRIER_HIT`) hoặc file tạm dùng chung, đổi tên file tạm sang token ngẫu nhiên thay vì chỉ `pid`.
- **Slice S03 (Tùy chọn):** Thêm helper `tests/common/mod.rs` khởi tạo sẵn 1 `site_root` mẫu (`OnceLock`) cho mỗi integration binary và sao chép sang thư mục test, kết hợp poll `/api/health` chu kỳ `10ms`.

---

## Alternatives Considered

- **Tắt `fsync` hoàn toàn trên macOS:** Bị bác bỏ vì vi phạm A-DURABILITY.
- **Chỉ dùng `cargo-nextest` mà không tối ưu I/O:** Đã thử trên nhóm `i18n` — chậm hơn (`27.69s` vs `19.95s`) và fail `3/18` test do tranh chấp `F_FULLFSYNC`.
- **Gộp thành 1 binary duy nhất:** Bị bác bỏ vì mất khả năng chạy chọn lọc theo domain trong `run-tests.py --changed`.

---

## Non-Functional Requirements

- **Độ tin cậy:** Không có spin-wait không giới hạn thời gian; mọi barrier test phải có deadline rõ ràng.
- **Tương thích đa nền tảng:** `#[cfg(target_os = "macos")]` dùng `libc::fsync` đã có sẵn trong `Cargo.toml`; Linux/Windows giữ `file.sync_data()`.

---

## Detailed Implementation Plan

### Slice S01: Fix M8 Writer Barrier Audit for Split Route Modules & Plugin Worker Deadlock Guard
- **Dependencies:** None (chạy ngay trên `refactor/feature-first-modularization`).
- **File Ownership:** `cms/crates/opendash-server/tests/ops/writer_barrier.rs`, `cms/crates/opendash-server/src/plugin/runtime/store_limits.rs`, `cms/crates/opendash-server/src/plugin/review_regressions/suite_02.rs`.
- **Acceptance Criteria:** `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test ops -- m8_s01_t02_writer_barrier` và `plugin::review_regressions::` pass 100%; mọi mutating handler trong `routes/{entries,operations,plugin,themes}/*.rs` đều được kiểm tra `AdmittedMutation`.
- **Rollback:** `git checkout` 3 file thuộc quyền sở hữu của slice.

### Slice S02: Scope `PLUGIN_TEST_SERIAL` to Global-Hook Tests for Parallel Unit Execution
- **Dependencies:** Slice S01.
- **File Ownership:** `cms/crates/opendash-server/src/plugin/tests/suite_01.rs`, `cms/crates/opendash-server/src/plugin/review_regressions/suite_01.rs`, `cms/crates/opendash-server/src/plugin/review_regressions/suite_02.rs`.
- **Acceptance Criteria:** `opendash-server` unit tests (`283` tests) pass 100% với tổng thời gian `< 25s` (giảm từ `147.09s`); chạy lặp 3 lần liên tiếp không có race/flaky.
- **Rollback:** Khôi phục `PLUGIN_TEST_SERIAL` lock trên các test đã gỡ.

### Slice S03: Shared Pre-Seeded `site_root` Template & Fast Health Polling Helper
- **Dependencies:** Slice S01.
- **File Ownership:** `cms/crates/opendash-server/tests/common/mod.rs`, các file `spawn_server*` trong `cms/crates/opendash-server/tests/*/*.rs`.
- **Acceptance Criteria:** Thời gian mỗi integration binary giảm thêm ≥ 40% và `--group all` hoàn tất `< 45s` warm.
- **Rollback:** Gỡ `tests/common/mod.rs` và giữ `spawn_server*` cục bộ.

### Target Files to Create/Modify
- `cms/crates/opendash-server/tests/ops/writer_barrier.rs`: thêm helper `read_route_sources(routes_dir: &Path, file: &str) -> String` đọc `routes_dir.join(file)` nếu tồn tại hoặc nối nội dung mọi `*.rs` trong `routes_dir.join(stem)` nếu là thư mục.
- `cms/crates/opendash-server/src/plugin/tests/suite_01.rs`: gỡ `PLUGIN_TEST_SERIAL` khỏi các test `sqlite::memory:` độc lập không chạm global state.
- `cms/crates/opendash-server/tests/common/mod.rs` *(S03)*: helper `ensure_seeded_site_root_template()` và `wait_for_health()`.

### Verification Plan
```bash
python3 cms/scripts/check-standards.py && python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py && python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
python3 cms/scripts/run-tests.py --self-test && python3 cms/scripts/run-tests.py --list
cargo fmt --manifest-path cms/Cargo.toml --all --check
python3 cms/scripts/run-tests.py --group all
git diff --check
```
