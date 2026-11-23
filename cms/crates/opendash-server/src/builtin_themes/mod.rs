/// Public Tailwind v4 stylesheet is compiled at build time and embedded in the
/// executable so public pages never depend on Node or a remote CDN.
pub const PUBLIC_TAILWIND_CSS: &[u8] = include_bytes!("opendash-tailwind.css");

use std::collections::BTreeMap;

pub const STARTER_THEME_ID: &str = "opendash-starter";
pub const STARTER_THEME_NAME: &str = "OpenDash Starter";
pub const STARTER_THEME_VERSION: &str = "1.0.1";

/// Immutable embedded Starter theme resources, validated using the same v3
/// boundary as uploaded packages before publishing an on-disk revision.
pub fn builtin_starter_theme_v100_files() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        (
            "404.html".to_owned(),
            include_bytes!("opendash-starter/404.html").to_vec(),
        ),
        (
            "archive-post.html".to_owned(),
            include_bytes!("opendash-starter/archive-post.html").to_vec(),
        ),
        (
            "archive.html".to_owned(),
            include_bytes!("opendash-starter/archive.html").to_vec(),
        ),
        (
            "assets/style.css".to_owned(),
            include_bytes!("opendash-starter/assets/style.css").to_vec(),
        ),
        (
            "blocks/quote-box/block.json".to_owned(),
            include_bytes!("opendash-starter/blocks/quote-box/block.json").to_vec(),
        ),
        (
            "blocks/quote-box/render.html".to_owned(),
            include_bytes!("opendash-starter/blocks/quote-box/render.html").to_vec(),
        ),
        (
            "blocks/stats-bar/block.json".to_owned(),
            include_bytes!("opendash-starter/blocks/stats-bar/block.json").to_vec(),
        ),
        (
            "blocks/stats-bar/render.html".to_owned(),
            include_bytes!("opendash-starter/blocks/stats-bar/render.html").to_vec(),
        ),
        (
            "footer.html".to_owned(),
            include_bytes!("opendash-starter/footer.html").to_vec(),
        ),
        (
            "header.html".to_owned(),
            include_bytes!("opendash-starter/header.html").to_vec(),
        ),
        (
            "home.html".to_owned(),
            include_bytes!("opendash-starter/home.html").to_vec(),
        ),
        (
            "index.html".to_owned(),
            include_bytes!("opendash-starter/index.html").to_vec(),
        ),
        (
            "page-layouts/canvas.html".to_owned(),
            include_bytes!("opendash-starter/page-layouts/canvas.html").to_vec(),
        ),
        (
            "page-layouts/default.html".to_owned(),
            include_bytes!("opendash-starter/page-layouts/default.html").to_vec(),
        ),
        (
            "page-layouts/full-width.html".to_owned(),
            include_bytes!("opendash-starter/page-layouts/full-width.html").to_vec(),
        ),
        (
            "page.html".to_owned(),
            include_bytes!("opendash-starter/page.html").to_vec(),
        ),
        (
            "patterns/about-story-split.json".to_owned(),
            include_bytes!("opendash-starter/patterns/about-story-split.json").to_vec(),
        ),
        (
            "patterns/cta-newsletter-banner.json".to_owned(),
            include_bytes!("opendash-starter/patterns/cta-newsletter-banner.json").to_vec(),
        ),
        (
            "patterns/faq-accordion.json".to_owned(),
            include_bytes!("opendash-starter/patterns/faq-accordion.json").to_vec(),
        ),
        (
            "patterns/features-bento-grid.json".to_owned(),
            include_bytes!("opendash-starter/patterns/features-bento-grid.json").to_vec(),
        ),
        (
            "patterns/hero-launchpad.json".to_owned(),
            include_bytes!("opendash-starter/patterns/hero-launchpad.json").to_vec(),
        ),
        (
            "patterns/pricing-tiers.json".to_owned(),
            include_bytes!("opendash-starter/patterns/pricing-tiers.json").to_vec(),
        ),
        (
            "single-post.html".to_owned(),
            include_bytes!("opendash-starter/single-post.html").to_vec(),
        ),
        (
            "single.html".to_owned(),
            include_bytes!("opendash-starter/single.html").to_vec(),
        ),
        (
            "taxonomy.html".to_owned(),
            include_bytes!("opendash-starter/taxonomy.html").to_vec(),
        ),
        (
            "template-part/post-card.html".to_owned(),
            include_bytes!("opendash-starter/template-part/post-card.html").to_vec(),
        ),
        (
            "theme.json".to_owned(),
            include_bytes!("opendash-starter/theme.json").to_vec(),
        ),
    ])
}

/// v1.0.1 shares immutable assets, layouts, patterns and blocks with the
/// frozen v1.0.0 snapshot, changing only the four explicitly overlaid files.
pub fn builtin_starter_theme_files() -> BTreeMap<String, Vec<u8>> {
    let mut files = builtin_starter_theme_v100_files();
    for (path, bytes) in [
        (
            "theme.json",
            include_bytes!("opendash-starter-1.0.1/theme.json").as_slice(),
        ),
        (
            "header.html",
            include_bytes!("opendash-starter-1.0.1/header.html").as_slice(),
        ),
        (
            "home.html",
            include_bytes!("opendash-starter-1.0.1/home.html").as_slice(),
        ),
        (
            "single-post.html",
            include_bytes!("opendash-starter-1.0.1/single-post.html").as_slice(),
        ),
    ] {
        files.insert(path.to_owned(), bytes.to_vec());
    }
    files
}
