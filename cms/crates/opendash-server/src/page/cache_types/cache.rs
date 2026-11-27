use super::*;

#[derive(Debug, Clone)]
pub struct ValidatedBlockTemplate {
    pub block_id: String,
    pub source: String,
    pub base_env: std::sync::Arc<minijinja::Environment<'static>>,
}

#[derive(Debug, Default)]
pub struct BlockTemplateCache {
    pub(crate) inner: std::sync::RwLock<
        std::collections::BTreeMap<
            (String, String, u64, bool),
            std::sync::Arc<ValidatedBlockTemplate>,
        >,
    >,
}

pub(crate) fn render_contact_form_html(
    node_id: &str,
    form: &crate::storage::contact::ContactFormRecord,
    submit_label: &str,
    is_preview: bool,
    l10n: &crate::content_render::CoreL10n,
) -> String {
    use opendash_core::contact::FormKind;
    let mut out = String::new();
    if is_preview {
        out.push_str(
            "<div class=\"builder-contact-form builder-contact-preview\"><fieldset disabled>",
        );
    } else {
        let nonce = crate::auth::generate_random_token();
        out.push_str(&format!(
            "<form class=\"builder-contact-form\" method=\"post\" action=\"/api/public/contact/{}\">",
            form.id
        ));
        out.push_str(&format!(
            "<input type=\"hidden\" name=\"form_version\" value=\"{}\">",
            form.version
        ));
        out.push_str(&format!(
            "<input type=\"hidden\" name=\"nonce\" value=\"{nonce}\">"
        ));
        out.push_str(&format!(
            "<div class=\"opendash-hp\" hidden><label>{}<input type=\"text\" name=\"honeypot\" tabindex=\"-1\" autocomplete=\"off\"></label></div>",
            l10n.select_empty
        ));
    }

    for field in &form.schema.fields {
        let input_id = format!("cf_{}_{}", escape_html(node_id), escape_html(&field.key));
        let field_name = escape_html(&field.key);
        let label = escape_html(&field.label);
        let req_attr = if field.required { " required" } else { "" };
        out.push_str("<div class=\"builder-contact-field\">");
        match field.kind {
            FormKind::Checkbox => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\"><input id=\"{input_id}\" type=\"checkbox\" name=\"{field_name}\" value=\"true\"{req_attr}> {label}</label>"
                ));
            }
            FormKind::Textarea => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><textarea id=\"{input_id}\" name=\"{field_name}\" maxlength=\"4000\"{req_attr}></textarea>"
                ));
            }
            FormKind::Select => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><select id=\"{input_id}\" name=\"{field_name}\"{req_attr}>"
                ));
                if !field.required {
                    out.push_str(&format!(
                        "<option value=\"\">{}</option>",
                        l10n.select_placeholder
                    ));
                }
                if let Some(options) = &field.options {
                    for opt in options {
                        let eopt = escape_html(opt);
                        out.push_str(&format!("<option value=\"{eopt}\">{eopt}</option>"));
                    }
                }
                out.push_str("</select>");
            }
            FormKind::Email => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><input id=\"{input_id}\" type=\"email\" name=\"{field_name}\" maxlength=\"254\"{req_attr}>"
                ));
            }
            FormKind::Text => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><input id=\"{input_id}\" type=\"text\" name=\"{field_name}\" maxlength=\"254\"{req_attr}>"
                ));
            }
        }
        out.push_str("</div>");
    }

    let btn_label = escape_html(submit_label.trim());
    if is_preview {
        out.push_str(&format!(
            "<button type=\"button\" disabled>{btn_label}</button></fieldset></div>"
        ));
    } else {
        out.push_str(&format!(
            "<button type=\"submit\">{btn_label}</button></form>"
        ));
    }
    out
}
