use std::collections::BTreeMap;

use opendash_plugin_sdk::{
    EntryViewOutput, Envelope, HttpRequestInput, LogLevel, SdkError, SeoHeadOutput, export_plugin,
    http_request, kv_get, kv_set, log, plugin_secret_get, plugin_settings_get,
    serde_json::{self, Value, json},
    write_output,
};

const KV_PUBLISHED_PING_COUNT: &str = "published_ping_count";
const SEO_PING_URL: &str = "https://hooks.example.com/seo-ping";
const MAX_SEO_DESCRIPTION_CHARS: usize = 160;

fn trimmed_setting_or(
    settings: &serde_json::Map<String, Value>,
    key: &str,
    fallback: &str,
) -> String {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn non_empty_payload_str<'a>(val: Option<&'a Value>) -> Option<&'a str> {
    val.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        collected.trim_end().to_owned()
    } else {
        collected
    }
}

fn handle_seo_head(env: &Envelope) -> Result<(), SdkError> {
    let settings = plugin_settings_get()?;
    let brand_name = trimmed_setting_or(&settings, "brand_name", "OpenDash");

    let entry_title = non_empty_payload_str(env.payload.get("title")).unwrap_or("Untitled");
    let type_key = non_empty_payload_str(env.payload.get("type_key"))
        .or_else(|| non_empty_payload_str(env.payload.get("content_type_key")))
        .unwrap_or("post");
    let description = env
        .payload
        .get("values")
        .and_then(Value::as_object)
        .and_then(|values| {
            non_empty_payload_str(values.get("summary"))
                .or_else(|| non_empty_payload_str(values.get("excerpt")))
        })
        .map(|s| truncate_chars(s, MAX_SEO_DESCRIPTION_CHARS));

    let schema_type = if type_key == "product" {
        "Product"
    } else {
        "Article"
    };
    let json_ld = vec![json!({
        "@context": "https://schema.org",
        "@type": schema_type,
        "headline": entry_title,
        "publisher": {
            "@type": "Organization",
            "name": brand_name,
        },
    })];

    let out = SeoHeadOutput {
        title: Some(format!("{entry_title} | {brand_name}")),
        description,
        og_image_id: None,
        canonical_url: None,
        noindex: Some(false),
        json_ld,
    };
    write_output(&out)
}

fn handle_entry_view(_env: &Envelope) -> Result<(), SdkError> {
    let settings = plugin_settings_get()?;
    let badge = trimmed_setting_or(&settings, "default_badge", "Mới");
    let brand_name = trimmed_setting_or(&settings, "brand_name", "OpenDash");

    let mut ext = EntryViewOutput::new();
    ext.insert("badge".to_owned(), json!(badge));
    ext.insert("seo_managed".to_owned(), json!(true));
    ext.insert("brand_name".to_owned(), json!(brand_name));
    write_output(&ext)
}

fn handle_after_publish(env: &Envelope) -> Result<(), SdkError> {
    let prev_count = kv_get(KV_PUBLISHED_PING_COUNT)?
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let next_count = prev_count + 1;
    kv_set(KV_PUBLISHED_PING_COUNT, &json!(next_count))?;

    let token_opt = plugin_secret_get("webhook_token")?;
    let mut webhook_sent = false;
    if let Some(token) = token_opt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let slug = non_empty_payload_str(env.payload.get("slug")).unwrap_or("");
        let type_key = non_empty_payload_str(env.payload.get("type_key"))
            .or_else(|| non_empty_payload_str(env.payload.get("content_type_key")))
            .unwrap_or("post");
        let entry_id = env.payload.get("entry_id").and_then(Value::as_i64);
        let body = serde_json::to_string(&json!({
            "event": env.event,
            "entry_id": entry_id,
            "type_key": type_key,
            "slug": slug,
            "locale": env.locale,
            "count": next_count,
        }))
        .map_err(|e| SdkError::Json(e.to_string()))?;

        let resp = http_request(&HttpRequestInput {
            method: "POST",
            url: SEO_PING_URL,
            headers: BTreeMap::from([
                ("authorization".to_owned(), format!("Bearer {token}")),
                ("content-type".to_owned(), "application/json".to_owned()),
            ]),
            body: Some(&body),
            timeout_ms: Some(3000),
        })?;
        webhook_sent = (200..300).contains(&resp.status);
    }

    log(
        LogLevel::Info,
        &format!("processed content/after_publish #{next_count} (webhook_sent={webhook_sent})"),
    )?;
    write_output(&json!({
        "ok": true,
        "count": next_count,
        "webhook_sent": webhook_sent,
    }))
}

pub fn handle(env: Envelope) -> Result<(), SdkError> {
    match env.event.as_str() {
        "filter:seo/head" => handle_seo_head(&env),
        "filter:entry/view" => handle_entry_view(&env),
        "content/after_publish" => handle_after_publish(&env),
        other => write_output(&json!({ "ok": true, "ignored_event": other })),
    }
}

export_plugin!(handle);

#[cfg(test)]
mod tests {
    use opendash_plugin_sdk::{HttpResponse, MockHost, run_in_mock_host};

    use super::*;

    fn sample_envelope(event: &str, payload: Value) -> Envelope {
        Envelope {
            envelope_version: 2,
            kind: if event.starts_with("filter:") {
                "filter".to_owned()
            } else {
                "hook".to_owned()
            },
            event: event.to_owned(),
            occurred_at_ms: 1_700_000_000_000,
            correlation_id: "corr-rust-seo".to_owned(),
            locale: Some("vi".to_owned()),
            principal: "system".to_owned(),
            dispatch_depth: 0,
            payload,
        }
    }

    #[test]
    fn manifest_json_conforms_to_v3_contract() {
        let raw: Value =
            serde_json::from_str(include_str!("../manifest.json")).expect("valid manifest JSON");
        assert_eq!(raw["id"], "rust-seo-plugin");
        assert_eq!(raw["api_version"], 1);
        assert_eq!(raw["manifest_schema"], 3);
        assert_eq!(raw["min_host_version"], "0.1.0");
        assert_eq!(raw["entrypoint"], "plugin.wasm");
        assert_eq!(raw["hooks"], json!(["content/after_publish"]));
        assert_eq!(
            raw["filters"],
            json!(["filter:seo/head", "filter:entry/view"])
        );
        assert_eq!(raw["permissions"]["content_read"], json!(["*"]));
        assert_eq!(raw["permissions"]["plugin_settings_read"], json!(true));
        assert_eq!(
            raw["permissions"]["plugin_secrets_read"],
            json!(["webhook_token"])
        );

        let schema = raw["settings_schema"]
            .as_array()
            .expect("settings_schema array");
        let secret_field = schema
            .iter()
            .find(|f| f["key"] == "webhook_token")
            .expect("webhook_token field");
        assert_eq!(secret_field["type"], "secret");
        assert!(secret_field["default"].is_null());
    }

    #[test]
    fn filter_seo_head_builds_title_description_and_json_ld() {
        let mut host = MockHost::default();
        host.plugin_settings
            .insert("brand_name".to_owned(), json!("OpenDash Việt Nam"));

        let env = sample_envelope(
            "filter:seo/head",
            json!({
                "entry_id": 42,
                "type_key": "post",
                "title": "Hướng dẫn Plugin SDK",
                "values": {
                    "summary": "Tóm tắt ngắn gọn về Rust Guest SDK và C-SEC."
                }
            }),
        );

        let outcome = run_in_mock_host(host, &env, || {
            let loaded = opendash_plugin_sdk::read_envelope()?;
            handle(loaded)
        });
        assert_eq!(outcome.result, Ok(()));

        let out: SeoHeadOutput =
            serde_json::from_slice(outcome.output_bytes.as_deref().unwrap()).unwrap();
        assert_eq!(
            out.title.as_deref(),
            Some("Hướng dẫn Plugin SDK | OpenDash Việt Nam")
        );
        assert_eq!(
            out.description.as_deref(),
            Some("Tóm tắt ngắn gọn về Rust Guest SDK và C-SEC.")
        );
        assert_eq!(out.noindex, Some(false));
        assert_eq!(out.json_ld.len(), 1);
        assert_eq!(out.json_ld[0]["@context"], "https://schema.org");
        assert_eq!(out.json_ld[0]["@type"], "Article");
        assert_eq!(out.json_ld[0]["headline"], "Hướng dẫn Plugin SDK");
        assert_eq!(out.json_ld[0]["publisher"]["name"], "OpenDash Việt Nam");
    }

    #[test]
    fn filter_entry_view_emits_configured_and_default_badge() {
        let mut custom_host = MockHost::default();
        custom_host
            .plugin_settings
            .insert("default_badge".to_owned(), json!("Nổi bật"));
        custom_host
            .plugin_settings
            .insert("brand_name".to_owned(), json!("OpenDash"));

        let env = sample_envelope("filter:entry/view", json!({ "entry_id": 42 }));
        let outcome = run_in_mock_host(custom_host, &env, || {
            let loaded = opendash_plugin_sdk::read_envelope()?;
            handle(loaded)
        });
        assert_eq!(outcome.result, Ok(()));
        let out: Value = serde_json::from_slice(outcome.output_bytes.as_deref().unwrap()).unwrap();
        assert_eq!(
            out,
            json!({
                "badge": "Nổi bật",
                "seo_managed": true,
                "brand_name": "OpenDash"
            })
        );

        // Fallback to default values when plugin settings are unset
        let fallback_outcome = run_in_mock_host(MockHost::default(), &env, || {
            let loaded = opendash_plugin_sdk::read_envelope()?;
            handle(loaded)
        });
        assert_eq!(fallback_outcome.result, Ok(()));
        let fallback_out: Value =
            serde_json::from_slice(fallback_outcome.output_bytes.as_deref().unwrap()).unwrap();
        assert_eq!(
            fallback_out,
            json!({
                "badge": "Mới",
                "seo_managed": true,
                "brand_name": "OpenDash"
            })
        );
    }

    #[test]
    fn content_after_publish_increments_kv_sends_secret_bearer_without_leaking_secret() {
        let mut host = MockHost::default();
        host.kv.insert(KV_PUBLISHED_PING_COUNT.to_owned(), json!(2));
        host.plugin_secrets.insert(
            "webhook_token".to_owned(),
            "sec-token-do-not-log".to_owned(),
        );
        host.http_handler = Some(Box::new(|method, url, headers, body| {
            assert_eq!(method, "POST");
            assert_eq!(url, SEO_PING_URL);
            assert_eq!(
                headers.get("authorization").map(String::as_str),
                Some("Bearer sec-token-do-not-log")
            );
            let parsed: Value = serde_json::from_str(body.unwrap_or("{}")).unwrap();
            assert_eq!(parsed["entry_id"], 99);
            assert_eq!(parsed["slug"], "bai-viet-moi");
            assert_eq!(parsed["count"], 3);
            Ok(HttpResponse {
                status: 202,
                headers: BTreeMap::new(),
                body: "{\"ok\":true}".to_owned(),
            })
        }));

        let env = sample_envelope(
            "content/after_publish",
            json!({
                "entry_id": 99,
                "type_key": "post",
                "slug": "bai-viet-moi"
            }),
        );
        let outcome = run_in_mock_host(host, &env, || {
            let loaded = opendash_plugin_sdk::read_envelope()?;
            handle(loaded)
        });
        assert_eq!(outcome.result, Ok(()));
        assert_eq!(outcome.kv.get(KV_PUBLISHED_PING_COUNT), Some(&json!(3)));

        let out_bytes = outcome.output_bytes.as_deref().unwrap();
        let out_str = std::str::from_utf8(out_bytes).unwrap();
        assert!(!out_str.contains("sec-token-do-not-log"));
        let out_json: Value = serde_json::from_slice(out_bytes).unwrap();
        assert_eq!(
            out_json,
            json!({ "ok": true, "count": 3, "webhook_sent": true })
        );

        assert_eq!(outcome.logs.len(), 1);
        assert_eq!(outcome.logs[0].0, LogLevel::Info);
        assert!(!outcome.logs[0].1.contains("sec-token-do-not-log"));

        // When webhook_token is missing, increments KV counter cleanly without HTTP calls
        let no_secret_outcome = run_in_mock_host(MockHost::default(), &env, || {
            let loaded = opendash_plugin_sdk::read_envelope()?;
            handle(loaded)
        });
        assert_eq!(no_secret_outcome.result, Ok(()));
        assert_eq!(
            no_secret_outcome.kv.get(KV_PUBLISHED_PING_COUNT),
            Some(&json!(1))
        );
        let no_secret_out: Value =
            serde_json::from_slice(no_secret_outcome.output_bytes.as_deref().unwrap()).unwrap();
        assert_eq!(
            no_secret_out,
            json!({ "ok": true, "count": 1, "webhook_sent": false })
        );
    }
}
