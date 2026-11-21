# EGM Spec 2 S03 — Pattern Library native macOS Intel verification (#431)

**Scope:** Pre-merge implementation evidence for [Issue #431](https://github.com/thientran2359/opendash-cms/issues/431) and the [approved CPT / Pattern design](../designs/cpt-content-builder-and-pattern-library.md). Source baseline `d7de1dc2b83b05945f027e5334d24911c0a1779c`, implementation commit `73e6eff7ac6c4f66a3b133f98f52bca01221376a`; Agent: Harbor.

Host: native macOS 15.8.2 Intel `x86_64`; Node 24.22.2, Cargo/Rust 1.99.1 and native Chrome. Every browser/API fixture used an isolated temporary canonical site root, private SQLite database, disposable browser context and OS-assigned loopback port. The live review site on port `18964` was not used.

## Behavior implemented and verified

- Reused the already integrated S02 read-only theme Pattern APIs: `GET /api/admin/patterns/library?category=&q=` and `GET /api/admin/patterns/library/theme/{key}`. The typed Admin client uses the shared request guard, signals and explicit path/query encoding.
- Added the **Patterns** sidebar tab and `Explore all patterns` modal with category filter, debounced search, image preview and empty-state handling. The existing **Blocks** tab and independent `core/pattern_ref` synchronized Pattern CPT flow remain available.
- `cloneWithFreshNodeIds` deep-copies every Section, Row, Column and Element with a fresh node ID, then appends the copied Sections to the current Builder draft. Target Builder checks preserve locked node anchors, node/byte limits, list limits and PatternRef target rules. No browser action writes to the theme source.

| Native verification | Result |
| --- | --- |
| `npm run build` (Admin TypeScript + Vite, 102 modules) | **PASS** |
| `check-standards.py` and `--self-test` (Python 3.13) | **PASS** |
| `check-plans.py` and `--self-test` (Python 3.13) | **PASS** |
| `.agents/scripts/check-agent-artifacts.py` | **PASS** |
| `git diff --check` and Python script syntax check | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked -- --test-threads=1` (canonical private `TMPDIR`) | **PASS — 337/337 tests, 74 suites, 0 failed, 0 ignored** |
| Native Chrome + HTTP/SQLite S03 acceptance runner | **PASS (6/6)** |
| Repo-wide `cargo fmt --all --check` | **FAIL (baseline unrelated Rust formatter differences; no Rust files changed by S03)** |

The Chrome runner `cms/scripts/egm_pattern_library_s03_e2e.py` used the compiled debug `opendash` binary (SHA-256 `a9b908779e234a6233e826770b66b308a9450b31ffc24ab617c8bb6c2d52526c`) from implementation commit `73e6eff`. It confirmed: anonymous `401` and no-active-theme empty catalog; valid v3 theme ZIP activation with `landing`/`cta` categories and mixed `category`/`q` filters; immutable theme descriptor/AST and preview asset; real modal navigation/search; two consecutive insertions with **8 distinct cloned IDs** disjoint from source IDs; no database changes before Save; CAS-backed Page Builder Save; subsequent target-only edit; original theme AST unchanged; no uncaught browser errors.

The initial unqualified release invocation encountered **8 environment-only failures** in `operations` and `paths` because macOS `/var` is a symlink and `SitePaths` intentionally rejects symlinked roots (146 passed/154 unit tests). Re-running the complete, unfiltered workspace release suite with `TMPDIR` set to the canonical private workspace evidence directory and `--test-threads=1` yielded **PASS — 337/337 across 74 suites**, including all 154 unit tests, no ignores or skips. Full log: `.g5-checkouts/evidence/egm-pattern-s03/release-full-serial.log` (SHA-256 `9338c76067a03523ddfd94e5336504be575b76ae2920873dbe0ae6974be47ff5`).

Artifacts (private, gitignored) are under `.g5-checkouts/evidence/egm-pattern-s03/`: `receipt.json`, `pattern-copies-before-save.png`, `pattern-copy-after-edit.png`. The receipt records exact commit, binary hash, platform and each passing check.

The formatter exception is verified against the clean pre-change source: `cargo fmt --all --check` proposes formatting in pre-existing Rust files such as `opendash-core/src/backup.rs`; this S03 commit contains only TypeScript and the Python runner. No unrelated formatting was applied.

`node cms/scripts/codegraph-index-docs.mjs --list-sources` **PASS** for all active design/contract/decision sources. Actual `codegraph-index-docs.mjs` indexing is **NOT RUN** because the pinned CodeGraph engine `@astudioplus/codegraph-mcp@0.21.0` / `CODEGRAPH_ENGINE_PATH` is unavailable on this host.

## Boundaries / follow-up

This is macOS pre-merge proof for the S03 Pattern Library, not cross-OS proof, an exact-merged-source release or a public deploy. Existing S01/S02 test coverage remains the authority for CPT modes, synchronized `PatternRef`, media ACL and 401/412 behavior. The EGM drift report remains a design-closure responsibility before the spec can be promoted from `Implemented` to `Verified`.
