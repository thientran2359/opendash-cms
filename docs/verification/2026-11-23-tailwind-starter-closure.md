# Tailwind Frontend & Starter v3 — Coordinator Verification

- Date: 2026-11-23 (Asia/Ho_Chi_Minh)
- Agent: Aster; native Darwin x86_64.
- Exact implementation source: `091310c2e0d6e74b56be72221a928c3e5b6a0049` on `agent/aster`, safely fast-forwarded from `9f9368c`. Harbor branch untouched.
- Scope: both slices of [approved spec](../designs/tailwind-frontend-and-starter-theme-v3.md); this is not a drift audit of every design in the repository.
- Decision: **Implemented**, not Verified; [drift report](../../.agents/research/2026-11-23_TAILWIND_STARTER_DRIFT_REPORT.md) identifies three P2 discrepancies tracked in [#495](https://github.com/thientran2359/opendash-cms/issues/495).

## Merge and CI evidence

GitHub Connector independently confirmed [PR #493](https://github.com/thientran2359/opendash-cms/pull/493) merged `5baf005eb7d4d89d6538e96dd3dcde22d2725e63` as `a251b3a1a1e0b2c2df0863e1d0c96896b5fb7f70`; [PR #494](https://github.com/thientran2359/opendash-cms/pull/494) merged `7928f89b893dffbc30172b6b06010ab567d474b7` as `091310c2e0d6e74b56be72221a928c3e5b6a0049`, base `a251b3a1a1e0b2c2df0863e1d0c96896b5fb7f70`.

Exact-head workflow queries returned SUCCESS for CMS and Governance: S01 [CMS](https://github.com/thientran2359/opendash-cms/actions/runs/45124668430) / [Governance](https://github.com/thientran2359/opendash-cms/actions/runs/45124668471); S02 [CMS](https://github.com/thientran2359/opendash-cms/actions/runs/45131609154) / [Governance](https://github.com/thientran2359/opendash-cms/actions/runs/45131608995). Job-level follow-up queries also independently confirmed `cms-ci`, `governance-ubuntu-latest` and `governance-windows-latest` all completed SUCCESS for both reviewed heads. Both #491 and #492 were independently fetched CLOSED; GitHub CLI fallback (`gh issue view --json projectItems`) independently confirmed both Project items have status Done; the connector has no Project v2 read tool.

## Native checks

| Command | Result |
| --- | --- |
| `npm --prefix cms/admin run build:public-tailwind` | PASS (exit 0) |
| `npm --prefix cms/admin run typecheck` | PASS (exit 0) |
| `npm --prefix cms/admin run build` | PASS (exit 0) |
| `python3 cms/scripts/check-standards.py` | PASS (exit 0) |
| `python3 cms/scripts/check-standards.py --self-test` | PASS (exit 0) |
| `python3 cms/scripts/check-plans.py` | PASS (exit 0) |
| `python3 cms/scripts/check-plans.py --self-test` | PASS (exit 0) |
| `python3 .agents/scripts/check-agent-artifacts.py` | PASS (exit 0) |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS (exit 0) |
| `cargo build --manifest-path cms/Cargo.toml --locked` | PASS (exit 0) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | PASS (exit 0) |
| `git diff --check` | PASS (exit 0) |

Full release run completed: **78 result groups, 402 passed, 0 failed, 0 ignored**, including `starter_theme_v3_test`. Run used `TMPDIR=/private/tmp` for native macOS path canonicalization. Local raw log SHA-256: `54a9453f5efc5892431fcdc9599f607c30c48015105a702845279de7b9769974`; scratch logs are at `/tmp/aster-tailwind-closure/`. Governance checks were rerun after documentation changes and passed.

Public CSS rebuilt deterministically with no tracked asset diff: 30,724 bytes, gzip 6,411 bytes, below spec limits. The source scan includes Starter, fixture, built-in block, Rust public renderer and safelist candidates.

## Source conformance and limits

S01: verified embedded CSS constant/asset headers, document injection, removed fallback global body width, preview rewrite and five core block source changes. S02: verified embedded file map and manifest counts (9 templates, 3 parts, 3 layouts, 6 patterns, 2 blocks, 5 options, sample 5 entries/2 menus/2 settings), startup lock ordering, repair and activation code, writer inventory, and fixture changes. Three exceptions are in the drift report.

CodeGraph MCP responded but flagged changed files and did not resolve the new Starter symbols from its old index; current source files were read directly. `node cms/scripts/codegraph-index-docs.mjs` was attempted and exited 1: CodeGraph engine not found (`CODEGRAPH_ENGINE_PATH` is not configured). No indexing success is claimed; no dependency was installed.

[Harbor handoff](https://github.com/thientran2359/opendash-cms/issues/492#issuecomment-7094615345) reports native Chrome sample import/publication/Reading Settings/Studio preview PASS and post-merge focused tests. These are **Harbor-reported evidence**, not a browser run by Aster. Independent live-browser repetition, console/CSP inspection and the warm `ensure_default_starter_theme <5 ms` timing measurement are **NOT_RUN** in this audit. Existing compile warnings are not represented as failures when command exit is zero.

## Independent HTTP reproduction

Aster ran a disposable-site probe using the freshly built debug binary at the exact merged source. Setup/login succeeded; `PUT /api/themes/opendash-starter/options` saved `header_cta_url=/aster-configured-target` and `header_cta_label=ASTER_CTA_PROBE` (200), and GET options confirmed both values at version 2. `GET /posts` rendered the new label but its anchor still pointed to `/gioi-thieu`; the configured destination did not appear. This reproduces D1 independently. A second probe created a published Post and a published Page `/aster-news`, assigned that Page as Posts Page through Reading Settings, confirmed `/aster-news` returned 200, and confirmed the Post still rendered its back link to `/posts`; D2 is also independently reproduced. The initial probe against the empty-site `/` did not find the header label; moving the probe to the theme-rendered post archive exposed the intended boundary. The probe did not publish or alter any real site. Owned server process was terminated in `finally`; no pinned site or Harbor worktree was used.

## Typed URL evaluation

Prefer a reusable schema-typed public URL mechanism over whitelisting arbitrary option strings. The delta must choose an exact schema-derived URL expression boundary, reuse existing URL validators, validate defaults/save/import and existing values, define attribute scope, preserve private/untyped rejection, cover preview/Admin parity and handle immutable Starter version upgrades. A narrower core-produced validated URL context remains an alternative for Goldfish comparison. [#495](https://github.com/thientran2359/opendash-cms/issues/495) is a design follow-up, not an approved implementation task. No code or C-SEC change is authorized by this report. Post back-link authority and Banner richtext typography need conformance fixes and regression verification in the same follow-up.

Promotion to Verified requires approved fixes, merged implementation, a clean fresh drift report and all spec verification evidence. Completed slices #491/#492 remain closed; Harbor must not be resumed on those Done issues.
