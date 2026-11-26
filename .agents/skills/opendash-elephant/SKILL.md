---
name: opendash-elephant
description: Nạp ngữ cảnh kiến trúc, hợp đồng lõi, ranh giới code và danh mục thiết kế của OpenDash CMS cho Elephant và Goldfish trong quy trình EGM (Elephant-Goldfish Model).
---

# OpenDash Elephant Context Loader (`opendash-elephant`)

Đọc skill này trước khi chạy `/egm-brainstorm`, `/egm-draft`, `/egm-review`, `/egm-review-loop`, `/egm-resolve` hoặc `/egm-drift-report` trên OpenDash CMS để nạp đầy đủ ngữ cảnh dự án và danh sách `GoldfishContext` chuẩn.

## 1. Bất biến Kiến trúc & Sản phẩm của OpenDash

1. **Ràng buộc Sản phẩm ([`docs/product/PRODUCT_REQUIREMENTS.md`](../../../docs/product/PRODUCT_REQUIREMENTS.md)):**
   - CMS viết bằng **Rust + SQLite**, đóng gói thành **một file thực thi duy nhất** (`opendash` / `opendash.exe`) có nhúng React Admin (`rust-embed`).
   - Dữ liệu của website (`database/`, `themes/`, `plugins/`, `uploads/`, `backups/`, `config/`) tách biệt hoàn toàn với mã nguồn phát triển trong `cms/`.
   - **Bảo toàn dữ liệu:** Đổi/cập nhật theme hoặc tắt/gỡ plugin giữ nguyên CPT, custom fields, taxonomy terms, nội dung đã nhập và bố cục người dùng đã chỉnh.
   - **Lưu vs Xuất bản:** Với `post`, `page` và các CPT đặt `publication: "draft_publish"`, thao tác **Lưu** trên bản `draft` chỉ ghi DB (không công khai URL/archive/sitemap/media); chỉ thao tác **Xuất bản** mới công khai. Với Visual Builder (homepage/template overrides) và CPT chế độ `immediate`, **Lưu** cập nhật trang công khai ngay.
2. **Ranh giới Phân tầng & Base APIs ([`docs/contracts/CORE_FOUNDATION.md`](../../../docs/contracts/CORE_FOUNDATION.md)):**
   - `cms/crates/opendash-core/`: Thuần Rust (`serde`, `serde_json`, `url`), chứa domain types, validators (`id`, `registry`, `hooks`, `content`, `builder`, `theme`, `plugin`, `contact`, `backup`, `admin_extensions`). Tuyệt đối không chứa I/O, HTTP hay SQL.
   - `cms/crates/opendash-server/src/storage/`: Sở hữu **100% câu lệnh `sqlx::query*` và `sqlx::raw_sql`**. Không viết SQL trực tiếp trong `routes/`.
   - `cms/crates/opendash-server/src/routes/`: Chỉ thực hiện guard xác thực/CSRF (`require_session`, `require_admin`, `require_session_with_csrf`, `require_admin_with_csrf`), admission lease (`AdmittedMutation` từ `SiteCoordinator`), kiểm tra input và ánh xạ HTTP response.
   - `cms/admin/src/`: React 18 + TypeScript + Vite + Tailwind CSS v4. Mọi lời gọi `fetch` đi qua `cms/admin/src/api/client.ts`; giữ bản nháp qua `EditorDraftStore` khi gặp `401`/`412`.
3. **Runtime Plugin & Giao thức ABI ([`docs/contracts/PLUGIN_ABI.md`](../../../docs/contracts/PLUGIN_ABI.md)):**
   - Chạy WebAssembly qua **Wasmi (`=2.0.0`)** trong tiến trình worker cô lập (`--plugin-worker`), giới hạn fuel, bộ nhớ 16 MiB, deadline 5s, DNS-pinned HTTP egress và `StagedCommitGuard`.

## 2. Danh sách `GoldfishContext` Mặc định cho `/egm-review`

Khi khởi chạy `/egm-review` hoặc `/egm-review-loop` cho một Design Spec trong [`docs/designs/`](../../../docs/designs/SITEMAP.md), luôn cung cấp các file nền tảng sau vào `GoldfishContext` (cộng với các file mã nguồn trong `cms/` được liên kết trực tiếp từ Design Spec):

- [`docs/product/PRODUCT_REQUIREMENTS.md`](../../../docs/product/PRODUCT_REQUIREMENTS.md)
- [`docs/architecture/CMS_ARCHITECTURE.md`](../../../docs/architecture/CMS_ARCHITECTURE.md)
- [`docs/architecture/TECH_STACK.md`](../../../docs/architecture/TECH_STACK.md)
- [`docs/contracts/CORE_FOUNDATION.md`](../../../docs/contracts/CORE_FOUNDATION.md)
- [`docs/contracts/UNIFIED_FIELD_SCHEMA.md`](../../../docs/contracts/UNIFIED_FIELD_SCHEMA.md)
- [`docs/contracts/PLUGIN_ABI.md`](../../../docs/contracts/PLUGIN_ABI.md)
- [`docs/contracts/M8_WRITER_INVENTORY.md`](../../../docs/contracts/M8_WRITER_INVENTORY.md)
- [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md)

## 3. Kiểm chứng Bắt buộc khi Triển khai (Implementation & Verification)

Mọi mục `### Verification Plan` trong `docs/designs/<feature>.md` và mọi lượt triển khai code trong `cms/` đều phải chạy và vượt qua các kiểm tra sau từ gốc repository:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
cargo fmt --manifest-path cms/Cargo.toml --all --check
python cms/scripts/run-tests.py --changed
# Trước release hoặc khi đổi lõi dùng chung:
# python cms/scripts/run-tests.py --group all --release
```
