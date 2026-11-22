# Declarative `block.json` Block Engine & OpenDash Studio Schema-Driven Blocks Closure — 2026-11-22

- **Environment:** macOS Intel (`x86_64`, Orion Coordinator) + GitHub Actions CI (`OpenDash CMS Build & Test`, `governance-ubuntu-latest`, `governance-windows-latest`)
- **Audited Source:** `main@5a94b6549d6f9372745d27787dfd86b1d8d32909`
- **Release Binary SHA-256 (`cms/target/release/opendash`):** `30ba142cd84ec8823d3feca8e512eff7a1a178b057fe16d66ae58745f5970a48`
- **Design Spec:** [Declarative `block.json` Block Engine & OpenDash Studio Schema-Driven Blocks](../designs/declarative-block-json-engine.md)
- **Drift Audit:** [EGM Drift Report](../../.agents/research/2026-11-22_DECLARATIVE_BLOCK_ENGINE_DRIFT_REPORT.md) (`egm-codebase-details` `13da9925-689b-4942-8019-9af6f3d8e4bf`, **`0 Blocker · 0 Major`**, 3 non-blocking Minor/Nit notes)
- **Related Issues & Merged PRs:**
  - `#472` (Slice S01 — `opendash-core` `BlockSchemaV1`, Reserved Grammar, Pure-Rust RichText & `validate_known`, delivered by Nova): [PR #477](https://github.com/thientran2359/opendash-cms/pull/477) (`head=ca7c031e89a036ddb2313b5083cfde0a601ab344`, `merge=9c860ffcda5fe8b4a53a9e224ef54413d456644f`, CI 3/3 `SUCCESS`)
  - `#473` (Slice S02 — Migration `0046`, C-SEC Joint Checker, Scoped CSS, `RenderBudget` & 5 Core Blocks, delivered by Nova): [PR #482](https://github.com/thientran2359/opendash-cms/pull/482) (`head=75f898e4c84b4fa9e3ea04757c5c2c121702f205`, `merge=b9d95892692c1ffeec8d1b26daabf06b34371491`, CI 3/3 `SUCCESS`)
  - `#474` (Slice S03 — Package Auto-Discovery `blocks/<slug>/`, Media ACL Sync/Resync & `GET /api/admin/builder/blocks`, delivered by Forge): [PR #485](https://github.com/thientran2359/opendash-cms/pull/485) (`head=3d15db7ee2089fa59f719f4cd4b5574a7b437798`, `merge=c05287a81a1f3ed6e2e741a355b40b6f84f62143`, CI 3/3 `SUCCESS`)
  - `#475` (Slice S04 — Studio `<BlockSchemaForm>`, CSS Data-URI Preview, `MediaPicker 'any'` & Official Naming Cutover, delivered by Harbor): [PR #488](https://github.com/thientran2359/opendash-cms/pull/488) (`head=07554fe4fd42e07378a7fe7da0945efe35b0fba0`, `merge=5a94b6549d6f9372745d27787dfd86b1d8d32909`, CI 3/3 `SUCCESS`)

## Exact-Main Automated Verification (`main@5a94b65`)

| Check | Result | Notes |
| --- | --- | --- |
| `npm --prefix cms/admin run build` (includes `tsc`) | **PASS** | `tsc && vite build` (`108` modules, `dist/assets/index-Duukf6us.css`, `dist/assets/index-CVcVShs0.js`) |
| `npm --prefix cms/admin run typecheck` | **PASS** | `tsc --noEmit` exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --release --locked` | **PASS** | Exit `0`, SHA-256 `30ba142cd84ec8823d3feca8e512eff7a1a178b057fe16d66ae58745f5970a48` |
| `TMPDIR=... cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | **PASS** | `77` suites, `399 passed; 0 failed; 0 ignored` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` + `--self-test` | **PASS** | `82/82` mutation handlers resolved, core/storage/route/Admin boundaries clean |
| `python3 cms/scripts/check-plans.py` + `--self-test` | **PASS** | Markdown links, SITEMAP coverage & 13 self-test fixtures clean |
| `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** | Clean |
| `git diff --check` | **PASS** | Clean |

## Exact-Main Live Browser E2E (`http://127.0.0.1:18766`, `main@5a94b65`)

Executed against release binary `30ba142c...` on `http://127.0.0.1:18766` via Chromium browser subagent (`c7f45751-3d23-4500-8b5c-4352b1d82e4e`, host-verified origin `http://127.0.0.1:18766`, `blocked_origins: none`, final screenshot `final_step_53.png`) and direct `curl -sSI`:

1. **Setup & Login — PASS:** Initial Admin setup (`201 Created`) and login into Dashboard (`200 OK`).
2. **Official Naming Cutover (`"OpenDash Studio"` / `"Trình dựng trang"`) — PASS:** Verified sidebar navigation displays `OpenDash Studio`, Single Edit button `[data-testid="open-ux-builder-toolbar"]` displays `Sửa với Trình dựng trang`, and `0` occurrences of `"UX Builder"` remain in rendered UI.
3. **5 Core Blocks & `<BlockSchemaForm>` (`#/studio/entry/page/1`) — PASS:**
   - Verified `Element mới` palette groups all 5 core blocks by category (`core/divider`, `core/spacer`, `core/banner`, `core/accordion`, `core/card`).
   - Inserted `core/banner`: edited `title` (`Hero Banner S04`), `richtext` description, responsive `min_height` across `Desktop` (`480px`) vs `Mobile` (`240px`) generating `@media (width < 768px)` with `--od-banner-h:480px` / `--od-banner-h:240px`, and verified `show_if` toggling CTA fields.
   - Inserted `core/accordion`: tested `repeater` controls (`+ Thêm mục`, `Nhân bản`, `Lên`, `Xuống`, `Xóa`) to configure 3 FAQ items; verified Live Preview inside `#canvas` and saved draft (`200 OK`, `0` console errors).
4. **Media ACL (`draft` `404` → `published` `200`, CSS `data:` URI preview & delete-pin `409`) — PASS:**
   - Uploaded 1×1 PNG `s04-banner.png` (`eacb1a012fc0820bb358ea06380857fd97d62a420932142014ac89bcc4afbbc3`, `201 Created`), assigned to `core/banner` background on draft Page `block-engine-test`, and saved draft:
     - Studio Live Preview displayed the background image via `data:image/png;base64,...` CSS rewrite in `<style data-od-block-styles>`.
     - Public `GET /media/eacb1a01.../file` returned **`404 Not Found`** while Page remained `draft`.
     - `DELETE /api/admin/media/eacb1a01...` returned **`409 Conflict`** (`total_pins: 1`, `source_kind: "content_entry"`).
   - Published Page `block-engine-test` (`v4`):
     - `curl -sSI http://127.0.0.1:18766/media/eacb1a012fc0820bb358ea06380857fd97d62a420932142014ac89bcc4afbbc3/file` -> **`HTTP/1.1 200 OK`** (`content-type: image/png`).
     - `curl -sSI http://127.0.0.1:18766/block-engine-test` -> **`HTTP/1.1 200 OK`**, rendering `core/banner` (`<style data-od-block-styles>` with scoped `--od-banner-bg` and responsive `--od-banner-h`) and interactive `<details>` items in `core/accordion`.

*Observations / Notes:*
- When a newly added `repeater` item in `core/accordion` still has empty required sub-fields (`title` / `body`) before user typing completes, transient preview requests (`POST /api/admin/entries/page/1/builder/preview`) return `400 Bad Request` as expected by server validation; once required text is entered, preview and save return `200 OK`.
- `core/banner` and `core/card` declare `media_kind: "image"`, so their `MediaPicker` opens filtered to images, whereas fields declared with `media_kind: "any"` and the main Media Library (`#/media`) expose the `Tất cả` / `Ảnh` / `Video` tabs.

## CodeGraph Docs Reindex

- Executed with pinned temporary Darwin x64 engine `@astudioplus/codegraph-mcp@0.21.0` (`CODEGRAPH_ENGINE_PATH=.../codegraph-server-darwin-x64 node cms/scripts/codegraph-index-docs.mjs`): **PASS** — indexed all `31` active doc sources; temporary `/private/tmp/cg-tmp` removed afterward with zero repo/lockfile edits.

## Closure Outcome

- **Spec Status:** `docs/designs/declarative-block-json-engine.md` and `docs/designs/SITEMAP.md` promoted to **`Verified`**.
