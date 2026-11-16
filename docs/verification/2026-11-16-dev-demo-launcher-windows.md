# Windows dev launcher: isolated Mộc Studio demo by default (2026-11-16)

## Scope and behavior

The explicit owner request is for `python dev.py` to show the existing Mộc Studio
demo website rather than the unrelated/empty site database in the checkout.
This change is limited to `dev.py`, the existing demo seed runner, launcher
regression tests and documentation. It does not alter production site startup,
Rust storage, fixtures, or migrations.

- Default development site root: `runtime/demo-website/dev-local/` (Git-ignored).
- First launch: build the current React Admin and Rust debug binary, seed the
  existing demo fixture into a uniquely named staging directory, then rename
  that directory to `dev-local` only when no destination exists.
- Later launches: reuse existing demo database and changes without reseeding.
- Explicit `--site-root`, `--no-demo`, `OPENDASH_SITE_ROOT`, or
  `OPENDASH_DB_PATH` preserves the preexisting site's selection and suppresses
  automatic seeding. The old checkout-root site is not deleted or modified.
- The seeding HTTP server may use a temporary port. Persist the actual intended
  launcher `--port` for public settings and absolute demo navigation links.
- On Windows, port takeover verifies that an existing OpenDash process uses the
  **same checkout's debug binary**. A different agent worktree's process is
  rejected rather than terminated.

## Observed validation (Forge Windows)

| Check | Result |
| --- | --- |
| `python -m unittest discover -s cms/scripts -p test_dev_launcher.py -v` | **PASS**, 12 tests: default/override selection, first/repeat seed, partial site preservation, failed seed cleanup, port ownership, source watch, stop |
| `python -m py_compile dev.py cms/scripts/demo-website-runner.py cms/scripts/test_dev_launcher.py` | **PASS** |
| `npm run build` in `cms/admin/` | **PASS** (`tsc`, Vite 6.4.6) |
| `cargo build --locked -p opendash-server` in `cms/` | **PASS**; warnings about unused code and future-incompatible dependency remain unrelated |
| Real Mộc Studio seed by `DevRunner.ensure_demo_site()` | **PASS**, created `runtime/demo-website/dev-local/seed-meta.json` and SQLite; reused existing without overwrite |
| SQLite read-only inspection | **PASS**: 30 `content_entries`, 13 `media_assets`, 4 `content_types`, 2 `users`, 1 `plugins` |
| Real binary from new debug build against seeded site | **PASS**: `GET /` 200 with Mộc Studio title; `GET /content/post` 200; `GET /admin/` 200; authenticated dashboard and content-types API 200 |
| Direct `DevRunner.launch_server()` on an ephemeral port | **PASS**: `GET /` and `GET /api/health` 200; spawned server terminated by launcher |
| Windows current port 18766 owner verification | **PASS**: existing `opendash.exe` from `opendash-cms-win-codex` is **not** the Forge debug binary; not stopped or altered |
| `git diff --check` | **PASS** |

## Workspace preservation

The separate historical preview-site and visual folders are not Git worktrees
and have no relevant active process. Three such directories were moved (not
deleted) under sibling `_workspace-archive/` with a restore manifest; each
folder's file count and total bytes were verified after the move. Other agent
checkouts, detached review clones, SQLite data with uncommitted work, and
forensic/safety backups were left unchanged. The archive itself is local and
not part of this Git change.

## Limitations

- The current `python dev.py` process in the **separate Codex checkout** runs its
  old launcher until that agent updates its working tree and restarts it. This
  verification does not stop that process or change its site data.
- Changing a seeded site's launcher port later does not automatically rewrite
  user-edited public-site settings or menu links. The initial seed records the
  chosen port. A new site can be created only through an explicit backup/reset.
- Demo credentials are intentionally test-only and the server is loopback-bound;
  do not use the demo site as a production install or commit its SQLite/media.
