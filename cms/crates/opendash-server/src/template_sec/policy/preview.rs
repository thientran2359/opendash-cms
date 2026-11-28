use super::*;

impl Default for RenderBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            builder_lists_used: 0,
            locale: "vi".to_owned(),
            icon_nonces: IconNonceTable::default(),
            image_nonces: ImageNonceTable::default(),
            richtext_nonces: RichTextNonceTable::default(),
            hook_nonces: HookNonceTable::default(),
        }
    }
}

impl RenderBudget {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn charge_html_bytes(&mut self, add_bytes: usize) -> Result<(), &'static str> {
        self.html_bytes_used = self.html_bytes_used.saturating_add(add_bytes);
        if self.html_bytes_used > MAX_RENDERED_HTML_BYTES {
            return Err("Builder template composition exceeds 512 KiB.");
        }
        Ok(())
    }

    pub fn charge_builder_list(&mut self) -> Result<(), &'static str> {
        if self.builder_lists_used >= MAX_BUILDER_LISTS_PER_REQUEST {
            return Err("Builder vượt giới hạn 8 list trên mỗi request.");
        }
        self.builder_lists_used += 1;
        Ok(())
    }
}

impl Default for PreviewCompositionBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            builder_lists_used: 0,
            expansions: 0,
            stack: Vec::new(),
            icon_nonces: IconNonceTable::default(),
            image_nonces: ImageNonceTable::default(),
            richtext_nonces: RichTextNonceTable::default(),
            hook_nonces: HookNonceTable::default(),
        }
    }
}

/// Validated literal edges in source order, including repeated occurrences.
/// No execution is needed to discover the mixed preview graph.
pub(crate) fn preview_csec_part_keys(
    name: &str,
    source: &str,
) -> Result<Vec<String>, &'static str> {
    preview_csec_part_keys_with_theme_urls(name, source, &ThemeUrlPolicy::default())
}

pub(crate) fn preview_csec_part_keys_with_theme_urls(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<String>, &'static str> {
    preview_composition_sites(name, source, policy)?
        .into_iter()
        .map(|site| match site.call {
            CompositionCall::Part(key) => Ok(key),
            CompositionCall::Content => Err("Template part không được chứa od_content()."),
        })
        .collect()
}

pub(crate) fn preview_composition_sites(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    if source.contains(PREVIEW_PART_NONCE_PREFIX)
        || source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(BLOCK_CHILDREN_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(IMAGE_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    composition_sites(name, source, true, policy, true)
}

/// Private preview bridge. Theme descendants share their parent's lexical
/// MiniJinja context. Only DB Builder children (or a server-owned missing-part
/// fallback) are supplied as already rendered, trusted HTML.
pub(crate) fn render_preview_csec_fragment(
    name: &str,
    source: &str,
    context: Value,
    theme_sources: &BTreeMap<String, String>,
    trusted_builder_parts: &BTreeMap<String, String>,
    budget: &mut PreviewCompositionBudget,
) -> Result<String, &'static str> {
    render_preview_csec_fragment_with_theme_urls(
        name,
        source,
        context,
        theme_sources,
        trusted_builder_parts,
        budget,
        &ThemeUrlPolicy::default(),
    )
}

pub(crate) fn render_preview_csec_fragment_with_theme_urls(
    name: &str,
    source: &str,
    context: Value,
    theme_sources: &BTreeMap<String, String>,
    trusted_builder_parts: &BTreeMap<String, String>,
    budget: &mut PreviewCompositionBudget,
    policy: &ThemeUrlPolicy,
) -> Result<String, &'static str> {
    render_preview_csec_fragment_with_theme_urls_and_config(
        name,
        source,
        context,
        theme_sources,
        trusted_builder_parts,
        budget,
        policy,
        &RenderFormattingConfig::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_preview_csec_fragment_with_theme_urls_and_config(
    name: &str,
    source: &str,
    context: Value,
    theme_sources: &BTreeMap<String, String>,
    trusted_builder_parts: &BTreeMap<String, String>,
    budget: &mut PreviewCompositionBudget,
    policy: &ThemeUrlPolicy,
    fmt_cfg: &RenderFormattingConfig,
) -> Result<String, &'static str> {
    if budget.fuel_remaining == 0 || budget.fuel_remaining > TEMPLATE_FUEL_LIMIT {
        return Err("Fuel preview không hợp lệ hoặc đã hết.");
    }
    if budget.expansions > MAX_COMPOSITION_EXPANSIONS {
        return Err("od_part vượt quá 32 lần mở rộng.");
    }
    let mut compiler = PreviewFragmentCompiler {
        theme_sources,
        trusted_builder_parts,
        nonces: BTreeMap::new(),
        policy,
    };
    let mut program = String::new();
    compiler.flatten(name, source, 0, &mut program)?;
    // Revalidate across source boundaries; the original source checks alone
    // cannot detect markup or syntax introduced by concatenation.
    validate_template_csec_with_usage_flags(name, &program, policy, true, true, true)?;
    let mut env = restricted_environment_with_config(fmt_cfg);
    let local_icons = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let local_images = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    install_icon_function(
        &mut env,
        &budget.icon_nonces,
        &local_icons,
        budget.html_bytes_used,
        &overflowed,
    );
    install_image_function(
        &mut env,
        &budget.image_nonces,
        &local_images,
        budget.html_bytes_used,
        &overflowed,
    );
    install_hook_function(&mut env, &budget.hook_nonces, &local_hooks);
    env.set_fuel(Some(budget.fuel_remaining));
    env.add_template(name, &program)
        .map_err(|_| "Không thể biên dịch theme preview.")?;
    let captured = env
        .get_template(name)
        .map_err(|_| "Không tìm thấy theme preview.")?
        .render_captured(context)
        .map_err(|_| {
            if overflowed.load(std::sync::atomic::Ordering::Relaxed) {
                budget.html_bytes_used = MAX_RENDERED_HTML_BYTES + 1;
            }
            "Lỗi thực thi theme preview (fuel/recursion/undefined)."
        })?;
    let (consumed, remaining) = captured
        .state()
        .fuel_levels()
        .ok_or("Thiếu bộ theo dõi fuel preview.")?;
    let debited = budget
        .fuel_remaining
        .checked_sub(consumed)
        .ok_or("Fuel preview không hợp lệ.")?;
    if debited == 0 || debited != remaining {
        return Err("Fuel preview không hợp lệ hoặc đã hết.");
    }
    budget.fuel_remaining = debited;
    let shell = captured.output();
    if shell.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(shell, true, true, true)?;
    let out = insert_preview_fragments(shell, &compiler.nonces)?;
    let additions = local_icons
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let icon_growth: usize = additions
        .iter()
        .map(|(n, s)| s.len().saturating_sub(n.len()))
        .sum();
    budget.html_bytes_used = budget.html_bytes_used.saturating_add(icon_growth);
    if budget.html_bytes_used > MAX_RENDERED_HTML_BYTES {
        return Err("HTML preview chứa icon vượt giới hạn 512 KiB.");
    }
    budget.icon_nonces.extend(additions);
    let image_additions = local_images
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let image_growth: usize = image_additions
        .iter()
        .map(|(n, s)| s.len().saturating_sub(n.len()))
        .sum();
    budget.html_bytes_used = budget.html_bytes_used.saturating_add(image_growth);
    if budget.html_bytes_used > MAX_RENDERED_HTML_BYTES {
        return Err("HTML preview chứa ảnh vượt giới hạn 512 KiB.");
    }
    budget.image_nonces.extend(image_additions);
    let hook_additions = local_hooks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    budget.hook_nonces.extend(hook_additions);
    Ok(out)
}

pub(crate) struct PreviewFragmentCompiler<'a> {
    pub(crate) theme_sources: &'a BTreeMap<String, String>,
    pub(crate) trusted_builder_parts: &'a BTreeMap<String, String>,
    pub(crate) nonces: BTreeMap<String, &'a str>,
    pub(crate) policy: &'a ThemeUrlPolicy,
}

impl<'a> PreviewFragmentCompiler<'a> {
    pub(crate) fn flatten(
        &mut self,
        name: &str,
        source: &str,
        depth: usize,
        out: &mut String,
    ) -> Result<(), &'static str> {
        // The mixed graph is already checked by storage. This guard also
        // bounds recursion if an inconsistent segment is passed internally.
        if depth > MAX_COMPOSITION_PART_DEPTH {
            return Err("Độ sâu od_part vượt giới hạn 3.");
        }
        let sites = preview_composition_sites(name, source, self.policy)?;
        let mut cursor = 0;
        for site in sites {
            append_preview_fragment(out, &source[cursor..site.start])?;
            let CompositionCall::Part(key) = site.call else {
                return Err("Template part không được chứa od_content().");
            };
            if depth >= MAX_COMPOSITION_PART_DEPTH {
                return Err("Độ sâu od_part vượt giới hạn 3.");
            }
            // Resolution follows the request-local DB-before-theme graph.
            if let Some(html) = self.trusted_builder_parts.get(&key) {
                if html.len() > MAX_RENDERED_HTML_BYTES {
                    return Err("HTML part vượt quá giới hạn 512 KiB.");
                }
                let mut random = [0_u8; 16];
                OsRng
                    .try_fill_bytes(&mut random)
                    .map_err(|_| "Không tạo được nonce preview.")?;
                let mut nonce = PREVIEW_PART_NONCE_PREFIX.to_owned();
                for byte in random {
                    use std::fmt::Write;
                    write!(&mut nonce, "{byte:02x}")
                        .map_err(|_| "Không tạo được nonce preview.")?;
                }
                if self.nonces.insert(nonce.clone(), html).is_some() {
                    return Err("Nonce preview bị trùng.");
                }
                append_preview_fragment(out, &nonce)?;
            } else {
                let child = self
                    .theme_sources
                    .get(&key)
                    .ok_or("od_part trỏ tới part không tồn tại.")?;
                // Substitute source at each occurrence, not independently
                // rendered HTML: set/with locals and conditional paths survive.
                self.flatten(&key, child, depth + 1, out)?;
            }
            cursor = site.end;
        }
        append_preview_fragment(out, &source[cursor..])
    }
}

pub(crate) fn append_preview_fragment(
    out: &mut String,
    fragment: &str,
) -> Result<(), &'static str> {
    if fragment.len() > MAX_RENDERED_HTML_BYTES.saturating_sub(out.len()) {
        return Err("HTML preview vượt quá giới hạn 512 KiB.");
    }
    out.push_str(fragment);
    Ok(())
}
