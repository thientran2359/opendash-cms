use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use minijinja::{
    Environment, Value,
    machinery::ast::{CallArg, Expr},
    value::{Enumerator, Kwargs, Object, ObjectRepr, Rest, ValueOrKwargs, from_args},
};
use opendash_core::{
    field_schema::valid_sha256_media_id,
    icons::valid_icon_class_attr,
    media::{
        ResponsivePictureInput, render_responsive_picture_html, valid_image_decoding_attr,
        valid_image_fetchpriority_attr, valid_image_loading_attr, valid_image_sizes_attr,
        valid_media_render_url, valid_media_srcset,
    },
};

use super::{
    FieldTemplateNode, MAX_ICON_DOCUMENT_BYTES, MAX_RENDERED_HTML_BYTES, ThemeUrlPolicy,
    UrlScopeStack, UrlSourceKind, extract_attr_chain, filter_invalid_op, json_to_safe_value,
    resolve_field_node_and_tail, substitute_nonces_into_body, validate_expr,
};

pub const IMAGE_NONCE_PREFIX: &str = "OD_IMAGE_";
pub const MAX_IMAGES_PER_REQUEST: usize = 128;
pub const MEDIA_PROVENANCE_KEY: &str = "__od_media_provenance";

pub fn media_provenance_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        let mut bytes = [0_u8; 16];
        OsRng.fill_bytes(&mut bytes);
        let mut out = String::with_capacity(32);
        for byte in bytes {
            use std::fmt::Write;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    })
}

#[derive(Debug, Clone)]
pub struct SafeMediaObject {
    pub id: String,
    pub version: i64,
    pub kind: String,
    pub url: String,
    pub thumbnail_url: String,
    pub alt: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub variants: Vec<Value>,
    pub webp_srcset: Option<String>,
    pub fallback_srcset: Option<String>,
    pub is_preview: bool,
}

impl SafeMediaObject {
    pub(crate) fn try_from_tagged_json(
        map: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<Self> {
        let token = map.get(MEDIA_PROVENANCE_KEY)?.as_str()?;
        if token != media_provenance_token() {
            return None;
        }
        let id = map.get("id")?.as_str()?.to_owned();
        if !valid_sha256_media_id(&id) {
            return None;
        }
        let kind = map.get("kind")?.as_str()?.to_owned();
        let url = map.get("url")?.as_str()?.to_owned();
        let is_preview = url.starts_with("/api/admin/media/");
        if !valid_media_render_url(&url, is_preview) {
            return None;
        }
        let thumbnail_url = map
            .get("thumbnail_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if !thumbnail_url.is_empty() && !valid_media_render_url(&thumbnail_url, is_preview) {
            return None;
        }
        let alt = map
            .get("alt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let version = map
            .get("version")
            .and_then(|v| v.as_i64())
            .unwrap_or(1)
            .max(1);
        let width = map.get("width").and_then(|v| v.as_i64());
        let height = map.get("height").and_then(|v| v.as_i64());
        let webp_srcset = map
            .get("webp_srcset")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        if let Some(ref ws) = webp_srcset
            && !valid_media_srcset(ws, is_preview)
        {
            return None;
        }
        let fallback_srcset = map
            .get("fallback_srcset")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        if let Some(ref fs) = fallback_srcset
            && !valid_media_srcset(fs, is_preview)
        {
            return None;
        }
        let variants = map
            .get("variants")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().map(json_to_safe_value).collect())
            .unwrap_or_default();

        Some(Self {
            id,
            version,
            kind,
            url,
            thumbnail_url,
            alt,
            width,
            height,
            variants,
            webp_srcset,
            fallback_srcset,
            is_preview,
        })
    }
}

impl Object for SafeMediaObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "id" => Some(Value::from(self.id.clone())),
            "version" => Some(Value::from(self.version)),
            "kind" => Some(Value::from(self.kind.clone())),
            "url" => Some(Value::from(self.url.clone())),
            "thumbnail_url" => Some(Value::from(self.thumbnail_url.clone())),
            "alt" => Some(Value::from(self.alt.clone())),
            "width" => Some(
                self.width
                    .map(Value::from)
                    .unwrap_or_else(|| Value::from(())),
            ),
            "height" => Some(
                self.height
                    .map(Value::from)
                    .unwrap_or_else(|| Value::from(())),
            ),
            "variants" => Some(Value::from(self.variants.clone())),
            "webp_srcset" => Some(
                self.webp_srcset
                    .clone()
                    .map(Value::from)
                    .unwrap_or_else(|| Value::from(())),
            ),
            "fallback_srcset" => Some(
                self.fallback_srcset
                    .clone()
                    .map(Value::from)
                    .unwrap_or_else(|| Value::from(())),
            ),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&[
            "id",
            "version",
            "kind",
            "url",
            "thumbnail_url",
            "alt",
            "width",
            "height",
            "variants",
            "webp_srcset",
            "fallback_srcset",
        ])
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImageNonceTable {
    pub(super) entries: Vec<(String, String)>,
}

impl ImageNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn extend(&mut self, entries: Vec<(String, String)>) {
        self.entries.extend(entries);
    }

    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        substitute_nonces_into_body(
            &self.entries,
            html,
            1,
            MAX_ICON_DOCUMENT_BYTES,
            "Nonce hình ảnh xuất hiện nhiều hơn một lần.",
            "Nonce hình ảnh không nằm trong thân HTML an toàn.",
            "Document chứa hình ảnh vượt giới hạn HTML.",
        )
    }
}

pub(crate) fn new_image_nonce() -> Result<String, &'static str> {
    let mut bytes = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "Không tạo được nonce hình ảnh.")?;
    let mut nonce = IMAGE_NONCE_PREFIX.to_owned();
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce hình ảnh.")?;
    }
    Ok(nonce)
}

pub(crate) struct OdImageCall<'a> {
    pub media_root: &'a str,
    pub media_attrs: Vec<&'a str>,
    pub alt_expr: Option<&'a Expr<'a>>,
}

const IMAGE_PARAM_NAMES: [&str; 6] = [
    "sizes",
    "class",
    "alt",
    "loading",
    "decoding",
    "fetchpriority",
];

fn validate_od_image_param_expr<'a>(
    slot: usize,
    expr: &'a Expr<'a>,
) -> Result<Option<&'a Expr<'a>>, &'static str> {
    if slot == 2 {
        return match expr {
            Expr::Const(c) if c.value.as_str().is_some() => Ok(None),
            Expr::Const(_) => Err("od_image() alt literal phải là chuỗi."),
            other => {
                validate_expr(other)?;
                Ok(Some(other))
            }
        };
    }
    let Expr::Const(c) = expr else {
        return Err(
            "od_image() tham số thuộc tính (sizes/class/loading/decoding/fetchpriority) phải là string literal.",
        );
    };
    let s = c
        .value
        .as_str()
        .ok_or("od_image() tham số thuộc tính phải là string literal.")?;
    let ok = match slot {
        0 => valid_image_sizes_attr(s),
        1 => s.is_empty() || valid_icon_class_attr(s),
        3 => valid_image_loading_attr(s),
        4 => valid_image_decoding_attr(s),
        5 => valid_image_fetchpriority_attr(s),
        _ => false,
    };
    if !ok {
        return Err("od_image() giá trị thuộc tính không hợp lệ theo C-SEC.");
    }
    Ok(None)
}

pub(crate) fn parse_od_image_call<'a>(
    expr: &'a Expr<'a>,
) -> Result<Option<OdImageCall<'a>>, &'static str> {
    let Expr::Call(call) = expr else {
        return Ok(None);
    };
    let Expr::Var(var) = &call.expr else {
        return Ok(None);
    };
    if var.id != "od_image" {
        return Ok(None);
    }
    let [CallArg::Pos(first_arg), rest @ ..] = call.args.as_slice() else {
        return Err("od_image() yêu cầu đối số media ở vị trí đầu tiên.");
    };
    let Some((media_root, media_attrs)) = extract_attr_chain(first_arg) else {
        return Err("od_image() đối số đầu phải là tham chiếu trường media hợp lệ.");
    };

    let mut seen = [false; 6];
    let mut pos_idx = 0usize;
    let mut seen_kwarg = false;
    let mut alt_expr = None;

    for arg in rest {
        match arg {
            CallArg::Pos(e) => {
                if seen_kwarg || pos_idx >= IMAGE_PARAM_NAMES.len() {
                    return Err("od_image() nhận quá nhiều tham số vị trí.");
                }
                seen[pos_idx] = true;
                if let Some(a) = validate_od_image_param_expr(pos_idx, e)? {
                    alt_expr = Some(a);
                }
                pos_idx += 1;
            }
            CallArg::Kwarg(name, e) => {
                seen_kwarg = true;
                let Some(slot) = IMAGE_PARAM_NAMES.iter().position(|n| n == name) else {
                    return Err("od_image() chứa tham số keyword không được hỗ trợ.");
                };
                if seen[slot] {
                    return Err("od_image() truyền trùng lặp tham số.");
                }
                seen[slot] = true;
                if let Some(a) = validate_od_image_param_expr(slot, e)? {
                    alt_expr = Some(a);
                }
            }
            CallArg::PosSplat(_) | CallArg::KwargSplat(_) => {
                return Err("od_image() không cho phép splat (*args/**kwargs).");
            }
        }
    }

    Ok(Some(OdImageCall {
        media_root,
        media_attrs,
        alt_expr,
    }))
}

pub(crate) fn is_valid_od_image_media_target(
    root: &str,
    attrs: &[&str],
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some((node, tail)) = resolve_field_node_and_tail(root, attrs, scopes, policy) {
        if matches!(
            (node, tail),
            (FieldTemplateNode::Media, &[])
                | (
                    FieldTemplateNode::TermRef { multiple: false },
                    &["featured_image"]
                )
        ) {
            return true;
        }
    }
    (attrs == ["cover_media"] && scopes.lookup(root) == Some(&UrlSourceKind::TrustedEntry))
        || (attrs == ["image_media"] && scopes.lookup(root) == Some(&UrlSourceKind::TrustedField))
}

fn extract_str_arg(
    pos: &[Value],
    kwargs: &Kwargs,
    pos_idx: usize,
    key: &str,
    default: &str,
) -> Result<String, minijinja::Error> {
    let pos_val = pos.get(pos_idx);
    let kw_val: Option<Value> = kwargs.get(key)?;
    if pos_val.is_some() && kw_val.is_some() {
        return Err(filter_invalid_op(
            "od_image: trùng tham số vị trí và keyword",
        ));
    }
    match pos_val.or(kw_val.as_ref()) {
        None => Ok(default.to_owned()),
        Some(v) if v.is_none() || v.is_undefined() => Ok(default.to_owned()),
        Some(v) => v
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| filter_invalid_op("od_image: tham số thuộc tính phải là chuỗi")),
    }
}

pub(crate) fn install_image_function(
    env: &mut Environment<'_>,
    existing: &ImageNonceTable,
    local: &Arc<Mutex<Vec<(String, String)>>>,
    html_bytes_used: usize,
    overflowed: &Arc<AtomicBool>,
) {
    let existing_count = existing.entries.len();
    let local = local.clone();
    let overflowed = overflowed.clone();
    env.add_function(
        "od_image",
        move |rest: Rest<ValueOrKwargs>| -> Result<String, minijinja::Error> {
            let raw = rest.into_values();
            let (pos, kwargs) = from_args::<(&[Value], Kwargs)>(&raw)?;
            if pos.is_empty() || pos.len() > 7 {
                return Err(filter_invalid_op(
                    "od_image: số lượng tham số vị trí không hợp lệ",
                ));
            }
            let media = &pos[0];
            if media.is_none() || media.is_undefined() {
                let _ = kwargs.get::<Option<Value>>("sizes")?;
                let _ = kwargs.get::<Option<Value>>("class")?;
                let _ = kwargs.get::<Option<Value>>("alt")?;
                let _ = kwargs.get::<Option<Value>>("loading")?;
                let _ = kwargs.get::<Option<Value>>("decoding")?;
                let _ = kwargs.get::<Option<Value>>("fetchpriority")?;
                kwargs.assert_all_used()?;
                return Ok(String::new());
            }
            let Some(safe_media) = media.downcast_object_ref::<SafeMediaObject>() else {
                return Err(filter_invalid_op(
                    "od_image: đối số media thiếu provenance hợp lệ",
                ));
            };
            let sizes = extract_str_arg(pos, &kwargs, 1, "sizes", "100vw")?;
            let class = extract_str_arg(pos, &kwargs, 2, "class", "")?;
            let alt = extract_str_arg(pos, &kwargs, 3, "alt", &safe_media.alt)?;
            let loading = extract_str_arg(pos, &kwargs, 4, "loading", "lazy")?;
            let decoding = extract_str_arg(pos, &kwargs, 5, "decoding", "async")?;
            let fetchpriority = extract_str_arg(pos, &kwargs, 6, "fetchpriority", "")?;
            kwargs.assert_all_used()?;

            if safe_media.kind != "image" {
                return Ok(String::new());
            }
            let html = render_responsive_picture_html(
                &ResponsivePictureInput {
                    url: &safe_media.url,
                    width: safe_media.width,
                    height: safe_media.height,
                    webp_srcset: safe_media.webp_srcset.as_deref(),
                    fallback_srcset: safe_media.fallback_srcset.as_deref(),
                    sizes: &sizes,
                    class: &class,
                    alt: &alt,
                    loading: &loading,
                    decoding: &decoding,
                    fetchpriority: &fetchpriority,
                },
                safe_media.is_preview,
            )
            .ok_or_else(|| filter_invalid_op("od_image: thuộc tính ảnh không hợp lệ"))?;

            let mut table = local.lock().unwrap_or_else(|e| e.into_inner());
            if existing_count.saturating_add(table.len()) >= MAX_IMAGES_PER_REQUEST {
                return Err(filter_invalid_op("od_image vượt quá 128 ảnh/request"));
            }
            let nonce = new_image_nonce().map_err(filter_invalid_op)?;
            let local_growth: usize = table
                .iter()
                .map(|(n, s)| s.len().saturating_sub(n.len()))
                .sum();
            if html_bytes_used
                .saturating_add(local_growth)
                .saturating_add(html.len().saturating_sub(nonce.len()))
                > MAX_RENDERED_HTML_BYTES
            {
                overflowed.store(true, Ordering::Relaxed);
                return Err(filter_invalid_op("od_image vượt giới hạn HTML 512 KiB"));
            }
            table.push((nonce.clone(), html));
            Ok(nonce)
        },
    );
}
