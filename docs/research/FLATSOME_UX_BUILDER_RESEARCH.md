# Nghiên cứu chuyên sâu: Flatsome UX Builder và khả năng áp dụng cho OpenDash CMS

> **Loại:** nghiên cứu đối chiếu sản phẩm, UX và kiến trúc kỹ thuật. **Ngày khảo sát:** 07/11/2026.
> **Trạng thái:** `RESEARCH / PROPOSED` — chưa được chủ dự án chốt thành ADR; không thay đổi stack, schema, ABI hoặc mã thực thi.
> **Đối tượng chính:** **Flatsome Classic 3.x / UX Builder**, với **Flatsome Site Builder (thế hệ block, giới thiệu 2026)** là đối chứng về hướng tiến hóa.
> **Phạm vi kiểm chứng:** tài liệu chính thức, demo công khai, ví dụ shortcode và thành phần đăng ký builder công khai; so sánh với source OpenDash tại repository hiện tại. Chưa mở một bản Flatsome Classic có license để ghi DOM/network trace hoặc test thao tác thực tế.

## 0. Câu trả lời dành cho quyết định sản phẩm

**Có thể và nên học UX Builder cho trải nghiệm người dùng OpenDash**, đặc biệt ở khung editor, cây Section → Row → Column → Element, sidebar Edit/Add, chọn phần tử trên preview, form sinh từ khai báo, thiết lập responsive, preset và UX Block. Mẫu hình này sát đối tượng người dùng đã quen WordPress và sát [yêu cầu V1](../product/PRODUCT_REQUIREMENTS.md).

**Không nên lấy shortcode làm định dạng gốc của OpenDash.** UX Builder Classic lưu cấu trúc dựng trang bằng chuỗi shortcode WordPress. Flatsome đã giới thiệu hệ Site Builder mới dựa trên native WordPress blocks, đồng thời cung cấp Migrator để chuyển UX Builder shortcode và UX Blocks cũ sang blocks/patterns. Đây là bằng chứng từ chính nhà sản xuất rằng cần có đường nâng cấp mô hình nội dung và xử lý nội dung thiếu thành phần. [F01] [F02] [F03] [F13]

**Hướng nghiên cứu đề xuất:** giao diện trực quan gần UX Builder + **document AST JSON có version, node ID ổn định và schema typed** + **React Admin** + **Rust validation/SSR (MiniJinja)** + SQLite sở hữu nội dung + **theme/plugin ZIP đăng ký element và template qua contract**. Chưa triển khai kiến trúc này chỉ vì báo cáo: cần prototype và quyết định hợp đồng trước khi mở rộng M4.

### 0.1. Các kết luận có thứ tự ưu tiên

| Ưu tiên | Điều nên học | Điều không nên kế thừa nguyên trạng | Kết quả cho OpenDash |
| --- | --- | --- | --- |
| P0 | Cây phần tử + panel lựa chọn/thêm/chỉnh thuộc tính | Cấu trúc nội dung dạng chuỗi shortcode | AST JSON có ràng buộc cha/con |
| P0 | Live preview, giữ phần sửa chưa lưu | Cho UI preview chạy một logic render khác public | Preview và public dùng chung renderer |
| P0 | Schema option của mỗi element, preset và điều kiện chèn | UI option hard-code cho từng loại block | Block Registry và schema-driven inspector |
| P0 | Responsive desktop/tablet/mobile | Phụ thuộc tên breakpoint/CSS class riêng Flatsome | Responsive props typed, quy tắc kế thừa rõ |
| P0 | UX Blocks / bố cục được tái sử dụng | Khóa dữ liệu vào theme/shortcode có thể mất renderer | Giữ dữ liệu site-owned và placeholder |
| P1 | Studio: thư viện section/layout mẫu | Nhập mẫu tự ghi đè trang hiện có | Import **copy một lần**, preview và xác nhận |
| P1 | Dựng trang từ ngữ cảnh nội dung thực | Các element WooCommerce đặc thù | CPT + typed field binding + collection query |
| P1 | Chỉnh được header/footer/templates | Header builder tách logic không đồng bộ với page builder | Một AST và hệ template chung, nhiều context |
| P1 | Extension element theo schema | PHP hook không sandbox và shortcode PHP tùy ý | Theme declarative + plugin Wasmi đã chốt |
| P2 | Hiệu ứng slider, banner, hotspot, focus point | Sao chép tất cả hiệu ứng trước khi core ổn | Element nâng cao triển khai tăng dần |
| Sau V1 | Synced patterns, biến thể, thư viện có override | Phức tạp hóa giao thức lưu đồng thời ngay V1 | Ghi nhận hướng nâng cấp, chưa đưa vào V1 |

### 0.2. Phân loại bằng chứng và giới hạn

- **[OFFICIAL]**: tài liệu UX Themes / Flatsome và demo thuộc đơn vị phát triển; xác nhận **hành vi công bố**, không tự xác nhận cấu trúc nội bộ chưa công khai.
- **[EXAMPLE]**: shortcode mẫu do UX Themes phát hành; xác nhận cú pháp/ngữ nghĩa ở trường hợp được nêu.
- **[MIRROR]**: một số file PHP/Angular hiển thị trong GitHub mirror do bên thứ ba đăng, **không phải repository chính thức**, không pin được chính xác bản 3.x hiện hành. Chỉ dùng để hiểu *kiểu giao diện đăng ký và pipeline ở một bản lịch sử*, không khẳng định source hiện hành hoạt động y hệt; tuyệt đối không nhập mã đó vào OpenDash.
- **[INFERENCE]**: hệ quả kỹ thuật suy luận từ nguồn được gắn nhãn; không gán thành chức năng đã xác nhận của Flatsome.
- **[PROPOSAL]**: thiết kế riêng cho OpenDash để cân nhắc/kiểm chứng. Ví dụ schema/API dưới đây là minh họa, **chưa phải contract chính thức**.

Nghiên cứu này **không bao gồm**: bản đo CPU/RAM/TTFB của UX Builder hay OpenDash, kiểm tra license mã Flatsome tại máy khách, kiểm thử editor chính chủ, capture traffic Save/Preview hoặc phân tích toàn bộ bundle Angular. Không có căn cứ để tuyên bố tốc độ, undo stack, tính chất autosave hay isolation của Flatsome từ bên trong.

## 1. Phải phân biệt hai thế hệ Flatsome

### 1.1. Flatsome Classic 3.x với UX Builder

UX Builder là công cụ dựng nội dung front-end của theme Flatsome Classic trên WordPress. Trang demo chính thức mô tả:

1. Thêm/kéo thả các layout elements và content elements.
2. Sắp xếp lại trực tiếp trong sidebar hoặc trên vùng nội dung.
3. Sửa text và options rồi thấy thay đổi tức thời.
4. Đổi chế độ desktop/tablet/mobile và sử dụng option theo thiết bị.
5. Dùng Section, Row, Column, Banner, Slider, hình ảnh, nút, các element shop.
6. Lưu nội dung dựng trang dưới **shortcodes** có thể nhìn thấy trong WordPress editor thường.
7. Trang giới thiệu lịch sử gọi ứng dụng editor là **Angular**. Điều này chỉ xác nhận kỹ thuật thuộc Classic; không suy ra Angular là yêu cầu cho OpenDash. [F01]

Classic tích hợp rất chặt với theme/WP/WooCommerce: theme options qua WordPress Customizer, Header Designer riêng, footer có thể dùng UX Block, nhiều element shop phụ thuộc WooCommerce context. [F04] [F05] [F07]

### 1.2. Flatsome Site Builder mới (2026)

Flatsome công bố thế hệ mới ngày **30/09/2026**. Sản phẩm mới tách **Site Builder plugin** và **Flatsome Block Theme**, dựng trang bằng WordPress blocks chuẩn, quản lý page/post/product/template/template part, patterns và global styles. Classic vẫn được công bố tiếp tục hỗ trợ riêng. [F02] [F03] [F14]

Migrator do Flatsome công bố dùng chu trình **Scan → Convert → Review → Apply**: kiểm kê shortcode/UX Blocks và thành phần site, tạo bản ứng viên bên cạnh bản đang phục vụ, cảnh báo phần không tương thích, duyệt từng trang, áp dụng rồi có đường rollback. UX Blocks chuyển thành synced patterns, UX templates thành unsynced patterns. Các shortcut chưa có block tương đương sẽ bị cảnh báo và có thể ngừng render sau chuyển đổi. [F03]

**Bài học mang tính kiến trúc**: UI builder thay đổi được, nhưng định dạng lưu trữ, identifier và đường migration mới quyết định khả năng sống lâu của website. Không dùng WordPress block serialization làm chuẩn cho OpenDash vì OpenDash không chạy WordPress; học tính *có cấu trúc, có version, có đường chuyển đổi*.

### 1.3. Những gì tài liệu hai thế hệ dễ bị nhầm

| Chủ đề | UX Builder Classic | Site Builder mới | Lưu ý khi nghiên cứu |
| --- | --- | --- | --- |
| Cốt lõi nội dung | WordPress shortcodes | Native WordPress blocks | Không trộn cú pháp hay API |
| Application editor | Demo Classic mô tả Angular | Trình dựng block trên nền WordPress | Không được suy luận framework mới từ framework cũ |
| Reuse | UX Blocks/shortcode tham chiếu | Patterns synced/unsynced, template parts | Hai đời có metadata khác nhau |
| Theme | UX Builder gắn với Flatsome Classic | Builder plugin tách Block Theme | Mốc mới đáng học về ranh giới |
| Responsive | Classic có responsive options | Tài liệu mới xác định cascade từng breakpoint | Không mang định mức px của mirror thành chuẩn |
| Lưu/publish | UX Builder lưu shortcode vào nội dung WP | WordPress Save draft / Publish / Update | Chính sách lưu OpenDash do user chốt |
| Template shop | UX Blocks + Woo template hooks | Block template | Không trộn với yêu cầu commerce ngoài V1 |
| Migrate | Không có chuẩn JSON AST công khai trong Classic | Migrator dịch shortcode sang block | Có chi phí tương thích đáng kể |

## 2. Trải nghiệm người dùng: dựng một trang trong Classic

### 2.1. Entry points và vòng đời một phiên sửa

**[OFFICIAL]** UX Builder được mở từ trang/bài viết hỗ trợ builder; ví dụ nguồn chính thức cho CPT yêu cầu đăng ký bằng `add_ux_builder_post_type(...)` và template dùng `the_content()`. Có thể tạo trang trắng hoặc chọn layout/preset; Classic demo khẳng định thao tác trên sidebar và preview. [F01] [F06]

**Mô hình thao tác được mô tả ở mức hành vi** (không phải bản ghi network từng bước):

```text
Trang đang hiển thị / Danh sách trang WordPress
           │ mở Edit with UX Builder
           ▼
┌────────────────────┬─────────────────────────────────────┐
│ Sidebar            │ Canvas / Live Front-end              │
│                    │                                     │
│ TREE / CHỌN NODE   │ Section                             │
│ ADD / ELEMENTS     │ └─ Row                              │
│ EDIT / OPTIONS     │    ├─ Column → Text → Button        │
│                    │    └─ Column → Image                │
│                    │                                     │
│ Các option theo     │ Bấm chọn phần tử, xem và sắp xếp    │
│ element/breakpoint  │ bố cục theo viewport                │
└────────────────────┴─────────────────────────────────────┘
       │ thay đổi trên phiên đang sửa
       ▼
Lưu trang → nội dung WordPress có shortcode → frontend render
```

Chưa xác minh chi tiết **autosave**, chính sách trước/sau `Save`, cách khóa khi hai admin sửa cùng trang, xử lý lỗi mạng/401, undo stack bên trong hoặc số lượng request render. Đó là các yêu cầu phải được tự thiết kế và kiểm thử trong OpenDash.

### 2.2. Sidebar, canvas và thao tác

Các nhóm chức năng xuất hiện ở demo và hướng dẫn vận hành công khai:

- **Tree / tổ chức**: phần tử nằm trong hierarchy, chọn và kéo thả thay đổi thứ tự; builder hướng đến việc tránh thao tác raw shortcode.
- **Add / Inserter**: thêm element theo nhóm (layout, content, shop) và preset. Khu vực chèn nên chỉ hiển thị thành phần hợp lệ theo parent.
- **Edit / Inspector**: nhóm thuộc tính của element (ảnh, text, width/height, spacing, màu, effects, visibility).
- **Canvas**: hiển thị gần thiết kế thực, cho chọn trực quan; thay đổi text/option có preview trực tiếp.
- **Viewport**: desktop, tablet, mobile; giá trị khác nhau theo thiết bị.
- **Quick operation**: sao chép, sắp xếp, chèn thêm vào vị trí được chọn là mô hình trải nghiệm đáng học. Mọi phím tắt cụ thể của Classic cần xác minh trong bản chạy trước khi coi là tương thích hành vi.

**[PROPOSAL]** OpenDash giữ một bên là cây/inspector, bên kia là iframe renderer. Không cần bắt chước từng nút hay giao diện độc quyền của Flatsome. Phải ưu tiên chọn đúng node, không mất focus và undo trong một thao tác nhất quán.

### 2.3. Hệ phân cấp của layout

Cách sử dụng phổ biến và ví dụ shortcode chính thức thể hiện `row` / `col` (và `row_inner` / `col_inner`) có lồng nhau; UX Builder còn có `section`, `ux_banner`, `ux_slider`, `ux_image`... [F01] [F08]

**[PROPOSAL]** V1 OpenDash dùng canonical hierarchy đã chốt, với *slot đặc biệt* cho component có children riêng:

```text
Document(page | header | footer | archive | single)
└─ Section
   └─ Row
      ├─ Column
      │  ├─ Heading
      │  ├─ RichText
      │  ├─ Image
      │  └─ Button
      └─ Column
         └─ Banner (có khả năng chứa overlay children theo contract)
```

Không giả định tất cả component có thể chứa mọi component. Ví dụ Slider chỉ cho phép slide type hợp lệ; Column đặt trong Row, không đặt tùy tiện dưới Button. Khi làm Nested Rows, cần chốt tối đa chiều sâu, semantics và xử lý cột trong cột theo quy tắc minh bạch. Đây là điểm mà một tree có validator vượt trội so với chuỗi shortcode tùy ý.

### 2.4. Một ví dụ nhìn dưới dạng shortcode

Tài liệu chính thức về product layout có ví dụ `[row]`, `[col span="9" span__sm="12"]`, `[ux_product_title]` và `[ux_product_price]`. Để minh họa nguyên lý, một **mẫu đơn giản do nghiên cứu biên soạn** sẽ có dạng:

```text
[section]
  [row]
    [col span="6" span__sm="12"]
      [ux_image id="123"]
    [/col]
    [col span="6" span__sm="12"]
      [ux_text]Nội dung minh họa[/ux_text]
    [/col]
  [/row]
[/section]
```

Mẫu trên dùng syntax shortcode tiêu biểu, **không phải output capture từ bản chạy hoặc bảo đảm cú pháp đúng mọi version**. Cú pháp chính thức được xác nhận ở [F08]. Shortcode là một DSL dạng text, nesting và attribute trong string; WordPress thực thi chuỗi đó bằng callback shortcode đăng ký. Việc UX Builder hiện shortcode khi quay lại editor thường được tài liệu chính thức xác nhận. [F09]

**Hệ quả:** editor cần parse string thành một representation nội bộ để biểu diễn hierarchy; mỗi lần chỉnh sửa/serialize cần giữ nguyên cú pháp và option. Việc Flatsome *thực sự giữ AST nào trong RAM* và parser cụ thể của bản hiện hành chưa được xác minh, nên không khẳng định implementation.

### 2.5. Shortcode đem lại gì và mất gì?

| Thuộc tính | Ưu điểm trong WordPress Classic | Hệ quả nếu OpenDash dùng làm nguồn thật |
| --- | --- | --- |
| Dễ copy/paste một đoạn shortcode | WordPress vốn hiểu shortcode | Khó đảm bảo node ID, version và schema |
| Tài liệu dễ chia sẻ bằng text | Người dùng kỹ thuật có thể thấy cấu hình | Phải có parser chuỗi lồng nhau và escape chuẩn |
| Render bằng shortcode callback | Tái dùng hệ sinh thái WordPress/PHP | Không phù hợp với Rust/MiniJinja và isolation Wasmi |
| Extension theo shortcode | Addon dễ đăng ký trong WP | Khó enforce type permissions ở node/field |
| Thiếu extension | Có thể mất render tương ứng | Dữ liệu vẫn cần giữ, cần placeholder + recovery |
| Responsive qua attribute hậu tố | Gọn khi lưu thành text | Khó migrate, validate, query, diff chính xác |
| Sửa text ngoài builder | Thuận WordPress Classic | Dễ biến dạng cấu trúc nếu người dùng sửa cú pháp |
| Migrations | Có thể dịch sang format khác | Bản Migrator chính thức có nhiều loại warning [F03] |

**Kết luận thiết kế:** tham khảo shortcode để hiểu **ngữ nghĩa component**, **không đưa shortcode string vào database như canonical document của OpenDash**. Nếu sau này cần nhập từ Flatsome Classic, viết parser/importer tách biệt với AST chính thức.

## 3. Element Registry: chìa khóa tái tạo UX tốt

### 3.1. Phân rã các phần của element

Bằng chứng từ các file đăng ký addon và một **mirror bên thứ ba** cho thấy giao diện cấu hình dạng `add_ux_builder_shortcode('name', {...})` có các loại metadata sau: [C01] [C02] [C03]

| Nhóm khai báo | Ví dụ ý nghĩa trong Classic | Ánh xạ đề xuất cho OpenDash |
| --- | --- | --- |
| Identity | Tên shortcode, nhãn, category, thumbnail | `type` dạng `namespace/name`, label, icon, category |
| Cấu trúc | `type: container`, `allow`, `allow_in`, children | `slots`, `allowedChildren`, `allowedParents` |
| Form | `options`, `type`, `heading`, `description` | Typed JSON field schema + UI schema |
| Default & presets | `default`, `presets` | Defaults và insert presets có version |
| Responsive | `responsive: true` | Giá trị theo breakpoint + inherit |
| Preview hint | `on_change`, `style`, `class`, `selector` | Renderer impact hint / typed CSS binding |
| Dependency | script/style cho element | Asset manifest, defer/load-on-demand |
| Output | PHP shortcode + template editor | Server render contract và preview metadata |
| Điều kiện | `conditions` trong một số option | Predicates khai báo để ẩn/hiện inspector |

Chỉ xác nhận **các key trong ví dụ source**; không suy diễn rằng tất cả version dùng một API thống nhất hoặc mọi field hỗ trợ điều kiện.

### 3.2. Option controls

**[MIRROR]** Ví dụ `ux_image` và `ux_slider` cho thấy các input type: textfield, select, radio-buttons, slider, scrubfield, colorpicker, image, margins, group. Thuộc tính có thể khai báo min/max/step, danh sách options và `responsive`. Một số option có `on_change` cập nhật class/style trên phần tử khi điều khiển đổi giá trị; một số có `recompile: false`. [C01] [C02]

Nhận xét: thiết kế schema-based inspector có giá trị cao vì cùng một khai báo phục vụ catalog, validator, form và trình render. **Nhưng không bê chuỗi selector/CSS hoặc interpolation tùy ý từ khai báo vào OpenDash**; đó là dữ liệu từ gói mở rộng cần kiểm soát.

**[PROPOSAL]** Các field type tối thiểu của Builder nên gồm: `text`, `textarea`, `richtext`, `number`, `unit`, `boolean`, `select`, `color`, `image`, `media-gallery`, `url`, `spacing`, `alignment`, `responsive`, `reference` và `binding` (loại cuối triển khai sau khi content/CPT đủ). Cần phân biệt option của block với field nhập nội dung của CPT.

### 3.3. Container, placement và toolbar

**[MIRROR]** Mẫu `ux_slider` có `type: container`, danh sách `allow`, metadata `children`, `toolbar` và `tools`. Đây là bằng chứng giới hạn tối thiểu rằng builder có thể cho phép tùy biến loại children và công cụ riêng theo element ở phiên bản lịch sử. [C02]

**[PROPOSAL]** Không mở API cho gói ZIP cung cấp React component hoặc JavaScript tùy ý trong Admin V1. Mọi element do core/theme/plugin thêm phải định nghĩa UI qua schema. Nếu cần một toolbar chuyên biệt, dùng **declarative actions** từ danh mục đã kiểm tra quyền, tránh ràng buộc vào Angular/React của editor.

### 3.4. Render và cập nhật live

**[MIRROR]** Một shortcode renderer `section` trong mirror dùng PHP để dựng HTML, tạo CSS theo element ID/attributes rồi thực thi shortcode con bằng `do_shortcode(...)`. Mẫu editor `ux_banner.html` dùng chỉ dẫn Angular để hiện preview; với một số video YouTube, template cho biết nội dung chỉ nạp trên frontend sau khi lưu. [C04] [C05]

Hai quan sát có ý nghĩa:

1. **Editor preview có thể dùng template/đường cập nhật riêng** với output thực từ WordPress, nên khó đảm bảo fidelity 100% ở phần tử có JS/third-party effects.
2. **Làm nhanh option thông thường** (màu, width) có thể khác chi phí với **re-render cấu trúc** (thêm/đổi children, data query).

**[PROPOSAL]** OpenDash sử dụng cùng Rust render contract để bảo đảm ngữ nghĩa preview/public. Tối ưu local CSS patch và fragment render là các cải tiến có điều kiện, chỉ dùng khi tạo ra output tương đương. Không suy luận từ mirror rằng UX Builder Classic có bất kỳ cơ chế iframe/postMessage cụ thể nào trong bản mới.

## 4. Responsive design và các thuộc tính thiết bị

### 4.1. UX Builder Classic

Tài liệu Classic khẳng định desktop/tablet/mobile và thay option theo thiết bị; shortcode mẫu cho thấy `span__sm="12"`. Một mirror lịch sử có breakpoint `sm=550`, `md=850`, `lg=1000`. **Các con số này chỉ là quan sát bản mirror lịch sử**, không phải breakpoint mặc định được xác nhận cho Flatsome Classic đang phát hành ngày khảo sát. [F01] [F08] [C06]

Responsive gồm ít nhất ba bài toán độc lập:

1. Đổi **kích thước preview viewport**.
2. Lưu **giá trị option** khác nhau theo breakpoint.
3. Render HTML/CSS đúng khi browser thật thay đổi kích thước (không phụ thuộc vào việc admin đang chọn viewport).

### 4.2. Hệ kế thừa và cách hiển thị nên học

Tài liệu Site Builder mới mô tả mô hình một thiết kế dùng trên nhiều màn hình: desktop là nền, tablet ghi đè khi cần, mobile ghi đè tablet/desktop khi cần; UI có dấu trạng thái khi một kích thước có giá trị riêng và cho reset về inherited value. Đó là pattern UX rất tốt cho OpenDash, dù **không nên gán behavior này là cơ chế đã xác nhận của UX Builder Classic**. [F15]

**[PROPOSAL]** Schema responsive:

```json
{
  "columns": {
    "desktop": 6,
    "tablet": null,
    "mobile": 12
  },
  "gap": {
    "desktop": {"value": 24, "unit": "px"},
    "tablet": {"value": 16, "unit": "px"},
    "mobile": null
  }
}
```

`null` nghĩa là **kế thừa**, không nghĩa là `0` hoặc xóa field. Rule ví dụ: giá trị tablet nếu đặt thì override desktop; mobile kế thừa tablet nếu tablet có giá trị, còn không thì desktop. Ghi tường minh rule ở core validator và trong Inspector.

### 4.3. Breakpoint không đồng nghĩa device vật lý

Viewport desktop/tablet/mobile là **nhóm CSS layout**, không phải detection thiết bị. Thiết bị 2-in-1, resize cửa sổ và zoom có thể rơi vào breakpoint khác. Cần kiểm tra cả viewport lân cận ngưỡng và xử lý `min-width`/`max-width` nhất quán.

**[PROPOSAL]** Định nghĩa một default breakpoint map từ core hoặc theme design tokens, nhưng tránh để mỗi theme đổi hẳn bộ breakpoint làm document cũ mất ý nghĩa. Trước M4 phải quyết định: breakpoint cấu hình toàn site hay core-fixed; theme chỉ thay token layout. Không tự chọn ngưỡng theo mirror Classic.

### 4.4. Visibility, image focal point, stack/reflow

Classic demo giới thiệu visibility responsive và banner focus point; những tính năng này xử lý vấn đề thật trên mobile. [F01]

- **Visibility**: dùng CSS/lớp responsive, có cảnh báo khả năng mất nội dung với screen reader và SEO nếu tùy chọn ẩn chỉ mang tính trình bày.
- **Image focal point**: lưu cặp tọa độ tương đối 0..1, render `object-position` hoặc background-position đúng context.
- **Column reflow**: thứ tự nguồn DOM và thứ tự thị giác cần rõ; đảo cột mobile không làm sai tab order/reading order.
- **Text overlay**: tính tương phản màu, gradient, crop và giới hạn chiều rộng text cho banner/hero.

## 5. Component tái sử dụng và thư viện thiết kế

### 5.1. UX Blocks trong Classic

**[OFFICIAL]** UX Blocks là một loại nội dung WordPress riêng (post type `blocks`) để lưu vùng nội dung xây bằng UX Builder rồi chèn qua shortcode tham chiếu (ví dụ `[block id="homepage-slider"]`). Có thể dùng ở page, widget, theme options, shop/blog header, footer thay thế, mega menu; tài liệu hướng dẫn tạo Block, lưu và tham chiếu. [F07] [F10] [F11]

Cơ chế tham chiếu này tạo sự **tái sử dụng**: chỉnh nguồn thì nơi sử dụng có thể thay đổi theo; đồng thời tạo coupling ở runtime khi ID hoặc loại nội dung bị mất. Không coi UID hay mô hình quản lý revision của UX Blocks là đã xác minh ở đây.

### 5.2. Flatsome Studio

**[OFFICIAL]** Flatsome Studio là thư viện mẫu import bố cục, được mở từ Add Content → Flatsome Studio khi feature được bật trong tích hợp của Classic. Thư viện giúp người dùng bắt đầu từ section/layout có sẵn thay vì thiết kế từng phần tử. [F12]

**[PROPOSAL]** OpenDash V1 nên có thư viện **preset copy-on-insert**, được copy thành các node mới với ID mới, có thể chỉnh độc lập. Khi importer gặp ID trùng/asset thiếu hoặc content đã có, yêu cầu người dùng chọn vị trí thêm và preview tác động; nhập mẫu theme phải giữ content hiện hữu. Bộ **synced pattern/override** giống Site Builder mới nên để sau V1 như yêu cầu sản phẩm. [F16]

### 5.3. Template khác pattern và khác dữ liệu

| Khái niệm | Dùng cho | Ví dụ OpenDash | Ownership đề xuất |
| --- | --- | --- | --- |
| Element/block | Đơn vị render, có props và slots | Image, Heading, QueryGrid | Registry từ core/theme/plugin |
| Preset | Copy cây node mới | Hero split, grid 3 cột | Theme cung cấp, site sở hữu bản copy |
| Synced pattern (sau V1) | Một nguồn hiển thị nhiều nơi | CTA dùng ở 10 bài | Site-owned shared document |
| Template | Quy tắc hiển thị một loại route/content | `single/species` | Theme default + override site |
| Template part | Vùng chung của nhiều template | Header, footer, breadcrumb | Site-owned nếu đã chỉnh |
| Content entry | Dữ liệu nội dung quản trị bằng form | Species “Cá hề” | Site-owned, không xóa khi đổi theme |

**Lỗi thiết kế cần tránh**: nhập một demo theme đè `single` đang chỉnh, hoặc sửa pattern rồi âm thầm thay tất cả page trong khi người dùng nghĩ mình chỉ sửa trang hiện tại.

## 6. Header, footer, single/archive, nội dung động

### 6.1. Header và footer của Classic

Flatsome Classic có **Header Designer** drag/drop chuyên biệt cho logo, nav, cart, search, contact, icon, HTML/shortcode, mobile/off-canvas; footer có thể thay bằng một UX Block tạo bằng builder. Vì vậy slogan “dựng mọi phần bằng cùng một canvas” sẽ **không phản ánh đúng toàn bộ UX Builder Classic**: nhiều giao diện nằm ở hệ theme options/header designer riêng. [F04] [F05]

**[PROPOSAL]** OpenDash dùng chung document AST cho page, header, footer, archive, single, nhưng có **document context và vùng chỉnh sửa được** khác nhau. Header có slot nav/logo/menu; single có CurrentEntry; archive có QueryContext. Giữ một inspector thống nhất và template resolver trung tâm, tránh tạo ba DSL độc lập.

### 6.2. Custom product layout như ví dụ dynamic context

**[OFFICIAL]** Classic cho phép dùng UX Block làm product page layout, chọn global/category/product-specific và edit trên một product context có dữ liệu thật. Docs liệt kê product title, price, add-to-cart, gallery...; có hỗ trợ hook và filter để chọn block theo điều kiện. [F08] [F17] [F18]

OpenDash **chưa có thương mại điện tử V1**, nên điều nên học là **context-aware template**. Ví dụ tương đương:

- `single/species` render với `CurrentEntry.title`, `CurrentEntry.fields.scientific_name`, `CurrentEntry.fields.min_tank_liters`.
- `archive/species` có `CollectionQuery(type="species", taxonomy="fish", page=...)`.
- Chuyển mẫu preview từ “Cá hề” sang “Hải quỳ” để kiểm tra template không hard-code dữ liệu.

### 6.3. Field binding và collection query

Classic chứng minh UX dành cho component nội dung động (đặc biệt WooCommerce), nhưng **không tự suy ra Classic có generic CPT field binding đầy đủ như OpenDash cần**. Site Builder mới quảng bá custom fields/dynamic binding, song nghiên cứu này không xác nhận mọi chi tiết API triển khai. [F14]

**[PROPOSAL]** Tách hai hình thức gán giá trị:

- **Literal:** text/màu/ID media do editor nhập trực tiếp.
- **Binding:** nguồn dữ liệu theo scope, field key, transformer an toàn và fallback.

Binding phải type-check: ảnh chỉ bind tới media/image field; số chỉ tới number; URL cần policy an toàn; rich text cần sanitizer. Collection element khai báo CPT, taxonomy/filter/sort, page size, pagination và có context per-item; không cho phép nhập raw SQL hay code template tự do ở giao diện editor.

## 7. Các bài học kỹ thuật và giới hạn nguồn của Classic

### 7.1. Cơ chế chắc chắn có thể suy ra

1. **Shortcode serialization tồn tại**: tài liệu UX Themes nói rõ người dùng thấy shortcode sau khi save và mở WordPress Editor. [F09]
2. **Bố cục lồng nhau**: mẫu shortcode chính thức có rows/cols/child widgets và responsive attributes. [F08]
3. **Registry tùy biến**: ví dụ addon đăng ký shortcode element và options; mirror có container, presets, allowed children, preview metadata. [C01] [C02] [C03]
4. **Render phụ thuộc WordPress/theme runtime**: shortcode callbacks PHP và Woo context trong tài liệu/demo; không có căn cứ để dùng lại runtime nguyên bản trong Rust. [F17] [C04]
5. **Không phải mọi preview có cùng runtime**: editor Angular template mirror dùng placeholders cho nội dung video; bản chính thức ngày khảo sát chưa được trace xác nhận. [C05]
6. **Migration có bất tương thích**: Flatsome chính thức liệt kê shortcode/hiệu ứng không có block tương ứng, cần review thủ công. [F03]

### 7.2. Những điều **chưa** xác nhận và không được dùng làm cơ sở quyết định

- Tên/trình tự API REST hoặc WordPress AJAX nội bộ của bản Classic mới nhất.
- Format JSON tạm giữ trong browser, model state thật, cơ chế request cancel, history stack, optimistic locking.
- Có hay không iframe và chính xác sandbox/CSP/postMessage của bản Classic hiện hành.
- Cấu trúc database meta khác ngoài shortcode content và UX Block post type theo tài liệu.
- Trình xử lý CSS recompile tương đương giữa editor và production ở mọi element/version.
- Số lượng shortcode/element **chính xác** của phiên bản đang bán. Trang demo Classic là mô tả tại thời điểm viết; nhà sản xuất nói Migrator chuyển **87** loại element, con số này không phải tổng runtime hiện tại. [F19]
- Tốc độ “fast as lightning” và các nhận định benchmark: là lời giới thiệu marketing, không có phép đo tái lập.
- Tình trạng license của bản mirror; không sử dụng mirror như nguồn mã có thể copy/redistribute.

### 7.3. Khả năng áp dụng theo nguyên tắc clean-room

Được dùng **ý tưởng tính năng/hành vi**: cây, responsive options, presets, panel schema, field binding, renderer đúng preview. Không copy code/theme assets/DOM/CSS/icon/bundle hoặc các file PHP/Angular từ Flatsome vào OpenDash. Tạo module, API, giao diện và quy tắc biểu đạt riêng với tài liệu yêu cầu rõ ràng, test độc lập. Mọi đánh giá giấy phép với một thư viện hay mã nguồn cụ thể phải làm riêng trước khi đưa vào repo.
