use std::{cell::RefCell, collections::BTreeMap};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    abi::INITIAL_RESP_BUF_CAP,
    types::{
        ContentEntry, ContentSaveInput, Envelope, HttpResponse, LogLevel, MediaAsset, SdkError,
        TaxonomyTerm,
    },
};

pub type MockHttpHandler = Box<
    dyn Fn(&str, &str, &BTreeMap<String, String>, Option<&str>) -> Result<HttpResponse, String>,
>;

pub struct MockHost {
    pub kv: BTreeMap<String, Value>,
    pub entries: BTreeMap<(String, i64), ContentEntry>,
    pub taxonomies: BTreeMap<String, Vec<TaxonomyTerm>>,
    pub media: BTreeMap<String, MediaAsset>,
    pub site_options: Map<String, Value>,
    pub plugin_settings: Map<String, Value>,
    pub plugin_secrets: BTreeMap<String, String>,
    pub logs: Vec<(LogLevel, String)>,
    pub http_handler: Option<MockHttpHandler>,
    pub initial_resp_buf_cap: usize,
}

impl Default for MockHost {
    fn default() -> Self {
        Self {
            kv: BTreeMap::new(),
            entries: BTreeMap::new(),
            taxonomies: BTreeMap::new(),
            media: BTreeMap::new(),
            site_options: Map::new(),
            plugin_settings: Map::new(),
            plugin_secrets: BTreeMap::new(),
            logs: Vec::new(),
            http_handler: None,
            initial_resp_buf_cap: INITIAL_RESP_BUF_CAP,
        }
    }
}

impl MockHost {
    pub fn with_initial_resp_buf_cap(mut self, cap: usize) -> Self {
        self.initial_resp_buf_cap = cap;
        self
    }

    fn dispatch_call(&mut self, req_json: &[u8]) -> Result<Vec<u8>, String> {
        let req: MockOpRequest = serde_json::from_slice(req_json).map_err(|e| e.to_string())?;
        let resp_val = match req {
            MockOpRequest::KvGet { key } => match self.kv.get(&key) {
                Some(v) => json!({ "ok": true, "found": true, "value": v }),
                None => json!({ "ok": true, "found": false, "value": Value::Null }),
            },
            MockOpRequest::KvSet { key, value } => {
                self.kv.insert(key, value);
                json!({ "ok": true })
            }
            MockOpRequest::KvDelete { key } => {
                let deleted = self.kv.remove(&key).is_some();
                json!({ "ok": true, "data": { "deleted": deleted } })
            }
            MockOpRequest::KvList { prefix, limit } => {
                let lim = limit.unwrap_or(50).clamp(1, 100);
                let items: Vec<Value> = self
                    .kv
                    .iter()
                    .filter(|(k, _)| prefix.as_deref().is_none_or(|p| k.starts_with(p)))
                    .take(lim)
                    .map(|(k, v)| json!({ "key": k, "value": v }))
                    .collect();
                json!({ "ok": true, "data": { "items": items } })
            }
            MockOpRequest::HttpRequest {
                method,
                url,
                headers,
                body,
                _timeout_ms: _,
            } => match &self.http_handler {
                Some(handler) => match handler(&method, &url, &headers, body.as_deref()) {
                    Ok(resp) => json!({
                        "ok": true,
                        "status": resp.status,
                        "headers": resp.headers,
                        "body": resp.body,
                    }),
                    Err(msg) => json!({ "ok": false, "error": msg }),
                },
                None => json!({
                    "ok": false,
                    "error": "http_handler not configured on MockHost"
                }),
            },
            MockOpRequest::ContentSave {
                type_key,
                entry_id,
                expected_version,
                locale,
                title,
                slug,
                status,
                values,
                term_ids,
            } => {
                if let Some(eid) = entry_id {
                    let Some(existing) = self.entries.get_mut(&(type_key.clone(), eid)) else {
                        return Ok(serde_json::to_vec(&json!({
                            "ok": false,
                            "error": format!("entry ({type_key}, {eid}) not found")
                        }))
                        .unwrap());
                    };
                    let Some(exp_ver) = expected_version else {
                        return Ok(serde_json::to_vec(&json!({
                            "ok": false,
                            "error": "expected_version required when updating existing entry"
                        }))
                        .unwrap());
                    };
                    if existing.version != exp_ver {
                        return Ok(serde_json::to_vec(&json!({
                            "ok": false,
                            "error": format!("version conflict: expected {exp_ver}, actual {}", existing.version)
                        }))
                        .unwrap());
                    }
                    if let Some(loc) = locale {
                        existing.locale = loc;
                    }
                    if let Some(t) = title {
                        existing.title = t;
                    }
                    if let Some(s) = slug {
                        existing.slug = s;
                    }
                    if let Some(st) = status {
                        existing.status = st;
                    }
                    for (k, v) in values {
                        existing.values.insert(k, v);
                    }
                    if let Some(tids) = term_ids {
                        existing.term_ids = tids;
                    }
                    existing.version += 1;
                } else {
                    let next_id = self
                        .entries
                        .keys()
                        .filter(|(tk, _)| tk == &type_key)
                        .map(|(_, id)| *id)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    let created = ContentEntry {
                        id: next_id,
                        type_key: type_key.clone(),
                        slug: slug.unwrap_or_else(|| format!("entry-{next_id}")),
                        title: title.unwrap_or_else(|| format!("Entry {next_id}")),
                        status: status.unwrap_or_else(|| "draft".to_owned()),
                        version: 1,
                        locale: locale.unwrap_or_else(|| "vi".to_owned()),
                        values,
                        term_ids: term_ids.unwrap_or_default(),
                    };
                    self.entries.insert((type_key, next_id), created);
                }
                return serde_json::to_vec(&json!({ "ok": true, "data": { "staged": true } }))
                    .map_err(|e| e.to_string());
            }
            MockOpRequest::ContentDelete {
                type_key,
                entry_id,
                expected_version,
            } => {
                let Some(existing) = self.entries.get(&(type_key.clone(), entry_id)) else {
                    return Ok(serde_json::to_vec(&json!({
                        "ok": false,
                        "error": format!("entry ({type_key}, {entry_id}) not found")
                    }))
                    .unwrap());
                };
                if existing.version != expected_version {
                    return Ok(serde_json::to_vec(&json!({
                        "ok": false,
                        "error": format!("version conflict: expected {expected_version}, actual {}", existing.version)
                    }))
                    .unwrap());
                }
                self.entries.remove(&(type_key.clone(), entry_id));
                return Ok(serde_json::to_vec(&json!({
                    "ok": true,
                    "data": { "staged": true, "entry_id": entry_id },
                    "_mock_delete": [type_key, entry_id, expected_version]
                }))
                .unwrap());
            }
            MockOpRequest::ContentGet {
                type_key,
                entry_id,
                locale,
            } => {
                let found = self
                    .entries
                    .get(&(type_key, entry_id))
                    .filter(|e| locale.as_deref().is_none_or(|loc| e.locale == loc));
                json!({ "ok": true, "data": { "entry": found } })
            }
            MockOpRequest::ContentList {
                type_key,
                locale,
                limit,
                offset,
                published_only,
            } => {
                let matching: Vec<&ContentEntry> = self
                    .entries
                    .iter()
                    .filter(|((tk, _), e)| {
                        tk == &type_key
                            && locale.as_deref().is_none_or(|loc| e.locale == loc)
                            && (!published_only.unwrap_or(false) || e.status == "published")
                    })
                    .map(|(_, e)| e)
                    .collect();
                let total = matching.len() as u64;
                let off = offset.unwrap_or(0);
                let lim = limit.unwrap_or(20).clamp(1, 50);
                let items: Vec<&ContentEntry> = matching.into_iter().skip(off).take(lim).collect();
                json!({ "ok": true, "data": { "items": items, "total": total } })
            }
            MockOpRequest::TaxonomyList {
                kind,
                _locale: _,
                limit,
            } => {
                let lim = limit.unwrap_or(50).clamp(1, 100);
                let items: Vec<&TaxonomyTerm> = self
                    .taxonomies
                    .get(&kind)
                    .map(|v| v.iter().take(lim).collect())
                    .unwrap_or_default();
                json!({ "ok": true, "data": { "items": items } })
            }
            MockOpRequest::MediaGet { media_id } => {
                let item = self.media.get(&media_id);
                json!({ "ok": true, "data": { "media": item } })
            }
            MockOpRequest::SiteOptionsGet { _locale: _ } => {
                json!({ "ok": true, "data": { "options": self.site_options } })
            }
            MockOpRequest::PluginSettingsGet => {
                json!({ "ok": true, "data": { "settings": self.plugin_settings } })
            }
            MockOpRequest::PluginSecretGet { key } => {
                let val = self.plugin_secrets.get(&key).cloned();
                json!({ "ok": true, "data": { "value": val } })
            }
            MockOpRequest::Log { level, message } => {
                self.logs.push((level, message));
                json!({ "ok": true, "data": { "staged": true } })
            }
        };
        serde_json::to_vec(&resp_val).map_err(|e| e.to_string())
    }
}

pub struct MockHostOutcome<R> {
    pub result: R,
    pub output_bytes: Option<Vec<u8>>,
    pub kv: BTreeMap<String, Value>,
    pub entries: BTreeMap<(String, i64), ContentEntry>,
    pub staged_content_saves: Vec<ContentSaveInput>,
    pub staged_content_deletes: Vec<(String, i64, i64)>,
    pub logs: Vec<(LogLevel, String)>,
    pub host_calls: usize,
    pub oversized_readbacks: usize,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum MockOpRequest {
    KvGet {
        key: String,
    },
    KvSet {
        key: String,
        value: Value,
    },
    KvDelete {
        key: String,
    },
    KvList {
        #[serde(default)]
        prefix: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    HttpRequest {
        method: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default, rename = "timeout_ms")]
        _timeout_ms: Option<u64>,
    },
    ContentSave {
        type_key: String,
        #[serde(default)]
        entry_id: Option<i64>,
        #[serde(default)]
        expected_version: Option<i64>,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        slug: Option<String>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        values: Map<String, Value>,
        #[serde(default)]
        term_ids: Option<Vec<i64>>,
    },
    ContentDelete {
        type_key: String,
        entry_id: i64,
        expected_version: i64,
    },
    ContentGet {
        type_key: String,
        entry_id: i64,
        #[serde(default)]
        locale: Option<String>,
    },
    ContentList {
        type_key: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        published_only: Option<bool>,
    },
    TaxonomyList {
        kind: String,
        #[serde(default, rename = "locale")]
        _locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    MediaGet {
        media_id: String,
    },
    SiteOptionsGet {
        #[serde(default, rename = "locale")]
        _locale: Option<String>,
    },
    PluginSettingsGet,
    PluginSecretGet {
        key: String,
    },
    Log {
        level: LogLevel,
        message: String,
    },
}

struct ActiveMockState {
    host: MockHost,
    input_bytes: Vec<u8>,
    output_bytes: Option<Vec<u8>>,
    last_resp_bytes: Option<Vec<u8>>,
    staged_content_saves: Vec<ContentSaveInput>,
    staged_content_deletes: Vec<(String, i64, i64)>,
    host_calls: usize,
    oversized_readbacks: usize,
}

thread_local! {
    static ACTIVE_MOCK: RefCell<Option<ActiveMockState>> = const { RefCell::new(None) };
}

struct MockResetGuard {
    prev: Option<ActiveMockState>,
}

impl Drop for MockResetGuard {
    fn drop(&mut self) {
        ACTIVE_MOCK.with(|cell| {
            *cell.borrow_mut() = self.prev.take();
        });
    }
}

pub fn run_in_mock_host<F, R>(host: MockHost, envelope: &Envelope, f: F) -> MockHostOutcome<R>
where
    F: FnOnce() -> R,
{
    let input_bytes = serde_json::to_vec(envelope).expect("serialize envelope for MockHost");
    let prev = ACTIVE_MOCK.with(|cell| {
        cell.borrow_mut().replace(ActiveMockState {
            host,
            input_bytes,
            output_bytes: None,
            last_resp_bytes: None,
            staged_content_saves: Vec::new(),
            staged_content_deletes: Vec::new(),
            host_calls: 0,
            oversized_readbacks: 0,
        })
    });
    let mut guard = MockResetGuard { prev };
    let result = f();
    let final_state = ACTIVE_MOCK
        .with(|cell| cell.borrow_mut().take())
        .expect("active mock state missing after run");
    // Restore previous nested state if any before returning.
    let _ = guard.prev.take().map(|p| {
        ACTIVE_MOCK.with(|cell| {
            *cell.borrow_mut() = Some(p);
        });
    });

    MockHostOutcome {
        result,
        output_bytes: final_state.output_bytes,
        kv: final_state.host.kv,
        entries: final_state.host.entries,
        staged_content_saves: final_state.staged_content_saves,
        staged_content_deletes: final_state.staged_content_deletes,
        logs: final_state.host.logs,
        host_calls: final_state.host_calls,
        oversized_readbacks: final_state.oversized_readbacks,
    }
}

pub(crate) fn abi_get_input_len() -> Result<i32, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let borrow = cell.borrow();
        let state = borrow.as_ref().ok_or(SdkError::MockNotInitialized)?;
        Ok(state.input_bytes.len() as i32)
    })
}

pub(crate) fn abi_read_input(dest: &mut [u8]) -> Result<i32, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let borrow = cell.borrow();
        let state = borrow.as_ref().ok_or(SdkError::MockNotInitialized)?;
        let n = dest.len().min(state.input_bytes.len());
        dest[..n].copy_from_slice(&state.input_bytes[..n]);
        Ok(n as i32)
    })
}

pub(crate) fn abi_write_output(bytes: &[u8]) -> Result<i32, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow.as_mut().ok_or(SdkError::MockNotInitialized)?;
        if bytes.len() > 1024 * 1024 {
            return Ok(-2);
        }
        if std::str::from_utf8(bytes).is_err() {
            return Ok(-3);
        }
        if serde_json::from_slice::<Value>(bytes).is_err() {
            return Ok(-4);
        }
        state.output_bytes = Some(bytes.to_vec());
        Ok(0)
    })
}

pub(crate) fn abi_initial_resp_buf_cap() -> Result<usize, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let borrow = cell.borrow();
        let state = borrow.as_ref().ok_or(SdkError::MockNotInitialized)?;
        Ok(state.host.initial_resp_buf_cap)
    })
}

pub(crate) fn abi_host_call(req_json: &[u8], resp_buf: &mut [u8]) -> Result<i32, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow.as_mut().ok_or(SdkError::MockNotInitialized)?;
        state.host_calls += 1;
        state.last_resp_bytes = None;

        // Track staged save/delete inputs cleanly on the active mock state when valid
        if let Ok(req) = serde_json::from_slice::<MockOpRequest>(req_json) {
            match req {
                MockOpRequest::ContentSave {
                    type_key,
                    entry_id,
                    expected_version,
                    locale,
                    title,
                    slug,
                    status,
                    values,
                    term_ids,
                } => {
                    let save_input = ContentSaveInput {
                        type_key,
                        entry_id,
                        expected_version,
                        locale,
                        title,
                        slug,
                        status,
                        values,
                        term_ids,
                    };
                    let resp_bytes = state
                        .host
                        .dispatch_call(req_json)
                        .map_err(SdkError::HostError)?;
                    if let Ok(v) = serde_json::from_slice::<Value>(&resp_bytes)
                        && v.get("ok").and_then(Value::as_bool) == Some(true)
                    {
                        state.staged_content_saves.push(save_input);
                    }
                    return write_mock_resp(state, resp_bytes, resp_buf);
                }
                MockOpRequest::ContentDelete {
                    type_key,
                    entry_id,
                    expected_version,
                } => {
                    let resp_bytes = state
                        .host
                        .dispatch_call(req_json)
                        .map_err(SdkError::HostError)?;
                    let clean_resp = if let Ok(mut v) = serde_json::from_slice::<Value>(&resp_bytes)
                    {
                        if v.get("ok").and_then(Value::as_bool) == Some(true) {
                            state.staged_content_deletes.push((
                                type_key,
                                entry_id,
                                expected_version,
                            ));
                            if let Some(obj) = v.as_object_mut() {
                                obj.remove("_mock_delete");
                            }
                            serde_json::to_vec(&v).unwrap_or(resp_bytes)
                        } else {
                            resp_bytes
                        }
                    } else {
                        resp_bytes
                    };
                    return write_mock_resp(state, clean_resp, resp_buf);
                }
                _ => {}
            }
        }

        let resp_bytes = state
            .host
            .dispatch_call(req_json)
            .map_err(SdkError::HostError)?;
        write_mock_resp(state, resp_bytes, resp_buf)
    })
}

fn write_mock_resp(
    state: &mut ActiveMockState,
    resp_bytes: Vec<u8>,
    resp_buf: &mut [u8],
) -> Result<i32, SdkError> {
    if resp_bytes.len() > resp_buf.len() {
        state.last_resp_bytes = Some(resp_bytes);
        state.oversized_readbacks += 1;
        return Ok(-2);
    }
    let n = resp_bytes.len();
    resp_buf[..n].copy_from_slice(&resp_bytes);
    Ok(n as i32)
}

pub(crate) fn abi_host_last_resp_len() -> Result<i32, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let borrow = cell.borrow();
        let state = borrow.as_ref().ok_or(SdkError::MockNotInitialized)?;
        Ok(state
            .last_resp_bytes
            .as_ref()
            .map(|b| b.len() as i32)
            .unwrap_or(0))
    })
}

pub(crate) fn abi_host_read_last_resp(dest: &mut [u8]) -> Result<i32, SdkError> {
    ACTIVE_MOCK.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow.as_mut().ok_or(SdkError::MockNotInitialized)?;
        let Some(bytes) = state.last_resp_bytes.as_ref() else {
            return Ok(-1);
        };
        let to_copy = dest.len().min(bytes.len());
        dest[..to_copy].copy_from_slice(&bytes[..to_copy]);
        if to_copy == bytes.len() {
            state.last_resp_bytes = None;
        }
        Ok(to_copy as i32)
    })
}
