# OpenDash Core (`opendash-core`)

## Overview

Pure-Rust domain crate (`Cargo.toml` dependencies: `serde`, `serde_json`, `url` only; zero SQL, HTTP, filesystem I/O, or Wasm runtime code). Defines canonical identifiers, ownership-aware registry catalogs, lifecycle hook/filter envelopes, unified field schemas (`field_schema.rs`), content-type schemas, the 4-tier Builder AST (`Section -> Row -> Column -> Element`) and its `<!-- od:... -->` comment-block codec, declarative block schemas, RichText HTML sanitization, SVG icons, theme v3 manifests, plugin v1–v3 manifests, typed admin extensions, contact forms, CMS UI language catalog, manual redirect/404 normalization & CSV codec (`redirects/`), and backup/restore manifest contracts across the 17 modules exported by [`src/lib.rs`](src/lib.rs).

See [Core Foundation](../../../docs/contracts/CORE_FOUNDATION.md) and [CMS Architecture](../../../docs/architecture/CMS_ARCHITECTURE.md) for cross-crate boundaries.

## Interface

- **Identifiers & Namespaces** ([`src/id.rs`](src/id.rs)):
  - `[id.rs:RESERVED_SYSTEM_ROOTS]`, `[id.rs:is_reserved_system_root]`, `[id.rs:valid_snake_key]`, `[id.rs:valid_snake_key_with_limit]`, `[id.rs:valid_kebab_segment_with_limit]`, `[id.rs:valid_bcp47_tag]`, `[id.rs:valid_lowercase_locale_code]`, `[id.rs:valid_namespaced_block_id]`, `[id.rs:valid_namespaced_snake_key]`, `[id.rs:validate_package_id]`, `[id.rs:valid_package_id]`, `[id.rs:validate_semver]`, `[id.rs:valid_layout_slice_panel_id]`, and `[id.rs:ScopedResourceId]` (`core:<key>`, `user:<key>`, `theme:<pkg>:<key>`, `plugin:<pkg>:<key>`).
- **Manual Redirects, Legacy %XX Path & CSV Codec** ([`src/redirects/`](src/redirects/mod.rs)):
  - `[redirects/mod.rs:RedirectValidationError]`, `[redirects/mod.rs:CsvRowError]`, `[redirects/mod.rs:RedirectStatusCode]`, `[redirects/mod.rs:ConflictPolicy]`, `[redirects/mod.rs:CsvImportRequest]`, `[redirects/mod.rs:CsvImportResponse]`, `[redirects/path.rs:decode_safe_legacy_request_path]`, `[redirects/path.rs:normalize_redirect_source_path]`, `[redirects/path.rs:normalize_redirect_target_url]`, `[redirects/path.rs:build_redirect_location]`, `[redirects/csv.rs:parse_redirects_csv]`, and `[redirects/csv.rs:format_redirects_csv]`.
- **Ownership Registry** ([`src/registry.rs`](src/registry.rs)):
  - `[registry.rs:ResourceOwner]`, `[registry.rs:ProviderStatus]`, `[registry.rs:DeclarationState]`, `[registry.rs:OwnedDescriptor]`, `[registry.rs:RegistryCatalog]`, and `[registry.rs:RegistryError]`.
- **Hooks, Filters & Diagnostics** ([`src/hooks.rs`](src/hooks.rs)):
  - `[hooks.rs:HookEvent]` (15 lifecycle hooks and `filter:*` variants), `[hooks.rs:HookEnvelope]`, `[hooks.rs:PluginEnvelopeV2]`, `[hooks.rs:EnvelopeKind]`, `[hooks.rs:ExecutionPrincipal]`, `[hooks.rs:HookExecutionMode]`, `[hooks.rs:ContentBeforeSaveDecision]`, and `[hooks.rs:sanitize_plain_text]`.
- **Content Types, Fields & Editor Layout** ([`src/content.rs`](src/content/mod.rs)):
  - `[content.rs:ContentTypeDefinition]`, `[content.rs:ContentTypeDefinitionInput]`, `[content.rs:ContentTypeSettings]`, `[content.rs:ContentPublication]`, `[content.rs:ContentSupport]`, `[content.rs:ContentMode]`, `[field_schema.rs:FieldType]`, `[field_schema.rs:FieldSpec]`, `[field_schema.rs:FieldRole]`, `[field_schema.rs:PublicExposure]`, `[field_schema.rs:FieldScopeKind]`.
  - `[content.rs:ManifestFieldGroupInput]`, `[content.rs:UserFieldGroupInput]`, `[content.rs:FieldGroupRecord]`, `[content.rs:EffectiveContentSchema]`, `[content.rs:verify_effective_schema_transition]`, `[content.rs:CntDeclarations]`, `[content.rs:TaxonomyDefinition]`, `[content.rs:ContentEntryInput]`, `[content.rs:PatchField]`.
  - `[content.rs:EditorLayoutConfigV1]`, `[content.rs:EditorRegion]`, `[content.rs:PanelDefinition]`, `[content.rs:resolve_and_validate_editor_layout]`, and `[content.rs:project_effective_editor_config]`.
- **Builder AST & Responsive Layout** ([`src/builder.rs`](src/builder/mod.rs)):
  - `[builder.rs:BuilderDocument]` (`SectionNode -> RowNode -> ColumnNode -> ElementNode`), `[builder.rs:ElementContent]`, `[builder.rs:KnownElement]` (12 variants including `core/*`, `legacy/block`, `plugin/block`), `[builder.rs:Responsive]`, `[builder.rs:ResponsiveStyle]`, `[builder.rs:BoxSpacing]`, `[builder.rs:compile_box_spacing_tailwind]`, `[builder.rs:NodeMeta]`, `[builder.rs:SectionStyle]`, `[builder.rs:ElementBinding]`, `[builder.rs:BindingSource]`, and `[builder.rs:BuilderError]`.
- **Comment-Block Codec** ([`src/builder_blocks.rs`](src/builder_blocks/mod.rs)):
  - `[builder_blocks.rs:serialize_builder_document_to_blocks]`, `[builder_blocks.rs:parse_blocks_to_builder_document]`, `[builder_blocks.rs:parse_blocks_lenient_or_recover]`, `[builder_blocks.rs:extract_visible_text]`, `[builder_blocks.rs:extract_visible_text_with_schemas]`, `[builder_blocks.rs:extract_block_refs_lenient_with_schemas]`, `[builder_blocks.rs:encode_comment_json]`, `[builder_blocks.rs:ParsedBlockDocument]`, and `[builder_blocks.rs:ExtractedBlockRefs]`.
- **Unified Field Schema** ([`src/field_schema.rs`](src/field_schema/mod.rs)):
  - `[field_schema.rs:FieldSpec]`, `[field_schema.rs:FieldType]`, `[field_schema.rs:FieldContext]`, `[field_schema.rs:validate_field_specs]`, `[field_schema.rs:validate_field_values]`, `[field_schema.rs:collect_media_ids]`, `[field_schema.rs:collect_public_media_ids]`, `[field_schema.rs:collect_relational_refs]`, `[field_schema.rs:collect_options_source_values]`, `[field_schema.rs:global_site_option_fields]`, `[field_schema.rs:global_site_option_defaults]`, `[field_schema.rs:valid_safe_url]`, and `[field_schema.rs:valid_color_value]`.
- **Declarative Block Schemas** ([`src/block_schema.rs`](src/block_schema.rs)):
  - `[block_schema.rs:BlockCategory]`, `[block_schema.rs:BlockSchemaV1]`, `[block_schema.rs:RegisteredBlockSchema]`, `[block_schema.rs:MAX_NODE_VALUES_KEYS]`, and `[block_schema.rs:MAX_NODE_VALUES_BYTES]`.
- **RichText Sanitizer** ([`src/richtext.rs`](src/richtext/mod.rs)):
  - `[richtext.rs:sanitize_richtext]`, `[richtext.rs:strip_richtext_to_plain]`, and `[richtext.rs:decode_html5_entities]`.
- **Theme & Plugin Manifests** ([`src/theme.rs`](src/theme/mod.rs), [`src/plugin.rs`](src/plugin/mod.rs)):
  - `[theme.rs:ThemeV3Manifest]`, `[theme.rs:ThemeTemplateDecl]`, `[theme.rs:TemplateCandidate]`, `[theme.rs:TemplateCandidateKind]`, `[theme.rs:PageLayoutDecl]`, `[theme.rs:ThemePatternDecl]`, and `[theme.rs:valid_v3_asset_path]`.
  - `[plugin.rs:PluginManifest]`, `[plugin.rs:PluginPermissions]`, `[plugin.rs:ScopedFieldGrant]`, `[plugin.rs:NetworkScopeGrant]`, `[plugin.rs:PluginActionDeclaration]`, `[plugin.rs:PluginSettingField]`, `[plugin.rs:plugin_can_read_field]`, `[plugin.rs:plugin_can_write_field]`, and `[plugin.rs:PLUGIN_ABI_VERSION]`.
- **Admin Extensions, Contact Forms, i18n & Backup** ([`src/admin_extensions.rs`](src/admin_extensions.rs), [`src/contact.rs`](src/contact.rs), [`src/i18n.rs`](src/i18n.rs), [`src/backup.rs`](src/backup.rs)):
  - `[admin_extensions.rs:AdminExtensions]`, `[admin_extensions.rs:AdminColumn]`, `[admin_extensions.rs:AdminTermPanel]`, and `[admin_extensions.rs:AdminExtensions::validate]`.
  - `[contact.rs:FormSchema]`, `[contact.rs:FormField]`, `[contact.rs:FormKind]`, `[contact.rs:validate_form_schema]`, `[contact.rs:validate_and_normalize_submission]`, and `[contact.rs:valid_ascii_email]`.
  - `[i18n.rs:CMS_LANGUAGE_CATALOG]` (32 locales; `en` and `vi` installed), `[i18n.rs:CmsLocaleOption]`, `[i18n.rs:find_cms_locale]`, and `[i18n.rs:resolve_effective_ui_locale]`.
  - `[backup.rs:BackupManifestV1]`, `[backup.rs:BackupFileRecord]`, `[backup.rs:BackupFileKind]`, `[backup.rs:PortableRuntimeConfigV1]`, `[backup.rs:canonicalize_trusted_origin]`, `[backup.rs:validate_logical_key]`, and `[backup.rs:validate_manifest_v1]`.

## Invariants

- **Purity**: No network, SQLite, filesystem, or Wasm dependencies in `Cargo.toml`; all validation and schema assembly logic is deterministic.
- **Canonical Identifiers & Reserved Names**: Package IDs forbid `"core"` and `"legacy"` and enforce `3..=64` `[a-z0-9-]` without leading/trailing `-` (`[id.rs:validate_package_id]`). `ScopedResourceId::parse` rejects the reserved `"layout:"` site-layout namespace (`[id.rs:ScopedResourceId::parse]`). `RESERVED_ENTRY_KEYS` and `"status"` cannot be shadowed by extension field groups (`[field_schema.rs:validate_field_specs]`).
- **Ownership Non-Takeover & Data Preservation**: `RegistryCatalog::register` and `redeclare` reject claims by a different `ResourceOwner` with `RegistryError::OwnerConflict`, while `deactivate_owner` and `withdraw` retain descriptors (`[registry.rs:RegistryCatalog]`). `verify_effective_schema_transition` blocks removing fields with existing entries and forbids changing field types (`[content.rs:verify_effective_schema_transition]`).
- **Field ACL & Exposure Safety**: `validate_field_specs` forbids `min_edit_role < min_view_role`, forbids `PublicExposure::Public` when `min_view_role == Admin`, and requires `default_value` for admin-only required declared fields (`[field_schema.rs:validate_field_specs]`). `plugin_can_read_field` and `plugin_can_write_field` enforce `PluginManifest` grants, action capabilities, and `FieldRole` (`[plugin.rs:plugin_can_read_field]`, `[plugin.rs:plugin_can_write_field]`).
- **Bounded Trees, HTML & URL Safety**: `BuilderDocument` enforces `MAX_BUILDER_NODES` (512), `MAX_BUILDER_JSON_BYTES` (256 KiB), `MAX_BUILDER_LISTS` (8), and preserves unresolved elements only when identical to trusted `previous` state (`[builder.rs:BuilderDocument::validate_for_save_with_schemas]`). `builder_blocks` caps `MAX_BUILDER_BLOCK_BODY_BYTES` (384 KiB) and escapes `<`, `>`, `--` inside comment JSON (`[builder_blocks.rs:encode_comment_json]`). `sanitize_richtext` caps input (64 KiB), output (32 KiB), and nesting depth (`MAX_RICHTEXT_DEPTH = 16`), decodes HTML5 entities before checking `<a href>` with `valid_block_url`, and strips active/foreign tags (`[richtext.rs:sanitize_richtext]`).
- **Manifest & Backup Bounds**: `PluginManifest::validate` rejects wildcards in `ScopedFieldGrant` and `plugin_secrets_read`, forbids combining `kv_write`/`kv_delete`, `content_write`, and `content_delete` in a single action, and gates v2/v3 capabilities behind `manifest_schema` and `min_host_version` (`[plugin.rs:PluginManifest::validate]`). `validate_manifest_v1` enforces sorted unique `logical_key` values, flat `entry_name` (`00000001.bin`..), Windows reserved-name rejection, `db/main.sqlite` presence, and size caps (`[backup.rs:validate_manifest_v1]`).

## Side Effects

None. `opendash-core` performs zero I/O, spawns no threads or processes, and mutates only caller-owned in-memory structs.

## Verification

Run the workspace core tests and standards checks from the repository root:

```powershell
cargo test --manifest-path cms/Cargo.toml -p opendash-core --locked
python cms/scripts/check-standards.py
```
