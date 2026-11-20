# Builder template và content — yêu cầu ngày 19/11/2026

**Nguồn:** chủ dự án xác nhận trong chat với Atlas ngày 19/11/2026; bổ sung
Page layout và cấu trúc theme ngày 20/11/2026.
**Trạng thái:** yêu cầu sản phẩm đã xác nhận; [resolver G2](../contracts/BUILDER_TEMPLATE_RESOLVER.md)
và [ADR-0020](../decisions/ADR-0020-builder-template-resolver.md) đã được Atlas chốt
trong working tree, có hiệu lực contract sau merge. API/schema toàn bộ tính năng
và G3 chưa hoàn chỉnh; chưa IMPLEMENTED/VERIFIED. Coordinator tài liệu: Atlas.

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

## Hai phạm vi Builder

| Phạm vi | Đối tượng | Hiệu lực của chỉnh sửa |
| --- | --- | --- |
| Template/thành phần dùng lại | Header, footer, archive, mẫu single, section… | Các nơi sử dụng template/thành phần theo cơ chế áp dụng của nó |
| Content bản ghi | Nội dung một Page/Post/CPT đăng ký hỗ trợ Builder | Chỉ bản ghi đang sửa |

Mẫu single quy định bố cục chung và vị trí đưa content của bản ghi vào. Ví dụ,
mẫu single Dự án dùng cho nhiều dự án; Builder content của Dự án A chỉ dựng
phần content của A, không sửa mẫu single hay Dự án B. Header/footer được ghép
một lần. Mở Builder từ màn sửa bản ghi phải giữ đúng loại nội dung và ID,
không chuyển sang sửa template single toàn cục hay homepage độc lập.

Section thuộc phạm vi thành phần dùng lại. Cơ chế hiện đã chốt là sao chép AST
khi chèn; sửa Section nguồn không tự thay các bản sao đã chèn. Phân định này
không bổ sung synced propagation hoặc thay contract Section hiện hành.

## Đăng ký Builder cho content

Khả năng dùng Builder phải được khai báo ở loại nội dung, áp dụng cho built-in
Page/Post và CPT khi có đăng ký phù hợp. Hỗ trợ trình soạn thảo thông thường
và hỗ trợ Builder là hai khả năng riêng; có template single không có nghĩa
CPT tự được mở Builder content. UI và backend cùng kiểm tra cấu hình hiệu lực,
quyền của người dùng và target bản ghi; ẩn nút không thay thế kiểm tra backend.

Khi dùng Builder, dữ liệu form/trình soạn thảo và layout đã có phải được giữ.
Không tự chuyển văn bản thành AST hay ghi AST thành văn bản. Lựa chọn nguồn
content hiển thị và chuyển chế độ phải rõ ràng; mất đăng ký hỗ trợ, tắt plugin
hoặc đổi theme không được xóa dữ liệu hay âm thầm ghi đè nguồn còn lại.
Cách lưu lựa chọn, mặc định của từng built-in và hành vi fallback cần chốt G2.

Builder content tuân theo workflow của bản ghi: draft vẫn riêng tư sau Save;
chỉ Xuất bản mới công khai với loại dùng draft/publish. Save nội dung đã công
khai cập nhật đúng bản ghi đó. Preview chưa lưu không sửa DB/public hoặc cấp
quyền media. Template dùng chung giữ quyền quản trị riêng; quyền chỉnh content
một bản ghi không cấp quyền sửa template toàn cục.

## Template hierarchy và nguồn template

**Định nghĩa chủ dự án chốt:** `home` = trang chủ; trang Blog là archive của
`post`, chọn `archive-post` nếu có, nếu không dùng `archive` mặc định. Không
có role `blog` hoặc `front-page`. Mọi loại nội dung đều có nhánh `single`
và `archive` theo type; riêng Page mặc định dùng template `page` và có thể
chọn Page layout khi chỉnh sửa theo bổ sung 20/11 bên dưới. `archive-post` là tùy biến theo type của
`archive`, không là role riêng. Có template không tự bật public/has_archive hoặc đăng ký Builder
content: đó là những khả năng và quyền độc lập.

Theo chỉ đạo bổ sung cùng ngày, dùng mô hình phân cấp template của WordPress
làm tham chiếu cho cách chọn template. Ảnh hierarchy do chủ dự án cung cấp là
tham chiếu hành vi; OpenDash giữ Rust, SQLite, template engine và Builder AST
hiện hành, không chạy PHP hoặc nhận gói theme WordPress.

Cần phân biệt hai trục: **template nào phù hợp với request** (cụ thể đến tổng
quát) và **nguồn của template** (bản Builder trong database hay bản của theme
đang active). Bản tùy chỉnh lưu database được ưu tiên so với bản theme tương
ứng. [Bảng resolver G2](../contracts/BUILDER_TEMPLATE_RESOLVER.md) chốt xét từng
candidate từ cụ thể đến tổng quát, tại cùng candidate xét database rồi active
theme. Override chung không thắng template cụ thể chỉ vì nằm trong database. Header/footer là template parts, được template sử dụng;
không phải một nhánh fallback của single/archive.

Bảng tóm tắt dưới đây dùng tên logic, không là tên file/API. Thứ tự đầy đủ,
selector stable ID, nguồn và ngoại lệ theo contract resolver G2:

| Ngữ cảnh | Hướng chọn template |
| --- | --- |
| Một bản ghi Post/CPT ngoài Page | Template được gán rõ → single theo type/bản ghi → single theo type → single → singular → index |
| Page thông thường | Page layout được chọn → page → index |
| Archive CPT | archive theo CPT → archive → index |
| Taxonomy | Template theo taxonomy/term → taxonomy → archive → index; category/tag có nhánh riêng |
| Trang đầu website | home → nhánh Page layout/page của Page Trang chủ được chọn |
| Page được chọn làm Trang bài viết | archive-post → archive → index; phân biệt với content Page và home |
| Search/404 | search/404 → index khi chức năng/route tương ứng được hỗ trợ |

Sơ đồ có cả attachment, date/author archive và embed. Sơ đồ không tự chứng minh
OpenDash đã có các route này hoặc tự cấp scope triển khai chúng. G1 phải kiểm
kê route thực tế, G2/G3 phân định rõ nhánh được làm và nhánh chưa hỗ trợ; không
ghi WordPress parity hoặc placeholder là đã hoạt động.

Admin phải phân biệt template/part từ theme với bản tùy chỉnh Builder trong
database, nêu target và nguồn hiệu lực. Lưu Builder ghi bản tùy chỉnh, không
sửa file theme. Cho phép tạo template/part tùy chỉnh; lựa chọn gán/điều kiện áp
dụng phải rõ. Thao tác dùng lại bản theme cần xác nhận khi bỏ tùy chỉnh/nháp.
Cập nhật/đổi theme giữ các bản đã chỉnh và content bản ghi; contract resolver xác định
identity, tương thích, cách áp dụng lại và fallback khi theme/block không còn.
Các quyết định resolver này đã chốt tại contract G2; G3 phải hiện thực đúng matrix.
Không tự áp layout không tương thích sang theme khác hoặc xóa dữ liệu cũ.

## Cấu trúc theme và Page layout — đã xác nhận 20/11/2026

Các template chính đặt ngay root gói theme, gồm cả header/footer; không có
thư mục templates hoặc parts cho nhóm này. Cấu trúc đích:

```text
my-theme/
├── theme.json
├── index.html
├── home.html
├── page.html
├── single.html
├── single-post.html
├── archive.html
├── archive-post.html
├── taxonomy.html
├── 404.html
├── header.html
├── footer.html
├── template-part/
│   ├── post-card.html
│   ├── breadcrumbs.html
│   └── pagination.html
├── page-layouts/
│   ├── full-width.html
│   ├── sidebar.html
│   └── canvas.html
├── sections/
│   └── hero.json
└── assets/
    ├── styles.css
    └── images/
```

Các file ví dụ không mặc định là bắt buộc hoặc bằng chứng đã có route tương
ứng. Theme phải đăng ký tài nguyên qua manifest đã kiểm tra; không tự execute
mọi file trong thư mục. Manifest/DTO và package validator của format mới phải được chốt trước
triển khai; không cần nhận hoặc chuyển đổi các format theme thử nghiệm cũ.

| Nhóm | Vai trò |
| --- | --- |
| Root HTML | Template chính và header/footer; header/footer vẫn là parts về ngữ nghĩa dù file đặt root |
| template-part/ | Template thành phần nhỏ: card, breadcrumbs, pagination…; core composition có kiểu, không bật include/script tùy ý |
| page-layouts/ | Layout có tên cho màn sửa Page, không đổi content hoặc publication của Page |
| sections/ | Section Builder JSON do theme cung cấp, liên kết với thư viện Section trong Admin |
| assets/ | CSS, ảnh và tài nguyên tĩnh được package validator cho phép |

**Page layout:** Page mặc định dùng page.html. Trong sidebar màn sửa Page,
người dùng chọn Mặc định hoặc layout do theme đăng ký (ví dụ Toàn chiều rộng,
Có sidebar, Canvas). Canvas không ghép header/footer; các layout khác khai báo
parts rõ, không ghép trùng. Content Page vẫn từ editor/Builder theo cấu hình;
đổi layout không convert hoặc xóa content. Mất layout thì giữ reference/content,
báo Admin và fallback page → index → core. Đây là bổ sung thay quyết định cũ
Page luôn dùng single theo type page. Page Blog vẫn dùng archive-post → archive
→ index; lựa chọn layout Page thông thường không thay archive bài viết.
Home template vẫn đứng trước nhánh Page khi Page được chọn làm Trang chủ.

**Section theme ↔ Admin:** theme cung cấp bản gốc trong sections/; Admin hiển
thị/mở chỉnh qua Section hiện hành và lưu bản tùy chỉnh site-owned vào database.
Dùng ID ổn định có provenance theme để nhận biết cùng Section khi cập nhật,
không chiếm ID hoặc ghi đè Section người dùng đã tạo. Cập nhật theme giữ bản
người dùng đã sửa; dùng bản theme mới là thao tác chủ động. Lưu Admin không
sửa file immutable theme revision, đổi theme không xóa Section đã lưu. Schema,
import/update/CAS, media remap, collision và export nếu có cần G2/G3 riêng.
Chèn Section vẫn COPY AST với node IDs mới; không bổ sung việc sửa Section
nguồn tự lan tới mọi trang đã chèn. Không triển khai watcher đồng bộ file nền.

**Block cho Builder — để thảo luận sau:** chủ dự án chưa chốt cách developer
khai báo/viết custom block. Ví dụ custom-title, descriptor/renderer/CSS và thư
mục blocks đã trao đổi chỉ là gợi ý, chưa là schema, SDK hoặc contract được
duyệt; chưa đưa vào phạm vi implementation. Giữ yêu cầu theme/plugin có thể
cung cấp block trong PRD, không loại chức năng này khỏi V1.

## Cài đặt Trang chủ, Trang bài viết và dữ liệu hiện có

Admin trong **Cài đặt** có hai lựa chọn độc lập bằng Page ID:

- **Trang chủ:** chọn một Page để làm homepage tại `/`. Page có trong danh
  sách Trang; chỉnh content mở đúng bản ghi, editor/Builder theo đăng ký hỗ trợ.
  Template bao ngoài chọn theo home rồi nhánh Page layout/page.
- **Trang bài viết (Blog):** chọn một Page khác để đại diện archive của
  built-in `post` tại URL của Page đó. Dùng archive-post nếu có, nếu không dùng archive mặc định, không
  dùng body hoặc Builder content của Page để thay danh sách bài. Page vẫn có
  trong danh sách Trang; content sẵn có giữ nguyên khi gán/bỏ gán vai trò Blog.

Gán Page không tự xuất bản. Hai Page phải khác nhau; Page được chọn cần tồn
 tại, public và published khi áp dụng. Thay đổi trạng thái/xóa sau đó không
được làm lộ draft hoặc âm thầm chọn Page khác. Route claims, canonical/redirect,
sitemap/SEO và thao tác đổi/bỏ chọn theo một route authority; chi tiết API/schema
phải chốt trước triển khai.

Không convert homepage thử nghiệm hiện tại thành Page content mới. Admin tạo
và chọn Page theo mô hình đích. Không có adapter, migration compatibility hoặc
chế độ legacy. Format/schema/API thử nghiệm không phải baseline sản phẩm cũ
cần hỗ trợ; fixtures và kiểm chứng sẽ theo baseline mới.

## Quan hệ với contract hiện hành

- [Admin review](../contracts/ADMIN_REVIEW_2026_11.md) đã chốt Page Builder
  theo entry scope. Đây là nền tảng cần mở rộng cho các CPT đăng ký Builder,
  không phải bằng chứng generic CPT content Builder đã tồn tại.
- [Content foundation](../contracts/CONTENT_FOUNDATION_2026_11.md) C1 hiện
  chỉ có `supports: [editor,featured_image]`. Yêu cầu này cần contract delta
  cho đăng ký Builder; không tự thêm enum/API vào contract đã merge.
- Mô tả homepage lưu độc lập/cập nhật public ngay trong các ADR, G3 và receipt
  cũ giữ ý nghĩa lịch sử. Hierarchy/nguồn template và content bản ghi cần delta
  riêng, không thay bằng chứng thực chạy hoặc tự mở rộng acceptance task cũ.

## Nghiệm thu cần đưa vào G3

1. Cài đặt cho Admin chọn hai Page khác nhau làm Trang chủ và Trang bài viết;
   cả hai có trong danh sách Trang. Homepage dùng đúng Page content; Blog dùng
   archive post, giữ body Page. Không tự tạo/convert Page từ homepage legacy.
2. CPT bật Builder mở đúng content bản ghi; CPT không hỗ trợ bị chặn ở cả
   UI/backend. Kiểm tra Page/Post và một CPT tùy chỉnh, không hardcode Page.
3. Sửa content A không đổi B, template single, header/footer; sửa template
   chung áp dụng đúng phạm vi mà không ghi đè content của từng bản ghi.
4. Draft/preview/media giữ riêng tư, kể cả Page trang chủ; Save bản đã xuất
   bản cập nhật đúng public destination. Kiểm quyền Admin/Editor, CSRF,
   stale CAS, 401/412 và bảo toàn thay đổi chưa lưu khi chuyển target.
5. Đổi chế độ soạn thảo, tắt hỗ trợ Builder/provider, đổi theme và restart
   giữ dữ liệu/layout. Chèn Section tiếp tục là bản sao độc lập.
6. Hierarchy chọn đúng template theo context/scope/nguồn, có fallback rõ;
   UI phân biệt theme/database. Save/reset template không sửa file theme hay
   content bản ghi. Upgrade/backup/restore giữ homepage hiện hữu, không tạo
   Page ngoài ý muốn; kiểm canonical/redirect/sitemap/SEO khi thay resolver.

Các mục này là yêu cầu cho thiết kế/triển khai tiếp theo, chưa cấp Project
Ready, đổi ownership của task đang chạy hoặc nghiệm thu Beta.

## Nguồn tham chiếu

- [WordPress Template Hierarchy](https://developer.wordpress.org/themes/templates/template-hierarchy/)
- [WordPress Templates](https://developer.wordpress.org/themes/templates/templates/)
- [WordPress Template Parts](https://developer.wordpress.org/themes/templates/template-parts/)

Các nguồn này giải thích WordPress; quyết định OpenDash nằm trong contract
resolver G2, không là bằng chứng implementation của OpenDash.
