# M3-S23-T01 — Media Library UI, macOS implementation and integration

**Task:** [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36) · **Author:** Harbor (CoS macOS) · **Status:** IMPLEMENTED and locally verified on macOS; **G5 pending exact-head CI, merge and Project closeout**.

## Contract, source, and ownership

- G3 contract: [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36), accepted task source SHA `f7e2ac9c15b2b6bccfe173ef21101814d5ee57de`.
- Implementation base: `origin/main@31980960a594f436fe379c93ee50947fbeb794b9` (merged G3 registration in PR #37); task worktree branch `task/harbor-m3-s23-t01`.
- The original standalone phase followed parallel ownership while Forge's [Issue #35](https://github.com/thientran2359/opendash-cms/issues/35) remained open. Forge's typed `MediaRoute` seam merged in [PR #39](https://github.com/thientran2359/opendash-cms/pull/39), satisfying the handoff dependency. Integration fast-forward base: `origin/main@ddab2515fc87c8a5b8145ad66c67afdb43e950e1` (including PRs #38, #39 and #40).
- Integration changes exactly [MediaRoute.tsx](../../cms/admin/src/shell/MediaRoute.tsx) to mount [MediaLibraryPage.tsx](../../cms/admin/src/features/media/MediaLibraryPage.tsx). Original isolated files remain under [media-library.css](../../cms/admin/src/features/media/media-library.css) and `cms/admin/src/features/media/library/`. Existing `MediaPicker.tsx`, shared `App.tsx`, `AdminShell.tsx`, `styles.css`, Rust routes/storage and migrations remain unchanged.
- Existing authenticated media endpoints are reused: `GET /api/admin/media?limit=&offset=`, CSRF-protected `POST /api/admin/media`, and authenticated `GET /api/admin/media/{id}/thumbnail` and `/file` (from [media.ts](../../cms/admin/src/api/media.ts)). No public draft media URL, delete, video, or invented server search API.

## Implemented behavior

- Standalone responsive image gallery, keyboard-focusable selection, per-page search explicitly labeled as limited to currently loaded results, 36-item pagination, loading/empty/error states, refresh and detail panel for original preview, file name, format, bytes, dimensions, thumbnail dimensions and upload time.
- Multiple-image upload in sequence with friendly MIME/zero-byte/8 MiB/browser-decode/6000 px/20-million-pixel preflight. Rust validates actual content on receipt. Failed batches report completed count without claiming remaining uploads succeeded; API failures reach shared auth error callback, including 401.
- Component contract: `MediaLibraryPage({ user, csrfToken, onApiError })`, forwarded by the merged Forge route seam. Authenticated thumbnail/file paths are retained; no `MediaPicker` behavior changes.

## Verification on Intel macOS, 2026-11-11

| Check performed | Result and bounds |
| --- | --- |
| `export PATH="$HOME/.nvm/versions/node/v24.22.2/bin:$HOME/.cargo/bin:$PATH"` and `node --version` | PASS — Node `v24.22.2`. Bare `npm` initially unavailable in CoS shell PATH (exit 127); resolved by explicit PATH. |
| `cd cms/admin && npm ci` | PASS — 83 packages installed, npm audit reported zero vulnerabilities; install-script advisory for esbuild/fsevents was shown. |
| `cd cms/admin && npm run typecheck` | PASS — `tsc --noEmit` against task source. |
| `cd cms/admin && npm run build` | PASS — `tsc && vite build`, Vite v6.4.6. Feature has no live route import until the Forge seam merges, so this is type/build validation and **not** an integrated UI build proof. |
| `python3 cms/scripts/media_smoke.py --binary ../../opendash-workspace/opendash-cms-mac-cos/cms/target/debug/opendash` | PASS — existing macOS debug binary from Harbor main workspace, with temporary site/SQLite; covers upload, type/size/CSRF/Origin, Admin/Editor, 401, private thumbnail/file, references and restart. This tests current backend behavior; no new Rust binary was built in this worktree. |
| Isolated React preview in local Chrome, mocked 42 metadata rows, viewport 390 px | PASS — 36 cards on page 1, search `anh-thu-vien-036` matched exactly one card, page 2 had six and disabled Next; no horizontal document overflow (390 px viewport, `scrollWidth=375`). |
| Isolated React preview in local Chrome, viewport 1440 px | PASS — `scrollWidth=1425`, gallery/detail columns `1057 px / 310 px`, select an image shows original Admin endpoint and metadata. |

The temporary browser preview harness was removed before the initial commit. Its mock media IDs were not backed by actual images; the tests above describe **only the initial independent phase**. Integrated behavior with real authenticated media endpoints was subsequently verified as recorded below.

## Integrated Rust and real browser verification, Intel macOS 2026-11-11

The integration was tested from the fast-forwarded task worktree at `ddab2515fc87c8a5b8145ad66c67afdb43e950e1` plus the one-file `MediaRoute.tsx` diff. A fresh temporary site/SQLite under `/tmp/opendash-harbor-s23-*/` ran on loopback port 18853 and was used for all browser mutations; this did not access the user's live site database. The binary was rebuilt **after** compiling Admin with the integration:

| Exact check / command | Observed result |
| --- | --- |
| `cd cms/admin && npm ci && npm run typecheck && npm run build` (Node v24.22.2 PATH) | **PASS** — 83 packages; TypeScript passed; Vite v6.4.6 compiled 54 modules with the real `MediaRoute` import. |
| `CARGO_TARGET_DIR="$HOME/Local Sites/opendash-workspace/opendash-cms-mac-cos/cms/target" cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** — embedded Admin built from this worktree's `cms/admin/dist`, reusing existing macOS Cargo target caches. |
| `python3 cms/scripts/media_smoke.py --binary "$HOME/Local Sites/opendash-workspace/opendash-cms-mac-cos/cms/target/debug/opendash"` | **PASS** — real isolated SQLite upload, raster formats, wrong MIME, 0-byte, >8 MiB, width 6001 px, auth/CSRF/Origin, private images and restart. |
| `python3 cms/scripts/content_entries_smoke.py --binary "$HOME/Local Sites/opendash-workspace/opendash-cms-mac-cos/cms/target/debug/opendash"` | **PASS** — entries/fields/versioning/restart/upgrade regression. |
| `python3 cms/scripts/public_content_smoke.py --binary "$HOME/Local Sites/opendash-workspace/opendash-cms-mac-cos/cms/target/debug/opendash"` | **PASS** — CPT public rendering, template/XSS/media ACL and pagination regression. |
| Plugin Playwright Chrome, real `http://127.0.0.1:18853/admin/#/media` | **PASS** — Admin login, empty state, JPEG/PNG/WebP upload in a batch; real API returned 3 items and private `/file` + `/thumbnail` both returned HTTP 200 for all formats; images in browser had non-zero natural width; `private, no-store` and `nosniff` observed. |
| Upload negative cases in authenticated Admin UI | **PASS** — SVG MIME, zero-byte PNG, 8 MiB+1 byte file and 6001 px wide PNG each displayed a specific error; original 3 assets remained. |
| Role and media picker regression | **PASS** — created an Editor in the temporary site via Admin UI; Editor opened Media route and uploaded another PNG, chose an image as a Post featured image via existing MediaPicker, navigated to Builder/back and retained the unsaved selected image, then selected an image via the existing Content MediaPicker. All selected images used authenticated thumbnails. |
| Pagination with 44 actual images | **PASS** — a browser-driven upload of another 40 distinct PNGs succeeded; page 1 showed 36, page 2 showed 8, Next disabled on last page. Filename search filtered only loaded items and the explicit current-page limitation label remained visible. |
| Authentication expiration | **PASS** — set `sessions.expires_at=0` in the **temporary** SQLite database only, clicked Media “Tải lại”; UI displayed login and session-expired message and hid authenticated library. |
| Responsive and keyboard-addressable controls | **PASS (observed)** — 390 px viewport produced 375 px document/client width, two image columns, no horizontal overflow; 1440 px viewport produced 1425 px width with separate gallery/detail columns; native buttons/inputs/links support keyboard focus. Full keyboard-only walkthrough/screen-reader audit was not performed. |

Three real Admin screenshots were captured from the isolated site and stored with the media feature:

- [Admin desktop, real WebP original detail](../../cms/admin/src/features/media/library/tests/screenshots/harbor-media-admin-desktop.png)
- [Editor mobile, page 2 at 390 px](../../cms/admin/src/features/media/library/tests/screenshots/harbor-media-admin-mobile-390.png)
- [Editor desktop, real pagination page 2](../../cms/admin/src/features/media/library/tests/screenshots/harbor-media-admin-pagination-desktop.png)

Browser fixture uploads used Canvas-created JPEG/PNG/WebP `File` objects dispatched through the real React upload input; actual bytes were accepted by the real Rust server. Browser navigation/selection used Playwright clicks. The plugin session reported three browser console errors across the full session, including expected unauthorized requests; a dedicated page-exception audit was not run. All test media and users were confined to the temporary SQLite/site.

## Pending release gates

1. Commit the final integration + evidence, open a follow-up PR toward `main` for Issue #36 and verify GitHub Ubuntu/Windows governance checks on the exact PR head, mergeability and no security/data blockers.
2. Merge with expected head SHA only after all required checks PASS under ADR-0005. Report merged SHA and G5 evidence to Issue #36 and update Project `Done` only when actual acceptance is complete. Coordinate a separate `TASK_INDEX.md` status sync with a single assigned writer.
