use super::*;

#[test]
fn check_theme_path_validates_dir_zip_and_security_boundaries() {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let starter_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtin_themes/opendash-starter");
    let summary = check_theme_path(&starter_dir).expect("opendash-starter must pass");
    assert!(summary.ok);
    assert_eq!(summary.theme_id, "opendash-starter");
    assert!(!summary.templates.is_empty());
    assert!(summary.parts.windows(2).all(|w| w[0] <= w[1]));
    assert!(summary.templates.windows(2).all(|w| w[0] <= w[1]));

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_root = std::env::temp_dir().join(format!(
        "opendash-check-theme-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp_root).unwrap();

    // Build minimal valid theme dir + test binary zip with non-.zip extension.
    let valid_dir = tmp_root.join("valid-theme");
    std::fs::create_dir_all(valid_dir.join("assets")).unwrap();
    let manifest_bytes = br#"{"manifest_version":3,"theme_id":"cli-sample","name":"CLI Sample","version":"1.0.0","templates":[{"kind":"index","file":"index.html"}],"assets":["assets/main.css"],"options":[{"key":"cta","label":"CTA","type":"text","default":"Hi"}]}"#;
    let index_bytes = b"<main><h1>{{ site.title }}</h1>{% for e in entries %}<a href=\"{{ e.url }}\">{{ e.title }}</a>{% endfor %}</main>";
    let css_bytes = b"body { color: #111; }";
    std::fs::write(valid_dir.join("theme.json"), manifest_bytes).unwrap();
    std::fs::write(valid_dir.join("index.html"), index_bytes).unwrap();
    std::fs::write(valid_dir.join("assets/main.css"), css_bytes).unwrap();
    std::fs::write(valid_dir.join("README.md"), b"# Sample Theme Docs\n").unwrap();

    let dir_summary = check_theme_path(&valid_dir).expect("valid dir passes");
    assert_eq!(dir_summary.theme_id, "cli-sample");
    assert_eq!(dir_summary.options_count, 1);
    assert_eq!(dir_summary.assets, vec!["assets/main.css"]);

    // Subdirectory README.md must still be rejected by offline CLI check.
    let sub_readme_dir = tmp_root.join("sub-readme-theme");
    std::fs::create_dir_all(sub_readme_dir.join("assets")).unwrap();
    std::fs::write(sub_readme_dir.join("theme.json"), manifest_bytes).unwrap();
    std::fs::write(sub_readme_dir.join("index.html"), index_bytes).unwrap();
    std::fs::write(sub_readme_dir.join("assets/main.css"), css_bytes).unwrap();
    std::fs::write(sub_readme_dir.join("assets/README.md"), b"# Not Allowed\n").unwrap();
    assert!(check_theme_path(&sub_readme_dir).is_err());

    // ZIP package without `.zip` extension must still validate via magic/file read.
    let bin_zip_path = tmp_root.join("cli-sample.bundle");
    {
        let f = std::fs::File::create(&bin_zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        zw.start_file("theme.json", opts).unwrap();
        zw.write_all(manifest_bytes).unwrap();
        zw.start_file("index.html", opts).unwrap();
        zw.write_all(index_bytes).unwrap();
        zw.start_file("assets/main.css", opts).unwrap();
        zw.write_all(css_bytes).unwrap();
        zw.finish().unwrap();
    }
    let zip_summary = check_theme_path(&bin_zip_path).expect("non-.zip zip passes");
    assert_eq!(zip_summary.theme_id, "cli-sample");

    // Hidden file rejection.
    let hidden_dir = tmp_root.join("hidden-theme");
    std::fs::create_dir_all(&hidden_dir).unwrap();
    std::fs::write(hidden_dir.join("theme.json"), manifest_bytes).unwrap();
    std::fs::write(hidden_dir.join(".env"), b"SECRET=1").unwrap();
    assert!(check_theme_path(&hidden_dir).is_err());

    // Depth > MAX_THEME_DIR_DEPTH (16) rejection.
    let deep_dir = tmp_root.join("deep-theme");
    let mut nested = deep_dir.clone();
    for i in 0..=16 {
        nested = nested.join(format!("d{i}"));
    }
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(deep_dir.join("theme.json"), manifest_bytes).unwrap();
    assert!(check_theme_path(&deep_dir).is_err());

    // Oversized text file rejection.
    let big_dir = tmp_root.join("big-theme");
    std::fs::create_dir_all(&big_dir).unwrap();
    std::fs::write(big_dir.join("theme.json"), manifest_bytes).unwrap();
    std::fs::write(
        big_dir.join("index.html"),
        vec![b'x'; V2_MAX_TEXT_BYTES + 1],
    )
    .unwrap();
    assert!(check_theme_path(&big_dir).is_err());

    // Oversized ZIP rejection.
    let big_zip = tmp_root.join("big.zip");
    std::fs::write(&big_zip, vec![0u8; V2_MAX_ZIP_BYTES + 1]).unwrap();
    assert!(check_theme_path(&big_zip).is_err());

    // Internal symlink rejection (Unix).
    #[cfg(unix)]
    {
        let sym_dir = tmp_root.join("sym-theme");
        std::fs::create_dir_all(&sym_dir).unwrap();
        std::fs::write(sym_dir.join("theme.json"), manifest_bytes).unwrap();
        std::os::unix::fs::symlink(valid_dir.join("index.html"), sym_dir.join("index.html"))
            .unwrap();
        assert!(check_theme_path(&sym_dir).is_err());
    }

    // Unsafe C-SEC template rejection.
    let bad_csec = tmp_root.join("bad-csec");
    std::fs::create_dir_all(&bad_csec).unwrap();
    std::fs::write(
        bad_csec.join("theme.json"),
        br#"{"manifest_version":3,"theme_id":"bad-csec","name":"Bad","version":"1.0.0","templates":[{"kind":"index","file":"index.html"}]}"#,
    )
    .unwrap();
    std::fs::write(bad_csec.join("index.html"), b"<iframe>unsafe</iframe>").unwrap();
    assert!(check_theme_path(&bad_csec).is_err());

    let _ = std::fs::remove_dir_all(&tmp_root);
}
