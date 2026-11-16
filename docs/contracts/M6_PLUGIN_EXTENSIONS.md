# M6 Plugin Extensions — Selected Contracts (G2 AGREED)

**Owner:** Nova (tiếp nhận hoàn tất thay Atlas) · **Date:** 2026-11-13 · **Design status:** **G2 AGREED** (`D01–D10` resolved).
**Decision authority:** [ADR-0013](../decisions/ADR-0013-m6-plugin-extensions.md) · **Baseline:** `main@176b469` (migration `0021_theme_import_journal.sql`).
**Implementation/verification:** `NOT_STARTED / NOT_RUN` until G3 slices are implemented and verified.

## 1. Package Revisions, Capabilities & Isolation (`D01`, `D02`, `P06`)

- **ABI & Manifest:** Keep Wasmi transport ABI v1 (`opendash_plugin_v1`). Extend manifest with optional `manifest_schema: 2`, `min_host_version`, and strict typed capability declarations. Unknown capability keys, unsupported `manifest_schema`, or unsatisfied `min_host_version` fail closed at install/enable with zero partial execution.
- **Directory & Staging (`A-DURABILITY` reuse under `<site_root>/plugins/`):**
  - Staging: `<site_root>/plugins/.staging/<run_uuid>/` (invisible to readers; cleaned/quarantined on startup).
  - Immutable revisions: `<site_root>/plugins/revisions/<revision_digest>/` (64-hex SHA-256) with `.complete` marker and `sync_all()` before drop-handle rename (Windows-safe).
- **SQLite Revision Pointer & Generation Lease:**
  - Table `plugin_revisions` + active pointer `revision_id` and monotonic `revision_generation` + CAS `version` on `plugins`.
  - Valid ZIP install atomically stages revision files, commits `plugin_revisions`, updates `plugins` pointer (`enabled = 1`), and increments `revision_generation` (atomic auto-enable).
  - **Disable / Update Fencing:** On disable or update, first increment/invalidate `revision_generation` in SQLite, then cancel/kill active worker permits. Every in-flight IPC commit must recheck `(plugin_id, enabled = 1, revision_generation)` inside the SQLite write transaction and fail closed on mismatch.
  - Parent creates an immutable invocation context `(plugin_id, revision_digest, revision_generation, phase, capabilities, actor_id, actor_role, deadline)`. Worker IPC cannot assert or escalate capabilities.

## 2. Owner-Neutral CNT & Taxonomy Bridge (`P08`, `P04`, `D03`, `D08`)

- **Owner-Neutral CNT Adapter:** Generalize `theme_cnt_declarations` / CNT declaration lifecycle to `ResourceOwner` (`Site`, `Theme(theme_id)`, `Plugin(plugin_id)`). Theme switching/withdrawal never mutates `Plugin(id)` declarations; plugin disable/uninstall/withdrawal marks its declarations `provider_inactive` or `withdrawn` without deleting canonical `content_types`, `content_fields`, `content_entries`, or `content_entry_values`.
- **Built-in & Cross-Owner Protection:** Built-in `post`/`page` and other owners' CPT keys cannot be claimed or overwritten (`409 Conflict`). Admin and Editor retain full CRUD on entries of `provider_inactive`/`withdrawn` CPTs.
- **Taxonomy Eligibility (`D08`):** Only core `category` and `tag` taxonomy kinds are supported in M6 (no custom taxonomy kinds). A plugin may declare `category`/`tag` eligibility only for its own canonical CPTs inside its declaration transaction. Terms and `entry_terms` links remain site-owned and editable by Editor/Admin even when the provider is inactive or withdrawn; re-enabling reuses canonical IDs without duplicating terms or reviving explicitly withdrawn eligibility.

## 3. ContentBeforeSave, ContentAfterSave & Actions (`P01`, `D04`, `D05`)

- **Save Hook Scope:** `ContentBeforeSave` and `ContentAfterSave` fire on canonical entry **create** and **update** (both draft Save and published entry edit). `Publish`, `Unpublish`, and `Delete` remain distinct operations without Save hooks in M6.
- **`ContentBeforeSave` (Pure Bounded Validator/Veto):**
  - Runs **before** opening the SQLite write transaction on an immutable bounded candidate envelope `(operation, type_key, entry_id?, expected_version?, schema_token, candidate_values, title, slug, term_ids, correlation_id, deadline)`. Only plugins with explicit content-read capability for that type may subscribe.
  - Mode restrictions enforced by parent + worker: **no HTTP, no KV writes, no content writes, no nested actions, no recursive dispatch**.
  - Output: strict tagged `Allow` or `Veto { code, field_errors }`. Diagnostic strings are length-bounded, control-char-stripped **raw UTF-8 plain text** (React/Admin renders as text nodes; no server HTML-entity escaping).
  - Total phase deadline: `5s` across ordered handlers `(priority, id)` (max 32 handlers). Any veto, invalid output, trap, timeout, or exhausted phase budget **fails closed** with zero DB/KV writes.
  - **Write-tx Revalidation:** After `ContentBeforeSave` returns `Allow`, the SQLite write transaction (`BEGIN IMMEDIATE`) re-verifies actor session/role, entry CAS (`412`), schema/provider generation, **slug uniqueness, reserved-root collision, media existence/visibility, and term validity/eligibility**, returning `409`/`412`/`422` deterministically without re-running validators.
- **`ContentAfterSave`:** Runs **after** durable commit with committed `(entry_id, version)` and bounded read/diagnostic output only (no KV/content writes, no HTTP, `5s` total phase cap). Failures are logged/reported but never revert a committed Save. No exactly-once durable outbox is promised in M6.
- **Standalone Actions (`host_call`):** Authenticated CMS-dispatched actions with declared `(action_id, input_schema, required_role, capabilities, timeout, public_effect)`. Reject mixed KV + content writes in V1. Enforce `actor RBAC ∩ plugin capabilities` + `enabled` + `revision_generation` both at admission and inside the SQLite write transaction before commit. Reject recursive self-dispatch deterministically.

## 4. External Egress & SSRF Pinning (`P02`, `D06`)

- **Scope:** Preserve v1 origin grants under explicit compatibility labelling. V2 network scope requires normalized `(scheme, host, effective_port, methods[], path_prefix?)` with HTTPS default. Reject credentials, URL fragments, malformed encodings, wildcard private networks, and `Host`/proxy/hop-by-hop/CRLF guest headers.
- **SSRF & DNS Pinning:** Implement a custom `reqwest::dns::Resolve` that resolves via async lookup, rejects the **entire** answer set if *any* returned IP is non-public (covering loopback, RFC1918, `169.254.0.0/16` metadata, `100.64.0.0/10` CGNAT, `::1`, `fc00::/7` ULA, `fe80::/10` link-local, `::ffff:0:0/96` IPv4-mapped IPv6, `64:ff9b::/96` NAT64, multicast, unspecified), and pins only verified `SocketAddr`s to the client connection.
- **Client Hardening:** Configure `.no_proxy()` on `reqwest::ClientBuilder` (preventing `HTTP_PROXY`/`HTTPS_PROXY` env bypass) and disable redirects (`.redirect(Policy::none())`).
- **Quotas & Streaming Cap:** `2s` per-request bound within invocation deadline, `10` HTTP requests per invocation sub-quota, `100` total host calls, `1 MiB` request body cap, and `1 MiB` response body cap enforced **incrementally per streamed chunk** (aborting immediately on overflow, including `Transfer-Encoding: chunked`). Redact `Authorization`, `Cookie`, `Set-Cookie`, and query/body secrets from logs and UI diagnostics.

## 5. Domain Owner-Neutral Blocks, Templates & Settings (`P03`, `D07`)

- Extend Builder blocks, template selection, and `SITE-01` settings storage with `ResourceOwner` (`Theme` vs `Plugin(id)`), keeping canonical domain registries as the sole authority.
- Plugin blocks use canonical Builder AST + validated MiniJinja `C-SEC` safe-subset renderer (no guest raw HTML). Disabling/removing a plugin preserves user AST nodes and retained settings values, omitting execution with safe fallback.

## 6. Declarative Admin UI & Guarded Actions (`P05`, `D09`)

- Schema v1 supports `form`, `table`, `list`, `tabs`, `button` only; bounded tree (`depth <= 8`, `<= 200` nodes/screen, `<= 20` screens/plugin, list page `<= 100`). No guest HTML/JS/CSS/eval/arbitrary URLs or secret fields.
- All reads/actions enforce session, per-action RBAC, CSRF, and Origin on the server. Host-held drafts keyed by `(plugin_id, screen_id, form_id, object_id, schema_version)` preserve unsaved edits across tabs, `401` re-auth (never auto-submitting), `403`/`412`/`422`, and timeout.

## 7. Cross-Module E2E & Cross-OS Acceptance (`P07`, `D10`)

- Require integrated E2E covering Plugin ZIP install -> CPT/fields/taxonomy/blocks/template/settings -> edit/Save/veto/action/egress -> disable/restart/reinstall -> data/layout preservation + snapshot/matching-binary restore.
- Release G5 for M6 requires native evidence on **both** Windows and macOS from one common source SHA (`M5 #157` macOS G5 remains independent).
