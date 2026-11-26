// Reference Theme Functions script for OpenDash CMS (Boa ES2024 sandbox).
// Type definitions: ../../../sdk/js/opendash.d.ts

od.on("theme/after_activate", (env) => {
  const payload = env && env.payload ? env.payload : {};
  od.log.info("theme-js-starter activated: " + (payload.revision_id || ""));
  return { ok: true };
});

od.addFilter("filter:render/head", () => {
  return '<meta name="generator" content="OpenDash Theme JS Starter">';
});

od.addFilter("filter:render/slot", (env) => {
  const payload = env && env.payload ? env.payload : {};
  if (payload.slot !== "hero_banner") return "";
  const resp = od.settings.get() || {};
  const settings = resp.settings || {};
  const banner = typeof settings.banner_text === "string" && settings.banner_text
    ? settings.banner_text
    : "Welcome to OpenDash JS Theme";
  return '<div class="starter-banner">' + od.escapeHtml(banner) + "</div>";
});

od.addFilter("filter:entry/view", (env) => {
  const payload = env && env.payload ? env.payload : {};
  const title = payload.entry && payload.entry.title ? String(payload.entry.title) : "";
  return {
    ext: {
      word_count: title.trim() ? title.trim().split(/\s+/).length : 0
    }
  };
});

od.addRoute("hello", (env) => {
  const req = env && env.payload ? env.payload : {};
  return {
    status: 200,
    content_type: "application/json",
    json: {
      ok: true,
      theme: "theme-js-starter",
      path: req.path || ""
    }
  };
});

od.addAction("ping", () => {
  const prev = Number(od.kv.get("ping_count") || 0);
  const next = prev + 1;
  od.kv.set("ping_count", next);
  od.log.info("ping executed #" + next);
  return { pong: true, count: next };
});
