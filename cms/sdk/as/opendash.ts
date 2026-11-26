// OpenDash AssemblyScript SDK v1.1 (ABI opendash_v1 + ABI v1.1 -2 readback & Host Ops)
import { JSON } from "assemblyscript-json/assembly";
export { JSON };

export const INITIAL_RESP_BUF_CAP: i32 = 65536;
export const MAX_IPC_FRAME_BYTES: i32 = 2097152;
export const BUFFER_TOO_SMALL: i32 = -2;

/**
 * Escape string theo đúng đặc tả RFC 8259 của JSON,
 * đảm bảo các ký tự đặc biệt ", \, \n, \r, \t, \b, \f và ký tự điều khiển đều hợp lệ.
 */
export function escapeJsonString(str: string): string {
  let out = '"';
  let savedIndex = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    if (
      char < 0x20 ||
      char == 0x22 || // "
      char == 0x5c // \
    ) {
      out += str.substring(savedIndex, i);
      savedIndex = i + 1;
      if (char == 0x22) {
        out += '\\"';
      } else if (char == 0x5c) {
        out += "\\\\";
      } else if (char == 0x0a) {
        out += "\\n";
      } else if (char == 0x0d) {
        out += "\\r";
      } else if (char == 0x09) {
        out += "\\t";
      } else if (char == 0x08) {
        out += "\\b";
      } else if (char == 0x0c) {
        out += "\\f";
      } else {
        const hex = char.toString(16);
        out += "\\u00" + (hex.length == 1 ? "0" + hex : hex);
      }
    }
  }
  out += str.substring(savedIndex, str.length);
  out += '"';
  return out;
}

/**
 * Chuyển đổi an toàn bất kỳ JSON.Value nào thành chuỗi JSON hợp lệ.
 */
export function stringifyValue(val: JSON.Value): string {
  if (val instanceof JSON.Str) {
    return escapeJsonString((<JSON.Str>val)._str);
  } else if (val instanceof JSON.Obj) {
    const obj = <JSON.Obj>val;
    const keys = obj.keys;
    const parts: string[] = new Array<string>(keys.length);
    for (let i = 0; i < keys.length; i++) {
      const k = keys[i];
      const v = obj.get(k);
      if (v !== null) {
        parts[i] = escapeJsonString(k) + ":" + stringifyValue(v);
      } else {
        parts[i] = escapeJsonString(k) + ":null";
      }
    }
    return "{" + parts.join(",") + "}";
  } else if (val instanceof JSON.Arr) {
    const arr = (<JSON.Arr>val)._arr;
    const parts: string[] = new Array<string>(arr.length);
    for (let i = 0; i < arr.length; i++) {
      parts[i] = stringifyValue(arr[i]);
    }
    return "[" + parts.join(",") + "]";
  } else {
    return val.stringify();
  }
}

@external("opendash_v1", "host_get_input_len")
declare function host_get_input_len(): i32;

@external("opendash_v1", "host_read_input")
declare function host_read_input(destPtr: usize, maxLen: i32): i32;

@external("opendash_v1", "host_write_output")
declare function host_write_output(srcPtr: usize, len: i32): i32;

@external("opendash_v1", "host_call")
declare function host_call(opPtr: usize, opLen: i32, respPtr: usize, maxRespLen: i32): i32;

@external("opendash_v1", "host_last_resp_len")
declare function host_last_resp_len(): i32;

@external("opendash_v1", "host_read_last_resp")
declare function host_read_last_resp(destPtr: usize, maxLen: i32): i32;

/**
 * Đọc toàn bộ chuỗi JSON input từ OpenDash host.
 */
export function getInput(): string {
  const len = host_get_input_len();
  if (len <= 0 || len > MAX_IPC_FRAME_BYTES) {
    return "";
  }
  const buf = new ArrayBuffer(len);
  const ptr = changetype<usize>(buf);
  const readLen = host_read_input(ptr, len);
  if (readLen <= 0 || readLen > len) {
    return "";
  }
  return String.UTF8.decode(readLen == len ? buf : buf.slice(0, readLen));
}

/**
 * Ghi chuỗi JSON kết quả trả về cho OpenDash host.
 * Trả về 0 nếu thành công, hoặc số âm nếu có lỗi.
 */
export function setOutput(jsonStr: string): i32 {
  const buf = String.UTF8.encode(jsonStr);
  const ptr = changetype<usize>(buf);
  return host_write_output(ptr, buf.byteLength);
}

/**
 * Gọi hàm host op qua giao thức JSON opendash_v1.
 * Tự động đọc lại payload vượt đệm 64 KiB khi host trả mã -2 (BUFFER_TOO_SMALL)
 * qua host_last_resp_len() / host_read_last_resp() mà không gọi lại host_call().
 */
export function hostCall(opJson: string): string {
  const opBuf = String.UTF8.encode(opJson);
  const opPtr = changetype<usize>(opBuf);

  // Cấp phát vùng đệm khởi đầu 64 KiB cho phản hồi từ host
  const maxRespLen: i32 = INITIAL_RESP_BUF_CAP;
  const respBuf = new ArrayBuffer(maxRespLen);
  const respPtr = changetype<usize>(respBuf);

  const respLen = host_call(opPtr, opBuf.byteLength, respPtr, maxRespLen);
  if (respLen > 0) {
    return String.UTF8.decode(respBuf.slice(0, respLen));
  }
  if (respLen == 0) {
    return '{"ok":false,"error":"Empty host response"}';
  }
  if (respLen == BUFFER_TOO_SMALL) {
    const fullLen = host_last_resp_len();
    if (fullLen <= 0 || fullLen > MAX_IPC_FRAME_BYTES) {
      return '{"ok":false,"error":"Invalid oversized response length"}';
    }
    const fullBuf = new ArrayBuffer(fullLen);
    const copied = host_read_last_resp(changetype<usize>(fullBuf), fullLen);
    if (copied <= 0 || copied > fullLen) {
      return '{"ok":false,"error":"Failed to read oversized host response"}';
    }
    return String.UTF8.decode(copied == fullLen ? fullBuf : fullBuf.slice(0, copied));
  }
  return '{"ok":false,"error":"Host call returned error code"}';
}

function extractDataObj(resStr: string): JSON.Obj | null {
  const resVal = JSON.parse(resStr);
  if (!(resVal instanceof JSON.Obj)) {
    return null;
  }
  const resObj = <JSON.Obj>resVal;
  const okVal = resObj.getBool("ok");
  if (okVal === null || !okVal.valueOf()) {
    return null;
  }
  const dataVal = resObj.getValue("data");
  if (dataVal !== null && dataVal instanceof JSON.Obj) {
    return <JSON.Obj>dataVal;
  }
  return resObj;
}

function readIntField(obj: JSON.Obj, key: string, fallback: i64): i64 {
  const intVal = obj.getInteger(key);
  if (intVal !== null) {
    return intVal.valueOf();
  }
  const numVal = obj.getNum(key);
  if (numVal !== null) {
    return i64(numVal.valueOf());
  }
  return fallback;
}

function readStringField(obj: JSON.Obj, key: string, fallback: string): string {
  const strVal = obj.getString(key);
  if (strVal !== null) {
    return strVal.valueOf();
  }
  return fallback;
}

export class Envelope {
  envelopeVersion: i32 = 1;
  kind: string = "hook";
  event: string = "";
  occurredAtMs: i64 = 0;
  correlationId: string = "";
  locale: string = "";
  principal: string = "system";
  dispatchDepth: i32 = 0;
  payload: JSON.Obj = new JSON.Obj();
  raw: JSON.Obj = new JSON.Obj();
}

/**
 * Đọc đầu vào của plugin, hỗ trợ cả HookEnvelope v1 lẫn PluginEnvelopeV2.
 */
export function readEnvelope(): Envelope | null {
  const inputStr = getInput();
  if (inputStr.length == 0) {
    return null;
  }
  const parsed = JSON.parse(inputStr);
  if (!(parsed instanceof JSON.Obj)) {
    return null;
  }
  const raw = <JSON.Obj>parsed;
  const env = new Envelope();
  env.raw = raw;
  env.event = readStringField(raw, "event", "");
  env.occurredAtMs = readIntField(raw, "occurred_at_ms", 0);
  env.correlationId = readStringField(raw, "correlation_id", "");
  env.locale = readStringField(raw, "locale", "");

  const payloadObj = raw.getObj("payload");
  if (payloadObj !== null) {
    env.payload = payloadObj;
  }

  if (raw.has("envelope_version")) {
    env.envelopeVersion = i32(readIntField(raw, "envelope_version", 2));
    env.kind = readStringField(raw, "kind", "hook");
    env.principal = readStringField(raw, "principal", "system");
    env.dispatchDepth = i32(readIntField(raw, "dispatch_depth", 0));
  } else {
    env.envelopeVersion = 1;
    env.kind = "hook";
    env.principal = "system";
    env.dispatchDepth = 0;
  }

  return env;
}

/**
 * Đọc giá trị từ SQLite KV store của plugin.
 * Trả về chuỗi JSON raw của value nếu tìm thấy, hoặc null nếu không tồn tại.
 */
export function kvGet(key: string): string | null {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "kv_get");
  reqObj.set("key", key);

  const dataObj = extractDataObj(hostCall(stringifyValue(reqObj)));
  if (dataObj === null) {
    return null;
  }
  const foundVal = dataObj.getBool("found");
  if (foundVal === null || !foundVal.valueOf()) {
    return null;
  }
  const val = dataObj.getValue("value");
  if (val === null) {
    return null;
  }
  return stringifyValue(val);
}

/**
 * Ghi giá trị vào SQLite KV store của plugin.
 * valueJson phải là JSON hợp lệ (số, chuỗi "...", hoặc object/array/null).
 */
export function kvSet(key: string, valueJson: string): bool {
  const parsedVal = JSON.parse(valueJson);
  const reqObj = new JSON.Obj();
  reqObj.set("op", "kv_set");
  reqObj.set("key", key);
  reqObj.set("value", parsedVal);

  const resStr = hostCall(stringifyValue(reqObj));
  const resVal = JSON.parse(resStr);
  if (!(resVal instanceof JSON.Obj)) {
    return false;
  }
  const okVal = (<JSON.Obj>resVal).getBool("ok");
  return okVal !== null && okVal.valueOf();
}

/**
 * Xóa một khóa khỏi SQLite KV store của plugin.
 */
export function kvDelete(key: string): bool {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "kv_delete");
  reqObj.set("key", key);

  const dataObj = extractDataObj(hostCall(stringifyValue(reqObj)));
  if (dataObj === null) {
    return false;
  }
  const deletedVal = dataObj.getBool("deleted");
  return deletedVal !== null && deletedVal.valueOf();
}

/**
 * Liệt kê danh sách khóa/giá trị trong KV store (tùy chọn prefix & limit).
 */
export function kvList(prefix: string | null = null, limit: i32 = 50): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "kv_list");
  if (prefix !== null && prefix.length > 0) {
    reqObj.set("prefix", prefix);
  }
  reqObj.set("limit", limit);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Gửi HTTP request (GET/POST/...) tới endpoint đã được cấp quyền trong manifest.
 */
export function httpRequest(
  method: string,
  url: string,
  headersJson: string = "{}",
  body: string = ""
): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "http_request");
  reqObj.set("method", method);
  reqObj.set("url", url);
  if (headersJson.length > 0 && headersJson != "{}") {
    reqObj.set("headers", JSON.parse(headersJson));
  }
  if (body.length > 0) {
    reqObj.set("body", body);
  }
  return hostCall(stringifyValue(reqObj));
}

/**
 * Đọc một entry theo loại nội dung, ID và locale tùy chọn.
 */
export function contentGet(typeKey: string, entryId: i64, locale: string | null = null): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_get");
  reqObj.set("type_key", typeKey);
  reqObj.set("entry_id", entryId);
  if (locale !== null && locale.length > 0) {
    reqObj.set("locale", locale);
  }
  return hostCall(stringifyValue(reqObj));
}

/**
 * Liệt kê danh sách entries theo loại nội dung, locale, limit, offset và published_only.
 */
export function contentList(
  typeKey: string,
  locale: string | null = null,
  limit: i32 = 20,
  offset: i32 = 0,
  publishedOnly: bool = false
): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_list");
  reqObj.set("type_key", typeKey);
  if (locale !== null && locale.length > 0) {
    reqObj.set("locale", locale);
  }
  reqObj.set("limit", limit);
  reqObj.set("offset", offset);
  reqObj.set("published_only", publishedOnly);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Ghi (tạo/cập nhật) một entry từ action có capability content_write.
 * Tự động gán "op": "content_save" nếu caller chưa đặt.
 */
export function contentSave(opObj: JSON.Obj): string {
  if (!opObj.has("op")) {
    opObj.set("op", "content_save");
  }
  return hostCall(stringifyValue(opObj));
}

/**
 * Chuyển một entry vào Thùng rác (recoverable Trash) theo cơ chế CAS expected_version.
 */
export function contentDelete(typeKey: string, entryId: i64, expectedVersion: i64): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_delete");
  reqObj.set("type_key", typeKey);
  reqObj.set("entry_id", entryId);
  reqObj.set("expected_version", expectedVersion);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Đọc metadata-only của một entry đang nằm trong Thùng rác (cần capability content_trash_read).
 */
export function contentTrashGet(
  typeKey: string,
  entryId: i64,
  locale: string | null = null
): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_trash_get");
  reqObj.set("type_key", typeKey);
  reqObj.set("entry_id", entryId);
  if (locale !== null && locale.length > 0) {
    reqObj.set("locale", locale);
  }
  return hostCall(stringifyValue(reqObj));
}

/**
 * Liệt kê danh sách metadata-only của các entries trong Thùng rác theo type_key, locale, limit và offset.
 */
export function contentTrashList(
  typeKey: string,
  locale: string | null = null,
  limit: i32 = 20,
  offset: i32 = 0
): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_trash_list");
  reqObj.set("type_key", typeKey);
  if (locale !== null && locale.length > 0) {
    reqObj.set("locale", locale);
  }
  reqObj.set("limit", limit);
  reqObj.set("offset", offset);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Khôi phục một entry từ Thùng rác theo cơ chế CAS expected_version (cần capability content_restore).
 */
export function contentRestore(typeKey: string, entryId: i64, expectedVersion: i64): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_restore");
  reqObj.set("type_key", typeKey);
  reqObj.set("entry_id", entryId);
  reqObj.set("expected_version", expectedVersion);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Xóa vĩnh viễn một entry đang nằm trong Thùng rác theo cơ chế CAS expected_version (chỉ Admin, cần xác nhận Host).
 */
export function contentPurge(typeKey: string, entryId: i64, expectedVersion: i64): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "content_purge");
  reqObj.set("type_key", typeKey);
  reqObj.set("entry_id", entryId);
  reqObj.set("expected_version", expectedVersion);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Liệt kê taxonomy terms theo kind, locale và limit tùy chọn.
 */
export function taxonomyList(kind: string, locale: string | null = null, limit: i32 = 50): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "taxonomy_list");
  reqObj.set("kind", kind);
  if (locale !== null && locale.length > 0) {
    reqObj.set("locale", locale);
  }
  reqObj.set("limit", limit);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Đọc metadata và URL của một media item.
 */
export function mediaGet(mediaId: string): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "media_get");
  reqObj.set("media_id", mediaId);
  return hostCall(stringifyValue(reqObj));
}

/**
 * Đọc tùy chọn công khai của Theme đang kích hoạt theo locale tùy chọn.
 */
export function siteOptionsGet(locale: string | null = null): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "site_options_get");
  if (locale !== null && locale.length > 0) {
    reqObj.set("locale", locale);
  }
  return hostCall(stringifyValue(reqObj));
}

/**
 * Đọc cấu hình riêng của plugin (trường kiểu secret được che chỉ còn {configured: bool}).
 */
export function pluginSettingsGet(): string {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "plugin_settings_get");
  return hostCall(stringifyValue(reqObj));
}

/**
 * Giải mã và đọc một khóa secret thuộc plugin_secrets_read.
 */
export function pluginSecretGet(key: string): string | null {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "plugin_secret_get");
  reqObj.set("key", key);

  const dataObj = extractDataObj(hostCall(stringifyValue(reqObj)));
  if (dataObj === null) {
    return null;
  }
  const val = dataObj.getValue("value");
  if (val !== null && val instanceof JSON.Str) {
    return (<JSON.Str>val).valueOf();
  }
  return null;
}

/**
 * Ghi log có cấu trúc ("info" | "warn" | "error") lên Host.
 */
export function log(level: string, message: string): bool {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "log");
  reqObj.set("level", level);
  reqObj.set("message", message);

  const resStr = hostCall(stringifyValue(reqObj));
  const resVal = JSON.parse(resStr);
  if (!(resVal instanceof JSON.Obj)) {
    return false;
  }
  const okVal = (<JSON.Obj>resVal).getBool("ok");
  return okVal !== null && okVal.valueOf();
}

/**
 * Kết xuất quyết định cho phép từ hook content/before_save.
 */
export function outputAllow(): i32 {
  const obj = new JSON.Obj();
  obj.set("decision", "allow");
  return setOutput(stringifyValue(obj));
}

/**
 * Kết xuất quyết định từ chối (veto) từ hook content/before_save.
 */
export function outputVeto(
  code: string,
  message: string,
  fieldErrorsObj: JSON.Obj | null = null
): i32 {
  const obj = new JSON.Obj();
  obj.set("decision", "veto");
  obj.set("code", code);
  obj.set("message", message);
  if (fieldErrorsObj !== null) {
    obj.set("field_errors", fieldErrorsObj);
  }
  return setOutput(stringifyValue(obj));
}

/**
 * Kết xuất bản vá ứng viên từ filter:content/prepare_save.
 */
export function outputPrepareSavePatch(patchObj: JSON.Obj): i32 {
  return setOutput(stringifyValue(patchObj));
}

/**
 * Kết xuất quyết định cho phép gửi form liên hệ từ filter:contact/before_submit.
 */
export function outputContactAllow(): i32 {
  const obj = new JSON.Obj();
  obj.set("decision", "allow");
  return setOutput(stringifyValue(obj));
}

/**
 * Kết xuất quyết định từ chối form liên hệ từ filter:contact/before_submit.
 */
export function outputContactReject(code: string, message: string): i32 {
  const obj = new JSON.Obj();
  obj.set("decision", "reject");
  obj.set("code", code);
  obj.set("message", message);
  return setOutput(stringifyValue(obj));
}

/**
 * Kết xuất dữ liệu projection cho filter:seo/head.
 */
export function outputSeoHead(seoObj: JSON.Obj): i32 {
  return setOutput(stringifyValue(seoObj));
}

/**
 * Kết xuất dữ liệu mở rộng entry.ext.<plugin_id> cho filter:entry/view.
 */
export function outputEntryView(extObj: JSON.Obj): i32 {
  return setOutput(stringifyValue(extObj));
}
