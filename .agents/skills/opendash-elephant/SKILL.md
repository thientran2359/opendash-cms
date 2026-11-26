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

## 2. Danh sách `GoldfishContext` theo Phạm vi cho `/egm-review`

Khi khởi chạy `/egm-review` hoặc `/egm-review-loop` cho một Design Spec trong [`docs/designs/`](../../../docs/designs/SITEMAP.md), **lọc `GoldfishContext` theo phạm vi thực tế** (cộng với các file mã nguồn trong `cms/` kèm dải dòng cụ thể):

- **Lõi mặc định (luôn nạp):**
  - [`docs/product/PRODUCT_REQUIREMENTS.md`](../../../docs/product/PRODUCT_REQUIREMENTS.md)
  - [`docs/contracts/CORE_FOUNDATION.md`](../../../docs/contracts/CORE_FOUNDATION.md)
  - [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md) *(chỉ nạp khi cần kiểm tra xung đột cross-spec)*
- **Nạp có điều kiện theo phạm vi spec:**
  - [`docs/contracts/PLUGIN_ABI.md`](../../../docs/contracts/PLUGIN_ABI.md) *(chỉ khi spec đụng plugin, Wasmi worker hoặc hooks)*
  - [`docs/contracts/M8_WRITER_INVENTORY.md`](../../../docs/contracts/M8_WRITER_INVENTORY.md) *(chỉ khi spec thêm/sửa write endpoints, mutations hoặc admission lease)*
  - [`docs/contracts/UNIFIED_FIELD_SCHEMA.md`](../../../docs/contracts/UNIFIED_FIELD_SCHEMA.md) *(chỉ khi spec đụng fields, blocks hoặc CPT schemas)*
  - [`docs/architecture/CMS_ARCHITECTURE.md`](../../../docs/architecture/CMS_ARCHITECTURE.md) / [`docs/architecture/TECH_STACK.md`](../../../docs/architecture/TECH_STACK.md) *(chỉ khi spec đổi kiến trúc hoặc dependency)*

### 2.1. Quy tắc Tối ưu Token & Ngân sách Ngữ cảnh EGM

1. **Pre-scoping bằng CodeGraph / `rg`:** Trước khi gọi Goldfish, Elephant tìm sẵn danh sách file và dải dòng cụ thể (`path#Lstart-Lend`) truyền vào prompt; yêu cầu Goldfish dùng `codegraph_explore` trước và tránh đọc tuần tự toàn bộ các file lớn (`i18n.rs`, `template_sec.rs`, `field_schema.rs`, `entries.rs`, `settings.rs`).
2. **Giới hạn vòng review (`/egm-review-loop`):** Tối đa **2 vòng**. Vòng 2 chỉ gọi lại đúng reviewer nào còn Blocker/Major ở vòng 1 và giới hạn kiểm tra trên phần diff vừa sửa.
3. **Phân tầng model subagent (`invoke_subagent`):**
   - `Model: "flash"`: `research`, `browser`, `egm-structural`, `egm-style`, `egm-comprehension`.
   - Model cân bằng/mặc định (tránh XHigh trừ khi kiến trúc lõi phức tạp): `egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, `egm-zoom-out`.
4. **Cắt ngữ cảnh hội thoại:** 1 Spec / 1 Feature = 1 conversation; chạy `/compact` hoặc mở conversation mới khi context vượt ~120K–150K hoặc khi chuyển pha lớn (Draft → Review → Closure).

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

## 4. Vòng đời Spec, Provenance & Drift Report

- **3 trường provenance bắt buộc khi chuyển `Status: Approved`:** `**Approved By:**` (`Owner` hoặc `<Coordinator> (delegated)` theo [ADR-0010](../../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md)), `**Approval Evidence:**` và `**Goldfish Review:**`; không còn `Open Questions` mở hay `TODO`/`TBD`.
- **Chia Slice cho Design Spec lớn:** Mỗi `### Slice S01: ...` trong `## Detailed Implementation Plan` phải có `**Dependencies:**`, `**File Ownership:**`, `**Acceptance Criteria:**`, `**Rollback:**`, và tạo Issue bằng `python .agents/scripts/prepare-github-issue.py --source docs/designs/<feature>.md --slice S01`.
- **Đóng Spec:** Sau khi triển khai & kiểm chứng, Coordinator chạy `/egm-drift-report` (lưu tại `.agents/research/` hoặc đính kèm PR closure) trước khi đánh dấu `Verified` trong [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md) và chạy `node cms/scripts/codegraph-index-docs.mjs`.
