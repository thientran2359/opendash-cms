# CodeGraph cho OpenDash CMS — kiểm chứng cài đặt 07/11/2026

**Trạng thái:** CodeGraph engine và chỉ mục code/docs `VERIFIED`; cấu hình MCP Codex `REGISTERED`; cấu hình MCP Antigravity `REGISTERED`; Chat On Steroids Plugins **chưa kết nối CodeGraph** (cần thao tác trong ứng dụng).

## Cài đặt đã thực hiện trên Windows

- Phiên bản: `@astudioplus/codegraph-mcp@0.21.0`, engine Windows x64 `codegraph-server 0.21.0`, Node.js `v22.16.0`.
- Cài bằng `npm install --global @astudioplus/codegraph-mcp@0.21.0`. Package tải engine và static model về máy. `--embedding-model static` sử dụng model `jina-code-static-256` cục bộ, không đòi hỏi API key.
- Workspace: thư mục gốc OpenDash, `cms/` chứa code và SDK; loại `database/`, `uploads/` khỏi index. Các thư mục build (`target`, `node_modules`) được CodeGraph loại mặc định. Đặt `CODEGRAPH_TELEMETRY=off` trong cấu hình từng client.
- Lệnh đã chạy từ gốc repo để lập chỉ mục và thử entry points: engine `--workspace <repo> --embedding-model static --exclude uploads --exclude database --run-tool codegraph_find_entry_points`.
- Kết quả lần đầu: **61 file parse**, **711 node**, **1.607 cạnh**, **365 symbol vectors**. Graph lưu tại `~/.codegraph/projects/opendash-cms-fc1b/` trên máy thử. Lần chạy sau tải được vector đã lưu và chỉ cập nhật file thay đổi.

## Kiểm chứng truy vấn quan hệ

- `codegraph_symbol_search` với `HookDispatcher` trả về lớp `HookDispatcher` trong `cms/crates/opendash-server/src/hooks.rs` và các symbol liên quan trong `opendash-core/src/hooks.rs`.
- `codegraph_get_call_graph` trên `cms/crates/opendash-server/src/plugin/host_ops.rs`, `handle_host_call` (dòng 94), chiều sâu 2, trả về chuỗi caller từ `plugin/runtime.rs` (`register_host_imports`, `execute_wasm_sync`) và `plugin/worker.rs` (`run_worker_from_stdio`), cùng nhiều callee trong `host_ops.rs`.
- `node cms/scripts/codegraph-query.mjs symbol HookDispatcher` trả về 10 kết quả (đầu tiên là `HookDispatcher`); `node cms/scripts/codegraph-query.mjs callgraph cms/crates/opendash-server/src/plugin/host_ops.rs 94` trả về 20 edges và 20 nodes; truy vấn `docs 'plugin ABI'` trả về JSON có `query`, `results`, `total`.
- Kiểm thử trực tiếp MCP stdio với `codegraph-mcp.js`: JSON-RPC `initialize` thành công; `tools/list` trả về **42 tools**, có `codegraph_symbol_search` và `codegraph_get_call_graph`. Đây là kiểm chứng giao thức, chưa xác nhận từng ứng dụng AI đã nạp lại plugin.
- Một cảnh báo parser: cú pháp `@external(...)` của AssemblyScript tại `cms/sdk/as/opendash.ts:86` gây lỗi tree-sitter. CodeGraph tiếp tục trích xuất các symbol khác trong file; kết quả ở đây chỉ là phân tích tĩnh, có thể thiếu các lời gọi qua hook, macro hoặc runtime Wasm.

## Tài liệu đã nạp vào CodeGraph docs

`codegraph_index_markdown` và `codegraph_list_doc_sources` xác nhận đủ sáu nguồn:

1. [Yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md)
2. [Tech stack](../architecture/TECH_STACK.md)
3. [Kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md)
4. [Core Foundation](../contracts/CORE_FOUNDATION.md)
5. [Plugin ABI](../contracts/PLUGIN_ABI.md)
6. [Bằng chứng import M1/M2](2026-11-07-m1-m2-import.md)

Sau khi cập nhật một trong sáu tài liệu, chạy `node cms/scripts/codegraph-index-docs.mjs` để nạp lại. Chỉ mục docs không thay thế tài liệu nguồn.

## Kết nối các agent

### Codex

Đã đăng ký server `codegraph` qua `codex mcp add`, dùng `node.exe` chạy `codegraph-mcp.js` kèm `--workspace <repo> --embedding-model static --exclude database --exclude uploads`. `codex mcp get codegraph` xác nhận `enabled: true`, `transport: stdio`. Mở phiên Codex mới để nhận danh sách MCP tools mới.

### Antigravity

Đã thêm server `codegraph` vào `%USERPROFILE%/.gemini/config/mcp_config.json`, giữ nguyên server `stitch` có trước. Cấu hình dùng cùng engine, workspace và static model với Codex. Trong Antigravity mở Settings → Customizations → Installed MCP Servers, làm mới/restart MCP để dùng; còn cần kiểm tra kết nối thực tế từ giao diện Antigravity.

### Chat On Steroids Plugins

CodeGraph đã cài trên máy và sẵn sàng cho CoS, nhưng connector `Chat On Steroids Plugins` hiện không cung cấp thao tác cài thêm MCP server từ chat. Trong ứng dụng **Chat On Steroids → Plugins → + → Custom executable**, thêm:

- **Name:** `CodeGraph OpenDash`
- **Command / executable:** đường dẫn đầy đủ tới `C:\Program Files\nodejs\node.exe` (hoặc kết quả `Get-Command node`)
- **Arguments** (mỗi mục một đối số): đường dẫn đầy đủ tới `%APPDATA%\npm\node_modules\@astudioplus\codegraph-mcp\bin\codegraph-mcp.js`, `--workspace`, đường dẫn Windows đầy đủ tới root `opendash-cms`, `--embedding-model`, `static`, `--exclude`, `database`, `--exclude`, `uploads`
- **Environment** (nếu có): `CODEGRAPH_TELEMETRY=off`

Sau khi plugin báo **Ready**, làm mới `Chat On Steroids Plugins` trong ChatGPT để các tool `codegraph_*` hiển thị. Không coi việc package đã cài trên máy là bằng chứng connector ChatGPT đã nhận tools.

Trước khi hoàn tất bước giao diện này, Chat On Steroids **Core** vẫn có thể gọi CodeGraph qua `exec_command` với các lệnh được kiểm thử tại root:

```powershell
node cms/scripts/codegraph-query.mjs symbol HookDispatcher
node cms/scripts/codegraph-query.mjs callgraph cms/crates/opendash-server/src/plugin/host_ops.rs 94
node cms/scripts/codegraph-query.mjs docs "plugin ABI"
```

## Giới hạn và cách kiểm tra lại

- Static graph mang tính heuristic và phụ thuộc cú pháp tree-sitter, cần đối chiếu code khi đánh giá thay đổi có rủi ro. Không coi design được index là đã implemented/verified.
- Chạy `codex mcp get codegraph` để kiểm tra cấu hình Codex; chạy `node cms/scripts/codegraph-index-docs.mjs` để xác minh kho docs. Kiểm tra trạng thái **Ready** trong Antigravity và Chat On Steroids sau khi làm mới từng ứng dụng.
- Đã chạy `node --check` trên hai script và `git diff --check`: đều trả về exit code `0`. Chưa chạy build/test CMS vì thay đổi chỉ liên quan cấu hình và công cụ tra cứu mã nguồn.
- Không sửa nội dung M1/M2, không build lại CMS hoặc thay đổi dữ liệu website trong tác vụ thiết lập CodeGraph.
