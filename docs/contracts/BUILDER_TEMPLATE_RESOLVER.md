# Builder template resolver — G2, 19/11/2026

**Decision owner:** Atlas, theo yêu cầu chủ dự án “chốt bảng”.
**Trạng thái:** Final Decision G2 cho resolver; có hiệu lực làm contract sau
PR merge. Chưa là G3/API/schema hoàn chỉnh, implementation hay runtime verification.
Nguồn sản phẩm: [Builder template và content](../product/BUILDER_TEMPLATE_AND_CONTENT.md).
Quyết định: [ADR-0020](../decisions/ADR-0020-builder-template-resolver.md).

## Chỉ đạo chủ dự án — không tương thích ngược, 20/11/2026

OpenDash đang phát triển, chưa có bản sản phẩm cũ cần hỗ trợ. Thiết kế mới
không phải giữ format theme/API/schema cũ, adapter legacy, dual-read/dual-write,
converter hay lộ trình upgrade từ các bản thử nghiệm. Chốt một format/API/schema
đích thống nhất; cập nhật source, fixtures và kiểm chứng theo cùng baseline mới.
Những điều khoản tương thích ngược trong tài liệu lịch sử không ràng buộc
phạm vi thiết kế mới này. Không phát sinh task compatibility riêng.

Điều này không bỏ yêu cầu sản phẩm bảo toàn content/layout khi người dùng
đổi/cập nhật theme, hay quyền, publication, backup/restore trong mô hình mới.
Không tự xóa dữ liệu hoặc thư mục hiện có để thực hiện quyết định thiết kế.

## 1. Một resolver, hai trục ưu tiên

### Định nghĩa role do chủ dự án chốt

| Role | Ý nghĩa |
| --- | --- |
| `home` | Template trang chủ, dùng Page Trang chủ được chọn trong Cài đặt |
| `single` theo type | Template một bản ghi Post/CPT ngoài nhánh Page chuyên biệt |
| `page` | Template mặc định cho Page; Page layout được chọn rõ đứng trước page |
| `archive` theo type | Template danh sách mặc định của mọi CPT; tùy biến theo type, ví dụ `archive-post` cho bài viết |
| `header` / `footer` | Template parts dùng chung |

Không có role `front-page` hoặc `blog` trong mô hình đích. `home` chỉ dùng cho trang chủ.
Bổ sung 20/11/2026: Page mặc định dùng role `page`, cho chọn layout từ
page-layouts/ theo [đặc tả sản phẩm](../product/BUILDER_TEMPLATE_AND_CONTENT.md).
Thay quyết định trước đó Page chỉ dùng single + type page. Override Page body
hiện hữu vẫn là content của bản ghi, không tự chuyển thành template dùng chung.
Mọi CPT có hai nhánh template single/archive; `public`, `has_archive`, trạng
thái provider và publication vẫn kiểm soát route có được phục vụ hay không.
Section/private CPT không tự có URL public vì có nhánh template.


Core xác thực route, publication, visibility và quyền trước khi chọn template.
Template không tạo route, không làm entry/term private trở thành public, không
query database hoặc tự ghép URL. Tiếp tục dùng route authority của
[Content foundation](CONTENT_FOUNDATION_2026_11.md), canonical registries,
storage APIs, MiniJinja/CSP/AST validators và CAS hiện hành.

Resolver tạo danh sách candidate có thứ tự theo bảng dưới. Với **từng candidate**,
tìm bản site-owned Builder trong SQLite trước, rồi bản trong active theme
revision; nếu không có cả hai thì sang candidate kế tiếp. Không quét toàn bộ
database rồi mới quét toàn bộ theme. Một candidate chọn tối đa một nguồn.
Preview chưa lưu chỉ phủ lên đúng candidate/target của phiên preview hợp lệ,
không ảnh hưởng resolver public hoặc ghi DB.

Tên trong bảng là **khóa logic**, không phải đường dẫn, tên file cần nối từ
slug hoặc schema DTO đã tồn tại. Identity được biểu diễn bằng các trường có
kiểu (kind, CPT/taxonomy key, stable entry/term ID hoặc selector slug). Không
parse khóa bằng cách tách dấu gạch nối; không để slug thành filesystem path.
G3 ánh xạ khóa logic vào một manifest/storage schema đích; không cần hỗ trợ format thử nghiệm cũ.

## 2. Bảng candidate, trái sang phải

| Ngữ cảnh | Thứ tự candidate |
| --- | --- |
| Một bản ghi Post/CPT ngoài Page | Custom template được gán cho bản ghi → single theo `(type, entry ID)` → single theo `(type, slug)` → single theo type → single chung → singular → index |
| Page thông thường | Page layout đã gán hợp lệ → page → index; mất layout giữ reference, báo Admin, fallback page |
| Archive một CPT | archive theo type → archive chung → index |
| Custom taxonomy term | taxonomy theo `(taxonomy key, term ID)` → taxonomy theo `(taxonomy key, term slug)` → taxonomy theo key → taxonomy chung → archive chung → index |
| Category core | category theo term ID → category theo slug → category chung → archive chung → index |
| Tag core | tag theo term ID → tag theo slug → tag chung → archive chung → index |
| Trang chủ tại `/`, Page được chọn trong Cài đặt | home → nhánh Page layout/page của Page trang chủ |
| Page được chọn làm Trang bài viết | archive-post → archive → index; context archive post, không chạy nhánh single của Page hoặc lấy body Page làm nội dung archive |
| Chưa chọn Page trang chủ hoặc Trang bài viết | Trạng thái chưa cấu hình theo mục 6; không tạo Page hoặc suy diễn từ homepage legacy |
| Request không tìm thấy | 404 → index có context NotFound; HTTP vẫn 404, không render entry khác |

Custom template phải có identity đã đăng ký, đúng kind và cho phép type đang
đọc; chọn bằng reference hợp lệ, không nhập filename tùy ý. Nếu reference
không còn khả dụng, giữ reference và dữ liệu, báo Admin rồi tiếp tục nhánh
thông thường; không tự chọn một custom template khác. Stable ID ưu tiên slug
để thiết kế gắn riêng với bản ghi/term không bị đổi khi đổi slug. Đây là lựa
chọn OpenDash, không tuyên bố sao chép từng chi tiết WordPress.

Search, author/date archive, attachment page và embed chưa thuộc lần triển
khai resolver này. Không sinh các route đó chỉ vì có template; khi có task
được duyệt, phải bổ sung context, candidate, ACL và acceptance riêng.
Archive mọi CPT, kể cả Page, theo cờ has_archive; có template không tự bật archive.
Child theme và PHP template không nằm trong phạm vi.

Core kiểm tra các Page được chọn tồn tại, public và published trước resolver.
Chọn Page không tự xuất bản. Nếu Page đã chọn bị draft/private/xóa thì URL
chức năng tương ứng trả 404/noindex; không dùng template để vượt kiểm tra,
không tự đổi sang Page khác hoặc nội dung homepage legacy.

## 3. Nguồn, identity và đổi theme

| Đối tượng | Identity và hành vi |
| --- | --- |
| Bản theme | `(theme_id, revision_id, template/part key)`; file revision bất biến |
| Bản tùy chỉnh | Site-owned, target bằng khóa logic/candidate; không đổi owner thành theme; base theme/revision là provenance |
| Tạo từ theme | Lưu một bản site-owned có cùng target; không sửa file theme |
| Đổi/cập nhật theme | Giữ tùy chỉnh site-owned và content; vẫn xét override đó tại đúng candidate. Không tự rebase/ghi đè AST bằng mẫu mới |
| Block/provider không còn | Giữ node/data để round-trip; áp dụng chính sách missing block hiện hành, diagnostic ở Admin; không chạy provider thiếu hoặc xóa node ngầm |
| Template cũ chỉ nằm trong theme không active | Không dùng file revision không active làm template mặc định; reference còn được giữ để sửa hoặc áp dụng lại rõ ràng |
| Không có template hợp lệ cuối nhánh | Core fallback theo đúng context và trạng thái HTTP; không chuyển sang entry/type khác |

Override site-owned có hiệu lực xuyên theme theo đúng target là chủ đích bảo
toàn tùy chỉnh, không là tự sao chép thiết kế theme A sang theme B. Chỉ áp
dụng phần còn hợp lệ với registry/validators hiện hành; không nới quyền hoặc
validator để cố render. Package/AST hỏng hoặc vi phạm bảo mật không được coi
là “không tồn tại” để bỏ qua lỗi: trả diagnostic, dùng core safe fallback
theo context; nếu fallback không dựng được thì generic noindex 503. Dữ liệu
gốc giữ nguyên. Missing provider node hợp lệ khác với tài liệu hỏng.

Ví dụ bắt buộc: theme có single theo CPT Dự án, DB chỉ có single chung →
dùng template Dự án của theme. Khi DB có single Dự án → dùng bản DB đó.
Custom template đã gán hợp lệ đứng trước cả hai. Thay content của Dự án A
không tạo candidate template riêng hoặc thay template Dự án.

## 4. Template parts, Section và vùng content

Header/footer giải quyết riêng theo part key mà template chọn: DB part cùng
key → part cùng key trong active theme → core fallback hiện hành của part.
Không âm thầm thay part tùy chỉnh bị thiếu bằng một part khác tên. Template
không khai báo part thì không tự chèn thêm. Renderer đích ghép header/footer
một lần; không cần adapter cho renderer thử nghiệm trước đó.
Chỉ core thực hiện composition có kiểu; không thêm MiniJinja include/raw HTML
hoặc cho phép provider vượt C-SEC. G3 phải định nghĩa typed part references,
giới hạn nesting/cycle/depth trước triển khai, không cho recursive include tự do.

Template cho entry phải có đúng một vị trí content do core kiểm soát. Content
đến từ lựa chọn editor/Builder của **bản ghi đã xác định**, được kiểm tra
publication, field ACL, media và đăng ký hỗ trợ. Template không ghi đè content
và content Builder không thay template/parts. Front-page không có entry hoặc
archive không bắt buộc có entry-content slot; dùng context và danh sách
được phép của chính nó. Preview draft không cấp public media.

Section thư viện tiếp tục COPY AST với IDs mới khi chèn; không đi qua cơ chế
live template-part reference và không bổ sung synced Section propagation.

## 5. Lưu và “Dùng lại bản theme”

| Thao tác | Kết quả bắt buộc |
| --- | --- |
| Lưu template/part | Admin + guard hiện hành; CAS đúng target; ghi DB override, không ghi revision files/content entry |
| Dùng lại bản theme | Nêu chính xác override sẽ bỏ và template/part hiệu lực sau đó; xác nhận bỏ tùy chỉnh/nháp; chỉ reset candidate hiện tại |
| Theme không có bản tương ứng | Không gọi thao tác là dùng lại theme. Cho phép “Bỏ tùy chỉnh, dùng fallback” riêng, hiển thị fallback thực tế trước xác nhận |
| Candidate thuộc entry content | Không được gọi reset template để xóa AST content. Content reset/chuyển editor là thao tác riêng theo contract content |
| Theme hoặc override đổi trong lúc xác nhận | Kiểm CAS của override và active-theme selection/revision trong cùng transaction; stale trả 412, không reset nhầm; giữ nháp để tải lại |

Reset bỏ override theo hành động rõ ràng, không xóa các override cấp khác,
không đổi active theme, publication hoặc dữ liệu content. Public request sau
commit resolver lại có thể chọn fallback DB ở mức tổng quát hơn; UI phải nói
đúng nguồn này, không hứa luôn thấy theme. Không thêm hệ revision history/undo
trong quyết định này. Preview/GET không gây reset hay chuyển nguồn.

## 6. Cài đặt Trang chủ / Trang bài viết

Chủ dự án làm rõ: Admin trong **Cài đặt** chọn độc lập hai Page:

| Cài đặt | Vai trò và hiển thị |
| --- | --- |
| Trang chủ | Một Page làm homepage tại `/`. Content thuộc Page đó, editor/Builder theo đăng ký hỗ trợ; template theo nhánh home rồi Page layout/page |
| Trang bài viết (Blog) | Một Page đại diện archive của built-in `post` tại URL canonical của Page đó. Dùng nhánh archive-post → archive → index; không lấy body hoặc Builder content của Page làm nội dung archive |

Hai lựa chọn là reference stable Page ID, không là slug/template key. Chốt
hai Page khác nhau để một Page không đồng thời là homepage content và archive
post. Admin mới đổi được cấu hình; server kiểm loại `page`, sự tồn tại và
publication/visibility, guards và CAS theo API settings chuẩn. Chọn Page chưa
published không tự publish; UI hướng dẫn xuất bản trước khi áp dụng.
Chỉ archive chứa các post đủ điều kiện public, giữ pagination/filter/count,
media ACL và SEO hiện hành. Page blog không đổi type thành `post`, không
chuyển content Page thành bài viết; title/metadata có thể dùng làm nhãn và SEO
của archive theo DTO đã được lọc. Body/AST Page vẫn được giữ nguyên khi gán,
đổi hoặc bỏ chọn; khi không còn là Page blog, Page trở lại nhánh Page layout/page.

Khi đã cấu hình, core route authority sở hữu claim `/` của Page trang chủ và
URL Page blog dưới dạng archive `post`, không tạo hai route trả hai nội dung
khác nhau ở cùng URL. Link Admin/menu/canonical/sitemap phải đồng ý với cùng
route authority. Quy tắc alias/redirect từ URL cũ, xung đột claim, bỏ chọn và
transaction của hai settings cần API/schema G2 riêng trước code; không ghép
URL hoặc xử lý redirect ở template resolver.

Khi chưa chọn Page, Admin báo chưa cấu hình; site mới không tự tạo Page hoặc
chọn theo slug. Không convert homepage thử nghiệm thành Page, không giữ
legacy runtime hoặc adapter. Không cần mapping/versioning tương thích gói cũ,
duy trì URL cũ hoặc upgrade từ format thử nghiệm. Chốt một format/schema/API
mới; fixtures và tests phải được cập nhật theo baseline đích. Không thay dữ
liệu demo để giả lập chức năng đã implemented; không tự xóa site hiện có.

## 7. Bằng chứng phải có trước nghiệm thu

Kiểm matrix candidate × DB/theme × target gồm: theme-specific thắng DB-generic;
DB cùng candidate thắng theme; custom assignment mất provider; ID/slug đổi;
reset chính xác và race theme/CAS; A→B→A giữ AST; missing block và corrupt
package khác nhau; entry content A/B độc lập; parts ghép đúng một lần; draft
và media không lộ; 404 giữ status; chọn hai Page trong Settings, role không trùng;
blog dùng archive post và giữ body Page; không convert homepage thử nghiệm;
restart và backup/restore theo baseline mới giữ dữ liệu. G3 cần exact file ownership,
dependencies, API/schema delta và live Project Ready trước code.

## 8. Evidence và phạm vi quyết định

Đọc source tại `cb76f21a2094ad8a1807addc1fe3187e8480475e`:
storage/overrides.rs hiện ưu tiên scope trong DB, routes/public.rs đọc homepage
riêng; M5 contract mục 3 ghi DB overrides trước theme. Remote main
`4fa8f57` sau fetch chỉ thêm thay đổi adapter/governance, không thay các source
template được đối chiếu. Chưa chạy runtime resolver mới.

Tham chiếu [WordPress hierarchy](https://developer.wordpress.org/themes/templates/template-hierarchy/)
và [template parts](https://developer.wordpress.org/themes/templates/template-parts/).
WordPress là nguồn tham khảo; precedence, stable ID, hai Page settings và giới
hạn ở đây là quyết định OpenDash. Lựa chọn DB trước mọi template theme đã bị
loại vì override chung sẽ che thiết kế cụ thể của CPT. Chọn theo hierarchy
trước, nguồn sau; giữ site-owned overrides xuyên theme để bảo toàn layout.

## 9. Cấu trúc gói theme và phần hoãn — bổ sung 20/11/2026

[Đặc tả cấu trúc theme](../product/BUILDER_TEMPLATE_AND_CONTENT.md) chốt file
HTML chính/header/footer tại root, template-part/ cho thành phần nhỏ,
page-layouts/ cho layout Page, sections/ cho Section liên kết Admin và assets/.
Đây là layout gói đích; reader/manifest/package v2 hiện hành chưa tự hỗ trợ
mọi khai báo mới. G2 package delta định nghĩa format/validator đích và typed
composition; không bật MiniJinja include hoặc script tùy ý.

Section theme dùng bản gốc và tùy chỉnh DB có ID/provenance ổn định, bảo toàn
chỉnh sửa khi cập nhật/đổi theme; không ghi ngược immutable files từ Admin và
không đổi COPY insertion thành live propagation. Chi tiết DTO/storage/import
còn cần chốt. Custom Builder block authoring/SDK/format chưa chốt, thảo luận
riêng sau; không coi ví dụ custom-title là contract triển khai.
