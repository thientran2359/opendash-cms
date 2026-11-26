use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{
    abi::{host_call_raw, read_input_bytes, write_output_bytes},
    types::{
        ContentEntry, ContentListResult, ContentSaveInput, ContentTrashListResult, Envelope,
        HostResponse, HttpRequestInput, HttpResponse, KvItem, LogLevel, MediaAsset, SdkError,
        TaxonomyTerm, TrashEntrySummary,
    },
};

pub fn read_envelope() -> Result<Envelope, SdkError> {
    let bytes = read_input_bytes()?;
    let raw_str =
        std::str::from_utf8(&bytes).map_err(|e| SdkError::InvalidInputUtf8(e.to_string()))?;
    serde_json::from_str(raw_str).map_err(|e| SdkError::Json(e.to_string()))
}

pub fn write_output<T: Serialize>(value: &T) -> Result<(), SdkError> {
    let bytes = serde_json::to_vec(value).map_err(|e| SdkError::Json(e.to_string()))?;
    write_output_bytes(&bytes)
}

fn call_host(req: &impl Serialize) -> Result<HostResponse, SdkError> {
    let req_bytes = serde_json::to_vec(req).map_err(|e| SdkError::Json(e.to_string()))?;
    let resp_bytes = host_call_raw(&req_bytes)?;
    let resp: HostResponse = serde_json::from_slice(&resp_bytes)
        .map_err(|e| SdkError::UnexpectedResponse(e.to_string()))?;
    if !resp.ok {
        return Err(SdkError::HostError(
            resp.error
                .unwrap_or_else(|| "unknown host operation error".to_owned()),
        ));
    }
    Ok(resp)
}

fn extract_data_obj(resp: HostResponse) -> Result<Map<String, Value>, SdkError> {
    let data = resp
        .data
        .ok_or_else(|| SdkError::UnexpectedResponse("missing `data` object".into()))?;
    match data {
        Value::Object(map) => Ok(map),
        other => Err(SdkError::UnexpectedResponse(format!(
            "expected object `data`, got {other}"
        ))),
    }
}

pub fn kv_get(key: &str) -> Result<Option<Value>, SdkError> {
    let resp = call_host(&json!({ "op": "kv_get", "key": key }))?;
    if let Some(Value::Object(map)) = resp.data {
        let found = map.get("found").and_then(Value::as_bool).unwrap_or(false);
        return Ok(if found {
            map.get("value").cloned()
        } else {
            None
        });
    }
    let found = resp.found.unwrap_or(false);
    Ok(if found { resp.value } else { None })
}

pub fn kv_set(key: &str, value: &Value) -> Result<(), SdkError> {
    let _ = call_host(&json!({ "op": "kv_set", "key": key, "value": value }))?;
    Ok(())
}

pub fn kv_delete(key: &str) -> Result<bool, SdkError> {
    let resp = call_host(&json!({ "op": "kv_delete", "key": key }))?;
    let map = extract_data_obj(resp)?;
    Ok(map.get("deleted").and_then(Value::as_bool).unwrap_or(false))
}

pub fn kv_list(prefix: Option<&str>, limit: Option<usize>) -> Result<Vec<KvItem>, SdkError> {
    #[derive(Serialize)]
    struct KvListReq<'a> {
        op: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        prefix: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    }
    let resp = call_host(&KvListReq {
        op: "kv_list",
        prefix,
        limit,
    })?;
    let mut map = extract_data_obj(resp)?;
    let items_val = map
        .remove("items")
        .ok_or_else(|| SdkError::UnexpectedResponse("missing `data.items`".into()))?;
    serde_json::from_value(items_val).map_err(|e| SdkError::UnexpectedResponse(e.to_string()))
}

pub fn http_request(req: &HttpRequestInput<'_>) -> Result<HttpResponse, SdkError> {
    #[derive(Serialize)]
    struct WireHttpReq<'a> {
        op: &'static str,
        #[serde(flatten)]
        input: &'a HttpRequestInput<'a>,
    }
    let resp = call_host(&WireHttpReq {
        op: "http_request",
        input: req,
    })?;
    if let Some(data) = resp.data {
        return serde_json::from_value(data)
            .map_err(|e| SdkError::UnexpectedResponse(e.to_string()));
    }
    let status = resp
        .status
        .ok_or_else(|| SdkError::UnexpectedResponse("missing http `status`".into()))?;
    Ok(HttpResponse {
        status,
        headers: resp.headers.unwrap_or_default(),
        body: resp.body.unwrap_or_default(),
    })
}

pub fn content_save(input: &ContentSaveInput) -> Result<Value, SdkError> {
    #[derive(Serialize)]
    struct WireContentSave<'a> {
        op: &'static str,
        #[serde(flatten)]
        input: &'a ContentSaveInput,
    }
    let resp = call_host(&WireContentSave {
        op: "content_save",
        input,
    })?;
    Ok(resp.data.unwrap_or_else(|| json!({ "ok": true })))
}

pub fn content_delete(
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
) -> Result<(), SdkError> {
    let _ = call_host(&json!({
        "op": "content_delete",
        "type_key": type_key,
        "entry_id": entry_id,
        "expected_version": expected_version,
    }))?;
    Ok(())
}

pub fn content_restore(
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
) -> Result<(), SdkError> {
    let _ = call_host(&json!({
        "op": "content_restore",
        "type_key": type_key,
        "entry_id": entry_id,
        "expected_version": expected_version,
    }))?;
    Ok(())
}

pub fn content_purge(type_key: &str, entry_id: i64, expected_version: i64) -> Result<(), SdkError> {
    let _ = call_host(&json!({
        "op": "content_purge",
        "type_key": type_key,
        "entry_id": entry_id,
        "expected_version": expected_version,
    }))?;
    Ok(())
}

pub fn content_trash_get(
    type_key: &str,
    entry_id: i64,
    locale: Option<&str>,
) -> Result<Option<TrashEntrySummary>, SdkError> {
    #[derive(Serialize)]
    struct ContentTrashGetReq<'a> {
        op: &'static str,
        type_key: &'a str,
        entry_id: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
    }
    let resp = call_host(&ContentTrashGetReq {
        op: "content_trash_get",
        type_key,
        entry_id,
        locale,
    })?;
    let mut map = extract_data_obj(resp)?;
    match map.remove("entry") {
        None | Some(Value::Null) => Ok(None),
        Some(val) => serde_json::from_value(val)
            .map(Some)
            .map_err(|e| SdkError::UnexpectedResponse(e.to_string())),
    }
}

pub fn content_trash_list(
    type_key: &str,
    locale: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<ContentTrashListResult, SdkError> {
    #[derive(Serialize)]
    struct ContentTrashListReq<'a> {
        op: &'static str,
        type_key: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        offset: Option<usize>,
    }
    let resp = call_host(&ContentTrashListReq {
        op: "content_trash_list",
        type_key,
        locale,
        limit,
        offset,
    })?;
    let data = resp
        .data
        .ok_or_else(|| SdkError::UnexpectedResponse("missing content_trash_list `data`".into()))?;
    serde_json::from_value(data).map_err(|e| SdkError::UnexpectedResponse(e.to_string()))
}

pub fn content_get(
    type_key: &str,
    entry_id: i64,
    locale: Option<&str>,
) -> Result<Option<ContentEntry>, SdkError> {
    #[derive(Serialize)]
    struct ContentGetReq<'a> {
        op: &'static str,
        type_key: &'a str,
        entry_id: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
    }
    let resp = call_host(&ContentGetReq {
        op: "content_get",
        type_key,
        entry_id,
        locale,
    })?;
    let mut map = extract_data_obj(resp)?;
    match map.remove("entry") {
        None | Some(Value::Null) => Ok(None),
        Some(val) => serde_json::from_value(val)
            .map(Some)
            .map_err(|e| SdkError::UnexpectedResponse(e.to_string())),
    }
}

pub fn content_list(
    type_key: &str,
    locale: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
    published_only: Option<bool>,
) -> Result<ContentListResult, SdkError> {
    #[derive(Serialize)]
    struct ContentListReq<'a> {
        op: &'static str,
        type_key: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        offset: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        published_only: Option<bool>,
    }
    let resp = call_host(&ContentListReq {
        op: "content_list",
        type_key,
        locale,
        limit,
        offset,
        published_only,
    })?;
    let data = resp
        .data
        .ok_or_else(|| SdkError::UnexpectedResponse("missing content_list `data`".into()))?;
    serde_json::from_value(data).map_err(|e| SdkError::UnexpectedResponse(e.to_string()))
}

pub fn taxonomy_list(
    kind: &str,
    locale: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<TaxonomyTerm>, SdkError> {
    #[derive(Serialize)]
    struct TaxonomyListReq<'a> {
        op: &'static str,
        kind: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    }
    let resp = call_host(&TaxonomyListReq {
        op: "taxonomy_list",
        kind,
        locale,
        limit,
    })?;
    let mut map = extract_data_obj(resp)?;
    let items_val = map
        .remove("items")
        .ok_or_else(|| SdkError::UnexpectedResponse("missing `data.items`".into()))?;
    serde_json::from_value(items_val).map_err(|e| SdkError::UnexpectedResponse(e.to_string()))
}

pub fn media_get(media_id: &str) -> Result<Option<MediaAsset>, SdkError> {
    let resp = call_host(&json!({ "op": "media_get", "media_id": media_id }))?;
    let mut map = extract_data_obj(resp)?;
    match map.remove("media") {
        None | Some(Value::Null) => Ok(None),
        Some(val) => serde_json::from_value(val)
            .map(Some)
            .map_err(|e| SdkError::UnexpectedResponse(e.to_string())),
    }
}

pub fn site_options_get(locale: Option<&str>) -> Result<Map<String, Value>, SdkError> {
    #[derive(Serialize)]
    struct SiteOptsReq<'a> {
        op: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<&'a str>,
    }
    let resp = call_host(&SiteOptsReq {
        op: "site_options_get",
        locale,
    })?;
    let mut map = extract_data_obj(resp)?;
    let opts = map
        .remove("options")
        .ok_or_else(|| SdkError::UnexpectedResponse("missing `data.options`".into()))?;
    match opts {
        Value::Object(m) => Ok(m),
        other => Err(SdkError::UnexpectedResponse(format!(
            "expected object `data.options`, got {other}"
        ))),
    }
}

pub fn plugin_settings_get() -> Result<Map<String, Value>, SdkError> {
    let resp = call_host(&json!({ "op": "plugin_settings_get" }))?;
    let mut map = extract_data_obj(resp)?;
    let settings = map
        .remove("settings")
        .ok_or_else(|| SdkError::UnexpectedResponse("missing `data.settings`".into()))?;
    match settings {
        Value::Object(m) => Ok(m),
        other => Err(SdkError::UnexpectedResponse(format!(
            "expected object `data.settings`, got {other}"
        ))),
    }
}

pub fn plugin_secret_get(key: &str) -> Result<Option<String>, SdkError> {
    let resp = call_host(&json!({ "op": "plugin_secret_get", "key": key }))?;
    let mut map = extract_data_obj(resp)?;
    match map.remove("value") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(SdkError::UnexpectedResponse(format!(
            "expected string or null `data.value`, got {other}"
        ))),
    }
}

pub fn log(level: LogLevel, message: &str) -> Result<(), SdkError> {
    let _ = call_host(&json!({
        "op": "log",
        "level": level,
        "message": message,
    }))?;
    Ok(())
}
