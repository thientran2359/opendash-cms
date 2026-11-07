# Nghiên cứu tham chiếu Daypress và EmDash cho OpenDash

> **Loại tài liệu:** Nghiên cứu kỹ thuật / đánh giá phương án (không phải ADR, không phải quyết định triển khai).
> **Ngày khảo sát:** 07/11/2026.
> **Trạng thái:** `RESEARCH / PROPOSED`. Các đề xuất bên dưới **chưa được chủ dự án chốt**.
> **Phạm vi:** Phân tích mã nguồn và tài liệu công khai; đối chiếu với OpenDash V1. Không thay đổi code, dependency, schema, ABI hoặc roadmap đã duyệt.

## 1. Kết luận và phạm vi sử dụng

**Định hướng đề xuất:** học mô hình **layout AST, SSR và importer** của Daypress; học **content schema, versioned blocks, plugin capability/lifecycle và UI theo khai báo** của EmDash. Xây hợp đồng dữ liệu và runtime riêng cho OpenDash, vì các phần được khảo sát khác với yêu cầu theme ZIP, plugin Wasmi, React Admin và một Rust executable.

Đây là một **đề xuất kỹ thuật**, không tự thay thế [yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md), [stack đã duyệt](../architecture/TECH_STACK.md), [kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) hoặc [Plugin ABI v1](../contracts/PLUGIN_ABI.md). Mọi thay đổi contract cần có quyết định riêng trước khi viết code.

### 1.1. Phương pháp, nguồn và mức độ chắc chắn

- **Đã đọc source chọn lọc:** Cargo workspace, AST/schema/renderer, database/CPT, Admin builder state, cache, MCP, plugin của Daypress; package workspace, schema registry/validator, plugin manifest/context/lifecycle, MCP và LICENSE của EmDash.
- **Đã đối chiếu tài liệu công bố:** README, roadmap và mô tả kiến trúc/khả năng của hai dự án. Với những tính năng chỉ được công bố, chưa có bài kiểm thử độc lập, dùng nhãn **công bố**, không coi là đã được tự kiểm chứng.
- **Chưa thực hiện:** clone/build/chạy hai CMS, kiểm thử end-to-end, benchmark tốc độ/nhớ, pentest, kiểm tra license của tất cả dependency. Mọi nhận định hiệu năng và độ hoàn thiện có giới hạn đó.
- **Bản nguồn được đối chiếu:** Daypress commit [`92cc95e`][D-repo] (05/11/2026, workspace version `0.8.0`); EmDash commit [`eebdc3e`][E-repo] (06/11/2026). Các liên kết file ở cuối đều pin commit để tránh `main` thay đổi về sau.
- **Quy ước:** **Quan sát source** = có file/logic tương ứng; **Công bố** = README/roadmap nói tới, chưa kiểm chứng chạy; **Đề xuất** = đánh giá cho OpenDash, cần ADR hoặc thử nghiệm nếu áp dụng.

### 1.2. Baseline OpenDash cần giữ

Theo [PRODUCT_REQUIREMENTS](../product/PRODUCT_REQUIREMENTS.md): Rust + SQLite, **một executable cho một website**, Windows để phát triển, VPS Linux để vận hành; Axum/Tokio, SQLx, React/TypeScript/Vite, MiniJinja, Wasmi đã được chọn ở [TECH_STACK](../architecture/TECH_STACK.md).

V1 yêu cầu CPT, custom fields, taxonomy cha–con, form nhập liệu, builder cây **Section → Row → Column → Element**, responsive preview, template page/header/footer/archive/single, dynamic field binding, theme/plugin cài ZIP **không biên dịch lại**, logic plugin server và API dưới quyền được khai báo, SEO, form liên hệ và backup/restore thủ công. Chưa lưu phải hiển thị trong preview; **Lưu cập nhật trang công khai**.

**Bảo toàn dữ liệu là điều kiện cứng:** đổi/cập nhật theme giữ content và bố cục đã chỉnh; tắt plugin giữ CPT/fields/content; thiếu block/template cần có cảnh báo/đường khôi phục. Người quản trị **được thông báo quyền plugin nhưng không phải duyệt quyền riêng ở V1**. Repeater, revision history, đồng bộ block, đa ngữ, publish workflow độc lập và thương mại điện tử thuộc các mốc sau.

**Tình trạng code OpenDash tại thời điểm viết:** M1/M2 đã được nhập và có [báo cáo kiểm chứng tại workspace đích](../verification/2026-11-07-m1-m2-import.md): Auth/RBAC/CSRF, theme ZIP một block, Preview/Lưu, Wasmi worker và ABI v1 đã qua bộ kiểm tra trên Windows. M3 content/media, M4 builder nhiều tầng và backup/restore toàn website còn tiếp theo. Không đề xuất viết lại các module M1/M2 đã có chỉ vì hệ thống tham khảo sử dụng cách khác.

## 2. Ma trận so sánh

| Lĩnh vực | Daypress | EmDash | Đánh giá cho OpenDash |
| --- | --- | --- | --- |
| Runtime | Rust, Axum/Tokio, `rusqlite`, SQLite WAL, binary | TypeScript, Astro integration; Workers/D1 hoặc Node/SQLite | Giữ stack OpenDash; Daypress gần hơn ở tầng hệ thống |
| Lưu nội dung | Schema tương thích WordPress (`posts`, `postmeta`, term tables), CPT/field groups mở rộng | Schema registry cho collection/typed fields, validation, phiên bản field/block | Học cả hai; thiết kế ownership và lifecycle dữ liệu riêng |
| Builder | JSON layout AST, node tree, CSS responsive, Rust SSR | Blocks/structured content + visual editing Astro | Daypress phù hợp yêu cầu layout nhiều tầng hơn |
| Rich text | HTML/Gutenberg và các AST mapper | Portable Text (JSON có cấu trúc) | Nghiên cứu tách nội dung khỏi HTML ở M3 |
| Theme | Template động/DayStudio; nhiều phần tích hợp core | Astro starter project/template | Cả hai không đáp ứng trọn vẹn theme ZIP runtime đã chốt |
| Plugin | Nhiều tính năng tích hợp native Rust; Wasm extensions nằm ở roadmap | Sandboxed Worker isolates + manifest capability, hook, host context | Học EmDash ở contract; thực thi bằng Wasmi hiện có |
| Admin | HTML/JS do Rust serve; builder state JS nhúng | React Admin và schema-based interfaces | Giữ React Admin hiện có, tham khảo UX EmDash |
| Import WordPress | WXR, Elementor/Divi/Gutenberg → AST; fidelity report | Content migration/Portable Text ecosystem | Daypress đáng tham khảo khi làm importer sau V1 |
| MCP | Rust adapter qua HTTP/stdio | MCP dùng API/handlers với auth/scopes/revision | Làm sau khi content API và phân quyền ổn định |
| Performance | In-memory SSR cache, Gzip/ETag, SQLite FTS5 | Cache và optimizations hướng serverless | Chỉ chọn sau benchmark với workload OpenDash |
| Giấy phép repository | AGPL-3.0-or-later + lựa chọn commercial | MIT | Chưa lấy mã Daypress; xem xét mã EmDash theo từng file/dependency |

## 3. Daypress — bóc tách theo source

### 3.1. Crate boundaries và DB

**Quan sát source:** `Cargo.toml` định nghĩa workspace gồm `daypress-server`, `daypress-ast`, `daypress-transpiler` và `generated_slugify`. Server dùng Axum/Tokio; SQLite qua `rusqlite` (bundled), khác SQLx của OpenDash. [Cargo workspace][D-cargo] · [Server manifest][D-server-cargo].

`daypress-server` có adapters/database, api, handlers, importer, templates/admin, theme_builder, publisher và MCP. Dù kiến trúc tài liệu đặt mục tiêu domain/ports/application, README Daypress thừa nhận việc phân tầng mới chỉ áp dụng một phần; một số handler còn gọi database adapter trực tiếp. **Không coi sơ đồ kiến trúc là bằng chứng code đã tách lớp hoàn toàn.** [Architecture][D-arch] · [README][D-readme].

**Đánh giá:** giữ ranh giới `opendash-core` (types, schema, validation) và `opendash-server` (use cases, HTTP, SQLx, render và Wasmi). Không đổi `rusqlite` hoặc chia thêm crate theo mẫu Daypress trước khi đo được lợi ích.

### 3.2. Layout AST và responsive renderer

**Quan sát source:**

1. `DaypressLayout` có `version`, `root` (node) và `global_styles` trong [`schema/layout.rs`][D-layout].
2. Kiểu node được mô tả có cấu trúc (Section, Container, Column, Grid/Flex, text, image, button và các widget khác), cùng styles và children trong [`schema/node/repr.rs`][D-node].
3. [`render/mod.rs`][D-render] biên dịch global design tokens, CSS cho layout/responsive rồi render HTML từ cây; có bước làm sạch output.
4. [`templates/admin/builder/scripts/core_state.rs`][D-builder-state] chứa trạng thái JS của builder (AST, selection, breakpoint, dirty flag, history/undo/redo).

**Điểm nên học:** một mô hình layout JSON chuẩn, node ID ổn định, schema version, styles responsive, bindings và render public từ cùng dữ liệu. Phù hợp trực tiếp với nhu cầu page/header/footer/archive/single và sửa giao diện không phá content.

**Không bê nguyên:** Daypress AST chứa nhiều widget/khái niệm ngoài V1; builder UI thực tế dùng các đoạn JavaScript nhúng trong Rust, trong khi OpenDash đã có React Admin. [Tài liệu kiến trúc][D-arch] nhắc Leptos/Wasm builder, nhưng source builder state hiện khảo sát chứa JS thường; đánh giá implementation theo file thay vì diễn giải sơ đồ là tình trạng đang chạy.

**Về an toàn render:** [`render/sanitizer.rs`][D-sanitize] có thao tác regex trên HTML/class/attribute của editor. OpenDash nên xây API render với validation/encoding theo ngữ cảnh tại **từng node**, gồm HTML text, safe rich text, URL, CSS và asset. Không dựa vào regex cuối pipeline như cơ chế an toàn chính. Đây là **khuyến nghị thiết kế**, chưa phải kết luận một lỗ hổng cụ thể trong Daypress.

### 3.3. CPT, taxonomy, field groups

**Quan sát source:** [`database/ddl.rs`][D-ddl] có cấu trúc posts/postmeta/options/terms/term_taxonomy/term_relationships theo phong cách WordPress, và `posts.layout_json`. [`database/content_model.rs`][D-content] quản lý post type definitions, custom taxonomies, field groups, slug/ràng buộc, preset và đường import ACF/CPT UI.

**Lợi ích:** migration từ WordPress dễ lập bản đồ hơn; hành vi CPT/taxonomy gần với người quen WordPress.

**Điểm cần tránh khóa cứng:** chọn mô hình `postmeta` key/value cho mọi field ở CMS mới có thể gây khó khăn cho typed queries, index, validation và migration schema. Đó là trade-off cần so sánh với EmDash bằng prototype SQLite, không phải lý do để bỏ tính tương thích WordPress.

### 3.4. WordPress importer và fidelity

**Quan sát source:** `daypress-ast` có parser/mappers cho Gutenberg, Elementor, Divi và HTML/CSS; `daypress-server/src/importer/` có module WXR, content, assets, menu, taxonomy, ID remapping. README công bố fidelity analyzer và conversion một số thành phần WP; tỷ lệ chuyển đổi website thực tế chưa được độc lập kiểm chứng. [Architecture][D-arch] · [README][D-readme].

**Đề xuất cho tương lai:**

`WXR/JSON → parse → intermediate representation → OpenDash content + AST → validation → preview → báo cáo fidelity`.

Importer chỉ nên xây sau khi Content Registry/AST ổn định. Không chuyển dữ liệu WordPress trực tiếp sang schema riêng của Daypress rồi tiếp tục chuyển lần nữa.

### 3.5. SEO, form, backup, cache, MCP

- [`hot_cache.rs`][D-cache] lưu SSR HTML và Gzip trong RAM, hỗ trợ ETag/304, chia key theo theme variant, bypass phiên đăng nhập và invalidation. Ý tưởng hữu ích **sau khi** xác định invalidation chính xác khi Save/theme/plugin/settings thay đổi; không dựa vào tuyên bố độ trễ dưới 50µs nếu chưa benchmark.
- [`adapters/mcp/mod.rs`][D-mcp] chứa manifest tool và adapter MCP gọi use cases. Hữu ích cho API AI trong tương lai; auth/scoping và tránh ghi đè dữ liệu phải thống nhất với Admin.
- README mô tả forms có lưu submission trước SMTP, backup SQLite, SEO/sitemap và media processing. Có thể tham khảo nghiệp vụ/kiểm thử V1, nhưng cần kiểm chứng từng đường thực thi trước khi tái dùng code. [README][D-readme].
- [`plugins.rs`][D-plugin] thiên về danh mục module first-party; Wasm extensions vẫn nằm ở roadmap 0.9 khi khảo sát. Không dùng Daypress để thay host Wasmi đã nhập của OpenDash. [Roadmap][D-roadmap].

## 4. EmDash — bóc tách theo source

### 4.1. Nền tảng và cấu trúc

**Quan sát source:** EmDash là TypeScript monorepo với `packages/core`, `admin`, `blocks`, `auth`, `cloudflare`, `plugin-types`, `plugins` và các package khác. CMS được tích hợp vào Astro; chạy Cloudflare D1/R2/Workers hoặc Node với SQLite. [README][E-readme] · [Root package][E-package].

**Đánh giá:** không đem Astro/workerd vào OpenDash core vì trái mô hình một executable Rust và thay đổi stack đã chốt. Học các contracts và cách tách feature/service, không copy runtime.

### 4.2. Content schema, validator và compatibility

**Quan sát source:**

- [`schema/block-type-registry.ts`][E-block-registry] quản lý định nghĩa block/field, source, version, kiểm tra slug, xử lý các phiên bản và tương tác storage.
- [`schema/block-type-contract.ts`][E-block-contract] xác thực kiểu field, tên, select options, repeater subfields, giá trị validation và so sánh tính tương thích.
- [`schema/zod-generator.ts`][E-zod] sinh validator runtime theo field definitions; xử lý required/null, URL và các kiểu dữ liệu.
- README mô tả content theo **Portable Text**, tách dữ liệu rich text khỏi markup đầu ra. [README][E-readme].

**Đề xuất:** content registry độc lập theme/plugin, typed fields và validation tại server. Cần lưu thông tin **ai khai báo schema**, **version**, **quyền thay đổi**, **hành vi khi chủ sở hữu bị vô hiệu hóa**, và **quy trình migration không làm mất field values**.

Không đưa nguyên Zod vào Rust. Có thể dùng cùng một schema JSON có version cho React UI và Rust validator; quyền quyết định giá trị hợp lệ phải ở server. Chưa chốt schema vật lý: bảng riêng cho từng CPT, bảng entries + typed values, hoặc JSON + indexes đều cần so sánh với truy vấn thực.

**Chú ý phạm vi:** repeater, relations và history revision của nội dung **chưa thuộc V1**, dù EmDash có hỗ trợ hoặc source đã có các cấu trúc tương ứng. `schema_version` để xử lý tương thích không đồng nghĩa đưa chức năng revision history vào V1.

### 4.3. Plugin manifest, host capabilities và lifecycle

**Quan sát source:**

- [`plugins/manifest-schema.ts`][E-manifest] khai báo capability như `content:read`, `content:write`, `network:request`, `media:read`, `email:send`, hook events, các tên quyền deprecated và validation manifest.
- [`plugins/context.ts`][E-plugin-context] xây host context với các API có giới hạn tới content, media, taxonomy, users, network, storage và settings.
- [`plugins/lifecycle.ts`][E-lifecycle] xử lý enable/disable, lưu trạng thái, kích hoạt/hủy cron và rollback nếu kích hoạt gặp lỗi.
- README mô tả plugin sandbox chạy trong Worker isolates, khác Wasmi của OpenDash. [README][E-readme].

**Học cách thiết kế:** manifest rõ quyền, host mediation cho mọi thao tác đặc quyền, plugin-scoped storage, hooks/actions có kiểu, lifecycle và đường phục hồi. Trường hợp plugin tắt/không còn tồn tại phải **giữ schema và content do nó đã định nghĩa** theo PRODUCT_REQUIREMENTS.

**Không chuyển runtime:** EmDash dựa trên Cloudflare Worker Loader hoặc workerd, nên API/cơ chế giới hạn tài nguyên không tương thích trực tiếp Wasmi. OpenDash giữ [Plugin ABI v1](../contracts/PLUGIN_ABI.md), worker cô lập tiến trình, fuel/memory/resource limits đã kiểm chứng ở M2; nếu cần thêm capabilities thì thực hiện bằng mở rộng ABI được version và test đúng quyền.

**Khác biệt sản phẩm cần tôn trọng:** EmDash có flow xác nhận capabilities của plugin; OpenDash V1 hiển thị để thông báo và tự áp dụng quyền hợp lệ theo khai báo, không thêm bước duyệt riêng.

### 4.4. Declarative Admin UI

**Ý tưởng nên tham khảo:** plugin/theme mô tả giao diện settings và actions theo schema; CMS sở hữu renderer cho form, table, list, tab, button thay vì cho plugin chạy tùy ý JavaScript trong Admin. Điều này phù hợp đúng yêu cầu V1.

**Đề xuất contract sơ bộ:** `id` của màn hình, `title`, `layout` (form/table/tabs), `fields` được validation, `actions` trỏ đến host endpoint có capability check, cùng error/loading states. React Admin làm renderer; plugin Wasmi chỉ trả metadata/data/action results qua host. Đây là **đề xuất** và cần đối chiếu cụ thể với ABI và phần settings đang có trước khi thiết kế API mới.

### 4.5. Theme, backup và MCP

- **Theme:** EmDash dùng Astro starter templates cho website, không có mô hình theme ZIP được cài/đổi ngay trong Admin như yêu cầu OpenDash. Chỉ tham khảo cách tổ chức component/seed settings; OpenDash cần theme runtime riêng. [README][E-readme].
- **Backup/restore:** phân biệt export JSON content với **backup có thể restore toàn site**. OpenDash phải kiểm chứng DB + uploads + installed themes/plugins + configuration khôi phục đồng bộ trên bản cài mới. Không mặc định rằng export content của CMS tham khảo đáp ứng yêu cầu này.
- **MCP:** [`mcp/server.ts`][E-mcp] có tool schema, auth scope, revision token (`_rev`), edit lock và conflict handling. Đây là hướng tham khảo đáng giá khi OpenDash có AI/API sau này; không đưa publish workflow hoặc content revisions vào V1 chỉ để theo mẫu này.

## 5. Bảng đề xuất tái sử dụng

`P0` = nghiên cứu/đặt contract trước khi mở tính năng liên quan; `P1` = triển khai trong phạm vi V1 khi tới đúng mốc; `P2` = chưa làm trong V1. **Không có mục nào ở bảng này mặc nhiên cho phép import code.**

| Mục | Nguồn | Cách sử dụng phù hợp | Ưu tiên | Điểm cần chứng minh |
| --- | --- | --- | --- | --- |
| Layout AST, node ID, schema version | Daypress | Học mô hình; Rust types/schema riêng | P0 / M4 | Preview chưa lưu, Save, đổi theme giữ layout |
| Dynamic typed field schema & validator | EmDash | Học registry/version/validation; tự viết Rust | P0 / M3 | Field lifecycle, validate, migration dữ liệu |
| CPT/taxonomy & WordPress mapping | Daypress | Học quy ước và importer mapping | P0 / M3 | Taxonomy cha–con, API/query, không mất dữ liệu |
| Block compatibility khi đổi theme | EmDash + Daypress | Thiết kế ownership/override riêng | P0 / M3–M4 | Missing block, theme update, không ghi đè user layout |
| Responsive CSS, SSR/public renderer | Daypress | Học pipeline; giữ MiniJinja + escape theo ngữ cảnh | P0 / M4 | Preview/public parity, URLs/CSS/rich text an toàn |
| Plugin capability & lifecycle | EmDash | Mở rộng chọn lọc ABI/host Wasmi hiện có | P0 khi cần | Scope quyền, timeout, rollback, Admin phục hồi |
| Admin form/table/tab theo schema | EmDash | React renderer riêng | P1 / V1 | Inputs, errors, actions, quyền và UX |
| Contact forms + SMTP + submission vault | Daypress | Học luồng nghiệp vụ và retry | P1 / V1 | Lưu submission trước gửi email, lỗi SMTP |
| SEO sitemap/metadata | Daypress | Học tính năng và test cases | P1 / V1 | Canonical/OG/noindex/sitemap đúng |
| Backup/restore full-site | Cả hai | Xác định contract riêng | P1 / V1 | Restore sang site mới đúng DB/media/themes/plugins |
| Media DAM / replace asset | Daypress | Học UX/metadata | P1–P2 | URL/ID ổn định, size, mime, references |
| WXR/Elementor/Divi/Gutenberg import | Daypress | Học pipeline + fidelity | P2 | Không mất link/media/content khi import |
| MCP agent API | Cả hai | Sau auth/content APIs ổn định | P2 | Authorization, concurrent editing, conflict |
| FTS5 / SSR hot cache | Daypress | Benchmark và chọn lọc sau | P2 | Invalidation, hit rate, RAM và latency |
| Astro runtime / Worker Loader | EmDash | Chỉ nghiên cứu | Không áp dụng | Không phù hợp stack đã chốt |
| Daystudio JS nhúng trong Rust | Daypress | Học UX; không copy UI vào React | Không áp dụng | Tránh thêm hệ state/editor song song |

## 6. Ranh giới dữ liệu/renderer đề xuất để nghiên cứu tiếp

**Sơ đồ khái niệm, chưa phải sơ đồ đã duyệt:**

`React Admin / Builder → HTTP API → Domain Schema & Layout AST → SQLx/SQLite → MiniJinja Renderer → HTML/CSS`

Theme ZIP cung cấp block/template/Theme Options đã khai báo; plugin ZIP cung cấp schema/setting/action/event trong quyền Wasmi. Dữ liệu content và layout **thuộc website**, không thuộc vòng đời cài đặt theme/plugin.

**Ví dụ minh họa cho một node (không phải API/DB schema đã chốt):**

```json
{
  "schema_version": 1,
  "root": {
    "id": "section-001",
    "type": "section",
    "props": {},
    "styles": {
      "desktop": {},
      "tablet": {},
      "mobile": {}
    },
    "bindings": {},
    "children": []
  }
}
```

Điểm cần xác định tại M4: mapping cây `Section → Row → Column → Element` với registry block; vùng cho CSS tùy chỉnh; breakpoints; thứ tự style cascade; asset references; dynamic field binding; schema migrations; quy tắc khi block bị thiếu. Lưu user overrides tách biệt theme defaults, có hành động nhập/áp dụng thiết kế mới do người dùng chủ động lựa chọn.

**Quy tắc Preview/Lưu:** Preview nhận AST + giá trị chưa lưu và dùng **cùng renderer** với public; Preview không ghi dữ liệu trang công khai. Khi bấm Lưu, validate và commit, sau đó public đọc bản mới. Giữ nguyên bản form/AST đang sửa trong React nếu 401, lỗi mạng hoặc render preview thất bại; không nhân đôi business logic giữa React và Rust.

## 7. Rủi ro chính và hướng kiểm chứng

| Rủi ro | Hệ quả | Kiểm chứng nên làm |
| --- | --- | --- |
| Chuyển nguyên Daypress AST với quá nhiều widgets | Block/schema cồng kềnh, khó nâng cấp | Chốt whitelist các element V1, test round-trip JSON |
| Dữ liệu gắn lifecycle theme/plugin | Mất content/field hoặc layout khi đổi/tắt | Fixture đổi theme, upgrade theme, disable plugin, missing block |
| Schema động không có migration rõ | Field data mất, query sai hoặc API lỗi | Tạo/sửa schema và kiểm tra giữ dữ liệu/transaction |
| Preview và public renderer khác nhau | Lệch bố cục, nội dung | Cùng input/render pipeline; screenshot và HTML test |
| HTML/CSS/URL do theme hoặc user đưa vào | XSS/SSRF/escape sai ngữ cảnh | Test từng sink (text, rich text, URL, CSS, SVG/asset) |
| Plugin capabilities mở rộng thiếu kiểm soát | Plugin vượt quyền, không tự phục hồi | Wasmi tests: quyền, timeout/fuel, memory, rollback, UI recovery |
| Copy code Daypress khi license OpenDash chưa chốt | Ràng buộc AGPL/phân phối ngoài dự kiến | Review license trước từng lần chép mã; ưu tiên triển khai độc lập |
| Cache/benchmark chỉ dựa README | Chọn kiến trúc theo số liệu chưa xác nhận | Workload và môi trường đo lặp lại, profiler/invalidation test |

## 8. Hướng nghiên cứu/đánh giá tiếp theo (chưa ủy quyền triển khai)

1. **M3 — Content schema contract:** tài liệu so sánh ba phương án storage; đề xuất DDL/API/validation, taxonomy parent-child, ownership field theo theme/plugin, và migration/disable scenarios. Trước khi tạo code cần xác nhận đây là contract M3 được duyệt. **Nghiệm thu tương lai:** tạo CPT và field bằng Admin; nhập/sửa/đọc qua API; đổi theme/tắt plugin không mất schema/content.
2. **M4 — AST/Builder contract:** chọn node schema nhỏ nhất, style/binding data model, shared renderer, user override, migration khi theme cập nhật. **Nghiệm thu tương lai:** drag/drop cây Section → Row → Column → Element, preview chưa lưu, Save lên public, đổi viewport, đổi/cập nhật theme giữ layout.
3. **Plugin UI extension contract:** rà [ABI v1](../contracts/PLUGIN_ABI.md), đề xuất thêm những host ops và schema Admin còn thiếu mà không phá M2 hiện tại. **Nghiệm thu tương lai:** cài ZIP và hiển thị settings/actions, quyền được enforced, plugin lỗi bị cách ly và Admin cứu hộ.
4. **V1 Form/SEO/Backup:** xác định acceptance fixtures và test restoration thực; dùng Daypress làm đối chiếu nghiệp vụ. **Nghiệm thu tương lai:** submission persist trước SMTP; SEO public đúng; restore DB + media + themes/plugins/settings từ gói backup trên một bản cài trống.
5. **Sau V1:** migration WP, MCP, SSR cache, FTS5 và media nâng cao. Chỉ triển khai khi có yêu cầu sản phẩm và số đo/test liên quan.

## 9. Giấy phép và chính sách tái sử dụng

**Daypress:** repository công bố **AGPL-3.0-or-later**, đồng thời có lựa chọn giấy phép thương mại ([LICENSE][D-license], [commercial info][D-commercial]). Chưa nên copy trực tiếp mã, CSS hay asset Daypress vào OpenDash khi license/mô hình thu phí của OpenDash còn mở; nếu có nhu cầu thực sự, cần đánh giá nghĩa vụ cung cấp source đối với phần mềm chạy qua mạng và trao đổi license thương mại. Có thể nghiên cứu nguyên tắc, biểu diễn dữ liệu và tự triển khai độc lập.

**EmDash:** repository dùng **MIT**, cho phép sửa đổi/tái sử dụng với điều kiện giữ thông báo copyright và license phù hợp ([LICENSE][E-license]). Trước khi copy component/file, vẫn kiểm tra từng dependency, asset, provenance, compatibility Rust/React và đường build. Việc dùng MIT không biến code TypeScript/Astro thành module Rust có thể dùng ngay.

Phần này là đánh giá kỹ thuật sơ bộ, không phải kết luận tư vấn pháp lý.

## 10. Danh mục source pin commit để tra cứu

### Daypress — `Trooper-lab/Daypress`, commit `92cc95e`

- [D-repo] — repo và commit nghiên cứu.
- [D-cargo], [D-server-cargo] — workspace/dependency.
- [D-layout], [D-node], [D-render], [D-sanitize] — layout schema, node representation và SSR.
- [D-builder-state] — trạng thái builder JavaScript.
- [D-ddl], [D-content] — SQLite DDL, CPT/field groups.
- [D-plugin], [D-cache], [D-mcp] — plugin catalog, cache, MCP.
- [D-arch], [D-readme], [D-roadmap], [D-license], [D-commercial] — giải thích và giới hạn dự án.

### EmDash — `emdash-cms/emdash`, commit `eebdc3e`

- [E-repo], [E-package], [E-readme], [E-license] — repository, stack và license.
- [E-block-registry], [E-block-contract], [E-zod] — schema động, field validation, version compatibility.
- [E-manifest], [E-plugin-context], [E-lifecycle] — capability, host context và lifecycle.
- [E-mcp] — MCP scopes, optimistic revision và edit locks.

### Liên kết chi tiết

[D-repo]: https://github.com/Trooper-lab/Daypress/tree/92cc95ed4b6d87d49dfabb0776178b696430086a
[D-cargo]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/Cargo.toml
[D-server-cargo]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/Cargo.toml
[D-arch]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/ARCHITECTURE.md
[D-readme]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/README.md
[D-roadmap]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/ROADMAP.md
[D-layout]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-ast/src/schema/layout.rs
[D-node]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-ast/src/schema/node/repr.rs
[D-render]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-ast/src/render/mod.rs
[D-sanitize]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-ast/src/render/sanitizer.rs
[D-builder-state]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/src/templates/admin/builder/scripts/core_state.rs
[D-ddl]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/src/adapters/database/ddl.rs
[D-content]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/src/adapters/database/content_model.rs
[D-plugin]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/src/plugins.rs
[D-cache]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/src/hot_cache.rs
[D-mcp]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/crates/daypress-server/src/adapters/mcp/mod.rs
[D-license]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/LICENSE
[D-commercial]: https://github.com/Trooper-lab/Daypress/blob/92cc95ed4b6d87d49dfabb0776178b696430086a/LICENSE-COMMERCIAL.md

[E-repo]: https://github.com/emdash-cms/emdash/tree/eebdc3e1c5b90fcecd7530fff010b6d21b839cba
[E-package]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/package.json
[E-readme]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/README.md
[E-license]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/LICENSE
[E-block-registry]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/schema/block-type-registry.ts
[E-block-contract]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/schema/block-type-contract.ts
[E-zod]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/schema/zod-generator.ts
[E-manifest]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/plugins/manifest-schema.ts
[E-plugin-context]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/plugins/context.ts
[E-lifecycle]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/plugins/lifecycle.ts
[E-mcp]: https://github.com/emdash-cms/emdash/blob/eebdc3e1c5b90fcecd7530fff010b6d21b839cba/packages/core/src/mcp/server.ts
