# Công cụ hỗ trợ agent

- `bootstrap-worktree.py`: xem trước thao tác tạo worktree/branch riêng cho
  Atlas, Nova, Forge, Harbor và Orion; chỉ thực hiện thay đổi khi thêm
  `--apply`, không tự push hoặc merge.
- `check-agent-artifacts.py`: kiểm tra file đang có trong vùng `.agents/` để
  phát hiện hook/skill đã retired còn active hoặc artifact/secret rõ ràng.
- `prepare-github-issue.py`: đọc Design Spec EGM (`Status: Approved` trong
  `docs/designs/<feature>.md`, hỗ trợ tách `--slice S01`) hoặc nguồn G3 lịch sử
  từ **Git blob tại `origin/main`** (hoặc commit `--ref` đã duyệt), validate
  gate/nội dung và xuất JSON `title`/`body`/`labels` cho GitHub API.
  Không tạo Issue, không ghi file hoặc gửi task; phải kiểm tra trùng Task ID
  ở GitHub trước khi publish. Ví dụ:

  ```shell
  python .agents/scripts/prepare-github-issue.py --source docs/designs/<feature>.md --agent forge --format json
  python .agents/scripts/prepare-github-issue.py --source docs/designs/<feature>.md --slice S01 --agent forge --format json
  python -m unittest discover -s .agents/scripts -p 'test_prepare_github_issue.py' -v
  ```
- `cos_dispatch.py`: adapter guarded một lần (one-shot + resume theo ACK) để
  Atlas/Aster giao Issue (`EGM-<slug>` hoặc `M*-S*-T*` lịch sử) và sửa PR tài
  liệu cho Forge/Harbor qua Chat On Steroids.

Chạy trên từng checkout bằng Python 3. Kiểm chứng unit tests bằng:

```shell
python -m unittest discover -s .agents/scripts -p 'test_cos_*.py' -v
```
