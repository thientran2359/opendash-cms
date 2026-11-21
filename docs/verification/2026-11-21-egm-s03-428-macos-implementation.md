# EGM S03 / Issue #428 — Reading Settings native macOS Intel implementation verification

**Scope:** Pre-merge implementation receipt for [Issue #428](https://github.com/thientran2359/opendash-cms/issues/428), [PR #440](https://github.com/thientran2359/opendash-cms/pull/440), [approved Theme Format v3 S03](../designs/theme-format-and-template-resolver.md). Implementation commit `cacff3cb3a9de20555c630675c658d5e6a2faa3a`; `main` baseline `f1979a3ad8250d16da854f9396d6d903fca299a5`. Agent: Harbor.

Host: macOS 15.8.2 (24H232), Intel `x86_64`, Cargo/Rust 1.99.1, Node 24.22.2, Chrome native. All server/browser test fixtures used disposable canonical site roots and OS-assigned loopback ports. No requests to the owner's live port `18964`, no modification of existing site database, themes, plugins, or uploads.

## Implemented contract and direct checks

- Admin-only `GET/PUT /api/admin/settings/reading`, `If-Match` `428`/`412`, CSRF and `AdmittedMutation`; one SQLite transaction for assignments, version, permalink ownership and history. Two distinct *published public Pages* required; custom Blog permalink can be assigned, changed and removed without a `409` namespace conflict.
- Homepage `/` reads the selected published Page through `Home → PageLayout → Page → Index` resolution. Posts Page resolves the post archive at its Page URL without exposing or deleting Page `values_json`. Assigned Page IDs remain after draft/deletion; public routes return `404` + `X-Robots-Tag: noindex`. Sitemap has one canonical `/` when indexable and removes the deleted Posts archive.
- Admin Reading Settings UI supports draft retention and refresh/rebase after `401` or stale `412`. Removed the experimental `/api/admin/builder/homepage*` handlers and client calls; Page Builder and typed templates remain available.
- Updated 12 legacy Rust integration fixtures, Page/Section browser fixtures, M7 sitemap 5,000/5,001 route boundary, and M8 writer-inventory test to reflect the approved route authority. Assertions were kept; no tests were deleted or ignored.

| Native verification | Result |
| --- | --- |
| Admin `npm run build` | **PASS** (TypeScript + Vite, 99 modules) |
| `python3 cms/scripts/check-standards.py` and `--self-test` | **PASS** |
| `python3 cms/scripts/check-plans.py` and `--self-test` | **PASS** |
| `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** |
| Rustfmt `--edition 2024 --check` on all changed `.rs` files; `git diff --check` | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked --release -p opendash-server --bin opendash` | **PASS** |
| S03 native HTTP `reading_public_s03` + M7 sitemap boundary | **PASS** (1/1 + 1/1) |
| M8 route writer-inventory release test | **PASS** (2/2) |
| macOS release workspace without one individually verified nonterminating Wasmi barrier test | **PASS** (332 tests across 72 suites, 0 failures; 1 filtered test) |
| Complete release `opendash` unit binary with the barrier test, one test thread | **PASS** (153/153, 0 failures, 0 ignored) |
| Native Chrome Page Builder + Reading UI + Section Builder | **PASS** (18/18 + 7/7 + 19/19, no uncaught JS errors) |

The workspace release command used a canonical private `TMPDIR` and `-- --test-threads=1 --skip plugin::review_regressions::abort_during_staged_commit_must_cleanup_connection_and_pool`. That barrier test is **not waived**: it passed in the separate complete 153/153 release unit-binary run on the same source. The test has an unbounded `yield_now` loop and intermittently stalled during the all-target run on this Intel host; the hung process was stopped after a diagnostic sample was captured. Earlier unqualified workspace invocations also failed on macOS's symlinked `/var` temporary root and, separately, on parallel C4 server health timeouts. C4 passed **9/9** with serial execution. These non-PASS attempts are retained in the local evidence history and are not represented as successful full invocations.

The repository-wide `cargo fmt --all --check` reports pre-existing formatting differences in unrelated baseline files; the **changed-Rust-file formatter used in `cms-ci` passed**. `node cms/scripts/codegraph-index-docs.mjs` could not run because `@astudioplus/codegraph-mcp` / `CODEGRAPH_ENGINE_PATH` is unavailable on this host; no index result is claimed.

## Native Chrome evidence and isolation

All three runs used native Chrome and the compiled release binary (`SHA-256 3c3b0b3e54bd369562ebaf39a9a8fa4e3f73cb176ea51888b2f911fb610a4fe1`), fresh Admin accounts and disposable data. The 7 Reading UI checks cover successful atomic assignment, public Home/Blog resolution, duplicate Page rejection, draft preservation after `412`, refresh/rebase, and zero JavaScript exceptions. The 18 Page checks cover exact Page scope, draft/publication isolation, `401`/`412`, media ACL, mobile 390px and restart. The 19 Section checks cover private Section ownership, theme v3 A→B→A, plugin lifecycle, CAS, media, backup/restore and restart. The optional older-baseline binary upgrade/collision branch of the Section runner was **NOT_RUN** because no historical baseline binary was supplied.

The following local ignored receipts are retained under `.g5-checkouts/evidence/` (not committed):

| Evidence file | SHA-256 |
| --- | --- |
| `s03-428-workspace-release-exclude-flaky.log` | `abcb3a074d84e5191050cc9ceac121dbc671eb7ce83d69d1a9de65f93e7fad17` |
| `s03-428-bin-release.log` | `c051a30c25f07aa24a788f667ce7481f90e2880bc19e0f6e14d086c61c92a80d` |
| `s03-428-writer-inventory-release.log` | `c5722fe6d7bc91f83d1cd72d370b8eacc6c4a57df0d1071e87edb278ccc7ea1d` |
| `s03-428-native-page/receipt.json` | `625cb634d6f12cbeb9154405ebdcf1ad25f3e2f77f1045a255eda369c6c12a06` |
| `s03-428-native-sections/receipt.json` | `8855f3d295c3ea4cfd7c6856dd3b1654cbb8fe1865da14dca9aaea5747a49150` |
| `s03-428-native-reading/receipt.json` | `090bd5bddcc348311fff22b59f736e323673c9006dd06bff2fb44dff3c507f41` |

## GitHub gates and closure boundary

Required governance checks on PR #440's first head `cf41599958104b71daea4ddf35dfa79719f54c86` passed Ubuntu/Windows. The first `cms-ci` run [44894785055](https://github.com/thientran2359/opendash-cms/actions/runs/44894785055) failed because M8's historical inventory test expected the removed `homepage_put` handler. Commit `a55b72905e35bcfd9206d80246bdff6ee53e2259` updates that same audit to demand `reading_put` with `AdmittedMutation`; the focused native release audit passed 2/2. A new CI run is required on the **final PR head** before merge; this receipt does not claim that it already passed.

This is a pre-merge native implementation receipt, not an exact-merged-source G5 or a public release. After PR merge, the coordinator/closing agent must perform or attach exact-merged-source verification and the EGM drift report before updating the design sitemap to `Verified` and marking Issue/Project `Done`.
