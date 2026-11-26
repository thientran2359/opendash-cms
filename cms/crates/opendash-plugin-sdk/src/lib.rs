pub mod abi;
pub mod host;
#[cfg(not(target_arch = "wasm32"))]
pub mod mock;
pub mod types;

pub use serde_json;

pub use host::{
    content_delete, content_get, content_list, content_purge, content_restore, content_save,
    content_trash_get, content_trash_list, http_request, kv_delete, kv_get, kv_list, kv_set, log,
    media_get, plugin_secret_get, plugin_settings_get, read_envelope, site_options_get,
    taxonomy_list, write_output,
};
#[cfg(not(target_arch = "wasm32"))]
pub use mock::{MockHost, MockHostOutcome, run_in_mock_host};
pub use types::{
    BeforeSaveDecision, ContactBeforeSubmitDecision, ContentEntry, ContentListResult,
    ContentPrepareSavePatch, ContentSaveInput, ContentTrashListResult, EntryViewOutput, Envelope,
    HostResponse, HttpRequestInput, HttpResponse, KvItem, LogLevel, MediaAsset, SdkError,
    SeoHeadOutput, TaxonomyTerm, TrashEntrySummary,
};

#[macro_export]
macro_rules! export_plugin {
    ($handler:path) => {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn opendash_run() -> i32 {
            let handler: fn($crate::Envelope) -> ::std::result::Result<(), $crate::SdkError> =
                $handler;
            match $crate::host::read_envelope().and_then(handler) {
                Ok(()) => 0,
                Err(err) => {
                    let _ = $crate::host::log($crate::LogLevel::Error, &err.to_string());
                    1
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Map, json};

    use super::*;

    fn sample_envelope_v2(event: &str, payload: serde_json::Value) -> Envelope {
        Envelope {
            envelope_version: 2,
            kind: "action".to_owned(),
            event: event.to_owned(),
            occurred_at_ms: 1_700_000_000_000,
            correlation_id: "corr-01".to_owned(),
            locale: Some("vi".to_owned()),
            principal: "admin".to_owned(),
            dispatch_depth: 0,
            payload,
        }
    }

    // Confirm export_plugin! expands cleanly on native and compiles fn pointer signature.
    fn _smoke_handler(env: Envelope) -> Result<(), SdkError> {
        write_output(&json!({ "event": env.event }))
    }
    export_plugin!(_smoke_handler);

    #[test]
    fn host_ops_all_15_and_dual_v1_v2_envelopes_work_on_mock_host() {
        let mut host = MockHost::default();
        host.kv.insert("counter".to_owned(), json!(7));
        host.entries.insert(
            ("post".to_owned(), 10),
            ContentEntry {
                id: 10,
                type_key: "post".to_owned(),
                slug: "hello-world".to_owned(),
                title: "Hello World".to_owned(),
                status: "published".to_owned(),
                version: 2,
                locale: "vi".to_owned(),
                values: Map::from_iter([("summary".to_owned(), json!("Short"))]),
                term_ids: vec![5],
            },
        );
        host.taxonomies.insert(
            "category".to_owned(),
            vec![TaxonomyTerm {
                id: 5,
                kind: "category".to_owned(),
                slug: "rust".to_owned(),
                name: "Rust".to_owned(),
                parent_id: None,
            }],
        );
        host.media.insert(
            "m_001".to_owned(),
            MediaAsset {
                id: "m_001".to_owned(),
                filename: "cover.webp".to_owned(),
                mime_type: "image/webp".to_owned(),
                url: "/media/m_001/file".to_owned(),
                width: Some(1200),
                height: Some(630),
                alt: Some("Cover".to_owned()),
            },
        );
        host.site_options
            .insert("brand".to_owned(), json!("OpenDash"));
        host.plugin_settings
            .insert("default_badge".to_owned(), json!("Hot"));
        host.plugin_secrets
            .insert("webhook_token".to_owned(), "secret-123".to_owned());
        host.http_handler = Some(Box::new(|method, url, headers, body| {
            assert_eq!(method, "POST");
            assert_eq!(url, "https://hooks.example.com/seo-ping");
            assert_eq!(
                headers.get("authorization").map(String::as_str),
                Some("Bearer secret-123")
            );
            assert!(body.unwrap_or("").contains("hello-world"));
            Ok(HttpResponse {
                status: 202,
                headers: BTreeMap::from([("x-ack".to_owned(), "yes".to_owned())]),
                body: "{\"queued\":true}".to_owned(),
            })
        }));

        let env = sample_envelope_v2("plugin/manual_run", json!({ "trigger": "manual" }));
        let outcome = run_in_mock_host(host, &env, || -> Result<(), SdkError> {
            let loaded = read_envelope()?;
            assert_eq!(loaded.envelope_version, 2);
            assert_eq!(loaded.locale.as_deref(), Some("vi"));

            // 1-4: KV ops
            assert_eq!(kv_get("counter")?, Some(json!(7)));
            assert_eq!(kv_get("missing")?, None);
            kv_set("alpha_one", &json!({ "ready": true }))?;
            kv_set("alpha_two", &json!(2))?;
            let listed = kv_list(Some("alpha_"), Some(10))?;
            assert_eq!(listed.len(), 2);
            assert!(kv_delete("alpha_one")?);
            assert!(!kv_delete("never_existed")?);

            // 5-9: Content ops
            let got = content_get("post", 10, Some("vi"))?.expect("post #10");
            assert_eq!(got.slug, "hello-world");
            assert_eq!(content_get("post", 999, None)?, None);

            let list = content_list("post", Some("vi"), Some(10), Some(0), Some(true))?;
            assert_eq!(list.total, 1);

            let _ = content_save(&ContentSaveInput {
                type_key: "post".to_owned(),
                entry_id: None,
                expected_version: None,
                locale: Some("en".to_owned()),
                title: Some("Created Entry".to_owned()),
                slug: Some("created-entry".to_owned()),
                status: Some("draft".to_owned()),
                values: Map::from_iter([("note".to_owned(), json!("ok"))]),
                term_ids: Some(vec![5]),
            })?;
            content_delete("post", 10, 2)?;

            // 10-14: Taxonomy, Media, Site Options, Plugin Settings, Secrets
            let terms = taxonomy_list("category", Some("vi"), Some(10))?;
            assert_eq!(terms[0].slug, "rust");

            let asset = media_get("m_001")?.expect("media m_001");
            assert_eq!(asset.url, "/media/m_001/file");
            assert_eq!(media_get("m_missing")?, None);

            let site_opts = site_options_get(Some("vi"))?;
            assert_eq!(site_opts.get("brand"), Some(&json!("OpenDash")));

            let p_settings = plugin_settings_get()?;
            assert_eq!(p_settings.get("default_badge"), Some(&json!("Hot")));

            let secret = plugin_secret_get("webhook_token")?.expect("secret");
            assert_eq!(secret, "secret-123");
            assert_eq!(plugin_secret_get("unknown_secret")?, None);

            // 15: HTTP & Log
            let http = http_request(&HttpRequestInput {
                method: "POST",
                url: "https://hooks.example.com/seo-ping",
                headers: BTreeMap::from([("authorization".to_owned(), format!("Bearer {secret}"))]),
                body: Some(&json!({ "slug": got.slug }).to_string()),
                timeout_ms: Some(1500),
            })?;
            assert_eq!(http.status, 202);
            assert_eq!(http.headers.get("x-ack").map(String::as_str), Some("yes"));

            log(LogLevel::Info, "sync completed")?;

            let mut ext = EntryViewOutput::new();
            ext.insert("badge".to_owned(), json!("Hot"));
            write_output(&ext)
        });

        assert_eq!(outcome.result, Ok(()));
        assert_eq!(outcome.oversized_readbacks, 0);
        assert_eq!(outcome.staged_content_saves.len(), 1);
        assert_eq!(
            outcome.staged_content_saves[0].locale.as_deref(),
            Some("en")
        );
        assert_eq!(
            outcome.staged_content_deletes,
            vec![("post".to_owned(), 10, 2)]
        );
        assert!(!outcome.entries.contains_key(&("post".to_owned(), 10)));
        assert!(
            outcome
                .entries
                .values()
                .any(|e| e.slug == "created-entry" && e.locale == "en")
        );
        assert_eq!(
            outcome.logs,
            vec![(LogLevel::Info, "sync completed".to_owned())]
        );
        let out_json: serde_json::Value =
            serde_json::from_slice(outcome.output_bytes.as_deref().unwrap()).unwrap();
        assert_eq!(out_json, json!({ "badge": "Hot" }));
    }

    #[test]
    fn oversized_response_triggers_minus_two_readback_without_repeating_host_call() {
        let mut host = MockHost::default().with_initial_resp_buf_cap(64);
        for i in 1..=8 {
            host.entries.insert(
                ("post".to_owned(), i),
                ContentEntry {
                    id: i,
                    type_key: "post".to_owned(),
                    slug: format!("oversized-post-{i}"),
                    title: format!("Oversized Post Title #{i} with padding for buffer overflow"),
                    status: "published".to_owned(),
                    version: 1,
                    locale: "vi".to_owned(),
                    values: Map::new(),
                    term_ids: vec![],
                },
            );
        }

        let env = sample_envelope_v2("plugin/manual_run", json!({}));
        let outcome = run_in_mock_host(host, &env, || {
            content_list("post", None, Some(20), Some(0), Some(true))
        });

        let list = outcome
            .result
            .expect("content_list must succeed via -2 readback");
        assert_eq!(list.total, 8);
        assert_eq!(list.items.len(), 8);
        assert_eq!(outcome.host_calls, 1, "host_call must only execute once");
        assert_eq!(outcome.oversized_readbacks, 1);
    }

    #[test]
    fn parses_legacy_hook_envelope_and_spec2_data_responses_and_filter_decisions() {
        let legacy_json = json!({
            "event": "content/before_save",
            "occurred_at_ms": 999,
            "correlation_id": "corr-legacy",
            "payload": { "type_key": "post", "slug": "bad-slug" }
        });
        let legacy_env: Envelope = serde_json::from_value(legacy_json).unwrap();
        assert_eq!(legacy_env.envelope_version, 1);
        assert_eq!(legacy_env.kind, "hook");
        assert_eq!(legacy_env.principal, "system");
        assert_eq!(legacy_env.dispatch_depth, 0);

        // Also test Spec 2 `data` wrapped kv_get + http_request parsing and typed outputs
        let mut host = MockHost::default();
        host.kv.insert("mode".to_owned(), json!("strict"));
        let outcome = run_in_mock_host(host, &legacy_env, || -> Result<(), SdkError> {
            assert_eq!(kv_get("mode")?, Some(json!("strict")));
            let veto = BeforeSaveDecision::Veto {
                code: "invalid_slug".to_owned(),
                message: "Bad slug".to_owned(),
                field_errors: BTreeMap::from([("slug".to_owned(), "reserved".to_owned())]),
            };
            write_output(&veto)
        });
        assert_eq!(outcome.result, Ok(()));
        let out: BeforeSaveDecision =
            serde_json::from_slice(outcome.output_bytes.as_deref().unwrap()).unwrap();
        assert!(matches!(out, BeforeSaveDecision::Veto { .. }));

        // Verify Spec 2 `data` envelope variants on HostResponse
        let data_kv: HostResponse =
            serde_json::from_value(json!({ "ok": true, "data": { "found": true, "value": "v2" } }))
                .unwrap();
        assert!(data_kv.ok && data_kv.data.is_some());

        let seo = SeoHeadOutput {
            title: Some("SEO".to_owned()),
            noindex: Some(false),
            json_ld: vec![json!({ "@context": "https://schema.org", "@type": "Article" })],
            ..Default::default()
        };
        let seo_json = serde_json::to_value(&seo).unwrap();
        assert_eq!(seo_json["title"], "SEO");

        let patch = ContentPrepareSavePatch {
            title: Some("Cleaned".to_owned()),
            values_patch: BTreeMap::from([("tag".to_owned(), json!("news"))]),
            ..Default::default()
        };
        assert_eq!(serde_json::to_value(&patch).unwrap()["title"], "Cleaned");

        let contact = ContactBeforeSubmitDecision::Reject {
            code: "spam".to_owned(),
            message: "Blocked".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&contact).unwrap()["decision"],
            "reject"
        );
    }

    #[test]
    fn mock_host_cas_conflicts_and_panic_guard_cleanup_work() {
        let mut host = MockHost::default();
        host.entries.insert(
            ("post".to_owned(), 1),
            ContentEntry {
                id: 1,
                type_key: "post".to_owned(),
                slug: "s1".to_owned(),
                title: "T1".to_owned(),
                status: "draft".to_owned(),
                version: 3,
                locale: "vi".to_owned(),
                values: Map::new(),
                term_ids: vec![],
            },
        );
        let env = sample_envelope_v2("plugin/manual_run", json!({}));
        let out = run_in_mock_host(host, &env, || {
            let err = content_save(&ContentSaveInput {
                type_key: "post".to_owned(),
                entry_id: Some(1),
                expected_version: Some(1), // stale CAS
                title: Some("New".to_owned()),
                ..Default::default()
            })
            .unwrap_err();
            assert!(matches!(err, SdkError::HostError(_)));

            let del_err = content_delete("post", 1, 1).unwrap_err();
            assert!(matches!(del_err, SdkError::HostError(_)));
        });
        assert!(out.staged_content_saves.is_empty());
        assert!(out.staged_content_deletes.is_empty());

        // Panic inside run_in_mock_host must still clear ACTIVE_MOCK via RAII Drop
        let panic_res = std::panic::catch_unwind(|| {
            let env2 = sample_envelope_v2("plugin/manual_run", json!({}));
            let _ = run_in_mock_host(MockHost::default(), &env2, || -> () {
                panic!("simulated guest panic");
            });
        });
        assert!(panic_res.is_err());
        assert_eq!(read_envelope(), Err(SdkError::MockNotInitialized));
    }

    #[test]
    fn s02_trash_read_restore_and_purge_ops_work_on_mock_host() {
        let mut host = MockHost::default();
        host.entries.insert(
            ("post".to_owned(), 21),
            ContentEntry {
                id: 21,
                type_key: "post".to_owned(),
                slug: "draft-a".to_owned(),
                title: "Draft A".to_owned(),
                status: "draft".to_owned(),
                version: 1,
                locale: "vi".to_owned(),
                values: Map::new(),
                term_ids: vec![],
            },
        );
        host.trashed_entries.insert(
            ("post".to_owned(), 22),
            TrashEntrySummary {
                id: 22,
                type_key: "post".to_owned(),
                locale: "en".to_owned(),
                title: "Trashed B".to_owned(),
                slug: "trashed-b".to_owned(),
                publication_status: "published".to_owned(),
                version: 4,
                trashed_at: "2026-02-01T00:00:00Z".to_owned(),
                published_at: Some("2026-01-10T00:00:00Z".to_owned()),
            },
        );

        let env = sample_envelope_v2("plugin/lifecycle_action", json!({}));
        let out = run_in_mock_host(host, &env, || -> Result<(), SdkError> {
            // Move entry 21 to trash via content_delete
            content_delete("post", 21, 1)?;
            assert_eq!(content_get("post", 21, None)?, None);

            // Inspect trash metadata
            let trashed_21 = content_trash_get("post", 21, Some("vi"))?.expect("trashed 21");
            assert_eq!(trashed_21.version, 2);
            assert_eq!(trashed_21.slug, "draft-a");
            assert_eq!(content_trash_get("post", 21, Some("en"))?, None);

            let trash_list = content_trash_list("post", None, Some(10), Some(0))?;
            assert_eq!(trash_list.total, 2);
            assert_eq!(trash_list.limit, 10);
            assert_eq!(trash_list.offset, 0);
            // Sorted newest trashed_at DESC
            assert_eq!(trash_list.items[0].id, 22);
            assert_eq!(trash_list.items[1].id, 21);

            // Stale version fails on restore/purge
            assert!(matches!(
                content_restore("post", 21, 1),
                Err(SdkError::HostError(_))
            ));
            assert!(matches!(
                content_purge("post", 22, 3),
                Err(SdkError::HostError(_))
            ));

            // Valid restore & purge succeed
            content_restore("post", 21, 2)?;
            content_purge("post", 22, 4)?;
            Ok(())
        });

        assert_eq!(out.result, Ok(()));
        assert_eq!(out.staged_content_deletes, vec![("post".to_owned(), 21, 1)]);
        assert_eq!(
            out.staged_content_restores,
            vec![("post".to_owned(), 21, 2)]
        );
        assert_eq!(out.staged_content_purges, vec![("post".to_owned(), 22, 4)]);
        assert!(out.entries.contains_key(&("post".to_owned(), 21)));
        assert_eq!(out.entries[&("post".to_owned(), 21)].version, 3);
        assert!(!out.trashed_entries.contains_key(&("post".to_owned(), 22)));
    }
}
