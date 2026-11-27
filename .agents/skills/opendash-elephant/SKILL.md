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

## 3. Kiểm chứng 3 Tầng khi Triển khai (`Owner Directive 2026-11-27`)

Mọi mục `### Verification Plan` trong `docs/designs/<feature>.md` và mọi Feature Worktree (`feat/<slug>`) áp dụng đúng 3 tầng kiểm chứng:

```bash
# Tầng 0 (giữa các Slice trung gian S01..S(n-1) — chỉ kiểm biên dịch nhanh, KHÔNG chạy test):
python3 cms/scripts/run-tests.py --check
# (+ `npm --prefix cms/admin run typecheck` nếu sửa cms/admin/)

# Tầng 1 (chạy 1 lần duy nhất khi code xong toàn bộ tính năng, trước Bước 6 báo cáo Owner):
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
cargo fmt --manifest-path cms/Cargo.toml --all --check
# (+ `npm --prefix cms/admin run build && node cms/scripts/check-i18n.mjs --strict` nếu sửa cms/admin/)
python3 cms/scripts/run-tests.py --changed
git diff --check

# Tầng 2 (theo đợt gom 3–5 tính năng, trước release hoặc khi Owner yêu cầu):
# python3 cms/scripts/run-tests.py --group all --release
```

## 4. Vòng đời 7 Bước, Provenance & Drift Report

- **Chọn cỡ Goldfish & Giải thích Tự nhiên (Bước 2–4):** Tính năng nhỏ/vừa/UI chạy 1 vòng Goldfish gọn (hoặc bỏ qua nếu Owner yêu cầu); tính năng lớn / đụng lõi / DB / C-SEC / Plugin ABI chạy 1–2 vòng. Tóm tắt ngắn gọn bằng ngôn ngữ tự nhiên cho Owner (làm gì, đổi gì, không làm gì, rủi ro/đánh đổi); khi được chốt mới đặt `Status: Approved` với đủ 3 trường provenance (`**Approved By:**`, `**Approval Evidence:**`, `**Goldfish Review:**`), không còn `Open Questions` mở hay `TODO`/`TBD`. Đặt trước số SQL migration ngay Bước 2 nếu chạy nhiều tính năng song song.
- **Chia Slice nội bộ trong Spec (không tạo Issue lẻ theo Slice):** Mỗi `### Slice S01: ...` trong `## Detailed Implementation Plan` vẫn giữ đủ `**Dependencies:**`, `**File Ownership:**`, `**Acceptance Criteria:**`, `**Rollback:**` để dẫn hướng triển khai, nhưng agent tự code tuần tự tất cả Slice trên 1 worktree `feat/<slug>` và gom vào **1 Feature PR duy nhất**.
- **Shift-Left Drift & Báo cáo Tự nhiên (Bước 5–6):** Sau khi code xong toàn bộ tính năng và chạy Tầng 1, thực hiện kiểm tra drift gọn (`/egm-drift-report` trên `feat/<slug>`), mở 1 Feature PR rồi báo cáo cho Owner bằng ngôn ngữ tự nhiên (đã làm gì, test đạt mức nào, còn giới hạn gì).
- **Owner chốt, Merge & Dọn Worktree (Bước 7):** Khi Owner đồng ý, sync `origin/main` 1 lần cuối (nếu cần), merge PR vào `main`, cập nhật [`docs/designs/SITEMAP.md`](../../../docs/designs/SITEMAP.md) (`Implemented` / `**Verified**` kèm link [`docs/verification/`](../../../docs/verification/README.md)), chạy `node cms/scripts/codegraph-index-docs.mjs`, rồi xóa branch + xóa worktree + dọn `cms/target`.
