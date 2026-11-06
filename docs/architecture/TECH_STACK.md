# OpenDash — Đánh giá tech stack V1

**Chủ dự án đã duyệt hướng stack V1 và chốt dùng Wasmi cho runtime plugin.** Phạm vi theo [yêu cầu V1 đã xác nhận](../product/PRODUCT_REQUIREMENTS.md). Phần tích hợp `wasmi` và SDK plugin trên host Rust vẫn cần kiểm chứng theo tiêu chí trong tài liệu này.

> Các kết quả M1/M2 được ghi dưới đây là lịch sử kiểm chứng từ repository nguồn. Workspace `/opendash-cms` đã có scaffold Rust/React mẫu, nhưng chưa nhập những module CMS đã kiểm chứng trước đó. Xem [verification](../verification/README.md) để phân biệt kiểm chứng scaffold mới với hệ thống CMS đầy đủ.

Stack V1 gồm Axum, SQLx, React/TypeScript/Vite, MiniJinja và Wasmi. Chủ dự án quyết định chọn thẳng Wasmi thay vì chạy thử nghiệm so sánh ba bên với Wasmtime và QuickJS.

## Stack đề xuất

| Phần | Đề xuất | So sánh và lý do |
|---|---|---|
| HTTP server | **Axum + Tokio + Tower** | Axum dùng router và middleware tương thích Tower. Phù hợp với Admin, API, public render và route plugin đi qua một lớp phân quyền chung. Actix Web là phương án hợp lệ nếu nhóm quen hệ sinh thái của nó hơn. [Axum](https://docs.rs/axum/latest/axum/) · [Actix Web](https://docs.rs/actix-web/latest/actix_web/) |
| SQLite | **SQLx + SQLite bundled** | SQLx hợp với server async và hỗ trợ migration. Dùng transaction cho thao tác Lưu, kiểm chứng cấu hình pool/WAL trên tải thật. Rusqlite có API backup SQLite và là phương án tốt nếu nhóm chấp nhận tách thao tác đồng bộ sang worker. SQLite snapshot cần phối hợp với tệp media, cấu hình và gói mở rộng để gói backup khôi phục được cả website. [SQLx SQLite](https://docs.rs/sqlx/latest/sqlx/sqlite/index.html) · [Rusqlite backup](https://docs.rs/rusqlite/latest/rusqlite/backup/) · [SQLite backup](https://www.sqlite.org/backup.html) |
| Admin và builder | **React + TypeScript + Vite, thử dnd kit cho cây bố cục** | CMS làm cây Section → Row → Column → Element và sinh panel từ khai báo của block. Preview tải HTML từ renderer server trong iframe. Puck có thể tiết kiệm thời gian làm UI nhưng mẫu cấu hình gắn block với hàm render React, nên cần thử adapter cho block từ ZIP và preview server trước khi chọn Puck. Vite tạo asset tĩnh để nhúng vào binary. [dnd kit](https://dndkit.com/) · [Puck component config](https://puckeditor.com/docs/integrating-puck/component-configuration) · [Vite build](https://vite.dev/guide/build.html) |
| Theme và public render | **MiniJinja + khai báo block/setting có kiểm tra kiểu** | Theme ZIP cung cấp template tại runtime. Preview và trang công khai dùng cùng renderer, với dữ liệu đang chỉnh hoặc dữ liệu đã lưu tương ứng. Askama kiểm tra template lúc biên dịch Rust, phù hợp hơn với template có sẵn trong binary. HTML escaping là một lớp xử lý đầu ra; URL, CSS, rich text và asset cần quy tắc riêng. [MiniJinja](https://docs.rs/minijinja/latest/minijinja/) · [Askama](https://docs.rs/askama/latest/askama/) |
| Logic server của plugin | **WebAssembly qua Wasmi (đã chốt)** | Chủ dự án chọn Wasmi (interpreter WebAssembly thuần Rust, có cơ chế fuel và giới hạn bộ nhớ, giữ binary gọn và không phụ thuộc C hay trình biên dịch JIT). Bản thử Python trước đó dùng Wasmtime để chứng minh luồng nạp `.wasm` từ ZIP, giới hạn hàm host theo quyền khai báo và ngắt vòng lặp bằng fuel. Có thể cấp tài nguyên qua host API mà không bật WASI cho plugin. Bước tiếp theo tích hợp trực tiếp `wasmi` vào host Rust và kiểm chứng quyền, tài nguyên, deadline host call cùng đường phục hồi Admin. [Wasmi fuel](https://docs.rs/wasmi/latest/wasmi/struct.Config.html#method.consume_fuel) · [Wasmtime Security](https://docs.wasmtime.dev/security.html) · [rquickjs](https://docs.rs/rquickjs/latest/rquickjs/struct.Runtime.html) |
| Viết và đóng gói plugin | **Plugin SDK có phiên bản, cung cấp ví dụ AssemblyScript** | Đường thử đã build plugin từ AssemblyScript rồi đóng ZIP chạy được, developer không cần công cụ Rust cho ví dụ này. AssemblyScript có cú pháp giống TypeScript và quy tắc riêng, nên cần thử một SDK với chuỗi/JSON, input lỗi và thông báo debug trước khi quyết định công cụ authoring chính. [AssemblyScript concepts](https://www.assemblyscript.org/concepts.html) · [Compiler](https://www.assemblyscript.org/compiler.html) |
| Phân phối | **Nhúng asset Admin vào executable Rust, SQLite bundled** | Node dùng khi build Admin. Binary phục vụ asset đã nhúng và quản lý dữ liệu, media, theme/plugin của site. Kiểm tra bản Linux release trên máy sạch để xác nhận tập thư viện hệ điều hành cần có. [RustEmbed](https://docs.rs/rust-embed/latest/rust_embed/) · [SQLx SQLite](https://docs.rs/sqlx/latest/sqlx/sqlite/index.html) |

## Bằng chứng đã chạy

Bản thử ở `spikes/stack-v1` dùng dữ liệu tổng hợp. Contract thử theme gồm file khai báo block và template HTML. Contract thử plugin gồm manifest quyền, module Wasm và một hàm host đọc dữ liệu mẫu. Đây là hai thử nghiệm rời, không phải contract SDK đã chốt.

| Thử nghiệm | Kết quả quan sát |
|---|---|
| Theme ZIP trong trình duyệt | Tải ZIP từ builder, chọn block sample/banner, form hiện Tiêu đề và Mô tả theo khai báo. Preview hiện block từ MiniJinja. Trang công khai trống trước khi bấm Lưu và hiện đúng nội dung sau khi bấm Lưu. |
| Kiểm tra theme | Nội dung A < B được escape thành HTML. Chuỗi cú pháp MiniJinja do người dùng nhập hiển thị như text. ZIP có đường dẫn lạ bị từ chối. |
| Plugin ZIP | Quyền content.read đã khai báo cho phép plugin gọi đúng một hàm host và trả 42. Thiếu quyền thì import không được cung cấp. Module yêu cầu WASI cũng bị từ chối. |
| Plugin bị lỗi | Vòng lặp vô hạn hết fuel và dừng. Module đòi bộ nhớ vượt hạn mức bị từ chối. Sau khi tắt plugin lỗi, cùng host vẫn chạy được plugin hợp lệ. |
| Đường build cho developer | AssemblyScript 0.28.20 biên dịch guest.ts thành Wasm, nạp Wasm từ ZIP và nhận kết quả 42, với một lần gọi host hợp lệ. |

Chạy **7 kiểm tra tự động thành công** với MiniJinja 3.0.5 và Wasmtime 50.0.1 qua binding Python trên Windows. Luồng builder chạy trong trình duyệt qua localhost. Logic plugin thử nghiệm được gọi qua bộ kiểm tra, chưa chạy qua router CMS. Hàm host thử đọc dữ liệu mẫu, chưa thực hiện ghi DB hoặc gọi mạng.

Chủ dự án chọn phát triển trước trên Windows. Lát cắt Rust đã build bằng MSVC và chạy luồng theme ZIP, preview và Lưu vào SQLite. Smoke test khởi động lại cùng database đã qua. Kiểm chứng React drag/drop, host Rust cho plugin và bản phát hành Linux còn ở các bước tiếp theo. [Hướng dẫn chạy scaffold trên Windows](../../README.md#chạy-scaffold-trên-windows)

## Phản biện từ Antigravity

- Antigravity cũng chọn Axum, SQLx, React/TypeScript/Vite với dnd kit và MiniJinja. Với preview HTML phía server và block nạp từ ZIP, editor tự dựng cho phép CMS giữ một mô hình bố cục và tái sử dụng form sinh từ khai báo cho block, Theme Options và plugin settings. Puck vẫn là phương án tham khảo nếu editor tự dựng tốn nhiều công hơn dự kiến.
- Antigravity đề nghị thử Wasmi và QuickJS trước khi chọn runtime. Mẫu AssemblyScript chỉ truyền số nguyên, còn SDK thật cần trao đổi chuỗi/JSON, báo lỗi dễ đọc và định nghĩa giới hạn dữ liệu. Ưu thế về dung lượng, RAM hoặc độ dễ viết plugin của từng phương án là giả thuyết cần đo.
- Fuel giới hạn thực thi guest. Khi guest đang chờ host call, cần kiểm tra việc hủy thao tác thực tế và đường phục hồi của CMS. Timeout async phụ thuộc future nhường quyền chạy, nên phần SQLite hoặc tác vụ blocking cần kiểm chứng riêng. [Tokio timeout](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html)

Trao đổi được thực hiện ở chế độ review chỉ đọc. [Phiên phản biện](https://antigravity.google.com/u/1/r/889cd9a6-5545-453d-a968-6662e5167b18-v2?p=c%2Febaa6058-ae74-4485-9e61-c98cfe449852)

## Những việc cần chứng minh trước khi triển khai đầy đủ

1. **Host Rust trên Linux.** Dựng một executable thử có asset Admin nhúng, SQLite bundled, MiniJinja và runtime plugin. Chạy trên Linux sạch, kiểm tra phụ thuộc bằng ldd và đo dung lượng, thời gian khởi động, thời gian build và bộ nhớ với cấu hình release đã ghi rõ.
2. **Theme và editor.** Nạp block từ ZIP, sinh panel trong React, thao tác cây bằng bàn phím và kéo thả, đổi viewport và thử cập nhật theme có bố cục đã chỉnh. Kiểm tra preview riêng với HTML/CSS/theme asset.
3. **Plugin Wasmi với dữ liệu thật.** Tích hợp `wasmi` vào host Rust và xử lý payload JSON thật qua một route hoặc event. Kiểm tra đọc/ghi có giới hạn quyền, JSON lỗi, host call HTTP/SQLite chạy quá hạn, đích mạng được phép/bị chặn và Admin vẫn truy cập được.
4. **Sao lưu và phục hồi.** Tạo backup từ site đang hoạt động, phục hồi trên một bản cài mới và kiểm tra DB, media, theme/plugin và cấu hình khớp nhau.

Phiên bản crate `wasmi`, API plugin và mô hình dữ liệu sẽ chốt sau khi có lát cắt Rust tương ứng và review.
