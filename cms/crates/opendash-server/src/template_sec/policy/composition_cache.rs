use super::*;
/// The permitted number of content slots depends on the resolved template target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub enum CsecContentKind {
    /// Home, Page, PageLayout, Single* and Singular require exactly one slot.
    Entry,
    /// Index may omit the slot or include it once.
    Index,
    /// Archive*, Taxonomy*, NotFound and template parts cannot inject content.
    NonEntry,
}

impl ThemeCompositionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn global() -> &'static Arc<Self> {
        static GLOBAL: OnceLock<Arc<ThemeCompositionCache>> = OnceLock::new();
        GLOBAL.get_or_init(|| Arc::new(Self::new()))
    }

    pub fn clear(&self) {
        self.inner
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub fn get_or_compile(
        &self,
        revision_digest: &str,
        target_key: &str,
        source: &str,
        parts: &BTreeMap<String, String>,
        content_kind: CsecContentKind,
        policy: &ThemeUrlPolicy,
    ) -> Result<Arc<CompiledCsecComposition>, &'static str> {
        let compile = || {
            validate_and_compile_csec_composition_with_theme_urls(
                target_key,
                source,
                parts,
                content_kind,
                policy,
            )
            .map(Arc::new)
        };
        if revision_digest.len() != 64 || !revision_digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return compile();
        }
        let key = ThemeCompositionCacheKey {
            revision_digest: revision_digest.to_owned(),
            target_key: target_key.to_owned(),
            content_kind: match content_kind {
                CsecContentKind::Entry => 0,
                CsecContentKind::Index => 1,
                CsecContentKind::NonEntry => 2,
            },
            entry_fields_key: policy.entry_fields_fingerprint(),
        };
        if let Some(found) = self
            .inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
        {
            return Ok(found);
        }
        let compiled = compile()?;
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = guard.get(&key).cloned() {
            return Ok(found);
        }
        if guard.len() >= MAX_THEME_CACHE_ENTRIES {
            guard.clear();
        }
        guard.insert(key, compiled.clone());
        Ok(compiled)
    }
}

impl CompiledCsecComposition {
    pub(crate) fn part_expansions(&self) -> usize {
        self.part_expansions
    }
    /// Renders the C-SEC shell before substituting sanitized entry HTML.
    ///
    /// The injected HTML was sanitized by the content renderer and can contain
    /// a contact form. The theme shell itself is checked with the strict
    /// template allowlist, including the ban on forms and scripts.
    pub fn render(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
    ) -> Result<String, &'static str> {
        let mut remaining = TEMPLATE_FUEL_LIMIT;
        self.render_with_remaining_fuel(context, sanitized_content_html, &mut remaining)
    }

    /// Public Builder composition can contain multiple independently compiled
    /// theme segments; all of them consume the same request-local fuel.
    pub(crate) fn render_with_remaining_fuel(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
        fuel_remaining: &mut u64,
    ) -> Result<String, &'static str> {
        self.render_with_config_and_remaining_fuel(
            context,
            sanitized_content_html,
            fuel_remaining,
            &RenderFormattingConfig::default(),
        )
    }

    /// Renders the compiled theme shell with host-configured date/timezone formatting
    /// while sharing the request-local fuel budget.
    pub(crate) fn render_with_config_and_remaining_fuel(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
        fuel_remaining: &mut u64,
        fmt_cfg: &RenderFormattingConfig,
    ) -> Result<String, &'static str> {
        let mut budget = RenderBudget::new();
        budget.fuel_remaining = *fuel_remaining;
        let result = self.render_with_config_and_budget(
            context,
            sanitized_content_html,
            &mut budget,
            fmt_cfg,
        );
        *fuel_remaining = budget.fuel_remaining;
        result
    }

    pub(crate) fn render_with_config_and_budget(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
        budget: &mut RenderBudget,
        fmt_cfg: &RenderFormattingConfig,
    ) -> Result<String, &'static str> {
        if budget.fuel_remaining == 0 || budget.fuel_remaining > TEMPLATE_FUEL_LIMIT {
            return Err("Fuel composition không hợp lệ hoặc đã hết.");
        }
        let nonce = if self.content_count == 1 {
            let mut bytes = [0_u8; 16];
            OsRng.fill_bytes(&mut bytes);
            let mut nonce = String::from("OD_CONTENT_");
            for byte in bytes {
                use std::fmt::Write;
                write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce.")?;
            }
            Some(nonce)
        } else {
            None
        };

        let source = if let Some(nonce) = &nonce {
            self.source.replace(CONTENT_SLOT_TOKEN, nonce)
        } else {
            self.source.clone()
        };
        let mut env = restricted_environment_with_config(fmt_cfg);
        let local = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let local_images = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        install_icon_function(
            &mut env,
            &budget.icon_nonces,
            &local,
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
        env.add_template(&self.name, &source)
            .map_err(|_| "Không thể biên dịch theme composition.")?;
        let captured = env
            .get_template(&self.name)
            .map_err(|_| "Không tìm thấy theme composition.")?
            .render_captured(context)
            .map_err(|_| {
                if overflowed.load(std::sync::atomic::Ordering::Relaxed) {
                    budget.html_bytes_used = MAX_RENDERED_HTML_BYTES + 1;
                }
                "Lỗi thực thi theme composition (fuel/recursion/undefined)."
            })?;
        let (consumed, remaining) = captured
            .state()
            .fuel_levels()
            .ok_or("Thiếu bộ theo dõi fuel composition.")?;
        if budget.fuel_remaining.checked_sub(consumed) != Some(remaining) {
            return Err("Fuel composition không hợp lệ.");
        }
        budget.fuel_remaining = remaining;
        let shell = captured.output();
        if shell.len() > MAX_RENDERED_HTML_BYTES {
            return Err("HTML render vượt quá giới hạn 512 KiB.");
        }
        validate_html_and_css_context(&shell, true, true, true)?;
        let out = if let Some(nonce) = nonce {
            if shell.matches(&nonce).count() != 1 {
                return Err("Nonce nội dung của theme không xuất hiện đúng một lần.");
            }
            let content = sanitized_content_html.unwrap_or("");
            let final_len = shell.len() - nonce.len() + content.len();
            if final_len > MAX_RENDERED_HTML_BYTES {
                return Err("HTML sau khi chèn nội dung vượt quá giới hạn 512 KiB.");
            }
            shell.replacen(&nonce, content, 1)
        } else {
            shell.to_owned()
        };
        let additions = local.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let icon_growth: usize = additions
            .iter()
            .map(|(n, s)| s.len().saturating_sub(n.len()))
            .sum();
        budget.charge_html_bytes(icon_growth)?;
        budget.icon_nonces.extend(additions);
        let image_additions = local_images
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let image_growth: usize = image_additions
            .iter()
            .map(|(n, s)| s.len().saturating_sub(n.len()))
            .sum();
        budget.charge_html_bytes(image_growth)?;
        budget.image_nonces.extend(image_additions);
        let hook_additions = local_hooks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        budget.hook_nonces.extend(hook_additions);
        Ok(out)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn flatten_composition(
    source: &str,
    sites: &[CompositionSite],
    parts: &BTreeMap<String, String>,
    part_sites: &BTreeMap<String, Vec<CompositionSite>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
    expansions: &mut usize,
    content_count: &mut usize,
    max_part_depth: &mut usize,
) -> Result<String, &'static str> {
    let mut out = String::new();
    let mut cursor = 0_usize;
    for site in sites {
        out.push_str(&source[cursor..site.start]);
        match &site.call {
            CompositionCall::Content => {
                *content_count += 1;
                out.push_str(CONTENT_SLOT_TOKEN);
            }
            CompositionCall::Part(key) => {
                *max_part_depth = (*max_part_depth).max(depth + 1);
                *expansions += 1;
                if *expansions > MAX_COMPOSITION_EXPANSIONS {
                    return Err("od_part vượt quá 32 lần mở rộng.");
                }
                if depth >= MAX_COMPOSITION_PART_DEPTH {
                    return Err("Độ sâu od_part vượt giới hạn 3.");
                }
                if !visiting.insert(key.to_owned()) {
                    return Err("Phát hiện vòng lặp od_part.");
                }
                let part = parts
                    .get(key)
                    .ok_or("od_part trỏ tới part không tồn tại.")?;
                let child_sites = part_sites.get(key).ok_or("Thiếu part đã kiểm tra.")?;
                let child = flatten_composition(
                    part,
                    child_sites,
                    parts,
                    part_sites,
                    visiting,
                    depth + 1,
                    expansions,
                    content_count,
                    max_part_depth,
                )?;
                visiting.remove(key);
                out.push_str(&child);
            }
        }
        if out.len() > MAX_RENDERED_HTML_BYTES {
            return Err("Template composition vượt giới hạn 512 KiB.");
        }
        cursor = site.end;
    }
    out.push_str(&source[cursor..]);
    if out.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template composition vượt giới hạn 512 KiB.");
    }
    Ok(out)
}

pub(crate) fn composition_sites(
    name: &str,
    source: &str,
    is_part: bool,
    policy: &ThemeUrlPolicy,
    allow_search_form: bool,
) -> Result<Vec<CompositionSite>, &'static str> {
    composition_sites_with_context(
        name,
        source,
        is_part,
        !is_part && is_taxonomy_target_name(name),
        policy,
        allow_search_form,
    )
}

pub(crate) fn composition_sites_with_context(
    name: &str,
    source: &str,
    is_part: bool,
    is_taxonomy_template: bool,
    policy: &ThemeUrlPolicy,
    allow_search_form: bool,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(CONTENT_SLOT_TOKEN)
        || source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(BLOCK_CHILDREN_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(IMAGE_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    reject_reserved_context_rebinding_for(&ast, is_taxonomy_template)?;
    let is_full_doctype_doc = !is_part
        && source
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype");
    let _ = walk_theme_ast_with_url_scopes_for(
        &ast,
        is_part,
        is_full_doctype_doc,
        is_taxonomy_template,
        policy,
        true,
        allow_search_form,
    )?;
    let mut sites = Vec::new();
    collect_composition_sites(&ast, source, false, &mut sites)?;
    sites.sort_by_key(|site| site.start);
    let mut cursor = 0;
    for site in &sites {
        if site.start < cursor || site.end > source.len() {
            return Err("Vị trí od_part/od_content không hợp lệ.");
        }
        if is_part && matches!(site.call, CompositionCall::Content) {
            return Err("Template part không được chứa od_content().");
        }
        cursor = site.end;
    }
    Ok(sites)
}
