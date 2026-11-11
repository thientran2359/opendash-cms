# M3-S22-T01 — Forge Posts/Pages Admin UI, Windows verification

- **Scope:** [Issue #35](https://github.com/thientran2359/opendash-cms/issues/35), M3-P08; frontend Post/Page and MediaRoute integration seam.
- **Contracts:** [ADR-0004](../decisions/ADR-0004-m3-post-page-publication-boundary.md), [ADR-0005](../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md).
- **Branch:** task/forge-m3-s22-t01. **Base:** origin/main SHA 31980960a594f436fe379c93ee50947fbeb794b9.
- **Exact code commit tested:** a9fa2e74d5b135e03a497aff329ba9f1f36fab1e.
- **Host:** Windows 11, PowerShell, Node/npm, locked Rust/Cargo, Python 3.13, installed Chrome and Python Playwright.
- **Isolation:** all HTTP/Chrome checks use disposable SQLite and site roots; no real website data changed.
- **Outcome:** Windows acceptance **PASS** for the scoped Post/Page UI at the exact code commit. GitHub CI/merge and cross-OS verification have separate gates; this report does not claim macOS or whole-M3 G5.

## Delivered behavior

Both Admin routes /admin/#/posts and /admin/#/pages now use a reusable list/editor with create, edit, delete, independent pagination, title, slug, body, featured-image MediaPicker, explicit Save/Publish/Unpublish, status and version display. The new typed API adapter reuses the existing Rust endpoints and the existing MediaPicker without modifications. Save keeps published entries published and creates new entries as private draft; only explicit publication changes status. Update/delete/publication send version-CAS If-Match. Error 412 retains local form data and offers a confirmation before reloading the stored version.

Authenticated persisted preview opens in a new tab because the server CSP disallows embedding. Preview is not a preview of unsaved edits; draft public links are hidden. Tests confirm draft-only public media 404 and authenticated Admin media 200; Publish makes the public single/media readable and Unpublish revokes public access. Session expiry 401 retains same-account unsaved form data, while logging in as a different account clears the previous draft. Both editor views remain mounted between Admin navigation changes.

Forge also added a typed MediaRoute with user, csrfToken and onApiError props, wired through App/AdminShell. It presents a truthful pending Media Library until Harbor [Issue #36](https://github.com/thientran2359/opendash-cms/issues/36) consumes the seam **after Forge merge**. No Harbor-owned media files, Rust routes, SQLite migrations or backend schemas changed.

## Executed Windows checks

All commands in this table were actually run. Paths are relative to the repository root unless a working directory is stated.

| Command | Result |
| --- | --- |
| npm ci (from cms/admin) | PASS; no npm vulnerabilities reported |
| npm run typecheck (from cms/admin) | PASS, repeated on exact code commit |
| npm run build (from cms/admin) | PASS, repeated on exact code commit |
| powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release | PASS: Admin, sample plugin and locked Rust release build |
| cargo build --manifest-path cms/Cargo.toml -p opendash-server --release --locked | PASS; rebuilt embedded Admin after final UI/CSS |
| cargo test --manifest-path cms/Cargo.toml --workspace --release --locked | PASS: 19 core and 40 server unit tests |
| python cms/scripts/content_types_smoke.py --binary cms/target/release/opendash.exe | PASS |
| python cms/scripts/content_entries_smoke.py --binary cms/target/release/opendash.exe | PASS |
| python cms/scripts/media_smoke.py --binary cms/target/release/opendash.exe | PASS |
| python cms/scripts/public_content_smoke.py --binary cms/target/release/opendash.exe | PASS, repeated at code commit |
| python cms/scripts/windows_smoke.py --binary cms/target/release/opendash.exe | PASS |
| python cms/scripts/windows_kernel_smoke.py --binary cms/target/release/opendash.exe | PASS |
| python scripts/windows_auth_smoke.py --binary target/release/opendash.exe (from cms) | PASS, auth/CSRF/Origin/expiry/upgrade |
| python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe (from cms) | PASS, plugin security/lifecycle |
| python cms/admin/src/features/posts/tests/windows_browser_smoke.py | PASS in real Chrome with temporary SQLite; repeated at code commit |
| python cms/scripts/check-standards.py and git diff --check | PASS |

The [Chrome acceptance script](../../cms/admin/src/features/posts/tests/windows_browser_smoke.py) covers Admin/Editor CRUD, featured upload/picker, 22-entry pagination, persisted private preview with CSP/no-store and escaped XSS, draft-only media/public 404, published Save, Publish and Unpublish, optimistic-CAS 412 with preserved form, session 401 same-account recovery, cross-account draft isolation, Builder draft retained across navigation, keyboard-accessible controls, and desktop 1440px/mobile 390px with no horizontal overflow. The mobile assertions also check featured-image card width. The existing public_content_smoke suite independently checks negative CSRF/Origin/If-Match, legacy CPT compatibility, SQL publication visibility and media ACL.

### Browser screenshots (Chrome with disposable data)

- [Desktop Post draft with selected image](../../cms/admin/src/features/posts/tests/screenshots/forge-post-desktop.png)
- [390px Page editor with publication controls](../../cms/admin/src/features/posts/tests/screenshots/forge-pages-mobile-editor.png)
- [390px Page listing after deletion](../../cms/admin/src/features/posts/tests/screenshots/forge-pages-mobile-list.png)

## Known existing script limitations

- **Legacy windows_admin_browser.py**: FAIL at an obsolete dashboard selector expecting the text HTTP: ok. The current merged AdminShell dashboard no longer includes that string. The task-owned Chrome script checks the real current Builder and Content routes, with all new Post/Page flows.
- **Legacy windows_distribution_smoke.py**: FAIL at its historical assertion that the SQLx migration count equals 3, although M3 migrations now extend through 0008; its failure cleanup also hits an open SQLite file handle on Windows. This is outside Issue #35 and not changed here.
- Initial windows_auth_smoke and windows_plugin_smoke invocations from repository root could not find their relative fixture/sample plugin paths. Both **PASS** from their expected cms working directory, as recorded above.

No high-severity security/data-loss finding remains within the scope exercised by these tests. Re-fetch PR head/base, verify both current-head governance CI checks, confirm mergeability and merge only after ADR-0005 gates pass. Issue/Project Done must reflect exact SHA and Windows evidence, not just the presence of a merge commit.
