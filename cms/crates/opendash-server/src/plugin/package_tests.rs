use super::*;

use super::*;
use std::io::Write;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn make_zip(entries: &[(String, Vec<u8>)], with_directories: bool) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let manifest = br#"{"id":"sample-plugin","version":"1.0.0","api_version":1,"name":"Sample","entrypoint":"plugin.wasm","permissions":{}}"#;
        let wasm = wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"opendash_run\") (result i32) (i32.const 0)))").unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(&wasm).unwrap();
        if with_directories {
            zip.add_directory("blocks/", opts).unwrap();
            zip.add_directory("blocks/notice/", opts).unwrap();
        }
        for (name, bytes) in entries {
            zip.start_file(name.as_str(), opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buffer
}

fn schema(slug: &str, owner: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "id":format!("{owner}/{slug}"),
        "title":"Notice", "category":"content", "render":"render.html",
        "fields": []
    }))
    .unwrap()
}

fn pair(slug: &str) -> Vec<(String, Vec<u8>)> {
    vec![
        (
            format!("blocks/{slug}/block.json"),
            schema(slug, "sample-plugin"),
        ),
        (
            format!("blocks/{slug}/render.html"),
            b"<p>Safe</p>".to_vec(),
        ),
    ]
}

#[test]
fn zip_accepts_paired_blocks_and_empty_directory_entries() {
    let package = PluginPackage::from_zip(
        &make_zip(&pair("notice"), true),
        crate::plugin::runtime::PluginRuntime::new().engine(),
    )
    .expect("paired plugin block");
    assert_eq!(package.blocks.len(), 1);
    assert_eq!(package.blocks[0].0.id, "sample-plugin/notice");
}

#[test]
fn zip_rejects_missing_pair_case_collisions_reserved_and_unsafe_templates() {
    let engine = crate::plugin::runtime::PluginRuntime::new();
    let mut missing_pair = pair("notice");
    missing_pair.pop();
    assert!(PluginPackage::from_zip(&make_zip(&missing_pair, false), engine.engine()).is_err());

    let mut case_collision = pair("notice");
    case_collision.push((
        "blocks/Notice/block.json".into(),
        schema("notice", "sample-plugin"),
    ));
    assert!(PluginPackage::from_zip(&make_zip(&case_collision, false), engine.engine()).is_err());

    let mut wrong_owner = pair("notice");
    wrong_owner[0].1 = schema("notice", "core");
    assert!(PluginPackage::from_zip(&make_zip(&wrong_owner, false), engine.engine()).is_err());

    let mut unsafe_html = pair("notice");
    // S02 permits static trusted scripts in Declarative Blocks; iframe
    // remains an unsafe element and must still be rejected at install.
    unsafe_html[1].1 = b"<iframe src='/'></iframe>".to_vec();
    assert!(PluginPackage::from_zip(&make_zip(&unsafe_html, false), engine.engine()).is_err());

    let mut traversal = pair("notice");
    traversal[1].0 = "blocks/../evil.html".into();
    assert!(PluginPackage::from_zip(&make_zip(&traversal, false), engine.engine()).is_err());
}

#[test]
fn zip_rejects_too_many_files_and_blocks() {
    let engine = crate::plugin::runtime::PluginRuntime::new();
    let mut entries = Vec::new();
    for i in 0..36 {
        entries.extend(pair(&format!("notice-{i}")));
    }
    assert!(PluginPackage::from_zip(&make_zip(&entries, false), engine.engine()).is_err());
    entries.extend(pair("notice-extra"));
    assert!(PluginPackage::from_zip(&make_zip(&entries, false), engine.engine()).is_err());
}

#[test]
fn check_plugin_path_validates_dir_zip_and_security_boundaries() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_root = std::env::temp_dir().join(format!(
        "opendash-check-plugin-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp_root).unwrap();

    let wasm_abi_v1_1 = wat::parse_str(
        r#"(module
            (import "opendash_v1" "host_get_input_len" (func (result i32)))
            (import "opendash_v1" "host_read_input" (func (param i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func (param i32 i32) (result i32)))
            (import "opendash_v1" "host_call" (func (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_last_resp_len" (func (result i32)))
            (import "opendash_v1" "host_read_last_resp" (func (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32) (i32.const 0))
        )"#,
    )
    .unwrap();

    let manifest_v3 = serde_json::json!({
        "id": "cli-sample-plugin",
        "version": "1.2.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "CLI Sample",
        "entrypoint": "plugin.wasm",
        "hooks": ["content/after_publish", "content/after_delete"],
        "filters": ["filter:seo/head", "filter:entry/view"],
        "actions": [
            { "id": "sync-now", "title": "Sync", "capabilities": ["log"] },
            { "id": "audit-check", "title": "Audit", "capabilities": [] }
        ],
        "settings_schema": [
            { "key": "api_key", "label": "Key", "type": "secret" },
            { "key": "mode", "label": "Mode", "type": "text", "default": "fast" }
        ],
        "permissions": {}
    })
    .to_string();

    // 1. Valid plugin directory with paired blocks & ABI v1.1 imports.
    let valid_dir = tmp_root.join("valid-plugin");
    std::fs::create_dir_all(valid_dir.join("blocks/zebra")).unwrap();
    std::fs::create_dir_all(valid_dir.join("blocks/alpha")).unwrap();
    std::fs::write(valid_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
    std::fs::write(valid_dir.join("plugin.wasm"), &wasm_abi_v1_1).unwrap();
    std::fs::write(valid_dir.join("README.md"), b"# Sample").unwrap();
    std::fs::write(
        valid_dir.join("blocks/zebra/block.json"),
        schema("zebra", "cli-sample-plugin"),
    )
    .unwrap();
    std::fs::write(valid_dir.join("blocks/zebra/render.html"), b"<p>Z</p>").unwrap();
    std::fs::write(
        valid_dir.join("blocks/alpha/block.json"),
        schema("alpha", "cli-sample-plugin"),
    )
    .unwrap();
    std::fs::write(valid_dir.join("blocks/alpha/render.html"), b"<p>A</p>").unwrap();

    let dir_summary = check_plugin_path(&valid_dir).expect("valid v3 dir passes");
    assert!(dir_summary.ok);
    assert_eq!(dir_summary.plugin_id, "cli-sample-plugin");
    assert_eq!(dir_summary.version, "1.2.0");
    assert_eq!(dir_summary.manifest_schema, 3);
    assert_eq!(
        dir_summary.hooks,
        vec!["content/after_delete", "content/after_publish"]
    );
    assert_eq!(
        dir_summary.filters,
        vec!["filter:entry/view", "filter:seo/head"]
    );
    assert_eq!(dir_summary.actions, vec!["audit-check", "sync-now"]);
    assert_eq!(
        dir_summary.blocks,
        vec!["cli-sample-plugin/alpha", "cli-sample-plugin/zebra"]
    );
    assert_eq!(dir_summary.settings_count, 2);

    // 2. Non-.zip archive extension is inspected as ZIP by file content.
    let bundle_path = tmp_root.join("cli-sample.bundle");
    {
        let f = std::fs::File::create(&bundle_path).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(manifest_v3.as_bytes()).unwrap();
        zw.start_file("plugin.wasm", opts).unwrap();
        zw.write_all(&wasm_abi_v1_1).unwrap();
        zw.finish().unwrap();
    }
    let bundle_summary = check_plugin_path(&bundle_path).expect("non-.zip bundle passes");
    assert_eq!(bundle_summary.plugin_id, "cli-sample-plugin");
    assert!(bundle_summary.blocks.is_empty());

    // 3. Hidden entry rejection.
    let hidden_dir = tmp_root.join("hidden-plugin");
    std::fs::create_dir_all(&hidden_dir).unwrap();
    std::fs::write(hidden_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
    std::fs::write(hidden_dir.join("plugin.wasm"), &wasm_abi_v1_1).unwrap();
    std::fs::write(hidden_dir.join(".env"), b"SECRET=1").unwrap();
    assert!(check_plugin_path(&hidden_dir).is_err());

    // 4. Depth > MAX_PLUGIN_DIR_DEPTH (4) rejection.
    let deep_dir = tmp_root.join("deep-plugin");
    let mut nested = deep_dir.clone();
    for i in 0..=MAX_PLUGIN_DIR_DEPTH {
        nested = nested.join(format!("d{i}"));
    }
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(deep_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
    std::fs::write(deep_dir.join("plugin.wasm"), &wasm_abi_v1_1).unwrap();
    assert!(check_plugin_path(&deep_dir).is_err());

    // 5. Oversized manifest rejection.
    let big_manifest_dir = tmp_root.join("big-manifest-plugin");
    std::fs::create_dir_all(&big_manifest_dir).unwrap();
    std::fs::write(
        big_manifest_dir.join("manifest.json"),
        vec![b' '; MAX_MANIFEST_BYTES + 1],
    )
    .unwrap();
    std::fs::write(big_manifest_dir.join("plugin.wasm"), &wasm_abi_v1_1).unwrap();
    assert!(check_plugin_path(&big_manifest_dir).is_err());

    // 6. Oversized ZIP rejection.
    let big_zip = tmp_root.join("big-plugin.zip");
    std::fs::write(&big_zip, vec![0u8; MAX_PLUGIN_ZIP_BYTES + 1]).unwrap();
    assert!(check_plugin_path(&big_zip).is_err());

    // 7. Unsafe block C-SEC template rejection.
    let unsafe_block_dir = tmp_root.join("unsafe-block-plugin");
    std::fs::create_dir_all(unsafe_block_dir.join("blocks/notice")).unwrap();
    std::fs::write(
        unsafe_block_dir.join("manifest.json"),
        manifest_v3.as_bytes(),
    )
    .unwrap();
    std::fs::write(unsafe_block_dir.join("plugin.wasm"), &wasm_abi_v1_1).unwrap();
    std::fs::write(
        unsafe_block_dir.join("blocks/notice/block.json"),
        schema("notice", "cli-sample-plugin"),
    )
    .unwrap();
    std::fs::write(
        unsafe_block_dir.join("blocks/notice/render.html"),
        b"<iframe src='/'></iframe>",
    )
    .unwrap();
    assert!(check_plugin_path(&unsafe_block_dir).is_err());

    // 8. Disallowed WASI import rejection.
    let wasi_dir = tmp_root.join("wasi-plugin");
    std::fs::create_dir_all(&wasi_dir).unwrap();
    std::fs::write(wasi_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
    let wasi_wasm = wat::parse_str(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32) (i32.const 0))
        )"#,
    )
    .unwrap();
    std::fs::write(wasi_dir.join("plugin.wasm"), wasi_wasm).unwrap();
    assert!(check_plugin_path(&wasi_dir).is_err());

    // 9. Internal symlink rejection (Unix).
    #[cfg(unix)]
    {
        let sym_dir = tmp_root.join("sym-plugin");
        std::fs::create_dir_all(&sym_dir).unwrap();
        std::fs::write(sym_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
        std::os::unix::fs::symlink(valid_dir.join("plugin.wasm"), sym_dir.join("plugin.wasm"))
            .unwrap();
        assert!(check_plugin_path(&sym_dir).is_err());
    }

    let _ = std::fs::remove_dir_all(&tmp_root);
}
