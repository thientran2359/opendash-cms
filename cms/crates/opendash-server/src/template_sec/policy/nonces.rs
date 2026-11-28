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

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalIconBatch {
    pub(crate) table: IconNonceTable,
    pub(crate) bytes_delta: usize,
}

pub fn find_top_level_body_close(html: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut scanner = HtmlBodyScanner::new();
    let mut last_close = None;
    for (i, &b) in bytes.iter().enumerate() {
        if scanner.state == HtmlScanState::Body
            && scanner.depths.svg == 0
            && scanner.depths.foreign_object == 0
            && b == b'<'
            && bytes
                .get(i..i + 7)
                .is_some_and(|w| w.eq_ignore_ascii_case(b"</body>"))
        {
            last_close = Some(i);
        }
        scanner.consume(b);
    }
    last_close
}

impl IconNonceTable {
    #[cfg(test)]
    pub(crate) fn from_test_entries(entries: Vec<(String, String)>) -> Self {
        Self {
            entries,
            meta: BTreeMap::new(),
            unique_names: BTreeSet::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn merge_batch(&mut self, batch: LocalIconBatch) {
        self.entries.extend(batch.table.entries);
        self.meta.extend(batch.table.meta);
        self.unique_names.extend(batch.table.unique_names);
    }

    pub fn register_icon(
        &mut self,
        name: &str,
        class: Option<&str>,
        html_bytes_used: &mut usize,
    ) -> Result<String, &'static str> {
        let empty = BTreeSet::new();
        self.register_icon_against(&empty, 0, name, class, html_bytes_used)
    }

    pub(crate) fn register_icon_against(
        &mut self,
        base_unique: &BTreeSet<String>,
        base_usages: usize,
        name: &str,
        class: Option<&str>,
        html_bytes_used: &mut usize,
    ) -> Result<String, &'static str> {
        let inline_svg = opendash_core::icons::render_core_icon_svg(name, class)
            .ok_or("od_icon: tên icon hoặc class không hợp lệ")?;
        let sym_len = opendash_core::icons::render_core_icon_symbol(name)
            .ok_or("od_icon: tên icon hoặc class không hợp lệ")?
            .len();
        let use_svg_len = opendash_core::icons::render_core_icon_use(name, class)
            .ok_or("od_icon: tên icon hoặc class không hợp lệ")?
            .len();

        let seen_before = base_unique.contains(name) || self.unique_names.contains(name);
        let new_uniques_in_self = self.unique_names.difference(base_unique).count();
        let unique_total = base_unique.len().saturating_add(new_uniques_in_self);
        if !seen_before && unique_total >= MAX_UNIQUE_ICONS_PER_REQUEST {
            return Err("od_icon vượt quá 64 loại icon/request");
        }
        if base_usages.saturating_add(self.entries.len()) >= MAX_ICON_USAGES_PER_REQUEST {
            return Err("od_icon vượt quá 256 icon/request");
        }

        let nonce = new_icon_nonce()?;
        let delta = if seen_before {
            use_svg_len.saturating_sub(nonce.len())
        } else {
            inline_svg
                .len()
                .max(sym_len.saturating_add(use_svg_len))
                .saturating_sub(nonce.len())
        };
        let next_bytes = html_bytes_used.saturating_add(delta);
        if next_bytes > MAX_RENDERED_HTML_BYTES {
            return Err("od_icon vượt giới hạn HTML 512 KiB");
        }
        *html_bytes_used = next_bytes;
        self.meta
            .insert(nonce.clone(), (name.to_owned(), class.map(str::to_owned)));
        self.unique_names.insert(name.to_owned());
        self.entries.push((nonce.clone(), inline_svg));
        Ok(nonce)
    }

    /// Substitute server-generated SVG only after document normalization. Any
    /// nonce that disappeared during slot fallback is intentionally ignored.
    /// Icons used `>= 2` times are deduplicated into a single `<symbol>` sprite.
    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        if self.entries.is_empty() {
            return Ok(html);
        }
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for (nonce, _) in &self.entries {
            if html.contains(nonce.as_str())
                && let Some((name, _)) = self.meta.get(nonce)
            {
                *counts.entry(name.as_str()).or_insert(0) += 1;
            }
        }
        let mut sprite_names: BTreeSet<&str> = BTreeSet::new();
        let mut resolved = Vec::with_capacity(self.entries.len());
        for (nonce, inline_svg) in &self.entries {
            if let Some((name, class)) = self.meta.get(nonce)
                && counts.get(name.as_str()).copied().unwrap_or(0) >= 2
                && let Some(use_svg) =
                    opendash_core::icons::render_core_icon_use(name, class.as_deref())
            {
                sprite_names.insert(name.as_str());
                resolved.push((nonce.clone(), use_svg));
                continue;
            }
            resolved.push((nonce.clone(), inline_svg.clone()));
        }

        let mut out = substitute_nonces_into_body(
            &resolved,
            html,
            1,
            MAX_ICON_DOCUMENT_BYTES,
            "Nonce icon xuất hiện nhiều hơn một lần.",
            "Nonce icon không nằm trong thân HTML an toàn.",
            "Document chứa icon vượt giới hạn HTML.",
        )?;

        if !sprite_names.is_empty() {
            let mut defs = String::new();
            for name in sprite_names {
                if let Some(sym) = opendash_core::icons::render_core_icon_symbol(name) {
                    defs.push_str(&sym);
                }
            }
            if !defs.is_empty() {
                let sprite = format!(
                    "<svg aria-hidden=\"true\" width=\"0\" height=\"0\" class=\"absolute overflow-hidden pointer-events-none\"><defs>{defs}</defs></svg>"
                );
                if let Some(idx) = find_top_level_body_close(&out) {
                    out.insert_str(idx, &sprite);
                } else {
                    out.push_str(&sprite);
                }
                if out.len() > MAX_ICON_DOCUMENT_BYTES {
                    return Err("Document chứa icon vượt giới hạn HTML.");
                }
            }
        }
        Ok(out)
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
    let nonce = budget
        .icon_nonces
        .register_icon(name, class, &mut budget.html_bytes_used)?;
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
    local: &std::sync::Arc<std::sync::Mutex<LocalIconBatch>>,
    html_bytes_used: usize,
    overflowed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let base_unique = existing.unique_names.clone();
    let base_usages = existing.entries.len();
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
            let mut batch = local.lock().unwrap_or_else(|e| e.into_inner());
            let mut total_bytes = html_bytes_used.saturating_add(batch.bytes_delta);
            match batch.table.register_icon_against(
                &base_unique,
                base_usages,
                name_str,
                class,
                &mut total_bytes,
            ) {
                Ok(nonce) => {
                    batch.bytes_delta = total_bytes.saturating_sub(html_bytes_used);
                    Ok(nonce)
                }
                Err(msg) => {
                    if msg == "od_icon vượt giới hạn HTML 512 KiB" {
                        overflowed.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(filter_invalid_op(msg))
                }
            }
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
