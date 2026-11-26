# Themes của website

Thư mục cấp gốc lưu các bản cài đặt bất biến (immutable revisions) của Theme Format v3 (`manifest_version: 3`), tách biệt khỏi mã nguồn trong [`cms/`](../cms/README.md).

- **Bố cục lưu trữ:** Staging tạm tại `themes/.staging/<run_uuid>/` và revision đã xác thực tại `themes/revisions/<revision_digest>/` (kèm dấu `.complete` và kiểm tra digest khi kích hoạt, phục vụ `/theme/<digest>/...` và khôi phục M8).
- **Quản lý:** Cài đặt và kích hoạt Theme ZIP v3 qua giao diện Admin (`#/appearance/themes`) hoặc kiểm tra offline bằng `opendash --check-theme <path>`. Nội dung runtime bị `.gitignore` bỏ qua (chỉ giữ `README.md`). Xem [`docs/designs/theme-format-and-template-resolver.md`](../docs/designs/theme-format-and-template-resolver.md).
