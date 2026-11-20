# OpenDash Core (`opendash-core`)

Thư viện domain thuần Rust (chỉ phụ thuộc `serde`, `serde_json`, `url`; tuyệt đối không chứa HTTP, SQLite, filesystem I/O hoặc Wasmi). Cấu trúc [`src/lib.rs`](src/lib.rs) gồm 10 modules:

- `id.rs`: Kiểm tra định danh (`SnakeKey`, `PackageId`, `NamespacedBlockId`, `NamespacedSnakeKey`, `SemVer`, `ScopedResourceId`, `valid_layout_slice_panel_id`).
- `registry.rs`: Bộ đăng ký gắn quyền sở hữu (`ResourceOwner`, `ProviderStatus`, `DeclarationState`, `OwnedDescriptor<T>`, `RegistryCatalog<T>`).
- `hooks.rs`: `HookEvent`, `HookExecutionMode`, `ContentBeforeSaveDecision`, `HookEnvelope` và `sanitize_plain_text`.
- `content.rs`: `ContentTypeDefinition`, `ContentTypeSettings`, 9 loại custom fields, `FieldGroupRecord`, `EditorLayoutConfigV1`, `PanelDefinition`, `TaxonomyDefinition`, `PatchField<T>`, `schema_token` và entry validation.
- `builder.rs`: Mô hình cây `BuilderDocument` 4 tầng (`Section → Row → Column → Element`), responsive styles 3 viewports, `BindingSource` (`FixedEntry`, `RouteContext`), `ContentList`, `ContactForm`, `Video`.
- `theme.rs`: Kiểu `Block`, `Fields` và kiểm tra hợp lệ cho Theme packages.
- `plugin.rs`: `PluginManifest` (v1/v2), `PluginPermissions`, `ScopedFieldGrant`, `NetworkScopeGrant`, `PluginActionDeclaration`.
- `admin_extensions.rs`: `AdminExtensions`, `AdminColumn`, `AdminEntryAction`, `AdminTermPanel`.
- `contact.rs`: Schema và validation cho Contact Form V1.
- `backup.rs`: `BackupManifestV1`, `PortableRuntimeConfigV1`, `BackupFileKind` và hợp đồng snapshot/restore.

Xem ranh giới chi tiết tại [Core Foundation](../../../docs/contracts/CORE_FOUNDATION.md) và [Kiến trúc CMS](../../../docs/architecture/CMS_ARCHITECTURE.md).
