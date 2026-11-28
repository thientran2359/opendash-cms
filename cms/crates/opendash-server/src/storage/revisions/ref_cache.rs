use std::collections::{BTreeMap, BTreeSet};

use opendash_core::block_schema::RegisteredBlockSchema;
use opendash_core::content::EffectiveContentSchema;
use opendash_core::registry::RegistryCatalog;
use opendash_core::theme::ThemeManifest;
use sqlx::SqliteConnection;

use crate::error::ApiResult;
use crate::storage::fields::load_active_theme_manifest_in_conn;
use crate::storage::media::valid_media_id;

pub(super) struct InspectionCaches {
    pub(super) schemas: RegistryCatalog<RegisteredBlockSchema>,
    manifest: Option<ThemeManifest>,
    manifest_loaded: bool,
    active_media: BTreeMap<(String, Option<String>), bool>,
    active_cpts: BTreeMap<String, bool>,
    declared_taxonomies: BTreeMap<String, bool>,
    menus: BTreeMap<String, bool>,
    contact_forms_any: BTreeMap<i64, bool>,
    contact_forms_active: BTreeMap<i64, bool>,
    template_parts: BTreeMap<String, bool>,
    valid_patterns: BTreeMap<i64, bool>,
    live_entries: BTreeMap<i64, Option<(String, String)>>,
    valid_terms: BTreeMap<i64, Option<(String, String)>>,
    pub(super) eligible_taxonomies: BTreeSet<String>,
    exact_schemas: BTreeMap<String, Option<EffectiveContentSchema>>,
}

impl InspectionCaches {
    pub(super) async fn load(conn: &mut SqliteConnection, type_key: &str) -> ApiResult<Self> {
        let schemas = crate::storage::page::load_unified_block_catalog_in_conn(conn).await?;
        let eligible_taxonomies: BTreeSet<String> =
            crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key)
                .await?
                .into_iter()
                .collect();
        Ok(Self {
            schemas,
            manifest: None,
            manifest_loaded: false,
            active_media: BTreeMap::new(),
            active_cpts: BTreeMap::new(),
            declared_taxonomies: BTreeMap::new(),
            menus: BTreeMap::new(),
            contact_forms_any: BTreeMap::new(),
            contact_forms_active: BTreeMap::new(),
            template_parts: BTreeMap::new(),
            valid_patterns: BTreeMap::new(),
            live_entries: BTreeMap::new(),
            valid_terms: BTreeMap::new(),
            eligible_taxonomies,
            exact_schemas: BTreeMap::new(),
        })
    }

    pub(super) async fn ensure_manifest(
        &mut self,
        conn: &mut SqliteConnection,
    ) -> ApiResult<Option<&ThemeManifest>> {
        if !self.manifest_loaded {
            self.manifest = load_active_theme_manifest_in_conn(conn).await?;
            self.manifest_loaded = true;
        }
        Ok(self.manifest.as_ref())
    }

    pub(super) async fn media_ok(
        &mut self,
        conn: &mut SqliteConnection,
        id: &str,
        expected_kind: Option<&str>,
    ) -> ApiResult<bool> {
        if !valid_media_id(id) {
            return Ok(false);
        }
        let key = (id.to_owned(), expected_kind.map(str::to_owned));
        if let Some(&ok) = self.active_media.get(&key) {
            return Ok(ok);
        }
        let ok: i64 = match expected_kind {
            Some(kind) => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = ? AND lifecycle_state = 'active')",
                )
                .bind(id)
                .bind(kind)
                .fetch_one(&mut *conn)
                .await?
            }
            None => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND lifecycle_state = 'active')",
                )
                .bind(id)
                .fetch_one(&mut *conn)
                .await?
            }
        };
        let valid = ok != 0;
        self.active_media.insert(key, valid);
        Ok(valid)
    }

    pub(super) async fn pattern_ok(
        &mut self,
        conn: &mut SqliteConnection,
        id: i64,
    ) -> ApiResult<bool> {
        if id <= 0 {
            return Ok(false);
        }
        if let Some(&ok) = self.valid_patterns.get(&id) {
            return Ok(ok);
        }
        let ok: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE id = ? AND content_type_key = 'pattern' AND trashed_at IS NULL)",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
        let valid = ok != 0;
        self.valid_patterns.insert(id, valid);
        Ok(valid)
    }

    pub(super) async fn form_ok(
        &mut self,
        conn: &mut SqliteConnection,
        id: i64,
        require_active: bool,
    ) -> ApiResult<bool> {
        if id <= 0 {
            return Ok(false);
        }
        let cache = if require_active {
            &mut self.contact_forms_active
        } else {
            &mut self.contact_forms_any
        };
        if let Some(&ok) = cache.get(&id) {
            return Ok(ok);
        }
        let ok: i64 = if require_active {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM contact_forms WHERE id = ? AND status = 'active')",
            )
            .bind(id)
            .fetch_one(&mut *conn)
            .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM contact_forms WHERE id = ?)")
                .bind(id)
                .fetch_one(&mut *conn)
                .await?
        };
        let valid = ok != 0;
        if require_active {
            self.contact_forms_active.insert(id, valid);
        } else {
            self.contact_forms_any.insert(id, valid);
        }
        Ok(valid)
    }

    pub(super) async fn cpt_ok(
        &mut self,
        conn: &mut SqliteConnection,
        key: &str,
    ) -> ApiResult<bool> {
        if key == "pattern" || key == "section" {
            return Ok(false);
        }
        if let Some(&ok) = self.active_cpts.get(key) {
            return Ok(ok);
        }
        let ok: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_types WHERE key = ? AND provider_status = 'active' AND key NOT IN ('pattern', 'section'))",
        )
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
        let valid = ok != 0;
        self.active_cpts.insert(key.to_owned(), valid);
        Ok(valid)
    }

    pub(super) async fn tax_declared(
        &mut self,
        conn: &mut SqliteConnection,
        key: &str,
    ) -> ApiResult<bool> {
        if let Some(&ok) = self.declared_taxonomies.get(key) {
            return Ok(ok);
        }
        let ok: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM taxonomy_definitions WHERE key = ? AND provider_status = 'active' AND declaration_state = 'declared')",
        )
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
        let valid = ok != 0;
        self.declared_taxonomies.insert(key.to_owned(), valid);
        Ok(valid)
    }

    pub(super) async fn term_info(
        &mut self,
        conn: &mut SqliteConnection,
        id: i64,
    ) -> ApiResult<Option<(String, String)>> {
        if id <= 0 {
            return Ok(None);
        }
        if let Some(info) = self.valid_terms.get(&id) {
            return Ok(info.clone());
        }
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT tm.kind, tm.locale
               FROM taxonomy_terms tm
               JOIN taxonomy_definitions d ON d.key = tm.kind
               JOIN site_locales sl ON sl.code = tm.locale AND sl.deleted_at IS NULL
              WHERE tm.id = ?
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        self.valid_terms.insert(id, row.clone());
        Ok(row)
    }

    pub(super) async fn entry_info(
        &mut self,
        conn: &mut SqliteConnection,
        id: i64,
    ) -> ApiResult<Option<(String, String)>> {
        if id <= 0 {
            return Ok(None);
        }
        if let Some(info) = self.live_entries.get(&id) {
            return Ok(info.clone());
        }
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT e.content_type_key, e.locale
               FROM content_entries e
               JOIN content_types ct ON ct.key = e.content_type_key
               JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
              WHERE e.id = ?
                AND e.trashed_at IS NULL
                AND ct.provider_status = 'active'
                AND e.content_type_key NOT IN ('pattern', 'section')",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        self.live_entries.insert(id, row.clone());
        Ok(row)
    }

    pub(super) async fn menu_ok(
        &mut self,
        conn: &mut SqliteConnection,
        key: &str,
    ) -> ApiResult<bool> {
        if let Some(&ok) = self.menus.get(key) {
            return Ok(ok);
        }
        let ok: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM site_menus WHERE menu_key = ?)")
                .bind(key)
                .fetch_one(&mut *conn)
                .await?;
        let valid = ok != 0;
        self.menus.insert(key.to_owned(), valid);
        Ok(valid)
    }

    pub(super) async fn template_part_ok(
        &mut self,
        conn: &mut SqliteConnection,
        key: &str,
    ) -> ApiResult<bool> {
        if let Some(&ok) = self.template_parts.get(key) {
            return Ok(ok);
        }
        let in_manifest = self
            .ensure_manifest(&mut *conn)
            .await?
            .is_some_and(|m| m.parts.contains_key(key));
        let valid = if in_manifest {
            true
        } else {
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM template_overrides WHERE target_kind = 'part' AND canonical_target_key = ?)",
            )
            .bind(format!("part:{key}"))
            .fetch_one(&mut *conn)
            .await?;
            exists != 0
        };
        self.template_parts.insert(key.to_owned(), valid);
        Ok(valid)
    }

    pub(super) async fn schema_for(
        &mut self,
        conn: &mut SqliteConnection,
        type_key: &str,
    ) -> ApiResult<Option<&EffectiveContentSchema>> {
        if !self.exact_schemas.contains_key(type_key) {
            let schema =
                crate::storage::content::load_effective_content_schema_in_tx(conn, type_key)
                    .await?;
            self.exact_schemas.insert(type_key.to_owned(), schema);
        }
        Ok(self.exact_schemas.get(type_key).and_then(|s| s.as_ref()))
    }

    pub(super) async fn public_context_fields_for_cpt(
        &mut self,
        conn: &mut SqliteConnection,
        type_key: &str,
        caller_role: opendash_core::field_schema::FieldRole,
    ) -> ApiResult<Option<Vec<String>>> {
        use opendash_core::content::EffectiveFieldStatus;
        use opendash_core::field_schema::PublicExposure;
        let Some(schema) = self.schema_for(conn, type_key).await? else {
            return Ok(None);
        };
        let keys = schema
            .effective_fields
            .iter()
            .filter(|(_, fe)| {
                fe.effective_status == EffectiveFieldStatus::Active
                    && fe.definition.effective_acl().0 <= caller_role
                    && fe.definition.effective_acl().2 == PublicExposure::Public
            })
            .map(|(k, _)| k.clone())
            .collect();
        Ok(Some(keys))
    }
}
