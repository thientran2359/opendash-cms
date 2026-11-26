# OpenDash CMS Blueprint (`cms/`)

## System Overview

`cms/` contains the development source code, database migrations, plugin SDKs, reference examples, deployment templates, and verification tooling for OpenDash CMS — a single-binary Rust (`1.99.1`) + SQLite CMS built with Axum, MiniJinja, Wasmi (`=2.0.0`), and an embedded React 18 + TypeScript + Tailwind CSS Admin UI (`rust-embed`). Technical architecture, contracts, and EGM Design Specs live at [`../docs/README.md`](../docs/README.md); workspace run instructions live at [`../README.md`](../README.md).

## Local Interface & Invariants

- **Workspace manifest ([`Cargo.toml`](Cargo.toml)):** `resolver = "3"` with 3 workspace members: `crates/opendash-core`, `crates/opendash-server`, and `crates/opendash-plugin-sdk`.
- **Pinned toolchain & dependencies ([`rust-toolchain.toml`](rust-toolchain.toml), `Cargo.lock`, `admin/package-lock.json`):** Rust `1.99.1` (`minimal` profile); pinned crates include `axum =0.8.9`, `sqlx =0.9.0`, `minijinja =3.0.5`, `wasmi =2.0.0`, `wat =1.263.0`, `image =0.25.10`, `age =0.11.2`, `argon2 =0.5.3`, `lettre =0.11.23`, `reqwest =0.12.12`, `rust-embed =8.6.0`, `tokio =1.54.1`, `jiff =1.0.2`, `zip =9.3.0`.
- **Build invariants:** Builds and tests must pass with `--locked` (Cargo) and `npm ci` (Node lockfiles); React Admin output builds into `admin/dist/` before embedding into `opendash-server`.

## Subdirectories & System Boundaries

### Rust Crates
- `crates/opendash-core`: [`./crates/opendash-core/README.md`](./crates/opendash-core/README.md) — Pure-Rust domain types, validators, Builder AST/codec, declarative block schema, RichText sanitizer, icons, and backup contracts (15 modules in `src/lib.rs`).
- `crates/opendash-plugin-sdk`: [`./crates/opendash-plugin-sdk/README.md`](./crates/opendash-plugin-sdk/README.md) — Guest-side Rust SDK targeting `opendash_v1` ABI v1/v1.1 with `export_plugin!`, typed host wrappers, and native `MockHost`.
- `crates/opendash-server`: [`./crates/opendash-server/README.md`](./crates/opendash-server/README.md) — Single-binary Axum server (`opendash`) embedding `cms/admin/dist/` and orchestrating `SiteCoordinator`, `SitePaths`, C-SEC rendering, media/SMTP, and backup/restore.
  - `src/routes/`: [`./crates/opendash-server/src/routes/README.md`](./crates/opendash-server/src/routes/README.md) — 23 Axum HTTP handler modules (auth/RBAC, CSRF, `If-Match`, `AdmittedMutation`; zero direct SQL).
  - `src/storage/`: [`./crates/opendash-server/src/storage/README.md`](./crates/opendash-server/src/storage/README.md) — 32 SQLite/SQLx domain modules owning 100% of `sqlx::query*`/`sqlx::raw_sql` and migrations `0001`–`0054`.
  - `src/plugin/`: [`./crates/opendash-server/src/plugin/README.md`](./crates/opendash-server/src/plugin/README.md) — Isolated `--plugin-worker` Wasmi (`=2.0.0`) sandbox, `StagedCommitGuard`, and DNS-pinned SSRF-hardened egress.

### React Admin (`admin/src/`)
- `admin/src/api`: [`./admin/src/api/README.md`](./admin/src/api/README.md) — Single HTTP transport (`client.ts`) and typed endpoint layer across 21 TypeScript modules.
- `admin/src/shell`: [`./admin/src/shell/README.md`](./admin/src/shell/README.md) — 3-group navigation frame (`AdminShell`), auth/bootstrap panel, RBAC guards, and hash URL router (`router.ts`).
- `admin/src/studio`: [`./admin/src/studio/README.md`](./admin/src/studio/README.md) — Fullscreen OpenDash Studio workspace combining Mode 1 site customizer/template overrides and Mode 2 entry visual page builder.
- `admin/src/builder`: [`./admin/src/builder/README.md`](./admin/src/builder/README.md) — Custom 4-layer AST visual builder (`Section -> Row -> Column -> Element`), schema-driven forms, responsive controls, and sandboxed preview frame.
- `admin/src/features`: [`./admin/src/features/README.md`](./admin/src/features/README.md) — Domain feature panels/workspaces across 17 `features/*` subdirectories (`editor/`, `content/`, `media/`, `themes/`, `plugins/`, `operations/`, etc.).
- `admin/src/components`: [`./admin/src/components/README.md`](./admin/src/components/README.md) — Reusable Tailwind + React 18 UI, form, and locale-tab primitives.
- `admin/src/i18n`: [`./admin/src/i18n/README.md`](./admin/src/i18n/README.md) — Site-wide i18n provider, `Intl` formatters, and `en`/`vi` translation catalogs (`2,290` keys).

### Database, SDK, Tooling & Deployment
- `migrations/`: [`./migrations/README.md`](./migrations/README.md) — 53 embedded forward-only SQLite migrations (`0001`–`0054`, omitting `0016`).
- `sdk/`: [`./sdk/README.md`](./sdk/README.md) — AssemblyScript guest SDK (`sdk/as/opendash.ts`) & JS type contract (`sdk/js/opendash.d.ts`) for `opendash_v1` ABI v1/v1.1 (19 Host Ops) and Node test harness.
- `scripts/`: [`./scripts/README.md`](./scripts/README.md) — 36 build, grouped fast test, health, architecture/plan/i18n gate, CodeGraph, smoke, benchmark, and live-browser E2E scripts.
- `deploy/`: [`./deploy/README.md`](./deploy/README.md) — Linux installer, hardened `systemd` unit, and Nginx TLS reverse-proxy template.

### Examples & Fixtures
- `examples/plugins/rust-seo-plugin`: [`./examples/plugins/rust-seo-plugin/README.md`](./examples/plugins/rust-seo-plugin/README.md) — Standalone Rust WASM plugin example (`manifest_schema: 3`) with `MockHost` tests.
- `examples/plugins/sample-plugin`: [`./examples/plugins/sample-plugin/README.md`](./examples/plugins/sample-plugin/README.md) — Legacy v1 AssemblyScript transformer plugin example (`manifest_schema: 1`).
- `examples/themes/sample-theme-v3`: [`./examples/themes/sample-theme-v3/README.md`](./examples/themes/sample-theme-v3/README.md) — Minimal Theme Format v3 (`manifest_version: 3`) reference package.
- `fixtures/`: [`./fixtures/README.md`](./fixtures/README.md) — Baseline SQLite upgrade fixture (`baseline_0001.sqlite`), `admin-review/` manifests, [`./fixtures/demo-website/README.md`](./fixtures/demo-website/README.md), [`./fixtures/m7/README.md`](./fixtures/m7/README.md), and [`./fixtures/m8/README.md`](./fixtures/m8/README.md).

## Global Invariants

- **Strict layering:** `opendash-core` stays pure Rust without I/O, HTTP, SQL, or Wasm; 100% of `sqlx::query*`/`sqlx::raw_sql` statements reside in `opendash-server/src/storage/`; all authenticated HTTP mutations enforce `require_*_with_csrf` and `AdmittedMutation`.
- **Single frontend HTTP boundary:** Every `fetch` in `admin/src/` goes through `admin/src/api/client.ts` with `X-CSRF-Token`, `If-Match` concurrency checks, and in-memory draft preservation on `401`/`412`.
- **Runtime data separation (`SitePaths`):** Source code under `cms/` never stores website runtime data (`database/`, `themes/revisions/`, `plugins/revisions/`, `uploads/`, `backups/`, `config/site.json`, `runtime/`); smoke/E2E harnesses isolate via `OPENDASH_SITE_ROOT`.

## Global Side Effects

- Compiling and testing writes build outputs into gitignored paths (`cms/target/`, `cms/admin/dist/`, `cms/admin/node_modules/`).
- Running the `opendash` server opens the SQLite WAL database, acquires a single-instance site lockfile, writes staged files under `SitePaths`, and spawns isolated `--plugin-worker` / `--operations-worker` child processes.

## System Verification

- Run: `python cms/scripts/check-standards.py && python cms/scripts/check-plans.py && python .agents/scripts/check-agent-artifacts.py`
- Run: `node cms/scripts/check-i18n.mjs --strict && npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build`
- Run: `cargo fmt --manifest-path cms/Cargo.toml --all --check && python cms/scripts/run-tests.py --changed` (pre-release full suite: `python cms/scripts/run-tests.py --group all --release`)
