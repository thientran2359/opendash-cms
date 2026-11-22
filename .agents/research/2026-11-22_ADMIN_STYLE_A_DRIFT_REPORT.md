# EGM Drift Report — Admin Design System (Style A — Shadcn + Linear) & UI Polish

- **Date:** 2026-11-22
- **Auditor:** Nova
- **Target Spec:** `docs/designs/admin-design-system-and-ui-polish.md`
- **Audited Source:** `origin/main@9f9368c` (including PRs #441, #442, #444, #445, #446, #448, #452)

## Summary

**Verdict: NO UNINTENDED DRIFT — READY FOR `Verified`**

All goals and slices specified in `docs/designs/admin-design-system-and-ui-polish.md` are implemented and present on `main`:

1. **Design Tokens & CSS Layering (`cms/admin/src/styles.css`, `cms/admin/src/features/media/media-library.css`):**
   - `@layer theme, base, legacy, components, utilities;` synchronized across both CSS files.
   - Tailwind v4 `@theme` neutral light tokens (`canvas`, `surface`, `sidebar`, `border`, `text`, `muted`, `accent`, `danger`, `success`) active.
   - Legacy global tag selectors isolated strictly under `.od-legacy-scope` (remaining only on Visual Builder, Patterns, Template Library, and Single Edit inner legacy field wrappers).
2. **Reusable Primitives (`cms/admin/src/components/AdminPrimitives.tsx`):**
   - `AdminButton` (`primary | secondary | ghost | danger`, `sm | md`), `AdminBadge`, `AdminAlert` adopted across all core Admin screens (Waves 1–6: Shell, Auth, Dashboard, Listing, Media, Settings, Users, Tools, Menus, Taxonomy, Plugins, Profile, Themes, Theme Options, Schema CPT forms).
3. **Single `<main>` Invariant & Accessibility:**
   - Single `<main>` landmark preserved across Login, Dashboard, Listing, Media, Patterns, and Template Library.
4. **Owner-Approved Post-Wave-6 Enhancements (PR #452, `3e1dae2`):**
   - Sidebar IA reorganized (`NỘI DUNG` with top-level `Liên hệ` + per-CPT taxonomy submenus; `GIAO DIỆN` with `Visual Builder`, `Theme`, `Tùy chọn theme`, `Patterns`; `HỆ THỐNG` with consolidated `Cấu trúc nội dung`).
   - WordPress-style Single Edit (inline Permalink, Publish box with editable `created_at` / `updated_at`, clean Featured Image card enabled by default for all non-private CPTs, per-taxonomy sidebar cards, Page Attributes Template).
   - Listing table polish (Featured Image column, inline `#ID` after Title, row actions `Chỉnh sửa · Xem · Xóa` under Title, clickable taxonomy filters, aligned filter labels) and unified multi-file Media upload.
   - All enhancements were explicitly requested and approved by the Owner in live review on 2026-11-21.
