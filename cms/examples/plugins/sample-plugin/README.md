# Sample Plugin — draft

Mẫu `manifest.json` và hàm `opendash_run()` theo [Plugin ABI v1](../../../../docs/contracts/PLUGIN_ABI.md). Chưa có `plugin.wasm`, script biên dịch hoặc ZIP cài đặt.

Khi hoàn thiện plugin SDK, cần biên dịch AssemblyScript ra WebAssembly, xác minh guest export `memory` và `opendash_run() -> i32`, kiểm tra import whitelist, rồi đóng ZIP chứa `manifest.json` và `plugin.wasm`.
