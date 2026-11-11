# M4-S07-T01 — Responsive homepage, Windows G4 implementation

- **Issue:** [#103](https://github.com/thientran2359/opendash-cms/issues/103), BLD-04 / BLD-04.1, Forge, Full-Track.
- **Approved source:** [ADR-0011](../decisions/ADR-0011-m4-builder-editor-responsive-list.md) and [G3 brief](../plans/milestones/M4/slices/2026-11-11-m4-builder-remaining-g3.md), immutable G3 SHA `690dfa1167f11fba701cf8a0cfa4c36cc5ed29a0`.
- **Implementation baseline:** `origin/main@c42bbc1381d88d2c895eab3653110f4614e424b5`, clean dedicated worktree `task/forge-m4-s07-t01`. Implementation PR records the final head SHA after push.
- **Verdict:** **G4 local implementation and acceptance PASS**; **G5 independent post-merge verification NOT_RUN at this point**. Project Done and issue closure need separate verified merge evidence.

## Implemented and contract mapping

The v1 Builder document now accepts **optional**, per-container, sparsely serialized responsive metadata on Section, Row and Column. Viewports follow desktop ≥1024 CSS px, tablet 768–1023.999… CSS px, and mobile below 768. Each style declares only allowed fields: Section `padding_px`; Row `padding_px`, `gap_px`, `columns_per_row`; Column `padding_px` and `text_align`. Integer validation enforces padding/gap 0–64 and column count 1–6, rejects invalid keys and contexts, and preserves explicit zero. Empty overrides/objects are omitted during serialization. Old v1 documents without responsive metadata continue serializing and rendering without added CSS.

The server renderer generates **one** allowlisted scoped stylesheet for Preview and public from valid node IDs and typed numeric/enum values, with `@media (width < 1024px)` followed by `@media (width < 768px)`. Missing properties inherit individually from wider tiers. Any Row with gap or columns in any tier opts into CSS grid, default one column, with the existing AST child order; Rows without opt-in retain their original rendering. The stylesheet precedes `main` so the previous v1 document DOM hierarchy remains unchanged. No arbitrary CSS or theme code is accepted.

Inspector lets editors select desktop/tablet/mobile, edit allowed fields on the selected container and remove individual overrides. The Canvas uses the same selected viewport cascade and preserves v1 flex appearance for non-grid Rows. The actual opaque sandbox iframe is resized to the selected CSS width (1280 / 834 / 390px); viewport selection is **only UI state** and does not dirty or persist the document. Existing session/ETag, media protection, binding metadata, unknown node anchors and published Save behavior remain unchanged.

**Design Delta, file ownership:** `cms/crates/opendash-server/src/storage/builder.rs` is one additional file beyond the original G3 ownership. The core node structs gained optional fields, so the M3 legacy-to-v1 projection needed exactly three `responsive: None` constructor initializers. No storage SQL, migration, ACL or projection IDs changed. This extra file must be reviewed as part of the implementation PR.

## Executed tests (real Windows desktop)

From task worktree root after `npm ci --prefix cms/admin --no-audit --no-fund`:

| Check | Observed result |
| --- | --- |
| `npm run typecheck --prefix cms/admin` | **PASS**, TypeScript |
| `npm run build --prefix cms/admin` | **PASS**, Vite 6.4.6, 67 transformed modules; embedded Admin rebuilt before Rust |
| `cargo fmt --manifest-path cms/Cargo.toml --all -- --check` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked --target-dir ../opendash-cms-forge-m4-s06-t01/cms/target` | **PASS 88/88**, core 28 + server 60; responsive tests check sparse zero/type/context/limits and legacy v1; doctests 0 |
| `cargo build --manifest-path cms/Cargo.toml --locked --target-dir ../opendash-cms-forge-m4-s06-t01/cms/target` | **PASS**, Windows debug binary, existing unused backend warnings |
| `python cms/scripts/builder_responsive_browser.py --binary ../opendash-cms-forge-m4-s06-t01/cms/target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"` | **PASS**, real Chrome / disposable SQLite: CSS grid 3/2/1 at 767/767.25/768/1023/1023.25/1024, text-align and padding/gap zero, React Canvas, Preview/public CSS equal, no Preview DB writes, Save/reload/restart, binding intact, remove override, invalid 400 with unchanged ETag, pre-edit SQLite backup restored into isolated site |
| `python cms/scripts/builder_homepage_smoke.py --binary ../opendash-cms-forge-m4-s06-t01/cms/target/debug/opendash.exe` | **PASS**, M3 projection, security/ETag/CAS, theme and unresolved historical nodes |
| `python cms/scripts/builder_editor_browser.py --binary ../opendash-cms-forge-m4-s06-t01/cms/target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"` | **PASS**, existing Tree/Canvas/Inspector, 401/412, delayed Save, legacy iframe and protected media regression |
| `python cms/scripts/check-standards.py`, `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test`, `git diff --check` | **PASS**, project architecture, plan consistency and whitespace |

The browser probes actual iframe CSS viewports at the required integer thresholds and fractional quarter-pixel widths. Chromium rounds an iframe's internal layout viewport near half-pixel boundaries; the CSS source uses strict range queries, and no interpretation of the `767.5` nested viewport rounding is used as a product breakpoint. Public HTML and iframe Preview CSS are compared directly and evaluated with computed styles. The online SQLite backup is created **before** responsive Save, checked for integrity after restore and reopened with the new binary as a v1 document; no production site/data was used.

## Explicit limitations and follow-up gates

- No responsive visibility, CSS text editor, grid spans, arbitrary theme layout, publish workflow, or M5 composition was added.
- No new DB migration. Restoration of a pre-edit site uses the saved SQLite snapshot; optional historical M3 binary restore was **NOT_RUN**.
- Real macOS acceptance, complete M4 release parity and user-site deployment are **NOT_RUN** and belong to the separate M4 release tasks.
- To finish task #103: push a pinned implementation PR, ensure required governance CI Ubuntu/Windows PASS at the PR head, self-review and merge according to ADR-0010; independently rerun G5 on **the merged source SHA**, store that evidence via reviewed PR, and only then set Issue/Project Done and synchronize TASK_INDEX.

Agent: **Forge · Chat On Steroids Windows**, 2026-11-12.
