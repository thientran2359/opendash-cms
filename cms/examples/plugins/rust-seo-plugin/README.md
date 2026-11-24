# Rust SEO Plugin Example (`rust-seo-plugin`)

Plugin mẫu bằng Rust sử dụng [`opendash-plugin-sdk`](../../../crates/opendash-plugin-sdk/src/lib.rs) (ABI v1.1, `PluginEnvelopeV2`, 15 Host Ops và `MockHost`) tuân theo [Plugin ABI](../../../../docs/contracts/PLUGIN_ABI.md) và tích hợp với [sample-theme-v3](../../themes/sample-theme-v3/README.md).

## Cấu trúc thư mục

- [`Cargo.toml`](Cargo.toml): Khai báo `[workspace]` độc lập và `crate-type = ["cdylib", "rlib"]` dùng `edition = "2024"` để biên dịch ra `wasm32-unknown-unknown` mà không thêm crate ngoài mới hay phát sinh artifact vào workspace chính.
- [`manifest.json`](manifest.json): Manifest `manifest_schema: 3` khai báo `filter:seo/head`, `filter:entry/view`, `content/after_publish`, `content_read: ["*"]`, `plugin_settings_read: true`, `plugin_secrets_read: ["webhook_token"]` (`default: null`) và egress HTTPS tới `hooks.example.com`.
- [`src/lib.rs`](src/lib.rs): Handler `handle(Envelope)` gắn macro `export_plugin!(handle)` cùng bộ unit test chạy trực tiếp trên host qua `MockHost`.

## Chạy kiểm thử trên host (`MockHost`)

Không cần cài target `wasm32-unknown-unknown`:

```bash
cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --release
```

## Biên dịch WebAssembly, đóng gói ZIP và `--check-plugin`

Khi máy đã cài target `wasm32-unknown-unknown`:

```bash
rustup target add wasm32-unknown-unknown
cargo build --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --target wasm32-unknown-unknown --release
cp cms/examples/plugins/rust-seo-plugin/target/wasm32-unknown-unknown/release/rust_seo_plugin.wasm /tmp/plugin.wasm
zip -j /tmp/rust-seo-plugin.zip cms/examples/plugins/rust-seo-plugin/manifest.json /tmp/plugin.wasm
./cms/target/release/opendash --check-plugin /tmp/rust-seo-plugin.zip
```
