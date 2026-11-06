# Sample Transformer Plugin

Mẫu dùng [SDK AssemblyScript](../../../sdk/README.md) và [Plugin ABI v1](../../../../docs/contracts/PLUGIN_ABI.md). Từ thư mục này chạy `npm ci` và `npm run build` để tạo `sample-plugin.zip` gồm manifest và plugin.wasm. Đăng nhập Admin, chọn ZIP rồi chạy với payload `{"text":"OpenDash"}`. Kết quả chứa văn bản viết hoa và bộ đếm trong SQLite KV theo plugin.
