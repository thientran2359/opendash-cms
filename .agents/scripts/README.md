# Công cụ hỗ trợ agent

- `bootstrap-worktree.py`: xem trước thao tác tạo worktree/branch riêng cho
  Atlas, Nova, Forge, Harbor và Orion; chỉ thực hiện thay đổi khi thêm
  `--apply`, không tự push hoặc merge.
- `check-agent-artifacts.py`: kiểm tra file đang có trong vùng `.agents/` để
  phát hiện hook/skill đã retired còn active hoặc artifact/secret rõ ràng.

Chạy trên từng checkout bằng Python 3. Các adapter Muse → CoS/Antigravity
chưa được kiểm chứng nên **không có cơ chế tự gửi task đang bật**.
