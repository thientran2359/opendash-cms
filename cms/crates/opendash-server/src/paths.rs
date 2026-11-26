use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Centralized runtime paths for an OpenDash website instance.
///
/// Resolves `OPENDASH_SITE_ROOT` and `OPENDASH_DB_PATH` once so database,
/// theme/plugin packages, uploads, backups, and same-volume staging paths
/// remain consistent across server startup, CLI workers, and smoke tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePaths {
    site_root: PathBuf,
    db_path: PathBuf,
}

impl SitePaths {
    /// Resolves website paths from the executable location and environment overrides.
    pub fn from_env() -> io::Result<Self> {
        let current_exe = std::env::current_exe()?;
        let executable_dir = current_exe.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "executable has no parent directory",
            )
        })?;
        Ok(Self::resolve_from(
            std::env::var_os("OPENDASH_SITE_ROOT"),
            std::env::var_os("OPENDASH_DB_PATH"),
            executable_dir,
        ))
    }

    pub(crate) fn resolve_from(
        configured_site_root: Option<OsString>,
        configured_db_path: Option<OsString>,
        executable_dir: &Path,
    ) -> Self {
        let site_root = configured_site_root
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_site_root(executable_dir).to_path_buf());
        let db_path = match configured_db_path.filter(|value| !value.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => site_root.join("database").join("opendash.sqlite"),
        };

        Self { site_root, db_path }
    }

    #[allow(dead_code)]
    pub fn site_root(&self) -> &Path {
        &self.site_root
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    #[allow(dead_code)]
    pub fn database_dir(&self) -> PathBuf {
        self.site_root.join("database")
    }

    #[allow(dead_code)]
    pub fn secrets_dir(&self) -> PathBuf {
        self.database_dir().join("secrets")
    }

    #[allow(dead_code)]
    pub fn smtp_secrets_dir(&self) -> PathBuf {
        self.secrets_dir().join("smtp")
    }

    #[allow(dead_code)]
    pub fn plugin_secrets_dir(&self, plugin_id: &str) -> PathBuf {
        self.secrets_dir().join("plugins").join(plugin_id)
    }

    #[allow(dead_code)]
    pub fn theme_secrets_dir(&self, theme_id: &str) -> PathBuf {
        self.secrets_dir().join("themes").join(theme_id)
    }

    #[allow(dead_code)]
    pub fn themes_dir(&self) -> PathBuf {
        self.site_root.join("themes")
    }

    #[allow(dead_code)]
    pub fn plugins_dir(&self) -> PathBuf {
        self.site_root.join("plugins")
    }

    #[allow(dead_code)]
    pub fn uploads_dir(&self) -> PathBuf {
        self.site_root.join("uploads")
    }

    #[allow(dead_code)]
    pub fn backups_dir(&self) -> PathBuf {
        self.site_root.join("backups")
    }

    #[allow(dead_code)]
    pub fn backups_staging_dir(&self) -> PathBuf {
        self.backups_dir().join(".staging")
    }

    #[allow(dead_code)]
    pub fn backups_artifacts_dir(&self) -> PathBuf {
        self.backups_dir().join("artifacts")
    }

    #[allow(dead_code)]
    pub fn backups_restored_dir(&self) -> PathBuf {
        self.backups_dir().join("restored")
    }

    #[allow(dead_code)]
    pub fn config_dir(&self) -> PathBuf {
        self.site_root.join("config")
    }

    #[allow(dead_code)]
    pub fn site_config_path(&self) -> PathBuf {
        self.config_dir().join("site.json")
    }

    /// Same-volume staging directory inside `site_root` for ZIP extraction
    /// and atomic swap operations on Windows and Linux.
    #[allow(dead_code)]
    pub fn staging_dir(&self) -> PathBuf {
        self.site_root.join("runtime").join("staging")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.site_root.join("runtime")
    }

    pub fn site_lock_path(&self) -> PathBuf {
        self.runtime_dir().join("site.lock")
    }

    pub fn db_lock_path(&self) -> PathBuf {
        let parent = self
            .db_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = self
            .db_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("opendash.sqlite"));
        let mut lock_name = file_name.to_os_string();
        lock_name.push(".opendash.lock");
        parent.join(lock_name)
    }

    /// Returns canonical lock paths sorted deterministically to prevent lock-order inversion.
    pub fn ordered_lock_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.site_lock_path(), self.db_lock_path()];
        paths.sort();
        paths.dedup();
        paths
    }

    #[allow(dead_code)]
    pub fn from_parts(site_root: PathBuf, db_path: PathBuf) -> Self {
        Self { site_root, db_path }
    }

    /// Resolves relative paths against cwd, rejects symlinks/reparse points on roots and ancestors,
    /// ensures runtime/db parent directories exist, and canonicalizes `site_root` and `db_path` parent.
    pub fn canonicalize_for_runtime(&self) -> io::Result<Self> {
        let cwd = std::env::current_dir()?;
        let abs_site_root = if self.site_root.is_absolute() {
            self.site_root.clone()
        } else {
            cwd.join(&self.site_root)
        };
        let abs_db_path = if self.db_path.is_absolute() {
            self.db_path.clone()
        } else {
            cwd.join(&self.db_path)
        };

        reject_reparse_or_symlink_ancestors(&abs_site_root)?;
        reject_reparse_or_symlink_ancestors(&abs_db_path)?;

        fs::create_dir_all(&abs_site_root)?;
        reject_reparse_or_symlink_entry(&abs_site_root)?;

        let canonical_site_root = fs::canonicalize(&abs_site_root)?;
        let runtime_dir = canonical_site_root.join("runtime");
        fs::create_dir_all(&runtime_dir)?;
        reject_reparse_or_symlink_entry(&runtime_dir)?;

        let db_file_name = abs_db_path
            .file_name()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "db_path must have a file name")
            })?
            .to_os_string();
        let db_parent = abs_db_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "db_path must have a parent directory",
                )
            })?;

        fs::create_dir_all(db_parent)?;
        reject_reparse_or_symlink_entry(db_parent)?;
        let canonical_db_parent = fs::canonicalize(db_parent)?;
        let canonical_db_path = canonical_db_parent.join(db_file_name);

        if let Ok(meta) = fs::symlink_metadata(&canonical_db_path) {
            if is_symlink_or_reparse(&meta) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "symlink or reparse point not allowed at db_path: {}",
                        canonical_db_path.display()
                    ),
                ));
            }
            if meta.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "db_path points to a directory: {}",
                        canonical_db_path.display()
                    ),
                ));
            }
        }

        Ok(Self {
            site_root: canonical_site_root,
            db_path: canonical_db_path,
        })
    }
}

fn is_symlink_or_reparse(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return true;
        }
    }
    false
}

fn reject_reparse_or_symlink_entry(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if is_symlink_or_reparse(&meta) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("symlink or reparse point not allowed: {}", path.display()),
                ));
            }
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn reject_reparse_or_symlink_ancestors(path: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if matches!(
            component,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        ) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) => {
                if is_symlink_or_reparse(&meta) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "symlink or reparse point not allowed in path: {}",
                            current.display()
                        ),
                    ));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => break,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Lists installed package directories (such as `themes/<id>` or `plugins/<id>`),
/// ignoring top-level files like `README.md` and hidden directories starting with `.`.
#[allow(dead_code)]
pub fn list_package_dirs(root: &Path) -> io::Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        dirs.push(entry.path());
    }
    dirs.sort();
    Ok(dirs)
}

fn default_site_root(executable_dir: &Path) -> &Path {
    // Cargo builds live under <site>/cms/target/<profile> (or <triple>/<profile>).
    for directory in executable_dir.ancestors() {
        if directory.ends_with("cms/target")
            && let Some(cms) = directory.parent()
            && cms.join("Cargo.toml").is_file()
            && let Some(site) = cms.parent()
        {
            return site;
        }
    }
    executable_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempTestDir(PathBuf);

    impl TempTestDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "opendash-paths-{prefix}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temp test dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_default_site_subdirectories_and_same_volume_staging() {
        let temp = TempTestDir::new("defaults");
        let paths = SitePaths::resolve_from(
            Some(temp.path().as_os_str().to_owned()),
            None,
            Path::new("/unused"),
        );

        assert_eq!(paths.site_root(), temp.path());
        assert_eq!(
            paths.db_path(),
            temp.path().join("database").join("opendash.sqlite")
        );
        assert_eq!(paths.database_dir(), temp.path().join("database"));
        assert_eq!(paths.themes_dir(), temp.path().join("themes"));
        assert_eq!(paths.plugins_dir(), temp.path().join("plugins"));
        assert_eq!(
            paths.plugin_secrets_dir("demo-plugin"),
            temp.path()
                .join("database")
                .join("secrets")
                .join("plugins")
                .join("demo-plugin")
        );
        assert_eq!(paths.uploads_dir(), temp.path().join("uploads"));
        assert_eq!(paths.backups_dir(), temp.path().join("backups"));
        assert_eq!(
            paths.staging_dir(),
            temp.path().join("runtime").join("staging")
        );
    }

    #[test]
    fn explicit_db_path_overrides_default_database_file() {
        let temp = TempTestDir::new("db-override");
        let custom_db = temp.path().join("custom").join("test.sqlite");
        let paths = SitePaths::resolve_from(
            Some(temp.path().as_os_str().to_owned()),
            Some(custom_db.as_os_str().to_owned()),
            Path::new("/unused"),
        );

        assert_eq!(paths.site_root(), temp.path());
        assert_eq!(paths.db_path(), custom_db);
    }

    #[test]
    fn detects_site_root_for_workspace_build() {
        let temp = TempTestDir::new("workspace-build");
        let cms_dir = temp.path().join("cms");
        fs::create_dir_all(&cms_dir).expect("create cms dir");
        fs::write(cms_dir.join("Cargo.toml"), "[workspace]").expect("write workspace manifest");

        let paths = SitePaths::resolve_from(None, None, &cms_dir.join("target/debug"));
        assert_eq!(paths.site_root(), temp.path());
    }

    #[test]
    fn fresh_install_uses_executable_directory_when_launched_from_elsewhere() {
        let install = TempTestDir::new("fresh-install");
        let paths = SitePaths::resolve_from(None, None, install.path());
        assert_eq!(
            paths.db_path(),
            install.path().join("database/opendash.sqlite")
        );
    }

    #[test]
    fn canonicalize_for_runtime_normalizes_paths_and_orders_locks() {
        let temp = TempTestDir::new("canon");
        let nested_root = temp.path().join("site").join(".").join("instance");
        let ext_db = temp.path().join("external").join("custom.sqlite");
        let raw = SitePaths::from_parts(nested_root, ext_db);

        let canon = raw
            .canonicalize_for_runtime()
            .expect("canonicalize runtime paths");
        assert!(canon.runtime_dir().exists());
        assert_eq!(
            canon.site_lock_path(),
            canon.runtime_dir().join("site.lock")
        );
        assert!(
            canon
                .db_lock_path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "custom.sqlite.opendash.lock")
        );
        let ordered = canon.ordered_lock_paths();
        assert_eq!(ordered.len(), 2);
        assert!(ordered[0] < ordered[1]);
    }

    #[test]
    fn list_package_dirs_ignores_readme_and_hidden_entries() {
        let temp = TempTestDir::new("packages");
        let themes_dir = temp.path().join("themes");
        fs::create_dir_all(themes_dir.join("ocean")).expect("create ocean theme dir");
        fs::create_dir_all(themes_dir.join("aurora")).expect("create aurora theme dir");
        fs::create_dir_all(themes_dir.join(".staging")).expect("create hidden dir");
        fs::write(themes_dir.join("README.md"), "# Themes\n").expect("write README.md");

        let packages = list_package_dirs(&themes_dir).expect("list package dirs");
        assert_eq!(
            packages,
            vec![themes_dir.join("aurora"), themes_dir.join("ocean")]
        );
    }
}
