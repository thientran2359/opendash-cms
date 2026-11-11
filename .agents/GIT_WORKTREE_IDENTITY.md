# Git identity theo từng worktree cho 5 agent OpenDash

**Quyết định vận hành ngày 2026-11-11:** giữ nguyên Git global của người dùng
trên máy Windows/macOS; từng agent cấu hình `user.name` cho **worktree thực
sự đang dùng**, không sửa danh tính Git mặc định của các repository khác.
Đây là nhận diện **commit author/committer** để truy vết, **không tạo tài
khoản GitHub độc lập**: người push/review trên GitHub vẫn là account đã xác
thực. Xem [GitHub workflow](GITHUB_WORKFLOW.md#danh-tính-5-agent-trên-một-tài-khoản-github)
và [agents.yaml](agents.yaml).

| Agent | Máy/công cụ | `user.name` kỳ vọng |
| --- | --- | --- |
| Atlas | Codex Windows | `Atlas` |
| Nova | Antigravity Windows | `Nova` |
| Forge | CoS Windows | `Forge` |
| Harbor | CoS macOS | `Harbor` |
| Orion | Antigravity macOS | `Orion` |

## Quy trình cho **mỗi** agent (chạy ngay trong worktree của mình)

1. Xác minh `pwd` (hoặc `Get-Location` trên PowerShell), `git status
   --short --branch`, `git rev-parse --show-toplevel`,
   `git rev-parse --git-common-dir`, `git worktree list` và
   `git --version`; kiểm tra đúng repo `thientran2359/opendash-cms`.
   Không chạy trên repository hoặc worktree của dự án khác.
2. Ghi nhận baseline, **không thay đổi** các giá trị sau:

   ```bash
   git config --global --get user.name
   git config --global --get user.email
   git config --show-origin --show-scope --get-all user.name
   git config --show-origin --show-scope --get-all user.email
   git config --local --get extensions.worktreeConfig
   git config --local --get core.worktree
   git config --local --get core.bare
   ```

   Git có thể trả exit code 1 khi key chưa được đặt; đó không phải lỗi
   trong bước đọc baseline. Nếu `core.worktree`/`core.bare` hoặc
   `includeIf`/local config gây mơ hồ, **dừng và báo** thay vì tự di chuyển
   các giá trị dùng chung. Đặc biệt không bật extension mù trên repo bare.
3. Với Git hỗ trợ `worktreeConfig` và shared config bình thường, bật extension
   **một lần trên Git database hiện tại**, rồi ghi tên đúng worktree:

   ```bash
   git config --local extensions.worktreeConfig true
   git config --worktree user.name "Forge"
   ```

   **Thay `Forge` bằng tên agent của bạn.** `--local` chỉ dùng để bật
   `extensions.worktreeConfig` trong shared repository, **không** dùng cho
   `user.name` hay `user.email`. Nếu extension đã `true`, không cần bật
   lần nữa. Git linked worktree sẽ ghi `user.name` trong
   `config.worktree` của chính worktree; các checkout Git độc lập có config
   của riêng mình. Không cấu hình các worktree khác thay mặt agent khác.
4. Kiểm lại ngay:

   ```bash
   git config --get user.name
   git config --worktree --get user.name
   git config --show-origin --show-scope --get user.name
   git config --global --get user.name
   git config --get user.email
   ```

   Giá trị hiệu lực và worktree phải đúng tên agent, `origin` của
   `user.name` phải là `config.worktree`, còn global phải **giữ nguyên**
   như bước 2. Email nên kế thừa từ global **đã được xác minh với tài khoản
   GitHub**; nếu email hiện tại thiếu/sai thì hỏi người dùng, không tự bịa
   địa chỉ. Có thể kiểm thêm một worktree khác và một repo ngoài OpenDash
   chỉ bằng lệnh đọc để xác nhận chúng không đổi.
5. Giữ commit title gắn Task ID và trailer `Agent: <Agent>`, ví dụ:

   ```bash
   git commit -m "M3-Sxx-Txx: thực hiện task" -m "Agent: Forge"
   git log -1 --format='%h %an <%ae> | %cn <%ce>%n%B'
   ```

   Ví dụ commit **không phải** yêu cầu tạo commit mới chỉ để thử config.
   Chỉ commit khi thực sự có file thuộc nhiệm vụ đã được giao. Mỗi worktree
   mới do task/agent tạo phải tự kiểm tra và thiết lập danh tính của nó.

## Giới hạn, phương án dự phòng và hoàn tác

- **Không bao giờ** chạy `git config --global user.name "Forge"` (hoặc tên
  agent khác): điều đó làm thay đổi mặc định của tất cả repo trên tài khoản
  hệ điều hành. Không dùng `git config --local user.name` khi Git database
  đang dùng chung giữa nhiều worktree. Không chỉnh `~/.gitconfig` để bật
  `worktreeConfig`; extension chỉ thuộc repository OpenDash.
- Biến môi trường `GIT_AUTHOR_NAME`/`GIT_COMMITTER_NAME` hoặc
  `git -c user.name=...` trong lệnh riêng có thể ghi đè tên hiệu lực; nếu
  `git log` không đúng hãy kiểm tra invocation trước khi tiếp tục.
- Nếu chưa thể bật `worktreeConfig` an toàn, **không ép thay đổi config**;
  dùng `git -c user.name="Forge" commit -m "..." -m "Agent: Forge"` cho
  mỗi commit của agent và nêu rõ đang dùng fallback. Không được sửa email
  hay đổi global để xử lý lỗi.
- Hoàn tác tên **chỉ trên worktree hiện tại**:

  ```bash
  git config --worktree --unset user.name
  git config --get user.name  # trở về global hoặc config có độ ưu tiên thấp hơn
  ```

  **Không tự tắt `extensions.worktreeConfig` sau khi bật**: các worktree
  khác có thể đang phụ thuộc vào cấu hình riêng này. Mọi thay đổi config
  Git trên máy mỗi agent là **cục bộ**, không được push theo commit/PR;
  tài liệu này là thứ được đồng bộ từ GitHub.

## Báo cáo khi thiết lập

Agent ghi trong chat hoặc Issue phù hợp: OS, repo/worktree path, branch,
`git --version`, tên hiệu lực, nguồn `config.worktree`, global name
trước/sau (không đổi), email có xác minh hay không, kết luận `PASS` hoặc
`BLOCKED`. Không đăng credential/token hoặc nội dung private key.
