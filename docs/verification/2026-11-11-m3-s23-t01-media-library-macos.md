# M3-S23-T01 — Media Library UI, macOS independent slice

**Task:** [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36) · **Author:** Harbor (CoS macOS) · **Status:** IMPLEMENTED (isolated component), **G5 PENDING**.

## Contract, source, and ownership

- G3 contract: [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36), accepted task source SHA `f7e2ac9c15b2b6bccfe173ef21101814d5ee57de`.
- Implementation base: `origin/main@31980960a594f436fe379c93ee50947fbeb794b9` (merged G3 registration in PR #37); task worktree branch `task/harbor-m3-s23-t01`.
- Related sibling dependency: [Forge Issue #35](https://github.com/thientran2359/opendash-cms/issues/35) must merge its typed `MediaRoute` seam before Harbor mounts this component into real `/admin/#/media`.
- The scope is restricted to [MediaLibraryPage.tsx](../../cms/admin/src/features/media/MediaLibraryPage.tsx), [media-library.css](../../cms/admin/src/features/media/media-library.css) and new helpers under `cms/admin/src/features/media/library/`. Existing `MediaPicker.tsx`, shared `App.tsx`, `AdminShell.tsx`, `styles.css`, Rust routes/storage and migrations remain unchanged.
- Existing authenticated media endpoints are reused: `GET /api/admin/media?limit=&offset=`, CSRF-protected `POST /api/admin/media`, and authenticated `GET /api/admin/media/{id}/thumbnail` and `/file` (from [media.ts](../../cms/admin/src/api/media.ts)). No public draft media URL, delete, video, or invented server search API.

## Implemented behavior

- Standalone responsive image gallery, keyboard-focusable selection, per-page search explicitly labeled as limited to currently loaded results, 36-item pagination, loading/empty/error states, refresh and detail panel for original preview, file name, format, bytes, dimensions, thumbnail dimensions and upload time.
- Multiple-image upload in sequence with friendly MIME/zero-byte/8 MiB/browser-decode/6000 px/20-million-pixel preflight. Rust validates actual content on receipt. Failed batches report completed count without claiming remaining uploads succeeded; API failures reach shared auth error callback, including 401.
- Component contract: `MediaLibraryPage({ user, csrfToken, onApiError })`, matching the props expected for the future Forge route seam. Authenticated thumbnail/file paths are retained; no `MediaPicker` behavior changes.

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

The temporary browser preview harness was removed before commit. Its mock media IDs were not backed by real image files, so image loads in the harness failed as expected; **actual thumbnails/originals and browser uploads have not been proven through the integrated Admin UI**. The existing backend smoke above validates real authenticated bytes. Browser testing for the full authenticated route, actual image rendering, Admin/Editor, login expiry, upload errors and MediaPicker regression is still required after Forge's PR merges. The screenshots were inspected interactively, not preserved as PR artifacts.

## Pending integration and release gates

1. Wait for Forge Issue #35 to merge its typed `cms/admin/src/shell/MediaRoute.tsx`, fetch the merged `main` and reconcile the seam props. Harbor then owns **only** the route seam file to mount `MediaLibraryPage`.
2. Rebuild embedded Rust Admin from this branch, run actual browser tests with isolated SQLite for Admin/Editor, JPEG/PNG/WebP upload, 0-byte/size/dimension/401 failures, original and thumbnail loading, responsive 390 px and desktop, Content/Posts MediaPicker and Builder regressions. Save screenshots and precise results.
3. Recheck GitHub Ubuntu/Windows CI at the final PR head, mergeability and security gates, and only then perform ADR-0005 merge with the exact expected head SHA. G5 evidence and Project `Done` must be recorded separately; this standalone phase does not qualify.
