use super::*;

pub const JS_LOOP_ITERATION_LIMIT: u64 = 5_000_000;

pub const JS_RECURSION_LIMIT: usize = 128;

pub const JS_STACK_SIZE_LIMIT: usize = 512;

pub const MAX_JS_OUTPUT_BYTES: usize = 1024 * 1024;

pub(super) struct JsHostBridge {
    pub(super) ctx: HostContext,
    pub(super) in_registration: bool,
}

thread_local! {
    pub(super) static ACTIVE_HOST: RefCell<Option<JsHostBridge>> = const { RefCell::new(None) };
}

pub(super) struct ActiveHostGuard;

impl Drop for ActiveHostGuard {
    fn drop(&mut self) {
        ACTIVE_HOST.with(|cell| {
            let _ = cell.borrow_mut().take();
        });
    }
}

pub(super) fn configure_runtime_limits(ctx: &mut Context) {
    let limits = ctx.runtime_limits_mut();
    limits.set_loop_iteration_limit(JS_LOOP_ITERATION_LIMIT);
    limits.set_recursion_limit(JS_RECURSION_LIMIT);
    limits.set_stack_size_limit(JS_STACK_SIZE_LIMIT);
}

/// Pure static JS syntax check without executing user code (`--check-theme` / `--check-plugin` / install).
pub fn validate_js_module_syntax(source: &str) -> Result<(), String> {
    let mut ctx = Context::default();
    configure_runtime_limits(&mut ctx);
    Script::parse(Source::from_bytes(source.as_bytes()), None, &mut ctx)
        .map(|_| ())
        .map_err(|err| format!("Lỗi cú pháp JavaScript: {err}"))
}

pub(super) fn js_internal_host_call(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let req_str = args
        .get_or_undefined(0)
        .to_string(context)?
        .to_std_string_escaped();

    let resp_json = ACTIVE_HOST.with(|cell| -> Result<String, JsError> {
        let mut borrow = cell.borrow_mut();
        let Some(bridge) = borrow.as_mut() else {
            return Err(JsNativeError::error()
                .with_message("OpenDash host bridge không khả dụng.")
                .into());
        };
        if bridge.in_registration {
            return Err(JsNativeError::error()
                .with_message(
                    "top_level_host_call_forbidden: Không được gọi Host Op ở top-level; hãy gọi bên trong callback od.on/addFilter/addAction/addRoute.",
                )
                .into());
        }
        if bridge.ctx.is_cancelled() {
            return Err(JsNativeError::error()
                .with_message("deadline exceeded")
                .into());
        }
        let resp = host_ops::handle_host_call(&mut bridge.ctx, &req_str);
        if bridge.ctx.is_cancelled() {
            return Err(JsNativeError::error()
                .with_message("deadline exceeded")
                .into());
        }
        serde_json::to_string(&resp).map_err(|e| {
            JsNativeError::error()
                .with_message(format!("Internal host response JSON error: {e}"))
                .into()
        })
    })?;

    Ok(JsValue::from(js_string!(resp_json)))
}

pub(super) const OD_BOOTSTRAP_JS: &str = r#"
(function() {
  const _rawHostCall = globalThis.__od_internal_host_call;
  delete globalThis.__od_internal_host_call;

  const _hooks = Object.create(null);
  const _filters = Object.create(null);
  const _actions = Object.create(null);
  const _routes = Object.create(null);

  function _invoke(opObj) {
    const raw = _rawHostCall(JSON.stringify(opObj));
    const resp = JSON.parse(raw);
    if (!resp || resp.ok !== true) {
      const code = (resp && resp.code) ? String(resp.code) : "host_call_failed";
      const msg = (resp && resp.error) ? String(resp.error) : "host_call_failed";
      throw new Error(msg.startsWith("[") ? msg : ("[" + code + "] " + msg));
    }
    return resp;
  }

  function _ensureReg(kind, name, fn) {
    if (typeof name !== "string" || name.length === 0) {
      throw new TypeError("od." + kind + " requires a non-empty string name.");
    }
    if (typeof fn !== "function") {
      throw new TypeError("od." + kind + " requires a handler function.");
    }
  }

  function _escapeHtml(input) {
    return String(input == null ? "" : input)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function _escapeAttr(input) {
    return _escapeHtml(input).replace(/`/g, "&#96;");
  }

  const odApi = Object.freeze({
    on(event, handler) {
      _ensureReg("on", event, handler);
      if (!_hooks[event]) _hooks[event] = [];
      _hooks[event].push(handler);
    },
    addFilter(filterName, handler) {
      _ensureReg("addFilter", filterName, handler);
      if (!_filters[filterName]) _filters[filterName] = [];
      _filters[filterName].push(handler);
    },
    addAction(actionId, handler) {
      _ensureReg("addAction", actionId, handler);
      _actions[actionId] = handler;
    },
    addRoute(routeId, handler) {
      _ensureReg("addRoute", routeId, handler);
      _routes[routeId] = handler;
    },
    kv: Object.freeze({
      get(key) {
        const resp = _invoke({ op: "kv_get", key: String(key) });
        return resp.found ? resp.value : null;
      },
      set(key, value) {
        _invoke({ op: "kv_set", key: String(key), value: value === undefined ? null : value });
      },
      delete(key) {
        const resp = _invoke({ op: "kv_delete", key: String(key) });
        return resp.data || { deleted: true };
      },
      list(prefix, limit) {
        const req = { op: "kv_list" };
        if (prefix !== undefined && prefix !== null) req.prefix = String(prefix);
        if (limit !== undefined && limit !== null) req.limit = Number(limit);
        const resp = _invoke(req);
        return resp.data || { items: [] };
      },
    }),
    i18n: Object.freeze({
      __(msgid, domain) {
        const req = { op: "l10n_translate", msgid: String(msgid) };
        if (domain !== undefined && domain !== null) req.text_domain = String(domain);
        const resp = _invoke(req);
        return (resp && resp.data && typeof resp.data.text === "string") ? resp.data.text : String(msgid);
      },
      _x(msgid, context, domain) {
        const req = { op: "l10n_translate", msgid: String(msgid), context: String(context) };
        if (domain !== undefined && domain !== null) req.text_domain = String(domain);
        const resp = _invoke(req);
        return (resp && resp.data && typeof resp.data.text === "string") ? resp.data.text : String(msgid);
      },
      _n(singular, plural, count, domain) {
        const n = Number(count);
        const req = { op: "l10n_translate", msgid: String(singular), msgid_plural: String(plural), count: n };
        if (domain !== undefined && domain !== null) req.text_domain = String(domain);
        const resp = _invoke(req);
        return (resp && resp.data && typeof resp.data.text === "string") ? resp.data.text : (n === 1 ? String(singular) : String(plural));
      },
    }),
    content: Object.freeze({
      get(typeKey, entryId, locale) {
        const req = { op: "content_get", type_key: String(typeKey), entry_id: Number(entryId) };
        if (locale !== undefined && locale !== null) req.locale = String(locale);
        return _invoke(req).data;
      },
      list(params, maybeOpts) {
        let req;
        if (typeof params === "string") {
          const opts = (maybeOpts && typeof maybeOpts === "object") ? maybeOpts : {};
          req = { op: "content_list", type_key: params };
          if (opts.locale !== undefined && opts.locale !== null) req.locale = String(opts.locale);
          if (opts.limit !== undefined && opts.limit !== null) req.limit = Number(opts.limit);
          if (opts.offset !== undefined && opts.offset !== null) req.offset = Number(opts.offset);
          if (opts.published_only !== undefined && opts.published_only !== null) req.published_only = Boolean(opts.published_only);
        } else {
          const p = (params && typeof params === "object") ? params : {};
          req = { op: "content_list", type_key: String(p.type_key || "") };
          if (p.locale !== undefined && p.locale !== null) req.locale = String(p.locale);
          if (p.limit !== undefined && p.limit !== null) req.limit = Number(p.limit);
          if (p.offset !== undefined && p.offset !== null) req.offset = Number(p.offset);
          if (p.published_only !== undefined && p.published_only !== null) req.published_only = Boolean(p.published_only);
        }
        return _invoke(req).data;
      },
      save(params) {
        const p = (params && typeof params === "object") ? params : {};
        const req = {
          op: "content_save",
          type_key: String(p.type_key || ""),
          values: (p.values && typeof p.values === "object") ? p.values : {},
        };
        if (p.entry_id !== undefined && p.entry_id !== null) req.entry_id = Number(p.entry_id);
        if (p.expected_version !== undefined && p.expected_version !== null) req.expected_version = Number(p.expected_version);
        if (p.locale !== undefined && p.locale !== null) req.locale = String(p.locale);
        if (p.slug !== undefined && p.slug !== null) req.slug = String(p.slug);
        if (p.title !== undefined && p.title !== null) req.title = String(p.title);
        if (Array.isArray(p.term_ids)) req.term_ids = p.term_ids.map(Number);
        _invoke(req);
      },
      delete(params) {
        const p = (params && typeof params === "object") ? params : {};
        return _invoke({
          op: "content_delete",
          type_key: String(p.type_key || ""),
          entry_id: Number(p.entry_id),
          expected_version: Number(p.expected_version),
        }).data;
      },
      trashGet(typeKey, entryId, locale) {
        const req = { op: "content_trash_get", type_key: String(typeKey), entry_id: Number(entryId) };
        if (locale !== undefined && locale !== null) req.locale = String(locale);
        return _invoke(req).data;
      },
      trashList(params, maybeOpts) {
        let req;
        if (typeof params === "string") {
          const opts = (maybeOpts && typeof maybeOpts === "object") ? maybeOpts : {};
          req = { op: "content_trash_list", type_key: params };
          if (opts.locale !== undefined && opts.locale !== null) req.locale = String(opts.locale);
          if (opts.limit !== undefined && opts.limit !== null) req.limit = Number(opts.limit);
          if (opts.offset !== undefined && opts.offset !== null) req.offset = Number(opts.offset);
        } else {
          const p = (params && typeof params === "object") ? params : {};
          req = { op: "content_trash_list", type_key: String(p.type_key || "") };
          if (p.locale !== undefined && p.locale !== null) req.locale = String(p.locale);
          if (p.limit !== undefined && p.limit !== null) req.limit = Number(p.limit);
          if (p.offset !== undefined && p.offset !== null) req.offset = Number(p.offset);
        }
        return _invoke(req).data;
      },
      restore(params) {
        const p = (params && typeof params === "object") ? params : {};
        return _invoke({
          op: "content_restore",
          type_key: String(p.type_key || ""),
          entry_id: Number(p.entry_id),
          expected_version: Number(p.expected_version),
        }).data;
      },
      purge(params) {
        const p = (params && typeof params === "object") ? params : {};
        return _invoke({
          op: "content_purge",
          type_key: String(p.type_key || ""),
          entry_id: Number(p.entry_id),
          expected_version: Number(p.expected_version),
        }).data;
      },
    }),
    taxonomy: Object.freeze({
      list(kind, locale, limit) {
        const req = { op: "taxonomy_list", kind: String(kind) };
        if (locale !== undefined && locale !== null) req.locale = String(locale);
        if (limit !== undefined && limit !== null) req.limit = Number(limit);
        return _invoke(req).data;
      },
    }),
    media: Object.freeze({
      get(mediaId) {
        return _invoke({ op: "media_get", media_id: String(mediaId) }).data;
      },
    }),
    site: Object.freeze({
      options(locale) {
        const req = { op: "site_options_get" };
        if (locale !== undefined && locale !== null) req.locale = String(locale);
        return _invoke(req).data;
      },
    }),
    settings: Object.freeze({
      get() {
        return _invoke({ op: "plugin_settings_get" }).data;
      },
    }),
    secret: Object.freeze({
      get(key) {
        return _invoke({ op: "plugin_secret_get", key: String(key) }).data;
      },
    }),
    http: Object.freeze({
      request(params) {
        const p = (params && typeof params === "object") ? params : {};
        const req = {
          op: "http_request",
          method: String(p.method || "GET"),
          url: String(p.url || ""),
          headers: (p.headers && typeof p.headers === "object") ? p.headers : {},
        };
        if (p.body !== undefined && p.body !== null) req.body = String(p.body);
        if (p.timeout_ms !== undefined && p.timeout_ms !== null) req.timeout_ms = Number(p.timeout_ms);
        const resp = _invoke(req);
        return {
          status: resp.status,
          headers: resp.headers || {},
          body: resp.body || "",
        };
      },
    }),
    log: Object.freeze({
      info(message) {
        _invoke({ op: "log", level: "info", message: String(message == null ? "" : message) });
      },
      warn(message) {
        _invoke({ op: "log", level: "warn", message: String(message == null ? "" : message) });
      },
      error(message) {
        _invoke({ op: "log", level: "error", message: String(message == null ? "" : message) });
      },
    }),
    escapeHtml: _escapeHtml,
    escapeAttr: _escapeAttr,
  });

  Object.defineProperty(globalThis, "od", {
    value: odApi,
    writable: false,
    enumerable: true,
    configurable: false,
  });

  return function __od_dispatch(env) {
    const kind = env && typeof env === "object" ? env.kind : null;
    const event = env && typeof env === "object" ? env.event : null;

    if (kind === "action") {
      const actionId = typeof event === "string" && event.startsWith("action:")
        ? event.slice("action:".length)
        : String(event || "");
      const fn = _actions[actionId] || (_hooks[event] && _hooks[event][0]);
      if (!fn) {
        return { ok: false, error_type: "invalid_output", error: "Không tìm thấy JS action handler '" + actionId + "'." };
      }
      const res = fn(env);
      return { ok: true, value: res === undefined ? {} : res };
    }

    if (kind === "route") {
      const routeId = (env.payload && typeof env.payload.route_id === "string")
        ? env.payload.route_id
        : (typeof event === "string" && event.startsWith("route:") ? event.slice("route:".length) : String(event || ""));
      const fn = _routes[routeId];
      if (!fn) {
        return { ok: false, error_type: "invalid_output", error: "Không tìm thấy JS route handler '" + routeId + "'." };
      }
      const res = fn(env);
      return { ok: true, value: res === undefined ? null : res };
    }

    if (kind === "filter") {
      const list = _filters[event];
      if (!list || list.length === 0) {
        return { ok: true, value: null };
      }
      let out = null;
      for (let i = 0; i < list.length; i++) {
        const res = list[i](env);
        if (res !== undefined) {
          out = res;
        }
      }
      return { ok: true, value: out };
    }

    if (kind === "manual") {
      const list = _hooks["plugin/manual_run"];
      if (!list || list.length === 0) {
        return { ok: false, error_type: "invalid_output", error: "Không tìm thấy JS handler cho 'plugin/manual_run'." };
      }
      let out = {};
      for (let i = 0; i < list.length; i++) {
        const res = list[i](env);
        if (res !== undefined) {
          out = res;
        }
      }
      return { ok: true, value: out };
    }

    const hookList = typeof event === "string" ? _hooks[event] : null;
    if (!hookList || hookList.length === 0) {
      if (event === "content/before_save") {
        return { ok: true, value: { decision: "allow" } };
      }
      return { ok: true, value: { ok: true } };
    }
    let out = event === "content/before_save" ? { decision: "allow" } : { ok: true };
    for (let i = 0; i < hookList.length; i++) {
      const res = hookList[i](env);
      if (res !== undefined) {
        out = res;
        if (event === "content/before_save" && res && typeof res === "object" && res.decision === "veto") {
          return { ok: true, value: res };
        }
      }
    }
    return { ok: true, value: out };
  };
})();
"#;
