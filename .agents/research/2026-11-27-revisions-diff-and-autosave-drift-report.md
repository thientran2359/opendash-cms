# EGM Drift Report — Entry Revisions, Diff & Server Autosave (`revisions-diff-and-autosave`, 2026-11-27)

- **Design Spec:** [`docs/designs/revisions-diff-and-autosave.md`](../../docs/designs/revisions-diff-and-autosave.md)
- **Target Branch / Worktree:** `feat/revisions-autosave` (`opendash-revisions-autosave`)
- **Final Goldfish Verdict:** **`PASS — Ready for Feature PR`** (0 Blocker / 0 Major sau vá hồi quy & test thực tế)

---

## 1. Compact Goldfish Audit Summary

| Check | Subagent ID | Scope Audited | Initial Verdict | Final Status after Fix |
|---|---|---|---|---|
| **Compact Drift Audit** | `9596a715` (`egm-codebase-details`) | S01–S04 code, [`0059_entry_revisions_and_autosaves.sql`](../../cms/migrations/0059_entry_revisions_and_autosaves.sql) (nối tiếp `0058_responsive_media_and_replace.sql` khi đồng bộ `origin/main@4bfbeae`), `opendash-core::revisions`, `opendash-server` (`routes/revisions/`, `storage/revisions/`, `app_origin.rs`, `i18n.rs`), `cms/admin/` (`features/revisions/`, `SingleEdit*`, `Studio*`), [`M8_WRITER_INVENTORY.md`](../../docs/contracts/M8_WRITER_INVENTORY.md), tests & scripts | **NEEDS FIXES** (`0 Blocker / 1 Major / 1 Minor / 2 Nit`) | **PASS** (Major đã vá + thêm regression test; Minor/Nits đã xác nhận & ghi nhận) |

---

## 2. Findings & Remediations

1. **Major — False-positive `has_hidden_admin_fields = true` trên CPT không có trường `"body"` (`RESOLVED`):**
   - **Vị trí:** [`cms/crates/opendash-server/src/storage/revisions/autosave.rs#L96-L100`](../../cms/crates/opendash-server/src/storage/revisions/autosave.rs#L96-L100) (`get_entry_autosave`).
   - **Nguyên nhân:** `can_view_body` trả về `false` khi custom CPT không định nghĩa `"body"` trong `effective_fields`, khiến nhánh `if !body_viewable && type_key != "pattern" { has_hidden_admin_fields = true; }` bật cờ sai cho cả Admin/Editor dù CPT không có trường Admin-only nào. Trong khi đó `has_hidden_fields_in_map(&live.effective, &live.payload.values, caller_role)` đã kiểm tra toàn bộ `effective_fields` bao gồm `"body"` nếu tồn tại.
   - **Khắc phục:** Gỡ bỏ nhánh dư `if !body_viewable && type_key != "pattern"` trong `get_entry_autosave` và bổ sung regression test cho CPT `headline_note` (`supports: []`, không có `"body"`) trong [`cms/crates/opendash-server/tests/content/revisions_preflight_api.rs`](../../cms/crates/opendash-server/tests/content/revisions_preflight_api.rs) xác nhận `has_hidden_admin_fields == false` cho cả Admin và Editor.

2. **Minor — Feature-first submodule decomposition so với tên tệp dự kiến trong Spec (Accepted Intentional Decomposition):**
   - Để tuân thủ trần `<= 600 LOC` và tránh `part_*`:
     - Rust server tách `storage/revisions/` thành 15 submodule chuyên trách + `mod.rs` (tối đa `sanitize.rs` `591 LOC`), tách `i18n_revisions.rs`, và chia integration test thành `revisions_storage.rs`, `revisions_crud_api.rs`, `revisions_preflight_api.rs`, `revisions_builder.rs`.
     - Admin UI tách thêm [`PublishBoxPanel.tsx`](../../cms/admin/src/features/editor/PublishBoxPanel.tsx) (`290 LOC`), [`SingleEditTranslationModal.tsx`](../../cms/admin/src/features/editor/SingleEditTranslationModal.tsx) (`115 LOC`), [`SingleEditTranslationsPanel.tsx`](../../cms/admin/src/features/editor/SingleEditTranslationsPanel.tsx), và [`useStudioMode2Actions.ts`](../../cms/admin/src/features/revisions/useStudioMode2Actions.ts) (`482 LOC`) từ `useStudioMode2Revisions.ts` (`436 LOC`).
   - Toàn bộ file nguồn mới `.rs`/`.ts`/`.tsx` đều `<= 600 LOC`, không phát sinh thêm file vào `cms/scripts/loc-baseline.json`, và `check-standards.py` PASS 100%.

3. **Nits (Documented):**
   - [`docs/designs/SITEMAP.md`](../../docs/designs/SITEMAP.md) giữ trạng thái `Approved` trong Feature PR và chỉ nâng lên `Verified` kèm [`docs/verification/`](../../docs/verification/README.md) ở Bước 7 sau khi Owner chốt merge (theo Quy trình 7 bước mới).
   - [`StudioMode2RevisionsOverlay.tsx#L97-L152`](../../cms/admin/src/features/revisions/StudioMode2RevisionsOverlay.tsx#L97-L152): khi autosave chỉ đến từ Single Edit (`builder_document === null`), giao diện không hiện nút khôi phục layout mà chỉ cung cấp `Open in Editor` / `Compare with current` / `Discard autosave` để giữ an toàn dữ liệu.
