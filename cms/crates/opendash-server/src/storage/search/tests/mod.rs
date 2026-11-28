use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use opendash_core::field_schema::{FieldAcl, FieldRole, FieldSpec, FieldType, PublicExposure};

mod migration_and_restore;
mod query_and_sync;

static SEQ: AtomicU64 = AtomicU64::new(1);

pub(super) struct TempDirGuard(PathBuf);

impl TempDirGuard {
    pub(super) fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!(
                "opendash-search-{label}-{}-{nanos}-{seq}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn spec(
    field_type: FieldType,
    key: &str,
    label: &str,
    role: FieldRole,
    exposure: PublicExposure,
) -> FieldSpec {
    let mut f = FieldSpec::new(field_type, key, label);
    f.acl = Some(FieldAcl::canonical(role, role, exposure));
    f
}
