# Starter typed URL / conformance delta — macOS Intel native verification

- **Issue:** [#498](https://github.com/thientran2359/opendash-cms/issues/498) (`agent:harbor`)
- **Design:** [Approved Starter URL delta](../designs/starter-typed-url-and-conformance-delta.md), source and implementation baseline `3fb9ebe66c4a7343ed37fc8437a277f421f4d1fa`
- **Host:** macOS 15.8.2 / Darwin 24.6.0 / x86_64; Rust/Cargo 1.99.1; Node 24.22.2
- **Implementation commit:** `c34d2baac66b8dd216b8666428a3372e181ed4ad` (Agent: Harbor; frozen Starter 1.0.0 unchanged)
- **Merge SHA:** pending — **do not consider this verification record merged-source proof until replaced**

## Scope and invariant checks

Implementation adds `ThemeOptionType::Url` with raw + HTML5 entity URL validation and a capability derived exclusively from a selected manifest's public URL option schema, limited to a single exact `options.<key>` expression in anchor `href`. Package and import validation, Save/read fallback, Studio transient validation, preview part traversal and public Builder composition share this boundary. Built-in Starter 1.0.0 bytes are frozen; only four 1.0.1 overlays provide typed Header/Home CTA and authoritative Post archive navigation. Core Banner uses existing `.od-prose` typography. The Admin URL control retains raw drafts and counts Unicode scalars. No migrations/dependency upgrades or runtime site data are changed.

## Actual old release binary preserved before edits

The approved baseline was built in release mode **before touching source files**:

```text
cargo build --manifest-path cms/Cargo.toml --release --locked -p opendash-server --bin opendash
Result: PASS
Source: 3fb9ebe66c4a7343ed37fc8437a277f421f4d1fa
Preserved binary: /private/tmp/opendash-issue498-old-3fb9ebe/opendash-v100
SHA256: 2c183dac8f211a49d760e8b0d49f0229308fb912335ccf0305cc6005c6006687
```

The preserved binary and disposable test roots are private to the macOS test host. They are not added to Git or distributed in the release package.

## Verification commands

| Check | Result | Notes |
| --- | --- | --- |
| `npm --prefix cms/admin run build:public-tailwind` | PASS | Generated CSS unchanged byte-for-byte; no CSS file diff |
| `npm --prefix cms/admin run typecheck` | PASS | TypeScript checks |
| `python3.13 cms/scripts/check-standards.py` | PASS | Native run |
| `python3.13 cms/scripts/check-standards.py --self-test` | PASS | Native self tests |
| `python3.13 cms/scripts/check-plans.py` | PASS | EGM docs/sitemap validation |
| `python3.13 cms/scripts/check-plans.py --self-test` | PASS | 13 fixtures |
| `python3.13 .agents/scripts/check-agent-artifacts.py` | PASS | Native run |
| `npm --prefix cms/admin run build` | PASS | 108 modules; production bundle |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | PASS | Checked final changes |
| `cargo build --manifest-path cms/Cargo.toml --locked` | PASS | Native debug build; `dev` profile, exit code 0. |
| `TMPDIR=/private/tmp OPENDASH_STARTER_V100_BIN=/private/tmp/opendash-issue498-old-3fb9ebe/opendash-v100 cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | PASS | **79 suites / 410 passed / 0 failed**, native release, exit code 0. Full log `/private/tmp/opendash-issue498-full-release-final.log`. |
| `git diff --check` | PASS | Clean whitespace check after documentation updates |
| `node cms/scripts/codegraph-index-docs.mjs --list-sources` | PASS | Every canonical EGM document exists |
| `node cms/scripts/codegraph-index-docs.mjs` | BLOCKED (tooling) | Default Linux engine is absent on this Mac; installed `/Users/thientran/.codegraph/versions/v1.6.2/bin/codegraph` is a different CLI that rejects `--workspace`. Attempted both; documents were not indexed. No package/dependency changes made to work around this. |

## Native and browser evidence

The new `starter_url_delta_test.rs` uses canonical isolated site roots and randomly allocated loopback ports. It verifies fresh 1.0.1/ETag v1, raw invalid Save + invalid Studio transient with no mutation, valid unsaved preview + saved public anchors and restart. Its separate old→new→old test is run with `OPENDASH_STARTER_V100_BIN` set to the binary hash above: it creates old site content and a legacy option value, upgrades with known old revision repair, checks incompatible 1.0.1 activation 409, then corrects value and explicitly switches 1.0.0→1.0.1→1.0.0 using CAS before restarting the old binary and comparing content and options.

- Focused inline C-SEC/security/unit tests: **PASS 2/2**, including allowed anchor, forbidden private/legacy/filter/bracket/mixed-context/rebinding, Unicode scalar limits, entity injection and compiled render; extended post-render unsafe-context and sample validation were also compiled into the later workspace suite.
- Fresh-site native HTTP E2E: **PASS**, `typed_url_fresh_public_saved_and_unsaved_preview`; rejected invalid Save 400/no write, transient preview 422/no write, valid preview 200/no write, stored CTA rendered in Header+Home with MiniJinja HTML escaping `&#x2f;`/`&amp;`, startup idempotency and ETag v1.
- Actual old/new/old binary E2E: **PASS**, `actual_old_new_old_binary_roundtrip_preserves_data_and_cas` with `OPENDASH_STARTER_V100_BIN` set to SHA256-pinned actual release binary. Known 1.0.0 `.complete` marker was removed and repaired; selection/version stayed v1 while installing v101, incompatible saved option caused 409, correction followed by two explicit CAS activations yielded v2/v3; old binary restarted and returned health, authenticated options, original Page and public route with identical data.
- Assigned Posts Page `/news`: **PASS**, native published Post uses the authoritative back-link instead of a hardcoded `/posts`; disabled archive: **NOT_RUN** (the 1.0.1 template guards the backlink with `if archive_url`).
- Banner visual typography (desktop/mobile): **NOT_RUN**.
- Live Chrome Admin/Studio: **PASS for exercised scope**, macOS Chrome 158 desktop and Studio mobile 390px on disposable site root `/private/tmp/opendash-issue498-browser-m21slrqo`, port `62454` (not `18964`), new binary SHA256 `aa2720318242c4834dc177dd065f0ac27c051542d3e20e77a8ca01bf93b61684`. Browser Admin setup/login, typed URL help/scalar count, rejected `javascript:` Save while retaining draft, corrected URL Save version 1→2, Studio draft `#draft-preview` showing unsaved/save-enabled and returning to saved value showing clean/disabled. Desktop/Studio mobile CoS Desktop capture IDs `a3f98520-3fd1-435f-a5b1-836f724900e0` and `bea7f59a-838f-4f6e-90bf-655528cacdd6` are session captures, not checked-in PNGs. Console showed expected initial 401 (`/auth/me`), missing favicon 404 and deliberately invalid Save 400, with no unexpected JavaScript/CSP errors. **401/412 recovery browser flows NOT_RUN**; native CAS and draft cases covered separately.
- Post-merge exact-source rerun and Coordinator drift report: **NOT_RUN**.

The original Tailwind/Starter spec remains **Implemented**, and this delta is **not Verified** until native plus live-browser acceptance and independent Coordinator closure complete. Fresh 1.0.1 sites have no 1.0.0 record and no supported in-place old-binary downgrade. For upgraded sites, switch explicitly back to the preserved 1.0.0 revision using the new binary before using the old executable; never delete revision or site data for rollback.
