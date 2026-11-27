# OpenDash CMS Test & Demo Fixtures (`cms/fixtures/`)

## Overview

Deterministic, synthetic, and baseline fixtures supporting migration upgrade tests, Admin extension integration checks, M7 media/SMTP verification, M8 backup/restore E2E suites, and the isolated Mộc Studio demo website. None of these fixtures contain production credentials, personal data, or third-party proprietary media.

Directory index:
- `baseline_0001.sqlite` (`24576` bytes): Pre-M1.1 baseline SQLite database (`git` origin `1fba9ac`) containing legacy `theme_blocks` and `home_page` tables; managed by [make_baseline_fixture.py](../scripts/make_baseline_fixture.py) and consumed by upgrade/smoke tests (`cms/scripts/windows_auth_smoke.py`, `cms/scripts/content_types_smoke.py`, `cms/scripts/content_entries_smoke.py`, and `cms/crates/opendash-server/tests/m7_s09_t01_schema_field_groups.rs`).
- `admin-review/`: Isolated deterministic `admin_extensions` manifests (`plugin/plugin.json` for `review-provider` and `theme/theme.json` for `review-theme`) exercised by [admin_extensions_e2e.py](../scripts/admin_extensions_e2e.py); never installed on a live owner site.
- [demo-website](./demo-website/README.md): Mộc Studio isolated demo site fixture (`theme-moc-studio/` + [screen-inventory.md](./demo-website/screen-inventory.md)) driven by [demo-website-runner.py](../scripts/demo-website-runner.py) and `dev.py`.
- [m7](./m7/README.md): Synthetic MP4 (`video-avc-aac.mp4`) and localhost TLS test keypair (`smtp-test-cert.pem`, `smtp-test-key.pem`) for M7 media/SMTP E2E tests.
- [m8](./m8/README.md): Schema fingerprint baseline ([schema_fingerprints.json](./m8/schema_fingerprints.json)) and OPS-01 restore contract/negative-corpus definitions for M8 E2E harnesses.

## Interface

- **Baseline DB Generator & Verifier:** `[cms/scripts/make_baseline_fixture.py:create_baseline_fixture]` validates `baseline_0001.sqlite` schema/content (`theme_blocks`, `home_page` row `id = 1`) or regenerates it from a pre-M1.1 binary.
- **Admin Extension Fixture Manifests (`admin-review/`):**
  - `admin-review/plugin/plugin.json`: `review-provider` (`manifest_schema: 2`, `actions: ["inspect"]`, `admin_extensions` `columns` + `term_panels`).
  - `admin-review/theme/theme.json`: `review-theme` (`manifest_version: 3`, `templates: [{"kind": "single", "file": "single.html"}]`, `admin_extensions` `columns` + `term_panels`).
- **Subtree Packages:**
  - [demo-website/README.md](./demo-website/README.md): `seed`, `smoke`, `capture`, `all`, `serve`, and `reset` workflows under `runtime/demo-website/<run-name>/`.
  - [m7/README.md](./m7/README.md): SHA256-pinned synthetic `video-avc-aac.mp4` and loopback SMTP TLS certs.
  - [m8/README.md](./m8/README.md): `schema_fingerprints.json` (`schema_version: 33`, `trusted_schema: "OFF"`, bounded DDL limits) and `M8-S08-T01` restore checks.

## Invariants

- **Read-Only Source Fixtures:** Tests and scripts copy `baseline_0001.sqlite`, read `m7/` / `m8/` / `admin-review/` assets, or package `demo-website/theme-moc-studio/` into temporary/gitignored `OPENDASH_SITE_ROOT` directories—never mutating committed fixtures in place.
- **Zero Production Secrets:** Loopback TLS certs (`m7/`), demo credentials (`demo-website/`), and `admin-review/` manifests are strictly synthetic and test-scoped; `.odbackup.age` archives and passphrases are never committed.
- **Upgrade & Schema Determinism:** `baseline_0001.sqlite` preserves pre-M1.1 schema expectations for migration smoke tests, while `m8/schema_fingerprints.json` tracks the fail-closed restore bounds enforced by `[cms/crates/opendash-server/src/storage/restore.rs:validate_restored_db_read_only]`.

## Side Effects

- None from committed fixture files; running fixture-consuming scripts or tests creates temporary site roots (`std::env::temp_dir()` or `runtime/demo-website/<run-name>/`) and optional screenshots (`screenshots/demo-website/<run-name>/`).

## Verification

Verify the baseline SQLite fixture, demo runner, and M7/M8 integration suites:

```powershell
python cms/scripts/make_baseline_fixture.py --destination cms/fixtures/baseline_0001.sqlite
cargo test --manifest-path cms/Cargo.toml --locked --test m7_s08_t01_e2e --test m8_s08_t01_e2e
```
