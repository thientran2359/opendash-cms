# M4-P03 · Đề xuất G2 cho Tree, Canvas và Inspector

**Trạng thái:** `PROPOSED` · **Baseline:** `main@62d85ece47813d90079c78b3b84aae1a339a0d98` · **Tác giả:** Atlas.

[Issue #71](https://github.com/thientran2359/opendash-cms/issues/71) · [ADR-0007](../../docs/decisions/ADR-0007-m4-builder-taxonomy-foundation.md) · [G5 Builder homepage](../../docs/verification/2026-11-11-m4-s02-t01-builder-homepage.md) · [G5 Taxonomy macOS](../../docs/verification/2026-11-11-m4-s03-s04-taxonomy-g5-macos.md).

P03 đưa mô hình cây và API đã có vào màn chỉnh sửa. Tiêu chí là tạo, chọn, sửa, xóa và đổi thứ tự node từ tree hoặc canvas, dùng chuột hoặc bàn phím, giữ bản đang chỉnh qua chuyển màn, 401 và 412. P04 sở hữu responsive props, P05/P06 sở hữu binding/list, P07/P08 sở hữu nghiệm thu tích hợp. P03 tiếp tục dùng hợp đồng homepage, theme activation và ACL hiện hành.

## Hiện trạng và phương án canvas

Admin vẫn giữ `BlockData` một block. Preview hiện dùng `srcDoc` với `sandbox=""`. Backend Builder homepage đã có GET document/version, POST preview và PUT có `If-Match`. Preview trả HTML và chẩn đoán theo node. Renderer có `data-node-id`, nhưng DOM của iframe sandbox hiện tại bị cô lập khỏi Admin. [App.tsx](../../cms/admin/src/App.tsx) · [BuilderWorkspace.tsx](../../cms/admin/src/builder/BuilderWorkspace.tsx) · [Builder routes](../../cms/crates/opendash-server/src/routes/builder.rs) · [Renderer](../../cms/crates/opendash-server/src/page.rs) · [MDN về sandbox](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe#sandbox).

| Cách nhận click | Đánh đổi |
| --- | --- |
| Canvas chỉnh sửa do Admin dựng từ document có kiểu, dùng nút chọn trên node | Parent sở hữu vùng tương tác và ID. Có thể làm selection và keyboard với HTML preview vẫn cô lập. Cần nêu rõ canvas thể hiện cấu trúc, Preview server thể hiện kết quả xuất bản. |
| Đọc DOM của iframe với `allow-same-origin` | Tương tác trực tiếp trên HTML server, nhưng mở ranh giới origin cho HTML theme và cần thẩm định thêm các nguồn load, điều hướng và DOM. |
| Cầu `postMessage` chạy bên trong iframe | Nhận vị trí theo layout HTML thực. Cần cho phép script và tách được bridge do CMS kiểm soát khỏi HTML template. |

**Đề xuất:** Tree và canvas chỉnh sửa lấy cùng document từ API. Canvas dựng Section/Row/Column và core element bằng React từ props có kiểu. Nút chọn và dấu viền thuộc Admin. Node legacy hoặc thiếu renderer dùng thẻ có label, giữ ID và chẩn đoán. Chế độ **Xem trước** hiển thị HTML server trong iframe `sandbox=""`. Canvas chỉnh sửa thể hiện vị trí node và nội dung cơ bản. Người dùng đối chiếu giao diện theme thực qua Preview server. Nếu cần hiện HTML legacy trực tiếp trong canvas, thử iframe cô lập cho từng block và overlay do parent quản lý. Việc bổ sung fragment HTML theo node là một thay đổi hợp đồng riêng, cần được chốt trước khi triển khai.

## Trạng thái và thao tác

- Workspace giữ `savedDocument`, `savedVersion`, `draftDocument`, `selectedNodeId` và preview revision trong bộ nhớ. Tree, canvas và inspector dùng cùng `selectedNodeId`. Preview response chỉ áp dụng cho document revision và account hiện tại. Response cũ không đổi selection hoặc chẩn đoán của draft mới.
- Thêm node tại vị trí hợp lệ và chọn node mới. Reorder trong cùng parent, hoặc chuyển tới parent tương thích. Thao tác giữ ID và thứ tự các node khác. Xóa subtree có xác nhận khi còn node con. Sau xóa, chọn sibling gần nhất, nếu hết sibling thì chọn parent. Validation server là nguồn quyết định cuối.
- Tree hỗ trợ phím mũi tên, Home/End và Enter/Space theo hành vi cây một lựa chọn. Focus và selection hiển thị riêng. Nút **Lên**, **Xuống** và **Chuyển tới** cung cấp cùng thao tác với drag/drop. Canvas dùng vùng chọn có label đọc được, inspector cập nhật đúng node. Node không resolve vẫn hiện trong tree và giữ payload. [WAI-ARIA Tree View Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/treeview/).
- Preview gửi snapshot draft hiện tại. Save dùng `If-Match` của `savedVersion`. Chỉ xóa dirty state cho snapshot vừa lưu, giữ thay đổi tiếp theo nếu người dùng đã sửa trong lúc request chạy. 412 giữ draft và báo có phiên bản mới, cho người dùng chọn tải bản đã lưu sau khi xác nhận bỏ bản đang chỉnh hoặc giữ draft để đối chiếu. 401 khóa workspace và chỉ phục hồi khi cùng tài khoản đăng nhập lại và vẫn có quyền. Đổi tài khoản, mất quyền hoặc logout xóa draft/preview của phiên trước. Chuyển hash trong Admin giữ draft cùng phiên và `beforeunload` cảnh báo khi dirty.

## Kiểm chứng và điều kiện G3

| Ca | Tiêu chí |
| --- | --- |
| Tree ↔ canvas ↔ inspector | Chọn cùng ID từ tree hoặc canvas, đồng bộ viền và form, reorder bằng drag/drop và nút/phím, focus giữ được sau thao tác. |
| Legacy và iframe | Homepage M3 mở đúng node. Preview hiển thị HTML server với sandbox nguyên. Template script không chạy, thao tác chọn ở canvas không điều hướng trang. Node không hỗ trợ vẫn hiện chẩn đoán trong Admin. |
| Phiên, version và dữ liệu | Preview không ghi DB. Save đúng version cập nhật `/`. 401 cùng account phục hồi, account khác không thấy draft. 412 giữ bản đang chỉnh. Hai preview đến sai thứ tự không đổi bản mới. |
| Browser thật | Test cùng document với chuột và bàn phím, thao tác thêm/xóa/reorder, mở layout M3 và layout cây, giữ draft khi chuyển view, verify công khai sau Save và restart. |

**Thử nghiệm đã chạy:** Python Playwright với Chrome 157.0.8103.50 trên Windows, dùng HTML mẫu trong headless browser. Parent không đọc được `contentDocument` của iframe sandbox. Script mẫu trong iframe bị chặn, nút overlay của parent chọn được node legacy bằng click và Enter. Đây là thử nghiệm cô lập cho cơ chế tương tác, chưa chạy OpenDash hoặc kiểm tra layout nhiều node. Source được đọc trực tiếp vì CodeGraph workspace chưa có node. Tài liệu này cần được phản biện và chốt trước G3.
