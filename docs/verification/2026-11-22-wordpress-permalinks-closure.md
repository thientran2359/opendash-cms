# WordPress-Style Permalinks (Phase 1) Closure — 2026-11-22

- **Environment:** macOS Intel (`x86_64`, Orion Coordinator) + GitHub Actions CI (`OpenDash CMS Build & Test`, `governance-ubuntu-latest`, `governance-windows-latest`)
- **Audited Source:** `main@c05287a81a1f3ed6e2e741a355b40b6f84f62143` (contains S01–S03 merges through `c1cd643c7037032a35914e220a6ebd387542f6c8`)
- **Release Binary SHA-256 (`cms/target/release/opendash`):** `620cc3868045d102cb6fc17c9d0da3b7022b8c65f6c2045a6045b6e457b8f833`
- **Design Spec:** [WordPress-Style Permalinks (Phase 1)](../designs/wordpress-style-permalinks.md)
- **Drift Audit:** [EGM Drift Report](../../.agents/research/2026-11-22_WORDPRESS_PERMALINKS_DRIFT_REPORT.md) (`egm-codebase-details` `aa99c02a-e803-4fbf-9432-c2fa62bc1817`, **`0 Blocker · 0 Major`**, 3 non-blocking Minor/closure items)
- **Related Issues & Merged PRs:**
  - `#478` (Slice S01 — Migration `0045`, WordPress Default Permalinks, `%category%`, `DynamicPattern`, Namespace Guards & Trailing Slash `301`): [PR #481](https://github.com/thientran2359/opendash-cms/pull/481) (`merge=9505a2ae3a1ff8833eafd6333c21389f97b4b6e3`, CI 3/3 `SUCCESS`)
  - `#479` (Slice S02 — `primary_category_term_id` Persistence, Auto-Clear & Opt-In `auto_unique_slug`): [PR #483](https://github.com/thientran2359/opendash-cms/pull/483) (`merge=cd5395a064968dc283b18e5b521a7f0640d4b640`, CI 3/3 `SUCCESS`)
  - `#480` (Slice S03 — Admin UI `PermalinkSettingsPage`/`TermPicker`/`SingleEditWorkspace` & `wordpress_permalinks_test.rs`, delivered by Harbor): [PR #484](https://github.com/thientran2359/opendash-cms/pull/484) (`head=3b9149f02a5aaef23d5656434946ca55beb03ad9`, `merge=c1cd643c7037032a35914e220a6ebd387542f6c8`, CI 3/3 `SUCCESS`)

## Exact-Main Automated Verification (`main@c05287a`)

| Check | Result | Notes |
| --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | **PASS** | `tsc --noEmit` exit `0` |
| `npm --prefix cms/admin run build` | **PASS** | `tsc && vite build` (`107` modules, `dist/assets/index-B_DBZ1gN.css`, `dist/assets/index-q6EkVtSE.js`) |
| `cargo build --manifest-path cms/Cargo.toml --release --locked` | **PASS** | Exit `0`, SHA-256 `620cc3868045d102cb6fc17c9d0da3b7022b8c65f6c2045a6045b6e457b8f833` |
| `TMPDIR=... cargo test --manifest-path cms/Cargo.toml --release --locked --test wordpress_permalinks_test` | **PASS** | `4 passed; 0 failed; 0 ignored` (`1.08s`) |
| `TMPDIR=... cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | **PASS** | `77` suites, `399 passed; 0 failed; 0 ignored` (including `wordpress_permalinks_test` `4/4` in `0.55s`) |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** | Verified with closure checks |
| `python3 cms/scripts/check-standards.py` + `--self-test` | **PASS** | Core/storage/route/Admin boundaries & CSRF negative regressions clean |
| `python3 cms/scripts/check-plans.py` + `--self-test` | **PASS** | Markdown links, SITEMAP coverage & 13 self-test fixtures clean |
| `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** | Clean |
| `git diff --check` | **PASS** | Clean |

*Disclosure / Limitations:*
- An initial workspace test run executed immediately alongside test-binary compilation hit the 8-second readiness timeout on `wordpress_permalinks_test::primary_category_drives_public_path_with_fallback_and_renames` (`temporary permalink server was not ready`); the full clean workspace rerun on `main@c05287a` immediately afterward passed all `77` suites (`399 passed; 0 failed; 0 ignored`).
- Canonical macOS `TMPDIR` (`os.path.realpath(tempfile.gettempdir())` under `/private/var/...`) was used so `SitePaths` avoids macOS `/var -> /private/var` symlink mismatch.

## Exact-Main Live Browser E2E & HTTP Header Verification (`http://127.0.0.1:18977`, `main@c05287a`)

Executed against release binary `620cc386...` on an isolated temporary site root and ephemeral localhost port `18977` (without touching port `18964`), verified via Chromium browser subagent (`cf3256ed-f673-4cbd-b5ac-0a2fcd275936`, host-verified origin `http://127.0.0.1:18977`, `blocked_origins: none`, final screenshot `final_step_55.png`) and direct `curl -sSI`:

1. **Setup & Login — PASS:** Initial Admin setup (`POST /auth/setup -> 201 Created`) and login (`POST /auth/login -> 200 OK`).
2. **`Cài đặt → Đường dẫn tĩnh` (`PermalinkSettingsPage`) — PASS:** Verified default `/%postname%`, Page/Pattern description, `"Mặc định hệ thống"` CPT/Taxonomy labels (no `"legacy"`), `%category%` token chip, switching to `"Tùy chỉnh"` from `/%category%/%postname%` retaining the value in `#permalink-post-custom`, saving `/archives/%post_id%` and `/%postname%`, plus real server-saved preview updates.
3. **`409` Conflict List & Rollback — PASS:** Published Page `Dup Holder` (`/blog/dup-target`) + Post `Dup Post` (`dup-target`), then attempted saving `/blog/%postname%` -> `PUT /api/admin/settings/permalinks` returned `409 Conflict`, `[data-testid="permalink-conflicts"]` rendered the conflicting paths, and saved settings remained `/%postname%`.
4. **Primary Category & Opt-In `auto_unique_slug` (`-2`) — PASS:** Created categories `Alpha` (`alpha`) and `Beta` (`beta`), published Post `Primary Post` with `[data-testid="set-primary-term-2"]` (`Beta` -> `Chính`, `aria-pressed="true"`, checkbox kept checked) -> `/beta/primary-post` returned `200`. Switched primary category to `Alpha` -> `/alpha/primary-post` returned `200`, and `/beta/primary-post` returned `HTTP/1.1 301 Moved Permanently` (`location: /alpha/primary-post`). Created a second Post with duplicate slug `primary-post` in Admin Single Edit -> saved with `primary-post-2` and served `/alpha/primary-post-2` (`HTTP/1.1 200 OK`).
5. **Trailing Slash `301`, Draft Isolation & Security — PASS:**
   - `curl -sSI http://127.0.0.1:18977/alpha/primary-post/` -> `HTTP/1.1 301 Moved Permanently`, `location: /alpha/primary-post` (verified in browser too).
   - `curl -sSI http://127.0.0.1:18977/nhap/` & `/alpha/nhap/` (unpublished draft `Draft Hidden`) -> `HTTP/1.1 404 Not Found` (no redirect or draft title leak).
   - `curl --path-as-is -sSI http://127.0.0.1:18977//evil.com/` -> `HTTP/1.1 400 Bad Request`.

## CodeGraph Docs Reindex

- Default `node cms/scripts/codegraph-index-docs.mjs` on macOS Intel requires `CODEGRAPH_ENGINE_PATH` (linux binary path is the default fallback).
- Executed with pinned temporary Darwin x64 engine `@astudioplus/codegraph-mcp@0.21.0`: **PASS** — indexed all `31` active doc sources (`--list-sources` 31/31 `OK`, including `docs/designs/SITEMAP.md`, `docs/designs/wordpress-style-permalinks.md`, and `docs/contracts/CONTENT_FOUNDATION_2026_11.md`); temporary `/private/tmp/cg-tmp` removed afterward with zero repo/lockfile edits.

## Closure Outcome

- **Spec Status:** `docs/designs/wordpress-style-permalinks.md` and `docs/designs/SITEMAP.md` promoted to **`Verified`**.
- **Superseded Note Added:** Recorded `Superseded by docs/designs/wordpress-style-permalinks.md` at [`docs/contracts/CONTENT_FOUNDATION_2026_11.md`](../contracts/CONTENT_FOUNDATION_2026_11.md) §4 for legacy `/content/{type}/{slug}` & `/taxonomy/{kind}/{slug}` defaults, `"legacy"` label, and old `DynamicPattern` rules.
