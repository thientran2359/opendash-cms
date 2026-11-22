# OpenDash Studio (Customizer + UX Builder) & Unified Comment-Block Entry Content Closure — 2026-11-22

- **Environment:** macOS Intel (Orion Coordinator) + GitHub Actions CI (`OpenDash CMS Build & Test`, `governance-ubuntu-latest`, `governance-windows-latest`)
- **Audited Source:** `main@4309bc946343ac2b088f25476a5d01304156a349`
- **Release Binary SHA-256 (`cms/target/release/opendash`):** `3908128183a62d6e53afe52d1e6a94e4bd97dd44ea1f12bb89e0ca0425678d28`
- **Design Spec:** [OpenDash Studio Customizer & UX Builder Spec](../designs/opendash-studio-customizer-and-ux-builder.md)
- **Drift Audit:** [EGM Drift Report](../../.agents/research/2026-11-22_OPENDASH_STUDIO_DRIFT_REPORT.md) (`egm-codebase-details` `42cb2b2f`, **Zero Blocker / Zero Major Drift**)
- **Related Issues & Merged PRs:**
  - `#458` (Slice S01 — Unified Comment-Block Entry Content & Ref Sync): [PR #463](https://github.com/thientran2359/opendash-cms/pull/463) (`merge=3b891a2c5e53cc63601cdace86d929f7dbda5100`, CI 3/3 `SUCCESS`)
  - `#459` (Slice S02 — Preview Annotations, Studio Targets & Preview API): [PR #464](https://github.com/thientran2359/opendash-cms/pull/464) (`merge=f355d277d8550ec6f79396053f0eff40551e5d90`, CI 3/3 `SUCCESS`)
  - `#460` (Slice S03 — Studio Shell, Customizer & Entry Mode 2 UI): [PR #465](https://github.com/thientran2359/opendash-cms/pull/465) (`merge=8123a55f31fc57d2b7c0a67f62d329b616afcf4b`, CI 3/3 `SUCCESS`)
  - `#461` (Slice S04 — Interactive XSS-Safe Canvas Bridge & Full-Matrix Tests): [PR #466](https://github.com/thientran2359/opendash-cms/pull/466) (`merge=33304485815ed674fc5599f26ae15fc46de4bb81`, CI 3/3 `SUCCESS`)
  - Live Browser E2E Hotfix (React Error #310 hook-ordering fix in `SingleEditWorkspace.tsx` & `<iframe key={...}>` remount in `BuilderPreviewFrame.tsx`): [PR #467](https://github.com/thientran2359/opendash-cms/pull/467) (`merge=4309bc946343ac2b088f25476a5d01304156a349`, CI 3/3 `SUCCESS`)

## Exact-Main Automated Verification (`main@4309bc9`)

| Check | Result | Notes |
| --- | --- | --- |
| `BRIDGE_SCRIPT_SHA256` verification (`node -e`) | **PASS** | Exact SHA-256 digest `UiM/eIdicljiszHJtbLUkiT/TvTl34+g3v9/B6oYKjA=` matches `BuilderPreviewFrame.tsx` |
| `npm --prefix cms/admin run typecheck` | **PASS** | `tsc --noEmit` exit `0` |
| `npm --prefix cms/admin run build` | **PASS** | `tsc && vite build` (`107` modules, `dist/assets/index-BFX4Ckzc.js`) |
| `python3 cms/scripts/check-standards.py` | **PASS** | `82/82` mutation registrations resolved, SQL/core/Admin boundary checks clean |
| `python3 cms/scripts/check-standards.py --self-test` | **PASS** | Negative regression suite PASS |
| `python3 cms/scripts/check-plans.py` | **PASS** | Markdown links, SITEMAP coverage, verified evidence PASS |
| `python3 cms/scripts/check-plans.py --self-test` | **PASS** | `13` fixtures PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** | No retired hooks or secrets |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** | Exit `0` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked -- --test-threads=1` | **PASS** | `76` test suites, `355` passed, `0` failed, `0` ignored (includes `studio_builder_test` 3/3 & `studio_s02_preview_api` 1/1) |
| `git diff --check` | **PASS** | Clean |

*macOS local test environment note:* `TMPDIR=/private/tmp` (avoiding `/var -> /private/var` canonical path mismatch in `SitePaths`) and `--test-threads=1` were used for serial release workspace execution; CI executes full Ubuntu/Windows checks independently on GitHub Actions.

## Exact-Main Live Browser E2E (`http://127.0.0.1:18967/admin/`, `main@4309bc9`)

Executed via isolated Chromium subagent (`510f2c29`, host-verified visited origin `http://127.0.0.1:18967`, `blocked_origins: none`, visual evidence screenshot `final_step_97.png`) against release binary `39081281...` with clean `/private/tmp/orion-studio-e2e-clean` site root:

1. **Setup & Login — PASS:** Created admin (`POST /auth/setup -> 201`) and logged into Dashboard (`200 OK`).
2. **Theme v3 & Media Library — PASS:** Installed & activated `studio-e2e-theme` (`v3`); uploaded `e2e-photo.png` (`ID f14d9f65...`).
3. **Studio Mode 1 (`#/studio`) — PASS:** Customizer edits (`site_title`, `banner_text`) updated preview `srcdoc` transients live before Save and persisted across reload; initialized `part:header` UX Builder override, edited headings/text, and saved (`POST /api/admin/template-overrides -> 201`); verified `single` template locks final `core/content_slot` deletion in both Inspector and Tree (`canDeleteElement`).
4. **Studio Mode 2 (`#/studio/entry/page/1`), `<dialog>` Prompt, Unified `body` Round-Trip & Media ACL — PASS:** Opened `#/pages/new` with **0 React #310 errors**; triggered native unsaved `<dialog>` prompt (`Sửa với UX Builder` while dirty); verified Mode 2 preview renders read-only Header/Footer without `data-od-editable`, blue selection outline on active editable block (verified in `final_step_97.png`), `core/image` private preview resolution (`data:image/png;base64,...`), Classic `<textarea>` `<!-- od:... -->` round-trip edits + publish persistence, draft vs published media ACL (`/media/<id>/file`: `404` when draft -> `200` when published), and clean public HTML (`/content/page/studio-e2e-page` `200 OK` with zero `data-od-editable`).
5. **Custom CPT `landing` with `Builder` Support — PASS:** Created CPT `landing` (`builder` enabled), created entry `#/cpt/landing/new` (0 React #310 errors), edited & persisted in `#/studio/entry/landing/2`.

*Disclosure / Limitations:*
- An initial E2E pass (`8fe5a4a2` on `main@3330448`) exposed `Minified React error #310` in `SingleEditWorkspace.tsx` and stale iframe sandbox mounting; both were resolved in PR #467 (`main@4309bc9`) and fully re-verified in clean run `510f2c29` with zero React/uncaught errors.
- Playwright cross-origin `sandbox="allow-scripts"` locator clicks (`page.mouse.click` over `#canvas`) throw `UNSUPPORTED_IN_CROSS_ORIGIN_FRAME`, while frame locator click on inner elements and `od-highlight` DOM outlines inside `#canvas` functioned normally; programmatic CSP, nonce, `event.source`, `locateNode`, and XSS sanitization are covered by `studio_builder_test.rs`.

## CodeGraph Docs Reindex

- Default `node cms/scripts/codegraph-index-docs.mjs` lookup targets the Linux engine path on macOS (`Không tìm thấy CodeGraph engine...`).
- Executed with pinned temporary Darwin x64 engine `@astudioplus/codegraph-mcp@0.21.0` (`CODEGRAPH_ENGINE_PATH=.../codegraph-server-darwin-x64 node cms/scripts/codegraph-index-docs.mjs`): **PASS** — `Hoàn tất: đã nạp 29 nguồn tài liệu vào CodeGraph docs index.`; temporary `/private/tmp/cg-tmp` removed afterward with zero repo/lockfile edits.

## Closure Outcome

- **Spec Status:** `docs/designs/opendash-studio-customizer-and-ux-builder.md` and `docs/designs/SITEMAP.md` promoted to **`Verified`**.
- **Superseded Note Added:** Recorded `Superseded by docs/designs/opendash-studio-customizer-and-ux-builder.md` for entry dual-store separation in `docs/designs/cpt-content-builder-and-pattern-library.md`.
