# M8 Operations & Release E2E Fixtures

Tài liệu này mô tả bộ fixture xác định (deterministic) cho harness E2E V1 của M8 (`cms/crates/opendash-server/tests/m8_s08_t01_e2e.rs` và `cms/scripts/m8_browser_e2e.py`).

## 1. Thành phần trong thư mục

- `schema_fingerprints.json`: dấu vân tay schema chuẩn (`schema_version: 33`, `trusted_schema: OFF`, giới hạn đối tượng schema) để đối chiếu an toàn trước khi khôi phục.
- Fixture runtime được khởi tạo xác định trong test/script: không lưu binary `.odbackup.age` hay secret tĩnh trong Git.

## 2. Phạm vi dữ liệu V1 trong E2E (`M8-S08-T01`)

- **Auth & Users**: tài khoản Admin ban đầu qua `/auth/setup`.
- **Content Types & Entries**: custom content type, entry `published`, entry `draft` và trạng thái nội bộ để xác minh không rò rỉ ra public sau restore.
- **Media & Pinning**: upload media asset và gắn tham chiếu từ builder/entry để xác minh tệp vật lý và tham chiếu ID được bảo toàn.
- **Menus & Themes**: cấu hình menu public (`/api/admin/menus`) và theme gói ZIP / active theme.
- **Plugins & `plugin_kv`**: cài plugin ZIP hợp lệ + dữ liệu `plugin_kv` được giữ nguyên sau restore.
- **SEO Routes, Contact Forms & Settings**: cập nhật `/api/admin/settings/general`, `/api/admin/settings/public`, tạo form liên hệ `/api/admin/contact/forms` và SEO route/slug.
- **Builder Homepage**: lưu bố cục homepage (`/api/admin/builder/homepage`) với text/image block và xuất bản ra `/`.

## 3. Danh sách ngoại lệ được reset có chủ đích khi Restore

Khi khôi phục từ gói `.odbackup.age` sang `site_root` mới, các trạng thái sau **được phép và bắt buộc reset** theo contract `OPS-01`:

1. **`sessions` / token đăng nhập cũ**: không giữ phiên từ nguồn; Admin phải đăng nhập lại trên target root.
2. **Safe Mode & Recovery Gate**: target khởi động ở `safe_mode = true` (`X-Robots-Tag: noindex, nofollow` trên public route, chặn anonymous contact submit với `503`) cho đến khi Admin xác thực mật khẩu và kích hoạt (`/api/admin/operations/recovery/activate`).
3. **SMTP credentials & `smtp_enabled`**: `smtp_enabled = false`, không mang secret SMTP thực tế sang môi trường khôi phục tự động.
4. **`operation_jobs` đang chạy/queued tại thời điểm snapshot**: khi target khởi động lại sẽ chuyển sang `interrupted` (không tự động retry ngầm).
5. **External DB path rebasing**: nếu nguồn dùng `OPENDASH_DB_PATH` ngoài `site_root`, bản khôi phục rebase về `<target_root>/database/opendash.sqlite` và áp quyền ACL riêng tư cho thư mục `database/` & `private/`.

Mọi dữ liệu nội dung, ID, slug, `values`, `plugin_kv`, media asset, menu, theme, settings, contact form và bố cục builder khác phải đạt **100% tương đương**.

## 4. Negative Corpus

1. **Tampered ciphertext**: sửa 1 byte trong payload `.odbackup.age` → restore thất bại, nguồn nguyên vẹn, target không publish.
2. **Wrong passphrase**: nhập sai passphrase → restore thất bại, nguồn nguyên vẹn, target không publish.
3. **Newer schema rejection**: gói backup khai báo `schema_version` cao hơn binary hiện tại → restore worker từ chối fail-closed.
4. **Interrupted job**: job `running`/`queued` trong DB khi server restart → chuyển `interrupted`, không tự chạy lại.
5. **Artifact quota (`507`)**: đạt `MAX_BACKUP_ARTIFACTS = 10` → request backup thứ 11 trả `507 Insufficient Storage`, không làm hỏng DB/artifact hiện có.
