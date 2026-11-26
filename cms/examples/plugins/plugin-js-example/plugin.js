// Reference zero-build JS Plugin for OpenDash CMS (Boa ES2024 sandbox).
// Type definitions: ../../../sdk/js/opendash.d.ts

od.addFilter("filter:render/footer", () => {
  const resp = od.settings.get() || {};
  const settings = resp.settings || {};
  const note = typeof settings.footer_note === "string" && settings.footer_note
    ? settings.footer_note
    : "Powered by OpenDash JS Plugin";
  return '<div class="od-plugin-footer">' + od.escapeHtml(note) + "</div>";
});

od.addFilter("filter:render/slot", (env) => {
  const payload = env && env.payload ? env.payload : {};
  if (payload.slot !== "plugin_badge") return "";
  return '<span class="od-plugin-badge">JS Plugin Active</span>';
});

od.addRoute("status", () => {
  const hits = Number(od.kv.get("echo_hits") || 0);
  return {
    status: 200,
    content_type: "application/json",
    json: { ok: true, plugin: "plugin-js-example", echo_hits: hits }
  };
});

od.addRoute("echo", (env) => {
  const req = env && env.payload ? env.payload : {};
  const hits = Number(od.kv.get("echo_hits") || 0) + 1;
  od.kv.set("echo_hits", hits);
  return {
    status: 200,
    content_type: "application/json",
    json: { ok: true, hits, received: req.body || null }
  };
});

od.addAction("reset_counter", () => {
  od.kv.set("echo_hits", 0);
  od.log.info("echo_hits reset to 0");
  return { reset: true };
});
