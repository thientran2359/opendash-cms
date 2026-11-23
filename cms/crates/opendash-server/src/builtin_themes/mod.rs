/// Public Tailwind v4 stylesheet is compiled at build time and embedded in the
/// executable so public pages never depend on Node or a remote CDN.
pub const PUBLIC_TAILWIND_CSS: &[u8] = include_bytes!("opendash-tailwind.css");
