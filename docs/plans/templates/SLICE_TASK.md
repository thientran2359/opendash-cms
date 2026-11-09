# <TASK-ID> — <Mục tiêu có thể nghiệm thu>

**Cách dùng:** mẫu viết một task **sau G3** để giao qua GitHub Issue (bản nháp nếu cần nằm ở `.agents/tasks/`) hoặc đoạn task trong Living Module Brief. Không tạo file Markdown riêng cho từng task chỉ để đủ thủ tục; G1/G2 không phải task code. Với task đặt trong module brief, dùng `### M3-Sxx-Txx — Tên task`, các heading con `####`; với file task riêng dùng `#` và `##`. Chỉ task được đánh dấu `**G3:** READY` trong tài liệu đã duyệt trên `main` mới được công cụ Muse chuẩn bị Issue implementation.

- **Thuộc:** Release ..., Milestone ..., Slice ..., Module ID / Capability ID ...
- **Primary Acceptance Slice của capability:** ... (một slice nghiệm thu chính, có thể được regression lại về sau)
- **Module:** CNT-xx
- **Capability:** CNT-xx.1
- **Track:** Full-Track / Fast-Track
- **G2:** PROPOSED / AGREED / NOT_APPLICABLE (chỉ Fast-Track, ghi lý do)
- **Lý do Fast-Track:** ... (chỉ bắt buộc nếu G2 `NOT_APPLICABLE`)
- **G3:** DRAFT / READY (cần có quyết định G3 chính thức và review hiện trạng trước READY)
- **Trạng thái:** PLANNED / READY / IN_PROGRESS / IMPLEMENTED / VERIFIED / BLOCKED.
- **Nguồn quyết định:** PRD ..., contract ..., ADR ..., module brief ..., predecessor ...
- **Người triển khai / review:** ...

## Tổng quan và giá trị

Mô tả bằng lời dễ hiểu cho người quản lý: khó khăn hiện tại, vì sao cần tính năng này và người dùng CMS được lợi gì. Không dùng riêng Task ID hoặc thuật ngữ nội bộ để thay cho mô tả.

## Mục tiêu và hành vi quan sát được

Từ tình huống ... người dùng làm ... hệ thống phải trả kết quả ... sau khi restart/đổi provider/401 (nếu liên quan).

## Phạm vi công việc

- Việc 1: ...
- Việc 2: ...

## Ngoài phạm vi

- Không tự mở rộng sang: ...

## Phụ thuộc và điều kiện chặn

- Dependency đã có/chưa có: ... (nếu không có, ghi rõ "Không có")

## Kỹ thuật và file ownership

- Đầu vào hoặc API đã có: ...
- Danh sách **file người thực thi được phép sửa trong lượt này** (kiểm tra source trước khi bắt đầu): ...
- File/hợp đồng do Codex hoặc agent khác sở hữu trong lượt, không được sửa đồng thời: ...
- Contract/API có liên quan: ...

## Cách triển khai theo contract

API lõi phải tái sử dụng, các thao tác storage, transaction, quyền, UI và backward compatibility; ghi phương án khi phát hiện thiết kế cần đổi. Không tự tạo API tương đương với API lõi.

**Stop-and-Propose:** nếu phát sinh thay đổi PRD, contract công khai, migration đã chạy, security/ABI hoặc phá G2 thì tạm dừng tại ranh giới, báo blocker và tối thiểu hai phương án; không tự chốt G2. Tinh chỉnh nội bộ không phá contract phải báo `Design Delta` và test để review.

## Nghiệm thu

- [ ] ...
- [ ] ...
- [ ] Có kiểm tra lỗi và quyền đối với đường có rủi ro.
- [ ] Bằng chứng thực chạy cho luồng chính, có ghi lệnh, commit/branch, môi trường và phạm vi.
- [ ] Không làm hỏng các hành vi M1/M2 hoặc dữ liệu cũ liên quan.

## Kiểm thử, rủi ro và rollback

- Lệnh test, các negative cases và môi trường thực hiện: ...
- Rủi ro với dữ liệu/permission/rollback: ...
- Với migration đã chạy, chỉ dùng forward migration, kiểm tra trên bản copy DB.

## Bàn giao và review

- Diff/đường dẫn thay đổi: ...
- Kiểm tra đã chạy, kết quả, giới hạn: ...
- Rủi ro, backup/rollback/migration (nếu có): ... (Git revert không rollback DB đã migrate; test bằng DB copy).
- Review phát hiện và xử lý: ...
- Link báo cáo verification: ...

**Quy tắc:** chỉ đánh VERIFIED cho đúng phạm vi có kết quả thực chạy. Mọi lần kiểm chứng sau đó vẫn giữ evidence lịch sử; khi có regression phải cập nhật trạng thái hiện hành. Không đổi PRD/contract vượt quyền hoặc phát động deploy từ task.

**Xuất Issue:** `.agents/scripts/prepare-github-issue.py --source docs/plans/modules/<file>.md --task-id M3-Sxx-Txx` đọc nội dung từ `origin/main`, yêu cầu đúng các heading trên, G3 READY và G2 phù hợp. Output có đủ mô tả dễ hiểu, phạm vi, nghiệm thu, nguồn commit và metadata để Muse gọi GitHub API. Không tạo Issue trực tiếp, không biến task lịch sử thành READY.
