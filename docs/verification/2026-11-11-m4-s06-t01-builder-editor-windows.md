# M4-S06-T01 — Homepage Tree / Canvas / Inspector (Windows G4)

- **Task:** [Issue #102](https://github.com/thientran2359/opendash-cms/issues/102), BLD-03 / BLD-03.1, Forge (CoS Windows).
- **Approved contract:** [ADR-0011](../decisions/ADR-0011-m4-builder-editor-responsive-list.md) and [G3 slice](../plans/milestones/M4/slices/2026-11-11-m4-builder-remaining-g3.md), immutable source/base SHA 690dfa1167f11fba701cf8a0cfa4c36cc5ed29a0.
- **Implementation worktree:** task/forge-m4-s06-t01, created from that exact origin/main commit. Final implementation/CI SHA is recorded in the task PR after push.
- **Status:** G4 local **PASS**; G5 on merged release SHA **NOT_RUN**. Issue and Project Done require separate G5.

## Behavior implemented

Admin #/builder now uses the AST v1 homepage endpoints: authenticated GET with version, POST Preview with CSRF and no DB writes, PUT Save using strong If-Match vN. React owns one draft, selected node ID, revision and server version. Tree selects/focuses nodes via click and arrows, adds the permitted Section → Row → Column → Element hierarchy, moves/deletes nodes with confirmation and keeps IDs stable. The controlled Canvas mirrors selected IDs and escapes core text via React DOM. The Inspector edits typed core props and available legacy field values, with the existing MediaPicker for Image. Unknown/historically unsupported nodes remain visible and immutable at their original index and ancestor path unless the user confirms deletion. Existing binding metadata is preserved when editing static fields.

Preview uses server HTML in an opaque iframe with sandbox="" and no same-origin, script or top-navigation access. Server-authorized images cannot normally load inside an opaque iframe because its origin does not send the parent session cookie. The parent UI therefore fetches only exact /api/admin/media/{id}/file raster URLs using its authenticated session and embeds PNG/JPEG/WebP bytes as data URLs in sandboxed Preview. It limits each preview to 64 unique private images / 48 MiB; skipped images are reported and never made public. No new endpoint, credential exposure, public ACL or migration is introduced.

### Design Delta: shared Admin binary HTTP helper

Approved G3 file ownership lists builder/API call sites, but not the shared client implementation. Real Chrome discovered that rendering private images in opaque sandbox requires an authenticated binary GET in the parent. Repository standards prohibit direct HTTP calls from React components or feature API files. Therefore this slice additionally adds **only** a generic requestBlob helper in cms/admin/src/api/client.ts, reusing the existing centralized credentials, error handling and AbortSignal policy. Builder API exposes a narrow getPrivatePreviewMedia wrapper. The original media route and permission model stay unchanged. This one-file ownership extension is explicitly identified for final code review; responsive, binding and list ownership are unaffected.

PUT snapshots and revision IDs keep edits made during an in-flight Save dirty. A stale Preview is aborted/discarded after editing or changing user. A 412 leaves the local draft intact, and reloading the server version requires confirmation before discarding dirty input. A 401 retains the current draft for the same user; deliberate logout or a different account clears it. The Admin before-unload guard protects dirty drafts. Responsive configuration, Field Binding editor, list block and persistence-across-page-reload scopes belong to later M4 tasks.

## Evidence executed on Windows

Commands from repository root, using **disposable SQLite sites**, compiled local binary and locally installed Chrome:

| Check | Observed result |
| --- | --- |
| npm ci --prefix cms/admin --no-audit --no-fund | **PASS** |
| npm run typecheck --prefix cms/admin | **PASS**, TypeScript 5.6.3 |
| npm run build --prefix cms/admin | **PASS**, Vite 6.4.6, 67 modules, before Rust embed |
| cargo build --manifest-path cms/Cargo.toml --locked | **PASS**, native Windows opendash.exe; four existing dead-code warnings in unrelated backend modules |
| python cms/scripts/builder_homepage_smoke.py --binary cms/target/debug/opendash.exe | **PASS** Open, Preview read-only, Save, 401/403/412, legacy projection/theme, image ACL, concurrent writes, restart and unresolved preservation |
| python cms/scripts/builder_editor_browser.py --binary cms/target/debug/opendash.exe --browser "C:\Program Files\Google\Chrome\Application\chrome.exe" | **PASS** real Chrome: Tree/Canvas node ID and keyboard focus; Inspector/MediaPicker; reorder stable across Save/reload; 401 same/different account; 412; snapshot Save; stale Preview; opaque iframe script/top navigation blocked; private image actually loads as PNG data URL; public media remains 404 before Save; unknown node anchor and explicit deletion |
| git diff --check | **PASS** (line endings normalized by Git on Windows) |

The browser acceptance script creates fresh sites, installs a real theme fixture and media asset, inspects ETag/version and public URL, and runs without access to the user's production database. Its server fixture runner explicitly waits for process exit before removing SQLite files on Windows; the earlier cleanup failure was corrected and the final browser run exited **0**. Another browser finding (opaque-sandbox images displayed blank even though the Preview URL was correct) was reproduced with naturalWidth=0, fixed in parent Preview handling, then verified with actual naturalWidth=2 and unchanged public 404.

## Scope and remaining gates

- **PASS G4 implementation tests** on this Windows checkout. ETag, session, authenticated media, legacy compatibility, client retention and sandbox boundaries have actual assertions.
- **NOT_RUN:** production sites, deployment, official same-SHA macOS/Windows release checks, M3 binary rollback fixture (optional --m3-binary argument). No schema or database migration changed in this slice. Rollback is code/binary plus the standard pre-change site snapshot for any existing migrations.
- **Still required before G5/Done:** final PR review and required CI at pinned head SHA, merge, then separate verification on merged release SHA. This evidence does not label the whole M4 milestone VERIFIED.

Agent: Forge · Role: Implementer / self-review under ADR-0010.
