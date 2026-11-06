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

    /// Same-volume staging directory inside `site_root` for ZIP extraction
    /// and atomic swap operations on Windows and Linux.
    #[allow(dead_code)]
    pub fn staging_dir(&self) -> PathBuf {
        self.site_root.join("runtime").join("staging")
    }
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
