use super::*;

#[test]
fn startup_guards_reject_safe_mode_and_remote_origin_without_override() {
    let mut cfg = PortableRuntimeConfigV1::default();
    assert!(validate_startup_guards(false, &cfg, false).is_ok());
    assert!(validate_startup_guards(true, &cfg, false).is_ok());

    cfg.recovery_safe_mode = true;
    assert!(validate_startup_guards(true, &cfg, true).is_err());

    cfg.recovery_safe_mode = false;
    cfg.trusted_public_origin = Some("https://cms.example.com".into());
    assert!(validate_startup_guards(true, &cfg, false).is_err());
    assert!(validate_startup_guards(true, &cfg, true).is_ok());
}

#[test]
fn safe_resolve_rejects_hidden_env_node_modules_and_symlinks() {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "opendash-dev-mode-unit-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join(".env"), "SECRET=1").unwrap();
    fs::write(root.join("node_modules/pkg/index.js"), "x").unwrap();
    fs::write(root.join("assets/style.css"), "body{}").unwrap();
    let canonical = fs::canonicalize(&root).unwrap();

    assert!(safe_resolve_under_root(&canonical, ".env").is_err());
    assert!(safe_resolve_under_root(&canonical, "node_modules/pkg/index.js").is_err());
    assert!(safe_resolve_under_root(&canonical, "../etc/passwd").is_err());
    assert!(safe_resolve_under_root(&canonical, "assets/style.css").is_ok());

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            canonical.join("assets/style.css"),
            canonical.join("assets/link.css"),
        )
        .unwrap();
        assert!(safe_resolve_under_root(&canonical, "assets/link.css").is_err());
    }

    let _ = fs::remove_dir_all(&root);
}
