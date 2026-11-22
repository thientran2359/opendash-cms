# EGM Drift Report — OpenDash Studio Customizer & Unified UX Builder

- **Date:** 2026-11-22
- **Auditor:** Orion (Coordinator) + clean-context subagent (`egm-codebase-details`, `42cb2b2f`)
- **Target Spec:** [`docs/designs/opendash-studio-customizer-and-ux-builder.md`](../../docs/designs/opendash-studio-customizer-and-ux-builder.md)
- **Audited Source:** `main@4309bc946343ac2b088f25476a5d01304156a349` (PRs `#463`, `#464`, `#465`, `#466`, `#467`)
- **GoldfishContext:** [`PRODUCT_REQUIREMENTS.md`](../../docs/product/PRODUCT_REQUIREMENTS.md), [`CORE_FOUNDATION.md`](../../docs/contracts/CORE_FOUNDATION.md), [`PLUGIN_ABI.md`](../../docs/contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](../../docs/contracts/M8_WRITER_INVENTORY.md)

## Summary

**Verdict: ZERO BLOCKER / ZERO MAJOR DRIFT — READY FOR `Verified`**

All functional requirements, schema/storage consolidation rules, RBAC guards, CAS concurrency checks, interactive XSS/CSP preview sandbox constraints, and layered architecture boundaries specified in `docs/designs/opendash-studio-customizer-and-ux-builder.md` (Slices `S01`–`S04`) match `main@4309bc946343ac2b088f25476a5d01304156a349`.

## Verified Implementation Matrix (`S01`–`S04`)

1. **S01 — Unified Comment Blocks, CPT `builder` Support, Ref Sync & Pattern vs. Section CPT:**
   - `cms/crates/opendash-core/src/builder_blocks.rs`: `MAX_BUILDER_BLOCK_BODY_BYTES` (`384 KiB`), `MAX_TEXT_NODE_CHARS` (`16,384`), `contains_blocks`, `encode_comment_json` (escaping `<`, `>`, `--`), `serialize_builder_document_to_blocks` (emitting `{id, responsive}` containers without `<div class="od-section">`, self-closing `od:text` fallback), `parse_blocks_to_builder_document`, `parse_blocks_lenient_or_recover` (newline preservation without `.trim()`, chunking freeform/malformed content into `core/text`), `extract_visible_text`, and `extract_block_refs_lenient`.
   - `cms/crates/opendash-core/src/content.rs`: rejects `"section"` definition, auto-injects synthetic Core `RichText` `"body"` (`Editor`/`Editor`/`Public`) when `!is_builtin && supports(Builder)`, extends `RichText` limit to `384 KiB` when `contains_blocks`, emits `core:body_editor` for `Editor || Builder`. `ContentMode` remains retained as a background preference while removed from render/ACL/UI gating per Spec §1/§3.
   - `cms/crates/opendash-server/src/storage/entries.rs`, `storage/builder.rs`, `storage/public_content.rs`, `storage/media_refs.rs`, `content_render.rs`, `routes/entries.rs`, and `cms/migrations/0044_pattern_cpt_and_refs.sql`: `"section"` returns `403`, `pattern` retains `entry_builder_documents`, normal CPTs unify on `values_json["body"]` across UI, REST, import, and Wasmi `content_save`, synced via `sync_entry_body_refs_in_conn`, and `media_is_referenced` supports direct refs + 1-hop Pattern ACL without `entry_builder_documents` or `content_mode = 'builder'` gating.
2. **S02 — Preview Annotations, Studio Targets & Full-Page Studio Preview API:**
   - `cms/crates/opendash-server/src/page.rs`: `RenderAnnotationScope { None, TemplateAll, EntryBodyOnly }`, emitting `data-od-editable="1" data-od-kind="..."` strictly for active editable preview scopes and isolating `PatternRef`, `TemplatePart`, `ContentSlot`, and public HTML.
   - `cms/crates/opendash-server/src/storage/overrides.rs`, `routes/themes.rs`, `routes/builder.rs`, `app.rs`: `GET /api/admin/studio/targets` (`Admin`), `POST /api/admin/studio/preview` (`Admin` + CSRF, read-only **without** `AdmittedMutation`, 512 KiB `DefaultBodyLimit`, `#[serde(deny_unknown_fields)]`), transient overrides (`general`, `theme_options` with `/api/admin/media/{id}/file` in preview, `menu_slots`), and full-page Header/Footer shell wrapping.
3. **S03 — OpenDash Studio UI, Mode 1 Customizer + Template Builder & Mode 2 Entry UI (+ PR #467):**
   - `cms/admin/src/shell/router.ts`, `AdminShell.tsx`, `App.tsx`, `studio/StudioWorkspace.tsx`, `StudioTopBar.tsx`, `StudioCustomizerNav.tsx`, `features/themes/ThemeOptionsForm.tsx`, `builder/{document,BuilderInspector,BuilderWorkspace}.tsx`, `features/editor/{SingleEditWorkspace,PanelRegionRenderer,EditorDraftStore}.ts`:
   - Route guards (`#/studio` Admin-only; `#/studio/entry/:typeKey/:id` Admin/Editor, rejecting `pattern`/`section`), chrome hiding + `.od-legacy-scope` isolation, 340px drill-down customizer with 250ms debounced transient preview & selective domain saves (`401`/`412` draft retention), `canDeleteElement` lock on final `core/content_slot`, unified `body_editor` `<textarea>` + `Sửa với UX Builder` `<dialog>` unsaved prompt, and hook-ordering fix (PR #467 in `SingleEditWorkspace.tsx`).
4. **S04 — Interactive XSS-Safe Canvas Bridge & Full-Matrix Tests (+ PR #467):**
   - `cms/admin/src/builder/BuilderPreviewFrame.tsx`: `sandbox={interactive ? 'allow-scripts' : ''}` (never `allow-same-origin`), `key={interactive ? 'interactive' : 'static'}`, `<script>`/conflicting `<meta>` stripping, private media data-URL inlining, `/theme/` & `/media/` origin rewrite, SHA-256 CSP (`'sha256-UiM/eIdicljiszHJtbLUkiT/TvTl34+g3v9/B6oYKjA='`), and `postMessage` verification (`event.source === targetWindow`, `nonce`, `locate(doc, nodeId)`).
   - `cms/crates/opendash-server/tests/studio_builder_test.rs`: 3 end-to-end integration tests covering CPT Builder round-trip, Wasmi plugin writer ref sync, 1-hop Pattern ACL, RBAC `Editor` `403`, zero `data-od-*` public HTML, and XSS/CSP hardening.
5. **Architectural Boundaries:**
   - `opendash-core` (`Cargo.toml`) uses only `serde`, `serde_json`, `url` (no SQL/HTTP/I/O).
   - All SQL (`sqlx::query*`) stays inside `cms/crates/opendash-server/src/storage/`.
   - All Admin API calls flow through `cms/admin/src/api/client.ts`.

## Non-Blocking Notes (Minor / Nit Only)

1. **Naming delta (Minor/Nit):** Shared ref-sync helper in `storage/entries.rs` is named `sync_entry_body_refs_in_conn` writing ref tables directly inside the entry transaction rather than `sync_entry_body_refs_in_tx` delegating to `storage::builder::replace_entry_builder_refs`; semantics (including `previous_body` deleted-pattern grace) are identical.
2. **Preview wrapper form flag (Nit):** `wrap_entry_body_in_preview_shell_in_tx` (`storage/overrides.rs`) does not OR header/footer `has_active_contact_form` into `RenderedLayout`, unlike `preview_studio`. Safely mitigated by `form-action 'none'` CSP and disabled `<fieldset>` in preview.
3. **Retained legacy routes/components (Nit):** `routes::entries::entry_publication` (superseded by `routes::content::content_entry_publication`), `BuilderWorkspace.tsx` unreachable non-pattern `scopedEntry` confirmation guard, and legacy homepage endpoints (`/registry`, `/page`, `/preview`, `/save`) remain dormant with zero runtime interference.
