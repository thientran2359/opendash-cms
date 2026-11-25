//! S02: exact source tests for curated icons, C-SEC validation and CLI.
//! All CLI fixtures live in temporary directories; no live site is touched.

use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_theme_dir() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("opendash-s02-icons-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_theme(path: &Path, icon: &str) {
    fs::write(path.join("theme.json"), r#"{"manifest_version":3,"theme_id":"safe-icons-test","name":"Safe Icons","version":"1.0.0","parts":{"header":"header.html"},"templates":[{"kind":"page","file":"page.html"}]}"#).unwrap();
    fs::write(
        path.join("header.html"),
        "<header>{{ od_icon('arrow-right') }}</header>",
    )
    .unwrap();
    fs::write(
        path.join("page.html"),
        format!("<main>{icon}{{{{ od_content() }}}}</main>"),
    )
    .unwrap();
}

fn check_theme(path: &Path) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .arg("--check-theme")
        .arg(path)
        .output()
        .unwrap();
    (
        output.status.success(),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn cli_accepts_inline_icon_and_rejects_untrusted_icon_forms() {
    let path = temp_theme_dir();
    write_theme(&path, "{{ od_icon('calendar', 'w-3.5 h-3.5') }}");
    let (ok, output) = check_theme(&path);
    assert!(ok, "theme validation failed: {output}");

    for invalid in [
        "{{ od_icon('not-real') }}",
        "{{ od_icon('calendar', 'a\" onclick=\"x') }}",
        "<div title=\"{{ od_icon('calendar') }}\">bad</div>",
        "{% set captured %}{{ od_icon('calendar') }}{% endset %}",
        "{{ od_icon('calendar') | upper }}",
        "OD_ICON_0123456789abcdef0123456789abcdef",
        "<svg><path d='M0 0'/></svg>",
    ] {
        write_theme(&path, invalid);
        let (ok, output) = check_theme(&path);
        assert!(!ok, "unsafe fixture was accepted: {invalid}: {output}");
    }
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn curated_icon_registry_matches_expected_names_and_safe_markup() {
    assert_eq!(opendash_core::icons::CORE_ICONS.len(), 48);
    for (name, svg) in opendash_core::icons::CORE_ICONS {
        assert!(opendash_core::icons::lookup_core_icon_paths(name).is_some());
        assert!(svg.contains('<'));
        for dangerous in [
            "<script",
            "<svg",
            " onload",
            " style=",
            " href=",
            "foreignObject",
        ] {
            assert!(!svg.contains(dangerous), "invalid static icon {name}");
        }
    }
}
