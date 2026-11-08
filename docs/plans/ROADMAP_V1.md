# OpenDash V1 — Roadmap theo release, milestone và slice

**Phạm vi sản phẩm:** [PRD V1 đã duyệt](../product/PRODUCT_REQUIREMENTS.md). **Hiện trạng 09/11/2026:** M3.1–M3.6 có source và evidence trên macOS, nhưng M3 vẫn `PARTIAL`; xem [bảng đối chiếu M3](M3_STATUS_2026-11-09.md). M4–M8 tiếp tục qua gate tương ứng.

**Mốc nghiên cứu 08/11/2026:** M2.5 đã [kiểm chứng](../verification/2026-11-07-m2-5-core-kernel.md); G1 CNT-01/CNT-05 được review, G2 là đề xuất vào thời điểm đó. **Sau mốc nghiên cứu**, M3.1–M3.6 đã có [kiểm chứng macOS](M3_STATUS_2026-11-09.md); không dùng nhãn `NOT_STARTED` cũ để giao lại việc.

## Hai cách theo dõi tiến độ

- **Roadmap này** cho biết kết quả nào phải có trước khi đóng một milestone, milestone nào phụ thuộc milestone khác, và các slice gần nhất.
- **[Module Catalog](MODULE_CATALOG_V1.md)** cho biết mọi năng lực V1 đang thuộc module nào, có thể được thực hiện ở nhiều milestone. Tham chiếu module bằng ID cố định như `CNT-01` hoặc `PLG-03`.

Xem [Planning Workflow](PLANNING_WORKFLOW.md) về research gates, status và mẫu task. Roadmap là công cụ điều phối: không thay thế thiết kế, ADR, code hoặc bằng chứng test.

## Release V1 — Các mốc và điều kiện đóng

| Milestone | Kết quả cần đạt | Module chính | Trạng thái & căn cứ |
| --- | --- | --- | --- |
| **M1** — CMS baseline | Auth/Admin/Editor, theme một block, Preview/Lưu, SQLite và Windows executable | FND-02/03/04/05, BLD-01, THM-01 | **Baseline verified** trong [M1/M2 import report](../verification/2026-11-07-m1-m2-import.md); chỉ phạm vi baseline |
| **M2** — Plugin sandbox baseline | Plugin ZIP + Wasmi process-isolated worker, permissions, timeout, log, quản lý lifecycle cơ bản | PLG-01, FND-03 | **Baseline verified** theo [M1/M2 import report](../verification/2026-11-07-m1-m2-import.md), chưa đồng nghĩa extension V1 đầy đủ |
| **M2.5** — Core Foundation | Registry ownership và lifecycle thực cho block, hook dispatcher, Base APIs dùng chung, boundary SQL/ZIP/auth, quy chuẩn có kiểm tra | FND-01, FND-05, PLG-02 | **Closed / verified** cho 8 mục đã chốt theo [kế hoạch](milestones/M2.5-core-foundation.md), [bằng chứng](../verification/2026-11-07-m2-5-core-kernel.md) và ADR-0002; Content hooks thuộc M3 |
| **M3** — Content & Media | CPT, fields, taxonomy, entry CRUD, media library, owner persistence và Admin forms; plugin bridge tối thiểu | CNT-01–05, PLG-03 (bridge tối thiểu) | **PARTIAL**: M3.1–M3.6 [đã có bằng chứng](../verification/2026-11-09-m3-6-public-content-rendering.md), [phạm vi chi tiết](M3_STATUS_2026-11-09.md); taxonomy/bridge/video/xóa media/backup chưa hoàn tất |
| **M4** — Visual Builder | Cây Section/Row/Column/Element, core elements, tree + preview + inspector, responsive, binding và list block | BLD-01–05, CNT-01–05 | **Proposed** |
| **M5** — Theme & Templates | Dựng page/header/footer/archive/single, Theme Options qua SITE-01 engine tối thiểu, import mẫu, đổi/cập nhật theme bảo toàn dữ liệu/layout | THM-01–03, SITE-01, BLD-05, CNT-04 | **Proposed**; request URL context cần tách khỏi template selection |
| **M6** — Plugin Extensions | APIs/action/event, external calls trong quyền, đăng ký CPT/fields/block/template/settings, declarative plugin Admin UI | PLG-02–04 | **Proposed**; một phần PLG-03 phải được thử ngay tại M3 |
| **M7** — Site Features | Site settings/navigation, SEO, sitemap, contact form + lưu submissions + SMTP | SITE-01–03 | **Proposed** |
| **M8** — Operations & Release | Backup/restore đủ DB+media+theme+plugin, phân phối Linux VPS, hồi quy V1 và tài liệu cài đặt | OPS-01/02 và các module V1 | **Proposed** |

Không gắn ngày kết thúc khi chưa có phạm vi, thời lượng kiểm chứng và dependency được xác nhận. Không đánh dấu `CLOSED` chỉ vì code tồn tại hay số unit test đã tăng.

## Gates quan trọng xuyên các milestone

1. **M2.5 → M3 — Đã đạt tại `da79304`:** registry có owner cho block, lifecycle/hook sau commit đã nối vào luồng thực, chuẩn storage và kiểm tra quy chuẩn đủ minh chứng. M3 vẫn phải chốt riêng ownership/persistence của CPT và content hooks.
2. **M3 → M4:** content schema/field value, quyền và lifecycle provider đã đủ ổn định để builder bind dữ liệu; API CRUD có test lưu/đọc và nếu có public entry route phải được G2 phê duyệt/kiểm escaping/CSP, không mở mặc định mọi CPT; media reference có đường quản lý.
3. **M4 → M5:** AST, responsive properties và binding contract có trường hợp lưu/mở lại, xác nhận preview chưa lưu độc lập với trang public.
4. **M5 → M6:** nghiệm thu luồng tạo nội dung → gán template → preview chưa lưu → Lưu → URL công khai/archive/phân trang; đổi/cập nhật theme giữ dữ liệu và layout tùy chỉnh, mở lại chỉnh sửa được; SITE-01 settings engine tối thiểu phục vụ Theme Options; extension mới đăng ký qua Base APIs với đường fallback khi provider tắt.
5. **M6 → M7:** plugin permissions, lifecycle, Admin declarative UI và event/action có kết quả test thực.
6. **M7 → M8:** các chức năng V1 nhìn thấy được từ Admin/public và có kiểm thử hành vi qua HTTP/UI.
7. **Đóng V1:** test và bằng chứng trên Linux, restore từ backup rồi chạy website với content/media/theme/plugin; kiểm tra quyền, CSRF, preview, lưu và vô hiệu hóa extension.

Các gates này đề cập **hành vi cần có**. Một milestone có thể được chia lại khi research cho thấy phụ thuộc khác; ID module không đổi.

## Backlog slice sơ bộ cho các mốc sắp tới

### M2.5 — Đã hoàn tất, giữ làm baseline cho M3

Theo [bằng chứng M2.5](../verification/2026-11-07-m2-5-core-kernel.md), 8 mục đã được kiểm chứng bằng unit tests, `check-standards.py` cùng self-test, `windows_kernel_smoke.py` và bộ Windows regressions. Content hooks chưa được kích hoạt cho mô hình CPT chưa có; điểm này được chuyển đúng sang M3. Không mở lại M2.5 chỉ vì M3 phát triển thêm nghiệp vụ sử dụng Base APIs.

### M3 — Content & Media (backlog lịch sử; [phần triển khai hiện hành](M3_STATUS_2026-11-09.md))

| Slice dự kiến | Hành vi nghiệm thu | Module | Phụ thuộc |
| --- | --- | --- | --- |
| `M3-S01` — CPT definition & lifecycle | Admin tạo CPT, lưu qua restart; catalog phân biệt Core/User/Theme/Plugin và giữ định nghĩa khi provider tắt. Nối plugin tối thiểu theo contract chỉ sau G2; **chưa yêu cầu E2E đổi theme M5** | CNT-01, PLG-03 bridge khi sẵn sàng | M2.5 verified; G1/G2 owner + persistence, compatibility với `home_page` |
| `M3-S02` — Field schema & entries | Field schema hỗ trợ đủ hành vi V1; Admin form nghiệm thu trước 7 biến thể không phụ thuộc media picker (chữ ngắn, chữ dài, rich text, số, toggle, ngày, lựa chọn); entry CRUD lưu qua restart. Bổ sung plugin field declaration bridge qua CNT-02 để S05 kiểm chứng CPT + fields. Public entry route nếu cần phải có G2 visibility/CSP/escaping | CNT-02, CNT-04, PLG-03 (field bridge) | M3-S01; G2 CNT-02/04 và PLG-03; MediaRef contract G2 trước khi khóa schema image/gallery |
| `M3-S03` — Taxonomy | Tạo category cha–con và tag; liên kết entry, query theo taxonomy, phân quyền | CNT-03, CNT-04 | M3-S01/S02 |
| `M3-S04` — Media library & picker | Upload/serve ảnh/video an toàn trong uploads, DB metadata và reference ổn định; tích hợp picker cho image/gallery, test restart/missing-file | CNT-05, CNT-02 | G2 MediaRef và upload security; picker cần schema CNT-02 từ M3-S02 |
| `M3-S05` — Cross-module verification | E2E **plugin** khai báo CPT/field qua cầu nối đã duyệt → entry/media/taxonomy → disable/restart/re-enable; schema/data vẫn quản lý được; quyền và migration test trên DB copy | CNT-01–05, PLG-03 | M3-S01–S04; **theme switch E2E thuộc THM-03 ở M5** |

**Lưu ý:** S01–S05 là phân rã dự kiến của giai đoạn G0/G2 trước đây, **không ánh xạ 1:1** với những slice M3.1–M3.6 đã code. Xem [đối chiếu M3](M3_STATUS_2026-11-09.md) trước khi tạo Issue mới; phần chưa hoàn thiện vẫn phải qua gate liên quan.

S02 và phần backend upload của S04 có thể nghiên cứu/thi công độc lập **sau khi** chốt MediaRef; phần picker tích hợp phải đi sau schema field S02. Thay đổi HTTP/upload/migration thuộc **Full-Track**. Các ID slice là nhãn đề xuất, không phải task `READY` hay capability được verified.

**Ranh giới bridge M3:** S01 cung cấp plugin CPT registration; S02 cung cấp field registration qua CNT-02; S05 nghiệm thu cả hai cùng entry/media/taxonomy, bao gồm sửa và Lưu nội dung khi plugin đã tắt. PLG-03 chỉ làm adapter, các domain giữ schema/validation/data. Trước khi khóa migration liên quan hoặc giao task phụ thuộc, mở brief CNT-02/CNT-04/PLG-03 ở mức giao diện cần thiết và chốt G2 tương ứng; không cần thiết kế toàn bộ extension M6 ngay. Blocks/templates/settings của plugin tiếp tục thuộc các mốc sau.

**Định hướng G1 — chưa là G2:** cân nhắc `post`/`page` do Core quản lý (giữ route `/` và `home_page` M1), plugin manifest khai báo CPT tùy chọn (không tự đổi Wasm ABI), và opaque `media_id` độc lập URL/path. G2 phải quyết định riêng schema/migration và seed/idempotency, owner/provider status qua restart, manifest validation và lifecycle, media file staging/cleanup, reserved URL namespaces, cách hiển thị public entry và policy backup. **Migration đã apply không thể rollback chỉ bằng Git revert**; cần backup DB và forward migration phù hợp.

### M4 — Builder (dự kiến)

`M4-S01` layout AST, core element descriptors & validation → `M4-S02` tree/canvas/inspector + Heading/Text/Image/Button → `M4-S03` responsive preview → `M4-S04` field binding/list/pagination → `M4-S05` persistence, Save và regression với dữ liệu cũ. Mỗi slice phải có test với page thật thay vì chỉ test shape của AST.

### M5–M8 — Theo dõi ở cấp capability

Giữ ở cấp module và gate trong [catalog](MODULE_CATALOG_V1.md). Chỉ chia slice/task chi tiết khi module gần đến lượt research; đặc biệt không khóa sớm mô hình cache/artifact hoặc cơ chế merge theme trước khi có thử nghiệm và số liệu.

**Pipeline public delivery V1 cần tách ownership:** CNT-04 resolve request/context (phối hợp CNT-03), SITE-02 xử lý URL/SEO/canonical, THM-02 chọn template + layout, BLD-01/05 render + binding/diagnostics; hợp đồng HTTP/precedence phải qua G1/G2. SITE-01 sở hữu engine settings/options dùng chung; **slice tối thiểu phải đi trước Theme Options M5**, các màn site settings còn lại có thể hoàn tất tại M7.

### Nghiệm thu xuyên module cần đưa vào các brief khi mở

Các mục dưới đây cụ thể hóa PRD và dependency, không chốt schema/API G2 hay tạo thêm module. Khi chia slice, gắn mỗi capability với một primary acceptance slice theo Planning Workflow.

| Mốc | Owner phối hợp | Hành vi phải nghiệm thu |
| --- | --- | --- |
| M5 — Public delivery | CNT-04, SITE-02, THM-02, BLD-01/05 | Tạo bài/CPT và dữ liệu field → gán template single/archive, header/footer → sửa và preview chưa lưu không đổi trang public → Lưu → nội dung hiện ở đúng URL và archive/list có phân trang. URL policy tối thiểu (collision, reserved namespace, precedence, visibility) phải chốt trước route/binding phụ thuộc; SEO đầy đủ vẫn ở M7. |
| M5 — Theme declarations & sample import | THM-01/03, CNT-01/02/03, CNT-05, SITE-01 | Theme ZIP định nghĩa sẵn CPT/fields/taxonomy qua domain APIs; người dùng chọn nhập trang/menu/ảnh/cấu hình mẫu mà không ghi đè nội dung hiện có. Phần menu/settings tối thiểu dùng trong import cần có ở M5; hoàn thiện màn hình site settings/navigation ở M7. |
| M5 — Bảo toàn thiết kế | THM-03, THM-02, BLD-01/02/03 | Phân biệt thiết kế mặc định của theme với layout người dùng đã sửa. Sau đổi/cập nhật theme, dữ liệu và layout còn, mở lại chỉnh sửa được; chỉ thay bằng mặc định mới khi người dùng chủ động chọn. Thiếu block/template phải có thông báo Admin và fallback public an toàn, không xóa node hoặc dữ liệu để che lỗi. Cơ chế lưu/merge cụ thể qua G2. |
| M6 — Plugin permissions & recovery | PLG-01/02/03/04 | Hiển thị quyền yêu cầu, tự áp dụng quyền hợp lệ đã khai báo, không thêm bước duyệt quyền; chặn thao tác ngoài quyền. Plugin lỗi/timeout có log, vô hiệu hóa được qua Admin, phần website độc lập vẫn hoạt động; dữ liệu giữ lại vẫn quản lý được. |
| M7 — Contact form | SITE-03, BLD-02/03, SITE-01 | Người dùng tạo form với trường cơ bản, đặt qua builder; khách gửi form, submission xem được trong Admin và thông báo gửi qua SMTP đã cấu hình. Kiểm cả validation và SMTP lỗi; chính sách lưu/thông báo lỗi phải rõ, không chỉ nghiệm thu form cố định. |

**Kiểm chứng vận hành sớm:** OPS-02 phối hợp FND-04/CNT-05 chạy build và smoke Linux cho binary, migration/restart, upload/serve và hành vi path/filesystem khi M3 bắt đầu triển khai media; ghi môi trường, lệnh, kết quả và giới hạn trong verification. Nếu chưa có môi trường thì ghi `NOT_RUN` và rủi ro còn mở, không suy diễn từ Windows. M8 vẫn là mốc nghiệm thu phân phối và hồi quy Linux toàn V1.

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

Theo PRD: thương mại điện tử, AI dựng trang từ ảnh, draft/publish riêng, revision history, repeater/relation, reusable blocks tùy ý, advanced visitor filters, UI plugin tùy ý, scheduled tasks/backup, website đa ngữ và chuyển ngôn ngữ Admin. Các yêu cầu này chỉ mở ở roadmap sau V1 khi chủ dự án quyết định.
