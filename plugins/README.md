# Plugins của website

Thư mục cấp gốc dành cho **plugin đã cài đặt** của website OpenDash. Khi chức năng quản lý plugin được triển khai, mỗi gói plugin ZIP hợp lệ sẽ được cài vào `plugins/<plugin-id>/`, tuân theo [Plugin ABI v1](../docs/contracts/PLUGIN_ABI.md).

Ví dụ cấu trúc dự kiến:

```text
plugins/
├── README.md
└── sample-plugin/
    ├── manifest.json
    └── plugin.wasm
```

Đây là nơi lưu gói plugin của site, **không phải** nơi triển khai Rust Wasmi host (nằm tại `cms/crates/opendash-server/src/plugin/`). Scaffold hiện chưa cài, tải hay thực thi plugin. Mẫu source AssemblyScript xem ở [`cms/examples/plugins/sample-plugin/`](../cms/examples/plugins/sample-plugin/README.md).

Các plugin cài đặt được loại trừ khỏi Git của CMS (trừ README này). Mã nguồn plugin nên có repository riêng.
