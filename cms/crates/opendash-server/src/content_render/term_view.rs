use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EntryParentView {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicTermDetailView {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub sort_order: i32,
    pub url: Option<String>,
    pub featured_image: Option<Value>,
    pub fields: BTreeMap<String, Value>,
}

pub(crate) fn rewrite_media_urls_in_value(val: &mut Value) {
    match val {
        Value::Object(obj) => {
            for k in ["url", "thumbnail_url", "cover_url"] {
                if let Some(Value::String(s)) = obj.get_mut(k)
                    && s.starts_with("/media/")
                {
                    *s = s.replacen("/media/", "/api/admin/media/", 1);
                }
            }
            for k in ["webp_srcset", "fallback_srcset"] {
                if let Some(Value::String(s)) = obj.get_mut(k)
                    && (s.starts_with("/media/") || s.contains(", /media/"))
                {
                    *s = s.replace("/media/", "/api/admin/media/");
                }
            }
            for child in obj.values_mut() {
                if child.is_object() || child.is_array() {
                    rewrite_media_urls_in_value(child);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                rewrite_media_urls_in_value(item);
            }
        }
        _ => {}
    }
}

pub(crate) fn rewrite_preview_term_media_urls(
    mut view: PublicTermDetailView,
) -> PublicTermDetailView {
    if let Some(ref mut fi) = view.featured_image {
        rewrite_media_urls_in_value(fi);
    }
    for val in view.fields.values_mut() {
        rewrite_media_urls_in_value(val);
    }
    view
}
