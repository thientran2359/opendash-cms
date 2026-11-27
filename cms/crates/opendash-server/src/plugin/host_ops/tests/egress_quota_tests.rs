use super::*;
use opendash_core::plugin::{NetworkScopeGrant, PluginPermissions};

fn test_ctx(manifest: PluginManifest, caps: Option<Vec<String>>) -> HostContext {
    let owner = RuntimeOwner::plugin(manifest.id.clone());
    HostContext {
        owner,
        plugin_id: manifest.id.clone(),
        manifest,
        input_bytes: vec![],
        output_bytes: None,
        last_resp_bytes: None,
        host_calls_count: 0,
        http_calls_count: 0,
        l10n_calls_count: 0,
        default_locale: None,
        db_pool: None,
        secrets_root: None,
        read_ipc: None,
        _correlation_id: "test".into(),
        allow_loopback: true,
        staged_kv: Default::default(),
        staged_kv_bytes: 0,
        staged_logs: vec![],
        cancellation_token: Arc::new(AtomicBool::new(false)),
        deadline: Instant::now() + Duration::from_secs(5),
        execution_mode: HookExecutionMode::Standard,
        principal: ExecutionPrincipal::Admin,
        staged_content_command: None,
        staged_lifecycle_command: None,
        action_capabilities: caps,
    }
}

#[test]
fn http_sub_quota_blocks_11th_call_per_invocation() {
    let manifest = PluginManifest {
        id: "quota-test".into(),
        version: "1.0.0".into(),
        api_version: 1,
        manifest_schema: 2,
        min_host_version: None,
        name: "Quota Test".into(),
        description: "Quota test".into(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            network: vec![NetworkScopeGrant {
                scheme: "http".into(),
                host: "127.0.0.1".into(),
                port: Some(1),
                methods: vec!["GET".into()],
                path_prefix: Some("/".into()),
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let mut ctx = test_ctx(manifest, None);
    ctx.http_calls_count = super::super::egress::MAX_HTTP_CALLS_PER_INVOCATION;
    let res = handle_http_request(
        &mut ctx,
        "GET",
        "http://127.0.0.1:1/",
        BTreeMap::new(),
        None,
        None,
    );
    assert!(!res.ok);
    assert!(
        res.error
            .as_deref()
            .unwrap_or("")
            .contains("quota_exceeded"),
        "expected quota_exceeded, got {:?}",
        res.error
    );
}

#[test]
fn content_save_stages_optional_locale_and_hook_envelope_extracts_locale() {
    let manifest = PluginManifest {
        id: "locale-test".into(),
        version: "1.0.0".into(),
        api_version: 1,
        manifest_schema: 2,
        min_host_version: None,
        name: "Locale Test".into(),
        description: "Locale test".into(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            content_write: vec!["post".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let mut ctx = test_ctx(manifest, Some(vec!["content_write".into()]));
    let req_json = r#"{"op":"content_save","type_key":"post","locale":"en","slug":"en-post","title":"EN Post"}"#;
    let resp = handle_host_call(&mut ctx, req_json);
    assert!(resp.ok, "unexpected error: {:?}", resp.error);
    let staged = ctx.staged_content_command.expect("staged command");
    assert_eq!(staged.locale.as_deref(), Some("en"));

    let env = opendash_core::hooks::HookEnvelope::new(
        opendash_core::hooks::HookEvent::ContentAfterSave,
        1_700_000_000_000,
        "locale-test",
        serde_json::json!({
            "id": 1,
            "locale": "en",
            "translation_group_id": "tg_123"
        }),
    );
    assert_eq!(env.locale.as_deref(), Some("en"));
    assert_eq!(env.translation_group_id.as_deref(), Some("tg_123"));
}

#[test]
fn host_policy_enforces_tombstones_mixed_scope_logs_and_api_p_block() {
    let manifest = PluginManifest {
        id: "policy-v3".into(),
        version: "1.0.0".into(),
        api_version: 1,
        manifest_schema: 3,
        min_host_version: Some("0.1.0".into()),
        name: "Policy v3".into(),
        description: "Policy v3".into(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_read".into(), "kv_write".into()],
            http: vec!["http://127.0.0.1:8080/".into()],
            content_read: vec!["post".into()],
            content_write: vec!["post".into()],
            content_delete: vec!["post".into()],
            ..Default::default()
        },
        ..Default::default()
    };

    // 1. /api/p/ loopback blocked even when allow_loopback = true
    let mut ctx = test_ctx(manifest.clone(), None);
    let api_p_resp = handle_host_call(
        &mut ctx,
        r#"{"op":"http_request","method":"GET","url":"http://127.0.0.1:8080/api/p/foo/bar"}"#,
    );
    assert!(!api_p_resp.ok);
    assert!(api_p_resp.error.unwrap_or_default().contains("/api/p/"));

    // 2. kv_set then kv_delete tombstone visible immediately in kv_get
    assert!(handle_host_call(&mut ctx, r#"{"op":"kv_set","key":"k1","value":{"x":1}}"#).ok);
    assert!(handle_host_call(&mut ctx, r#"{"op":"kv_delete","key":"k1"}"#).ok);
    let get_after_del = handle_host_call(&mut ctx, r#"{"op":"kv_get","key":"k1"}"#);
    assert!(get_after_del.ok);
    assert_eq!(get_after_del.found, Some(false));

    // 3. ERR_MIXED_WRITE_SCOPE forbids combining kv_* with content_delete
    let mix_del = handle_host_call(
        &mut ctx,
        r#"{"op":"content_delete","type_key":"post","entry_id":1,"expected_version":1}"#,
    );
    assert!(!mix_del.ok);
    assert!(
        mix_del
            .error
            .unwrap_or_default()
            .contains("ERR_MIXED_WRITE_SCOPE")
    );

    // 4. log cap 20 & control char stripping
    for i in 0..20 {
        let log_req = serde_json::json!({
            "op": "log",
            "level": "info",
            "message": format!("msg\n{i}")
        });
        assert!(handle_host_call(&mut ctx, &log_req.to_string()).ok);
    }
    assert!(
        !handle_host_call(
            &mut ctx,
            r#"{"op":"log","level":"info","message":"overflow"}"#
        )
        .ok
    );
    assert_eq!(ctx.staged_logs[0].message, "msg0");
}
