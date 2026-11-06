// OpenDash AssemblyScript SDK v1
import { JSON } from "assemblyscript-json/assembly";
export { JSON };

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

/**
 * Đọc toàn bộ chuỗi JSON input từ OpenDash host.
 */
export function getInput(): string {
  const len = host_get_input_len();
  if (len <= 0) {
    return "";
  }
  const buf = new ArrayBuffer(len);
  const ptr = changetype<usize>(buf);
  const readLen = host_read_input(ptr, len);
  if (readLen <= 0) {
    return "";
  }
  return String.UTF8.decode(buf);
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
 */
export function hostCall(opJson: string): string {
  const opBuf = String.UTF8.encode(opJson);
  const opPtr = changetype<usize>(opBuf);

  // Cấp phát vùng đệm 64 KB cho phản hồi từ host
  const maxRespLen: i32 = 65536;
  const respBuf = new ArrayBuffer(maxRespLen);
  const respPtr = changetype<usize>(respBuf);

  const respLen = host_call(opPtr, opBuf.byteLength, respPtr, maxRespLen);
  if (respLen <= 0) {
    return '{"ok":false,"error":"Host call returned error code"}';
  }
  return String.UTF8.decode(respBuf.slice(0, respLen));
}

/**
 * Đọc giá trị từ SQLite KV store của plugin.
 * Trả về chuỗi JSON raw của value nếu tìm thấy, hoặc null nếu không tồn tại.
 */
export function kvGet(key: string): string | null {
  const reqObj = new JSON.Obj();
  reqObj.set("op", "kv_get");
  reqObj.set("key", key);

  const resStr = hostCall(stringifyValue(reqObj));
  const resVal = JSON.parse(resStr);
  if (!(resVal instanceof JSON.Obj)) {
    return null;
  }
  const resObj = <JSON.Obj>resVal;
  const okVal = resObj.getBool("ok");
  if (okVal === null || !okVal.valueOf()) {
    return null;
  }
  const foundVal = resObj.getBool("found");
  if (foundVal === null || !foundVal.valueOf()) {
    return null;
  }
  const val = resObj.getValue("value");
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
 * Gửi HTTP request (GET/POST) tới endpoint đã được cấp quyền trong manifest.
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
