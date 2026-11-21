# EGM #430 — Pattern CPT / PatternRef S02: kiểm chứng macOS Intel

- **Ngày:** 2026-11-21.
- **Issue:** [#430](https://github.com/thientran2359/opendash-cms/issues/430).
- **Implementation PR:** [#447](https://github.com/thientran2359/opendash-cms/pull/447), branch `agent/harbor`; SHA được xác nhận cùng biên bản self-review trên PR.
- **Design:** [Generic CPT Content Builder, Pattern CPT & Pattern Library](../designs/cpt-content-builder-and-pattern-library.md), `Slice S02`; S03 (Gutenberg Pattern Library) là Issue độc lập.
- **Máy native:** macOS 15.8.2, `x86_64` (Intel), Rust 1.99.1, Node v24.22.2.
- **Cô lập:** `OPENDASH_SITE_ROOT` ở thư mục tạm canonical, SQLite riêng, port loopback ngẫu nhiên. Không dùng port `18964` của review site.

## Phạm vi đã triển khai

1. Migration `0044`: chuyển CPT riêng tư `section` sang `pattern` trên cùng ID, giữ `content_entries.version`, AST và media refs; hợp nhất vào `entry_builder_documents`, `entry_builder_media_refs`, thêm `entry_pattern_refs` với cascade bảo vệ FK và SQLite trigger bảo vệ publish/type.
2. `core/pattern_ref` (depth = 1): xác nhận ID trỏ tới Pattern, không lồng PatternRef hoặc ContentList trong Pattern; tải batch tối đa 16 Pattern từ SQLite, giới hạn 1024 node sau mở rộng và 512 KiB HTML; bỏ qua bản ghi Pattern thiếu mà vẫn giữ nguyên AST của trang.
3. API `/api/admin/patterns`: Session GET, Admin + CSRF cho POST/PUT/DELETE, CAS/ETag và báo 409 kèm nội dung đang tham chiếu trước khi `force=true`. Media của Pattern chỉ public theo entry đang `published` + `content_mode=builder` + CPT public/active/supports Builder.
4. Admin PatternsPage, route/sidebar, Inspector chọn Reusable Pattern, khôi phục draft/412, Builder tập trung theo entry. Shared-template override từ chối PatternRef bằng 400 để tránh lưu tham chiếu thiếu chỉ mục; Admin cũng không hiển thị lựa chọn này trên shared templates.

## Kết quả thực chạy

| Kiểm chứng | Kết quả | Ghi chú |
| --- | --- | --- |
| `python3 cms/scripts/egm_pattern_s02_migration.py` | **PASS** | Fixture SQLite chạy migration thật 0001–0044, giữ ID/CAS/AST/media; FK và trigger riêng tư đúng. |
| Rust Core unit tests | **PASS 54/54** | Giữ các hợp đồng Builder, CPT, content. |
| S01 CPT Builder `egm_cpt_builder_s01` | **PASS 2/2** | Không làm mất hành vi CPT Builder từ Issue #429. |
| `egm_cpt_builder_s02` (native HTTP; 3 Page) | **PASS 1/1** trên source `59c061e` | Home/Contact/About đồng bộ từ 1 Pattern; 403 Editor; depth 1; 409 force; CAS 412; media public/draft; shared template PatternRef trả 400. |
| C4 `content_foundation_m6_s11_t01` release `--test-threads=1` | **PASS 9/9** | Nâng cấp từ migration 0040, kiểm tra permalink/ownership, Wasmi và site data. |
| Admin `npm run typecheck` / `npm run build` | **PASS / PASS** | Vite build 101 modules; có warning chunk >500KB (không fail). |
| Standards/Plans checkers + self-tests, agent artifacts, `git diff --check` | **PASS** | Python 3.9 dùng `PYTHONPATH` shim `tomllib` từ `pip._vendor.tomli`, không sửa dependency của repo. |
| `rustfmt --edition 2024 --check` theo danh sách file Rust của PR | **PASS** | Đúng lệnh gate `cms-ci` (changed files). |
| `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked -- --test-threads=2` | **PARTIAL (exit 101)** | Mac Intel: server unit **154/154 PASS**, Core **54/54 PASS**, C4 8/9 PASS, 1 fixture server start timed out khi chạy đồng thời; cùng case PASS riêng 1/1 ngay sau đó. Không tuyên bố full macOS PASS. |
| `c4_conflicting_provider_zip_rolls_back_owner_and_canonical_claims` release, chạy riêng | **PASS 1/1** | `TMPDIR=/private/tmp`, chứng minh case bị timeout trong full suite vẫn giữ assertions thật; 8 trường hợp C4 còn lại cũng PASS ở lượt 2-thread và C4 9/9 PASS ở lượt tuần tự. |
| Required CI trên implementation SHA `59c061e` (`governance-ubuntu`, `governance-windows`, `cms-ci`) | **PASS 3/3** | GitHub Actions Governance [run 44914111328](https://github.com/thientran2359/opendash-cms/actions/runs/44914111328) và CMS [run 44914111378](https://github.com/thientran2359/opendash-cms/actions/runs/44914111378) `success`. Commit biên bản sau đó cần CI trên SHA mới. |

## Giới hạn và ghi chú môi trường

- Lượt release đầu dùng macOS default `TMPDIR` có thành phần `/var` là symlink, bị bảo vệ canonical path từ chối tại 8 tests, rồi xử lý bằng `TMPDIR=/private/tmp`. Test C4 còn cần cập nhật expectation migration mới nhất từ 0043 → 0044; sau sửa đã PASS 9/9 chạy tuần tự. Không đổi security path validator.
- Một lượt full release ép `--test-threads=1` bị kẹt ở `plugin::review_regressions::abort_during_staged_commit_must_cleanup_connection_and_pool`: test baseline có vòng đợi barrier chưa timeout; process test do lượt chạy này sinh ra đã được kết thúc `SIGTERM` sau khi không tiến triển. Không ghi nhận lượt bị kẹt là PASS.
- Lượt full release `--test-threads=4` đã PASS toàn bộ server unit 154/154 (gồm barrier plugin) nhưng hai trường hợp C4 khởi tạo nhiều server cùng lúc timeout với log startup trống; cùng file C4 đã PASS 9/9 khi chạy tuần tự trên máy này. Lượt `--test-threads=2` được dùng để giảm tranh CPU và vẫn cho phép barrier plugin tiến triển.
- Lượt 2-thread vẫn có một case C4 timeout khi start concurrent server mặc dù chạy riêng cùng case trên exact source đã PASS 1/1. Trên GitHub CI Linux, toàn bộ `cargo test --workspace --release --locked` đã PASS ở SHA `59c061e` (run `44914111378`); biên bản này ghi rõ mức macOS native là focused PASS + full suite PARTIAL, không quy kết timeout môi trường thành PASS toàn bộ.
- Native E2E HTTP chạy server thật và SQLite thật trong root riêng. Chưa có receipt browser Chrome UI chuyên biệt cho S02; frontend được typecheck/build và các HTTP assertions của S02 đã chạy native.
- S03 Pattern Library UI là phần việc độc lập; S02 chỉ triển khai các endpoint chuẩn bị cho S03.
- Lệnh `node cms/scripts/codegraph-index-docs.mjs`: **NOT RUN thành công** do chưa có CodeGraph engine (`@astudioplus/codegraph-mcp@0.21.0` / `CODEGRAPH_ENGINE_PATH`); không chạm DB website.
- Trạng thái `Verified` toàn bộ Design Spec chỉ áp dụng sau khi S03 hoàn tất và coordinator chạy drift report, theo EGM; biên bản này chứng minh riêng Slice S02.
