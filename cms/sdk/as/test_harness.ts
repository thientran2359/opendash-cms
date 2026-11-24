import * as opendash from "./opendash";
import { JSON } from "assemblyscript-json/assembly";

export function opendash_run(): i32 {
  const env = opendash.readEnvelope();
  if (env === null) {
    return 1;
  }

  if (env.event == "test/envelope_v1") {
    if (
      env.envelopeVersion != 1 ||
      env.kind != "hook" ||
      env.principal != "system" ||
      env.correlationId != "corr-v1" ||
      env.occurredAtMs != 111
    ) {
      return 2;
    }
    const hello = env.payload.getString("hello");
    if (hello === null || hello.valueOf() != "world") {
      return 3;
    }
    return opendash.outputAllow() < 0 ? 4 : 0;
  }

  if (env.event == "test/envelope_v2") {
    if (
      env.envelopeVersion != 2 ||
      env.kind != "filter" ||
      env.locale != "vi" ||
      env.principal != "editor" ||
      env.dispatchDepth != 2 ||
      env.correlationId != "corr-v2" ||
      env.occurredAtMs != 222
    ) {
      return 2;
    }
    const fieldErrors = new JSON.Obj();
    fieldErrors.set("slug", "already taken");
    return opendash.outputVeto("bad_slug", "Slug invalid", fieldErrors) < 0 ? 4 : 0;
  }

  if (env.event == "test/oversized_readback") {
    const oversized = opendash.contentList("post", "vi", 50, 0, true);
    return opendash.setOutput(oversized) < 0 ? 4 : 0;
  }

  if (env.event == "test/all_host_ops") {
    const summary = new JSON.Obj();

    const v1Kv = opendash.kvGet("legacy_key");
    summary.set("kv_v1", v1Kv !== null ? v1Kv : "missing");

    const v2Kv = opendash.kvGet("v2_key");
    summary.set("kv_v2", v2Kv !== null ? v2Kv : "missing");

    summary.set("kv_set", opendash.kvSet("k_new", '{"a":1}'));
    summary.set("kv_delete", opendash.kvDelete("k_old"));
    summary.set("kv_list", JSON.parse(opendash.kvList("k_", 10)));
    summary.set(
      "http",
      JSON.parse(
        opendash.httpRequest(
          "POST",
          "https://api.example.org/hook",
          '{"X-Test":"1"}',
          '{"ok":true}'
        )
      )
    );
    summary.set("content_get", JSON.parse(opendash.contentGet("post", 99, "en")));
    summary.set("content_list", JSON.parse(opendash.contentList("post", null, 20, 0, false)));

    const saveObj = new JSON.Obj();
    saveObj.set("type_key", "post");
    saveObj.set("slug", "new-post");
    saveObj.set("title", "New Post");
    saveObj.set("values", new JSON.Obj());
    summary.set("content_save", JSON.parse(opendash.contentSave(saveObj)));

    summary.set("content_delete", JSON.parse(opendash.contentDelete("post", 100, 3)));
    summary.set("taxonomy_list", JSON.parse(opendash.taxonomyList("category", "vi", 25)));
    summary.set("media_get", JSON.parse(opendash.mediaGet("media_abc")));
    summary.set("site_options_get", JSON.parse(opendash.siteOptionsGet("vi")));
    summary.set("plugin_settings_get", JSON.parse(opendash.pluginSettingsGet()));

    const secretFound = opendash.pluginSecretGet("api_key");
    summary.set("secret_found", secretFound !== null ? secretFound : "");
    const secretMissing = opendash.pluginSecretGet("missing_key");
    summary.set("secret_missing_is_null", secretMissing === null);

    summary.set("log_ok", opendash.log("info", "hello from harness"));

    return opendash.outputEntryView(summary) < 0 ? 4 : 0;
  }

  if (env.event == "test/outputs") {
    const which = env.payload.getString("which");
    const kind = which !== null ? which.valueOf() : "";
    if (kind == "prepare_save") {
      const patch = new JSON.Obj();
      patch.set("title", "Normalized Title");
      const valuesPatch = new JSON.Obj();
      valuesPatch.set("badge", "Verified");
      patch.set("values_patch", valuesPatch);
      return opendash.outputPrepareSavePatch(patch) < 0 ? 4 : 0;
    }
    if (kind == "contact_allow") {
      return opendash.outputContactAllow() < 0 ? 4 : 0;
    }
    if (kind == "contact_reject") {
      return opendash.outputContactReject("spam", "Blocked message") < 0 ? 4 : 0;
    }
    if (kind == "seo_head") {
      const seo = new JSON.Obj();
      seo.set("title", "SEO Override Title");
      seo.set("noindex", false);
      return opendash.outputSeoHead(seo) < 0 ? 4 : 0;
    }
    if (kind == "entry_view") {
      const ext = new JSON.Obj();
      ext.set("badge", "Featured");
      return opendash.outputEntryView(ext) < 0 ? 4 : 0;
    }
    return 5;
  }

  return 6;
}
