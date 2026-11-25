# OpenDash — Bản đồ Kiến trúc CMS và Cấu trúc Thư mục

**Nguồn định hướng:**
- [Yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md): Phạm vi tính năng và ràng buộc nghiệp vụ V1.
- [Tech Stack](TECH_STACK.md): Các thư viện đã pin trong `cms/Cargo.toml` và `cms/admin/package.json`.
- [Bộ khung Lõi & Quy chuẩn Code](../contracts/CORE_FOUNDATION.md): Danh mục Base APIs bắt buộc tái sử dụng và ranh giới phân tầng.

---

## 1. Ranh giới Kiến trúc: `opendash-core`, `opendash-server` và `admin`

Mã nguồn CMS được tổ chức thành **Workspace Rust 2 crate** kết hợp với ứng dụng **React Admin** và đóng gói ra **đúng một file thực thi duy nhất** (`opendash` / `opendash.exe`):

```mermaid
flowchart LR
    subgraph Admin["cms/admin/ (React + TS + Vite + Tailwind CSS)"]
        Shell["Admin Shell & 18 Feature Modules\n(Posts/Pages/CPT, Media, Builder, Themes, Plugins, Operations)"]
        Builder["Visual Builder Workspace\n(Section -> Row -> Column -> Element, Iframe Preview)"]
        ApiClient["API Client (cms/admin/src/api/client.ts)\n(CSRF, giữ draft khi 401/412 qua EditorDraftStore)"]
        Shell --> ApiClient
        Builder --> ApiClient
    end

    subgraph Server["cms/crates/opendash-server (Binary duy nhất)"]
        Routes["HTTP Routes & Guards (src/routes/)\n(Axum, Auth/RBAC, CSRF, AdmittedMutation)"]
        UseCases["Domain Services & Renderers\n(MiniJinja C-SEC, Document Normalizer, Media/Backup/SMTP)"]
        Storage["Storage & Migrations (src/storage/, 0001-0041)\n(100% SQLx + SQLite WAL)"]
        PluginHost["Plugin Host & Worker (src/plugin/)\n(Wasmi process-isolated, DNS-pinned Egress)"]
        Routes --> UseCases
        UseCases --> Storage
        UseCases --> PluginHost
    end

    subgraph Core["cms/crates/opendash-core (Thư viện thuần Rust)"]
        Domain["Domain Models & Validators\n(id, registry, hooks, content, builder, theme, plugin, contact, backup, admin_extensions)"]
    end

    ApiClient -->|HTTP / JSON| Routes
    UseCases --> Domain
    PluginHost --> Domain
```

### 1.1. Trách nhiệm từng phần

1. **`cms/crates/opendash-core` (Thư viện Rust thuần domain & kiểm tra hợp lệ):**
   - Chỉ phụ thuộc `serde`, `serde_json`, `url`. Tuyệt đối không dùng `axum`, `sqlx`, `wasmi`, `tokio`, `reqwest` hoặc `std::{fs, io, net, process, thread}`.
   - Gồm 10 modules ([`src/lib.rs`](../../cms/crates/opendash-core/src/lib.rs)): `id`, `registry`, `hooks`, `content`, `builder`, `theme`, `plugin`, `contact`, `backup`, `admin_extensions`.
2. **`cms/crates/opendash-server` (Ứng dụng thực thi phụ thuộc `opendash-core`):**
   - **`routes/` (22 modules):** Xác thực phiên/CSRF (`require_*`), admission lease (`AdmittedMutation`), kiểm tra body/headers và ánh xạ HTTP response (`ApiError`). Không chứa câu lệnh SQL.
   - **`storage/` (31 files):** Sở hữu **100% truy vấn `sqlx::query*` / `sqlx::raw_sql`**, transactions `BEGIN IMMEDIATE` và migrations `0001`–`0041`.
   - **`plugin/`:** Vòng đời plugin revisions, worker cô lập tiến trình (`--plugin-worker`), `host_ops` (`kv_get`, `kv_set`, `http_request`, `content_save`) và HTTP egress chống SSRF (`egress.rs`).
   - **Hạ tầng server:** `SiteCoordinator` (`operations.rs`), `SitePaths` (`paths.rs`), `template_sec.rs` (`C-SEC`), `document.rs` (`normalize_public_document`), `backup*.rs` & `restore.rs` (`age` encrypted), `media*.rs`, `smtp.rs`, `secrets.rs`, `config.rs`, và `rust-embed` (`app.rs`).
3. **`cms/admin/` (Giao diện quản trị và Builder trên trình duyệt):**
   - **`api/` (21 modules):** Mọi lời gọi HTTP đi qua [`client.ts`](../../cms/admin/src/api/client.ts).
   - **`shell/`:** `AdminShell.tsx` (điều hướng 3 nhóm `Nội dung`, `Giao diện`, `Tùy chỉnh`), `router.ts`, `AuthPanel.tsx`.
   - **`builder/`:** `BuilderWorkspace.tsx`, `BuilderTree.tsx`, `BuilderCanvas.tsx`, `BuilderInspector.tsx` (hỗ trợ Homepage, Template Overrides, Page-scoped Builder và private Section CPT).
   - **`features/` (18 thư mục):** `contact/`, `content/`, `dashboard/`, `editor/` (`SingleEditWorkspace.tsx`, `EditorDraftStore.ts`), `extensions/`, `library/`, `media/`, `operations/`, `patterns/`, `plugins/`, `posts/`, `seo/`, `settings/`, `shared/`, `system/`, `taxonomy/`, `themes/`, `users/`.

### 1.2. Luồng Xem trước (Preview), Lưu (Save), Xuất bản (Publish) và Quyền sở hữu Dữ liệu

- **Xem trước (Preview):** Gửi dữ liệu chưa lưu lên route preview có xác thực và `sequence ID` cục bộ; server dùng cùng bộ render MiniJinja (`C-SEC`) trả HTML vào `iframe sandbox=""`. Thay đổi chưa lưu **không ghi đè trang công khai**.
- **Lưu vs Xuất bản:**
  - Với `post`, `page` và các CPT cấu hình `publication: "draft_publish"` (`0038_content_type_settings.sql`), thao tác **Lưu** trên bản `draft` chỉ ghi DB (không công khai URL, archive, sitemap hoặc media ACL); chỉ thao tác **Xuất bản** mới công khai.
  - Với **Visual Builder** (homepage, template overrides, exact Page Builder) và các CPT để chế độ `immediate`, thao tác **Lưu** cập nhật công khai ngay.
- **Tách biệt Dữ liệu Website & Mã nguồn (`SitePaths`):**
  - Toàn bộ dữ liệu runtime của website tách khỏi mã nguồn trong `cms/` thông qua `SitePaths` (`OPENDASH_SITE_ROOT`, `OPENDASH_DB_PATH`): `database/opendash.sqlite`, `database/secrets/smtp/`, `themes/revisions/`, `plugins/revisions/`, `uploads/media/`, `uploads/.quarantine/`, `backups/artifacts/`, `config/site.json`, `runtime/site.lock`.
  - Staging cùng ổ đĩa (`runtime/staging/`, `.staging/<run_uuid>/`) đảm bảo `std::fs::rename` nguyên tử trên cả Windows và POSIX.
  - Mọi integration test và smoke script bắt buộc trỏ `OPENDASH_SITE_ROOT` vào thư mục tạm riêng, không ghi đè dữ liệu tại root repository.

---

## 2. Cấu trúc Workspace Thực tế

```text
opendash-cms/
├── .agents/                                # Skill ngữ cảnh opendash-elephant và tài liệu lịch sử
├── cms/                                    # Mã nguồn CMS, migrations và công cụ build/test
│   ├── Cargo.toml                          # [workspace] members = ["crates/opendash-core", "crates/opendash-server"]
│   ├── Cargo.lock
│   ├── crates/
│   │   ├── opendash-core/src/              # 10 domain modules thuần Rust (lib.rs)
│   │   └── opendash-server/src/            # Server binary: routes/ (22), storage/ (31), plugin/, tests/
│   ├── admin/src/                          # React + TS + Vite + Tailwind CSS (api/, shell/, builder/, features/)
│   ├── migrations/                         # 40 migrations SQLite (0001–0041, bỏ qua số 0016)
│   ├── fixtures/                           # Database mẫu kiểm tra nâng cấp (baseline_0001.sqlite)
│   ├── sdk/                                # AssemblyScript Plugin SDK (sdk/as/opendash.ts)
│   ├── examples/                           # Plugin/Theme mẫu (sample-plugin, rust-seo-plugin, sample-theme-v3)
│   ├── deploy/                             # Cấu hình systemd & nginx mẫu cho Linux VPS
│   └── scripts/                            # build.ps1, build-release-linux.sh, check-standards.py, check-plans.py, E2E scripts
├── docs/                                   # Tài liệu sản phẩm, kiến trúc, contracts, designs (EGM), verification, archive
├── database/, themes/, plugins/, uploads/  # Thư mục dữ liệu website ở root (tách khỏi cms/)
├── AGENTS.md                               # Quy trình chuẩn EGM & ranh giới kỹ thuật
└── README.md                               # Hướng dẫn chạy dev & build release
```

> Hồ sơ lịch sử chuyển đổi cấu trúc M1/M2 từ 1 crate sang 2 crate (`2026-11-07-m1-m2-module-map.md` và `2026-11-07-layout-migration.md`) được lưu giữ trong Git history.
