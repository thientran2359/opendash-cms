//! S02 typed resolver/migration regression tests.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use opendash_core::{
    builder::BuilderDocument,
    theme::{TemplateCandidate, TemplateCandidateKind as Kind},
};
use serde_json::json;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::storage::{
    init_db, media_refs,
    overrides::{
        PublicTemplateContext, TemplateResolution, build_candidates, get_typed_override,
        reset_typed_override, resolve_template_2axis, save_typed_override,
    },
    public_content,
    theme::{ThemeSourceFormat, list},
};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opendash-s02-theme-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn connect_file_pool(db_path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap()
}

fn target(kind: Kind) -> TemplateCandidate {
    TemplateCandidate::new(kind)
}

fn one_part(part_key: &str) -> BuilderDocument {
    serde_json::from_value(json!({
        "schema_version":1,
        "blocks":[{
            "id":"section-main",
            "block_id":"core/section",
            "children":[{
                "id":"row-main",
                "block_id":"core/row",
                "children":[{
                    "id":"column-main",
                    "block_id":"core/column",
                    "children":[{
                        "id":"part-node",
                        "block_id":"core/template-part",
                        "props":{"part_key":part_key}
                    }]
                }]
            }]
        }]
    }))
    .unwrap()
}

async fn insert_override(db: &SqlitePool, target: &TemplateCandidate) -> i64 {
    let kind = serde_json::to_value(target.kind)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let res = sqlx::query(
        "INSERT INTO template_overrides
         (target_kind,canonical_target_key,target_json,ast_json,version)
         VALUES (?,?,?,?,1)",
    )
    .bind(kind)
    .bind(target.canonical_key().unwrap())
    .bind(serde_json::to_string(target).unwrap())
    .bind(serde_json::to_string(&BuilderDocument::empty()).unwrap())
    .execute(db)
    .await
    .unwrap();
    res.last_insert_rowid()
}

mod suite_01;
mod suite_02;
