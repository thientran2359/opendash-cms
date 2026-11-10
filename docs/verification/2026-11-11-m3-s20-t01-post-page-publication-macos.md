# M3-S20-T01 — Post/Page publication backend · macOS verification

- **Ngày thực chạy:** 2026-11-11; **Agent:** Harbor (CoS macOS), implementation owner.
- **GitHub Issue:** [#24](https://github.com/thientran2359/opendash-cms/issues/24); **Task ID:** `M3-S20-T01`.
- **G3/contract:** [ADR-0004](../decisions/ADR-0004-m3-post-page-publication-boundary.md), [ADR-0005](../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md), [G3 source + renderer ownership Design Delta@922b598](../plans/milestones/M3/slices/2026-11-11-m3-autonomous-first-tasks.md). G3 nguồn đã merge trước G4, Issue được Project #3 Ready trước pickup; tiến độ runtime được chuyển In progress.
- **Code under test:** `ab56f8f6fc608d01b40a9641fbef9a584e5b19e5` — branch `task/harbor-m3-s20-t01`; baseline main `922b598c895756738ef143ecbf5eadce3fd43d3b`. Không dùng live DB.
- **Môi trường:** macOS 15.8.2, Intel/Darwin `x86_64`, Rust `1.99.1`, Python 3, SQLite `3.43.2`. Cargo tại `$HOME/.cargo/bin` (CoS app PATH không tự chứa thư mục này).
- **CI/merge:** báo cáo này xác nhận local test tại đúng code SHA; required GitHub PR checks và G5/Project Done chỉ được đánh dấu sau khi thật sự kiểm tra trên PR head.

## Thay đổi và ranh giới an toàn

1. Forward migration `0008_post_page_publication.sql` thêm `publication_status` CHECK `draft|published` với default `published` cho **legacy CPT**, index lọc published và SQLite trigger chặn Post/Page INSERT kế thừa mặc định published. Seed built-in `post/page` Core-owned; không đụng migration 0001–0007 hoặc home_page Builder.
2. `init_db` chạy preflight **trước** SQLx migrate: reserved key do user/theme/plugin hoặc Core descriptor lệch chuẩn ⇒ startup fail với hướng dẫn collision offline; không auto-rename/overwrite.
3. Storage create explicit `draft` cho Post/Page, legacy vẫn published; Save giữ status. Publish/Unpublish chỉ qua POST Admin/Editor với CSRF/Origin, `If-Match: "vN"`, SQL version-CAS. 401/403/400/404/412 và 428 theo hợp đồng; không phát success hook khi CAS lỗi.
4. GET preview chỉ cho Admin/Editor, render **persisted entry**; image URL dùng endpoint Admin có session, CSP `script-src 'none'`, `no-store`, escaping rich-text/title.
5. Public single và archive chỉ đọc `published` trong SQL **trước** pagination; public media file/thumbnail chỉ phục vụ ref `image/gallery` từ entry published, AND bao toàn bộ các nhánh JSON; legacy references giữ public, draft-only 404.

## Kết quả thực chạy (trên code SHA ở trên)

| Lệnh / kiểm thử | Kết quả |
| --- | --- |
| `cargo fmt --manifest-path cms/Cargo.toml --all -- --check` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS** — 19 Core + 40 Server = 59 unit tests, 0 failed |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| `python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash` | **PASS** — Core seeds, takeover 409, auth/CSRF, upgrade, restart |
| `python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash` | **PASS** — legacy CRUD/version CAS/RBAC/field validation |
| `python3 cms/scripts/media_smoke.py --binary cms/target/debug/opendash` | **PASS** — thumbnails/ACL/storage/restart |
| `python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash` | **PASS** — M3.6 regression **và M3-S20 Post/Page security end-to-end** |
| `python3 cms/scripts/check-plans.py`; `--self-test` | **PASS** — 12 planning fixtures |
| `python3 .agents/scripts/check-agent-artifacts.py`; `git diff --check` | **PASS** |
| `cargo clippy --manifest-path cms/Cargo.toml --all-targets --locked -- -D warnings` | **NOT_RUN** — clippy component chưa cài trong toolchain Mac; không thay đổi toolchain hệ thống |

## Negative cases và dữ liệu mẫu

`public_content_smoke.py::test_post_page_publication` chạy trên site SQLite **tạo bởi binary thật** trong `TemporaryDirectory`, không kết nối DB người dùng:

- Tạo Post và Page bằng Admin/Editor luôn draft; guest GET single 404, archive không lộ title/slug dù có 21 draft mới hơn 1 published; pagination lọc draft trong SQL.
- Draft-only cover: public `/media/{id}/file` và `/thumbnail` đều 404; authenticated preview 200, guest 401; CSP/no-store/HTML escaping và Admin media URL kiểm chứng.
- Publish bởi Editor trả `published`/ETag `"v2"`; guest 401, missing CSRF 403, Origin sai 403, missing If-Match 428, invalid status/If-Match 400, invalid type 400, missing ID 404, stale CAS 412; Save published vẫn published, Unpublish làm public single/media trở lại 404.
- Legacy gallery tham chiếu **cùng media ID** giữ asset public trong khi Post là draft; forced draft-gallery fixture làm cả single và file/thumb 404 (kiểm tra nhánh OR không bypass AND), publish gallery lại thấy public, xóa reference cuối làm 404.
- SQL direct INSERT Post thiếu status bị trigger `RAISE(ABORT)`; DB copy dạng pre-0008 có user-owned `post` hoặc Core descriptor sai đều fail **trước migration** và không sửa key/đưa marker 0008 vào DB.
- DB copy pre-0008 không collision: **SQLx nâng cấp thật** lên 0008, giữ legacy entry public và tái seed Core; restart giữ trạng thái draft, giữ homepage Builder `/` và provider-inactive CPT behavior.

## Giới hạn và rollback

- Chỉ **macOS x86_64 backend/HTTP/SQLite** đã test. Không nhận là Windows parity, không đánh dấu M3 toàn bộ G5/CLOSED; UI Posts/Pages sẽ được làm ở task sau (Forge AdminShell #25 là task riêng).
- `clippy` không chạy vì toolchain thiếu component; browser GUI E2E của form Publish chưa có trong scope backend này. Các assertions sử dụng HTTP thực chứ không chỉ parse source.
- Migration 0008 là **forward-only**: Git revert không tự rollback DB đã nâng cấp. Khi cần rollback deployment phải sao lưu DB/site, thử rollback **trên bản copy** và giữ public draft fail-closed; không tự xử lý collision trên dữ liệu thật.
- Ảnh từng public có thể đã được tải/cache bên ngoài, việc unpublish chỉ chặn **lượt GET mới**; media trùng SHA đang public ở entry khác vẫn public như hợp đồng G2.
