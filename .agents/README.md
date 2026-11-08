# Cơ chế Giao việc & Báo hiệu Tự động Codex ↔ Antigravity (`antigravity-handoff`)

> **Phạm vi nền tảng:** Các script PowerShell, hook `powershell.exe` và Windows `EventWaitHandle` dưới đây dành cho môi trường Windows. Trên macOS, xem [quy trình điều phối và giới hạn handoff](../docs/discussions/AGENT_COORDINATION.md) trước khi giao việc; chưa có cơ chế báo hoàn thành tự động tương đương được kiểm chứng.

Thư mục `.agents/` chứa **Workspace Skill** `antigravity-handoff`, cấu hình **Lifecycle Hook** của Antigravity và bộ script đồng bộ giúp **Codex** giao việc cho **Antigravity** chỉ với 1 dòng lệnh và nhận lại ngay báo cáo kết quả khi Antigravity làm xong mà **không cần quét (poll) file liên tục** (thư mục con `.agents/sync/` chứa dữ liệu tạm đã được ignore trong `.gitignore`).

---

## 1. Cấu trúc thư mục `.agents/`

| Đường dẫn | Vai trò |
| :--- | :--- |
| `skills/antigravity-handoff/SKILL.md` | Skill chuẩn hoá quy trình giao việc (Handoff) cho cả Codex và Antigravity. |
| `hooks.json` | Đăng ký sự kiện `Stop` của Antigravity để tự động gọi `notify-codex.ps1` ngay khi Antigravity kết thúc lượt thực thi. |
| `prepare-task.ps1` | Script cho Codex: xoá trạng thái cũ, reset `EventWaitHandle` và tạo file `.agents/sync/task-for-ag.md`. |
| `notify-codex.ps1` | Được gọi bởi `Stop` hook khi `fullyIdle: true`: tự động thu thập `.agents/sync/result-from-ag.md`, trích xuất câu trả lời cuối cùng từ `transcript.jsonl`, chụp `git status --short`, ghi vào `.agents/sync/ag-done.json` và bắn tín hiệu Kernel Event (`Local\OpenDash_Antigravity_Done`). |
| `wait-for-ag.ps1` | Script cho Codex: ngủ chờ sự kiện hoàn thành (0% CPU polling, độ trễ ~0ms) và in ra toàn bộ JSON kết quả ngay khi Antigravity dừng. |
| `sync/task-for-ag.md` | File chứa nội dung task do Codex chuẩn bị cho Antigravity. |
| `sync/result-from-ag.md` | File chứa báo cáo kết quả của Antigravity sau khi thực thi xong. |
| `sync/ag-done.json` | File JSON tổng hợp trạng thái hoàn thành, `resultSummary`, `lastResponse` và `gitStatusShort`. |

---

## 2. Quy trình 3 bước dành cho Codex

### Bước 1: Chuẩn bị Task (và tự động Clear cờ cũ)
Trước khi gửi yêu cầu sang web remote của Antigravity, Codex chạy:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\.agents\prepare-task.ps1 `
  -Title "Tên task ngắn gọn" `
  -TaskText "Nội dung chi tiết yêu cầu..."
```
*(Hoặc truyền `-FromFile đường_dẫn_file.md` nếu nội dung dài).*

### Bước 2: Gửi đúng 1 câu ngắn trên Web Remote
Trên giao diện web của Antigravity, Codex chỉ cần gửi:
```text
Thực hiện task trong .agents/sync/task-for-ag.md
```
*(Không lo lỗi xuống dòng, dấu nháy hay giới hạn độ dài).*

### Bước 3: Chờ kết quả trả về tự động (One-Shot Event Wait)
Ngay sau khi bấm gửi trên web, Codex chạy:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\.agents\wait-for-ag.ps1 -TimeoutSeconds 900
```
Ngay giây Antigravity hoàn thành, lệnh thoát với exit code `0` và trả về JSON có sẵn:
- `resultSummary`: Báo cáo kết quả từ `.agents/sync/result-from-ag.md`
- `lastResponse`: Nguyên văn câu trả lời cuối cùng của Antigravity trong chat
- `gitStatusShort`: Danh sách các file vừa thay đổi (`git status --short`)

---

## 3. Lưu ý an toàn & phạm vi

* Chỉ dùng `wait-for-ag.ps1` như một **one-shot waiter** cho từng lượt giao việc hữu hạn. Không bọc trong vòng lặp vô hạn tự động gọi qua lại nếu chưa có chỉ đạo của Chủ dự án (tuân thủ `AGENTS.md` và `docs/discussions/PROTOCOL.md`). Source code và scripts ở `cms/`, tài liệu ở `docs/`; các script handoff giữ tại `.agents/` ở gốc.
* Thư mục `/.agents/sync/` đã nằm trong `.gitignore`; không `git add -f` các file trạng thái tạm trong `.agents/sync/`.
