use super::*;
use serde_json::json;

fn template(kind: TemplateCandidateKind, file: &str) -> ThemeTemplateDecl {
    ThemeTemplateDecl {
        kind,
        content_type: None,
        taxonomy: None,
        selector_slug: None,
        custom_key: None,
        label: None,
        allowed_types: Vec::new(),
        file: file.into(),
    }
}

fn v3_manifest() -> ThemeV3Manifest {
    ThemeV3Manifest {
        manifest_version: 3,
        theme_id: "ocean-theme".into(),
        name: "Ocean Theme".into(),
        version: "1.2.3-beta.1".into(),
        default_locale: None,
        templates: vec![
            template(TemplateCandidateKind::Index, "index.html"),
            template(TemplateCandidateKind::Page, "page.html"),
        ],
        parts: BTreeMap::from([
            ("header".into(), "header.html".into()),
            ("footer".into(), "footer.html".into()),
            (
                "breadcrumbs".into(),
                "template-part/breadcrumbs.html".into(),
            ),
        ]),
        page_layouts: vec![PageLayoutDecl {
            key: "canvas".into(),
            label: "Canvas".into(),
            file: "page-layouts/canvas.html".into(),
            chrome: PageLayoutChrome::Default,
        }],
        patterns: vec![ThemePatternDecl {
            key: "hero".into(),
            title: "Hero".into(),
            category: "landing".into(),
            description: Some("Welcome".into()),
            preview_asset: Some("assets/images/hero.webp".into()),
            file: "patterns/hero.json".into(),
        }],
        assets: vec!["assets/images/hero.webp".into(), "assets/styles.css".into()],
        styles: Vec::new(),
        scripts: Vec::new(),
        options: Vec::new(),
        menus: None,
        sample: None,
        declarations: None,
        field_groups: None,
        admin_extensions: None,
        functions: None,
    }
}

fn enqueue_manifest() -> ThemeV3Manifest {
    let mut value = serde_json::to_value(v3_manifest()).unwrap();
    value["assets"] = json!([
        "assets/base.css",
        "assets/layout.css",
        "assets/home.css",
        "assets/print.css",
        "assets/base.js",
        "assets/middle.mjs",
        "assets/head.js",
        "assets/footer.js",
        "assets/async.mjs",
        "assets/other.js",
        "assets/icon.svg"
    ]);
    value["styles"] = json!([
        {"handle":"home", "src":"assets/home.css", "deps":["layout"], "conditions":["home"]},
        {"handle":"print", "src":"assets/print.css", "media":"(min-aspect-ratio: 16/9)", "conditions":["all"]},
        {"handle":"layout", "src":"assets/layout.css", "deps":["base"], "conditions":["page"]},
        {"handle":"base", "src":"assets/base.css", "deps":["opendash-tailwind"], "conditions":["single"]}
    ]);
    value["scripts"] = json!([
        {"handle":"head", "src":"assets/head.js", "deps":["middle","base"], "location":"head", "strategy":"blocking", "conditions":["home"]},
        {"handle":"footer", "src":"assets/footer.js", "deps":["base"]},
        {"handle":"middle", "src":"assets/middle.mjs", "deps":["base"], "module":true, "conditions":["page"]},
        {"handle":"base", "src":"assets/base.js", "conditions":["single"]},
        {"handle":"async", "src":"assets/async.mjs", "module":true, "strategy":"async"},
        {"handle":"other", "src":"assets/other.js", "strategy":"async", "conditions":["archive"]}
    ]);
    serde_json::from_value(value).unwrap()
}

mod suite_01;
mod suite_02;
