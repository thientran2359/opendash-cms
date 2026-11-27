use super::*;

pub(super) use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub(super) use opendash_core::{
    content::{FieldRole, PublicExposure},
    field_schema::{FieldType, default_value_for_field_spec},
    hooks::{ExecutionPrincipal, ExtensionOwnerKind, ExtensionRuntimeKind, HookExecutionMode},
    plugin::plugin_can_read_field_for_owner,
    registry::ProviderStatus,
};

pub(super) use serde_json::{Value, json};

pub(super) use sqlx::SqlitePool;

pub(super) use tokio::sync::{Semaphore, watch};

pub(super) use wasmi::{
    AsContextMut, Caller, Config, Engine, Error as WasmiError, Linker, StoreLimits,
};

pub(super) use crate::operations_worker_process::{
    GuardedChild, PLUGIN_BOOTSTRAP_PROOF, PLUGIN_WORKER_MEMORY_LIMIT_BYTES, WorkerSpawnConfig,
    spawn_worker_process,
};

pub(super) use crate::storage::{
    entries::{self, OwnerFence},
    media, plugin as plugin_storage, plugin_kv, settings, taxonomy,
};

pub(super) use crate::plugin::host_ops::{
    self, HostContext, HostOperationResponse, HostReadRequest, HostToWorkerFrame,
    MAX_IPC_FRAME_BYTES, MAX_LOG_MESSAGE_CHARS, MAX_STAGED_LOGS, WorkerToHostFrame,
};

pub(super) use crate::plugin::manifest::PluginManifest;

pub(super) use crate::plugin::worker::{IPC_STREAM_MAGIC, read_ipc_frame, write_ipc_frame};

pub(crate) struct WasmStoreData {
    pub(crate) host: HostContext,
    pub(crate) limits: StoreLimits,
}

pub const INVOCATION_FUEL: u64 = 20_000_000;

pub const MAX_LINEAR_MEMORY_BYTES: usize = 16 * 1024 * 1024; // 16 MB

pub const INVOCATION_DEADLINE: Duration = Duration::from_secs(5);

pub const MAX_CONCURRENT_INVOCATIONS: usize = 4;

pub const MAX_BACKGROUND_PUBLIC_INVOCATIONS: usize = 2;

#[allow(dead_code)]
pub const MAX_TOTAL_CONCURRENT_WORKERS: usize =
    MAX_CONCURRENT_INVOCATIONS + MAX_BACKGROUND_PUBLIC_INVOCATIONS;

#[derive(Debug)]
pub enum PluginError {
    Disabled,
    NotFound,
    ModuleCompilation(String),
    OutOfFuel,
    Timeout,
    BadPointer,
    GuestFailed(i32),
    Trap(String),
    InvalidOutput(String),
    ConcurrencyLimit,
    Internal(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Plugin chưa được bật hoặc đã bị vô hiệu hóa."),
            Self::NotFound => write!(f, "Plugin không tồn tại."),
            Self::ModuleCompilation(msg) => write!(f, "Module WebAssembly không hợp lệ: {msg}"),
            Self::OutOfFuel => write!(
                f,
                "Plugin hết fuel (vượt quá giới hạn CPU {INVOCATION_FUEL} đơn vị)."
            ),
            Self::Timeout => write!(f, "Quá thời gian thực thi (deadline 5 giây)."),
            Self::BadPointer => {
                write!(
                    f,
                    "Lỗi con trỏ bộ nhớ (Bad Pointer hoặc ngoài biên memory)."
                )
            }
            Self::GuestFailed(code) => write!(f, "Plugin kết thúc với mã lỗi {code}."),
            Self::Trap(msg) => write!(f, "Guest trap: {msg}"),
            Self::InvalidOutput(msg) => write!(f, "Dữ liệu đầu ra từ plugin không hợp lệ: {msg}"),
            Self::ConcurrencyLimit => write!(f, "Không thể lấy permit điều phối plugin."),
            Self::Internal(msg) => write!(f, "Lỗi máy chủ nội bộ: {msg}"),
        }
    }
}

impl std::error::Error for PluginError {}

pub const MAX_PUBLIC_RENDER_INVOCATIONS: usize = 4;

#[derive(Clone)]
pub struct PluginRuntime {
    pub(super) engine: Engine,
    pub(super) admin_semaphore: Arc<Semaphore>,
    pub(super) background_public_semaphore: Arc<Semaphore>,
    pub(super) public_render_semaphore: Arc<Semaphore>,
    pub(super) active_workers: Arc<AtomicUsize>,
    pub(super) secrets_root: PathBuf,
}

pub(super) fn scan_ipc_magic<R: std::io::Read>(reader: &mut R) -> Result<(), PluginError> {
    let mut matched = 0usize;
    let mut byte = [0u8; 1];
    for _ in 0..8192 {
        if reader.read_exact(&mut byte).is_err() {
            return Err(PluginError::Trap(
                "Worker process kết thúc trước khi gửi IPC magic".into(),
            ));
        }
        if byte[0] == IPC_STREAM_MAGIC[matched] {
            matched += 1;
            if matched == IPC_STREAM_MAGIC.len() {
                return Ok(());
            }
        } else {
            matched = usize::from(byte[0] == IPC_STREAM_MAGIC[0]);
        }
    }
    Err(PluginError::Trap(
        "Không tìm thấy IPC stream magic hợp lệ từ worker".into(),
    ))
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_host_read_request_for_owner(
    db: &SqlitePool,
    owner_kind: ExtensionOwnerKind,
    plugin_id: &str,
    manifest: &PluginManifest,
    execution_mode: HookExecutionMode,
    principal: ExecutionPrincipal,
    action_caps: Option<&[String]>,
    secrets_root: Option<&Path>,
    req: HostReadRequest,
) -> HostOperationResponse {
    if let Err(msg) =
        host_ops::check_read_policy(manifest, execution_mode, principal, action_caps, &req)
    {
        return HostOperationResponse::err(msg);
    }

    let actor_role = match principal {
        ExecutionPrincipal::Admin | ExecutionPrincipal::System => FieldRole::Admin,
        ExecutionPrincipal::Editor | ExecutionPrincipal::Public => FieldRole::Editor,
    };
    let is_public_scope = principal == ExecutionPrincipal::Public
        || execution_mode == HookExecutionMode::FilterProjection;

    match req {
        HostReadRequest::KvGet { key } => {
            if key.is_empty() || key.len() > host_ops::MAX_KV_KEY_BYTES {
                return HostOperationResponse::err("Key không hợp lệ hoặc quá dài.");
            }
            match plugin_kv::get_for_owner(db, owner_kind, plugin_id, &key).await {
                Ok(Some(val_str)) => match serde_json::from_str::<Value>(&val_str) {
                    Ok(v) => HostOperationResponse {
                        ok: true,
                        error: None,
                        data: None,
                        found: Some(true),
                        value: Some(v),
                        status: None,
                        headers: None,
                        body: None,
                    },
                    Err(_) => {
                        HostOperationResponse::err("Dữ liệu trong database không phải JSON hợp lệ.")
                    }
                },
                Ok(None) => HostOperationResponse {
                    ok: true,
                    error: None,
                    data: None,
                    found: Some(false),
                    value: None,
                    status: None,
                    headers: None,
                    body: None,
                },
                Err(e) => HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
            }
        }
        HostReadRequest::KvList { prefix, limit } => {
            let prefix_str = prefix.unwrap_or_default();
            let clamped = limit.unwrap_or(50).clamp(1, 200) as i64;
            match plugin_kv::list_for_owner(db, owner_kind, plugin_id, &prefix_str, clamped).await {
                Ok(rows) => {
                    let mut items = Vec::with_capacity(rows.len());
                    for (k, v_json) in rows {
                        let v = serde_json::from_str::<Value>(&v_json).unwrap_or(Value::Null);
                        items.push(json!({ "key": k, "value": v }));
                    }
                    HostOperationResponse::ok_data(json!({ "items": items }))
                }
                Err(e) => HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
            }
        }
        HostReadRequest::ContentGet {
            type_key,
            entry_id,
            locale,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            let effective_opt =
                match crate::storage::content::load_effective_content_schema(db, &type_key).await {
                    Ok(opt) => opt,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
            let Some(effective) = effective_opt else {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            };
            if is_public_scope
                && (effective.base.status != ProviderStatus::Active
                    || !effective.base.descriptor.settings.public)
            {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            }
            let entry_opt =
                match plugin_storage::get_entry_for_host(db, &type_key, entry_id, is_public_scope)
                    .await
                {
                    Ok(e) => e,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
            let Some(entry) = entry_opt else {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            };
            if is_public_scope && entry.publication_status != "published" {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            }
            if let Some(req_loc) = locale {
                let normalized = match settings::resolve_requested_locale(db, Some(&req_loc)).await
                {
                    Ok(l) => l,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
                if entry.locale != normalized {
                    return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
                }
            }
            let view = project_entry_for_host(
                owner_kind,
                manifest,
                action_caps,
                actor_role,
                is_public_scope,
                &type_key,
                &effective,
                entry,
            );
            HostOperationResponse::ok_data(json!({ "entry": view }))
        }
        HostReadRequest::ContentList {
            type_key,
            locale,
            limit,
            offset,
            published_only,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            let effective_opt =
                match crate::storage::content::load_effective_content_schema(db, &type_key).await {
                    Ok(opt) => opt,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
            let Some(effective) = effective_opt else {
                return HostOperationResponse::ok_data(json!({ "items": [], "total": 0 }));
            };
            if is_public_scope
                && (effective.base.status != ProviderStatus::Active
                    || !effective.base.descriptor.settings.public)
            {
                return HostOperationResponse::ok_data(json!({ "items": [], "total": 0 }));
            }
            let resolved_locale = match locale {
                Some(raw) => match settings::resolve_requested_locale(db, Some(&raw)).await {
                    Ok(l) => Some(l),
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                },
                None => None,
            };
            let clamped_limit = limit.unwrap_or(20).clamp(1, 50) as i64;
            let clamped_offset = offset.unwrap_or(0).min(10_000) as i64;
            let eff_published = is_public_scope || published_only.unwrap_or(false);
            let filters = entries::EntryFilters {
                category_id: None,
                tag_id: None,
                taxonomy_key: None,
                term_id: None,
                include_descendants: false,
                locale: resolved_locale.clone(),
            };
            let (rows, total) = match plugin_storage::list_entries_for_host(
                db,
                &type_key,
                clamped_limit,
                clamped_offset,
                filters,
                eff_published,
                is_public_scope,
            )
            .await
            {
                Ok(result) => result,
                Err(e) => return HostOperationResponse::err(e.1.to_string()),
            };
            let items: Vec<Value> = rows
                .into_iter()
                .map(|entry| {
                    project_entry_for_host(
                        owner_kind,
                        manifest,
                        action_caps,
                        actor_role,
                        is_public_scope,
                        &type_key,
                        &effective,
                        entry,
                    )
                })
                .collect();
            HostOperationResponse::ok_data(json!({ "items": items, "total": total }))
        }
        HostReadRequest::ContentTrashGet {
            type_key,
            entry_id,
            locale,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            match crate::storage::entry_lifecycle::get_trashed_entry_metadata_for_host(
                db,
                &type_key,
                entry_id,
                locale.as_deref(),
            )
            .await
            {
                Ok(entry) => HostOperationResponse::ok_data(json!({ "entry": entry })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::ContentTrashList {
            type_key,
            locale,
            limit,
            offset,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            let clamped_limit = limit.unwrap_or(20).clamp(1, 50) as i64;
            let clamped_offset = offset.unwrap_or(0).min(10_000) as i64;
            match crate::storage::entry_lifecycle::list_trashed_entries_metadata_for_host(
                db,
                &type_key,
                locale.as_deref(),
                clamped_limit,
                clamped_offset,
            )
            .await
            {
                Ok((items, total)) => HostOperationResponse::ok_data(json!({
                    "items": items,
                    "total": total,
                    "limit": clamped_limit,
                    "offset": clamped_offset,
                })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::TaxonomyList {
            kind,
            locale,
            limit,
        } => {
            let def_opt = match taxonomy::get_definition(db, &kind).await {
                Ok(d) => d,
                Err(e) => return HostOperationResponse::err(e.1.to_string()),
            };
            let Some(def) = def_opt else {
                return HostOperationResponse::ok_data(json!({ "items": [] }));
            };
            if def.provider_status != ProviderStatus::Active {
                return HostOperationResponse::ok_data(json!({ "items": [] }));
            }
            if !def.public && (is_public_scope || principal != ExecutionPrincipal::Admin) {
                return HostOperationResponse::err("permission_denied: taxonomy_private");
            }
            let clamped = limit.unwrap_or(50).clamp(1, 100) as i64;
            let page = match taxonomy::list_terms_for_host(
                db,
                Some(&kind),
                locale.as_deref(),
                clamped,
                0,
                is_public_scope,
            )
            .await
            {
                Ok(p) => p,
                Err(e) => return HostOperationResponse::err(e.1.to_string()),
            };
            let items: Vec<Value> = page
                .items
                .into_iter()
                .map(|t| {
                    json!({
                        "id": t.id,
                        "kind": t.kind,
                        "slug": t.slug,
                        "name": t.name,
                        "parent_id": t.parent_id,
                    })
                })
                .collect();
            HostOperationResponse::ok_data(json!({ "items": items }))
        }
        HostReadRequest::MediaGet { media_id } => {
            if !crate::storage::media::valid_media_id(&media_id) {
                return HostOperationResponse::ok_data(json!({ "media": Value::Null }));
            }
            if is_public_scope {
                match crate::storage::public_content::media_is_referenced(db, &media_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        return HostOperationResponse::ok_data(json!({ "media": Value::Null }));
                    }
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                }
            }
            match media::find(db, &media_id).await {
                Ok(Some(m)) => {
                    let url = if is_public_scope {
                        format!("/media/{}/file", m.id)
                    } else {
                        format!("/api/admin/media/{}/file", m.id)
                    };
                    HostOperationResponse::ok_data(json!({
                        "media": {
                            "id": m.id,
                            "filename": m.filename,
                            "mime_type": m.mime_type,
                            "url": url,
                            "width": Some(m.width),
                            "height": Some(m.height),
                            "alt": Value::Null,
                        }
                    }))
                }
                Ok(None) => HostOperationResponse::ok_data(json!({ "media": Value::Null })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::SiteOptionsGet { locale } => {
            match settings::get_active_public_theme_options_for_locale(db, locale.as_deref()).await
            {
                Ok((_, map)) => HostOperationResponse::ok_data(json!({ "options": map })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::PluginSettingsGet => {
            let raw_map =
                match plugin_storage::read_raw_owner_option_map(db, owner_kind, plugin_id).await {
                    Ok(m) => m,
                    Err(e) => return HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
                };
            let mut settings_map = serde_json::Map::new();
            for field in &manifest.settings_schema {
                if field.is_ui_only() {
                    continue;
                }
                if field.field_type == FieldType::Secret {
                    let configured = raw_map
                        .get(&field.key)
                        .and_then(settings::is_secret_sentinel)
                        .is_some();
                    settings_map.insert(field.key.clone(), json!({ "configured": configured }));
                } else {
                    let val = raw_map
                        .get(&field.key)
                        .filter(|val| {
                            !val.as_object()
                                .is_some_and(|obj| obj.contains_key("secret_ref"))
                        })
                        .cloned()
                        .unwrap_or_else(|| default_value_for_field_spec(field));
                    settings_map.insert(field.key.clone(), val);
                }
            }
            HostOperationResponse::ok_data(json!({ "settings": settings_map }))
        }
        HostReadRequest::PluginSecretGet { key } => {
            if !manifest
                .settings_schema
                .iter()
                .any(|f| f.key == key && f.field_type == FieldType::Secret)
            {
                return HostOperationResponse::err(
                    "Khóa secret không tồn tại trong settings_schema.",
                );
            }
            let raw_map =
                match plugin_storage::read_raw_owner_option_map(db, owner_kind, plugin_id).await {
                    Ok(m) => m,
                    Err(e) => return HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
                };
            let Some(secret_ref) = raw_map.get(&key).and_then(settings::is_secret_sentinel) else {
                return HostOperationResponse::ok_data(json!({ "value": Value::Null }));
            };
            let Some(sec_root) = secrets_root else {
                return HostOperationResponse::err("SecretStore không khả dụng.");
            };
            let store = crate::secrets::SecretStore::for_plugin_dir(
                sec_root.join(owner_kind.secrets_subdir()).join(plugin_id),
            );
            match store.read_plugin_secret(secret_ref) {
                Ok(plaintext) => HostOperationResponse::ok_data(json!({ "value": plaintext })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
    }
}

pub(crate) fn register_host_imports(linker: &mut Linker<WasmStoreData>) -> Result<(), WasmiError> {
    // 1. host_get_input_len() -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_get_input_len",
        |mut caller: Caller<'_, WasmStoreData>| -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            Ok(caller.data().host.input_bytes.len() as i32)
        },
    )?;

    // 2. host_read_input(dest_ptr: i32, max_len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_read_input",
        |mut caller: Caller<'_, WasmStoreData>,
         dest_ptr: i32,
         max_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if dest_ptr < 0 || max_len < 0 {
                return Ok(-1);
            }
            let dest = dest_ptr as usize;
            let max = max_len as usize;
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };

            let input = caller.data().host.input_bytes.clone();
            let to_copy = std::cmp::min(max, input.len());
            let Some(end) = dest.checked_add(to_copy) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }

            if memory.write(&mut caller, dest, &input[..to_copy]).is_err() {
                return Ok(-1);
            }
            Ok(to_copy as i32)
        },
    )?;

    // 3. host_write_output(src_ptr: i32, len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_write_output",
        |mut caller: Caller<'_, WasmStoreData>,
         src_ptr: i32,
         len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if src_ptr < 0 || len < 0 {
                return Ok(-1);
            }
            let src = src_ptr as usize;
            let length = len as usize;
            if length > 1024 * 1024 {
                return Ok(-2);
            }
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let Some(end) = src.checked_add(length) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }

            let mut buf = vec![0u8; length];
            if memory.read(&caller, src, &mut buf).is_err() {
                return Ok(-1);
            }

            let Ok(text) = std::str::from_utf8(&buf) else {
                return Ok(-3);
            };
            if serde_json::from_str::<Value>(text).is_err() {
                return Ok(-4);
            }

            caller.data_mut().host.output_bytes = Some(buf);
            Ok(0)
        },
    )?;

    // 4. host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_call",
        |mut caller: Caller<'_, WasmStoreData>,
         op_ptr: i32,
         op_len: i32,
         resp_ptr: i32,
         max_resp_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if op_ptr < 0 || op_len < 0 || resp_ptr < 0 || max_resp_len < 0 {
                return Ok(-1);
            }
            let op_src = op_ptr as usize;
            let op_length = op_len as usize;
            let resp_dest = resp_ptr as usize;
            let resp_max = max_resp_len as usize;

            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let Some(op_end) = op_src.checked_add(op_length) else {
                return Ok(-1);
            };
            if op_end > memory.data_size(&caller) {
                return Ok(-1);
            }

            let mut op_buf = vec![0u8; op_length];
            if memory.read(&caller, op_src, &mut op_buf).is_err() {
                return Ok(-1);
            }

            let Ok(op_str) = std::str::from_utf8(&op_buf) else {
                return Ok(-1);
            };

            caller.data_mut().host.last_resp_bytes = None;
            let resp = host_ops::handle_host_call(&mut caller.data_mut().host, op_str);

            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }

            let resp_bytes = serde_json::to_vec(&resp)
                .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"Internal JSON error\"}".to_vec());

            if resp_bytes.len() > MAX_IPC_FRAME_BYTES {
                return Ok(-1);
            }
            if resp_bytes.len() > resp_max {
                caller.data_mut().host.last_resp_bytes = Some(resp_bytes);
                return Ok(-2);
            }

            let Some(resp_end) = resp_dest.checked_add(resp_bytes.len()) else {
                return Ok(-1);
            };
            if resp_end > memory.data_size(&caller) {
                return Ok(-1);
            }

            if memory.write(&mut caller, resp_dest, &resp_bytes).is_err() {
                return Ok(-1);
            }
            Ok(resp_bytes.len() as i32)
        },
    )?;

    // 5. host_last_resp_len() -> i32 (ABI v1.1)
    linker.func_wrap(
        "opendash_v1",
        "host_last_resp_len",
        |mut caller: Caller<'_, WasmStoreData>| -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            Ok(caller
                .data()
                .host
                .last_resp_bytes
                .as_ref()
                .map(|b| b.len() as i32)
                .unwrap_or(0))
        },
    )?;

    // 6. host_read_last_resp(dest_ptr: i32, max_len: i32) -> i32 (ABI v1.1)
    linker.func_wrap(
        "opendash_v1",
        "host_read_last_resp",
        |mut caller: Caller<'_, WasmStoreData>,
         dest_ptr: i32,
         max_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().host.is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if dest_ptr < 0 || max_len < 0 {
                return Ok(-1);
            }
            let Some(bytes) = caller.data().host.last_resp_bytes.clone() else {
                return Ok(-1);
            };
            let dest = dest_ptr as usize;
            let max = max_len as usize;
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let to_copy = std::cmp::min(max, bytes.len());
            let Some(end) = dest.checked_add(to_copy) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }
            if memory.write(&mut caller, dest, &bytes[..to_copy]).is_err() {
                return Ok(-1);
            }
            if to_copy == bytes.len() {
                caller.data_mut().host.last_resp_bytes = None;
            }
            Ok(to_copy as i32)
        },
    )?;

    linker.func_wrap(
        "env",
        "abort",
        |mut caller: Caller<'_, WasmStoreData>,
         _msg: i32,
         _file: i32,
         _line: i32,
         _col: i32|
         -> Result<(), wasmi::Error> {
            let _ = caller.as_context_mut().set_fuel(0);
            Err(wasmi::Error::new("Guest aborted via env.abort"))
        },
    )?;

    Ok(())
}

#[cfg(test)]
pub(crate) static TEST_IPC_KV_GET_WAITING: AtomicBool = AtomicBool::new(false);

impl PluginRuntime {
    pub fn new() -> Self {
        let secrets_root = crate::paths::SitePaths::from_env()
            .map(|p| p.secrets_dir())
            .unwrap_or_else(|_| std::env::temp_dir().join("opendash_secrets_default"));
        Self::with_secrets_root(secrets_root)
    }

    pub fn with_secrets_root(secrets_root: PathBuf) -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        Self {
            engine,
            admin_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_INVOCATIONS)),
            background_public_semaphore: Arc::new(Semaphore::new(
                MAX_BACKGROUND_PUBLIC_INVOCATIONS,
            )),
            public_render_semaphore: Arc::new(Semaphore::new(MAX_PUBLIC_RENDER_INVOCATIONS)),
            active_workers: Arc::new(AtomicUsize::new(0)),
            secrets_root,
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn active_workers(&self) -> usize {
        self.active_workers.load(Ordering::SeqCst)
    }

    pub fn semaphore(&self) -> &Arc<Semaphore> {
        &self.admin_semaphore
    }

    pub fn background_public_semaphore(&self) -> &Arc<Semaphore> {
        &self.background_public_semaphore
    }

    pub fn public_render_semaphore(&self) -> &Arc<Semaphore> {
        &self.public_render_semaphore
    }

    pub async fn run_plugin(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
    ) -> Result<Value, PluginError> {
        self.run_plugin_with_generation(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_with_generation(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
    ) -> Result<Value, PluginError> {
        self.run_plugin_with_mode(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            expected_generation,
            HookExecutionMode::Standard,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_with_mode(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
        execution_mode: HookExecutionMode,
        timeout_override: Option<Duration>,
    ) -> Result<Value, PluginError> {
        self.run_plugin_action(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            expected_generation,
            execution_mode,
            timeout_override,
            None,
            None,
        )
        .await
        .map(|res| res.output)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_action(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
        execution_mode: HookExecutionMode,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        self.run_plugin_action_with_principal(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            expected_generation,
            execution_mode,
            ExecutionPrincipal::Admin,
            timeout_override,
            action_capabilities,
            actor_fence,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_action_with_principal(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        let runtime_kind = manifest.runtime_kind();
        let owner_fence = expected_generation.map(|expected_generation| OwnerFence::Plugin {
            plugin_id: manifest.id.clone(),
            expected_generation,
        });
        self.run_owner_action_with_principal(
            ExtensionOwnerKind::Plugin,
            runtime_kind,
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            owner_fence,
            execution_mode,
            principal,
            timeout_override,
            action_capabilities,
            actor_fence,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_owner_action_with_principal(
        &self,
        owner_kind: ExtensionOwnerKind,
        runtime_kind: ExtensionRuntimeKind,
        code_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        owner_fence: Option<OwnerFence>,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        let cancellation_token = Arc::new(AtomicBool::new(false));

        // Respect global 10-worker ceiling: 4 admin + 2 background/public + 4 public_render
        let permit = if matches!(
            execution_mode,
            HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
        ) {
            match tokio::time::timeout(
                PUBLIC_RENDER_SEMAPHORE_WAIT,
                self.public_render_semaphore.clone().acquire_owned(),
            )
            .await
            {
                Ok(Ok(p)) => p,
                _ => return Err(PluginError::ConcurrencyLimit),
            }
        } else {
            let target_sem = match execution_mode {
                HookExecutionMode::AsyncPostCommit
                | HookExecutionMode::FilterContactBeforeSubmit => &self.background_public_semaphore,
                HookExecutionMode::RoutePost if principal == ExecutionPrincipal::Public => {
                    &self.background_public_semaphore
                }
                _ => &self.admin_semaphore,
            };
            match target_sem.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => return Err(PluginError::ConcurrencyLimit),
            }
        };

        let start = Instant::now();
        let effective_budget = timeout_override
            .unwrap_or(INVOCATION_DEADLINE)
            .min(INVOCATION_DEADLINE);
        let deadline = start + effective_budget;
        let remaining = effective_budget;

        let current_exe = std::env::current_exe().map_err(|e| {
            PluginError::Internal(format!("Không thể lấy đường dẫn executable hiện tại: {e}"))
        })?;

        let worker_args: &[&str] = if cfg!(test) {
            &[
                "plugin::tests::suite_01::tests::test_worker_entry",
                "--exact",
                "--nocapture",
            ]
        } else {
            &["--plugin-worker"]
        };

        let mut child = spawn_worker_process(WorkerSpawnConfig {
            exe: &current_exe,
            args: worker_args,
            env_vars: &[("OPENDASH_WORKER_CHILD", "1")],
            memory_limit_bytes: PLUGIN_WORKER_MEMORY_LIMIT_BYTES,
            macos_bootstrap_proof: PLUGIN_BOOTSTRAP_PROOF,
            inherit_stderr: true,
        })
        .map_err(|e| PluginError::Internal(format!("Không khởi động được worker process: {e}")))?;

        let mut child_stdin = child.take_stdin().ok_or_else(|| {
            PluginError::Internal("Không mở được stdin của worker process".into())
        })?;
        let child_stdout = child.take_stdout().ok_or_else(|| {
            PluginError::Internal("Không mở được stdout của worker process".into())
        })?;

        let child_ref = Arc::new(std::sync::Mutex::new(Some(child)));
        let worker_thread_ref = Arc::new(std::sync::Mutex::new(None));
        self.active_workers.fetch_add(1, Ordering::SeqCst);

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut guard = ProcessGuard {
            child: Arc::clone(&child_ref),
            worker_thread: Arc::clone(&worker_thread_ref),
            active_workers: Arc::clone(&self.active_workers),
            cancellation_token: Arc::clone(&cancellation_token),
            cancel_tx,
            permit: Some(permit),
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tokio_handle = tokio::runtime::Handle::current();
        let worker_cancel = Arc::clone(&cancellation_token);
        let manifest_clone = manifest.clone();
        let db_pool_clone = db_pool.clone();
        let secrets_root_clone = self.secrets_root.clone();
        let child_ref_worker = Arc::clone(&child_ref);
        let action_caps_for_host = action_capabilities.clone();
        let correlation_for_log = correlation_id.clone();

        let thread_handle = std::thread::Builder::new()
            .name(format!("plugin-worker-{}", manifest.id))
            .spawn(move || {
                use std::io::{BufReader, Write};

                let cancel_watch = cancel_rx;
                let run = move || -> Result<super::host_ops::WorkerExecutionResult, PluginError> {
                    let code_len = code_bytes.len() as u32;
                    child_stdin
                        .write_all(&code_len.to_le_bytes())
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi code len: {e}")))?;
                    child_stdin
                        .write_all(&code_bytes)
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi code bytes: {e}")))?;

                    let req = crate::plugin::worker::WorkerInvocationRequest {
                        manifest: manifest_clone.clone(),
                        input,
                        allow_loopback,
                        correlation_id,
                        deadline_ms: remaining.as_millis() as u64,
                        execution_mode,
                        principal,
                        action_capabilities: action_caps_for_host.clone(),
                        runtime_kind,
                        owner_kind,
                    };
                    write_ipc_frame(&mut child_stdin, &req, MAX_IPC_FRAME_BYTES)
                        .map_err(PluginError::Internal)?;

                    let mut reader = BufReader::new(child_stdout);
                    scan_ipc_magic(&mut reader)?;

                    loop {
                        let frame: WorkerToHostFrame =
                            read_ipc_frame(&mut reader, MAX_IPC_FRAME_BYTES).map_err(|_| {
                                PluginError::Trap("Worker process kết thúc bất ngờ".into())
                            })?;
                        match frame {
                            WorkerToHostFrame::ReadRequest(read_req) => {
                                if worker_cancel.load(Ordering::Relaxed)
                                    || Instant::now() >= deadline
                                    || *cancel_watch.borrow()
                                {
                                    return Err(PluginError::Timeout);
                                }
                                let rem = deadline.saturating_duration_since(Instant::now());
                                if rem.is_zero() {
                                    return Err(PluginError::Timeout);
                                }

                                #[cfg(test)]
                                if matches!(read_req, HostReadRequest::KvGet { .. }) {
                                    TEST_IPC_KV_GET_WAITING.store(true, Ordering::SeqCst);
                                }

                                let db = db_pool_clone.clone();
                                let m = manifest_clone.clone();
                                let caps = action_caps_for_host.clone();
                                let sec_root = secrets_root_clone.clone();

                                let read_fut = async move {
                                    execute_host_read_request_for_owner(
                                        &db,
                                        owner_kind,
                                        &m.id,
                                        &m,
                                        execution_mode,
                                        principal,
                                        caps.as_deref(),
                                        Some(&sec_root),
                                        read_req,
                                    )
                                    .await
                                };

                                let mut watch_rx = cancel_watch.clone();
                                let resp = tokio_handle.block_on(async {
                                    tokio::select! {
                                        biased;
                                        _ = watch_rx.changed() => Err(PluginError::Timeout),
                                        res = read_fut => Ok(res),
                                        _ = tokio::time::sleep(rem) => Err(PluginError::Timeout),
                                    }
                                })?;

                                write_ipc_frame(
                                    &mut child_stdin,
                                    &HostToWorkerFrame::ReadResponse(resp),
                                    MAX_IPC_FRAME_BYTES,
                                )
                                .map_err(PluginError::Internal)?;
                            }
                            WorkerToHostFrame::Result(res) => {
                                if let Ok(mut lock) = child_ref_worker.lock()
                                    && let Some(ref mut c) = *lock
                                {
                                    let _ = c.wait();
                                }
                                if res.ok {
                                    return Ok(res);
                                }
                                let error_type = res.error_type.as_deref().unwrap_or("internal");
                                let err_msg =
                                    res.error.unwrap_or_else(|| "Lỗi thực thi plugin".into());
                                return Err(match error_type {
                                    "out_of_fuel" => PluginError::OutOfFuel,
                                    "timeout" => PluginError::Timeout,
                                    "memory_limit" => PluginError::Trap(err_msg),
                                    "invalid_output" => PluginError::InvalidOutput(err_msg),
                                    "guest_failed" => PluginError::GuestFailed(res.code),
                                    _ => PluginError::Trap(err_msg),
                                });
                            }
                        }
                    }
                };

                let res = run();
                let _ = tx.send(res);
            })
            .map_err(|e| PluginError::Internal(format!("Không thể tạo worker thread: {e}")))?;

        if let Ok(mut lock) = worker_thread_ref.lock() {
            *lock = Some(thread_handle);
        }

        let worker_res = match tokio::time::timeout(remaining, rx).await {
            Ok(Ok(res)) => match res {
                Ok(exec_res) => {
                    if let Ok(mut lock) = guard.child.lock() {
                        let _ = lock.take();
                    }
                    exec_res
                }
                Err(err) => {
                    guard.cleanup();
                    return Err(err);
                }
            },
            Ok(Err(_)) => {
                guard.cleanup();
                return Err(PluginError::Trap(
                    "Worker process hoặc thread kết thúc bất ngờ".into(),
                ));
            }
            Err(_) => {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }
        };

        let output_val = worker_res.output.unwrap_or(Value::Null);
        let staged_kv = worker_res.staged_kv;
        let staged_logs = worker_res.staged_logs;
        let staged_content = worker_res.staged_content_command;
        let staged_lifecycle = worker_res.staged_lifecycle_command;

        let scope_count = (!staged_kv.is_empty() as u8)
            + (staged_content.is_some() as u8)
            + (staged_lifecycle.is_some() as u8);
        if scope_count > 1 {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "ERR_MIXED_WRITE_SCOPE: Không cho phép trộn ghi/xóa KV và ghi/xóa nội dung.".into(),
            ));
        }

        let content_write_allowed = execution_mode == HookExecutionMode::Standard
            || (execution_mode == HookExecutionMode::RoutePost
                && principal != ExecutionPrincipal::Public);
        if (staged_content.is_some() || staged_lifecycle.is_some()) && !content_write_allowed {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Ghi/xóa nội dung bị cấm ngoài action mode.".into(),
            ));
        }
        if let Some(ref cmd) = staged_lifecycle
            && matches!(
                cmd.operation,
                host_ops::StagedLifecycleOperation::Restore
                    | host_ops::StagedLifecycleOperation::Purge
            )
            && action_capabilities.is_none()
        {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "content_restore và content_purge chỉ được phép qua action khai báo.".into(),
            ));
        }

        if !staged_kv.is_empty()
            && !matches!(
                execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::RoutePost
            )
        {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Ghi/xóa KV bị cấm trong hook/filter đồng bộ.".into(),
            ));
        }

        if !staged_logs.is_empty() && execution_mode == HookExecutionMode::FilterContactBeforeSubmit
        {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Ghi log bị cấm trong filter:contact/before_submit.".into(),
            ));
        }

        if !matches!(
            execution_mode,
            HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
        ) && (!staged_kv.is_empty() || !staged_logs.is_empty())
        {
            let had_kv_writes = !staged_kv.is_empty();
            if cancellation_token.load(Ordering::Relaxed) || Instant::now() >= deadline {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let remaining_commit = deadline.saturating_duration_since(Instant::now());
            if remaining_commit < Duration::from_millis(50) {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let pool_clone = db_pool.clone();
            let mut conn = match tokio::time::timeout(remaining_commit, pool_clone.acquire()).await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    guard.cleanup();
                    return Err(PluginError::Internal(format!(
                        "Không lấy được kết nối database: {e}"
                    )));
                }
                Err(_) => {
                    guard.cleanup();
                    return Err(PluginError::Timeout);
                }
            };

            let initial_busy_timeout: i64 =
                plugin_kv::busy_timeout(&mut conn).await.unwrap_or(5000);

            let mut commit_guard = StagedCommitGuard {
                conn: Some(conn),
                _initial_busy_timeout: initial_busy_timeout,
                committed: false,
            };
            let conn = commit_guard.conn.as_mut().unwrap();
            let plugin_id = manifest.id;

            let cur_rem = deadline.saturating_duration_since(Instant::now());
            if cur_rem.is_zero() || cancellation_token.load(Ordering::Relaxed) {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let begin_budget_ms = cur_rem.as_millis().max(1);
            if let Err(e) = plugin_kv::set_busy_timeout(conn, begin_budget_ms as u128).await {
                guard.cleanup();
                return Err(PluginError::Internal(format!("Lỗi set busy_timeout: {e}")));
            }

            if let Err(e) = plugin_kv::begin_immediate(conn).await {
                let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                guard.cleanup();
                let err_str = e.to_string();
                if err_str.contains("database is locked")
                    || err_str.contains("busy")
                    || Instant::now() >= deadline
                {
                    return Err(PluginError::Timeout);
                }
                return Err(PluginError::Internal(format!(
                    "Không mở được transaction: {e}"
                )));
            }

            if let Some(ref fence) = owner_fence {
                let fence_check = match fence {
                    OwnerFence::Plugin {
                        plugin_id: fence_pid,
                        expected_generation,
                    } => {
                        plugin_storage::is_plugin_generation_active(
                            conn,
                            fence_pid,
                            *expected_generation,
                        )
                        .await
                    }
                    OwnerFence::Theme {
                        revision_id,
                        selection_version,
                        ..
                    } => {
                        plugin_storage::verify_active_theme_fence_in_tx(
                            conn,
                            revision_id,
                            *selection_version,
                        )
                        .await
                    }
                };
                match fence_check {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Disabled);
                    }
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi kiểm tra fence gói mở rộng: {e}"
                        )));
                    }
                }
            }

            if let Some(ref fence) = actor_fence {
                match plugin_storage::is_actor_role_active(
                    conn,
                    fence.user_id,
                    &fence.required_role,
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(
                            "rbac_revoked: Quyền của tài khoản đã bị thu hồi.".into(),
                        ));
                    }
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi kiểm tra RBAC actor: {e}"
                        )));
                    }
                }
            }

            for (key, opt_val) in staged_kv {
                let cur_rem = deadline.saturating_duration_since(Instant::now());
                if cur_rem.is_zero() || cancellation_token.load(Ordering::Relaxed) {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    return Err(PluginError::Timeout);
                }
                let op_budget_ms = cur_rem.as_millis().max(1);
                let _ = plugin_kv::set_busy_timeout(conn, op_budget_ms as u128).await;

                let write_res = match opt_val {
                    Some(val) => {
                        let val_str = match serde_json::to_string(&val) {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = plugin_kv::rollback(conn).await;
                                let _ =
                                    plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128)
                                        .await;
                                guard.cleanup();
                                return Err(PluginError::Internal(format!(
                                    "Lỗi serialize staged KV: {e}"
                                )));
                            }
                        };
                        plugin_kv::set_for_owner(conn, owner_kind, &plugin_id, &key, &val_str).await
                    }
                    None => plugin_kv::delete_for_owner(conn, owner_kind, &plugin_id, &key).await,
                };

                if let Err(e) = write_res {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    let err_str = e.to_string();
                    if err_str.contains("database is locked")
                        || err_str.contains("busy")
                        || Instant::now() >= deadline
                    {
                        return Err(PluginError::Timeout);
                    }
                    return Err(PluginError::Internal(format!("Lỗi ghi database: {e}")));
                }
            }

            for entry in staged_logs.into_iter().take(MAX_STAGED_LOGS) {
                let level = match entry.level.as_str() {
                    "warn" => "warn",
                    "error" => "error",
                    _ => "info",
                };
                let msg = opendash_core::hooks::sanitize_plain_text(
                    &entry.message,
                    MAX_LOG_MESSAGE_CHARS,
                );
                if let Err(e) = plugin_storage::insert_owner_log_in_conn(
                    conn,
                    owner_kind,
                    &plugin_id,
                    &correlation_for_log,
                    level,
                    &msg,
                )
                .await
                {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    return Err(PluginError::Internal(format!("Lỗi ghi plugin_logs: {e}")));
                }
            }

            #[cfg(test)]
            {
                if TEST_STAGE_WRITE_PAUSE.load(Ordering::SeqCst) {
                    TEST_STAGE_WRITE_BARRIER_HIT.store(true, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            }

            // --- COMMIT HANDOVER POINT ---
            if Instant::now() >= deadline || cancellation_token.load(Ordering::Relaxed) {
                let _ = plugin_kv::rollback(conn).await;
                let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let cur_rem = deadline.saturating_duration_since(Instant::now());
            let commit_budget_ms = cur_rem.as_millis().max(1);
            let _ = plugin_kv::set_busy_timeout(conn, commit_budget_ms as u128).await;

            let commit_res = plugin_kv::commit(conn).await;
            let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;

            match commit_res {
                Ok(_) => {
                    commit_guard.committed = true;
                    if had_kv_writes {
                        crate::cache::SiteCacheEpoch::global().bump();
                    }
                }
                Err(e) => {
                    let _ = plugin_kv::rollback(conn).await;
                    guard.cleanup();
                    let err_str = e.to_string();
                    if err_str.contains("database is locked")
                        || err_str.contains("busy")
                        || Instant::now() >= deadline
                    {
                        return Err(PluginError::Timeout);
                    }
                    return Err(PluginError::Internal(format!(
                        "Lỗi commit transaction: {e}"
                    )));
                }
            }
        }

        guard.cleanup();
        Ok(ActionExecutionOutput {
            output: output_val,
            staged_content_command: staged_content,
            staged_lifecycle_command: staged_lifecycle,
        })
    }
}

#[cfg(test)]
pub(crate) static TEST_STAGE_WRITE_BARRIER_HIT: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(crate) static TEST_STAGE_WRITE_PAUSE: AtomicBool = AtomicBool::new(false);
