# OpenDash Studio Interactive Micro-JS Pack (`/assets/opendash-blocks.js`) & Starter Marketing Blocks (Spec A3 — Luồng A)

- **Status:** Approved
- **Author:** Orion / Elephant
- **Created:** 2026-11-28
- **Baseline SHA:** `018959e`
- **Approved By:** Owner
- **Approval Evidence:** Chủ dự án phê duyệt trực tiếp Spec A3 (bao gồm 7 Core Blocks với `core/popup` + 5 Starter Marketing Blocks) ngày 2026-11-28 (`"duyệt"`), và chỉ đạo chờ Spec A2 hoàn tất/merge vào `main` rồi mới bắt đầu triển khai Bước 5 (`S01`–`S04`).
- **Goldfish Review:** Round 1 & Round 2 Completed (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, `egm-comprehension` — 100% resolved) + Popup Delta Review Completed (`egm-design` [1 Blocker, 4 Major, 2 Minor] & `egm-codebase-details` [1 Blocker, 2 Minor] — 100% resolved; 0 Blocker / 0 Major tồn đọng).
- **Target Scope:** OpenDash Studio Interactive Core Blocks (`7` blocks `core/{tabs,tab-panel,slider,slide,gallery,social-share,popup}` + conditional `/assets/opendash-blocks.js`) & Starter Marketing Kit (`4` block mới + nâng cấp V1 `stats-bar` trong `opendash-starter`)
- **Migrations:** Không (`0` SQL migrations mới; `+0` `sqlite_schema` objects)
- **Dependency Layering:** Thiết kế tương thích cả độc lập trên `main` (đã có Spec A1 PR #632 — Unified Recursive AST V1) lẫn xếp chồng trên Spec A2 ([`studio-template-blocks-and-loop.md`](studio-template-blocks-and-loop.md), `Approved`); theo chỉ đạo Owner ngày 2026-11-28 sẽ **chờ Spec A2 merge vào `main`**, rebase nhánh `feat/studio-interactive-and-starter-blocks` lên nền sau A2 rồi triển khai Bước 5 (`S01`–`S04`) trực tiếp trên mục tiêu `48` Core Blocks (`0` conflict rebase ở cuối feature).

---

## Overview

Sau khi Spec A1 ([`studio-unified-recursive-ast.md`](studio-unified-recursive-ast.md)) hoàn thiện cây AST đệ quy hợp nhất (`BuilderDocument` V1) cùng `23` Core Blocks nền tảng và Spec A2 chuẩn hóa Theme Format V1, định tuyến `Home → Page/Index`, `19` Template-Context Blocks cùng engine `core/loop-item`, website dựng bằng OpenDash Studio (thuộc **Luồng A — Visual Builder & Theme Authoring**) vẫn cần 2 mảnh ghép để hoàn chỉnh trải nghiệm marketing, hộp thoại bật lên/thanh trượt điều hướng và trình bày nội dung hiện đại mà không làm phình kích thước trang tĩnh:

1. **Interactive Micro-JS Pack (`/assets/opendash-blocks.js`) & `7` Core Blocks tương tác / truyền thông / chia sẻ (`core/tabs`, `core/tab-panel`, `core/slider`, `core/slide`, `core/gallery`, `core/social-share`, `core/popup`):**
   - Toàn bộ `7` Core Blocks chạy qua bộ render `NativeRust` (code Rust biên dịch trực tiếp trong `opendash-server` với `render: "native"`, không tốn MiniJinja fuel — xem thêm [`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md) và [`declarative-block-json-engine.md`](declarative-block-json-engine.md)), tận dụng hệ thống `FieldType::Gallery` ([`UNIFIED_FIELD_SCHEMA.md`](../contracts/UNIFIED_FIELD_SCHEMA.md)), responsive `<picture>`/`srcset`, `register_core_icon_in_budget` và ngân sách render chuẩn.
   - Bất biến **0 KB JS thừa:** `/assets/opendash-blocks.js` (vanilla ES2020 IIFE `< 12 KiB` không phụ thuộc thư viện ngoài) **chỉ được chèn `<script src="/assets/opendash-blocks.js?v={hash}" defer></script>`** khi máy chủ xác nhận trang công khai thực sự có block cần JS phía client (`core/tabs` >= 2 tab, `core/slider` >= 2 slide, `core/gallery` có ảnh và bật `lightbox`, `core/social-share` bật nút `Copy link` / cần resolve đường dẫn tương đối thành URL tuyệt đối, hoặc `core/popup` có `popup_id` hợp lệ). Mọi trang chỉ có văn bản/layout/marketing tĩnh giữ nguyên `0` `<script>` từ Core.
2. **Bộ `5` Starter Marketing Blocks trong theme mặc định `opendash-starter` (`0` JS, `CompiledCsec` `block.json` + `render.html`):**
   - `CompiledCsec` là cơ chế biên dịch template block khai báo (`render.html`) dưới sandbox MiniJinja an toàn C-SEC (kiểm tĩnh bằng `validate_block_template_csec`), sử dụng các helper lõi đã có như `od_icon()`, `od_image()` và `od_content()` (xem [`theme-format-and-template-resolver.md`](theme-format-and-template-resolver.md) và [`theme-chrome-cache-and-safe-icons.md`](theme-chrome-cache-and-safe-icons.md)).
   - Gồm `4` block tạo mới (`icon-box`, `testimonial`, `logo-strip`, `feature-list`) và nâng cấp Clean-Slate V1 (bỏ field cũ thay mới đồng bộ, không giữ legacy shim) cho `stats-bar` — song hành cùng `opendash-starter/quote-box` giữ nguyên.

### Thuật ngữ & Ký hiệu trong Tài liệu

- **Luồng A (Visual Builder & Theme Ergonomics):** Nhánh tính năng dựng trang và theme của OpenDash Studio, gồm Spec A1 (`BuilderDocument` AST), Spec A2 (Template Blocks & `core/loop-item`), và Spec A3 (Interactive Core Blocks & Starter Marketing Kit).
- **Mode 1 vs Mode 2 Studio:** Mode 1 (`#/studio`) chỉnh sửa Theme Options, Site Parts (`part:header`/`part:footer` — cấu hình via `DbOverride`) và Templates; Mode 2 (`#/studio/entry/:typeKey/:id`) chỉnh sửa nội dung từng Entry/Page qua Visual Builder lưu trong Comment Block (xem [`opendash-studio-customizer-and-ux-builder.md`](opendash-studio-customizer-and-ux-builder.md)).
- **`ActiveTheme` / `DbOverride` / `CoreFallback`:** Ba mức phân giải template công khai theo [`theme-format-and-template-resolver.md`](theme-format-and-template-resolver.md) (ghi đè từ DB thắng file theme đang kích hoạt, và fallback mặc định Core nếu không có).
- **Micro-JS Pack (`/assets/opendash-blocks.js`):** Tập lệnh JavaScript thuần duy nhất của Core nhúng trong binary server (`PUBLIC_BLOCKS_JS`), thực thi nâng cao tương tác cho `core/tabs`, `core/slider`, `core/gallery` lightbox, `core/social-share` và `core/popup` modal/drawer.
- **Server-Owned Conditional JS Gate (`needs_blocks_js`):** Cờ boolean nội bộ trên `RenderBudget`, `RenderedLayout` và `ResolvedPublicHtml` do renderer phía server bật một cách xác định, không bao giờ suy từ nội dung HTML văn bản.
- **Deferred Media Priority (`in_deferred_media`):** Cơ chế truyền ngữ cảnh từ `core/slide` (`idx >= 1`) hoặc `core/popup` (ẩn trước tương tác) xuống `core/image` con để gắn `loading="lazy" fetchpriority="low"`, bảo toàn băng thông cho ảnh đầu tiên / LCP.
- **`BRIDGE_SCRIPT` & `HEAD_END_MARKER`:** `BRIDGE_SCRIPT` là script nội bộ duy nhất chạy trong iframe sandbox `allow-scripts` của Studio Preview ([`BuilderPreviewFrame.tsx`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx)), nơi mọi `<script>` khác bị tước và hash SHA-256 phải khớp `BRIDGE_SCRIPT_SHA256`; `HEAD_END_MARKER` là thẻ đánh dấu vị trí kết thúc `<head>` tạm thời khi nối render hooks trước bước dọn ở biên ngoài cùng.
- **Quy trình kiểm chứng `Tầng 0` / `Tầng 1` / `/egm-drift-report`:** Theo [`AGENTS.md`](../../AGENTS.md) và [`docs/designs/README.md`](README.md), Tầng 0 kiểm biên dịch nhanh giữa các Slice trung gian (`run-tests.py --check`), Tầng 1 kiểm toàn bộ gate tự động + E2E trước PR (Bước 5–6), và `/egm-drift-report` đối chiếu sai lệch giữa code và spec.

---

## The Problem

1. **Thiếu các block tương tác & hộp thoại cốt lõi trên landing page, header mobile & bài viết:** OpenDash mới có `core/accordion` (tận dụng `<details>` HTML thuần). Người dựng trang chưa có khối chuyển tab (`core/tabs`), trình chiếu slide (`core/slider`), thư viện ảnh phóng lớn lightbox (`core/gallery`), dải chia sẻ mạng xã hội & sao chép liên kết không tracker (`core/social-share`), hay khối hộp thoại/ngăn trượt linh hoạt có inner block (`core/popup`) để mở ảnh/video, form đăng ký hoặc menu điều hướng mobile trái/phải từ nút bấm hoặc liên kết `#id`.
2. **Rủi ro hiệu năng, trợ năng (a11y) và bảo mật nếu tự nhúng JS nặng hoặc `<script>` thủ công:** Nếu thiếu chuẩn thống nhất, các block tương tác dễ tải JS thừa trên trang tĩnh, làm kẹt scrollbar/focus bằng modal tự chế, nhân đôi nội dung trình đọc màn hình do clone DOM carousel, lộ vệt trống ở panel ẩn `hidden="until-found"`, kéo tracker vi phạm public CSP `default-src 'none'`, gây selector injection từ `#anchor` hoặc vô tình chèn `<form method="dialog">` làm giảm độ chặt của CSP `form-action 'none'`.
3. **Bộ Starter Marketing còn mỏng (`stats-bar` cứng 3 ô):** `opendash-starter` mới có `quote-box` và `stats-bar` bản cứng 3 trường lặp (`stat_1_*`..`stat_3_*`), chưa có `icon-box`, `testimonial`, `logo-strip` hay `feature-list` để mẫu dựng trang đích hoàn chỉnh.

---

## Goals

1. Cung cấp `7` Core Blocks chuẩn `NativeRust`: `core/tabs` + `core/tab-panel`, `core/slider` + `core/slide`, `core/gallery` (kèm `<dialog closedby="any">` lightbox), `core/social-share` (zero tracker + Copy Link), và `core/popup` (container hỗ trợ children cho modal `center` & ngăn trượt drawer `left`/`right`/`top`/`bottom` kèm tùy biến chiều rộng responsive và `popup_id` kích hoạt qua nút/`#id`).
2. Giữ nguyên `0` dòng JS trên trang công khai không sử dụng tương tác client; chỉ tải duy nhất `/assets/opendash-blocks.js` (`< 12 KiB` raw, `defer`, same-origin) khi có nhu cầu thật, giữ nguyên CSP công khai chặt (`script-src 'self' 'unsafe-inline'`, `form-action 'none'` khi không có form liên hệ).
3. Tuân thủ nghiêm ngặt tiêu chuẩn Web hiện đại & WCAG 2.2: keyboard tablist + `hidden="until-found"` + `beforematch` sync; carousel CSS Scroll Snap + `overscroll-behavior-x: contain` + CSS `order` parking không clone DOM + `prefers-reduced-motion` + `fetchpriority="low"` cho ảnh ẩn trong slide/popup; `<dialog>` modal top-layer bản địa với `@starting-style` + `overlay` + `transition-behavior: allow-discrete` + `[data-closing]` fallback không dùng `<form method="dialog">`; `navigator.clipboard.writeText` + `legacyCopy` fallback không disable nút + live region ngoài nút cho Copy Link.
4. Đồng bộ trải nghiệm Studio Preview (`BRIDGE_SCRIPT` trong [`BuilderPreviewFrame.tsx`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx)): bấm chọn block trên canvas hoặc chọn `core/tab-panel` / `core/slide` / `core/popup` (hoặc node con lồng sâu bên trong) trên `StudioBlockTree` sẽ tự bật toàn bộ chuỗi tab/popup chứa node đó (không dùng `showModal()` để tránh cướp focus khỏi thanh Inspector) và cuộn các slider cha tương ứng vào khung nhìn; vẫn tước toàn bộ `<script>` khác và không mở modal khi bấm thumbnail gallery.
5. Tạo `4` Starter Marketing Blocks mới (`icon-box`, `testimonial`, `logo-strip`, `feature-list`) và nâng cấp Clean-Slate V1 `stats-bar` cho `opendash-starter` hoàn toàn `0` JS.
6. Không thêm SQL migration (`0` mới), không thêm crate Rust hay npm runtime dependency ngoài, giữ mọi file mã nguồn mới/sửa `<= 600` LOC (tuân thủ ratchet [`loc-baseline.json`](../../cms/scripts/loc-baseline.json)).

---

## Non-Goals

- Không biến `core/tabs`, `core/slider`, `core/gallery`, `core/social-share`, `core/popup` thành block hợp lệ bên trong `core/loop-item` (giữ `allowed_contexts: ["*", "!loop_item"]` và không sửa `LOOP_ITEM_CORE_ALLOWLIST` của Spec A2).
- Không triển khai thư viện animation bên ngoài, tự bật popup theo timer/exit-intent/cookie tracking, kéo chuột ép cuộn phá chọn văn bản trên desktop, hay API đếm lượt share bên ngoài.
- Không thêm cột `alt_text` / `caption` vào bảng SQLite `media_assets` (bảng hiện chỉ lưu metadata kỹ thuật); chú thích ảnh trong `core/gallery` cấu hình gọn qua trường `captions` theo node.
- Không giữ lớp tương thích ngược cho field cũ `stat_1_*`..`stat_3_*` của `opendash-starter/stats-bar` (áp dụng Clean-Slate V1 và cập nhật đồng bộ toàn bộ dữ liệu mẫu/test).
- Không sửa phạm vi Template Resolver, `Home → Page/Index`, `core/loop-item` hay `19` Template Blocks đang do Spec A2 sở hữu.

---

## Critical User Journeys (CUJs)

### CUJ 1: Dựng khối Tabs (`core/tabs` + `core/tab-panel`), tìm kiếm Ctrl/Cmd+F & Studio Preview
1. Người dùng chèn `core/tabs` (tự sinh 2 `core/tab-panel` mặc định) thông qua `BlockSchema` + `<UnifiedFieldForm>` sẵn có của Spec A1 (không cần sửa Admin Form Engine).
2. Trong Studio Canvas Preview (`sandbox="allow-scripts"` không chạy `opendash-blocks.js`), click trực tiếp trên canvas vẫn dùng để chọn block như mọi container khác; khi bấm chọn `core/tab-panel` thứ 2 (hoặc bất kỳ block con nào bên trong, kể cả tabs lồng tabs) từ `StudioBlockTree`, `BRIDGE_SCRIPT` tự động bỏ `hidden` + đặt `tabindex="0"` cho toàn bộ panel tổ tiên, đặt `hidden="until-found"` cho panel anh em và cập nhật `aria-selected="true"` cho nút tab tương ứng.
3. Trên trang công khai: người dùng nhấn `ArrowLeft`/`ArrowRight` (tự đảo theo RTL) hoặc `Home`/`End` để chuyển tab; khi bấm `Ctrl/Cmd+F` tìm chữ trong tab ẩn, sự kiện `beforematch` tự đồng bộ trạng thái tab mà không cướp focus.

### CUJ 2: Dựng Carousel/Slider (`core/slider` + `core/slide`) với CSS Scroll Snap & Ưu tiên ảnh LCP
1. Người dùng chèn `core/slider` (tạo sẵn 3 `core/slide` mỗi slide có heading mẫu), tùy chỉnh `slides_per_view` (`1 | 2 | 3` responsive desktop/tablet/mobile), `show_arrows`, `show_dots`, `loop`, `autoplay_seconds` (`0` = tắt hoặc `3..=12` giây). Ở chế độ hiển thị 1 slide (`1` trên mọi màn hình) kèm `loop: true` và `>= 3` slide, carousel lặp vô tận bằng CSS `order` parking không clone DOM; ở chế độ đa cột (`slides_per_view > 1`) hoặc `<= 2` slide, carousel chạy hữu hạn và tắt nút biên một cách an toàn để tránh cắt chữ hoặc nhấp nháy cuộn.
2. Mọi `core/image` bên trong `core/slide` có `index > 0` (kể cả lồng qua `core/card`/`core/column`) tự nhận `loading="lazy" fetchpriority="low"` để nhường băng thông cho slide đầu / LCP.
3. Trình duyệt vuốt ngang bằng CSS Scroll Snap (`overscroll-behavior-x: contain`); nút & dots đạt `>= 24x24px`; polite `role="status"` chỉ đọc khi cuộn dừng (`scrollend` hoặc fallback `scroll` debounce); autoplay có nút Pause/Play và tự dừng khi `prefers-reduced-motion: reduce`, hover, focus hoặc chuyển tab.

### CUJ 3: Dựng Thư viện ảnh (`core/gallery`) & Xem phóng lớn `<dialog closedby="any">`
1. Người dùng chèn `core/gallery`, chọn tối đa `24` ảnh qua `FieldType::Gallery`, chọn bố cục `grid` (`1..=6` cột responsive) hoặc `masonry` (CSS columns), tỉ lệ khung hình, `captions` tùy dòng và bật `lightbox: true`.
2. Trên trang công khai, mỗi ô `<figure>` bọc `<button type="button">` chứa `<picture>`/`<img>` responsive (nằm bên trong `<figure>` để hợp chuẩn HTML5 phrasing content) và `<figcaption>` tùy chọn. Bấm vào ảnh mở `<dialog closedby="any">` qua `showModal()` (không gắn `<form method="dialog">` để giữ CSP `form-action 'none'`), cho phép lùi/tới, bấm `Esc`, click nút Đóng hoặc click vùng tối ngoài (kể cả Safari fallback).
3. Nếu tắt `lightbox` hoặc thư viện chưa chọn ảnh nào, không bật `needs_blocks_js` (`0` JS).

### CUJ 4: Chia sẻ bài viết (`core/social-share`) với Zero Tracker & Nút Copy Link
1. Người dùng đặt `core/social-share` trên Single/Page/Landing Page/Pattern/Footer, bật Facebook, LinkedIn, Email, Copy Link (tùy chọn X, Telegram) hoặc nhập `custom_url` (giới hạn `http(s)://` hoặc `/path`).
2. Các nút chia sẻ là `<a target="_blank" rel="noopener noreferrer">` với URL/tiêu đề percent-encoded từ server (`0` script theo dõi ngoài); nút Copy Link cập nhật `<span role="status">` đặt **ngoài** `<button>`, swap icon, không disable nút khi hiển thị `"Đã sao chép"` và có fallback `legacyCopy` đồng bộ nếu thiếu HTTPS.

### CUJ 5: Trang đích sử dụng Bộ `5` Starter Marketing Blocks (`0` JS)
1. Người dùng chèn `icon-box`, `testimonial`, `stats-bar` (`repeater` V1), `logo-strip`, và `feature-list` trên một Page không có block tương tác nào.
2. Trang công khai render đúng responsive grid `--od-cols` và SVG icons `<symbol>`/`od_icon()`, giữ nguyên **`0` thẻ `<script src="/assets/opendash-blocks.js">`** và **`0` byte JS Core**.

### CUJ 6: Dựng `core/popup` (Modal Trung tâm, Form Đăng ký, Xem Video/Ảnh & Drawer Điều hướng Mobile)
1. Người dùng chèn `core/popup` vào bất kỳ Page hoặc Site Part (`part:header`/`part:footer`), chọn preset `Center Modal`, `Left Drawer` hoặc `Right Drawer` (hoặc cấu hình `top`/`bottom`), chỉnh chiều rộng responsive `--od-popup-w` (desktop/tablet/mobile), bật `full_height` cho thanh điều hướng mobile, đặt `popup_id` (ví dụ `menu-mobile`, `dang-ky-tu-van`), rồi thả các inner block tùy ý bên trong (`core/contact-form`, `core/image`, `core/video`, `core/heading`, `core/text`, `core/stack`, `core/button`...; lưu ý `core/menu` thuộc Spec A2 chỉ hỗ trợ ngữ cảnh `template`/`part` và URL HTTP(S) hiện tại).
2. Trong Studio Preview, `core/popup` hiển thị bằng thẻ inline gọn (`.od-popup-preview-chip`, `Popup #menu-mobile · left`, chừa khoảng trống bên phải cho `.od-canvas-gear`) nên không che hay phá chiều cao `part:header`; khi bấm chọn chính `core/popup` hoặc bất kỳ block con bên trong trên `StudioBlockTree`/canvas, `BRIDGE_SCRIPT` gắn `data-od-popup-preview-open="1"` + thuộc tính `open` để xem trực quan ngay trong iframe mà không cướp focus bàn phím khỏi cột Inspector và vẫn cho phép click vùng ngoài hộp thoại để chọn block khác.
3. Trên trang công khai: khi người dùng bấm vào bất kỳ `core/button`, liên kết trong `core/heading`/`core/text` hoặc thẻ `<a href="#menu-mobile">` / `[data-od-popup-trigger="menu-mobile"]` (hoặc mở URL kết thúc bằng `#menu-mobile` khi `open_on_hash: true`), `opendash-blocks.js` mở `<dialog>` top-layer mượt mà với `@starting-style` + `overlay`/`allow-discrete` (kèm `[data-closing]` + fallback `pointerdown`/`pointerup` ngoài `.od-popup-surface` cho Safari/Firefox), tạm dừng `<video>`/`<audio>` bên trong khi đóng, trả focus về đúng nút mở khi `lastTrigger.isConnected`, và tự đóng drawer nếu người dùng bấm một liên kết `#section` cùng trang bên trong thanh điều hướng. Nếu trang không chứa `core/popup` (và không có block tương tác khác), `0` JS được tải.

---

## Technical Plan (Architecture)

### 0. Phân tầng trên Spec A1 & Spec A2, Số lượng Core Blocks, `0` Migration & Ranh giới Merge Song song

1. **Tổng số Core Blocks & Tính độc lập khi phát triển song song:**
   - Trên `main` hiện hữu, `EMBEDDED_CORE_BLOCK_SOURCES` (`cms/crates/opendash-server/src/storage/page/types.rs`) đăng ký **`23` Core Blocks** (`18` `NativeRust` + `5` `CompiledCsec`).
   - Spec A3 thêm **`7` Core Blocks mới** (`core/tabs`, `core/tab-panel`, `core/slider`, `core/slide`, `core/gallery`, `core/social-share`, `core/popup`), đều có `render: "native"`, `allowed_contexts: ["*", "!loop_item"]` và **không** nằm trong `LOOP_ITEM_CORE_ALLOWLIST`:
     - Chạy & kiểm thử độc lập trên `feat/studio-interactive-and-starter-blocks` (trước khi Spec A2 merge): `23 + 7 = 30` Core Blocks (`25` `NativeRust` + `5` `CompiledCsec`).
     - Sau khi xếp chồng cùng Spec A2 (`19` Template Blocks mới trong đó có `core/entry-content` thay thế `core/content-slot`): `41 + 7 = 48` Core Blocks.
2. **Điểm chạm hợp nhất khi đồng bộ Spec A2 ở Bước 5/Bước 7:**
   - Spec A3 tránh đụng file template/loop của Spec A2, nhưng có 4 điểm chạm chung được xác định trước để rebase ở cuối Bước 5 / Bước 7 nhanh chóng, không bất ngờ:
     1. [`storage/page/types.rs`](../../cms/crates/opendash-server/src/storage/page/types.rs) & [`tests/builder/studio_unified_ast.rs`](../../cms/crates/opendash-server/tests/builder/studio_unified_ast.rs): gộp danh sách block Core thành `48` và cập nhật count `30 → 48` sau khi A2 merge.
     2. `template_sec/policy/preview.rs` (`PreviewCompositionBudget.locale`) & Spec A2 `render_index_home_response_for_locale`: đảm bảo route `Index-home` của Spec A2 cũng gọi `finalize_document_blocks_js` và truyền `page_canonical_url` / `page_title`.
     3. [`builtin_themes/opendash-starter/theme.json`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/theme.json), [`builtin_themes/mod.rs`](../../cms/crates/opendash-server/src/builtin_themes/mod.rs), [`tests/theme/starter_theme.rs`](../../cms/crates/opendash-server/tests/theme/starter_theme.rs) & [`opendash-starter/languages/opendash-starter.pot`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/languages/opendash-starter.pot) / `opendash-starter-vi.po`: Spec A2 giữ `"version": "1.0.0"` (bắt buộc bởi `starter_theme.rs` & `theme_l10n.rs`) và đổi `manifest_version` sang `1`, trong khi Spec A3 chỉ thêm `blocks/*`, đổi phần `sample.pages[home_page]` dùng `stats-bar` V1 và cập nhật `patterns/{hero-launchpad,features-bento-grid}.json`.
     4. **`core/menu` (thuộc Spec A2) & `storage/menu/resolve_and_write.rs`:** trên nền `main` độc lập chưa có `core/menu`, và `validate_external_url` của `site_menus` hiện chỉ cho phép `http(s)://`. Vì vậy các nút/liên kết mở popup (`#popup-id`) và điều hướng neo (`#section`) trong phạm vi Spec A3 sử dụng `core/button`, `core/heading`/`core/text` link hoặc thẻ `<a href="#id">`; việc mở rộng item menu trỏ `#fragment` (nếu cần sau khi A2 merge) được xử lý tách riêng.
   - **Toàn bộ icon dùng trong block/preset & UI điều khiển:** chỉ sử dụng các khóa hợp lệ thuộc `CORE_ICONS` qua `crate::template_sec::register_core_icon_in_budget(budget, name, class)` (`template_sec/policy/nonces.rs`; lưu ý khi đồng bộ Spec A2 sẽ chuẩn hóa tên gọi cùng helper `IconNonceTable::register_icon`), và dùng badge ký tự/chữ cái HTML `<span>` (tương tự `core/social-follow/render.html`, **0 thẻ `<svg>` thô**) cho Pause/Play và monogram mạng xã hội (`f`, `X`, `in`, `TG`) để không bao giờ bị từ chối bởi `document/normalize` khi `active_v3.is_none()`.
- **Ngân sách hệ thống:** `0` SQL migration (`+0` `sqlite_schema` objects); `0` dependency Cargo/npm runtime mới; `<= 600` LOC trên mọi file `.rs`/`.ts`/`.tsx`/`.js` mới hoặc sửa không thuộc ratchet baseline.

---

### 1. Kiến trúc Micro-JS Pack (`/assets/opendash-blocks.js`) & Chèn Có điều kiện Phía Server

#### 1.1. Tệp nguồn JS, Build Gate & Route Phục vụ
- **Nguồn:** `cms/admin/src/public-blocks/opendash-blocks.js` (`<= 600` LOC, ES2020 thuần không `import`, thực thi trong `(function(){ "use strict"; ... })();`, idempotent).
- **Script build deterministic:** `cms/admin/scripts/build-public-blocks-js.mjs` (được gọi qua `npm --prefix cms/admin run build:public-blocks-js`) sử dụng `esbuild` sẵn có trong `cms/admin/node_modules` (`minify: true, format: 'iife', target: 'es2020'`) để tạo `cms/crates/opendash-server/src/builtin_themes/opendash-blocks.js` kèm 3 kiểm tra tự động sau khi build:
  1. Dung lượng output `< 12 * 1024` bytes (`< 12 KiB` raw).
  2. Không chứa bất kỳ sink DOM/eval nguy hiểm nào (`innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `eval(`, `new Function`).
  3. Không chứa hằng chuỗi `http://` hoặc `https://` nào (bảo đảm `0` tracker, `0` URL cứng bên thứ 3; `core/social-share` dùng placeholder token do server tạo sẵn như mô tả ở §3.4).
- **Nhúng binary & HTTP Route ([`builtin_themes/mod.rs`](../../cms/crates/opendash-server/src/builtin_themes/mod.rs) & [`routes/builder.rs`](../../cms/crates/opendash-server/src/routes/builder.rs)):**
  ```rust
  pub const PUBLIC_BLOCKS_JS: &[u8] = include_bytes!("opendash-blocks.js");

  pub fn public_blocks_js_version_hash() -> &'static str {
      static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
      HASH.get_or_init(|| {
          let digest = sha2::Sha256::digest(PUBLIC_BLOCKS_JS);
          digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
      })
  }
  ```
  - Trong [`routes/builder.rs`](../../cms/crates/opendash-server/src/routes/builder.rs)`::serve_asset`, nhánh `clean_path == "opendash-blocks.js"` trả HTTP `200`, `Content-Type: text/javascript; charset=utf-8`, `X-Content-Type-Options: nosniff`, `Cache-Control: public, max-age=3600` — same-origin hợp lệ sẵn dưới public CSP `script-src 'self' 'unsafe-inline'`.

#### 1.2. Cơ chế `needs_blocks_js`, `used_entry_content` & Outer-Boundary Injector
1. **Cờ Server-Owned trên toàn bộ Pipeline Render:**
   - Thêm `pub needs_blocks_js: bool`, `pub page_canonical_url: Option<String>`, `pub page_title: Option<String>` vào `RenderBudget` ([`template_sec/nonces.rs`](../../cms/crates/opendash-server/src/template_sec/nonces.rs) & `Default`/`new()` tại [`template_sec/policy/preview.rs`](../../cms/crates/opendash-server/src/template_sec/policy/preview.rs)), lưu ý `PublicCompositionBudget` ([`storage/overrides/public_render.rs`](../../cms/crates/opendash-server/src/storage/overrides/public_render.rs)) bọc trực tiếp `render_budget: page::RenderBudget`.
   - Đồng bộ đầy đủ các trường này (cùng **`pub locale: String`** để vá luôn lỗi hardcode `"vi"` hiện hữu ở `as_render_budget()`) trong `PreviewCompositionBudget` (`with_render_budget`, `as_render_budget`, `sync_from_render_budget`).
   - Thêm `#[serde(skip_serializing)] pub needs_blocks_js: bool` và `#[serde(skip_serializing)] pub used_entry_content: bool` vào `RenderedLayout` ([`page/cache_types/types.rs`](../../cms/crates/opendash-server/src/page/cache_types/types.rs)), cập nhật tất cả `6` nơi khởi tạo `RenderedLayout` (`page/node_renderer.rs`, `render_public_builder.rs`, `resolve_template_2axis_on_connection.rs`, `render_preview_part.rs`, `preview_studio_with_cache_and_locale.rs`, `wrap_entry_body_in_preview_shell_in_tx_with_budget.rs`), đồng thời trong `render_public_builder.rs` cộng dồn cả `child_needs_js |= rendered.needs_blocks_js` từ `render_public_part` và theo dõi `env.needs_blocks_js` trên `RenderEnv` (`page/node_renderer.rs`) để bảo toàn nhất quán kể cả khi caller tự tạo `RenderBudget` riêng.
   - Thay chữ ký `render_native_runtime` ([`page/blocks_runtime.rs`](../../cms/crates/opendash-server/src/page/blocks_runtime.rs)) nhận `budget: &mut RenderBudget` và bổ sung `pub used_entry_content: bool` trên `RuntimeBlockOutput`, rồi ở `page/node_renderer.rs` hợp nhất `env.used_entry_content |= rt.used_entry_content; env.needs_blocks_js = budget.needs_blocks_js;`:
     - Trong `core/pattern-ref`: `env.budget.needs_blocks_js |= pattern.needs_blocks_js;` và `used_entry_content: pattern.used_entry_content`.
     - Trong `core/content-slot` (và `core/entry-content` khi nối với Spec A2): trả `used_entry_content: slot_html.is_some()` để đánh dấu nội dung entry thực sự được gắn vào template.
   - Bổ sung `pub fn has_entry_content(&self) -> bool { self.content_count > 0 }` trên `CompiledCsecComposition` ([`template_sec/cache.rs`](../../cms/crates/opendash-server/src/template_sec/cache.rs)), và thêm `pub needs_blocks_js: bool` + `pub used_entry_content: bool` trên `ResolvedPublicHtml` ([`routes/public_content/types.rs`](../../cms/crates/opendash-server/src/routes/public_content/types.rs)) cho cả `5` vị trí trả về:
     - Nhánh lỗi normalizer `Err(_)` (`types.rs#L197`, fallback về `fallback_html`) → `used_entry_content = true`, `needs_blocks_js = false`.
     - `TemplateResolution::CoreFallback` → `used_entry_content = true`, `needs_blocks_js = false`.
     - `TemplateResolution::ActiveTheme` → `used_entry_content = compiled.has_entry_content()`, `needs_blocks_js = false`.
     - `TemplateResolution::DbOverride` → `used_entry_content = rendered.used_entry_content`, `needs_blocks_js = rendered.needs_blocks_js`.
   - Tại `render_entry_response_with_cache`: ghi nhận `let entry_needs_js = entry_body.as_ref().is_some_and(|b| b.needs_blocks_js);`, reset `entry_render_budget.needs_blocks_js = false` trước khi dựng `PublicCompositionBudget::with_render_budget(entry_render_budget)`, và ở bước cuối hợp nhất `(entry_needs_js && body.used_entry_content) || body.needs_blocks_js || budget.render_budget.needs_blocks_js`. Trên `Archive`, `Taxonomy`, `Search`, `NotFound` hợp nhất trực tiếp `body.needs_blocks_js || budget.render_budget.needs_blocks_js`. Nhờ đó nếu Admin Override không chứa `core/content-slot` / `od_content()`, JS của `entry_body` ẩn không bị nạp nhầm!
2. **Outer-Boundary Safe Injector (`content_render/document_compose.rs`):**
   - `pub fn finalize_document_blocks_js(mut html: String, needs_blocks_js: bool) -> ApiResult<String>`:
     - Luôn dọn `HEAD_END_MARKER` nếu còn sót; khi `needs_blocks_js == false` trả `Ok(html)` nguyên trạng (`0` JS).
     - Khi `needs_blocks_js == true`: dùng bộ quét **`HtmlBodyScanner`** ([`template_sec/composition/html_scan_state.rs`](../../cms/crates/opendash-server/src/template_sec/composition/html_scan_state.rs)) mở rộng theo dõi trạng thái `<textarea>` (RCDATA), duyệt toàn bộ HTML để ghi nhận vị trí thẻ đóng `</body>` **cuối cùng** nằm ở trạng thái an toàn `scanner.is_safe_container_body() && !in_textarea` (tránh tuyệt đối `<script>`, `<style>`, `<!-- -->`, `<textarea>`, `<svg>`, `<foreignObject>` bên trong Trusted Theme HTML!), rồi chèn duy nhất 1 thẻ `<script src="/assets/opendash-blocks.js?v={hash}" defer></script>` ngay trước `</body>`.
   - Gọi `finalize_document_blocks_js` tại biên ngoài cùng trên đủ `5` public routes (`render_entry_response_with_cache.rs`, `enqueued_for_route.rs::render_canonical_archive` & `render_not_found_response_for_locale`, `taxonomy_render.rs`, `search_render.rs` — đồng thời bổ sung `finalize_document_images` còn thiếu trong `search_render.rs`; và nối thêm vào `render_index_home_response_for_locale` khi gộp Spec A2). Không bao giờ gọi ở Preview (`preview_studio_with_cache_and_locale.rs`, `preview_context.rs`, `wrap_entry_body_in_preview_shell_in_tx_with_budget.rs`, `render_entry_builder_preview.rs`, `routes/content.rs`).

---

### 2. Ngữ cảnh `RenderBudget`, `NativeParentInfo`, `media_render.rs` & `CoreL10n`

1. **Khắc phục lỗi `RenderBudget.locale` bị kẹt `"vi"` & Cấp `page_canonical_url`/`page_title`:**
   - Trong `load_entry_render_snapshot_with_options` (đồng thời cập nhật caller [`hydration_and_related.rs#L67`](../../cms/crates/opendash-server/src/storage/public_content/hydration_and_related.rs#L67) truyền `canonical_path` tường minh): **gọi helper mới `storage/public_content/entry_render_meta.rs` (`populate_entry_render_budget`) TRƯỚC câu lệnh `let body = if ...`** (giữ [`entry_lookup.rs`](../../cms/crates/opendash-server/src/storage/public_content/entry_lookup.rs) (`588` LOC) dưới trần `600` LOC, và bảo đảm các bài Classic RichText không có Builder `body` vẫn có đủ metadata cho `core/social-share` trong `DbOverride`/`part:footer`!). Helper điền `render_budget.locale = entry_locale.clone()`, `render_budget.page_title = Some(entry.title.clone())`, và `render_budget.page_canonical_url` thông qua hàm dùng chung `format_public_canonical_url(public_origin, path)`.
   - Trên các route còn lại (`Archive`, `Taxonomy`, `Search`, `404`): tất cả đều dùng chung `format_public_canonical_url(site_ctx.origin, path)` (gắn `https://...` nếu `pub_settings.public_origin` đã cấu hình để `core/social-share` giữ `needs_blocks_js = false` khi tắt Copy Link) và gán `budget.render_budget.locale = resolved_locale.clone()`, `page_title`, `page_canonical_url` **trước** khi gọi `resolve_public_template_html` và `resolve_and_render_header_footer_with_context_for_locale`:
     - `Archive`: path `canonical_base` (+ query đã chuẩn hóa nếu `query.page > 1`); `page_title = Some(schema.label.clone())`.
     - `Taxonomy`: path `canonical_base`; `page_title = Some(schema.label.clone())`.
     - `Search`: dời tính `page_title` lên **trước** `resolve_public_template_html` tại [`search_render.rs`](../../cms/crates/opendash-server/src/routes/public_content/search_render.rs), gắn URL qua `build_search_url` đã chuẩn hóa.
     - `NotFound (404)`: `page_canonical_url = format_public_canonical_url(site_ctx.origin, "/")`, `page_title = Some(l10n.not_found_title.to_string())` (an toàn khi đặt `core/social-share` trong `part:footer`, không phản chiếu URL lỗi 404).
   - Trên Preview ([`preview_studio_with_cache_and_locale.rs`](../../cms/crates/opendash-server/src/storage/overrides/candidates/preview_studio_with_cache_and_locale.rs), [`preview_context.rs`](../../cms/crates/opendash-server/src/storage/overrides/preview_context.rs) — giữ thay đổi `<= 5` dòng vì file đang ở `590/600` LOC, [`wrap_entry_body_in_preview_shell_in_tx_with_budget.rs`](../../cms/crates/opendash-server/src/storage/overrides/candidates/wrap_entry_body_in_preview_shell_in_tx_with_budget.rs), [`render_entry_builder_preview.rs`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/render_entry_builder_preview.rs), và [`entry_builder_snapshot.rs`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/entry_builder_snapshot.rs)): truyền đúng `effective_locale` / `current.locale` vào `RenderBudget.locale` & `PreviewCompositionBudget.locale` (giữ `page_canonical_url = None`).
2. **Mở rộng `NativeParentInfo<'a>` ([`page/blocks_layout.rs`](../../cms/crates/opendash-server/src/page/blocks_layout.rs), [`page/node_renderer.rs`](../../cms/crates/opendash-server/src/page/node_renderer.rs), [`page/blocks_leaf.rs`](../../cms/crates/opendash-server/src/page/blocks_leaf.rs), [`page/media_render.rs`](../../cms/crates/opendash-server/src/page/media_render.rs)):**
   ```rust
   #[derive(Debug, Clone, Copy, Default)]
   pub(super) struct NativeParentInfo<'a> {
       pub parent_node_scope: Option<&'a str>,
       pub parent_block_id: Option<&'a str>,
       pub layout_mode: Option<&'static str>,
       pub active_child_count: usize,
       pub child_index: usize,
       pub initial_active_index: usize,
       pub in_deferred_media: bool,
       pub in_popup: bool,
   }
   ```
   - Để [`node_renderer.rs`](../../cms/crates/opendash-server/src/page/node_renderer.rs) (`564` LOC) luôn an toàn dưới `<= 600` LOC, giữ thay đổi gọn tối đa bằng cách tách logic tính `child_native_parent` và dispatch `tabs`/`slider`/`popup` vào helper trong `blocks_interactive.rs` và `blocks_popup.rs` (cập nhật chữ ký `render_native_container` tại `blocks_layout.rs` nhận thêm `budget`, `is_preview`, `node_scope`, `l10n`, `native_parent`, và `render_native_leaf` tại `blocks_leaf.rs` nhận `native_parent`):
     - Với `core/tabs`: `initial_active_index = if active_count == 0 { 0 } else { default_tab.clamp(1, active_count) - 1 }` (chống panic khi xóa hết `tab-panel`), truyền `parent_node_scope = Some(&node_scope)`, `child_index = child_meta_idx`, `initial_active_index`.
     - Với `core/slider`: truyền `parent_node_scope = Some(&node_scope)`, `child_index = child_meta_idx`, `active_child_count = active_count`.
     - Lan truyền đệ quy `in_deferred_media`: `native_parent.in_deferred_media || (node.block_id == "core/slide" && native_parent.child_index > 0) || node.block_id == "core/popup"` và `in_popup`: `native_parent.in_popup || node.block_id == "core/popup"` xuống toàn bộ cây con (và giữ `env.in_popup` xuyên qua `core/pattern-ref` tại `blocks_runtime.rs`).
   - Trong `page/media_render.rs`: thêm `render_responsive_image_html_with_priority(..., loading: &str, fetchpriority: &str)` truyền xuống **cả** `ResponsivePictureInput` lẫn nhánh fallback `<img>` khi thiếu hydration row; `blocks_leaf.rs` (`core/image`) và `blocks_media_share.rs` (`core/gallery`) cùng đọc `native_parent.in_deferred_media` để truyền `loading="lazy", fetchpriority="low"` cho mọi ảnh nằm từ slide thứ 2 trở đi hoặc bên trong `core/popup` (slide đầu giữ mặc định `loading="lazy", fetchpriority=""`).
3. **Mở rộng `CoreL10n` ([`content_render/l10n.rs`](../../cms/crates/opendash-server/src/content_render/l10n.rs)) & Gettext thủ công (`cms/l10n/{opendash.pot,vi.po}`):**
   - Lưu ý quan trọng: `l10n/extract.rs` không tự quét `tr("...")` trong `CoreL10n`, còn `build.rs` biên dịch trực tiếp `cms/l10n/vi.po` thành `EMBEDDED_VI_MO`; do đó bổ sung `26` trường vào `CoreL10n`, tái sử dụng **`msgid "Email"` đã có sẵn** trong `opendash.pot` / `vi.po` (`share_email_label` trỏ tới `"Email"` để **không tạo duplicate `msgid`**), và thêm **25 `msgid` mới** dưới đây vào `cms/l10n/{opendash.pot,vi.po}` trong `S01`:
     - `tabs_default_label`: `"Tabs"` / `"Thẻ nội dung"`
     - `tabs_empty_preview`: `"Tabs: Add at least one Tab Panel."` / `"Tabs: Hãy thêm ít nhất một Tab Panel."`
     - `slider_default_label`: `"Image carousel"` / `"Trình chiếu"`
     - `slider_empty_preview`: `"Slider: Add at least one Slide."` / `"Trình chiếu: Hãy thêm ít nhất một Slide."`
     - `slider_slides_group`: `"Slides"` / `"Danh sách slide"`
     - `slider_prev`: `"Previous slide"` / `"Slide trước"`
     - `slider_next`: `"Next slide"` / `"Slide tiếp theo"`
     - `slider_dots_label`: `"Choose a slide"` / `"Chọn slide"`
     - `slider_dot_item`: `"Go to slide"` / `"Tới slide"`
     - `slider_slide_position_template`: `"Slide {n} of {total}"` / `"Slide {n} / {total}"`
     - `slider_pause`: `"Pause autoplay"` / `"Tạm dừng tự động chạy"`
     - `slider_play`: `"Play autoplay"` / `"Tiếp tục tự động chạy"`
     - `gallery_default_label`: `"Image gallery"` / `"Thư viện ảnh"`
     - `gallery_empty_preview`: `"Gallery: Select images in inspector."` / `"Thư viện ảnh: Hãy chọn ảnh ở cột thiết lập."`
     - `gallery_open_image`: `"View image"` / `"Xem ảnh"`
     - `gallery_lightbox_label`: `"Image viewer"` / `"Trình xem ảnh"`
     - `gallery_close`: `"Close viewer"` / `"Đóng trình xem"`
     - `social_share_label`: `"Share this page"` / `"Chia sẻ trang này"`
     - `share_email_label`: `"Email"` / `"Email"`
     - `copy_link`: `"Copy link"` / `"Sao chép liên kết"`
     - `copied`: `"Copied"` / `"Đã sao chép"`
     - `copy_failed`: `"Copy failed"` / `"Không thể sao chép"`
     - `share_empty_preview`: `"Social Share: Turn on at least one channel."` / `"Chia sẻ: Hãy bật ít nhất một kênh hoặc nút sao chép."`
     - `popup_default_label`: `"Popup dialog"` / `"Hộp thoại bật lên"`
     - `popup_close`: `"Close popup"` / `"Đóng hộp thoại"`
     - `popup_invalid_id_preview`: `"Popup: Set a valid ID (letters, digits, - or _)."` / `"Popup: Hãy đặt ID hợp lệ (chữ, số, - hoặc _)."`

---

### 3. Đặc tả Chi tiết `7` Core Blocks (`NativeRust`, `allowed_contexts: ["*", "!loop_item"]`)

Tất cả `7` block đều triển khai `render: "native"` tại `page/blocks_interactive.rs` (`core/tabs`, `core/tab-panel`, `core/slider`, `core/slide`), `page/blocks_media_share.rs` (`core/gallery`, `core/social-share`) và `page/blocks_popup.rs` (`core/popup`), dùng **chỉ** icon hợp lệ trong `CORE_ICONS` cho mọi `preset.icon` và áp dụng khuôn mẫu tách wrapper margin/padding của [`blocks_layout.rs#L435-456`](../../cms/crates/opendash-server/src/page/blocks_layout.rs#L435-L456) bất cứ khi nào block có cả hai trường `margin` và `padding`.

#### 3.1. `core/tabs` & `core/tab-panel` (`page/blocks_interactive.rs`)

- **`core/tabs`** (`category: "interactive"`, `icon: "folder"`, `supports_children: true`, `allowed_children: ["core/tab-panel"]`):
  - **`default_children`:** `2` `core/tab-panel` (`title: "Tab 1"` chứa `1 core/text`, `title: "Tab 2"` chứa `1 core/text` — `4` nodes `<= 8`, depth `2 <= 2`).
  - **`presets` (`CORE_ICONS` verified):**
    - `underline_horizontal`: `{ "icon": "minus", "props": { "style": "underline" } }`
    - `boxed_pills`: `{ "icon": "folder", "props": { "style": "pills" } }`
  - **`fields`:**
    - `style` (`select`: `"underline"` — gạch chân border đáy ::after, `"pills"` — nút bo tròn nền `bg-od-surface-alt`, `"boxed"` — khung viền nhóm `rounded-xl border border-od-border bg-od-surface-alt p-1`, default `"underline"`).
    - `default_tab` (`number`, `min: 1, max: 20, step: 1`, default `1`).
    - `aria_label` (`text`, `max_chars: 120`, default `""`, fallback `l10n.tabs_default_label`).
    - `padding` (`spacing`, `property: "padding"`, `unit: "px"`, `responsive: true`).
    - `margin` (`spacing`, `property: "margin"`, `unit: "px"`, `min: -128, max: 128`, `responsive: true`).
  - **Markup SSR & Wrapper Spacing:**
    - Nếu `active_children.len() == 0`: khi `is_preview == true`, render `<div class="od-tabs od-tabs-empty ..." data-od-tabs-root>{l10n.tabs_empty_preview}</div>`; khi công khai trả `""` (giữ `needs_blocks_js = false`).
    - Khi `active_children.len() >= 2 && !is_preview`: bật `budget.needs_blocks_js = true` (`1` tab chỉ render 1 panel hiện sẵn, giữ `needs_blocks_js = false`).
    - Bọc ngoài `<div class="od-tabs-wrap {mar_cls}"{if mar_fb} data-od-spacing="margin"{/if}>` chỉ khi `!mar_cls.is_empty() || mar_fb` (khớp chuẩn `blocks_layout.rs#L435-456` bảo đảm `data-od-spacing="margin"` tách biệt khỏi `data-od-spacing="padding"`), trong có `<div class="od-tabs od-tabs-{style} {pad_cls}" data-od-tabs-root{if pad_fb} data-od-spacing="padding"{/if}>`:
      ```html
      <div class="od-tablist flex flex-nowrap overflow-x-auto border-b border-od-border"
           role="tablist" focusgroup="tablist nomemory" aria-label="{aria_label}">
        <button type="button" role="tab" id="od-tab-{node_scope}-{idx}"
                class="od-tab shrink-0 relative inline-flex items-center gap-2 whitespace-nowrap px-4 py-2.5 text-sm font-medium"
                aria-selected="{true|false}"
                tabindex="{0|-1}"
                data-od-tab-index="{idx}"
                {if is_active}focusgroupstart{/if}>
          {optional_core_icon_via_budget}<span>{panel_title}</span>
        </button>
      </div>
      <div class="od-tab-panels">{children_nonce}</div>
      ```
    - Dùng `node_scope` (`{env.scope}-{node.id}`) trên `id="od-tab-{node_scope}-{idx}"` để không bao giờ trùng `id` nếu một Pattern có `core/tabs` được nhúng lặp nhiều lần.

- **`core/tab-panel`** (`category: "interactive"`, `icon: "file-text"`, `supports_children: true`, `allowed_parents: ["core/tabs"]`, `allowed_children: ["*", "!core/section", "!core/tab-panel"]`):
  - **`fields`:** `title` (`text`, `required: true, max_chars: 80`, default `"Tab mới"`), `icon` (`icon`), `padding` (`spacing`, `property: "padding"`, `unit: "px"`, `responsive: true`, default `{"top":16,"right":0,"bottom":0,"left":0}`).
  - **Markup SSR:**
    ```html
    <div role="tabpanel" class="od-tab-panel"
         aria-labelledby="od-tab-{parent_node_scope}-{child_index}"
         data-od-panel-index="{child_index}"
         {if child_index == initial_active_index}tabindex="0"{else}hidden="until-found"{/if}>
      <div class="od-tab-panel-inner {pad_cls}"{if pad_fb} data-od-spacing="padding"{/if}>{children_nonce}</div>
    </div>
    ```
  - **Cơ chế `opendash-blocks.js` cho Tabs (chống lồng nhau + `beforematch` + `#hash`):**
    - Mọi hàm truy vấn tab/panel trong một `tabsRoot` lọc nghiêm ngặt `el.closest('[data-od-tabs-root]') === tabsRoot`.
    - Kích hoạt một tab: gán `aria-selected="true"`, `tabindex="0"`, `focusgroupstart` cho tab chọn (các tab còn lại: `aria-selected="false"`, `tabindex="-1"`, bỏ `focusgroupstart`); panel chọn bỏ `hidden` + đặt `tabindex="0"`, các panel còn lại đặt `panel.setAttribute('hidden', 'until-found')` + bỏ `tabindex`; gọi `scrollTabIntoView` (an toàn try/catch cho `container: 'nearest'`).
    - Keyboard (`ArrowLeft`/`ArrowRight`/`ArrowUp`/`ArrowDown` tôn trọng `direction === 'rtl'`, `Home`, `End`) trên `[role="tab"]` di chuyển focus + kích hoạt ngay (selection follows focus).
    - `document.addEventListener('beforematch', ..., true)`: khi `target` có `closest('[role="tabpanel"][data-od-panel-index]')`, duyệt vòng lặp `while (panel)` lên tất cả `core/tabs` tổ tiên để kích hoạt từng cặp `(tabsRoot, idx)` mà không cướp focus khỏi đoạn văn bản `Ctrl/Cmd+F`.
    - Khi tải trang / `hashchange`: nếu `location.hash` khớp element `[id]`, dùng cùng vòng lặp `while (anc)` mở toàn bộ `[role="tabpanel"]` chứa mục tiêu.

#### 3.2. `core/slider` & `core/slide` (`page/blocks_interactive.rs`)

- **`core/slider`** (`category: "interactive"`, `icon: "arrow-right"`, `supports_children: true`, `allowed_children: ["core/slide"]`):
  - **`default_children`:** **`3` `core/slide`** mỗi slide chứa **`1` `core/heading`** (`"Slide 1"`, `"Slide 2"`, `"Slide 3"` → tổng `6 <= 8` nodes, depth `2 <= 2`, thỏa tuyệt đối `MAX_DEFAULT_CHILDREN_NODES` và `MAX_DEFAULT_CHILDREN_DEPTH`).
  - **`presets` (`CORE_ICONS` verified & đúng kiểu string cho `Select`):**
    - `hero_single`: `{ "icon": "eye", "props": { "slides_per_view": { "desktop": "1", "tablet": "1", "mobile": "1" }, "show_arrows": true, "show_dots": true, "loop": true } }`
    - `card_carousel_3`: `{ "icon": "arrow-right", "props": { "slides_per_view": { "desktop": "3", "tablet": "2", "mobile": "1" }, "show_arrows": true, "show_dots": true, "loop": false } }`
  - **`fields`:**
    1. `slides_per_view` (`select`, `options`: `"1" | "2" | "3"`, `responsive: true`, default `"1"`).
    2. `gap` (`range`, `min: 0, max: 48, step: 4`, default `16`, `unit: "px"`, `css_var: "--od-slider-gap"`).
    3. `show_arrows` (`boolean`, default `true`), `show_dots` (`boolean`, default `true`), `loop` (`boolean`, default `true`).
    4. `autoplay_seconds` (`range`, `min: 0, max: 12, step: 1`, default `0`, **không** gán `"unit"` vì `FieldSpec.unit` chỉ nhận `CssUnit` px/rem/em/%/vw/vh và `FieldType::Range` không nhận `append`; ghi `(giây, 0 = tắt)` trong `label`/`instructions` — bật autoplay trên mọi chế độ khi `> 0`, tự tắt khi `prefers-reduced-motion: reduce`).
    5. `aria_label` (`text`, `max_chars: 120`, default `""`, fallback `l10n.slider_default_label`).
    6. `margin` (`spacing`, `property: "margin"`, `unit: "px"`, `min: -128, max: 128`, `responsive: true`).
  - **Responsive CSS Variable Classes (`spv_classes`) & Empty State:**
    - Phân giải `responsive_str_cascade(&node.props, "slides_per_view", "1")` → `(is_resp, d, t, m)`. Clamp `"1" | "2" | "3"`.
    - Phát class ngữ nghĩa `.od-slider-cols-{m}` (và `md:od-slider-cols-{t}` / `lg:od-slider-cols-{d}` nếu responsive khác cấp), mỗi class chỉ gán `--od-spv: 1|2|3;` trong `tailwind-public.css`. Slide con dùng `flex: 0 0 calc((100% - (var(--od-spv, 1) - 1) * var(--od-slider-gap, 16px)) / var(--od-spv, 1))`.
    - Tính `is_single_spv = d == "1" && t == "1" && m == "1"` → gắn `data-od-slider-spv="{if is_single_spv { "1" } else { "multi" }}"`.
    - Nếu `active_children.len() == 0`: khi `is_preview == true` render `<div class="od-slider od-slider-empty ..." data-od-slider-root>{l10n.slider_empty_preview}</div>`; khi công khai trả `""` và giữ `needs_blocks_js = false`.
  - **Markup SSR & Conditional JS:**
    - Bật `budget.needs_blocks_js = true` chỉ khi `active_children.len() >= 2 && !is_preview`.
    ```html
    <section class="od-slider relative {spv_classes} {mar_cls}"
             data-od-slider-root
             data-od-slider-spv="{1|multi}"
             data-od-slider-loop="{true|false}"
             data-od-slider-autoplay="{seconds}"
             data-od-slider-count="{active_count}"
             data-od-label-slide="{l10n.slider_slide_position_template}"
             data-od-label-pause="{l10n.slider_pause}"
             data-od-label-play="{l10n.slider_play}"
             {if mar_fb}data-od-spacing="margin"{/if}
             aria-roledescription="carousel"
             aria-label="{aria_label}">
      {if show_arrows && active_count >= 2}
      <div class="od-slider-controls pointer-events-none ...">
        <button type="button" class="od-slider-btn pointer-events-auto" data-od-slide-dir="prev" aria-label="{l10n.slider_prev}">{core_icon("chevron-left")}</button>
        <button type="button" class="od-slider-btn pointer-events-auto" data-od-slide-dir="next" aria-label="{l10n.slider_next}">{core_icon("chevron-right")}</button>
      </div>
      {/if}
      <div class="od-slider-track flex overflow-x-auto" tabindex="0" role="group" aria-label="{l10n.slider_slides_group}">
        {children_nonce}
      </div>
      <p class="sr-only" role="status" aria-live="polite" data-od-slider-status></p>
      {if show_dots && active_count >= 2}
      <nav class="od-slider-dots flex items-center justify-center gap-2 mt-4" aria-label="{l10n.slider_dots_label}">
        <button type="button" class="od-slider-dot" data-od-slide-to="{idx}"
                aria-label="{l10n.slider_dot_item} {idx + 1}"
                {if idx == 0}aria-current="true"{/if}></button>
      </nav>
      {/if}
      {if autoplay_seconds > 0 && active_count >= 2}
        <button type="button" class="od-slider-autoplay-toggle ..." data-od-slider-toggle aria-pressed="false" aria-label="{l10n.slider_pause}">
          <span class="od-pause-glyph" aria-hidden="true">❚❚</span>
          <span class="od-play-glyph hidden" aria-hidden="true">▶</span>
        </button>
      {/if}
    </section>
    ```
  - **Cơ chế CSS & JS chuẩn `carousel` guide:**
    - Track: `scroll-snap-type: x mandatory`, `overscroll-behavior-x: contain` (cấm shorthand), `touch-action: manipulation`, `.od-slide { scroll-snap-align: start; min-height: var(--od-slide-min-h, 0px); }`.
    - **Looping & Finite Fallback:** Chỉ kích hoạt CSS `order` loop parking khi `loop == true && active_count >= 3 && spv === "1"` (`!CSS.supports("order: sibling-count()")` tự set `--children-count`); trong bước cập nhật `--current` & bù vị trí, tạm gắn class `.od-slider-parking` (`overflow-x: hidden; scroll-snap-type: none; scroll-behavior: auto !important;`) và gỡ sau 2 animation frames (không clone DOM, marker click tạm gỡ `--current` về `initial` để trượt đúng chiều). Khi `spv === "multi"` hoặc `!loop` hoặc `active_count < 3`, slider dùng chế độ hữu hạn: khóa `disabled` nút biên và **chuyển focus sang nút đối diện** nếu nút vừa bị `disabled` đang giữ focus.

- **`core/slide`** (`category: "interactive"`, `icon: "bookmark"`, `supports_children: true`, `allowed_parents: ["core/slider"]`, `allowed_children: ["*", "!core/section", "!core/slide", "!core/slider"]`):
  - **`fields`:** `min_height` (`range`, `min: 0, max: 720, step: 16`, default `0`, `unit: "px"`, `css_var: "--od-slide-min-h"`), `padding` (`spacing`, `property: "padding"`, `unit: "px"`, `responsive: true`).
  - **Markup SSR:**
    ```html
    <div class="od-slide min-w-0 shrink-0 {pad_cls}"
         data-od-slide-index="{child_index}"
         {if pad_fb}data-od-spacing="padding"{/if}
         role="group" aria-roledescription="slide"
         aria-label="{formatted_slide_position}">
      {children_nonce}
    </div>
    ```

#### 3.3. `core/gallery` (`page/blocks_media_share.rs`)

- **Metadata (`builtin_blocks/gallery/block.json`):** `category: "media"`, `icon: "eye"`, `supports_children: false`, `allowed_contexts: ["*", "!loop_item"]`.
- **`fields`:**
  - `images` (`gallery`, `media_kind: "image"`, `max_items: 24`)
  - `captions` (`textarea`, `rows: 4, max_chars: 4000`)
  - `layout` (`select`: `"grid"` | `"masonry"`, default `"grid"`)
  - `columns` (`select`: `"1" | "2" | "3" | "4" | "5" | "6"`, `responsive: true`, default `"3"`)
  - `gap` (`select`: `"8" | "16" | "24"`, default `"16"`)
  - `aspect_ratio` (`select`: `"auto" | "16_9" | "4_3" | "1_1" | "3_4"`, default `"4_3"`)
  - `radius` (`select`: `"none" | "sm" | "md" | "lg" | "xl"`, default `"lg"`)
  - `lightbox` (`boolean`, default `true`), `show_captions` (`boolean`, default `false`)
  - `padding` / `margin` (`spacing`, `responsive: true`)
- **Render & Hợp chuẩn HTML5 / CSP / Normalizer:**
  - Lọc `valid_images` có `valid_sha256_media_id` và `hydration_media.get(id)` với `kind == "image"`.
  - **Early return trước `needs_blocks_js`:** nếu `valid_images.is_empty()` → trả `""` (công khai) hoặc placeholder `l10n.gallery_empty_preview` (preview), giữ `needs_blocks_js = false`.
  - Độ ưu tiên giải `alt`/`caption` cho ảnh thứ `i`: lấy dòng thứ `i` từ `captions` (trim); nếu có dùng cho cả `alt` ảnh và `<figcaption>` (khi `show_captions == true`); nếu rỗng, `alt` dùng fallback `format!("{} {}", l10n.gallery_default_label, i + 1)` và ẩn `<figcaption>`.
  - Single Root Element: `<div class="od-gallery-wrap {mar_cls}"{if mar_fb} data-od-spacing="margin"{/if}><div class="od-gallery {pad_cls}"{if pad_fb} data-od-spacing="padding"{/if}> ...{items}... {optional <dialog>} </div></div>` (khi không có margin, bỏ outer `od-gallery-wrap`, giữ `<div class="od-gallery ...">` duy nhất bọc cả lưới và `<dialog>` để `apply_single_root_wrapper` không sinh thêm vỏ `<div>`).
  - Mode `"masonry"`: dùng CSS columns (`columns-{m} ... gap-{px}`) với thẻ `<figure class="break-inside-avoid {mb_cls}">` (`mb-2`/`mb-4`/`mb-6` khớp `gap`) và `0` JS.
  - Mode `lightbox == true && !is_preview`:
    - Bật `budget.needs_blocks_js = true`.
    - **Hợp chuẩn HTML5 phrasing content:** `<figure>` bọc ngoài `<button type="button" class="od-gallery-trigger block w-full overflow-hidden text-left {radius_cls}" data-od-lb-trigger data-od-lb-idx="{i}" data-od-lb-src="{full_url}" data-od-lb-alt="{alt_esc}" data-od-lb-caption="{cap_esc}" aria-label="{l10n.gallery_open_image}: {alt_or_idx}">{picture_html}</button>` (+ optional `<figcaption>` bên ngoài `<button>`!).
    - `<dialog closedby="any" class="od-lightbox-dialog" aria-label="{l10n.gallery_lightbox_label}">` ở cuối bên trong `.od-gallery`:
      - **KHÔNG dùng `<form method="dialog">`** (vì `contains_html_form_tag` tại `routes/public.rs:173` sẽ vô tình nới CSP sang `form-action 'self'` & `Referrer-Policy: same-origin`!). Thay bằng `<button type="button" class="od-lb-close" data-od-lb-close autofocus aria-label="{l10n.gallery_close}">{core_icon("x")}</button>`.
      - Có Prev/Next dùng `register_core_icon_in_budget(budget, "chevron-left" | "chevron-right", ...)` (`hidden` nếu `valid_images.len() == 1`), `data-od-lb-counter`, `<figure><img data-od-lb-img src="{first_full_url}" alt="{first_alt}" loading="lazy"><figcaption data-od-lb-caption>{first_cap}</figcaption></figure>`.
      - Trong `opendash-blocks.js` (chỉ chạy công khai): kiểm tra `src` qua regex chặt khớp `valid_media_render_url(..., false)`: `/^\/media\/[0-9a-f]{64}\/(file|thumbnail|variant\/[a-z0-9_.-]+)(\?v=[1-9]\d*)?$/` (loại bỏ mọi đường dẫn `/api/admin/media/` hoặc traversal), hỗ trợ `ArrowLeft`/`ArrowRight`, và Safari `closedby` fallback kiểm tra `pointerdown` + `pointerup` ngoài `getBoundingClientRect()`.

#### 3.4. `core/social-share` (`page/blocks_media_share.rs`)

- **Metadata (`builtin_blocks/social-share/block.json`):** `category: "marketing"`, `icon: "share-2"`, `supports_children: false`, `allowed_contexts: ["*", "!loop_item"]`.
- **`fields`:**
  - `heading` (`text`, `max_chars: 120`, default `""`).
  - `show_facebook` (`boolean`, default `true`), `show_x` (`boolean`, default `false`), `show_linkedin` (`boolean`, default `true`), `show_telegram` (`boolean`, default `false`), `show_email` (`boolean`, default `true`), `show_copy_link` (`boolean`, default `true`).
  - `custom_url` (`url`, default `""` — nếu nhập chỉ chấp nhận `http://`, `https://` hoặc `/path` không bắt đầu `//`; bỏ qua an toàn `#anchor`, `mailto:`, `tel:`).
  - `display_style` (`radio_buttons`: `"pill"` | `"icon_only"` | `"minimal"`, default `"pill"`).
  - `align` (`radio_buttons`, `responsive: true`: `"left"` | `"center"` | `"right"`, default `"left"`).
  - `padding` / `margin` (`spacing`, `responsive: true`).
- **Render, Origin Resolution & 0-Tracker:**
  - Nếu toàn bộ 6 công tắc kênh đều `false`: trả `l10n.share_empty_preview` khi `is_preview == true`, hoặc `""` (`needs_blocks_js = false`) khi công khai.
  - Khi `is_preview == true`: render liên kết mẫu an toàn `"https://example.com/"` (`needs_blocks_js = false`), tuyệt đối không rò slug bài nháp hay URL nội bộ admin.
  - Khi `!is_preview`: chọn URL đích từ `custom_url` hợp lệ hoặc `budget.page_canonical_url` (trên 404 fallback `"/"`). Nếu kết quả là URL tuyệt đối `http(s)://...`: render thẳng `<a href="{encoded_share_url}" target="_blank" rel="noopener noreferrer">`, giữ `needs_blocks_js = false` nếu `show_copy_link == false`.
  - Nếu chưa cấu hình `public_origin` nên kết quả là đường dẫn tương đối `/path`: Rust vẫn dựng mẫu link hoàn chỉnh trong `data-od-share-href="https://www.facebook.com/sharer/sharer.php?u=__OD_SHARE_URL__"` (tương ứng cho X, LinkedIn, Telegram, Mail) + `data-od-share-path="{path}"`, giữ `href="#share"` tạm thời và bật `budget.needs_blocks_js = true` để `opendash-blocks.js` thay thế `__OD_SHARE_URL__` bằng `encodeURIComponent(window.location.origin + path)` mà **vẫn giữ 0 chuỗi `http(s)://` trong file JS**!
  - Bọc ngoài kép tương tự `core/tabs` nếu cần cả `data-od-spacing="margin"` và `data-od-spacing="padding"`, align responsive dùng `justify-{start|center|end}` + `md:justify-*` + `lg:justify-*`.
  - Các nút chia sẻ Facebook/X/LinkedIn/Telegram dùng huy hiệu chữ monogram `<span>` (`f`, `X`, `in`, `TG`) hệt `core/social-follow/render.html` (`0` thẻ `<svg>` thô), nút Email dùng `register_core_icon_in_budget(budget, "mail", ...)`, và nút Copy Link dùng `register_core_icon_in_budget` cho `"link"` và `"check"` (hiện/ẩn khi có `data-copied`).
  - Khi `show_copy_link == true && !is_preview`: bật `budget.needs_blocks_js = true` và render nút Copy Link + `<span class="od-copy-tip" role="status"></span>` **bên ngoài** `<button>`, kích thước tối thiểu `min-w-[24px] min-h-[24px]`, dùng `navigator.clipboard.writeText` + `legacyCopy` đồng bộ (khôi phục focus `{ preventScroll: true }`), không disable nút, nối `\u00A0` tránh VoiceOver bỏ qua khi lặp lại.

#### 3.5. `core/popup` — Modal & Ngăn trượt Left/Right/Top/Bottom (`page/blocks_popup.rs`)

- **Metadata (`builtin_blocks/popup/block.json`):** `category: "interactive"`, `icon: "external-link"`, `supports_children: true`, `allowed_children: ["*", "!core/section", "!core/popup"]`, `allowed_contexts: ["*", "!loop_item"]` (đặt được trong Page/Post, Template lẫn Site Parts `part:header`/`part:footer` cho thanh điều hướng mobile hoặc CTA toàn site). Để chặn **cả lồng gián tiếp lẫn qua Pattern** (`core/popup → stack/row/tabs/pattern-ref → core/popup`), bổ sung `in_popup: bool` đệ quy trong `ValidationWalk::visit_nodes` ([`checks.rs`](../../cms/crates/opendash-core/src/builder/validation/checks.rs), `298` LOC) và `NativeParentInfo` / `RenderEnv` (`blocks_runtime.rs` bỏ qua render lồng nếu `core/pattern-ref` chứa popup bên trong popup).
- **`default_children`:** `2` node (`1 core/heading` `"Tiêu đề hộp thoại"` + `1 core/text` `"Thêm ảnh, video, biểu mẫu hoặc liên kết điều hướng vào đây."`) — depth `1 <= 2`, `2 <= 8` nodes.
- **`presets` (`CORE_ICONS` verified):**
  - `center_modal`: `{ "icon": "sparkles", "props": { "position": "center", "width": { "desktop": 560, "tablet": 520, "mobile": 340 }, "full_height": false, "backdrop_style": "blur", "radius": "xl" } }`
  - `left_drawer`: `{ "icon": "menu", "props": { "position": "left", "width": { "desktop": 360, "tablet": 340, "mobile": 300 }, "full_height": true, "backdrop_style": "dark", "radius": "none" } }`
  - `right_drawer`: `{ "icon": "arrow-left", "props": { "position": "right", "width": { "desktop": 400, "tablet": 360, "mobile": 320 }, "full_height": true, "backdrop_style": "dark", "radius": "none" } }`
- **`fields`:**
  - Tab Cấu hình & Kích hoạt:
    - `popup_id` (`text`, `required: true, max_chars: 48`, default `"popup-1"`, ghi chú hướng dẫn đặt URL `#popup-1` trên `core/button`, link `core/heading`/`core/text` hoặc `<a href="#popup-1">`). Server lược bỏ dấu `#` đầu nếu tác giả lỡ nhập và kiểm regex chặt `^[A-Za-z][A-Za-z0-9_-]{0,47}$`.
    - `aria_label` (`text`, `max_chars: 120`, default `""`, fallback `l10n.popup_default_label`).
    - `close_on_backdrop` (`boolean`, default `true`), `show_close_button` (`boolean`, default `true`), `open_on_hash` (`boolean`, default `true`). Khi `close_on_backdrop == false`, server **luôn bắt buộc** render nút Đóng để bảo đảm người dùng không bao giờ bị kẹt.
  - Tab Bố cục & Giao diện:
    - `position` (`radio_buttons`: `"center" | "left" | "right" | "top" | "bottom"`, default `"center"`).
    - `width` (`range`, `min: 240, max: 1200, step: 10`, default `560`, `unit: "px"`, `responsive: true`, `css_var: "--od-popup-w"`).
    - `full_height` (`boolean`, default `false` — khi `true` kéo suốt `100dvh` / `margin: 0` trên `left`/`right`/`center` và `min-height: 100dvh` trên `top`/`bottom`; khi `false`, giới hạn `max-height: min(90dvh, calc(100dvh - 2rem))` hoặc `85dvh` với `top`/`bottom`, cuộn dọc bên trong `.od-popup-surface`).
    - `backdrop_style` (`select`: `"dark" | "blur" | "light" | "none"`, default `"blur"`).
    - `bg_color` (`color`, `css_var: "--od-popup-bg"`, default `""` — qua `valid_color_value`, fallback `var(--od-token-color-surface, #ffffff)`).
    - `radius` (`select`: `"none" | "md" | "lg" | "xl" | "2xl"`, default `"xl"`).
    - `padding` (`spacing`, `property: "padding"`, `unit: "px"`, `responsive: true`, default `{"top":24,"right":24,"bottom":24,"left":24}`).
- **Markup SSR, CSP & Tái sử dụng Hạ tầng Sẵn có:**
  - Nếu `sanitize_popup_id` trả `None`: khi `is_preview == true` render `<div class="od-popup-host" data-od-popup-host><span class="od-popup-preview-chip od-popup-preview-invalid">{l10n.popup_invalid_id_preview}</span></div>`; khi công khai trả `""` và giữ `needs_blocks_js = false`.
  - Khi `safe_popup_id` hợp lệ: bật `budget.needs_blocks_js = true` nếu `!is_preview` và render cấu trúc Single Root Element (nhận `--od-popup-w` & `--od-popup-bg` từ `scoped_css` qua kế thừa CSS xuống `<dialog>` và `.od-popup-surface`):
    ```html
    <div class="od-popup-host" data-od-popup-host data-od-popup-id="{safe_popup_id}">
      {if is_preview}
      <div class="od-popup-preview-chip">Popup #{safe_popup_id} · {position}</div>
      {/if}
      <dialog id="od-popup-dlg-{node_scope}"
              class="od-popup-dialog"
              data-od-popup-dialog="{safe_popup_id}"
              data-od-popup-pos="{position}"
              data-od-popup-full-h="{true|false}"
              data-od-popup-backdrop="{backdrop_style}"
              data-od-popup-radius="{radius}"
              data-od-popup-backdrop-close="{true|false}"
              data-od-popup-hash="{true|false}"
              closedby="{if close_on_backdrop}any{else}closerequest{/if}"
              aria-modal="true"
              aria-label="{aria_label}">
        <div class="od-popup-surface">
          {if effective_show_close}
          <button type="button" class="od-popup-close" data-od-popup-close aria-label="{l10n.popup_close}">{core_icon("x")}</button>
          {/if}
          <div class="od-popup-body {pad_cls}"{if pad_fb} data-od-spacing="padding"{/if}>{children_nonce}</div>
        </div>
      </dialog>
    </div>
    ```
  - **Không `<form method="dialog">` & Không Chiếm Chỗ Flexbox Công khai:** Dùng `<button type="button" data-od-popup-close>` + `register_core_icon_in_budget(budget, "x", ...)` giữ chặt CSP `form-action 'none'` khi popup không chứa `core/contact-form`; `.od-popup-host:not([data-od-editable="1"])` áp dụng `display: contents` để không tạo khe trống `gap` hay cột thừa khi đặt trong `part:header` (`core/row`/`core/stack`) trong khi vẫn truyền `--od-popup-w` & `--od-popup-bg` xuống `<dialog>`.
  - **Kế thừa ưu tiên media (`in_deferred_media`):** Mọi `core/image` và `core/gallery` bên trong `core/popup` tự nhận `loading="lazy" fetchpriority="low"` nhằm bảo toàn băng thông ảnh LCP đầu trang.
  - **Hành vi `opendash-blocks.js` cho `core/popup`:**
    1. **Kích hoạt bằng `#id` hoặc `[data-od-popup-trigger]` (0 Selector Injection & URL Guard):** Bỏ qua nếu `e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey`, hoặc link mang `target="_blank"` / `download`. Với `<a href>`, phân giải qua `new URL(link.href, location.href)` và **chỉ khớp khi cùng tài liệu** (`url.origin === location.origin && url.pathname === location.pathname && url.search === location.search && url.hash.length > 1`, loại trừ `#` rỗng hay link trang khác/ngoài site); đối chiếu bằng vòng lặp so khớp chuỗi chính xác `dlg.getAttribute('data-od-popup-dialog') === candidateId`. Nếu khớp popup: `e.preventDefault()`, đóng popup đang mở khác, xóa `data-closing` + tăng `closeSeq`, gọi `dlg.showModal()`, lưu `lastTrigger`, đồng bộ `aria-controls`/`aria-expanded="true"`. Nếu là link `#section` cùng trang nằm bên trong popup đang mở và không trỏ popup: tự đóng popup để người dùng thấy section vừa cuộn tới.
    2. **Đóng mượt Top-Layer & Fallback Safari/Firefox (`overlay` + `closedby`):** Kiểm hỗ trợ đủ cả hai tính năng `CSS.supports('transition-behavior', 'allow-discrete') && CSS.supports('overlay', 'auto')`; nếu thiếu một trong hai (Firefox/Safari cũ), đặt `data-closing="1"` kèm token `closeSeq` chống mở-lại giữa chừng, và chờ `dlg.getAnimations({ subtree: true }).map(a => a.finished)` (gồm `.od-popup-surface`) trước khi gọi `dlg.close()`. Bổ sung fallback `pointerdown` + `pointerup` kiểm tọa độ ngoài `.od-popup-surface.getBoundingClientRect()` chỉ khi `data-od-popup-backdrop-close === "true"`. Trên sự kiện `close` (`Esc`), tự `.pause()` mọi `video, audio` con và chỉ trả focus khi `lastTrigger && lastTrigger.isConnected` (`preventScroll: true`).
    3. **Hash Load / `hashchange`:** Khi `open_on_hash === true` và `location.hash.slice(1)` khớp `popup_id`, tự mở popup sau khi DOM khởi tạo.

---

### 4. Đặc tả `5` Starter Marketing Blocks (`opendash-starter`, `CompiledCsec`, `0` JS)

Toàn bộ `5` block đặt tại `cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/<slug>/`, khai báo `"schema_version": 1`, `"category": "marketing"`, `"render": "render.html"`, giữ `"version": "1.0.0"` trên `theme.json`, vượt qua `validate_block_template_csec` và không kích hoạt `needs_blocks_js`.

- **Giải quyết Responsive Columns trong `CompiledCsec` (`stats-bar`, `logo-strip`, `feature-list`):** Vì `hydrate_field_values` làm phẳng `Select` responsive về giá trị desktop trong HTML nhưng [`page/scoped_css.rs#L156-197`](../../cms/crates/opendash-server/src/page/scoped_css.rs#L156-L197) tự động phát CSS variable qua `@media` cho mọi `Select` có `"css_var"` + `"responsive": true`, cả `3` block đều khai báo `"css_var": "--od-cols"`, `"responsive": true` trên `columns` và render thẻ lưới `<section/div class="od-responsive-grid grid ...">` dùng `grid-template-columns: repeat(var(--od-cols, 1), minmax(0, 1fr))` trong `tailwind-public.css`.
- **Bảo vệ Optional `Icon` dưới `UndefinedBehavior::Strict`:** Mọi vị trí dùng trường `icon` tùy chọn trong `icon-box`, `stats-bar`, `feature-list` bắt buộc kiểm tra `{% if icon and icon.name %}{{ od_icon(icon.name, ...) }}{% endif %}` trước khi gọi `od_icon` (theo chuẩn `builtin_blocks/card/render.html#L14`).
- **Default hợp lệ cho Required Fields & Repeater (`clipboard_and_insert.ts`):** Hiện tại `initialBlockProps` trong [`cms/admin/src/builder/document/clipboard_and_insert.ts#L43-47`](../../cms/admin/src/builder/document/clipboard_and_insert.ts#L43-47) bỏ qua `field.default` của `repeater` và tạo `[{}]` từ subfield default, khiến `pluginValuesReadyForFreshNode` từ chối node mới chèn nếu subfield `required: true` chưa có default. Vì vậy tại `S03`, spec vừa cập nhật `initialBlockProps` (khi `field.type === 'repeater'` và `Array.isArray(field.default)` thì clone trực tiếp `field.default`), vừa khai báo tường minh `default` không rỗng cho `icon-box.title`, `testimonial.quote`, `testimonial.author`, mọi subfield `required: true`, và `2..=4` item mẫu sẵn trong `default` của `stats-bar`, `logo-strip`, `feature-list`.

| Block ID | Icon (`CORE_ICONS`) | Schema V1 & Defaults | Hợp đồng `render.html` & Tác động Mẫu |
|---|---|---|---|
| `opendash-starter/icon-box` | `sparkles` | `icon` (`icon`, default `"sparkles"`), `icon_style` (`soft \| solid \| outline`), `title` (`text`, req, default `"Tính năng nổi bật"`), `description` (`textarea`, max `600`, default `"Mô tả ngắn gọn giá trị mang lại cho khách hàng."`), `cta` (`link`), `surface_style` (`card \| bordered \| flat`), `align` (`radio_buttons`: `left \| center`), `padding` (`spacing`, resp) | Guard `{% if icon and icon.name %}` → badge icon + tiêu đề + mô tả + CTA `cta.url`. Dùng thay thế/nâng cấp card mẫu trong `patterns/features-bento-grid.json`. |
| `opendash-starter/testimonial` | `message-circle` | `quote` (`textarea`, req, max `1200`, default `"OpenDash giúp đội ngũ chúng tôi ra mắt website nhanh gấp đôi."`), `author` (`text`, req, default `"Nguyễn Minh Anh"`), `role` (`text`, default `"Giám đốc sản phẩm"`), `company` (`text`), `avatar` (`media`, `image`), `rating` (`range`, `0..=5`, default `5`), `surface_style` (`card \| bordered`), `padding` (`spacing`, resp) | Render `<figure><blockquote>...</blockquote>` + `{% if rating > 0 %}` bọc loop tĩnh `{% for i in [1,2,3,4,5] %}{% if i <= rating %}{{ od_icon("star", "od-star-filled h-4 w-4 text-amber-500") }}{% endif %}{% endfor %}{% endif %}` (class `.od-star-filled` gắn trực tiếp `fill: currentColor` lên `<svg>` để kế thừa xuyên `<use href="#od-sym-star"/>` của Spec A2) + `{% if avatar and avatar.url %}{{ od_image(avatar, alt=author, class="h-11 w-11 rounded-full object-cover shrink-0", sizes="44px") }}{% endif %}`. Giữ nguyên `quote-box` song hành. |
| `opendash-starter/stats-bar` *(Nâng cấp V1)* | `arrow-up` | `items` (`repeater`, `1..=6`, default `3` mục `"100+"`/`"99%"`/`"24/7"`, subfields: `value` text req default `"100+"`, `label` text req default `"Khách hàng"`, `prefix` text, `suffix` text, `icon` icon), `columns` (`select` resp `"2" \| "3" \| "4"`, `"css_var": "--od-cols"`, default `"3"`), `surface_style` (`card \| bordered \| minimal`), `align` (`center \| left`), `padding` (`spacing`, resp) | **Clean-Slate V1:** bỏ hoàn toàn field `stat_1_*`..`stat_3_*` (tác động người dùng: nội dung cũ dùng schema tiền-V1 sẽ hiển thị theo default/empty nếu chưa cập nhật sang `items`; an toàn Clean-Slate trước rollout). Cập nhật đồng bộ `patterns/hero-launchpad.json`, `theme.json` sample `home_page`, `assets/style.css` và `tests/theme/starter_theme.rs`. |
| `opendash-starter/logo-strip` | `globe` | `heading` (`text`, max `120`), `items` (`repeater`, `1..=12`, default `4` đối tác mẫu, subfields: `name` text req default `"Đối tác"`, `image` media `image`, `url` url), `grayscale` (`boolean`, default `true`), `columns` (`select` resp `"3" \| "4" \| "5" \| "6"`, `"css_var": "--od-cols"`, default `"4"`), `padding` (`spacing`, resp) | `{% for logo in items %}` → nếu `logo.image` dùng `{{ od_image(logo.image, alt=logo.name, class=..., sizes="160px") }}`, ngược lại hiện badge chữ `{{ logo.name }}`; bọc `<a href="{{ logo.url }}" target="_blank" rel="noopener noreferrer">` nếu `logo.url` có giá trị (đã xác minh `block_url_scopes.rs` & `image_nonces.rs` hỗ trợ `Repeater → Group → Media/Url`). |
| `opendash-starter/feature-list` | `check-circle` | `heading` (`text`), `items` (`repeater`, `1..=16`, default `3` mục mẫu, subfields: `title` text req default `"Hiệu năng cao"`, `description` textarea, `icon` icon default `"check-circle"`), `columns` (`select` resp `"1" \| "2"`, `"css_var": "--od-cols"`, default `"1"`), `dividers` (`boolean`, default `true`), `icon_color` (`primary \| emerald \| neutral`), `padding` (`spacing`, resp) | `<section><ul role="list" class="od-responsive-grid grid ...">` + guard `{% if item.icon and item.icon.name %}{{ od_icon(item.icon.name, ...) }}{% endif %}` + tiêu đề & mô tả. |

- Cập nhật [`opendash-starter/languages/opendash-starter.pot`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/languages/opendash-starter.pot) và [`opendash-starter-vi.po`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/languages/opendash-starter-vi.po) (chú ý tên file dùng dấu gạch ngang `-vi.po`).

---

### 5. Studio Preview `BRIDGE_SCRIPT` & Public Tailwind CSS (`BuilderPreviewFrame.tsx` + `tailwind-public.css`)

1. **Cập nhật `BRIDGE_SCRIPT` trong [`BuilderPreviewFrame.tsx`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx):**
   - Trong `applyAttr(attr, id, doScroll)` khi `attr === "data-od-selected"`, **chạy bước dọn trước câu lệnh `if (!id) return;`**: gỡ `data-od-popup-preview-open` khỏi mọi `[data-od-popup-host]` và gỡ `open` trên phần tử con `dialog.od-popup-dialog` (duyệt `children` thay cho `:scope >` và có null-guard khi preview lỗi `popup_id` rỗng); khi leo chuỗi tổ tiên `while (anc)`:
     - Mở `<details>` như hiện tại (`anc.tagName === "DETAILS" -> anc.open = true`).
     - Nếu `anc.hasAttribute('data-od-popup-host')`: gắn `anc.setAttribute('data-od-popup-preview-open', '1')` và tìm thẻ con trực tiếp `DIALOG` để đặt thuộc tính `open` (null-safe, không gọi `showModal()` để tránh cướp focus bàn phím khỏi Inspector; bật popup trước bước `scrollTo` của slider bên trong để slider có kích thước hợp lệ).
     - Nếu `anc.hasAttribute('role') && anc.getAttribute('role') === 'tabpanel'`: tìm `tabsRoot = anc.closest('[data-od-tabs-root]')`; tìm mọi `[role="tabpanel"]` và `[role="tab"]` thuộc trực tiếp `tabsRoot` (`el.closest('[data-od-tabs-root]') === tabsRoot`); bỏ `hidden` & đặt `tabindex="0"` trên `anc`, đặt `hidden="until-found"` & gỡ `tabindex` trên anh em cùng `tabsRoot`, và đồng bộ `aria-selected="true"/"false"` + `tabindex` trên nút `[role="tab"]` khớp `data-od-tab-index === panelIndex`. **Quá trình leo `while (anc)` tiếp tục lên trên** để mở đúng cả tab cha khi dùng tabs lồng tabs!
     - Nếu `anc.hasAttribute('data-od-slide-index')`: tìm `sliderRoot = anc.closest('[data-od-slider-root]')` và cuộn `track` thuộc trực tiếp `sliderRoot` (`el.closest('[data-od-slider-root]') === sliderRoot`) bằng `track.scrollTo({ left: track.scrollLeft + (anc.getBoundingClientRect().left - track.getBoundingClientRect().left), behavior: 'auto' })` (tương thích cả LTR/RTL và không xung đột với `scroll-behavior: smooth` của track khi `nodes[j].scrollIntoView` chạy nối tiếp). Tiếp tục leo `while (anc)` nếu có slider cha ngoài cùng.
   - Cập nhật lại hằng số `BRIDGE_SCRIPT_SHA256` khớp chính xác SHA-256 base64 của `BRIDGE_SCRIPT` (đã có kiểm tra tự động trong [`test-studio-ux.mjs`](../../cms/admin/scripts/test-studio-ux.mjs)).
2. **Mở rộng [`tailwind-public.css`](../../cms/admin/src/tailwind-public.css) & [`tailwind-public-safelist.txt`](../../cms/admin/src/tailwind-public-safelist.txt):**
   - **Quy tắc semantic mới trong `tailwind-public.css`:**
     - `.od-responsive-grid { grid-template-columns: repeat(var(--od-cols, 1), minmax(0, 1fr)); }`
     - Tabs: `[role="tab"][aria-selected="true"]::after` (border-block-end `2px solid var(--od-token-color-primary)`, tương thích Forced Colors), `.od-tab-panel { padding: 0; border: 0; margin: 0; background: transparent; }`, `.od-tab-panel[hidden="until-found"] { content-visibility: hidden; }`.
     - Slider: `.od-slider-cols-1 { --od-spv: 1; }`, `.od-slider-cols-2 { --od-spv: 2; }`, `.od-slider-cols-3 { --od-spv: 3; }` cùng biến thể `@media (min-width: 768px)` (`md:od-slider-cols-*`) và `@media (min-width: 1024px)` (`lg:od-slider-cols-*`), `.od-slider-track { display: flex; overflow-x: auto; scroll-snap-type: x mandatory; overscroll-behavior-x: contain; gap: var(--od-slider-gap, 16px); touch-action: manipulation; }`, `.od-slide { flex: 0 0 calc((100% - (var(--od-spv, 1) - 1) * var(--od-slider-gap, 16px)) / var(--od-spv, 1)); scroll-snap-align: start; min-height: var(--od-slide-min-h, 0px); }`, `@media (prefers-reduced-motion: no-preference) { .od-slider-track { scroll-behavior: smooth; } }`, `.od-slider-parking .od-slider-track { overflow-x: hidden; scroll-snap-type: none; scroll-behavior: auto !important; }`, quy tắc CSS `order` loop chỉ cho `[data-od-slider-spv="1"][data-od-slider-loop="true"] .od-slider-track` với `var(--children-count, sibling-count())`, nút & dots kích thước chạm `>= 24x24px` + `:focus-visible`.
     - Lightbox `<dialog>`: `dialog[open].od-lightbox-dialog { display: flex; ... }`, `dialog.od-lightbox-dialog:modal { overscroll-behavior: contain; }`, `dialog.od-lightbox-dialog::backdrop { background: rgba(15,23,42,.82); overflow: hidden; overscroll-behavior: contain; }`.
     - **`core/popup` Top-Layer Modal & Drawer:** Xóa mặc định UA trên `dialog.od-popup-dialog` (`margin: 0; border: 0; padding: 0; background: transparent; max-width: 100vw; max-height: 100dvh; color: inherit;`), giữ `display: flex;` trên `dialog.od-popup-dialog[open]` (không đặt điều kiện `:not([data-closing])` lên `display` để không gãy flex giữa lúc đóng); bao gồm `overlay` và `display` trong `transition-property`, đặt `transition-behavior: allow-discrete` ở khai báo riêng, áp transform theo hướng `center`/`left`/`right`/`top`/`bottom` chỉ trong `@media (prefers-reduced-motion: no-preference)` (rút về 120ms opacity fade dưới `prefers-reduced-motion: reduce`), và đặt opacity/transform trạng thái mở trong `:is([open]):where(:not([data-closing]))` bọc `@starting-style` + `&::backdrop`. `.od-popup-surface` dùng `display: flex; flex-direction: column; width: 100%; max-height: inherit; height: 100%; overflow-y: auto; overscroll-behavior: contain; background-color: var(--od-popup-bg, var(--od-token-color-surface, #ffffff));`, `.od-popup-close` `position: sticky; align-self: flex-end; inset-block-start: 0.75rem; flex-shrink: 0; min-width: 24px; min-height: 24px;`, định cỡ `width: min(var(--od-popup-w, 560px), calc(100vw - 2rem))` (`92vw` với drawer trái/phải, `100vw` với `top`/`bottom`), và kiểu trực tiếp trên `[data-od-popup-backdrop]`. Ở Studio Preview: `.od-popup-host:not([data-od-editable="1"]) { display: contents; }`, `.od-popup-preview-chip` chừa `padding-inline-end: 2.25rem` tránh đè `.od-canvas-gear`, `.od-popup-host[data-od-popup-preview-open="1"] { z-index: 60; }`, và `dialog.od-popup-dialog[open]` trong preview đặt `pointer-events: none` (còn `.od-popup-surface { pointer-events: auto; }`) để click vùng ngoài trên canvas không bị kẹt.
     - Copy tip & Star icon: `.od-copy-tip` điều khiển hiển thị hoàn toàn bằng `opacity` theo `[data-copied]`/`[data-failed]` trên button trước nó; `.od-star-filled { fill: currentColor; }` (tương thích `<symbol><use/>`).
   - **Bổ sung đầy đủ vào [`tailwind-public-safelist.txt`](../../cms/admin/src/tailwind-public-safelist.txt):** `sr-only`, `columns-1`..`columns-6`, `md:columns-1`..`md:columns-6`, `lg:columns-1`..`lg:columns-6`, `break-inside-avoid`, `rounded-sm`, `mb-2`, `mb-4`, `mb-6`, `text-amber-500`, `md:justify-start`, `md:justify-center`, `md:justify-end`, `lg:justify-start`, `lg:justify-center`, `lg:justify-end`, `grayscale`, `hover:grayscale-0`.
   - Build kiểm chứng `opendash-tailwind.css` giữ dung lượng **`< 120 KiB`** (nền hiện tại `~75 KiB`).

---

## Alternatives Considered

1. **Nạp `opendash-blocks.js` trên mọi trang công khai:** *Từ chối.* Lãng phí băng thông/CPU cho trang tĩnh và khó kiểm chứng biên `0 JS`. Cờ server-owned `needs_blocks_js` trên `RenderBudget`/`RenderedLayout` vừa chính xác 100% vừa miễn nhiễm HTML spoofing.
2. **Dùng `display: none` thay cho `hidden="until-found"` trên tab ẩn:** *Từ chối.* Làm trình duyệt không thể tìm chữ trong tab ẩn khi nhấn `Ctrl/Cmd+F`. Dùng `hidden="until-found"` + inner wrapper `.od-tab-panel-inner` giữ được tìm kiếm mà vẫn chống vệt trống padding.
3. **Clone DOM trong carousel hoặc kéo thư viện JS ngoài:** *Từ chối.* Clone DOM nhân đôi nội dung cho screen reader và phá ID con; CSS Scroll Snap + `order` parking theo `--current` gọn nhẹ, `0` dependency.
4. **Dùng `<form method="dialog">` bên trong `<dialog>` lightbox hoặc popup:** *Từ chối.* Hàm `document::contains_html_form_tag` trong [`routes/public.rs`](../../cms/crates/opendash-server/src/routes/public.rs#L173) tự động nới CSP sang `form-action 'self'` và `Referrer-Policy: same-origin` nếu gặp bất kỳ thẻ `<form` nào. Dùng `<button type="button" data-od-lb-close>` / `<button type="button" data-od-popup-close>` giữ vững CSP `form-action 'none'` chặt chẽ.
5. **Gọi `showModal()` trong Studio Preview hoặc đặt `id="{popup_id}"` trực tiếp lên `<dialog>` của `core/popup`:** *Từ chối.* Gọi `showModal()` trong iframe preview sẽ cướp focus bàn phím khỏi thanh Inspector của Studio, còn đặt `id` trùng tên user lên `<dialog>` dễ trùng với `meta.html_id` trên outer wrapper và làm trình duyệt nhảy cuộn bất ngờ; dùng `data-od-popup-preview-open="1"` + `open` attribute ở Preview và `data-od-popup-dialog="{safe_popup_id}"` ở runtime công khai giải quyết triệt để cả hai.

---

## Non-Functional Requirements

- **Dung lượng & Hiệu năng:** `opendash-blocks.js` minified **`< 12 KiB`** (`0` bytes trên trang tĩnh), `opendash-tailwind.css < 120 KiB`, `0` truy vấn SQL mới hoặc N+1 (tái sử dụng batch `load_media_hydration_map_in_conn`).
- **Bảo mật & CSP:** `0` tracker/request ngoài, regex kiểm URL ảnh lightbox khớp `valid_media_render_url(..., false)`, `popup_id` khóa chặt `^[A-Za-z][A-Za-z0-9_-]{0,47}$`, khớp chuỗi thuần không nội suy CSS selector và chỉ chặn/mở link `#id` cùng tài liệu (`origin + pathname + search`, bỏ `#` rỗng/`_blank`/modifier), `0` sink `innerHTML`/`eval`, `0` chuỗi `https://` trong JS, giữ chặt `form-action 'none'` (trừ khi trang có form liên hệ thật).
- **Trợ năng WCAG 2.2 & Đa trình duyệt:** Target chạm `>= 24x24px`, chỉ báo active Forced-Colors-safe, tôn trọng `prefers-reduced-motion: reduce`, đầy đủ fallback cho Safari/Firefox (`closedBy` ngoài `.od-popup-surface`, top-layer `[data-closing]` kích hoạt khi thiếu `allow-discrete` hoặc `overlay` với `getAnimations({ subtree: true })`, `scrollend`, `sibling-count()`, `navigator.clipboard`, `beforematch`).
- **Quy chuẩn LOC:** Toàn bộ file mã nguồn giữ `<= 600` LOC (tách `blocks_interactive.rs`, `blocks_media_share.rs`, `blocks_popup.rs`, `entry_render_meta.rs` giữ `entry_lookup.rs` (`588` LOC), `node_renderer.rs` (`564` LOC) và `preview_context.rs` (`590` LOC, chỉ thêm `<= 5` dòng) an toàn dưới trần `600`).

---

## Detailed Implementation Plan

### Target Files to Create/Modify

*(Lưu ý: các đường dẫn chưa tồn tại ở thời điểm `Draft` được viết trong backtick để vượt qua liên kết tĩnh `check-plans.py`; chúng được tạo tại Slice S01–S04.)*

1. **Micro-JS & CSS Assets (`cms/admin/`)**:
   - Create `cms/admin/src/public-blocks/opendash-blocks.js` (`<= 600` LOC)
   - Create `cms/admin/scripts/build-public-blocks-js.mjs`
   - Create `cms/admin/scripts/test-public-blocks-js.mjs`
   - Edit [`package.json`](../../cms/admin/package.json), [`tailwind-public.css`](../../cms/admin/src/tailwind-public.css), [`tailwind-public-safelist.txt`](../../cms/admin/src/tailwind-public-safelist.txt)
   - Create `cms/crates/opendash-server/src/builtin_themes/opendash-blocks.js` & regenerate [`opendash-tailwind.css`](../../cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css)
2. **Server Plumbing, Budget, L10n & Outer Boundary (`cms/crates/opendash-server/src/` & `cms/l10n/`)**:
   - Edit [`builtin_themes/mod.rs`](../../cms/crates/opendash-server/src/builtin_themes/mod.rs), [`routes/builder.rs`](../../cms/crates/opendash-server/src/routes/builder.rs)
   - Edit [`template_sec/nonces.rs`](../../cms/crates/opendash-server/src/template_sec/nonces.rs), [`template_sec/policy/preview.rs`](../../cms/crates/opendash-server/src/template_sec/policy/preview.rs), [`template_sec/cache.rs`](../../cms/crates/opendash-server/src/template_sec/cache.rs), [`template_sec/composition/html_scan_state.rs`](../../cms/crates/opendash-server/src/template_sec/composition/html_scan_state.rs)
   - Edit [`page/cache_types/types.rs`](../../cms/crates/opendash-server/src/page/cache_types/types.rs), [`content_render/document_compose.rs`](../../cms/crates/opendash-server/src/content_render/document_compose.rs), [`content_render/l10n.rs`](../../cms/crates/opendash-server/src/content_render/l10n.rs), [`opendash.pot`](../../cms/l10n/opendash.pot), [`vi.po`](../../cms/l10n/vi.po)
   - Create `cms/crates/opendash-server/src/storage/public_content/entry_render_meta.rs` (`<= 600` LOC) & Edit [`storage/public_content/mod.rs`](../../cms/crates/opendash-server/src/storage/public_content/mod.rs), [`entry_lookup.rs`](../../cms/crates/opendash-server/src/storage/public_content/entry_lookup.rs), [`hydration_and_related.rs`](../../cms/crates/opendash-server/src/storage/public_content/hydration_and_related.rs)
   - Edit [`routes/public_content/types.rs`](../../cms/crates/opendash-server/src/routes/public_content/types.rs), [`render_entry_response_with_cache.rs`](../../cms/crates/opendash-server/src/routes/public_content/assets_and_sitemap/render_entry_response_with_cache.rs), [`enqueued_for_route.rs`](../../cms/crates/opendash-server/src/routes/public_content/assets_and_sitemap/enqueued_for_route.rs), [`taxonomy_render.rs`](../../cms/crates/opendash-server/src/routes/public_content/taxonomy_render.rs), [`search_render.rs`](../../cms/crates/opendash-server/src/routes/public_content/search_render.rs)
   - Edit [`render_public_builder.rs`](../../cms/crates/opendash-server/src/storage/overrides/resolve_2axis/render_public_builder.rs), [`resolve_template_2axis_on_connection.rs`](../../cms/crates/opendash-server/src/storage/overrides/resolve_2axis/resolve_template_2axis_on_connection.rs), [`preview_context.rs`](../../cms/crates/opendash-server/src/storage/overrides/preview_context.rs), [`preview_studio_with_cache_and_locale.rs`](../../cms/crates/opendash-server/src/storage/overrides/candidates/preview_studio_with_cache_and_locale.rs), [`render_preview_part.rs`](../../cms/crates/opendash-server/src/storage/overrides/candidates/render_preview_part.rs), [`wrap_entry_body_in_preview_shell_in_tx_with_budget.rs`](../../cms/crates/opendash-server/src/storage/overrides/candidates/wrap_entry_body_in_preview_shell_in_tx_with_budget.rs), [`entry_builder_snapshot.rs`](../../cms/crates/opendash-server/src/storage/builder/read_and_preview/entry_builder_snapshot.rs), [`render_entry_builder_preview.rs`](../../cms/crates/opendash-server/src/storage/builder/context_and_lists/render_entry_builder_preview.rs)
3. **Core Blocks Schemas, Validation, Native Renderers & Builder Tests (`cms/crates/opendash-core/` & `cms/crates/opendash-server/`):**
   - Create `cms/crates/opendash-server/src/builtin_blocks/{tabs,tab-panel,slider,slide,gallery,social-share,popup}/block.json`
   - Create `cms/crates/opendash-server/src/page/blocks_interactive.rs` (`<= 600` LOC), `cms/crates/opendash-server/src/page/blocks_media_share.rs` (`<= 600` LOC), & `cms/crates/opendash-server/src/page/blocks_popup.rs` (`<= 600` LOC)
   - Edit [`validation/checks.rs`](../../cms/crates/opendash-core/src/builder/validation/checks.rs), [`page/mod.rs`](../../cms/crates/opendash-server/src/page/mod.rs), [`node_renderer.rs`](../../cms/crates/opendash-server/src/page/node_renderer.rs), [`blocks_layout.rs`](../../cms/crates/opendash-server/src/page/blocks_layout.rs), [`blocks_leaf.rs`](../../cms/crates/opendash-server/src/page/blocks_leaf.rs), [`blocks_runtime.rs`](../../cms/crates/opendash-server/src/page/blocks_runtime.rs), [`media_render.rs`](../../cms/crates/opendash-server/src/page/media_render.rs), [`storage/page/types.rs`](../../cms/crates/opendash-server/src/storage/page/types.rs)
   - Create `cms/crates/opendash-server/tests/builder/interactive_blocks_test.rs` & Edit [`tests/builder.rs`](../../cms/crates/opendash-server/tests/builder.rs), [`studio_unified_ast.rs`](../../cms/crates/opendash-server/tests/builder/studio_unified_ast.rs)
4. **Starter Marketing Blocks, Patterns, Theme L10n & Repeater Studio Insert (`opendash-starter` & `cms/admin/`):**
   - Create `cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/{icon-box,testimonial,logo-strip,feature-list}/{block.json,render.html}`
   - Replace [`stats-bar/block.json`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/stats-bar/block.json) & [`render.html`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/stats-bar/render.html)
   - Edit [`patterns/hero-launchpad.json`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/patterns/hero-launchpad.json), [`patterns/features-bento-grid.json`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/patterns/features-bento-grid.json), [`theme.json`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/theme.json), [`assets/style.css`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/assets/style.css), [`opendash-starter.pot`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/languages/opendash-starter.pot), [`opendash-starter-vi.po`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/languages/opendash-starter-vi.po)
   - Edit [`clipboard_and_insert.ts`](../../cms/admin/src/builder/document/clipboard_and_insert.ts) & [`tests/theme/starter_theme.rs`](../../cms/crates/opendash-server/tests/theme/starter_theme.rs)
5. **Studio Preview & Browser E2E (`cms/admin/`, `cms/scripts/`)**:
   - Edit [`BuilderPreviewFrame.tsx`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx), [`test-studio-ux.mjs`](../../cms/admin/scripts/test-studio-ux.mjs)
   - Create `cms/scripts/studio_interactive_blocks_e2e.py`

---

### Slice S01 — Hạ tầng Micro-JS Có điều kiện, Route Asset, `RenderBudget` Context & `CoreL10n`

- **Dependencies:** Nền `main` hiện tại (Spec A1 đã merge; `0` migration).
- **File Ownership:**
  - `cms/admin/src/public-blocks/opendash-blocks.js`, `cms/admin/scripts/{build-public-blocks-js,test-public-blocks-js}.mjs`, `cms/admin/package.json`
  - `cms/crates/opendash-server/src/builtin_themes/opendash-blocks.js` & `builtin_themes/mod.rs` (phần `PUBLIC_BLOCKS_JS` + `public_blocks_js_version_hash()`)
  - `cms/crates/opendash-server/src/routes/builder.rs`, `template_sec/{nonces.rs,policy/preview.rs,cache.rs,composition/html_scan_state.rs}`
  - `cms/crates/opendash-server/src/page/{cache_types/types.rs,blocks_runtime.rs}`
  - `cms/crates/opendash-server/src/content_render/{document_compose.rs,l10n.rs}`, `cms/l10n/{opendash.pot,vi.po}`
  - `cms/crates/opendash-server/src/storage/public_content/{mod.rs,entry_render_meta.rs,entry_lookup.rs,hydration_and_related.rs}`
  - `cms/crates/opendash-server/src/routes/public_content/{types.rs,taxonomy_render.rs,search_render.rs,assets_and_sitemap/{enqueued_for_route.rs,render_entry_response_with_cache.rs}}`
  - `cms/crates/opendash-server/src/storage/overrides/{preview_context.rs,resolve_2axis/{render_public_builder.rs,resolve_template_2axis_on_connection.rs},candidates/{preview_studio_with_cache_and_locale.rs,render_preview_part.rs,wrap_entry_body_in_preview_shell_in_tx_with_budget.rs}}`, `storage/builder/{read_and_preview/entry_builder_snapshot.rs,context_and_lists/render_entry_builder_preview.rs}`
- **Acceptance Criteria:**
  1. `npm --prefix cms/admin run build:public-blocks-js` tạo file `< 12 KiB`, chặn sink nguy hiểm & `http(s)://`, và `npm --prefix cms/admin run test:public-blocks` pass 100% unit test (`test-public-blocks-js.mjs`, giữ `opendash-blocks.js <= 600` LOC) cho Tabs (nested tabs, RTL arrows, `Home`/`End`, `beforematch`, `#hash`), Slider (`scrollend`/debounce, `.od-slider-parking` + `--current`/`--children-count`, focus redirect, `prefers-reduced-motion`), Lightbox (`closedby` coordinate fallback, regex chặt `/media/<64hex>/...`, không `/api/admin/media/`, `ArrowLeft`/`ArrowRight`), Share relative token (`__OD_SHARE_URL__`), Copy Link (`legacyCopy`, `\u00A0` VoiceOver, không disable nút), và Popup (`#popup-id` & `[data-od-popup-trigger]` exact string match không nội suy CSS selector, chặn/lọc link cùng tài liệu + modifier/`_blank`/`download`, `showModal()`, `aria-controls`/`aria-expanded`, pause `<video>`/`<audio>` khi `close`, trả focus khi `lastTrigger.isConnected`, `[data-closing]` khi thiếu `allow-discrete` hoặc `overlay` dùng `getAnimations({ subtree: true })` + `closeSeq`, fallback `pointerdown`/`pointerup` ngoài `.od-popup-surface`, internal `#section` đóng drawer, `open_on_hash`).
  2. `GET /assets/opendash-blocks.js` trả HTTP `200`, `text/javascript; charset=utf-8`, `nosniff`, `max-age=3600`.
  3. Toàn bộ `RenderBudget`, `PreviewCompositionBudget` (hết hardcode `"vi"`), `RenderedLayout` (`6` call sites), `ResolvedPublicHtml` (`5` call sites gồm `types.rs#L197`), `entry_render_meta.rs` (`entry_lookup.rs <= 600` LOC, gọi trước `let body`), `preview_context.rs` (`<= 600` LOC), `entry_builder_snapshot.rs` và `finalize_document_blocks_js` (quét bằng `HtmlBodyScanner` + `<textarea>`) biên dịch sạch trên mọi public/preview routes (kèm `finalize_document_images` trên `search_render.rs`).
  4. `CoreL10n` đủ `26` khóa mới (tái sử dụng `msgid "Email"` đã có và bổ sung `25` `msgid` mới vào `cms/l10n/{opendash.pot,vi.po}`, `0` duplicate `msgid`); kiểm chứng Tầng 0 `run-tests.py --check`, `build:public-blocks-js`, `test:public-blocks`, `check-i18n.mjs --strict` pass.
- **Rollback:** `git checkout --` toàn bộ file S01 và xóa file tạo mới; DB không chạm.

### Slice S02 — `7` Core Blocks (`core/tabs`, `tab-panel`, `slider`, `slide`, `gallery`, `social-share`, `popup`) & Public Tailwind CSS

- **Dependencies:** Hoàn tất `S01`.
- **File Ownership:**
  - `cms/crates/opendash-server/src/builtin_blocks/{tabs,tab-panel,slider,slide,gallery,social-share,popup}/block.json`
  - `cms/crates/opendash-core/src/builder/validation/checks.rs` (`in_popup` transitive subtree validation)
  - `cms/crates/opendash-server/src/storage/page/types.rs` (`EMBEDDED_CORE_BLOCK_SOURCES`)
  - `cms/crates/opendash-server/src/page/{mod.rs,blocks_interactive.rs,blocks_media_share.rs,blocks_popup.rs,node_renderer.rs,blocks_layout.rs,blocks_leaf.rs,blocks_runtime.rs,media_render.rs}`
  - `cms/admin/src/tailwind-public.css`, `cms/admin/src/tailwind-public-safelist.txt`, `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css`
  - `cms/crates/opendash-server/tests/builder.rs`, `tests/builder/interactive_blocks_test.rs`, `tests/builder/studio_unified_ast.rs`
- **Acceptance Criteria:**
  1. `EMBEDDED_CORE_BLOCK_SOURCES` đăng ký đủ `30` Core Blocks (tăng `+7`, hoặc `48` sau khi merge A2), mọi `preset.icon` qua `lookup_core_icon_paths` (`"eye"`, `"minus"`, `"folder"`, `"arrow-right"`, `"sparkles"`, `"menu"`, `"arrow-left"`), `slider.default_children` (`6 <= 8`) và `popup.default_children` (`2 <= 8`, depth `1 <= 2`), mọi block đều từ chối khi nhúng trong `core/loop-item`.
  2. `core/tabs` & `core/tab-panel`: chống panic `active_count == 0`, dùng `od-tab-{node_scope}-{idx}` duy nhất trong mọi pattern, `[role="tabpanel"]` 0 box style (`data-od-spacing="padding"` đặt trên `.od-tab-panel-inner`), đủ `data-od-tabs-root` & tách `data-od-spacing="margin"` ở wrapper ngoài khi cần; `active_count < 2` giữ `needs_blocks_js = false`.
  3. `core/slider` & `core/slide`: phát `.od-slider-cols-*`, `--od-spv`, `--od-slide-min-h`, `data-od-slider-root`, `data-od-slider-spv="{1|multi}"`, `data-od-slide-index`; `in_deferred_media` lan truyền đệ quy giúp ảnh trong `core/image` & `core/gallery` từ slide `>= 1` hoặc bên trong `core/popup` gắn `loading="lazy" fetchpriority="low"` trên cả `<picture>` lẫn fallback `<img>`; cập nhật chữ ký `render_native_container` & `render_native_leaf`; `spv = "multi"` hoặc `active_count < 3` giữ chế độ hữu hạn.
  4. `core/gallery`: single root `.od-gallery`, `<figure><button>...</button>{figcaption}</figure>`, `columns` `1..=6`, masonry `mb-*`, early return khi rỗng trước khi bật `needs_blocks_js`, `<dialog closedby="any">` luôn mang `src` ảnh đầu (không có `<form method="dialog">`).
  5. `core/social-share`: xử lý đủ `is_preview` (`https://example.com/`), `404` (`/`), `custom_url` strict (`http(s)`/`/`), token `__OD_SHARE_URL__` với `data-od-share-path`, Copy Link live region ngoài nút, và giữ `needs_blocks_js = false` khi chỉ có link tuyệt đối & tắt copy.
  6. `core/popup`: validation `popup_id` (`^[A-Za-z][A-Za-z0-9_-]{0,47}$`), hỗ trợ đủ `center`/`left`/`right`/`top`/`bottom`, `--od-popup-w` responsive, `full_height`, `backdrop_style`, `--od-popup-bg`, `radius`, `padding` trên `.od-popup-body`, ép hiện nút đóng khi `close_on_backdrop == false`, không chèn `<form method="dialog">` (giữ CSP `form-action 'none'`), cho phép đặt trong `part:header`/`part:footer`, chặn lồng `core/popup` bắc cầu ở mọi tầng (`checks.rs` + `core/pattern-ref` tại `blocks_runtime.rs`), và render `.od-popup-preview-chip` khi `is_preview == true`.
  7. `opendash-tailwind.css < 120 KiB` chứa đủ rule popup top-layer (`display: contents` public, UA `<dialog>` resets, `overlay`, `transition-behavior: allow-discrete` tách riêng, `display: flex` trên `[open]`, `@starting-style`, `[data-closing]`, `prefers-reduced-motion`, `.od-popup-surface` + sticky `.od-popup-close`, canvas preview click-through & `z-index: 60`) & safelist; `node_renderer.rs <= 600` LOC; `studio_unified_ast.rs` cập nhật `(25, 5, 30)`; Tầng 0 `run-tests.py --check` pass.
- **Rollback:** Gỡ `7` directory `builtin_blocks/*`, revert `page/` modules, tests và `opendash-tailwind.css`.

### Slice S03 — Bộ `5` Starter Marketing Blocks (`opendash-starter`), Patterns & Theme L10n

- **Dependencies:** Hoàn tất `S02`.
- **File Ownership:**
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/{icon-box,testimonial,logo-strip,feature-list}/{block.json,render.html}`
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/stats-bar/{block.json,render.html}`
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/patterns/{hero-launchpad,features-bento-grid}.json`
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/{theme.json,assets/style.css,languages/{opendash-starter.pot,opendash-starter-vi.po}}`
  - `cms/crates/opendash-server/src/builtin_themes/mod.rs` (nhúng `8` file block mới)
  - `cms/admin/src/builder/document/clipboard_and_insert.ts` (clone `field.default` của `repeater` khi chèn node mới)
  - `cms/crates/opendash-server/tests/theme/starter_theme.rs`
  - Rebuild `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css`
- **Acceptance Criteria:**
  1. Toàn bộ `5` block marketing đều vượt qua `validate_block_template_csec`, dùng `{% if icon and icon.name %}` bảo vệ optional icons, dùng `css_var: "--od-cols"` + `.od-responsive-grid` cho responsive `columns`, dùng `.od-star-filled { fill: currentColor; }` tương thích `<symbol><use/>`, và có `default` đầy đủ cho `required: true` + repeaters `min_items: 1` (tương thích chèn mới từ Studio qua `clipboard_and_insert.ts`).
  2. `hero-launchpad.json`, `features-bento-grid.json`, `theme.json` sample `home_page`, `assets/style.css` và `starter_theme.rs` cập nhật đồng bộ sang `stats-bar` V1 + `icon-box` (assert đủ cả `6` block `icon-box`, `testimonial`, `stats-bar`, `logo-strip`, `feature-list`, `quote-box`), giữ nguyên `"version": "1.0.0"` và `opendash-starter/quote-box`.
  3. Trang chỉ chứa 5 block này có `needs_blocks_js == false` (`0` JS); `cargo run -- --check-theme ...` + `run-tests.py --check` + `build:public-tailwind` (`< 120 KiB`) pass.
- **Rollback:** Revert thư mục `opendash-starter/`, `builtin_themes/mod.rs`, `clipboard_and_insert.ts`, `starter_theme.rs` và `opendash-tailwind.css`.

### Slice S04 — Studio Preview `BRIDGE_SCRIPT`, Regression Tests & Playwright Browser E2E

- **Dependencies:** Hoàn tất `S01`–`S03`.
- **File Ownership:**
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx` (`BRIDGE_SCRIPT` + `BRIDGE_SCRIPT_SHA256`)
  - `cms/admin/scripts/test-studio-ux.mjs` (kèm test cho `BRIDGE_SCRIPT` & `initialBlockProps` repeater `default`)
  - `cms/scripts/studio_interactive_blocks_e2e.py`
- **Acceptance Criteria:**
  1. `BRIDGE_SCRIPT` gỡ `data-od-popup-preview-open` + `open` (null-safe khi `children` chưa có `<dialog>`) **trước** `if (!id) return;` (bảo đảm deselection cũng đóng preview popup), mở `[data-od-popup-host]` chứa selection bằng attribute `open` (không gọi `showModal()` để giữ focus ở Inspector), mở toàn bộ `[role="tabpanel"]` (gỡ `hidden` + đặt `tabindex="0"` trên panel chọn, `hidden="until-found"` trên anh em cùng `tabsRoot`) và cuộn toàn bộ `[data-od-slide-index]` cha bằng `getBoundingClientRect()` delta + `behavior: 'auto'`; `BRIDGE_SCRIPT_SHA256` khớp chính xác SHA-256 base64; `npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build && node cms/admin/scripts/test-studio-ux.mjs` pass 100%.
  2. Playwright E2E `studio_interactive_blocks_e2e.py` chạy live trên Chrome: xác nhận trang chỉ có 5 Starter block có `0` `<script src="/assets/opendash-blocks.js">` và `0` lỗi CSP (`form-action 'none'` được giữ nguyên); trang tương tác pass đầy đủ Tabs (click, `Arrow*`, `Home`/`End`, `beforematch`/`#hash`, Studio Preview tree reveal lồng nhau), Slider (snap, arrows/dots, `fetchpriority="low"` cho slide `>= 2`, pause toggle, `prefers-reduced-motion`, Studio scroll slide), Gallery (grid & masonry, mở `<dialog closedby="any">`, mũi tên, `Esc` + click backdrop đóng dialog, trả focus, CSP vẫn `form-action 'none'`), Social Share (0 tracker, `__OD_SHARE_URL__`, Copy Link clipboard + `[role="status"]` ngoài nút), và **Popup** (nút `<a href="#id">` mở Center Modal xem ảnh/video + pause video khi đóng, drawer `left`/`right` trong `part:header` dùng `core/stack` + `core/button`/`core/text` link `#section` tự đóng drawer và `display: contents` không tạo ghost gap trên header, khóa `close_on_backdrop: false` chỉ đóng bằng nút/`Esc`, không chặn link `#id` khác trang/ngoài site, `open_on_hash`, popup có `core/contact-form` vẫn gửi hợp lệ, không có lỗi console/CSP, và Studio Preview mở/đóng popup theo selection + bỏ chọn mà không cướp focus Inspector hay chặn click ngoài bề mặt popup).
- **Rollback:** Revert `cms/admin/src/builder/BuilderPreviewFrame.tsx`, `cms/admin/scripts/test-studio-ux.mjs` và xóa `cms/scripts/studio_interactive_blocks_e2e.py`.

---

### Verification Plan

- **Tầng 0 (giữa các Slice trung gian S01–S03):**
  - `RUSTC_WRAPPER="" TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --check`
  - Tại S01: thêm `npm --prefix cms/admin run build:public-blocks-js && npm --prefix cms/admin run test:public-blocks`
  - Tại S02–S03: thêm `npm --prefix cms/admin run build:public-tailwind`
- **Tầng 1 (1 lần duy nhất sau khi hoàn tất S04, trước Bước 6):**
  ```bash
  python3 cms/scripts/check-standards.py && python3 cms/scripts/check-standards.py --self-test
  python3 cms/scripts/check-plans.py && python3 cms/scripts/check-plans.py --self-test
  python3 .agents/scripts/check-agent-artifacts.py
  cargo fmt --manifest-path cms/Cargo.toml --all --check
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build:public-blocks-js && npm --prefix cms/admin run build:public-tailwind
  npm --prefix cms/admin run build
  npm --prefix cms/admin run test:public-blocks && node cms/admin/scripts/test-studio-ux.mjs
  node cms/scripts/check-i18n.mjs --strict
  RUSTC_WRAPPER="" TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --changed
  python3 cms/scripts/studio_interactive_blocks_e2e.py
  git diff --check
  ```
- **Sync & Drift trước Merge (Bước 5 & Bước 7):** Thực thi 1 lượt `/egm-drift-report`, sync `origin/main` 1 lần ở cuối Bước 5 / Bước 7 để khớp hoàn toàn Spec A2 nếu A2 đã merge (`48` Core Blocks + `opendash-starter` V1 + `render_index_home_response_for_locale`), kiểm tra lại toàn bộ Tầng 1, cập nhật [`docs/designs/SITEMAP.md`](SITEMAP.md) rồi merge 1 Feature PR duy nhất `feat/studio-interactive-and-starter-blocks`.
