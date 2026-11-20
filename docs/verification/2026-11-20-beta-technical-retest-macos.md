# M7-S14-T05 / Issue #410 — technical Beta retest trên macOS Intel

**Ngày kiểm chứng:** 2026-11-20 (Asia/Bangkok). **Agent:** Harbor. **Verdict: TECHNICAL_BLOCKED** đối với *toàn bộ* ma trận Beta B01–B10; các kiểm chứng thành phần được giữ riêng bên dưới. **Human: NOT_RUN. Beta: NOT_GRANTED. Common RC/V1: NOT_GRANTED.** Không có kết luận product Beta PASS từ báo cáo này.

Đây là audit chỉ ghi evidence cho [Issue #410](https://github.com/thientran2359/opendash-cms/issues/410), theo [G3 M7-S14-T05](../archive/plans/milestones/M7/slices/2026-11-20-beta-technical-retest-g3.md) (PR [#409](https://github.com/thientran2359/opendash-cms/pull/409), G3 source SHA 639f3f2823f22a3365f18527fd949760d0689dfe). Không sửa mã sản phẩm, harness, SDK, schema, migration hoặc fixture nguồn. Các tập dữ liệu, script thăm dò cục bộ, ZIP, binary, log, JSON receipt và ảnh chụp được giữ ngoài Git.

## Candidate, quyền sở hữu và môi trường

| Thuộc tính | Quan sát |
| --- | --- |
| Candidate/base đã pin | 5ad37389404728bd1344fb56b8c8d4817eda7f60 |
| Source G3 đã merge | 639f3f2823f22a3365f18527fd949760d0689dfe |
| Checkout sau fast-forward tài liệu | 1b06be2b5766e850fe38326bfc52515944033c4b |
| CMS tree tại candidate và checkout | **baabe37cee4c250b3f241232ee91493281e72638** (bằng nhau); không đổi source sản phẩm |
| Nhánh / ownership | agent/harbor; Project #3 Ready được kiểm tra trước pickup và chuyển In progress; chỉ report này được phép commit |
| Dependency | #378 Closed/Done, PR [#408](https://github.com/thientran2359/opendash-cms/pull/408) merged; PR #409 merged; không có PR Harbor đang mở tại pickup |
| Hệ điều hành / kiến trúc | macOS 15.8.2, Intel x86_64 |
| Công cụ | rustc/cargo 1.99.1, Node 24.22.2, npm 11.19.1, Python 3.13.18, Chrome 158.0.8127.36 |
| Release executable **đã pin sau compilation test** | Mach-O x86_64; SHA-256 **f8cca0db26e6ac26bb1fd89f52a1914ed4d28dfa69859c1c9855b032bb932f1f** |
| Embedded Admin index | 2abb70657de1a07b18cc6433da35ef4c4fcffce6f0292ca15b4bacd27feea1b8 |
| Embedded JS | 6a7c76e661b0af4904068db7eb91d0a2b9f60666e54c66fc74c102167406225a |
| Embedded CSS | ec113de7634d17c72b958b1944400c65060760846eabcd135bb93e50d1b8b292 |
| ZIP plugin mẫu | b04306779e1a76381ec0ce3e00d416d7b98ce2da045b59845cefb30cdaab2a37 |

Sau build lần đầu, file cms/target/release/opendash từng có SHA 197606dde62013ea28a34f592aa3833a72fd5bb33d434428aac7513c581e2511; release test compilation thay đổi file tại đường dẫn đó. Để tránh trộn binary, Harbor sao chép executable cuối sang manifests/opendash-t05-pinned (SHA **f8cca0db…**) và chạy lại byte parity: **PASS cả HTML/JS/CSS** trên binary đã pin. Tất cả runner B01–B10/C1–C4 được trích dẫn dưới đây dùng SHA f8cca0db…, trừ phép byte parity ban đầu được lưu riêng. Không suy bằng nhau chỉ từ tên file.

Các root/site/SQLite, môi trường TMPDIR mode 0700, test Chrome context và cổng 127.0.0.1 ngẫu nhiên đều tách biệt; runner ghi port_18964_used=false. Không chạy test trên website review/operator tại cổng 18964. Mọi session và ZIP có thể chứa dữ liệu thử nghiệm ở vùng artifact riêng, không có trong PR.

## Build, test và manifest

Thực chạy từ root repository (exit 0 cho các lệnh PASS):

    npm run typecheck --prefix cms/admin
    npm run build --prefix cms/admin
    cargo build --manifest-path cms/Cargo.toml --locked --release
    cargo test --manifest-path cms/Cargo.toml --locked --release --workspace -- --test-threads=1
    cargo test --manifest-path cms/Cargo.toml --locked --release --workspace -- --test-threads=1
    cargo test --manifest-path cms/Cargo.toml --locked --release -p opendash-server --test m6_s04_t01_egress_ssrf -- --test-threads=1
    cargo test --manifest-path cms/Cargo.toml --locked --release -p opendash-server --test m8_s05_t01_restore -- --test-threads=1

Hai lượt full release chạy **tuần tự**, cùng CMS tree, cùng TMPDIR chuẩn 0700, không lọc test. **Lượt 1: 71 test targets, 267 passed, 0 failed, exit 0. Lượt 2: 71 targets, 267 passed, 0 failed, exit 0.** Focused egress SSRF **1/1 PASS** và restore **1/1 PASS**, đều exit 0. Warnings Rust về dead code/chunk build của Vite được giữ nguyên trong log, không đổi thành error.

Đường dẫn artifacts riêng tính từ root repository: ../../opendash-tasks/harbor-m7-s14-t05-BrezMg/. Manifest gộp logs/receipts/screenshots: manifests/technical-artifact-manifest.json, **SHA-256 363ec811fc6b10033c7baa3eb5a9c133fb6c4506b417030580ee9843097b5f6d**; gồm **65** JSON/log/script entries và **49** ảnh có SHA. Các file binary, screenshot, DB, backup, secret thử nghiệm và ZIP không commit. Hai log full Rust là logs/full-release-1.log và logs/full-release-2.log. Các lệnh browser dùng Python Playwright offline đã pin trên cùng binary; flags cụ thể của mỗi runner đã được kiểm tra qua --help trước khi chạy. Các biên nhận dưới đây là nguồn quyết định, không phải suy diễn từ CI.

## Ma trận hành trình kỹ thuật B01–B10

Trạng thái **PASS thành phần** nghĩa là assertion được quan sát bằng Chrome/HTTP/SQLite đúng binary. **BLOCKED** nghĩa là chưa đủ điều kiện khẳng định *toàn bộ case*; các subcheck đạt vẫn được ghi. Runner FAIL do locator cũ được giữ với exit code và lỗi, không tự gọi là lỗi sản phẩm.

| Case | Trạng thái toàn case | Evidence thực chạy và giới hạn |
| --- | --- | --- |
| **B01** | **PARTIAL / BLOCKED** | Cold root hoàn toàn trống, bootstrap/login/dashboard, deep link, logout, mobile 390: **15-step primary UI PASS** có các bước B01; browser Admin navigation **PASS** trên desktop/mobile, Editor RBAC. Đăng nhập sau restart ở private continuation; chưa có walkthrough đầy đủ back/breadcrumb/error states cho cả vai trò. |
| **B02** | **PARTIAL / BLOCKED** | Primary UI tạo Post/Page, site-owned CPT studio_work (draft_publish), menu. Continuation qua Chrome có media upload, định nghĩa taxonomy studio_genre, CPT association, menu item trỏ Post qua ID ổn định; private continuation **FAIL** ở locator User form sau 8 assertions đã qua. Tài khoản Editor và Post draft mobile được tạo thật qua UI trong private editor attempt (receipt FAIL do strict locator khi đọc màn 403); Editor 403 được kiểm chứng lại **PASS** riêng. Chưa hoàn chỉnh field group/panel, term CRUD và full Editor image picker trên *primary* root. C1/C2 seeded runner là coverage độc lập. |
| **B03** | **PARTIAL / BLOCKED** | Primary UI: Post draft public 404, saved draft preview authenticated 200/guest 401; Post publish public 200; unsaved input không đổi public; Save bản xuất bản cập nhật public; Page publish 200; CPT draft 404. Section private public 404 trên continuation; C1/C3/C4 regressions có preview/CAS/privacy. Chưa kiểm chứng Builder homepage Save thay đổi public và mọi loại archive/sitemap trong *primary* walkthrough. |
| **B04** | **PASS trong seeded harness; primary NOT_RUN** | Existing m5_beta_theme_v2_e2e.py **12/12 PASS**, v2 ZIP install/duplicate, explicit activation, sample import dry-run/cancel/execute/idempotence trên site không trống, preservation menu/content/layout, 401/403/409/412, mobile và restart. Mỗi ZIP có hash trong m5_beta_theme_v2_receipt.json. Primary không làm thao tác ZIP. |
| **B05** | **PARTIAL / BLOCKED** | Primary continuation tạo Section riêng tư, kiểm public 404, mở đúng Section Builder; mở homepage Builder sau restart. Seeded m5 theme harness đã đi qua header/footer/single/archive và authored header override. Chưa đủ primary Page Builder save/reopen, desktop/tablet/mobile saved public parity, copied template và missing block fallback. |
| **B06** | **PARTIAL / BLOCKED** | M5 theme seeded A2→B switch và restart bảo toàn content/menu/layout; C2/C4 kiểm provider inactive→re-enabled với dữ liệu; full release suite có runtime/timeout/Wasmi regression, C4 browser provider lifecycle **23 checks PASS**. Không có một primary end-to-end plugin trap→Admin recovery với dữ liệu Studio. |
| **B07** | **PARTIAL / BLOCKED** | Admin navigation Chrome + Editor forbidden Theme route (private editor-403 receipt **PASS**). C1/C2/C3/C4 real 401, cross-account isolation, CAS 412, collision 409, CSRF/role guards PASS trong seeded harness; C3 form giữ input khi 409/412 và C4 401 recovery. Chưa chạy tất cả chuỗi dirty navigation/role downgrade/expiry từ *hai browser sessions của primary* như một workflow hoàn chỉnh. |
| **B08** | **PARTIAL / BLOCKED** | C4 encrypted backup → source override mutation → restore root khác → safe recovery/explicit activation → canonical provider routes và re-edit **6/6 PASS**. Existing M8 browser runner normal **exit 1** ngay source_visual_builder_navigation do selector Visual Builder cũ; chưa tới riêng bước backup, safe-before-activation/restart, session clear, SMTP-off trong runner này. Không gộp C4 restore với M8 UI để tuyên bố toàn bộ B08. |
| **B09** | **PARTIAL / BLOCKED** | Full release M8 security/restore/codec suites và focused SSRF/restore PASS. Existing M8 browser wrong-password và wrong-backup-password đều **exit 1 ở cùng precondition Visual Builder**, không chạy tới nhánh sai passphrase. Missing asset/secret, newer schema, disk-full, hostile ZIP với trạng thái root trước/sau chưa retest đầy đủ trên candidate trong một negative matrix riêng. |
| **B10** | **PASS suite Rust/C1–C4; BLOCKED full browser breadth** | Full release hai lượt 267/267; SSRF isolated 1/1; C1–C4 có canonical, stable-ID redirect 301, 409 rollback, 412, SEO/OG/sitemap, ACL draft/private, media, backup restore. Không dùng Rust success suy thành Chrome workflow SEO/contact/video end-to-end hay Windows/Linux release acceptance. |

Các ngữ cảnh UI trên primary root là **kiểm thử agent**, không phải cold human usability. Screenshots: browser/primary-ui/ (10 ảnh, receipt 15 PASS), browser/primary-ui-cont/ (6 ảnh từ attempt FAIL với 8 bước đã chạy), browser/editor-ui/ (Post nháp Editor), browser/editor-403/ (màn 403, PASS). Không có API/SQLite seed trong *primary 15-step walkthrough*. Các runner seed ở root riêng và được nêu dưới đây; không đưa assertions của chúng vào primary để tạo kết quả PASS giả.

## Content Foundation C1–C4 — impact trên cùng binary

| Nhóm | Script/receipt riêng trong browser/ | Kết quả và phạm vi |
| --- | --- | --- |
| **C1** | c1/receipt.json | **PASS 23 checks**: CPT publication/public/archive settings, Post/Page/Section policies, draft privacy, Admin Chrome 390, 401/403/409/412, media, restart và SQLite integrity. |
| **C2** | c2/receipt.json | **PASS 30 checks**: taxonomy definitions/terms/eligibility, provider inactive/re-enable, CAS snapshot/rebase, Chrome custom columns/term picker, account isolation, data integrity và restart. |
| **C3** | c3-t01/receipt.json và c3-t02/receipt.json | **PASS 16 checks** canonical routes, stable-ID redirect, no-leak/409/412/restart; **PASS 8 API + 10 Chrome UI** permalink settings, canonical URL authority, unsaved/public boundary, 401/private, 409/412 draft preservation, mobile 390. |
| **C4** | c4/receipt.json và c4-backup/receipt.json | **PASS 23 checks** provider rewrite inheritance/explicit null/custom, same-owner lifecycle, 401/409/412, responsive form, native restart; **PASS 6 checks** encrypted restore, active canonical/term routes, re-edit after activation. |

C1–C4 evidence ghi SHA binary đồng nhất **f8cca0db…**. Per-case source_sha của một số runner trỏ đến checkout docs sau fast-forward **1b06be2…**, hợp lệ vì CMS tree **baabe37…** đã được đối chiếu với candidate. Receipt của primary và tất cả seeded runner được giữ riêng trong manifest. Không có ngoại suy từ kết quả kiểm thử của PR khác.

## Lỗi runner, triage và giới hạn nghiệm thu

| Evidence FAIL giữ nguyên | Diagnostic thực tế | Phân loại |
| --- | --- | --- |
| logs/m7-menu.log | Exit 1, đợi nhãn Trạng thái xuất bản trong màn Post mới, timeout 30s | **S1 verification coverage gap**: selector của runner cũ không khớp Post/Page workflow Single Edit hiện hành |
| logs/m7-media.log | Exit 1, không tìm nút Tạo & Lưu nội dung, timeout 30s | **S1 verification coverage gap**, chưa kiểm được toàn bộ media picker qua script F05 cũ |
| logs/m7-draft.log | Exit 1, timeout nhãn Trạng thái xuất bản | **S1 verification coverage gap**, không tự gọi F03 product regression |
| browser/m8-normal/run-receipt.json | Exit 1, source_visual_builder_navigation, thiếu link Visual Builder | **S1 verification coverage gap**, B08 cần runner/browser retest |
| browser/m8-wrong-password/run-receipt.json | Exit 1 cùng phase, chưa tới wrong-password assertion | **BLOCKED**, tuyệt đối không gọi wrong password PASS hoặc expected failure |
| browser/m8-wrong-backup/run-receipt.json | Exit 1 cùng phase, chưa tới wrong-backup-password assertion | **BLOCKED**, tuyệt đối không gọi wrong archive password PASS |
| browser/primary-ui-cont/receipt.json | Exit 1 sau 8 UI assertions; strict locator ở form tạo người dùng | **Private probe limitation**, dữ liệu đã ghi được giữ; không coi cả runner PASS |
| browser/editor-ui/receipt.json | Exit 1 sau Admin tạo Editor và Editor lưu draft; strict selector 403 bị match hai element | **Private probe limitation**, màn 403 có thật; independent editor-403 receipt PASS |

Hướng xử lý: **Aster (coordinator)** triage S1 verification gaps và quyết định G3/file ownership cho follow-up tương ứng. Harness owners cập nhật các selector theo Single Edit và menu hiện hành trong task khác; chạy lại B02/B05/B08/B09 với receipt mới trên candidate đã repin nếu source đổi. T05 report không được phép sửa harness/product; không âm thầm patch rồi tuyên bố PASS. Các primary gaps (term CRUD, exact Page Builder, plugin recovery, SMTP/restart, negative disk-full/schema) cần bài kiểm thử mới hoặc một vòng operator/agent UI chuyên biệt có bằng chứng. Chưa có bằng chứng ở đây về product S0/S1 *mới* trên macOS; **khoảng trống kiểm chứng không chứng minh sản phẩm an toàn**.

Đối chiếu lịch sử: [Windows Beta audit 2026-11-16](2026-11-16-usable-beta-windows-audit.md) có **F08 security hold** (restored root public trước activation) và **F09 full-suite SSRF flake**. [macOS F08 G5](2026-11-17-m8-s10-t01-g5-macos.md) và [macOS F09 G5](2026-11-17-m6-s09-t01-g5-macos.md) không tự giải quyết Windows finding. Phiên này macOS full Rust 2× PASS, isolated SSRF 1/1, C4 restore PASS; **Windows native F08/F09 đối chiếu vẫn NOT_RUN**, security hold / common RC authorization giữ nguyên. Không sửa hay xóa historical FAIL.

## Kết luận task và gate còn mở

Harbor đã hoàn thành **thu thập và phân tích các kết quả thực chạy có thể kiểm chứng**; **TECHNICAL_BLOCKED** cho Beta ma trận đầy đủ vì B02/B05/B07/B08/B09 chưa đủ mandatory journey evidence. Đây là **verification report**, không là phát hành hay hoàn thành usability của [#302](https://github.com/thientran2359/opendash-cms/issues/302), [#303](https://github.com/thientran2359/opendash-cms/issues/303) hoặc [#304](https://github.com/thientran2359/opendash-cms/issues/304). Human cold walkthrough **NOT_RUN**; Beta **NOT_GRANTED**; Windows/Linux/common RC/V1 **không được suy từ macOS G5**. Issue #410 chỉ có thể được đánh Done về **việc nộp technical report** sau khi report PR được self-review tại exact head, CI bắt buộc PASS và PR merge hợp lệ. Aster quyết định repin/follow-up/human readiness; Harbor không tự waive gate.
