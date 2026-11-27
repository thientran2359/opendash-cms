# Demo Website Fixture — Mộc Studio (`demo-website`)

## Overview

Isolated demo website fixture (**“Mộc Studio — Thiết kế không gian sống”**) driven by [demo-website-runner.py](../../scripts/demo-website-runner.py) and the root launcher `dev.py`. It provisions a full public + admin showcase site inside gitignored `runtime/demo-website/<run-name>/` without modifying root `database/`, `themes/`, `plugins/`, or `uploads/`.

Directory layout and reused assets:
- [screen-inventory.md](screen-inventory.md): Screen checklist covering public viewports (`PUB-01`–`PUB-14` across `1440x900`, `768x1024`, `390x844`), Admin/RBAC screens (`ADM-01`–`ADM-13`), and error routes (`ERR-01`–`ERR-02`).
- `theme-moc-studio/`: Self-contained Theme Format v3 package (`theme_id: "moc-studio"`, `manifest_version: 3`, `version: "1.0.0"`) with `theme.json`, `header.html`, `footer.html`, `home.html`, `page.html`, `archive.html`, `single.html`, `taxonomy.html`, and `assets/style.css`.
- Synthesized media (`12` `1200x800` PNGs generated locally via `[cms/scripts/demo-website-runner.py:make_arch_png]`), MP4 fixture [video-avc-aac.mp4](../m7/video-avc-aac.mp4), and [sample-plugin](../../examples/plugins/sample-plugin/README.md) (`sample-plugin.zip`).

## Interface

- **Runner Subcommands ([demo-website-runner.py](../../scripts/demo-website-runner.py)):**
  - `seed --run-name <name>`: Creates `runtime/demo-website/<name>/`, initializes `admin_moc` / `editor_moc`, uploads zipped `theme-moc-studio/` via `[cms/scripts/demo-website-runner.py:package_theme_zip]`, installs `sample-plugin.zip`, generates 12 PNGs + uploads `video-avc-aac.mp4`, seeds CPTs (`dich_vu`, `du_an`), posts, pages, taxonomies, menus, and builder homepage.
  - `smoke --run-name <name>`: Runs HTTP + Playwright functional, RBAC, and backup/restore checks against the seeded site.
  - `capture --run-name <name> [--out-dir <dir>]`: Captures all PNGs listed in [screen-inventory.md](screen-inventory.md).
  - `all --run-name <name>`: Runs `seed` -> `smoke` -> `capture`.
  - `serve --run-name <name> --port <port>` and `reset --run-name <name>`.
- **Root Dev Launcher (`dev.py`):** `python dev.py` uses `runtime/demo-website/dev-local/` by default (`--no-demo` switches back to root site data; demo credentials: `admin_moc` / `MocStudioDemo#2026!`, `editor_moc` / `MocEditorDemo#2026!`).

## Invariants

- **Site Isolation:** Every runner invocation sets `OPENDASH_SITE_ROOT` to `runtime/demo-website/<run-name>/` and never writes to root `database/`, `themes/`, `plugins/`, or `uploads/`.
- **Zero External Media Dependencies:** All 12 architectural PNGs are synthesized deterministically with Python stdlib (`zlib` + `struct`), and video/plugin fixtures come solely from repo-owned paths.
- **Reproducibility & Secret Hygiene:** Changes to seeded fixtures or [screen-inventory.md](screen-inventory.md) must pass two clean runs with distinct `--run-name` values; `runtime/demo-website/`, `.sqlite`, `.odbackup.age`, passwords, and session cookies must never be committed.

## Side Effects

- `seed`, `all`, and `dev.py` create/populate `runtime/demo-website/<run-name>/` (SQLite DB, uploads, theme/plugin revisions, backups), and `capture` writes PNG screenshots to `screenshots/demo-website/<run-name>/` (both gitignored).
- `reset --run-name <name>` removes `runtime/demo-website/<run-name>/`.

## Verification

Build the binary and run the end-to-end isolated demo pipeline:

```powershell
cargo build --manifest-path cms/Cargo.toml --locked
python cms/scripts/demo-website-runner.py all --run-name v1-moc-studio-win
```

Verify `dev.py` launcher integration via `cms/scripts/test_dev_launcher.py`.
