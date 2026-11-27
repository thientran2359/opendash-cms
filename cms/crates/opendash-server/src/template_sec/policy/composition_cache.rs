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
        let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        install_icon_function(
            &mut env,
            &budget.icon_nonces,
            &local,
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
        validate_html_and_css_context(&shell, true, true)?;
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
        let hook_additions = local_hooks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        budget.hook_nonces.extend(hook_additions);
        Ok(out)
    }
}
