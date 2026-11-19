# Sample Transformer Plugin

Mẫu dùng [SDK AssemblyScript](../../../sdk/README.md) và [Plugin ABI v1](../../../../docs/contracts/PLUGIN_ABI.md). Từ thư mục này chạy `npm ci` và `npm run build` để tạo `sample-plugin.zip` gồm manifest và plugin.wasm. Đăng nhập Admin, chọn ZIP rồi chạy với payload `{"text":"OpenDash"}`. Kết quả chứa văn bản viết hoa và bộ đếm trong SQLite KV theo plugin.

## Ví dụ khai báo C4 (tham khảo, chưa nằm trong ZIP mẫu)

`manifest.json` và `assembly/index.ts` trong thư mục này vẫn là **transformer legacy**; không có khai báo CPT/taxonomy và không đăng ký hook. Chúng tiếp tục hoạt động khi thiếu `declarations`, `hooks` và `actions`. Để làm **một plugin mới** quản lý Reviews, ví dụ `manifest.json` dưới đây dùng ABI Wasmi v1 (`api_version: 1`) và cấu trúc manifest 2:

```json
{
  "id": "reviews-provider",
  "version": "1.0.0",
  "api_version": 1,
  "manifest_schema": 2,
  "name": "Reviews Provider",
  "entrypoint": "plugin.wasm",
  "permissions": {
    "content_read": ["review"],
    "content_write": ["review"]
  },
  "hooks": ["content/before_save", "content/after_save"],
  "actions": [
    {
      "id": "create_review",
      "title": "Tạo review",
      "required_role": "editor",
      "capabilities": ["content_write"]
    }
  ],
  "declarations": {
    "taxonomy_definitions": [
      {
        "key": "review_genre",
        "label": "Thể loại review",
        "hierarchical": true,
        "public": true,
        "rewrite_base": "review-genres"
      }
    ],
    "content_types": [
      {
        "key": "review",
        "label": "Review",
        "fields": [],
        "taxonomies": ["review_genre", "category"],
        "settings": {
          "public": true,
          "has_archive": true,
          "publication": "draft_publish",
          "supports": ["editor", "featured_image"]
        },
        "rewrite_base": "reviews"
      }
    ],
    "taxonomy_eligibility": [
      { "content_type": "review", "taxonomies": ["tag"] }
    ]
  }
}
```

`taxonomy_definitions` tạo định nghĩa có ownership của plugin; `content_types[].taxonomies` và `taxonomy_eligibility` gắn CPT với taxonomy provider, lõi hoặc site đã tồn tại. Không khai báo chiếm `category`/`tag` và key thuộc package khác. `settings` và `rewrite_base` đều tùy chọn; thiếu chúng giữ behavior cũ. Base là đường dẫn tương đối không có `/` đầu, tối đa ba đoạn kebab-case; core/C3 kiểm tra reserved root, va chạm với entry/term/archive/redirect và quyền ưu tiên override của site trong transaction. Nếu trùng thì gói bị từ chối và phiên bản đang active được giữ.

`publication: "draft_publish"` cho review mới có workflow nháp/xuất bản; Save nháp không mở public URL, và hook không xuất bản thay người dùng. Việc đổi/tắt plugin giữ định nghĩa, entry, term và liên kết đã có theo chính sách bảo toàn dữ liệu/provider.

## Wasmi: Allow, Veto, AfterSave và ContentSave

Các hook dùng một export `opendash_run(): i32` và các hàm từ SDK. Ví dụ phần xử lý dành cho **plugin mới** (thay `assembly/index.ts` của plugin mới, không thay code transformer của mẫu này):

```ts
import * as opendash from "../../../../sdk/as/opendash";
import { JSON } from "assemblyscript-json/assembly";

export function opendash_run(): i32 {
  const input = JSON.parse(opendash.getInput());
  if (!(input instanceof JSON.Obj)) return 2;
  const envelope = <JSON.Obj>input;
  const event = envelope.getString("event");

  if (event !== null && event.valueOf() == "content/before_save") {
    // before_save chỉ được đọc dữ liệu mà permissions.content_read cho phép.
    const payload = envelope.getValue("payload");
    if (payload instanceof JSON.Obj) {
      const title = (<JSON.Obj>payload).getString("title");
      if (title !== null && title.valueOf().startsWith("BLOCK:")) {
        return opendash.setOutput(
          '{"decision":"veto","code":"review_title_blocked","message":"Tiêu đề bị chặn","field_errors":{"title":"Bỏ tiền tố BLOCK:"}}'
        ) < 0 ? 1 : 0;
      }
    }
    return opendash.setOutput('{"decision":"allow"}') < 0 ? 1 : 0;
  }

  if (event !== null && event.valueOf() == "content/after_save") {
    // Đã commit: có thể ghi phản hồi quan sát; host không dùng nó để rollback.
    return opendash.setOutput('{"observed":true}') < 0 ? 1 : 0;
  }

  // Chỉ khi được gọi qua action create_review ở chế độ bình thường.
  // Term 42 phải tồn tại và review phải eligible; dùng slug riêng mỗi lần gọi.
  const result = opendash.hostCall(
    '{"op":"content_save","type_key":"review","slug":"demo-review-1","title":"Review 1","values":{},"term_ids":[42]}'
  );
  return opendash.setOutput(result) < 0 ? 1 : 0;
}
```

`content/before_save` phải trả JSON có `decision: allow | veto`; khi veto, host chặn Save (422) trước khi ghi dữ liệu. Hook không được tùy tiện sửa `title`, `values`, `term_ids` hoặc gọi `content_save`/KV write/HTTP. `content/after_save` chạy sau commit và lỗi của nó chỉ được ghi nhận, không hủy Save. Với cả hai hook, quyền `content_read` và giới hạn truy cập field vẫn được áp dụng.

`content_save` là **host operation cho action**, có thêm kiểm tra vai trò, `actions[].capabilities: ["content_write"]`, `permissions.content_write` và CAS. Update cần `entry_id` cùng `expected_version`; `term_ids` bỏ qua giữ các link hiện có, `[]` xóa link rõ ràng. ID term thực tế phải thuộc taxonomy eligible; chỉ một lệnh ghi nội dung được stage mỗi action. Ví dụ tĩnh trên có thể gặp xung đột slug khi gọi lại; plugin thật phải nhận và kiểm tra dữ liệu action đầu vào.

C4 **không** hỗ trợ bộ lọc URL/regex tùy ý, hook mutate nội dung trước commit hoặc hook bypass phiên bản, quyền và public/draft policy. Xem [bảng hỗ trợ SDK](../../../sdk/README.md) và [contract C4](../../../../docs/contracts/CONTENT_FOUNDATION_2026_11.md).
