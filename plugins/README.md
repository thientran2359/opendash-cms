# Plugins của website

Thư mục cấp gốc lưu các bản cài đặt bất biến (immutable revisions) của Plugin ZIP (WebAssembly Wasmi `=2.0.0`, `manifest_schema: 1..=3`), tách biệt khỏi mã nguồn trong [`cms/`](../cms/README.md).

- **Bố cục lưu trữ:** Staging tạm tại `plugins/.staging/<run_uuid>/` và revision đã xác thực tại `plugins/revisions/<revision_digest>/` (kèm `.complete`, kiểm tra `revision_generation` khi bật/tắt/chạy trong `--plugin-worker`).
- **Quản lý & Ví dụ:** Cài/bật/tắt plugin qua Admin (`#/plugins`) hoặc kiểm tra offline bằng `opendash --check-plugin <path>`. Xem ví dụ [Rust SEO Plugin](../cms/examples/plugins/rust-seo-plugin/README.md), [Sample Plugin (AssemblyScript)](../cms/examples/plugins/sample-plugin/README.md) và hợp đồng [`docs/contracts/PLUGIN_ABI.md`](../docs/contracts/PLUGIN_ABI.md). Nội dung runtime bị `.gitignore` bỏ qua (chỉ giữ `README.md`).
