# Công cụ hỗ trợ agent

- `bootstrap-worktree.py`: xem trước thao tác tạo worktree/branch riêng cho
  Atlas, Nova, Forge, Harbor và Orion; chỉ thực hiện thay đổi khi thêm
  `--apply`, không tự push hoặc merge.
- `check-agent-artifacts.py`: kiểm tra file đang có trong vùng `.agents/` để
  phát hiện hook/skill đã retired còn active hoặc artifact/secret rõ ràng.
- `prepare-github-issue.py`: đọc đặc tả task G3 READY từ **Git blob tại
  `origin/main`** (hoặc commit `--ref` đã duyệt), validate gate/nội dung và
  xuất JSON `title`/`body`/`labels` có mô tả đầy đủ cho Muse/GitHub API.
  Không tạo Issue, không ghi file hoặc gửi task; phải kiểm tra trùng Task ID
  ở GitHub trước khi publish. Ví dụ khi đã có task hợp lệ:

  ```shell
  python .agents/scripts/prepare-github-issue.py --source docs/plans/modules/CNT-03-taxonomy.md --task-id M3-S07-T01 --agent forge --format json
  python -m unittest discover -s .agents/scripts -p 'test_prepare_github_issue.py' -v
  ```

  **Lưu ý:** đường dẫn/task ID chỉ là minh họa; mã hiện có CNT-01/CNT-05
  ghi các task G3 cũ là ứng viên, công cụ phải từ chối.

Chạy trên từng checkout bằng Python 3. Các adapter Muse → CoS/Antigravity
chưa được kiểm chứng nên **không có cơ chế tự gửi task đang bật**.
