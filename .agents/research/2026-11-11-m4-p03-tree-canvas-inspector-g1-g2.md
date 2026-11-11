# M4-P03 / Issue #71 — G1 và đề xuất G2 Tree / Canvas / Inspector

**Tình trạng:** G1 source-audited; G2 **PROPOSED**, chưa có Final Decision/ADR/PR merge; G3 **NOT READY**.
**Người soạn:** Forge (CoS Windows); **source baseline:** `origin/main@62d85ece47813d90079c78b3b84aae1a339a0d98` ngày 2026-11-11.
**Nguồn nhiệm vụ:** [Issue #71](https://github.com/thientran2359/opendash-cms/issues/71) (M4-P03, BLD-03/FND-05), [M4 Task Plan](../../docs/plans/milestones/M4/TASK_PLAN.md), [ADR-0007](../../docs/decisions/ADR-0007-m4-builder-taxonomy-foundation.md).

## 1. Ranh giới và nguồn kiểm chứng G1

- **Sản phẩm:** V1 dùng một document homepage dạng cây `Section → Row → Column → Element`; Open/Preview chỉ đọc, Save cập nhật public và giữ layout khi đổi theme. Không triển khai responsive P04, field binding P05, list block P06, workflow publish mới hoặc theme composition M5.
- **AST đã có:** `BuilderDocument {schema_version,sections}`, node `id` ổn định, typed `core/heading|text|image|button`, `legacy/block`; giới hạn 512 node/256 KiB. Validation kiểm đúng quan hệ và ID duy nhất ([builder.rs](../../cms/crates/opendash-core/src/builder.rs)). ID mới phải qua cùng bộ `valid_package_id`. Thứ tự là thứ tự phần tử mảng, ID không phụ thuộc vị trí.
- **Đặc biệt quan trọng:** `validate_for_save` của core chỉ cho phép payload không resolve/không còn descriptor tồn tại **đúng vị trí, cùng ancestor IDs và nội dung so với saved document** (`is_preserved`), nên UI không được reorder/move loại node đó hay dịch chuyển một node khác qua vị trí neo của nó. Cần hiển thị rõ “không thể di chuyển cho đến khi provider được khôi phục”; xóa nguyên node là hành động có chủ đích và cảnh báo mất dữ liệu.
- **Backend P02 đã có:** `GET /api/admin/builder/homepage → {document,source,version}`, `POST /api/admin/builder/homepage/preview → {html,diagnostics}`, `PUT /api/admin/builder/homepage` nhận `If-Match: "vN"` và trả document/html/version/diagnostics, dựa session + CSRF/Origin, `401/403/412/428` theo contract ([routes/builder.rs](../../cms/crates/opendash-server/src/routes/builder.rs)). Preview không ghi DB/public; Save mới commit. `source` có `empty|legacy|layout`. Các thao tác add/delete/reorder là phép biến đổi document ở client, **không tạo CRUD endpoint theo node**.
- **Renderer P02 đã có:** cấu trúc Section/Row/Column và element được render thành wrapper mang `data-node-id` (đã escape); khi legacy/provider không hỗ trợ, renderer **không sinh wrapper element** mà gửi `diagnostics: [{node_id,code}]` ([page.rs](../../cms/crates/opendash-server/src/page.rs)). Canvas không thể dùng làm nguồn danh sách node; tree phải phản ánh document kể cả node không render.
- **Khoảng trống UI thực tế:** `App.tsx` và `BuilderWorkspace.tsx` vẫn dùng biểu mẫu block M1 `/page`, `/preview`, `/save` và iframe `sandbox=""`/ `srcDoc`. `api/builder.ts` chưa có client cho homepage API. M3 flow đang gắn `blockData` thay vì `BuilderDocument`. Khi homepage đã chuyển sang layout, legacy API `/page` trả 409. P03 phải kết nối UI Builder mới với P02; phần UI cũ chỉ làm fallback tương thích khi thực sự cần, không ghi đè document layout đã Save.
- **Trạng thái tab/phiên:** AdminShell giữ màn Builder đã mount khi đổi hash view; App hiện giữ block draft trong React state qua 401 cho tài khoản đăng nhập lại cùng ID, xóa trên explicit logout hoặc đổi tài khoản, và có beforeunload guard ([App.tsx](../../cms/admin/src/App.tsx), [AdminShell.tsx](../../cms/admin/src/shell/AdminShell.tsx)). P03 tiếp nhận chính sách đó cho document và inspector mà không làm mất dữ liệu nhập.
- **Authorization:** Admin/Editor dùng API Builder theo P02; upload/activation theme giữ gate Admin riêng; `requestJson`/ `ApiClientError` và `If-Match` đã được dùng chung tại [api/client.ts](../../cms/admin/src/api/client.ts).
- **G1 browser probe Windows:** với Playwright + Chrome headless, một `srcdoc` iframe có nút `data-node-id="probe-node"` và một script. `sandbox=""`: parent `contentDocument=null`, không bắt được node; script không chạy. `sandbox="allow-same-origin"` **không có** `allow-scripts`: parent đọc được DOM, listener do **parent** gắn nhận `probe-node`, script trong frame vẫn không chạy. Kết quả 2/2 như dự kiến; phép thử tối thiểu này **chưa** chứng minh an toàn trên theme ZIP thực, CSP/media, Firefox/Safari hay E2E Builder. Hành vi sandbox được mô tả trong [MDN iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe) và [HTML Standard](https://html.spec.whatwg.org/multipage/iframe-embed-object.html).

## 2. Quyết định G2 đề xuất

| Chủ đề | Phương án đề xuất / contract | Các hướng đã đối chiếu |
| --- | --- | --- |
| Nguồn dữ liệu | **Một `BuilderDocument` draft** và revision tăng trên mọi mutation; một `selectedNodeId` duy nhất dùng chung tree, canvas, inspector. Selection/focus/collapse không nằm trong JSON để Save. | Mỗi panel giữ draft/selection riêng dễ drift và ghi đè dữ liệu khi đổi view. |
| Chọn node từ preview | Giữ `srcDoc` iframe với `sandbox="allow-same-origin"`, **tuyệt đối không thêm** `allow-scripts`. Sau mỗi `load`, code ở **parent** gắn listener `click`/keyboard vào `contentDocument`, tìm ancestor gần nhất `[data-node-id]`, xác thực ID thuộc draft hiện hành, phát `select(id)`. | `sandbox=""` không cho parent truy cập DOM; iframe script + `postMessage` cần cho phép script và gia tăng bề mặt tấn công. Transparent overlay không có hit testing chính xác khi theme tùy ý. |
| An toàn preview | Chỉ render HTML từ server vào iframe; không thực thi mã theme, không chạy HTML trong React parent và không nới `script-src 'none'`/CSP. Không dùng `allow-scripts`, `allow-forms`, `allow-popups`, `allow-top-navigation`. Listener capture ngăn click link/form trong chế độ chọn node. Kiểm lại CSP và media authenticated preview trên browser thực; nếu không chứng minh được security gate, **Stop-and-Propose**, không tự mở đặc quyền khác. | Chỉ dùng `postMessage` sau một quyết định bảo mật riêng về isolated origin/trusted bridge, ngoài V1 P03. |
| Overlay chọn | Vẽ khung chọn trong lớp UI do parent sở hữu, dùng `getBoundingClientRect()` của wrapper trong iframe + vị trí/scroll iframe; `pointer-events:none`; cập nhật khi load/resize/scroll/selection. Node thiếu wrapper có chỉ báo trong tree và diagnostics inspector. | Chèn UI/JS vào fragment theme hoặc chỉnh HTML public dễ làm preview khác Save. |
| Tạo node | Add Section tại root; Row chỉ trong Section, Column chỉ trong Row, Element chỉ trong Column. Menu core tạo payload typed hợp lệ; `legacy/block` chỉ cho phép khi descriptor active. Sinh ID không trùng, đúng validator core, không tái sử dụng ID sau delete trong phiên. | Thêm node sai tầng hoặc tự phát minh kiểu element sẽ bị server reject. |
| Delete / reorder | Lệnh transform immutable theo ID, kiểm quan hệ cha–con, bảo toàn ID, thứ tự mảng và toàn document. Reorder bằng drag handle trong tree và nút/keyboard Move up/down; kéo giữa cha hợp lệ cùng loại được phép sau validation. Xóa container cảnh báo xóa toàn bộ descendants, node legacy/unresolved yêu cầu xác nhận riêng. | Không dùng DOM order làm nguồn document, không gửi nhiều request save từng node. |
| Node không resolve | Tree và inspector vẫn hiển thị payload chẩn đoán, **read-only**; không tạo/sửa/reorder/move gây lệch chỉ số/ancestor của node neo (kể cả thao tác trên sibling làm dịch vị trí). Cho phép chủ động xóa với xác nhận trước; preview chẩn đoán node không render. | Mọi thao tác gây sai `is_preserved` phải từ chối trước ở UI hoặc trả server 400 và giữ draft. |
| Inspector | Form controlled được suy từ `selectedNodeId`; core props typed theo schema v1, legacy fields dùng registry M3. Giữ `draft` khi chuyển selection/view, không dùng `defaultValue` gây mất ký tự khi rerender; cập nhật nguyên document bằng immutable patch sau mỗi thay đổi. | Không đưa inspector state hoặc HTML rendered vào document save. |
| Preview | Explicit Preview hoặc debounce có hủy/sequence: chụp draft revision khi gọi, chỉ áp dụng HTML/diagnostics khi response còn đúng revision. Không tự gọi Save. Khi draft có sửa tiếp, hiển thị “preview chưa cập nhật” để tránh người dùng hiểu nhầm. | Response về trễ không ghi đè selection hoặc draft mới. |
| Save / conflict | `PUT` gửi snapshot draft + `If-Match` từ GET/Save thành công. Chỉ reset dirty khi local revision vẫn là snapshot vừa lưu; nếu người dùng tiếp tục nhập trong lúc Save, cập nhật server version nhưng giữ các edit mới. `412` giữ draft, hiện server version/diff và cho người dùng quyết định cách resolve; không auto-retry PUT. | Không silently overwrite từ tab khác; `428` là lỗi precondition cần được hiển thị. |
| 401 / khôi phục | Khi 401, giữ draft trong memory, ẩn editor sau màn đăng nhập, giữ `lastAuthenticatedUserId`; chỉ mở lại sau khi xác thực **cùng user ID**. Không tự lấy lại server data đè lên draft. Khác user hoặc logout chủ đích phải xóa dữ liệu nhạy cảm. | Đổi account không được đọc draft tài khoản cũ; 403 revoke quyền không được tự lưu. |

### Cấu trúc state và hành vi tương tác

```ts
type EditorState = {
  document: BuilderDocument;        // authoritative unsaved working document
  baseVersion: number;              // ETag version last read/saved
  source: 'empty' | 'legacy' | 'layout';
  editRevision: number;             // monotonically increasing local edits
  savedRevision: number;
  selectedNodeId: string | null;    // only local UI state
  focusedTreeNodeId: string | null; // focus may differ from selection
  expandedNodeIds: Set<string>;     // only local UI state
  previewRevision: number | null;
  diagnostics: Array<{node_id: string; code: string}>;
};
```

Luồng: `GET homepage → hydrate document, version, source → select node by id → immutable edit → preview (JSON draft) → PUT (If-Match) → update version/public`. Với `source=legacy` GET chỉ chiếu để xem, không tạo migration Save trước khi người dùng bấm **Lưu**. Khi `source=layout` mở từ endpoint Builder, không gọi legacy `/page`.

**Bàn phím và focus:** Tree áp dụng ARIA `role=tree/treeitem`, `aria-expanded`, `aria-selected`, roving `tabIndex`. Arrow Up/Down đổi **focus** trên node đang hiện, Arrow Left/Right collapse/expand hoặc focus parent/first child, Home/End đến node đầu/cuối; Enter/Space chọn node. `Alt+ArrowUp/Down` di chuyển node sang vị trí hợp lệ, tương đương nút “Lên/Xuống” và drag handle; thông báo `aria-live` mô tả kết quả. Escape hủy drag; Tab đưa focus đến toolbar/inspector. Không cướp Arrow/Space khi đang gõ trong `input`/`textarea`/`select`. Sau Add focus node mới; Delete focus sibling gần nhất hoặc parent; khi chọn từ canvas, tree mở ancestor và có nút “Đến node trong cây” để focus chủ động. Preview chỉ bắt click để chọn, không phụ thuộc bàn phím trong iframe để bảo đảm người dùng keyboard có thể thao tác mọi chức năng từ tree.

**Chặn hành vi ngoài ý muốn trong canvas:** Khi click element mang `data-node-id`, parent `preventDefault` trên event capture để link/button của theme không điều hướng hay submit. Nếu click khoảng trống không có ID thì giữ selection cũ; bên trong element chọn wrapper gần nhất, không cố map text offset. Khi preview tải lại, detach listener cũ và attach theo `load`; bỏ event từ document/iframe không khớp revision. Đối chiếu node ID với draft trước khi chọn, không tin dữ liệu `data-node-id` riêng lẻ.

**Draft recovery qua reload (đề xuất mở rộng có gate):** Cơ chế bắt buộc của Issue là giữ draft qua **đổi view và 401 cùng account**, đạt được bằng memory khi component còn mount. Có thể bổ sung `sessionStorage` theo tab để chống reload/crash: ghi bản nháp 256 KiB tối đa gồm `userId, baseVersion, document, editRevision` (không lưu token/CSRF/password/HTML preview), giới hạn tần suất, không upload. Khi mở lại phải xác thực account trước khi đề xuất “Khôi phục bản nháp”, so version và cảnh báo xung đột; xóa khi user khác, logout hoặc Save được chấp nhận và không còn edit tiếp. Vì `sessionStorage` chứa nội dung chưa công bố ở dạng rõ, **bản lưu qua reload chỉ triển khai sau review privacy/security**; nếu không được chấp thuận, memory + beforeunload guard là cam kết P03 và reload được báo rõ có thể mất dữ liệu. Quota/security error không được làm crash editor.

### Chỗ cần kiểm tra trước G3

1. **Security review bắt buộc** của `sandbox="allow-same-origin"` với HTML legacy/theme ZIP: script `onerror`, `javascript:`, iframe lồng, URL ngoài, CSS injection, event thuộc browser và `srcdoc` navigation. Duy trì CSP `script-src 'none'`, URL/image policy; không dùng cả `allow-scripts` và `allow-same-origin`.
2. **Version và conflict:** GET/PUT hiển thị source/version chính xác, không lưu khi 401/412/403/validation 400; 412 giữ cả bản nháp và server snapshot, cần thao tác resolve minh bạch. Có thể dùng API hiện có, không cần migration.
3. **Unresolved preservation:** Tạo fixture với provider thiếu hoặc payload lạ, kiểm pinned index/ancestors; update props của known node còn hợp lệ. Review việc “xóa có chủ đích” vì endpoint hiện nhận document thiếu node mà không từ chối.
4. **Accessibility/browser:** Mouse/pointer, drag cancel, keyboard-only tree, zoom 200%, screen reader live message và focus sau selection; Chrome Windows + Safari macOS/Firefox khi có runner. Kiểm click hit-test và overlay tại scroll/resize, ảnh lazy-loaded và theme không render node.
5. **Một writer trên `App.tsx`/`AdminShell.tsx` và CSS:** P03 UI Builder phải được tách phần `BuilderWorkspace`, model/state/api v1; phối hợp P04/P05/P07 tránh chồng file. Migration P02 đã merge. P03 không đổi Rust/SQLite nếu contract hiện tại đủ; mọi thay đổi renderer/CSP/route/API phải có Design Delta/owner được giao.
6. **Dữ liệu:** mở homepage M3 qua projection, preview không chuyển đổi; Save lần đầu chuyển `source=layout`; reload giữ IDs/order; đổi theme thiếu block vẫn giữ draft và dữ liệu đã lưu.

## 3. Acceptance / G3 handoff đề xuất

| Case | Hành động quan sát / kỳ vọng |
| --- | --- |
| P03-A01 | GET `empty` → Add Section/Row/Column/Heading → thay text trong Inspector → Preview hiển thị đúng; DB/public chưa đổi đến PUT. |
| P03-A02 | Click một heading trong canvas chọn đúng `selectedNodeId` và `aria-selected` ở tree; click node đó trong tree highlight đúng wrapper canvas. |
| P03-A03 | Dùng keyboard-only để chọn, sửa text, Move up/down, Add/Delete; tree order và preview order đúng, focus không mất, live announcement có nghĩa. |
| P03-A04 | Drag reorder và Move bằng bàn phím cho cùng JSON/order; ID giữ nguyên qua Save/reopen/restart. Move sai tầng/qua pinned unresolved bị ngăn. |
| P03-A05 | Delete container có descendants và delete legacy/unresolved phải xác nhận; cancel giữ document; confirm xóa đúng subtree, không xóa sibling. |
| P03-A06 | Đổi Builder → Posts → Builder khi đang gõ inspector: draft text, selected ID và dirty flag giữ nguyên. |
| P03-A07 | Làm API trả 401 trong Preview/Save; login lại **cùng account** giữ draft, refresh CSRF và version; login account **khác** xóa/không lộ draft. |
| P03-A08 | Tab khác Save trước → PUT stale 412; bản đang chỉnh giữ nguyên, không tự Save/retry; resolve phải do người dùng quyết định. |
| P03-A09 | Preview trả lỗi validation/403 hoặc response cũ về sau edit: inspector và draft không mất/rollback. Save valid tiếp theo cập nhật public. |
| P03-A10 | Có node theme unsupported → tree+diagnostics vẫn hiện; canvas không thể click node ẩn; switch theme tương thích hiển thị lại mà không rơi props. |
| P03-A11 | Probe sandbox: parent chọn được `data-node-id`, script/inline handler theme không chạy; link click không điều hướng, image preview chỉ đọc đúng session; CSP intact. |
| P03-A12 | Kiểm actual GET/POST/PUT trên SQLite test site; 401/403/412/428 và media ACL không đổi; chạy regression `builder_homepage_smoke.py` cùng UI browser E2E. |

**Đề xuất G3:** Một slice `M4-S03` / `BLD-03` gồm (a) typed UI model + homepage API adapter, (b) tree mutations & keyboard, (c) iframe selection + inspector, (d) 401/412 recovery + browser integration. Chỉ cấp Task ID `M4-S03-Txx`, ownership đường dẫn và trạng thái Ready sau Final Decision G2 đã review/merge, dependency/cổng Project #3 hợp lệ. Không mở G3 code từ Issue candidate #71.

**Chứng cứ thực chạy tại thời điểm G1:** Git SHA `62d85ec`; `git fetch --prune origin` PASS; đọc code/contract PASS; probe Playwright + Chrome Windows PASS cho 2 sandbox modes; **NOT_RUN** runtime Builder browser E2E, Safari, Firefox, Rust build/test (chỉ khảo sát/thiết kế). Các case A01–A12 ở trên là **đề xuất nghiệm thu**, chưa khẳng định đã PASS.

## 4. Đề nghị review / Final Decision

Đề nghị Atlas/Nova review G2 về: (a) thay đổi `sandbox` để parent đọc DOM trong khi scripts vẫn disabled, (b) semantics node unresolved/pinned, (c) 401/412 bảo toàn draft và security của sessionStorage tùy chọn, (d) keyboard/focus/ownership. Sau review, các quyết định đã đồng thuận mới chuyển sang `docs/` và phát hành G3. Issue #71 hiện là **design candidate**; không đổi thành `READY`/implementation hay tuyên bố `AGREED` chỉ từ tài liệu này.
