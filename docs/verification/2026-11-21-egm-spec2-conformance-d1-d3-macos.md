# EGM Spec 2 — D1–D3 conformance native macOS Intel (#451)

**Issue:** [#451](https://github.com/thientran2359/opendash-cms/issues/451). **Spec:** [CPT Content Builder, Pattern CPT and Pattern Library](../designs/cpt-content-builder-and-pattern-library.md), approved at source/base `c4a723160c12456854057ae475ea0e0ef8a6a375`. **Implementation source:** `8b2f80b326406c57c7891f14073f109a24fc5c21` before this verification report. Agent: Harbor.

Host: native macOS 15.8.2 Intel (`x86_64`); Node 24.22.2, Python 3.13.18, Rust/Cargo 1.99.1, native Google Chrome. Browser/API fixtures used a disposable temporary canonical site root, private SQLite database, private Chrome context and random loopback port. The existing live review site on port 18964 was not used.

## Conformance fixed

- **D1 — Media sidebar:** `BuilderWorkspace` exposes the three specified tabs **Blocks / Patterns / Media**. The Media tab reuses the authenticated `MediaPicker` and inserts or replaces a `core/image` at the selected Builder location. Missing Section/Row/Column ancestors are created when necessary; all edits use the existing unsaved draft, preview invalidation, node limits, account guard and CAS Save path.
- **D2 — Pattern placement:** A copied theme Pattern is inserted immediately after the Section containing the selected Section/Row/Column/Element; no selection falls back to end-of-canvas. Every imported node gets a fresh ID. Source theme AST stays immutable; existing locked anchors and `core/list`/`core/pattern_ref` limits still go through the established validation.
- **D3 — Builder Save timestamp:** The `EntryBuilderOpen` response contains `updated_at` from `content_entries.updated_at`. The snapshot is read from the same SQLite transaction after the entry CAS mutation; the existing content-entry version and ETag remain authoritative. The focused Rust regression checks response, persisted DB snapshot and reread equality, plus unchanged version, AST and timestamp after stale `412`. The legacy exact-payload Page Builder test now checks `updated_at` separately and preserves all prior ACL/draft/reset assertions.

## Verification receipts

| Check | Result |
| --- | --- |
| Admin `npm run build` (TypeScript + Vite, 102 modules) | **PASS** |
| `python3.13 cms/scripts/check-standards.py` and `--self-test` | **PASS** |
| `python3.13 cms/scripts/check-plans.py` and `--self-test` | **PASS** |
| `python3.13 .agents/scripts/check-agent-artifacts.py` | **PASS** |
| `git diff --check`, smoke script Python syntax and targeted `rustfmt --edition 2024 --check` for all three changed Rust files | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| Full locked release Rust workspace suite with canonical `TMPDIR`, serial test threads | **PASS: 337/337 tests, 74 suites, 0 failed, 0 ignored** |
| Focused release `-p opendash-server --test m4_s11_t01_page_scope` | **PASS: 1/1** |
| Native Chrome / HTTP / SQLite Pattern, placement and Media smoke | **PASS: 9/9 grouped checks** |
| Whole-workspace `cargo fmt --all --check` | **FAIL: pre-existing formatting outside modified Rust files (325 diff hunks)** |

Native Chrome command:

```sh
.g5-checkouts/egm451-venv/bin/python cms/scripts/pattern_library_studio_e2e.py \
  --binary cms/target/debug/opendash \
  --artifact-dir .g5-checkouts/evidence/egm451-chrome
```

Chrome receipt: `.g5-checkouts/evidence/egm451-chrome/receipt.json`, source SHA `d62554a913a861a5dbe722b15e2dc320d0fb29c9`, debug binary SHA-256 `18ddc452d3778c319852dff0ab1cbf31ff20df8c75015da74e6a5273ccf6c06f`. The subsequent commit `8b2f80b` changes only an older Rust integration test; runtime/Admin source used by this Chrome binary is unchanged. Browser and API assertions cover anonymous `401`, active Theme v3 category/search catalog, three Builder sidebar tabs, empty-selection append, explicit selected-Section insertion, descendant selection, four copies with sixteen unique node IDs, original theme immutability, no save before explicit CAS, subsequent target-only edit, authenticated MediaPicker and public `404` for private image, with no browser page errors. Screenshots: `pattern-copies-before-save.png`, `pattern-copy-after-edit.png`, `media-picker-unsaved-copy.png` in that evidence folder.

The first full release attempt failed in the direct legacy test `m4_s11_t01_page_scope`: its three exact JSON fixtures omitted the new `updated_at` field. This was fixed without weakening the exact comparisons, and the focused release test then passed. The subsequent **complete unfiltered** workspace release command:

```sh
TMPDIR="<canonical-private-evidence-dir>/" cargo test --manifest-path cms/Cargo.toml \
  --workspace --release --locked -- --test-threads=1
```

completed with exit code 0, **337 passed across 74 suites**, no failures/ignored tests. Actual log `.g5-checkouts/evidence/egm451-rust-release-final.log` SHA-256 `4dd9968930117aeac0fbfb09180e313f02d625650281be3d6f0e4be6360f38a3`; compiled release binary SHA-256 `28b37376088cfe6feb988a9d8f76c1c4869f1366d705f5eeeeea6170055d70f9`. Other local logs: `egm451-page-scope-targeted.log`, `egm451-cargo-build.log`, `egm451-fmt.log`. The whole-workspace formatting result matches the previously recorded Spec 2 baseline exception; the three Rust files changed for #451 independently pass scoped formatter verification. Existing source data and runtime site directories are untouched.

**Closure boundary:** This is native macOS implementation evidence for the D1–D3 follow-up. The approved Spec 2 remains `Implemented` in `docs/designs/SITEMAP.md`; coordinator runs the subsequent EGM drift report before promoting the design to `Verified`. No new migration, public release or change to Preview/CSP contract is included.
