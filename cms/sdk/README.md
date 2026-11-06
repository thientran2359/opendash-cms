# SDK cho plugin OpenDash

`as/opendash.ts` mô tả bốn host import của [Plugin ABI v1](../../docs/contracts/PLUGIN_ABI.md) bằng AssemblyScript. Đây mới là phần khai báo API; chưa có bộ build/package/versioning hoàn chỉnh.

Plugin chạy server sẽ được biên dịch thành `plugin.wasm` và đóng ZIP cùng `manifest.json`. Host plugin Wasmi chưa được triển khai trong scaffold nên chưa thể cài hoặc chạy ZIP.
