# Báo cáo Kiểm chứng: Hệ thống Bản địa hóa Chuẩn Gettext (`.pot` / `.po` / `.mo`) Toàn diện (`S01`–`S05`)

- **Ngày:** `2026-11-27`
- **Spec:** [gettext-pot-po-mo-localization.md](../designs/gettext-pot-po-mo-localization.md)
- **PR / Issues:** [PR #615](https://github.com/thientran2359/opendash-cms/pull/615) (`2655e8d`) · `#610` (`S01`), `#611` (`S02`), `#612` (`S03`), `#613` (`S04`), `#614` (`S05`) · bản vá hậu kiểm trên `agent/nova`
- **Môi trường:** Windows (`x86_64-pc-windows-msvc`), Rust `--locked --release`, Node.js / Vite (`cms/admin`).
- **Trạng thái chốt:** **`Verified`**

---

## 1. Kết quả Kiểm chứng Tự động (Sau bản vá cuối)

| Hạng mục / Lệnh | Phạm vi | Kết quả | Chi tiết thực tế |
|---|---|---|---|
| `cargo test --manifest-path cms/Cargo.toml --locked --release -p opendash-core -p opendash-plugin-sdk` | Lõi gettext, manifest `text_domain`, `MockHost` `l10n_translate` | **PASS** | `opendash-core`: `153 passed, 0 failed`; `opendash-plugin-sdk`: `5 passed, 0 failed` |
| `cargo test --manifest-path cms/Cargo.toml --locked --release -p opendash-server --test i18n --test theme --test plugin` | `i18n` (bao gồm `s05_language_pack_upload_catalog_etag_and_delete` & `s04_theme_l10n`), `theme`, `plugin` | **PASS** | `i18n`: `19 passed, 0 failed`; `theme`: `79 passed, 0 failed`; `plugin`: `27 passed, 0 failed` |
| `python cms/scripts/run-tests.py --group all --release` *(trước đợt vá cuối cùng)* | Toàn workspace `--release` | **PARTIAL PASS** | `opendash-core` (`153/153`), `opendash-plugin-sdk` (`5/5`), `opendash` unit (`278/278`), `builder` (`14/14`), `content` (`43/43`), `i18n` (`19/19`), `plugin` (`27/27`), `theme` (`79/79`) đều PASS; `ops` `24/25` PASS (`1` lỗi pre-existing tại `writer_barrier.rs:260`: `missing handler entry_trash in entries.rs`) |
| `python cms/scripts/check-standards.py`, `check-plans.py`, `check-agent-artifacts.py`, `node cms/scripts/check-i18n.mjs` | Ranh giới `core`/`storage`/`route`, CSRF/AdmittedMutation (`101` registrations), LOC ratchet, Markdown links, zero hardcoded Vietnamese | **PASS** | Toàn bộ exit `0` |
| `npm --prefix cms/admin run typecheck` & `npm --prefix cms/admin run build` | React Admin UI (`__`, `_x`, `_n`, `_nx`, `plural.ts`, `LanguagePacksPanel.tsx`) | **PASS** | `tsc --noEmit` & `vite build` (`157` modules) exit `0` |

---

## 2. Tóm tắt Goldfish Drift Audit & Khắc phục

1. **Vòng 1 & Vòng 2 (`egm-codebase-details`)** phát hiện các điểm lệch giữa spec và mã nguồn sau merge `#615`, toàn bộ Blocker & Major đã được xử lý:
   - **Sửa `CatalogRegistry::global()` & `reload_site_with_active_packages`:** Nạp đồng thời `EMBEDDED_VI_MO`, site overrides `<site_root>/languages/` và `languages/*.{mo,po}` đóng gói sẵn trong active Theme v3 revision + enabled Plugin revisions; nối vào `main.rs`, `routes/l10n.rs`, `restore_and_maintenance.rs`, `theme_activate`, `dev_sync`, `plugin_install`, `plugin_disable`, `plugin_enable`.
   - **Sửa `theme_render_catalogs` & `public_render.rs`:** Trả key theo đúng `domain` khớp `environment.rs` và hợp nhất catalog từ `CatalogRegistry::global()` lên trên `active.l10n_catalog_map(...)`.
   - **Sửa Plugin/Theme `l10n_translate` (`worker.rs`, `runtime_exec.rs`, `host_read.rs`):** Truyền `WorkerInvocationRequest.default_locale` và tra `CatalogRegistry::global()` theo thứ tự `Plugin -> Theme -> Core`.
   - **Siết `routes/l10n.rs`, `CoreL10n`, `opendash-core` & `MockHost`:** Dùng `validate_extension_text_domain`, kiểm tra `ensure_no_reparse_or_symlink` + `MAX_SITE_LANGUAGE_FILES` (`128`), kiểm `text_domain` trong `PluginManifest::validate()` & `ThemeV3Manifest::validate()`, hỗ trợ `L10nTranslate`/`L10nTranslateBatch` trong `MockHost`, đặt `DEFAULT_BREADCRUMB_HOME_LABEL = "Home"` + `CoreL10n.home_label`, và siết `validate_l10n_call` cho `__`/`_x`/`_n` trong `ast_validation.rs`.

---

## 3. Giới hạn Ghi nhận

- Lỗi `writer_barrier::audited_writer_inventory_covers_every_mutating_route_handler` (`writer_barrier.rs:260`: `missing handler entry_trash in entries.rs`) là lỗi pre-existing đã tồn tại trên `main` trước phạm vi spec này.
