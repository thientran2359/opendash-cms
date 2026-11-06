# Hợp đồng Plugin ABI v1 — OpenDash

**Phiên bản API**: `1`\
**Trạng thái**: `AGREED` (Thiết kế chuẩn hóa cho M2)\
**Tài liệu tham chiếu**: [docs/product/PRODUCT_REQUIREMENTS.md](../product/PRODUCT_REQUIREMENTS.md), [Wasmi 2.0.0 Documentation](https://docs.rs/wasmi/2.0.0/wasmi/)

---

## 1. Cấu trúc Gói Plugin ZIP

Gói ZIP cài đặt vào OpenDash phải tuân thủ nghiêm ngặt các giới hạn an toàn:

1. **Các file bắt buộc trong ZIP**:
   - `manifest.json`: Chứa thông tin định danh, phiên bản, ranh giới quyền và cấu hình plugin.
   - `plugin.wasm`: File nhị phân WebAssembly biên dịch cho kiến trúc 32-bit (`wasm32-unknown-unknown`).
2. **Quy tắc an toàn khi giải nén**:
   - Số lượng file tối đa trong ZIP: **10 files**.
   - Kích thước file ZIP tối đa: **5 MB** (`5 * 1024 * 1024` bytes).
   - Tổng dung lượng sau khi giải nén tối đa: **10 MB** (`10 * 1024 * 1024` bytes).
   - Dung lượng file `plugin.wasm` tối đa: **2 MB** (`2 * 1024 * 1024` bytes).
   - Tên file không được chứa đường dẫn tuyệt đối hoặc path traversal (`..`, `/`, `\`). Không chấp nhận symlink hoặc mục trùng lặp.
   - **Không thực thi code tại thời điểm cài đặt**; host chỉ kiểm tra tính hợp lệ của manifest và cú pháp của Wasm module (`wasmi::Module::new`).

---

## 2. Đặc tả `manifest.json`

```json
{
  "id": "my-plugin",
  "version": "1.0.0",
  "api_version": 1,
  "name": "Tên hiển thị của Plugin",
  "description": "Mô tả ngắn gọn về chức năng của plugin",
  "entrypoint": "plugin.wasm",
  "permissions": {
    "sqlite": ["kv_read", "kv_write"],
    "http": ["https://api.example.com", "http://127.0.0.1:8080"]
  }
}
```

### Các trường dữ liệu:
- `id` (string, bắt buộc): Định danh duy nhất của plugin, gồm 3 đến 64 ký tự chữ thường, số và dấu gạch ngang (`^[a-z0-9][a-z0-9-]{1,62}[a-z0-9]$`).
- `version` (string, bắt buộc): Chuỗi phiên bản tuân thủ SemVer (ví dụ: `"0.1.0"`, `"1.0.0"`).
- `api_version` (number, bắt buộc): Phiên bản ABI của host OpenDash. Ở M2, giá trị này **bắt buộc là 1**. Host sẽ từ chối cài đặt nếu `api_version != 1`.
- `name` (string, bắt buộc): Tên hiển thị người dùng (1–100 ký tự).
- `description` (string, tùy chọn): Mô tả plugin (tối đa 500 ký tự).
- `entrypoint` (string, bắt buộc): Tên file Wasm nhị phân, mặc định là `"plugin.wasm"`.
- `permissions` (object, tùy chọn):
  - `sqlite` (mảng string): Các quyền truy cập kho lưu trữ Key-Value cô lập của plugin. Giá trị hợp lệ: `"kv_read"`, `"kv_write"`.
  - `http` (mảng string): Danh sách các origin URL được phép gửi request mạng ra ngoài (ví dụ: `["https://api.weather.com"]`). Host chặn toàn bộ request đến các domain/port không nằm trong allowlist này.

---

## 3. WebAssembly Interface (ABI v1)

Giao tiếp giữa host (Rust/Wasmi) và guest (WebAssembly) diễn ra qua memory và các hàm xuất/nhập được định nghĩa rõ ràng. Host không đoán hay phụ thuộc vào bộ cấp phát bộ nhớ nội bộ của ngôn ngữ guest (AssemblyScript/Rust/C).

### 3.1. Guest Exports (Guest cung cấp cho Host)
Mọi plugin `.wasm` hợp lệ bắt buộc phải export hai thành phần sau:
1. `memory`: Linear memory WebAssembly chuẩn.
2. `opendash_run() -> i32`: Điểm thực thi chính khi một tác vụ plugin được gọi.
   - Giá trị trả về `0`: Tác vụ thành công.
   - Giá trị trả về khác `0`: Mã lỗi guest (ví dụ `1`: lỗi nghiệp vụ, `2`: input không hợp lệ).

### 3.2. Host Imports (Host cung cấp cho Guest trong module `"opendash_v1"`)

Mọi hàm host được đặt trong namespace `"opendash_v1"`:

#### 1. Đọc Input:
- `host_get_input_len() -> i32`:
  - Trả về độ dài theo bytes của chuỗi UTF-8 JSON đầu vào mà caller gửi tới plugin.
- `host_read_input(dest_ptr: i32, max_len: i32) -> i32`:
  - Sao chép tối đa `max_len` bytes từ payload đầu vào vào bộ nhớ guest tại địa chỉ `dest_ptr`.
  - Trả về số byte thực tế đã sao chép, hoặc số âm nếu có lỗi (ví dụ: con trỏ ngoài biên).

#### 2. Ghi Output:
- `host_write_output(src_ptr: i32, len: i32) -> i32`:
  - Guest gửi kết quả thực thi dạng chuỗi UTF-8 JSON từ địa chỉ `src_ptr` với độ dài `len` bytes cho host.
  - Host kiểm tra tính hợp lệ của con trỏ, độ dài, tính đúng đắn của chuỗi UTF-8 và cú pháp JSON.
  - Trả về `0` nếu thành công, hoặc số âm nếu thất bại.

#### 3. Thực thi Host Operations:
- `host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32`:
  - Guest gọi một thao tác hệ thống (SQLite KV hoặc HTTP) bằng cách truyền chuỗi JSON mô tả thao tác tại `[op_ptr .. op_ptr + op_len]`.
  - Host thực hiện thao tác (kiểm tra quyền manifest trước tiên), sau đó ghi chuỗi JSON kết quả vào `[resp_ptr .. resp_ptr + max_resp_len]`.
  - Trả về số byte thực tế đã ghi vào `resp_ptr`. Nếu buffer quá nhỏ để chứa kết quả, trả về mã lỗi âm `-2` (`BUFFER_TOO_SMALL`).

---

## 4. Định dạng Host Operations qua `host_call`

### 4.1. Thao tác Key-Value (`sqlite`)
Mỗi plugin sở hữu một namespace khóa riêng biệt, phân tách tuyệt đối trong bảng `plugin_kv(plugin_id, key, value_json, updated_at)`. Guest không được phép truy cập namespace của plugin khác hoặc gọi raw SQL.

#### `kv_get`:
- **Yêu cầu (Guest -> Host)**:
  ```json
  { "op": "kv_get", "key": "user_preference" }
  ```
- **Phản hồi (Host -> Guest)**:
  - Khi tìm thấy:
    ```json
    { "ok": true, "found": true, "value": { "theme": "dark" } }
    ```
  - Khi không tìm thấy:
    ```json
    { "ok": true, "found": false }
    ```
  - Khi thiếu quyền `"kv_read"`:
    ```json
    { "ok": false, "error": "permission_denied: sqlite:kv_read" }
    ```

#### `kv_set`:
- **Yêu cầu (Guest -> Host)**:
  ```json
  { "op": "kv_set", "key": "user_preference", "value": { "theme": "dark" } }
  ```
- **Phản hồi (Host -> Guest)**:
  - Khi ghi thành công:
    ```json
    { "ok": true }
    ```
  - Khi thiếu quyền `"kv_write"`:
    ```json
    { "ok": false, "error": "permission_denied: sqlite:kv_write" }
    ```

### 4.2. Thao tác HTTP Client (`http`)
Host kiểm tra URL đích dựa trên danh sách `permissions.http` khai báo trong `manifest.json`.

- **Yêu cầu (Guest -> Host)**:
  ```json
  {
    "op": "http_request",
    "method": "GET",
    "url": "https://api.example.com/data",
    "headers": { "Accept": "application/json" },
    "body": null
  }
  ```
- **Phản hồi (Host -> Guest)**:
  - Khi thành công:
    ```json
    {
      "ok": true,
      "status": 200,
      "headers": { "content-type": "application/json" },
      "body": "{\"result\": 42}"
    }
    ```
  - Khi URL không nằm trong allowlist:
    ```json
    { "ok": false, "error": "permission_denied: url not in http allowlist" }
    ```
  - Khi network timeout hoặc lỗi kết nối:
    ```json
    { "ok": false, "error": "http_error: connection timed out" }
    ```

---

## 5. Giới hạn Tài nguyên và Ranh giới Bảo vệ (Sandboxing)

Để ngăn chặn tấn công DoS, rò rỉ tài nguyên hoặc làm sập tiến trình máy chủ OpenDash:

| Tài nguyên | Giới hạn | Cơ chế kiểm soát |
|---|---|---|
| **Fuel (CPU)** | 20.000.000 units / invocation | Wasmi engine fuel metering; hết fuel lập tức trap `OutOfFuel`. |
| **Linear Memory** | 16 MB (256 pages Wasm) | Wasmi `StoreLimitsBuilder::memory_size(16 << 20)`. |
| **Upload Plugin ZIP** | 5 MB (`5 * 1024 * 1024` bytes) | `DefaultBodyLimit::max(5MB)` riêng trên route `POST /api/admin/plugins` (tách biệt khỏi giới hạn 64KB của theme). |
| **Input Payload JSON** | 1 MB (`1024 * 1024` bytes) | `DefaultBodyLimit::max(1MB)` riêng trên route `POST /api/admin/plugins/{id}/run` (tách biệt khỏi giới hạn 64KB của theme). |
| **Output Payload** | 1 MB (`1024 * 1024` bytes) | Host chặn nếu `host_write_output` vượt quá 1 MB. |
| **Host Calls** | 100 lần / invocation | Đếm số lần gọi `host_call` trong một lượt chạy. |
| **Invocation Deadline** | 5 giây / invocation | Tokio deadline kiểm soát tiến trình worker. |
| **HTTP Timeout** | 2 giây / request | Client HTTP giới hạn kết nối và đọc stream body tối đa 1 MB. |
| **Concurrency Pool** | 4 tác vụ plugin đồng thời | Tokio Semaphore fail-fast 1000ms (`ConcurrencyLimit`). |

### Tính toàn vẹn khi hủy bỏ, Cô lập Worker và Điểm Chuyển Giao Commit (Commit Handover Point):
- **Tiến trình Worker cô lập từ cùng Executable & Giám sát Chặt chẽ (Structured Supervision)**:
  - Mọi invocation được thực thi trong một tiến trình con cô lập sinh ra từ chính file thực thi máy chủ (`std::env::current_exe() --plugin-worker`), không phụ thuộc daemon hay runtime bên ngoài. Loại bỏ hoàn toàn nhánh fallback in-process khỏi môi trường production; nếu không thể spawn worker process, host lập tức trả về lỗi tường minh (`PluginError::Internal`).
  - Khi invocation timeout (5 giây) hoặc caller hủy yêu cầu (client disconnect / task abort), `ProcessGuard::cleanup` thực hiện quy trình dọn dẹp đồng bộ nghiêm ngặt:
    - Gửi tín hiệu hủy qua `watch::channel` và `AtomicBool` để lập tức hủy bỏ mọi truy vấn IPC `kv_get` đang chờ lấy kết nối từ SQLite pool trong tiến trình cha.
    - Gọi `child.kill()`, kích hoạt cơ chế `TerminateProcess` của kernel Windows để dừng ngay lập tức mọi luồng worker con (kể cả luồng DNS blocking `getaddrinfo` hay socket I/O).
    - Đồng bộ gặt tiến trình con bằng `child.wait()` và join luồng cha giám sát (`handle.join()`), triệt tiêu hoàn toàn nguy cơ tác vụ cha bị tách rời (detached task) chạy ngầm chiếm dụng tài nguyên.
    - Chỉ sau khi cả tiến trình con và luồng cha đã dọn dẹp xong 100%, permit của semaphore mới được giải phóng và hoàn trả đầy đủ cả 4 slots điều phối. Server chính và giao diện Admin/public luôn được bảo đảm đáp ứng trơn tru.
- **Ranh giới Transaction SQLite, Guard Loại bỏ Kết nối Dở dang và Điểm Quyết định Commit (Commit Handover Point)**:
  - Tiến trình worker không kết nối trực tiếp SQLite; các thao tác `kv_set` được stage trong bộ nhớ worker và chỉ gửi về tiến trình cha khi guest hoàn thành thành công.
  - **Khôi phục cấu hình động và Guard Loại bỏ Kết nối (`StagedCommitGuard`)**:
    - Ngay sau khi acquire connection, host truy vấn cấu hình chờ ban đầu (`initial_busy_timeout = PRAGMA busy_timeout`) để khôi phục chính xác thay vì hardcode giá trị cố định, tương thích với mọi cấu hình pool khác nhau.
    - Ngân sách chờ acquire, `BEGIN IMMEDIATE`, và từng câu `INSERT` được tính toán độc lập theo thời gian còn lại của deadline (`PRAGMA busy_timeout = budget_ms`).
    - Kết nối được bảo vệ bởi `StagedCommitGuard`: nếu caller abort hoặc future bị hủy bất ngờ trong khi transaction đang mở hoặc đang ghi dở dang, guard lập tức kích hoạt `conn.close_on_drop()`. Kết nối dở dang bị đóng và loại bỏ hoàn toàn khỏi pool (evicted), ngăn chặn tuyệt đối tình trạng rò rỉ transaction mở (`cannot start a transaction within a transaction`) hay timeout bị sai lệch sang các caller khác. Khi caller tiếp theo gọi pool, pool sẽ khởi tạo kết nối mới hoàn toàn sạch sẽ.
  - **Điểm chuyển giao commit (Commit Handover Point)**:
    - *Trước khi gửi lệnh `COMMIT`*: Nếu deadline đã hết hoặc yêu cầu bị hủy bỏ, host lập tức thực hiện `ROLLBACK`, khôi phục `PRAGMA busy_timeout = initial_busy_timeout`, xác nhận dọn dẹp hoàn tất (`count == 0`), và trả về `PluginError::Timeout`.
    - *Khi lệnh `COMMIT` đã được gửi tới SQLite và thực thi thành công*: Giao dịch được coi là đã ghi nhận vĩnh viễn trên đĩa (committed). Cấu hình ban đầu được khôi phục, cờ `committed = true` được xác nhận để trả kết nối sạch về pool, và host luôn trả về kết quả thành công `Ok(output_val)` tương ứng với dữ liệu đã commit; tuyệt đối KHÔNG BAO GIỜ báo `Timeout` cho một giao dịch đã commit thành công xuống cơ sở dữ liệu.
    - Nếu câu lệnh `COMMIT` gặp lỗi khóa hoặc timeout, transaction được rollback sạch sẽ và host trả về `PluginError::Timeout`. Tuyệt đối không có ghi muộn (zero late writes) sau khi timeout đã báo về caller, kể cả trong chế độ rollback-journal.

### Ranh giới an toàn Mạng & Gói cài đặt:
- **HTTP SSRF & DNS Pinning**: Mọi URL ngoài allowlist bị chặn. Với URL hợp lệ, host phân giải DNS và kiểm tra toàn bộ địa chỉ IP v4/v6 (chặn loopback, private 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, carrier NAT 100.64.0.0/10, link-local, multicast, documentation, unspecified). Địa chỉ hợp lệ được pin trực tiếp vào client (`resolve()`) tránh DNS rebinding; bật `no_proxy()`, không tự động redirect (`Policy::none()`), stream body giới hạn cứng `take(1MB + 1)`, và thông báo lỗi không làm lộ URL credentials/query params.
- **Xác thực ZIP & ABI**: Chỉ chấp nhận các file gốc hợp lệ (`manifest.json`, `plugin.wasm`, `README.md`), từ chối thư mục con, dấu `/`, dấu `\`, symlink Unix. Đếm bytes giải nén thực tế tích lũy (tối đa 10 MB); giới hạn `manifest.json` tối đa 64 KB, `plugin.wasm` tối đa 2 MB; kiểm tra SemVer 2.0.0 cho trường `version`.
- **Kiểm định Chữ ký Wasm lúc Cài đặt**: Host kiểm tra cấu trúc module Wasm qua `wasmi::Module`: export `memory`, export `opendash_run() -> i32`; chỉ cho phép import từ namespace `"opendash_v1"` với đúng 4 chữ ký quy chuẩn hoặc `"env:abort(i32,i32,i32,i32)"`. Từ chối toàn bộ namespace ngoài (WASI, unknown env). Khi guest gọi `env:abort`, host ngắt fuel và trap có kiểm soát.
