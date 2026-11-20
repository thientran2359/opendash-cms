# M3-S31-T01 — Content foundation C1, macOS Intel implementation verification

- Issue: [#373](https://github.com/thientran2359/opendash-cms/issues/373)
- Immutable G3: [M3 content foundation G3](../archive/plans/milestones/M3/slices/2026-11-18-content-foundation-g3.md) from merged PR #372, source `695fe94a7c6c6f24e0dd35629a32e8e2ace3bf4d`
- G2: [ADR-0019](../decisions/ADR-0019-content-foundation.md) and [C1](../contracts/CONTENT_FOUNDATION_2026_11.md)
- Implementer and self-reviewer: Harbor; coordinator: Aster
- Implementation base: `22628cbf6fa2de88a3695efdde1b29866a3e2e90` (`origin/main` at pickup)
- Host: macOS 15.8.2, Intel `x86_64`; all tests on disposable independent roots with random loopback ports, **never port 18964**

## Observed behavior

Canonical CPT `settings` (`public`, `has_archive`, `publication`, `supports`) is persisted and returned in the Admin API and effective editor projection. Legacy user/provider CPT declarations without settings retain immediate/public/archive defaults; Post/Page always use draft/publish, Section stays private. Core schema retains identity and fields; only supported behavior flags are mutable. Settings PUT requires admin/CSRF and `If-Match`; writer transactions handle version and schema token changes, reject reversing draft/publish to immediate while any draft exists, and do not expose rejected writes. Provider declaration settings are validated/canonicalized on install/reinstall.

Entry create/composite save/ContentSave and publication status follow the stored CPT policy. Public single, archive, taxonomy listings, sitemap, SEO/custom paths, Admin View links, Builder lists and bound fields, and media ACL gate by active provider, published status, and `public`; archives additionally gate by `has_archive`. Private Section cannot leak through generic public entry routes. Turning off editor/image supports preserves prior values; user-defined custom fields with the same names as Post/Page Core fields remain editable.

The Admin exposes settings controls with version CAS, CPT draft/publish actions, confirmed public View links and 390px layouts; a failed 409/412 retains the active form's values and switching CPTs prompts before discarding edits.

## Actual validation before G4 merge

| Check | Observed result |
| --- | --- |
| `cargo build --manifest-path cms/Cargo.toml --locked -p opendash-server` | PASS (Intel native debug) |
| `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m3_s31_t01` | **5 PASS** |
| `TMPDIR="$PWD/.g5-checkouts/evidence/c1-tmp" cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash -- --test-threads=4` | **90 PASS**; includes C1 public/media/sitemap/Builder and test fixture corrections |
| `npm run build --prefix cms/admin` | PASS (TypeScript + Vite) |
| `git diff --check` | PASS |
| `python3 -m py_compile cms/scripts/content_foundation_m3_s31_t01_e2e.py cms/scripts/content_foundation_m3_s31_t01_upgrade_g5.py` | PASS |
| `.g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m3_s31_t01_e2e.py --binary cms/target/debug/opendash --artifact-dir /tmp/opendash-c1-preflight` | PASS, native Chrome 158 and 23 accepted assertions (Admin create/edit/draft/publish, public/404, media ACL, Builder binding, role/CSRF/CAS, concurrency, restart, 390px) |
| `.g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m3_s31_t01_upgrade_g5.py --old-binary .g5-checkouts/evidence/c1-release/opendash-old-22628cb --new-binary cms/target/debug/opendash --artifact-dir /tmp/opendash-c1-upgrade-preflight` | PASS, 7 checks covering actual 0037→0038 migration, DB + media snapshots, old/new/old rollback using restored pre-upgrade SQLite and subsequent re-edit |

The old release binary was compiled from immutable `22628cb` with its original embedded Admin assets. Old binary SHA-256: `7cd18a2f790e95994867de73840800a33add7292aa0cd7fd27b3a709b75696d2`. The premerge debug runner records dirty working-tree status and diff digest separately from `HEAD` so that its `source_sha` is never mistaken for a committed C1 release source. Screenshots and JSON receipts are private test artifacts, not website-owned files.

`cargo fmt --all -- --check` observes formatting differences across many untouched files already present at baseline; no repository-wide reformat was performed. The native plugin lock fixture now uses the full migration schema and an actual plugin FK, and historical taxonomy test calls were updated to the current nine-argument signature; both existing tests pass.

## Scope and release gates

One necessary C1 caller integration is in `storage/builder_binding.rs`: the old direct published-entry lookup could return a published **private** CPT to public Builder bindings. The added SQL guards and native regression close that leak. The two historical test fixture updates are isolated to test modules.

Concurrent public requests that began before an administrator toggled visibility can finish against their earlier authorized read; the public response uses `no-store` headers. For future strict read-snapshot semantics across multi-stage rendering, see public-route refactor scope; no in-place downgrade of migration 0038 is supported.

This is the **implementation verification** record. Required CI, exact merged release SHA and postmerge native macOS G5 receipt must be recorded separately before Issue #373 becomes Done or the Harbor slot accepts #374. Human dogfood and Windows/Linux/common RC/V1 remain separate gates. Issue #302 remains open.
