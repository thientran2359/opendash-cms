# G5 — GitHub Management (09/11/2026)

**Phạm vi:** GitHub Issue Forms, Project, Labels/Milestones, bảo vệ `main`,
và CI cho tài liệu và task handoff. Không phải kiểm thử CMS, Muse dispatch,
Cloudflare Tunnel hoặc nghiệm thu M3.

**Môi trường:** Windows với Git, Python 3.13.4 và GitHub browser đã đăng nhập.
Baseline `main@b39bfd39db88499eb89d6598e24b52f9f33e9982` (PR
[#2](https://github.com/thientran2359/opendash-cms/pull/2) đã merge).

## Kiểm tra trực tiếp

| Hạng mục | Kết quả | Evidence và giới hạn |
| --- | --- | --- |
| Issue Forms | **PASS** | [Issue chooser](https://github.com/thientran2359/opendash-cms/issues/new/choose) hiện hai form G3 Implementation và G1/G2 Research/Design. Đã mở Implementation, thấy các trường bắt buộc; không tạo Issue giả. |
| Issue và Project | **PASS** | [Project #3 / Team items](https://github.com/users/thientran2359/projects/3/views/3) chứa Issue #1 với label và `In review`. |
| Merge không tự đóng Issue | **PASS (quan sát trạng thái)** | PR #2 đã merge nhưng [Issue #1](https://github.com/thientran2359/opendash-cms/issues/1) vẫn open / `In review`; không tạo PR giả để kiểm thử event. |
| Labels/Milestones | **PASS (cấu hình)** | [9 milestone](https://github.com/thientran2359/opendash-cms/milestones) gồm M2.5; [module/type/agent labels](https://github.com/thientran2359/opendash-cms/labels) đã có. |
| Branch Ruleset | **PASS (cấu hình)** | [Ruleset #26833341](https://github.com/thientran2359/opendash-cms/settings/rules/26833341) Active, target default branch: yêu cầu PR, chặn xóa và force-push; 0 required approvals. Đã bật hai required status checks đúng tên và nguồn GitHub Actions sau khi CI PASS, rồi reload xác nhận. Không thử force-push hoặc xóa nhánh thật. |
| GitHub Actions CI mới | **PASS — PR #3, commit `fff367e`** | [Run #42572931323](https://github.com/thientran2359/opendash-cms/actions/runs/42572931323): `governance-ubuntu-latest` và `governance-windows-latest` đều hoàn thành `success`; mọi bước test/check trong cả hai job PASS. |
| Muse tạo task / dispatch | **NOT_RUN** | Chưa cài adapter, không tạo task G3 giả. |

## Các lệnh thực chạy tại Windows

| Lệnh | Kết quả |
| --- | --- |
| `python -m unittest discover -s .agents/scripts -p test_prepare_github_issue.py -v` | **PASS** — 9 tests (gate, ID, Git blob, milestone mapping) |
| `python cms/scripts/check-plans.py` | **PASS** — links/module IDs/evidence |
| `python cms/scripts/check-plans.py --self-test` | **PASS** — 8 fixtures |
| `python .agents/scripts/check-agent-artifacts.py` | **PASS** — phát hiện artifact/secret rõ ràng trong `.agents` |
| `git diff --check` | **PASS** — whitespace trong working tree |

## Kết luận và giới hạn

- CI đã thực chạy tại GitHub trên PR #3 (commit `fff367e`) và hai job PASS;
  Ruleset đã bắt buộc hai check này. Nếu có thêm commit, phải đối chiếu run mới
  trên HEAD trước khi merge; không suy mọi commit tương lai sẽ PASS.
- Issue Forms không tự kiểm tra G3 READY khi người dùng nhập tay; gate thuộc
  `.agents/scripts/prepare-github-issue.py` và quy trình review.
- `MERGED` không đồng nghĩa `VERIFIED`; không tự đóng Issue #1 hoặc chuyển
  Project Done khi chưa đủ review và evidence đúng phạm vi.
- Không deploy, smoke CMS hoặc kết nối Muse/CoS trong lượt G5 này.
