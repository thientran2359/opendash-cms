# Themes của website

Thư mục cấp gốc dành cho **theme đã được cài đặt** của website OpenDash. Khi hệ thống cài theme ZIP được triển khai, gói theme sẽ được kiểm tra rồi đặt dưới `themes/<theme-id>/`.

Ví dụ cấu trúc dự kiến:

```text
themes/
├── README.md
└── ocean/
    ├── manifest.json
    ├── templates/
    └── assets/
```

Hiện tại **chưa có trình cài theme ZIP**; không có theme nào được kích hoạt hay nạp tự động từ thư mục này. Mẫu để nghiên cứu cấu trúc nằm tại [`cms/examples/themes/sample-theme/`](../cms/examples/themes/sample-theme/README.md).

Nội dung theme cài đặt là dữ liệu riêng của website và được loại trừ khỏi Git ở repository CMS (trừ README này). Khi phát triển theme cần quản lý mã nguồn bằng repository riêng.
