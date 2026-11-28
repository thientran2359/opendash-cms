# EGM Drift Report — Unified Field Engine (`unified-field-engine`, Đợt 1) (2026-11-27)

- **Design Spec:** [`docs/designs/unified-field-engine.md`](../../docs/designs/unified-field-engine.md)
- **Target Scope:** S01–S04 triển khai trên `main` (`PR #622` `ecbea04`, vá vòng 2 `PR #623` `0c8f1b7`, vá vòng 3 `PR #624` `46226b5`)
- **Final Goldfish Verdict:** **`PASS — Ready for Verified`** (`0 Blocker / 0 Major / 0 Minor / 1 Nit` sau 4 vòng kiểm toán fresh-context độc lập)

---

## 1. Fresh-Context Goldfish Rounds Summary

| Round | Subagent ID | Scope Audited | Verdict (Blocker / Major / Minor / Nit) | Key Outcome |
|---|---|---|---|---|
| **Round 1** | `19663c41` (`egm-codebase-details`) | Toàn bộ Spec S01–S04, [`UNIFIED_FIELD_SCHEMA.md`](../../docs/contracts/UNIFIED_FIELD_SCHEMA.md), `0055`–`0056`, `opendash-core::field_schema`, `field_hydrate.rs`, `template_sec`, `content_render`, `storage/*`, `cms/admin/src/fields/`, `opendash-starter` `1.0.0` | `NEEDS FIXES` (mục `F-01`–`F-20`) | `PR #622` (`ecbea04`) giải quyết loạt đầu (`F-02`, `F-04`, `F-05`, `F-07`–`F-10`, `F-13`–`F-16`, `F-18`) cùng `html_scan_state.rs` (HTML5 tag-open) & `theme-moc-studio/page.html`. |
| **Round 2** | `df358887` (`egm-codebase-details`) | Kiểm tra lại sau `PR #622` trên mã thực tế | `2 Blocker / 3 Major / 2 Minor / 2 Nit` | Xác nhận 12/20 mục R1 `RESOLVED`; phát hiện 2 Blocker (`R2-01` nested Group/Repeater ACL leak, `R2-02` Single Edit Featured Image picker `'media'` vs `'image'`) cùng `F-01`, `F-06`, `F-11`, `F-12`, `F-17`, `F-19`, `F-20`. Toàn bộ được xử lý ở [`PR #623`](https://github.com/thientran2359/opendash-cms/pull/623) (`0c8f1b7`). |
| **Round 3** | `88f15a54` (`egm-codebase-details`) | Xác minh 9 mục của R2 trên `main@0c8f1b7` (`ecbea04..0c8f1b7`) | `0 Blocker / 2 Major / 2 Minor / 1 Nit` | Xác nhận cả 2 Blocker (`R2-01`, `R2-02`) và toàn bộ 3 Major R2 (`F-01`, `F-06`, `F-11`) đã `RESOLVED`; phát hiện 2 Major hồi quy hợp đồng TS/Rust từ `F-17`/`F-20` (`R3-01` creator `entry_ref` `['*']`, `R3-02` margin mặc định `-128` vs `-256` & container margin `-128`) cùng `R3-03`..`R3-05`. Toàn bộ được vá sạch ở [`PR #624`](https://github.com/thientran2359/opendash-cms/pull/624) (`46226b5`). |
| **Round 4** | `4a70055c` (`egm-codebase-details`) | Kiểm tra dứt điểm diff `0c8f1b7..46226b5` (`PR #624`) | **`0 Blocker / 0 Major / 0 Minor / 1 Nit`** | Xác nhận **5/5** mục R3 (`R3-01`..`R3-05`) đạt **`RESOLVED`** với `0` hồi quy; còn 1 Nit UX copy không chặn (`R4-01`: chuỗi `"1 to 100 options"` khi `radio_buttons > 12`, chủ động giữ ổn định catalog `.pot`/`.po`). |

---

## 2. Remediations Verified Across `PR #623` & `PR #624`

1. **`R2-01` (Blocker — Nested Group/Repeater ACL Isolation):**
   - Đưa `HydrationVisibility` (`All`, `OptionsPublicOnly`, `ContentPublic { max_view_role }`) kèm `child_visibility()` (`OptionsPublicOnly -> All`, `ContentPublic -> ContentPublic`) vào [`cms/crates/opendash-core/src/field_schema/spec/types.rs`](../../cms/crates/opendash-core/src/field_schema/spec/types.rs).
   - Đồng bộ lọc đệ quy qua [`cms/crates/opendash-server/src/field_hydrate.rs`](../../cms/crates/opendash-server/src/field_hydrate.rs), allowlist C-SEC [`cms/crates/opendash-server/src/template_sec/policy/types.rs`](../../cms/crates/opendash-server/src/template_sec/policy/types.rs), [`cms/crates/opendash-server/src/content_render/entry_view.rs`](../../cms/crates/opendash-server/src/content_render/entry_view.rs) (cả nhánh hydrated và `entry_view_with_url` qua `R3-05`), và toàn bộ caller công khai.
2. **`R2-02` (Blocker — Featured Image Picker Mismatch):**
   - Bổ sung `matchesMediaPickerKind` (`'image' -> 'media'`, `'gallery' -> 'gallery'`) và kiểm tra `featured_image` trong `supportAllowsField` tại [`cms/admin/src/features/editor/editorFieldValidation.ts`](../../cms/admin/src/features/editor/editorFieldValidation.ts) & [`cms/admin/src/features/editor/SingleEditWorkspace.tsx`](../../cms/admin/src/features/editor/SingleEditWorkspace.tsx).
3. **`F-01`, `F-06`, `F-11`, `R3-03` (Majors — Indexed Media ACL, UI-only `base_fields`, CPT Target Overrides):**
   - `media_is_referenced.rs`: lọc provider `active`/`declared`, schema hợp lệ, top-level & nested `Active` fields với `collect_public_media_ids(&specs, &values, false)` đồng bộ 100% với `EntryView` và [`docs/designs/unified-field-engine.md`](../../docs/designs/unified-field-engine.md).
   - [`cms/admin/src/features/editor/PanelRegionRenderer.tsx`](../../cms/admin/src/features/editor/PanelRegionRenderer.tsx): giữ nguyên các trường UI-only (`tab`, `accordion`, `message`) trong `core:base_fields`.
   - [`cms/crates/opendash-server/src/storage/overrides/resolve_2axis/resolve_template_2axis_on_connection.rs`](../../cms/crates/opendash-server/src/storage/overrides/resolve_2axis/resolve_template_2axis_on_connection.rs): định tuyến `validate_cpt_target_fields_in_tx` qua `load_effective_content_schema_in_tx` kèm lọc `Active`, non-UI, `PublicExposure::Public`, `min_view_role <= Editor`.
4. **`F-12`, `F-17`/`R3-02`, `F-19`, `F-20`/`R3-01`/`R3-04` (Term Edit, Spacing Bounds, Site Timezone & Creator Fields):**
   - [`cms/admin/src/features/taxonomy/TermEditPage.tsx`](../../cms/admin/src/features/taxonomy/TermEditPage.tsx): lọc bỏ `isUiOnlyFieldType` & `read_only_keys` khi rebase và submit.
   - [`cms/admin/src/fields/helpers.ts`](../../cms/admin/src/fields/helpers.ts), [`SpacingControl.tsx`](../../cms/admin/src/fields/controls/SpacingControl.tsx), [`validation.ts`](../../cms/admin/src/builder/document/validation.ts), [`BuilderInspector.tsx`](../../cms/admin/src/builder/BuilderInspector.tsx), và [`test-field-engine.mjs`](../../cms/admin/scripts/test-field-engine.mjs): giữ biên explicit `min = -256` trong khi khôi phục mặc định khi thiếu `min` về `-128` và giới hạn container margin `-128..=256`.
   - [`cms/admin/src/studio/StudioCustomizerNav.tsx`](../../cms/admin/src/studio/StudioCustomizerNav.tsx): truyền `siteTimezone={generalDraft?.timezone}` vào Global Site Options.
   - [`cms/admin/src/features/content/types.ts`](../../cms/admin/src/features/content/types.ts), [`ContentTypeForm.tsx`](../../cms/admin/src/features/content/ContentTypeForm.tsx), `FieldGroupsManager.tsx`: mở đủ 21 kiểu Content, mặc định `entry_ref` sang `['post']` (`R3-01`), và siết `parseCreatorFieldOptions` khớp `radio_buttons` tối đa 12 lựa chọn & `value <= 64` ký tự (`R3-04`).
5. **Tracked Non-Blocking Nit (`R4-01`):**
   - `ContentTypeForm.tsx` / `FieldGroupsManager.tsx` dùng chung chuỗi `"Field \"{{value1}}\" requires 1 to 100 options."` khi `radio_buttons > 12` (mặc dù logic chặn đúng tại 12); theo dõi để cập nhật cùng đợt đổi chuỗi i18n tiếp theo, không ảnh hưởng an toàn dữ liệu hay runtime.
