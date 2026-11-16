# Báo cáo nghiệm thu E2E Windows — `M7-S13-T01` (Admin Navigation & Single Edit)

- **Issue:** [#284](https://github.com/thientran2359/opendash-cms/issues/284)
- **Milestone / Slice:** `M7` / `M7-S13-T01` (`FND-05.5`)
- **Base SHA:** `4fd21b5`
- **Người thực hiện & Self-review:** Nova (`ADR-0010`)
- **Môi trường & Artifact thực chạy:**
  - OS: Windows (`agent/nova`)
  - Release binary: `cms/target/release/opendash.exe`
  - Binary SHA-256: `b84e0afc8c6941e190dd9ba30a3e68e70182b6d3a20854b0bbaf3a324b739042`
  - Trình duyệt: `157.0.8103.50` (Playwright headless)
  - Ảnh chụp màn hình: `runtime/m7-s13-t01-screenshots/` (`01`..`06`, desktop `1440x900` & mobile `390x844`)

## 1. Kết quả kiểm chứng theo Ma trận Nghiệm thu (`AC-01`–`AC-11`)

| Nhóm tiêu chí | Bằng chứng thực chạy | Trạng thái |
| --- | --- | --- |
| `AC-01`, `AC-02`, `AC-03` | `m7_s09_t01_schema_field_groups` + `m7_s10_t01_panel_registry_and_layout` (workspace tests) | **PASS** |
| `AC-04` (`base_field_panels`, auto-append, mobile DOM order) | `m7_s10_t01_panel_registry_and_layout` + `admin_nav_single_edit_e2e.py` (mobile `390px` kiểm tra thứ tự DOM `header_bar → main_top → main_primary → main_bottom → sidebar`) | **PASS** |
| `AC-05`, `AC-06` (`public_exposure`, `ScopedFieldGrant`) | `m7_s09_t02_exposure_and_plugin_grants` | **PASS** |
| `AC-07`, `AC-08` (`CompositeEntrySaveInput`, `PatchField`, Pha 3 fence) | `m7_s11_t01_composite_save_and_profile` + thao tác Lưu composite trên Chrome thật (`#/cpt/project/new` → `#/cpt/project/1/edit`) | **PASS** |
| `AC-09` (0 CPT, menu CPT động, RBAC `403`, `/users/profile`) | `m7_s12_t01_navigation_and_schema_ui` + `admin_nav_single_edit_e2e.py` (`01`..`06`) | **PASS** |
| `AC-10`, `AC-11` (Action Panel tách Lưu, Quarantine `EditorDraftStore` & hạ quyền) | `m7_s12_t02_single_edit_and_draft_recovery` | **PASS** |
| Toàn vẹn SQLite (`PRAGMA integrity_check`, `PRAGMA foreign_key_check`) | Chạy trực tiếp trên DB sinh bởi `opendash.exe` release | **PASS** (`ok`, `[]`) |

## 2. Lệnh kiểm chứng thực chạy

1. `cargo build --manifest-path cms/Cargo.toml --release --locked` — **PASS**
2. `python cms/scripts/admin_nav_single_edit_e2e.py` — **PASS**
3. `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test`, `python cms/scripts/check-standards.py`, `git diff --check` — **PASS**
