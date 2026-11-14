# OpenDash V1 — Roadmap theo release, milestone và slice

**Phạm vi sản phẩm:** [PRD V1, bổ sung G0 M3 10/11/2026](../product/PRODUCT_REQUIREMENTS.md). **Chủ dự án đã chốt phương án A**: M3 là **Core CMS & WordPress-like Admin bằng Tailwind CSS**, dùng được từ browser, không phải chỉ Content & Media hoặc full WordPress. **Evidence M3.1–M3.6 ngày 09/11/2026** chứng minh một số slice cũ trên macOS, M3 vẫn `PARTIAL`; xem [snapshot theo scope cũ](M3_STATUS_2026-11-09.md) và [M3 task plan mới](milestones/M3/TASK_PLAN.md). M4–M8 giữ các năng lực V1 nâng cao.

**Mốc nghiên cứu 08/11/2026:** M2.5 đã [kiểm chứng](../verification/2026-11-07-m2-5-core-kernel.md); G1 CNT-01/CNT-05 được review, G2 là đề xuất vào thời điểm đó. **Sau mốc nghiên cứu**, M3.1–M3.6 đã có [kiểm chứng macOS](M3_STATUS_2026-11-09.md); không dùng nhãn `NOT_STARTED` cũ để giao lại việc.

## Hai cách theo dõi tiến độ

- **Roadmap này** cho biết kết quả nào phải có trước khi đóng một milestone, milestone nào phụ thuộc milestone khác, và các slice gần nhất.
- **[Module Catalog](MODULE_CATALOG_V1.md)** cho biết mọi năng lực V1 đang thuộc module nào, có thể được thực hiện ở nhiều milestone. Tham chiếu module bằng ID cố định như `CNT-01` hoặc `PLG-03`.

Xem [Planning Workflow](PLANNING_WORKFLOW.md) về research gates, status và mẫu task. Roadmap là công cụ điều phối: không thay thế thiết kế, ADR, code hoặc bằng chứng test.

**Phân quyền trạng thái:** Roadmap mô tả milestone, phụ thuộc và snapshot; [living module briefs](modules/CNT-01-content-types.md) mô tả scope/G0–G3; **GitHub Issues / Project #3** giữ trạng thái thực thi, assignee và dependency đang chạy; **PR** giữ diff/review; [verification](../verification/README.md) ghi kết quả thực chạy theo SHA, OS và phạm vi. Không coi các hàng backlog bên dưới là task `READY`.

**Kế hoạch V1 đầy đủ:** [Coverage Matrix — 27 Module → 9 Milestone → task](milestones/COVERAGE_MATRIX.md), [mục lục task theo từng milestone](milestones/README.md). Các `M1..M8/TASK_PLAN.md` phân biệt evidence và candidate, không thay Project Status. **Issue #15 là input nghiên cứu kỹ thuật lịch sử**, không phải toàn bộ roadmap: A0/A1 còn M3 verification, B taxonomy chuyển M4, C provider bridge chuyển M6, D video/lifecycle media chuyển M7/M8, E Builder ở M4.

## Release V1 — Các mốc và điều kiện đóng

| Milestone | Kết quả cần đạt | Module chính | Trạng thái & căn cứ |
| --- | --- | --- | --- |
| **M1** — CMS baseline | Auth/Admin/Editor, theme một block, Preview/Lưu, SQLite và Windows executable | FND-02/03/04/05, BLD-01, THM-01 | **Baseline verified** trong [M1/M2 import report](../verification/2026-11-07-m1-m2-import.md); chỉ phạm vi baseline |
| **M2** — Plugin sandbox baseline | Plugin ZIP + Wasmi process-isolated worker, permissions, timeout, log, quản lý lifecycle cơ bản | PLG-01, FND-03 | **Baseline verified** theo [M1/M2 import report](../verification/2026-11-07-m1-m2-import.md), chưa đồng nghĩa extension V1 đầy đủ |
| **M2.5** — Core Foundation | Registry ownership và lifecycle thực cho block, hook dispatcher, Base APIs dùng chung, boundary SQL/ZIP/auth, quy chuẩn có kiểm tra | FND-01, FND-05, PLG-02 | **Closed / verified** cho 8 mục đã chốt theo [kế hoạch](milestones/M2.5-core-foundation.md), [bằng chứng](../verification/2026-11-07-m2-5-core-kernel.md) và ADR-0002; Content hooks thuộc M3 |
| **M3** — Core CMS & WordPress-like Admin | Built-in `post`/`page`; Posts/Pages + draft/published đơn giản; media ảnh; Users/login/logout; Dashboard, Settings cơ bản, Themes ZIP/list/activate, Plugins ZIP/list/enable/disable; Admin **Tailwind CSS** | CNT-01/02/04/05, FND-02/05, SITE-01, THM-01, PLG-01 (MVP) | **PARTIAL**; source [M3.1–M3.6](M3_STATUS_2026-11-09.md) chỉ là baseline, **chưa có G5 M3 scope mới** |
| **M4** — Visual Builder + Taxonomy foundation | AST Section/Row/Column/Element, core elements, responsive/binding/list; category/tag và liên kết nội dung phục vụ list/filter | BLD-01–05, CNT-03/04 | **Proposed**; taxonomy không phải gate đóng M3 A |
| **M5** — Theme & Templates | Từ M3 activation → template composition, Theme Options, **menu model/storage tối thiểu cho import**, **theme ZIP khai báo CPT/fields/taxonomy**, import mẫu và theme update bảo toàn schema/dữ liệu/layout, URL policy nâng cao | THM-01–03, CNT-01/02/03/04, SITE-01/02, BLD-05 | **Proposed**; không làm lại screens cơ bản M3 |
| **M6** — Plugin Extensions | APIs/actions/events, external calls, CPT/field provider bridge và block/template/settings/taxonomy declarations, declarative plugin Admin UI | PLG-02–04 | **Proposed**; M3 chỉ có ZIP install/lifecycle UI |
| **M7** — Site Features + Media Video | Settings/menu nâng cao, SEO/sitemap, contact form + submissions/SMTP; video media an toàn | SITE-01–03, CNT-05 | **Proposed**; mở rộng settings M3 |
| **M8** — Operations & Release | Media referenced-delete/reconcile, backup/restore DB+media+theme+plugin, Linux VPS packaging/E2E và tài liệu cài đặt | OPS-01/02, CNT-05 và các module V1 | **Proposed** |

Không gắn ngày kết thúc khi chưa có phạm vi, thời lượng kiểm chứng và dependency được xác nhận. Không đánh dấu `CLOSED` chỉ vì code tồn tại hay số unit test đã tăng.

## Gates quan trọng xuyên các milestone

1. **M2.5 → M3 — Đã đạt tại `da79304`:** registry có owner cho block, lifecycle/hook sau commit đã nối vào luồng thực, chuẩn storage và kiểm tra quy chuẩn đủ minh chứng. M3 vẫn phải chốt riêng ownership/persistence của CPT và content hooks.
2. **M3 → M4 (Usable Core G5):** fresh install/upgrade + bootstrap/login → `post`/`page` built-in → soạn/chọn ảnh → draft không public, published URL/preview → Users/Settings → theme ZIP install/activate và plugin ZIP install/enable/disable → logout/restart vẫn còn dữ liệu. Kiểm Admin Tailwind, RBAC/CSRF/401/412, media ACL, XSS, Builder homepage compatibility và Windows regression; Linux early smoke không thay gate M8. Taxonomy/provider bridge/video/delete **không phải điều kiện đóng M3 A**.
3. **M4 → M5:** taxonomy category/tag/entry link và AST, responsive/binding contract có lưu/mở lại; query list/filter có test, preview chưa lưu độc lập trang public. M4 không phụ thuộc plugin CPT bridge M6.
4. **M5 → M6:** theme nâng cao page/header/footer/archive/single, **M5-P09 SITE-01 menu model + M5-P05 import menu mẫu**, **M5-P10 theme CPT/fields/taxonomy declarations qua core registry**, update/switch giữ schema/entry/layout/menu, URL policy cho post/page/taxonomy; dùng settings core M3 cho Theme Options. **M6-P08/P04 xử lý plugin declaration riêng** sau khi domain APIs ổn định.
5. **M6 → M7:** plugin permissions, lifecycle, Admin declarative UI và event/action có kết quả test thực.
6. **M7 → M8:** các chức năng V1 nhìn thấy được từ Admin/public và có kiểm thử hành vi qua HTTP/UI.
7. **Đóng V1:** test và bằng chứng trên Linux, restore từ backup rồi chạy website với content/media/theme/plugin; kiểm tra quyền, CSRF, preview, lưu và vô hiệu hóa extension.

Các gates này đề cập **hành vi cần có**. Một milestone có thể được chia lại khi research cho thấy phụ thuộc khác; ID module không đổi.

## Trạng thái M3 đã triển khai và khoảng trống so với G0 mới

**Nguồn:** [M3 status — snapshot 09/11 tại main@`b561805`](M3_STATUS_2026-11-09.md), [verification M3.1–M3.6](../verification/README.md), code hiện hành tại `main@e4a143291cbaaf3a302cd30e974e27ea9ab886f0` (không có thay đổi `cms/` sau baseline khảo sát [#15](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410)). Bằng chứng slice trên **macOS**, không suy thành Windows/Linux M3 PASS hoặc G5 toàn milestone.

- **Đã có theo slice:** CPT/9 fields (M3.1), CPT GET/POST và SQLite (M3.2), entry CRUD (M3.3), Admin content form (M3.4), raster media/picker (M3.5), public archive/single và media reference ACL (M3.6).
- **Khoảng trống G0 M3 A:** built-in `post`/`page` + draft/publish tối thiểu, AdminShell Tailwind/navigation, Dashboard, Users CRUD/role, Settings, Media screen, Theme install/list/activate toàn site, Plugin screen độc lập, public visibility/preview và full E2E/parity. M3 vẫn **PARTIAL**.
- **Khoảng trống V1 được phân bổ lại (không bỏ):** taxonomy B → M4; bridge C → M6; video D → M7; media delete/reconcile D → M8; Builder E → M4. A0 Windows smoke → A1 parity ở M3. [Issue #15 Final Decision](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410) là lịch sử thứ tự nghiên cứu, không tự coi đã có G2/G3 mới.

Các bảng S01–S05 dưới đây là **bản thiết kế lịch sử**, không ánh xạ 1:1 sang M3.1–M3.6 hoặc tạo queue thực thi. [Issue #14 Final Decision](https://github.com/thientran2359/opendash-cms/issues/14#issuecomment-6760144662) giữ nguyên các plan/snapshot tại đường dẫn hiện có trong đợt cập nhật index đầu tiên.

## Backlog slice sơ bộ cho các mốc sắp tới

### M2.5 — Đã hoàn tất, giữ làm baseline cho M3

Theo [bằng chứng M2.5](../verification/2026-11-07-m2-5-core-kernel.md), 8 mục đã được kiểm chứng bằng unit tests, `check-standards.py` cùng self-test, `windows_kernel_smoke.py` và bộ Windows regressions. Content hooks chưa được kích hoạt cho mô hình CPT chưa có; điểm này được chuyển đúng sang M3. Không mở lại M2.5 chỉ vì M3 phát triển thêm nghiệp vụ sử dụng Base APIs.

### M3 — Content & Media (backlog G0 **lịch sử trước khi đổi scope**, chỉ để truy vết)

| Slice dự kiến | Hành vi nghiệm thu | Module | Phụ thuộc |
| --- | --- | --- | --- |
| `M3-S01` — CPT definition & lifecycle | Admin tạo CPT, lưu qua restart; catalog phân biệt Core/User/Theme/Plugin và giữ định nghĩa khi provider tắt. Nối plugin tối thiểu theo contract chỉ sau G2; **chưa yêu cầu E2E đổi theme M5** | CNT-01, PLG-03 bridge khi sẵn sàng | M2.5 verified; G1/G2 owner + persistence, compatibility với `home_page` |
| `M3-S02` — Field schema & entries | Field schema hỗ trợ đủ hành vi V1; Admin form nghiệm thu trước 7 biến thể không phụ thuộc media picker (chữ ngắn, chữ dài, rich text, số, toggle, ngày, lựa chọn); entry CRUD lưu qua restart. Bổ sung plugin field declaration bridge qua CNT-02 để S05 kiểm chứng CPT + fields. Public entry route nếu cần phải có G2 visibility/CSP/escaping | CNT-02, CNT-04, PLG-03 (field bridge) | M3-S01; G2 CNT-02/04 và PLG-03; MediaRef contract G2 trước khi khóa schema image/gallery |
| `M3-S03` — Taxonomy | Tạo category cha–con và tag; liên kết entry, query theo taxonomy, phân quyền | CNT-03, CNT-04 | M3-S01/S02 |
| `M3-S04` — Media library & picker | Upload/serve ảnh/video an toàn trong uploads, DB metadata và reference ổn định; tích hợp picker cho image/gallery, test restart/missing-file | CNT-05, CNT-02 | G2 MediaRef và upload security; picker cần schema CNT-02 từ M3-S02 |
| `M3-S05` — Cross-module verification | E2E **plugin** khai báo CPT/field qua cầu nối đã duyệt → entry/media/taxonomy → disable/restart/re-enable; schema/data vẫn quản lý được; quyền và migration test trên DB copy | CNT-01–05, PLG-03 | M3-S01–S04; **theme switch E2E thuộc THM-03 ở M5** |

**Không dùng bảng S01–S05 cũ để tạo task hoặc đóng M3 mới.** Phân rã hiện hành là [M3 Core CMS phương án A](milestones/M3/TASK_PLAN.md); các mục taxonomy/plugin bridge/video/delete trong bảng lịch sử đã chuyển đến M4/M6/M7/M8. S01–S05 **không ánh xạ 1:1** với M3.1–M3.6 đã code. Xem [snapshot M3 cũ](M3_STATUS_2026-11-09.md) để tránh làm lại source.

S02 và phần backend upload của S04 có thể nghiên cứu/thi công độc lập **sau khi** chốt MediaRef; phần picker tích hợp phải đi sau schema field S02. Thay đổi HTTP/upload/migration thuộc **Full-Track**. Các ID slice là nhãn đề xuất, không phải task `READY` hay capability được verified.

**Ranh giới bridge lịch sử đã thay đổi:** CPT/field registration, provider persistence và E2E disable/restart là **M6**, sau taxonomy M4. M3 A không có bridge PLG-03 như gate sản phẩm; chỉ plugin ZIP/list/enable/disable baseline. Mọi migration/ABI vẫn cần G2 trước implementation.

**G0 mới đã chốt built-in `post`/`page`, chưa chốt G2 schema:** giữ route `/` và `home_page` M1; G2 cần quyết định seed/idempotency, status draft/published, preview/public visibility, reserved URL, active theme/block/site setting ownership và rollback. Plugin provider/manifest lifecycle có G2 riêng ở M6, media advanced ở M7/M8. **Migration đã apply không rollback chỉ bằng Git revert**; cần DB backup và forward migration.

### M4 — Builder (dự kiến)

`M4-S01` layout AST, core element descriptors & validation → `M4-S02` tree/canvas/inspector + Heading/Text/Image/Button → `M4-S03` responsive preview → `M4-S04` field binding/list/pagination → `M4-S05` persistence, Save và regression với dữ liệu cũ. Mỗi slice phải có test với page thật thay vì chỉ test shape của AST.

### M5–M8 — Theo dõi ở cấp capability

Giữ ở cấp module và gate trong [catalog](MODULE_CATALOG_V1.md). Chỉ chia slice/task chi tiết khi module gần đến lượt research; đặc biệt không khóa sớm mô hình cache/artifact hoặc cơ chế merge theme trước khi có thử nghiệm và số liệu.

**Pipeline public delivery V1 cần tách ownership:** M3 CNT-04 công khai `post`/`page` tối thiểu qua routes an toàn; M4 CNT-03 taxonomy; M5 SITE-02 URL/RequestContext và THM-02 template composition, M7 SITE-02 SEO/canonical. SITE-01 sở hữu settings engine: **General Settings tối thiểu từ M3**, Theme Options trên nền đó tại M5, màn nâng cao ở M7.

### Nghiệm thu xuyên module cần đưa vào các brief khi mở

Các mục dưới đây cụ thể hóa PRD và dependency, không chốt schema/API G2 hay tạo thêm module. Khi chia slice, gắn mỗi capability với một primary acceptance slice theo Planning Workflow.

| Mốc | Owner phối hợp | Hành vi phải nghiệm thu |
| --- | --- | --- |
| M5 — Public delivery | CNT-04, SITE-02, THM-02, BLD-01/05 | Tạo bài/CPT + field → template single/archive/header/footer → preview chưa lưu không đổi public; **Lưu content đã published** cập nhật URL/archive, còn **Lưu draft không public**; Builder Save giữ baseline. URL collision/reserved namespace/visibility qua G2; SEO đầy đủ M7. |
| M5 — Theme declarations & sample import | THM-01/03, CNT-01/02/03, CNT-05, SITE-01 | **M5-P10:** Theme ZIP khai báo CPT/fields/taxonomy qua canonical domain registry (không phải plugin adapter M6), đổi theme vẫn mở/sửa entry site-owned. **M5-P09:** SITE-01 menu model/storage persist ID/order/page refs trước **M5-P05** import menu mẫu; không ghi đè user menu/content; **M7-P02** thêm UI CRUD/binding dùng lại schema M5. |
| M5 — Bảo toàn thiết kế | THM-03, THM-02, BLD-01/02/03 | Phân biệt thiết kế mặc định của theme với layout người dùng đã sửa. Sau đổi/cập nhật theme, dữ liệu và layout còn, mở lại chỉnh sửa được; chỉ thay bằng mặc định mới khi người dùng chủ động chọn. Thiếu block/template phải có thông báo Admin và fallback public an toàn, không xóa node hoặc dữ liệu để che lỗi. Cơ chế lưu/merge cụ thể qua G2. |
| M6 — Plugin permissions & recovery | PLG-01/02/03/04 | Hiển thị quyền yêu cầu, tự áp dụng quyền hợp lệ đã khai báo, không thêm bước duyệt quyền; chặn thao tác ngoài quyền. Plugin lỗi/timeout có log, vô hiệu hóa được qua Admin, phần website độc lập vẫn hoạt động; dữ liệu giữ lại vẫn quản lý được. |
| M7 — Contact form | SITE-03, BLD-02/03, SITE-01 | Người dùng tạo form với trường cơ bản, đặt qua builder; khách gửi form, submission xem được trong Admin và thông báo gửi qua SMTP đã cấu hình. Kiểm cả validation và SMTP lỗi; chính sách lưu/thông báo lỗi phải rõ, không chỉ nghiệm thu form cố định. |

**Kiểm chứng vận hành sớm:** OPS-02 xác minh Windows CMS M3 và thử Linux smoke nếu có runner, ghi binary/SHA/migration/restart/UI/permissions/path. Thiếu Linux runner = `NOT_RUN`, không nói Linux VERIFIED; M8 mới nghiệm thu phân phối Linux/V1.

**Contract phục hồi từ đầu:** khi chốt G2 dữ liệu/file ở M3, và package/theme ở M5–M6, FND-03/CNT-05/THM-01/PLG-01 phải đối chiếu OPS-01 về nguồn dữ liệu canonical, snapshot nhất quán DB + files/config/packages, định danh qua restore, write isolation và recovery sau lỗi. Chốt ràng buộc trước khi triển khai phần phụ thuộc; chưa xây backup engine sớm. M8 kiểm restore vào site root riêng rồi chạy và chỉnh sửa website thực, không chỉ kiểm archive backup tồn tại.

## Hồ sơ G1/G2 lịch sử và phần còn mở sau M3.1–M3.6

| Module | Living Module Brief | Quyết định cần chốt trong G2 |
| --- | --- | --- |
| **CNT-01 — Content Types** | [Module brief](modules/CNT-01-content-types.md) | Registry và DB giữ quyền sở hữu thế nào, provider tắt vẫn quản lý nội dung ra sao, có cần migration nào? |
| **CNT-05 — Media Library** | [Module brief](modules/CNT-05-media.md) | Media reference ổn định thế nào, validation upload, link tới fields, cleanup và phạm vi backup? |

G1 của hai module được `REVIEWED` ngày 08/11, còn [G2 trong thread lịch sử](../discussions/threads/2026-11-08-m3-g2-content-types-media-design.md) ở trạng thái `PROPOSED` tại thời điểm đó. **Sau đó M3.1–M3.6 đã được triển khai**; không suy rằng phương án G2 đã được phê duyệt hồi tố. Việc tiếp tục plugin bridge, taxonomy, schema edit, video/media deletion và lifecycle cần đối chiếu với [trạng thái mới](M3_STATUS_2026-11-09.md), sau đó quyết định gate cho từng phạm vi.

## Definition of Done của Release V1

Một website mẫu có thể được cài và vận hành trên VPS Linux bằng binary OpenDash; quản trị viên và biên tập viên dùng Admin tiếng Việt; định nghĩa CPT/custom fields/taxonomy, nhập nội dung và media; dựng page/header/footer/archive/single ở Builder, xem desktop/tablet/mobile, bind dữ liệu và list phân trang; cài theme/plugin ZIP; khi đổi/tắt extension vẫn quản lý được dữ liệu và bố cục người dùng; dùng contact form, SEO/sitemap; sao lưu và phục hồi thủ công toàn bộ website.

Kết quả phải có hồ sơ kiểm chứng theo capability, đặc biệt cho các luồng có dữ liệu bền vững, quyền và extension. Cần lưu quyết định sản phẩm về giấy phép/mô hình thu phí riêng nếu ảnh hưởng phát hành; hiện PRD vẫn để mở.

## Ngoài phạm vi V1

Theo PRD: thương mại điện tử, AI dựng trang từ ảnh, **workflow duyệt bài/xuất bản nhiều bước (ngoài draft/published tối thiểu M3)**, revision history, repeater/relation, reusable blocks tùy ý, advanced visitor filters, UI plugin tùy ý, scheduled tasks/backup, website đa ngữ và chuyển ngôn ngữ Admin. Các yêu cầu này chỉ mở ở roadmap sau V1 khi chủ dự án quyết định.

## Cập nhật M7 planning — 14/11/2026

[ADR-0014](../decisions/ADR-0014-m7-site-features.md), [M7 contract](../contracts/M7_SITE_FEATURES.md) và [G3 toàn bộ14 task/8 candidate](milestones/M7/slices/2026-11-14-m7-site-features-g3.md) chốt thiết kế khi PR merge. Phần M7 Proposed trong bảng lịch sử bên dưới được thay bằng G2 AGREED/G3 READY cho documentation; implementation NOT_STARTED/runtime NOT_RUN. M5/M6 Windows development gate và Project Ready từng task vẫn bắt buộc; macOS hoãn theo ADR-0014, chưa multi-OS VERIFIED. Không sửa trạng thái milestone M3–M6 từ bản planning này.
