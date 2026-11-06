import * as opendash from "../../../../sdk/as/opendash";
import { JSON } from "assemblyscript-json/assembly";

export function opendash_run(): i32 {
  const inputStr = opendash.getInput();
  let text = "Hello from OpenDash Plugin";

  const parsedInput = JSON.parse(inputStr);
  if (parsedInput instanceof JSON.Obj) {
    const jsonObj = <JSON.Obj>parsedInput;
    if (jsonObj.has("text")) {
      const textVal = jsonObj.getString("text");
      if (textVal !== null) {
        text = textVal.valueOf();
      }
    }
  }

  // Đọc số lần chạy hiện tại từ SQLite KV store
  let runCount: i32 = 0;
  const currentCountStr = opendash.kvGet("run_count");
  if (currentCountStr !== null) {
    const parsed = JSON.parse(currentCountStr);
    if (parsed instanceof JSON.Integer) {
      runCount = i32((<JSON.Integer>parsed).valueOf());
    } else if (parsed instanceof JSON.Num) {
      runCount = i32((<JSON.Num>parsed).valueOf());
    } else {
      runCount = I32.parseInt(currentCountStr);
    }
  }
  runCount += 1;

  // Ghi lại số lần chạy mới vào SQLite KV store
  opendash.kvSet("run_count", runCount.toString());

  // Biến đổi chuỗi chữ hoa
  const transformed = text.toUpperCase();

  // Tạo JSON output
  const outObj = new JSON.Obj();
  outObj.set("status", "ok");
  outObj.set("transformed", transformed);
  outObj.set("run_count", runCount);

  const outJson = opendash.stringifyValue(outObj);
  const outRes = opendash.setOutput(outJson);
  if (outRes < 0) {
    return 1;
  }

  return 0; // 0 = Thành công
}
