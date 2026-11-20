# OpenDash — Design Sitemap (`SITEMAP.md`)

Bảng mục lục này theo dõi toàn bộ các đặc tả thiết kế kỹ thuật (Design Specifications) của OpenDash theo quy trình **EGM (Elephant-Goldfish Model)**.

- Khi hoàn tất bước `/egm-draft`, đăng ký file thiết kế mới vào bảng **1. Đặc tả Thiết kế EGM (`docs/designs/`)** bên dưới.
- Các hợp đồng và thiết kế nền tảng đã triển khai từ giai đoạn `M1`–`M8` (migrations `0001`–`0041`) được lập chỉ mục tại bảng **2. Hợp đồng & Thiết kế Nền tảng Đã Triển khai (`cms/` Baseline)** để làm `GoldfishContext` khi soạn hoặc review tính năng mới.

---

## 1. Đặc tả Thiết kế EGM (`docs/designs/`)

| Design Document | Date | Author | Status | Components | Description |
| --- | --- | --- | --- | --- | --- |
| [Hướng dẫn & Chuẩn Thiết kế EGM](README.md) | 2026-11-20 | Owner / Nova | Approved | `docs/designs/`, `.agents/skills/opendash-elephant/` | Quy chuẩn cấu trúc Design Spec (`FeatureRange` + `Detailed Implementation Plan`) và luồng làm việc Elephant → Goldfish → Code. |
| [Builder content, resolver và theme](BUILDER_THEME_CONTENT.md) | 2026-11-20 | Atlas | Proposed | Builder / Theme / Content | Bản chuyển quyết định đã chốt từ #413; còn câu hỏi API/schema và chưa cấp implementation Ready. |

---

## 2. Hợp đồng & Thiết kế Nền tảng Đã Triển khai (`cms/` Baseline `0001`–`0041`)

Các tài liệu dưới đây mô tả kiến trúc và hợp đồng kỹ thuật đã được hiện thực hóa trong `cms/`. Khi chạy `/egm-review` hoặc `/egm-drift-report`, nạp các file liên quan vào `GoldfishContext`:

| Baseline Specification / Contract | Date | Status | Migrations / Code Scope | Description |
| --- | --- | --- | --- | --- |
| [Yêu cầu Sản phẩm V1](../product/PRODUCT_REQUIREMENTS.md) | 2026-11-18 | Implemented (Baseline) | Toàn bộ `cms/` | Phạm vi sản phẩm CMS Rust + SQLite 1 file thực thi, Visual Builder, Theme/Plugin ZIP, Lưu vs Xuất bản và Usable Beta. |
| [Kiến trúc CMS & Workspace](../architecture/CMS_ARCHITECTURE.md) | 2026-11-07 | Implemented | `cms/crates/opendash-core`, `cms/crates/opendash-server`, `cms/admin` | Ranh giới 2 crate Rust + React Admin, `SitePaths` và nguyên tắc tách dữ liệu website khỏi theme/plugin. |
| [Đánh giá & Chọn Tech Stack](../architecture/TECH_STACK.md) | 2026-11-07 | Implemented | `cms/Cargo.toml`, `cms/admin/package.json` | Axum, SQLx/SQLite, MiniJinja, Wasmi, RustEmbed, React + TypeScript + Vite + Tailwind CSS. |
| [Core Foundation & Quy chuẩn Code](../contracts/CORE_FOUNDATION.md) | 2026-11-07 | Implemented | `opendash_core::{id, registry, hooks}`, `cms/scripts/check-standards.py` | Bảng Base APIs bắt buộc tái sử dụng, quy chuẩn định danh, phân tầng route/storage/core và kiểm tra tự động. |
| [Plugin ABI v1](../contracts/PLUGIN_ABI.md) | 2026-11-07 | Implemented | `cms/crates/opendash-server/src/plugin/`, `cms/sdk/as/opendash.ts`, `0003` | Giao thức WebAssembly `opendash_v1`, worker cô lập tiến trình, giới hạn fuel/memory/timeout và `StagedCommitGuard`. |
| [M5 Theme Packages, Templates & C-SEC](../contracts/M5_THEME_TEMPLATES.md) | 2026-11-13 | Implemented | `0015`, `0017`–`0021`, `theme.rs`, `template_sec.rs`, `public_context.rs`, `storage/{theme,overrides,menu,import}.rs` | Theme ZIP v2, revisions bất biến (`A-DURABILITY`), MiniJinja safe subset (`C-SEC`), template overrides, Theme Options và sample import 3-phase journal. |
| [M6 Plugin Extensions & Egress](../contracts/M6_PLUGIN_EXTENSIONS.md) | 2026-11-13 | Implemented | `0022`–`0024`, `plugin/{egress,host_ops,manager,package}.rs`, `hooks.rs`, `DeclarativePluginUi.tsx` | Plugin revisions & generation fencing, owner-neutral CNT/domain bridge, `ContentBeforeSave`/`ContentAfterSave`, standalone actions, DNS-pinned HTTP egress và declarative Admin UI. |
| [M7 Site Features, SEO, Contact/SMTP & Video](../contracts/M7_SITE_FEATURES.md) | 2026-11-14 | Implemented | `0025`–`0031`, `seo.rs`, `document.rs`, `contact.rs`, `smtp.rs`, `secrets.rs`, `media_video.rs` | Public site settings, menu management, SEO head/document normalizer, sitemap, contact forms, SMTP notifications và MP4 video ingestion/Range serving. |
| [M8 Operations, Media Lifecycle & Backup/Restore](../contracts/M8_OPERATIONS_RELEASE.md) | 2026-11-16 | Implemented | `0032`–`0033`, `operations.rs`, `media_lifecycle.rs`, `backup*.rs`, `restore.rs`, `config.rs`, [Writer Inventory](../contracts/M8_WRITER_INVENTORY.md) | `SiteCoordinator` file locks & `MutationLease` barrier, referenced media delete/quarantine/reconcile, `age`-encrypted snapshot backup, restore-to-new-root Safe Mode và Linux musl packaging. |
| [Admin Navigation, Panel Registry & Single Edit](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) | 2026-11-16 | Implemented | `0034`, `opendash-core/src/content.rs`, `SingleEditWorkspace.tsx`, `EditorDraftStore.ts` | `ScopedResourceId`, 3-axis field ACL (`min_view_role`, `min_edit_role`, `public_exposure`), field groups, panel registry và `CompositeEntrySaveInput` (`PatchField<T>`). |
| [Owner Admin Review Delta (Notes 01–07)](../contracts/ADMIN_REVIEW_2026_11.md) | 2026-11-17 | Implemented | `0035`–`0037`, `admin_extensions.rs`, `entry_table.rs`, `sections.rs`, `AdminShell.tsx`, `EntryListPage.tsx`, `TemplateLibraryPage.tsx` | Điều hướng Admin 3 nhóm, bảng danh sách `/entry-table`, declarative provider columns/actions/term panels, Page-scoped Builder, private Section CPT và thư viện mẫu cục bộ. |
| [Content Foundation (C1–C4)](../contracts/CONTENT_FOUNDATION_2026_11.md) | 2026-11-19 | Implemented | `0038`–`0041`, `storage/permalinks.rs`, `TaxonomyDefinitionsPage.tsx`, `PermalinkSettingsPage.tsx` | Cấu hình hành vi CPT (`public`, `has_archive`, `publication: immediate\|draft_publish`), custom taxonomy definitions, single route/permalink authority và provider rewrite defaults. |
