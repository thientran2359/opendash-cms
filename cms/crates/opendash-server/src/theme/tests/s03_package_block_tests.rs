use super::*;

fn block_schema(owner: &str, slug: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema_version": 1, "id": format!("{owner}/{slug}"),
        "title": "Banner", "category": "content", "render": "render.html", "fields": []
    }))
    .unwrap()
}

fn file_map() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("theme.json".into(), br#"{"manifest_version":3,"theme_id":"sample-theme","name":"Sample","version":"1.0.0"}"#.to_vec()),
        ("blocks/banner/block.json".into(), block_schema("sample-theme", "banner")),
        ("blocks/banner/render.html".into(), b"<p>Hello</p>".to_vec()),
    ])
}

#[test]
fn v3_discovers_valid_blocks_from_unlisted_manifest_paths() {
    let parsed = validate_theme_v3_files(file_map()).unwrap();
    assert_eq!(parsed.manifest.theme_id, "sample-theme");
}

#[test]
fn v3_pattern_can_reference_a_block_from_same_theme() {
    let mut files = file_map();
    files.insert("theme.json".into(), br#"{"manifest_version":3,"theme_id":"sample-theme","name":"Sample","version":"1.0.0","patterns":[{"key":"notice","title":"Notice","category":"banner","file":"patterns/notice.json"}]}"#.to_vec());
    let pattern = serde_json::json!({
        "schema_version":1,
        "sections":[{"id":"section-one","children":[{"id":"row-one","children":[{
            "id":"column-one","children":[{
                "id":"element-one","type":"plugin/block",
                "props":{"block_id":"sample-theme/banner","values":{}}
            }]
        }]}]}]
    });
    files.insert(
        "patterns/notice.json".into(),
        serde_json::to_vec(&pattern).unwrap(),
    );
    assert!(validate_theme_v3_files(files.clone()).is_ok());

    let mut unknown = pattern;
    unknown["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["block_id"] =
        "other-theme/banner".into();
    files.insert(
        "patterns/notice.json".into(),
        serde_json::to_vec(&unknown).unwrap(),
    );
    assert!(validate_theme_v3_files(files).is_err());
}

#[test]
fn v3_rejects_unpaired_blocks_wrong_slug_and_unsafe_csec() {
    let mut files = file_map();
    files.remove("blocks/banner/render.html");
    assert!(validate_theme_v3_files(files).is_err());

    let mut files = file_map();
    files.insert(
        "blocks/banner/block.json".into(),
        block_schema("sample-theme", "other"),
    );
    assert!(validate_theme_v3_files(files).is_err());

    let mut files = file_map();
    files.insert(
        "blocks/banner/render.html".into(),
        b"<object>unsafe</object>".to_vec(),
    );
    assert!(validate_theme_v3_files(files).is_err());

    let mut files = file_map();
    files.insert("blocks/Banner/render.html".into(), b"<p>Bad</p>".to_vec());
    assert!(validate_theme_v3_files(files).is_err());
}

#[test]
fn v3_rejects_over_32_blocks_and_invalid_owner() {
    let mut files = file_map();
    for i in 0..33 {
        let slug = format!("notice-{i}");
        files.insert(
            format!("blocks/{slug}/block.json"),
            block_schema("sample-theme", &slug),
        );
        files.insert(format!("blocks/{slug}/render.html"), b"<p>Hi</p>".to_vec());
    }
    assert!(validate_theme_v3_files(files).is_err());

    let mut files = file_map();
    files.insert("theme.json".into(), br#"{"manifest_version":3,"theme_id":"abcdefghijklmnopqrstuvwxyz-abcde","name":"Sample","version":"1.0.0"}"#.to_vec());
    assert!(validate_theme_v3_files(files).is_err());
}
