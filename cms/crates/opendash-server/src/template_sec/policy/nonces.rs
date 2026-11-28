use super::*;

impl RichTextNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.reserved_count == 0
    }

    pub fn total_count(&self) -> usize {
        self.entries.len().saturating_add(self.reserved_count)
    }

    pub fn reserve_count(&mut self, count: usize) -> Result<(), &'static str> {
        if self.total_count().saturating_add(count) > MAX_RICHTEXT_NONCES_PER_REQUEST {
            return Err("Số lượng khối RichText vượt giới hạn 128/request.");
        }
        self.reserved_count = self.reserved_count.saturating_add(count);
        Ok(())
    }

    pub fn try_extend(&mut self, other: Self) -> Result<(), &'static str> {
        if self.total_count().saturating_add(other.total_count()) > MAX_RICHTEXT_NONCES_PER_REQUEST
        {
            return Err("Số lượng khối RichText vượt giới hạn 128/request.");
        }
        self.reserved_count = self.reserved_count.saturating_add(other.reserved_count);
        self.entries.extend(other.entries);
        Ok(())
    }

    pub fn extend(&mut self, other: Self) {
        self.reserved_count = self
            .reserved_count
            .saturating_add(other.reserved_count)
            .min(MAX_RICHTEXT_NONCES_PER_REQUEST);
        for entry in other.entries {
            if self.total_count() < MAX_RICHTEXT_NONCES_PER_REQUEST {
                self.entries.push(entry);
            }
        }
    }

    pub fn register_sanitized(&mut self, sanitized_html: String) -> Result<String, &'static str> {
        if self.total_count() >= MAX_RICHTEXT_NONCES_PER_REQUEST {
            return Err("Số lượng khối RichText vượt giới hạn 128/request.");
        }
        let nonce = generate_richtext_nonce()?;
        self.entries.push((nonce.clone(), sanitized_html));
        Ok(nonce)
    }

    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        substitute_nonces_into_body(
            &self.entries,
            html,
            MAX_RICHTEXT_NONCE_REPEATS,
            MAX_RENDERED_HTML_BYTES,
            "Nonce RichText xuất hiện vượt quá số lần cho phép.",
            "Nonce RichText không nằm trong thân HTML an toàn.",
            "HTML sau khi chèn RichText vượt giới hạn 512 KiB.",
        )
    }
}

impl IconNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn extend(&mut self, entries: Vec<(String, String)>) {
        self.entries.extend(entries);
    }

    /// Substitute server-generated SVG only after document normalization. Any
    /// nonce that disappeared during slot fallback is intentionally ignored.
    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        substitute_nonces_into_body(
            &self.entries,
            html,
            1,
            MAX_ICON_DOCUMENT_BYTES,
            "Nonce icon xuất hiện nhiều hơn một lần.",
            "Nonce icon không nằm trong thân HTML an toàn.",
            "Document chứa icon vượt giới hạn HTML.",
        )
    }
}

pub(crate) fn register_core_icon_in_budget(
    budget: &mut super::RenderBudget,
    name: &str,
    class: Option<&str>,
) -> Result<Option<String>, &'static str> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    let Some(svg) = opendash_core::icons::render_core_icon_svg(name, class) else {
        return Err("od_icon: tên icon hoặc class không hợp lệ");
    };
    if budget.icon_nonces.entries.len() >= MAX_ICONS_PER_REQUEST {
        return Err("od_icon vượt quá 128 icon/request");
    }
    let nonce = new_icon_nonce()?;
    budget.charge_html_bytes(svg.len().saturating_sub(nonce.len()))?;
    budget.icon_nonces.entries.push((nonce.clone(), svg));
    Ok(Some(nonce))
}

pub(crate) fn new_icon_nonce() -> Result<String, &'static str> {
    let mut bytes = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "Không tạo được nonce icon.")?;
    let mut nonce = ICON_NONCE_PREFIX.to_owned();
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce icon.")?;
    }
    Ok(nonce)
}

pub(crate) fn install_icon_function(
    env: &mut Environment<'_>,
    existing: &IconNonceTable,
    local: &std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    html_bytes_used: usize,
    overflowed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let existing_count = existing.entries.len();
    let local = local.clone();
    let overflowed = overflowed.clone();
    env.add_function(
        "od_icon",
        move |name: Value, class: Option<&str>| -> Result<String, minijinja::Error> {
            if name.is_none() || name.is_undefined() {
                return Ok(String::new());
            }
            let Some(name_str) = name.as_str() else {
                return Err(filter_invalid_op("od_icon: tên icon phải là chuỗi"));
            };
            if name_str.is_empty() {
                return Ok(String::new());
            }
            let svg = opendash_core::icons::render_core_icon_svg(name_str, class)
                .ok_or_else(|| filter_invalid_op("od_icon: tên icon hoặc class không hợp lệ"))?;
            let mut table = local.lock().unwrap_or_else(|e| e.into_inner());
            if existing_count.saturating_add(table.len()) >= MAX_ICONS_PER_REQUEST {
                return Err(filter_invalid_op("od_icon vượt quá 128 icon/request"));
            }
            let nonce = new_icon_nonce().map_err(filter_invalid_op)?;
            let local_growth: usize = table
                .iter()
                .map(|(n, s)| s.len().saturating_sub(n.len()))
                .sum();
            if html_bytes_used
                .saturating_add(local_growth)
                .saturating_add(svg.len().saturating_sub(nonce.len()))
                > MAX_RENDERED_HTML_BYTES
            {
                overflowed.store(true, std::sync::atomic::Ordering::Relaxed);
                return Err(filter_invalid_op("od_icon vượt giới hạn HTML 512 KiB"));
            }
            table.push((nonce.clone(), svg));
            Ok(nonce)
        },
    );
}

pub(crate) fn new_hook_nonce() -> Result<String, &'static str> {
    let mut bytes = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "Không tạo được nonce hook.")?;
    let mut nonce = HOOK_NONCE_PREFIX.to_owned();
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce hook.")?;
    }
    Ok(nonce)
}

pub(crate) fn minijinja_value_to_json(
    val: &Value,
    depth: usize,
) -> Result<serde_json::Value, minijinja::Error> {
    use minijinja::value::ValueKind;
    if depth > 12 {
        return Err(filter_invalid_op("od_hook: tham số lồng quá sâu"));
    }
    match val.kind() {
        ValueKind::Undefined | ValueKind::None => Ok(serde_json::Value::Null),
        ValueKind::Bool => Ok(serde_json::Value::Bool(val.is_true())),
        ValueKind::Number => {
            if let Ok(i) = i64::try_from(val.clone()) {
                Ok(serde_json::Value::Number(i.into()))
            } else if let Ok(u) = u64::try_from(val.clone()) {
                Ok(serde_json::Value::Number(u.into()))
            } else if let Ok(f) = f64::try_from(val.clone()) {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| filter_invalid_op("od_hook: số không hợp lệ trong tham số"))
            } else {
                Err(filter_invalid_op("od_hook: số không hợp lệ trong tham số"))
            }
        }
        ValueKind::String => Ok(serde_json::Value::String(
            val.as_str().unwrap_or_default().to_owned(),
        )),
        ValueKind::Seq | ValueKind::Iterable => {
            let mut items = Vec::new();
            for item in val.try_iter()? {
                items.push(minijinja_value_to_json(&item, depth + 1)?);
            }
            Ok(serde_json::Value::Array(items))
        }
        ValueKind::Map => {
            let mut map = serde_json::Map::new();
            for key in val.try_iter()? {
                let k = key
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| key.to_string());
                let v = val.get_item(&key)?;
                map.insert(k, minijinja_value_to_json(&v, depth + 1)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Ok(serde_json::Value::String(val.to_string())),
    }
}
