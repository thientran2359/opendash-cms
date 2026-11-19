# SDK AssemblyScript — plugin ABI v1 và Content Foundation C4

`as/opendash.ts` cung cấp `getInput()`, `setOutput()` và `hostCall()` để guest AssemblyScript giao tiếp với host Wasmi theo [Plugin ABI v1](../../docs/contracts/PLUGIN_ABI.md). Khai báo CPT/taxonomy nằm trong **manifest JSON**, không cần thêm host function hoặc thay đổi ABI để dùng C4. Xem [plugin mẫu](../examples/plugins/sample-plugin/README.md) để build Wasm hiện có; ví dụ manifest C4 và hook nằm trong cùng tài liệu, nhưng chưa được đóng gói vào ZIP mẫu.

## Bảng hỗ trợ C4

| Tính năng | Cách dùng được hỗ trợ | Giới hạn |
| --- | --- | --- |
| CPT của plugin/theme | `declarations.content_types[]` với `key`, `label`, `fields`, `settings` tùy chọn | `settings`: `public`, `has_archive`, `publication` là `immediate` hoặc `draft_publish`, `supports: [editor, featured_image]`; thiếu `settings` giữ mặc định legacy |
| Taxonomy tùy chỉnh | `declarations.taxonomy_definitions[]` gồm `key`, `label`, `hierarchical` (mặc định `false`), `public` (mặc định `true`) | Không khai báo chiếm quyền các khóa lõi `category`/`tag` hoặc taxonomy thuộc provider khác; ownership được kiểm tại storage |
| Gắn taxonomy vào CPT | `content_types[].taxonomies` hoặc `declarations.taxonomy_eligibility[]` | Tham chiếu key của taxonomy lõi, site hoặc cùng provider; key phải tồn tại và đủ quyền, không trùng lặp |
| Đường dẫn mặc định | `rewrite_base` tùy chọn trong CPT/taxonomy declaration | 1–3 đoạn kebab-case, mỗi đoạn ≤64 ký tự; tránh root dành riêng. C3 kiểm tra toàn bộ route/redirect collision (409); site override thắng package default; post/page/section không nhận base của provider |
| Chặn/cho phép Save | Hook `content/before_save` trong `hooks[]`, phản hồi `{"decision":"allow"}` hoặc `{"decision":"veto",...}` | Plugin chỉ được nhận CPT có `permissions.content_read`; không được sửa trực tiếp dữ liệu ứng viên |
| Theo dõi Save đã commit | Hook `content/after_save` | Chạy sau commit, không đảo ngược giao dịch; lỗi được ghi nhận, không biến thành quyền sửa nội dung |
| Tạo/sửa entry từ action | Host operation `content_save` qua `hostCall()`, với `actions[].capabilities: ["content_write"]` và `permissions.content_write` | Một lệnh content write mỗi action; update dùng `entry_id`/`expected_version` (CAS); `term_ids` chỉ nhận taxonomy đủ điều kiện; không gọi được trong save hook |
| Tự sửa `title`/`values` từ hook; filter URL tùy ý | **Không hỗ trợ** | Không có callback thay đổi payload, custom URL resolver filter, regex/PHP rewrite hoặc quyền ghi đè canonical permalink |

Các khai báo được áp dụng cùng transaction với kiểm tra ownership, provider generation và collision. Khi cập nhật hoặc tắt package, schema và dữ liệu vẫn được giữ theo chính sách provider; C3 giữ lựa chọn permalink của site. `public: false` và `draft_publish` vẫn được thực thi bởi core; hook không tự biến nháp thành nội dung công khai.

## Hook và action: dữ liệu thực tế

Với `hooks: ["content/before_save", "content/after_save"]`, Wasmi nhận input bằng `getInput()`: một envelope JSON có `event`, `correlation_id`, `occurred_at_ms`, `payload`. `payload` của before-save chứa ứng viên, ví dụ `type_key`, `title`, `slug`, `candidate_values`, `term_ids` (được lọc theo quyền field). Guest ghi quyết định bằng `setOutput()`:

```ts
// Trong opendash_run(), sau khi kiểm tra input.event == "content/before_save":
// Ví dụ đã import * as opendash từ SDK:
return opendash.setOutput('{"decision":"allow"}') < 0 ? 1 : 0;
// Hoặc veto một giá trị, thay vì sửa payload:
return opendash.setOutput('{"decision":"veto","code":"review_policy","message":"Nội dung không đạt quy tắc","field_errors":{"title":"Cần kiểm tra tiêu đề"}}') < 0 ? 1 : 0;
```

Hai dòng trên là **hai nhánh thay thế**, không chạy nối tiếp. Với `content/after_save`, `payload` là entry đã lưu và đầu ra guest không quyết định thành công của Save. Cả hai save hook không cho phép `content_save`, ghi KV hay gọi HTTP ngoài; `content_save` dành riêng cho action/plugin run ở chế độ thường.

Ví dụ lệnh trong action (thay ID và thông tin entry bằng dữ liệu thật):

```ts
const response = opendash.hostCall('{"op":"content_save","type_key":"review","slug":"review-001","title":"Review 001","values":{},"term_ids":[42]}');
opendash.setOutput(response);
```

`term_ids: [42]` chỉ hợp lệ nếu term ID 42 tồn tại và được gắn cho CPT `review`. Bỏ `term_ids` khi update để giữ liên kết cũ; `[]` là xóa liên kết có chủ đích. Với update, cung cấp thêm `entry_id` và `expected_version`; lỗi CAS hoặc quyền không được bypass bằng Wasmi. Xem ví dụ manifest trong [sample-plugin](../examples/plugins/sample-plugin/README.md) và [sample-theme](../examples/themes/sample-theme/README.md).
