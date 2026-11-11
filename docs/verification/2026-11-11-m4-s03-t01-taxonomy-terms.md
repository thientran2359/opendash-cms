# M4-S03-T01 — Category cha–con và tag trong Admin

- **Task:** [#79](https://github.com/thientran2359/opendash-cms/issues/79) · CNT-03.1 · Harbor (CoS macOS).
- **Contract:** [ADR-0007 § P09](../decisions/ADR-0007-m4-builder-taxonomy-foundation.md) · [G3 slice](../plans/milestones/M4/slices/2026-11-11-m4-foundation-g3.md).
- **Approved source:** `0e3d9886afa6115f84d97aa8905bc29f18fa56cb`.
- **Implementation base:** `aa512c1a376f64391676ca16ba36ee82e902e3ac` (includes [ADR-0008](../decisions/ADR-0008-harbor-forge-autonomous-delivery.md)).
- **Implementation head:** xem commit và head SHA của task PR #79 trong phần review/Issue; kết quả sau merge phải được đối chiếu với head của PR.
- **Status:** **G4 implementation acceptance PASS trên Mac Intel với fixture độc lập; G5 deployment/release nghiệm thu vẫn pending.**

## Hợp đồng và phạm vi

SQLite site-owned `taxonomy_terms` lưu ID ổn định, kind category/tag, tên, slug, parent và version. Admin tạo/sửa/xóa term qua API có session, CSRF/Origin và CAS `If-Match`; Editor đọc được. Category có thể có parent category, giới hạn 8 cấp kể cả độ sâu nhánh khi reparent; tag không có parent. Truy vấn SQL thuộc storage, writer reservation được lấy trước mọi lần kiểm tra parent/subtree. Giao diện Admin hỗ trợ thao tác bàn phím và viewport 390px.

M4-S04-T01 [#80](https://github.com/thientran2359/opendash-cms/issues/80) sẽ thêm gắn term vào entry và lọc nội dung. URL taxonomy công khai và đăng ký taxonomy bởi theme/plugin thuộc milestone sau.
Migration `0012_taxonomy_terms.sql` **không** tạo `entry_terms`, tránh chiếm migration/file của #80. Đường xóa đã tương thích an toàn khi bảng `entry_terms` xuất hiện về sau (kiểm FK và liên kết trước khi xóa); unit test dựng thử bảng nối theo hợp đồng P10.

## Lệnh kiểm chứng

Chạy từ root checkout branch `task/harbor-m4-s03-t01` trên máy Mac Intel; mọi HTTP test sử dụng SQLite tạm.

```sh
cd cms/admin
npm run build
cd ../..
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --locked
python3 cms/scripts/taxonomy_terms_smoke.py --binary cms/target/debug/opendash --pre-m4-binary /path/to/actual/pre-m4/opendash
python3 cms/scripts/taxonomy_terms_browser.py --binary cms/target/debug/opendash --chromium /path/to/Chrome
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
git diff --check
```

## Nghiệm thu và giới hạn

Kiểm lần lượt guest 401, Editor GET/403 mutation, Admin CRUD/CSRF/Origin, slug trùng và đổi slug giữ ID, CAS 428/412, cycle/độ sâu và hai kết nối reparent, xóa có child, pagination, restart, cùng upgrade Post/Page/CPT trên bản sao DB và snapshot rollback bằng binary cũ. Kiểm giao diện với Playwright/Chromium tại desktop và 390px.

## Evidence thực chạy — Harbor (CoS macOS), 2026-11-11

Môi trường: Mac mini Intel x86_64 (macOS); Rust 1.99.1, Cargo 1.99.1; Node v24.22.2; Admin `tsc` 5.6.3 và Vite 6.4.6. SQLite tests dùng `TemporaryDirectory` hoặc site thử nghiệm `/tmp/opendash-m4-79-browser.*`, **không mở database site đang dùng**. Binary baseline M3 là bản x86_64 đã có ở workspace chính, migration cao nhất `0011`.

| Kiểm chứng | Kết quả | Evidence |
|---|---|---|
| `npm run build --prefix cms/admin` | **PASS** | 62 modules, Vite production bundle; UI có menu Taxonomy |
| `npm run typecheck --prefix cms/admin` | **PASS** | `tsc --noEmit`, exit 0 |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** | Rust binary `cms/target/debug/opendash` trên macOS |
| `cargo fmt --manifest-path cms/Cargo.toml --all -- --check` | **PASS** | exit 0 |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS** | 19 core + 53 server = **72 tests**; taxonomy unit test cycle, depth, writer races, FK và CAS |
| `taxonomy_terms_smoke.py --binary ... --pre-m4-binary ...` | **PASS** | Fresh, Admin/Editor/guest, CSRF/Origin, slug, ETag, pagination, cycle, 8 cấp, 2 reparent, restart; pre-M4 Post/Page/CPT giữ nguyên, snapshot rollback bằng binary cũ |
| `taxonomy_terms_browser.py --binary ... --chromium /Applications/Google Chrome.app/Contents/MacOS/Google Chrome` | **PASS** | Playwright/Chromium thật: CRUD category cha–con/tag, CAS 412 giữ draft, chuyển phiên bản, chặn xóa 409, Enter bàn phím, Editor read-only, viewport **390×844** không tràn ngang |
| Regression `content_types_smoke.py`, `content_entries_smoke.py`, `general_settings_smoke.py`, `themes_smoke.py` | **PASS** | M3 auth/CPT/entries/General Settings/Themes và persistence tương thích |
| `python3 cms/scripts/check-plans.py`, `--self-test`, `.agents/scripts/check-agent-artifacts.py` | **PASS** | 12 fixtures và artifact checker |
| `git diff --check` | **PASS** | không có whitespace errors |
| `python3 cms/scripts/check-standards.py` | **NOT_RUN (environment)** | Python hệ thống 3.9 thiếu `tomllib` (cần Python >=3.11); không chỉnh script ngoài ownership #79 |

**G5 không tự suy ra từ G4 hoặc merge.** Cần nghiệm thu sau tích hợp/deployment với đúng SHA phát hành, data/rollback evidence; chưa đánh dấu Project `Done`.
