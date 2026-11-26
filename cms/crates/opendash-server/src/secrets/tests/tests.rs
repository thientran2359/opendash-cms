use super::*;

#[test]
fn secret_store_enforces_acl_roundtrip_and_rejects_bad_ref() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("opendash-secret-test-{nanos}"));
    let store = SecretStore::new(dir.join("smtp"));
    store.verify_native_acl().expect("native ACL probe");

    let r = store.write_smtp_secret("p@ssw0rd!").expect("write secret");
    assert_eq!(r.as_str().len(), 64);
    assert_eq!(store.read_smtp_secret(r.as_str()).unwrap(), "p@ssw0rd!");
    assert!(SecretRef::parse("../escape").is_err());
    assert!(SecretRef::parse("short").is_err());
    assert!(store.write_smtp_secret(&"a".repeat(1025)).is_err());

    store.remove_secret_blob(r.as_str());
    assert!(store.read_smtp_secret(r.as_str()).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn plugin_secret_store_roundtrip_limit_and_removal() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("opendash-plugin-secret-test-{nanos}"));
    let store = SecretStore::for_plugin_dir(dir.join("plugins").join("acme"));

    let secret_ref = store
        .write_plugin_secret("plg-secret-value")
        .expect("write plugin secret");
    assert_eq!(secret_ref.as_str().len(), 64);
    assert_eq!(
        store.read_plugin_secret(secret_ref.as_str()).unwrap(),
        "plg-secret-value"
    );
    assert!(
        store
            .write_plugin_secret(&"x".repeat(MAX_PLUGIN_SECRET_BYTES + 1))
            .is_err()
    );

    store.remove_plugin_secret(secret_ref.as_str());
    assert!(store.read_plugin_secret(secret_ref.as_str()).is_err());
    let _ = fs::remove_dir_all(&dir);
}
