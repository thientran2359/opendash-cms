# M4-S07-T01 — G5 Windows responsive homepage

**Verdict:** **G5 VERIFIED** for [Issue #103](https://github.com/thientran2359/opendash-cms/issues/103) on the exact merged source. This verifies responsive Builder functionality on a disposable SQLite website under Windows and real Chrome. Full M4 release parity and production deployment have separate gates.

## Pinned source, binary and environment

- **G3/contract:** [ADR-0011](../decisions/ADR-0011-m4-builder-editor-responsive-list.md), [G3 brief](../plans/milestones/M4/slices/2026-11-11-m4-builder-remaining-g3.md), G3 source SHA `690dfa1167f11fba701cf8a0cfa4c36cc5ed29a0`.
- **Implementation:** [PR #119](https://github.com/thientran2359/opendash-cms/pull/119), head `38774a7981a5eed4f1b4d4bd9fa65696b93f87bb`, baseline `c42bbc1381d88d2c895eab3653110f4614e424b5`, merged into `main@1f66afd12b21f9017f47e46906bd7edc3619aa08`.
- **G5:** separate clean worktree initially detached at **`1f66afd12b21f9017f47e46906bd7edc3619aa08`**. Both `git rev-parse HEAD` and `origin/main` matched at test time. No product code was changed during G5.
- **Environment:** native Windows, Rust 1.99.1, Node v22.16.0, Python 3.13.4, Vite 6.4.6, installed Google Chrome through Playwright.
- **Embedded Admin Windows binary:** built from the exact merged checkout, SHA-256 `4d71d5ec4b2c62c89950b1c7fae9a137b8cdbc6cbb2b28481ec51dbbed09c2fa`. Reused a task-owned Cargo target cache path; Rust recompiled the source from the fresh G5 checkout before smoke tests.

## Executed G5 verification

All commands ran from the G5 worktree root. Cargo target and executable lived at `../opendash-cms-forge-m4-s06-t01/cms/target`; no active website data was read or written.

| Actual command | Verified outcome |
| --- | --- |
| `npm ci --prefix cms/admin --no-audit --no-fund` | **PASS**, 83 packages |
| `npm run typecheck --prefix cms/admin` | **PASS**, TypeScript |
| `npm run build --prefix cms/admin` | **PASS**, 67 Vite modules; Admin embedded before Rust build |
| `cargo fmt --manifest-path cms/Cargo.toml --all -- --check` | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked --target-dir ../opendash-cms-forge-m4-s06-t01/cms/target` | **PASS**, native Windows executable; pre-existing unused code warnings |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked --target-dir ../opendash-cms-forge-m4-s06-t01/cms/target` | **PASS 88/88**, 28 core + 60 server, doctests 0 |
| `python cms/scripts/builder_responsive_browser.py --binary ../opendash-cms-forge-m4-s06-t01/cms/target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"` | **PASS**: 767/767.25/768/1023/1023.25/1024 CSS-pixel breakpoint probes, grid 3/2/1, inherited properties and explicit 0, Inspector override removal, Preview/public identical CSS, real sandbox iframe widths and controlled Canvas, binding preserved, malformed payload 400 with unchanged ETag, Save/reopen/restart, online SQLite pre-edit snapshot restored on isolated site |
| `python cms/scripts/builder_homepage_smoke.py --binary ../opendash-cms-forge-m4-s06-t01/cms/target/debug/opendash.exe` | **PASS**, legacy M3, GET/Preview read-only, 409/412/CAS, theme changes, security, unresolved payload |
| `python cms/scripts/builder_editor_browser.py --binary ../opendash-cms-forge-m4-s06-t01/cms/target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"` | **PASS**, P03 Tree/Canvas keyboard and ID, Inspector and MediaPicker, 401/412, stale Preview, opaque sandbox, private media, unknown node |
| `python cms/scripts/check-standards.py`, `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test`, `python .agents/scripts/check-agent-artifacts.py` | **PASS**, source boundaries, document links, 12 planning checker fixtures and agent artifacts |
| `git status --short --branch` on the merged checkout | **PASS**, clean exact source during verification |

**GitHub CI:** implementation head `38774a7` [OpenDash Governance run #43073359535](https://github.com/thientran2359/opendash-cms/actions/runs/43073359535) finished **success** for both Ubuntu and Windows. Governance jobs do not substitute for the native Rust/Chrome tests above.

## Scope and safeguards

The new responsive object remains optional in Builder v1, no new SQLite migration was introduced, and existing documents without overrides retain their original rendering. Strict CSS allowlist prevents dynamic style strings, DOM selectors use validated node IDs, and the Preview sandbox does not receive scripts or same-origin access. Existing Field Binding and historical unresolved nodes remain intact. The G3 ownership Design Delta adding three `responsive: None` legacy constructor fields in `cms/crates/opendash-server/src/storage/builder.rs` is recorded in [G4 evidence](2026-11-12-m4-s07-t01-responsive-windows.md).

**NOT_RUN / outside Issue #103:** original pre-M4 binary restore; M4-S10 macOS/Windows full release parity, responsive visibility, arbitrary CSS, grid spans and production deployment. The snapshot restore test used the new binary to open the isolated pre-metadata database; no claim is made for a historical binary.

After merging this evidence, reconcile Project #3 Done and Issue #103 Closed/completed and then sync the [TASK_INDEX](../plans/TASK_INDEX.md) row in a separate PR. This document alone is evidence, not a GitHub status change.

Agent: **Forge · Chat On Steroids Windows** · 2026-11-12.
