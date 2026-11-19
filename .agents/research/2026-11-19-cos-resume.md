# CoS resume sau coordinator ACK — 19/11/2026

Owner yêu cầu Aster bổ sung cơ chế tiếp tục cùng task sau review. Lần gửi đầu vẫn
one-shot; giữ nguyên historical receipts và khóa gốc. Đây là luồng Issue implementation,
không thay send-pr docs correction, G3, Project, security/data, CI hoặc native G5.

## Thiết kế và kế hoạch đã thực hiện

1. Thêm negative tests trước code, quan sát FAIL vì resume_issue chưa tồn tại.
2. Thêm resume-issue trong adapter hiện hữu để SSH vẫn stream một file, không phụ thuộc
   module mới trên máy đích. Kiểm host trước đọc receipt/GitHub/API; không đọc token
   ngoài guard hiện hữu. Dùng approved source không đổi, PR/base/head hiện tại.
3. Xác minh previous delivery ACCEPTED/sent và metadata latest turn ended, đọc hết
   pagination sessions, chặn active/unclear project turn. ACK mới phải sau turn end.
4. Re-fetch Issue open đúng label/task/source, PR open cùng repo/base main/agent branch
   và marker ACK do account owner ký Aster/Atlas. Pin toàn bộ trước và sau reservation.
5. Khóa append-only theo target/Issue/Task/predecessor request, reserve trước POST,
   UNKNOWN trước mọi network mutation. Mỗi predecessor một successor, giữ khóa gốc.
   Cùng UUID trả receipt; đổi UUID không né pending/UNKNOWN hoặc dùng lại ACK cũ.
   status-task đi theo chuỗi để trả receipt mới nhất, kể cả thiếu receipt sau crash.
6. Thêm CLI/SSH forwarding, cập nhật skill/Operating Standard, chạy bộ regression.

## Marker của comment coordinator mới

Đăng trên chính Issue sau khi lượt trước kết thúc; comment mô tả bounded ACK/feedback
và các gate còn lại. Không copy nội dung comment vào shell hoặc payload chat. Ví dụ:

```text
Agent: Aster — tiếp tục theo bounded ACK đã ghi, giữ nguyên acceptance/ownership.
<!-- opendash-resume-v1
{"repository":"thientran2359/opendash-cms","target":"harbor","issue":378,"task_id":"M6-S11-T01","source_sha":"<40-hex-G3>","base_sha":"<40-hex-PR-base>","pr":406,"head_sha":"<40-hex-head>","intent":"RESUME_CURRENT","coordinator":"Aster"}
-->
```

Adapter chỉ gửi fixed prompt chứa identifiers, không gửi arbitrary Issue text hoặc
runtime state vào GitHub. Coordinator phải kiểm live Project In progress/In review,
G3/dependencies/file ownership/dirty work trước lệnh. Recipient re-fetch pins và scope
trước edit; chat delivery không cấp quyền mới. Không dùng để mở lại task Done.

## Kiểm chứng

Actual command: `python3.13 -m unittest discover -s .agents/scripts -p 'test_cos_*.py'`.
45/45 PASS, gồm 31 regression cũ và 14 resume tests: hai generation; duplicate UUID;
ACK reused/foreign/unsigned/wrong Issue/PR/head/source; closed task; late active session;
pagination lặp; incomplete receipt; timeout; head race trước POST; host denial; SSH pins.
Giữ original failing RED và logs ở private temp, không đưa runtime IDs vào Git.

Live transport bằng lệnh resume mới: NOT_RUN tại thời điểm ghi bản thiết kế này;
chỉ được báo sent/turn ended sau khi receipt thực xác nhận. PONG cũ chứng minh transport
cơ sở, không thay kiểm chứng resume. Windows native/SSH end-to-end NOT_RUN; SSH args
được kiểm bằng unit test. Không sửa sản phẩm hoặc site live :18964.

Agent: Aster
