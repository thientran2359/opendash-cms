use super::*;

pub const RENDER_HOOK_CALL_TIMEOUT: Duration = Duration::from_millis(1000);

pub const RENDER_HOOK_PHASE_BUDGET: Duration = Duration::from_millis(1500);

pub const MAX_RENDER_SUBSCRIBER_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct RenderHookContext<'a> {
    pub route_kind: &'a str,
    pub route_path: &'a str,
    pub canonical_query: &'a str,
    pub is_not_found: bool,
    pub locale: &'a str,
    pub origin_host: &'a str,
    pub site: Value,
    pub options: Value,
    pub entry: Option<Value>,
    pub entries: Option<Value>,
    pub principal: ExecutionPrincipal,
}

#[derive(Debug, Clone, Default)]
pub struct RenderHooksOutput {
    pub head_html: String,
    pub head_styles: Vec<String>,
    pub head_scripts: Vec<String>,
    pub footer_html: String,
    pub slot_replacements: Vec<(String, String)>,
}
