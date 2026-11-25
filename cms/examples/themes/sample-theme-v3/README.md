# Sample Theme v3 (`sample-theme-v3`)

## Overview

Minimal Theme Format v3 reference package (`theme_id: "sample-theme-v3"`, `manifest_version: 3`, `version: "1.0.0"`) illustrating [Theme Format and Template Resolver](../../../../docs/designs/theme-format-and-template-resolver.md), C-SEC helpers from [Theme Ergonomics](../../../../docs/designs/theme-ergonomics-and-csec-helpers.md), and two-step guarded consumption of write-time plugin projections (`entry.ext["rust-seo-plugin"]`) produced by [rust-seo-plugin](../../plugins/rust-seo-plugin/README.md).

Package layout:
- [theme.json](theme.json): Theme Format v3 manifest declaring templates and explicit static assets.
- [single.html](single.html): Single-entry MiniJinja template compliant with `UndefinedBehavior::Strict` and C-SEC rules.
- `assets/`: Static stylesheet in [assets/app.css](assets/app.css).

## Interface

- **Manifest Contract ([theme.json](theme.json)):**
  - `manifest_version: 3`, `theme_id: "sample-theme-v3"`, `templates: [{ "kind": "single", "file": "single.html" }]`, `assets: ["assets/app.css"]`.
- **Template Context & Helpers ([single.html](single.html)):**
  - Consumes `site.title`, `locale` (defaulted via `locale | default({})`), `theme_assets['assets/app.css']`, `breadcrumbs`, `entry.title`, `entry.published_at | date_format("%d/%m/%Y")`, `entry.reading_time_minutes | format_number`, `related_entries`, and `{{ od_content() }}`.
  - Reads optional plugin badge data safely under strict undefined checking:
    ```jinja
    {% set seo_ext = entry.ext["rust-seo-plugin"] | default({}) %}
    {% if seo_ext.badge | default("") %}
      <span class="sample-badge">{{ seo_ext.badge | default("") }}</span>
    {% endif %}
    ```
- **Offline Validator:** `opendash --check-theme` invokes `[cms/crates/opendash-server/src/theme.rs:check_theme_path]` via `[cms/crates/opendash-server/src/main.rs:run_check_cli_flag]`.

## Invariants

- **Explicit Asset Declaration (D01):** Only files declared in [theme.json](theme.json) (`theme.json`, `single.html`, `assets/app.css`) may appear in an uploadable theme ZIP; `README.md` must be excluded from runtime ZIP packages because D01 rejects undeclared files.
- **`UndefinedBehavior::Strict` Safety:** Optional context keys (`locale`, `entry.ext["rust-seo-plugin"]`, `seo_ext.badge`) are guarded with `| default(...)` prior to nested field access.
- **C-SEC Compliance:** [single.html](single.html) invokes `{{ od_content() }}` exactly once and [assets/app.css](assets/app.css) contains static CSS safe under C-SEC stylesheet sanitization.

## Side Effects

- None; this directory holds static theme package files validated read-only by `opendash --check-theme` or archived into a ZIP for Admin theme upload.

## Verification

Validate the theme directory offline (`[cms/crates/opendash-server/src/theme.rs:check_theme_path]`, also covered by `cms/crates/opendash-server/tests/theme_ergonomics_test.rs`):

```bash
./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3
```

To test an uploadable runtime archive (excluding `README.md`):

```bash
zip -r /tmp/sample-theme-v3.zip theme.json single.html assets/app.css
./cms/target/release/opendash --check-theme /tmp/sample-theme-v3.zip
```
