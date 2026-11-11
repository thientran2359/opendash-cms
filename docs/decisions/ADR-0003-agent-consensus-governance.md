# ADR-0003 — Agent chốt quyết định kỹ thuật; owner theo dõi

> **Cập nhật phân quyền:** [ADR-0010](ADR-0010-agent-project-and-merge-authority.md) là nguồn hiện hành về vai trò, tự chủ triển khai, review và merge. Các quy định cũ dưới đây được giữ làm lịch sử quyết định.

- **Trạng thái:** PROPOSED trong PR này; có hiệu lực khi reviewer độc lập review và merge vào main.
- **Ngày:** 2026-11-10.
- **Nguồn chỉ đạo:** Owner chuyển sang vai trò theo dõi tiến độ, không làm cổng phê duyệt mặc định cho thảo luận, G0–G3 hay tài liệu kỹ thuật.
- **Phạm vi:** Agent discussion, technical planning, contract/ADR, task handoff, peer review, documentation PR. Không sửa hồi tố hồ sơ lịch sử.

## Quyết định

1. **Tự chốt khi đồng thuận.** Các agent có tham gia thật, phản biện độc lập và đối chiếu source/PRD được quyền thống nhất phương án kỹ thuật trong phạm vi sản phẩm đã xác nhận. Coordinator/agent phụ trách viết một **Final Decision** với phương án + lý do, alternatives bị loại, source SHA, ảnh hưởng G0–G3/contract/ABI/DB, dependency, file ownership, acceptance/negative tests, giới hạn evidence và links đến từng turn. Kết luận được ghi **CONCLUDED_BY_AGENTS**, **Next agent: none**. Không gán NEEDS_OWNER chỉ vì đã hết lượt đối đáp.
   Coordinator cập nhật **Issue title/body metadata hoặc pinned decision index** sau khi comment Final Decision được xác minh, để Status/Current turn/Next agent không còn trỏ về lượt owner cũ. Chỉ cập nhật trạng thái thảo luận, không nâng task Project Ready/Done hay thay acceptance khi chưa qua gate.
2. **Bất đồng hoặc rủi ro trọng yếu.** Chỉ khi còn bất đồng thực chất hoặc cần kiểm định độc lập, coordinator chỉ định **một reviewer quyết định cuối** (ưu tiên Atlas hoặc Nova, phải độc lập với tác giả phương án và implementation). Reviewer so sánh evidence, ghi **DECIDED_BY_REVIEWER** với phương án được chọn hoặc **BLOCKED** với điều kiện phải bổ sung. Nếu reviewer cũng là tác giả, chọn reviewer độc lập khác; shared GitHub login không chứng minh độc lập. Review không bắt buộc cho mọi discussion đã đồng thuận nếu không có rủi ro đặc biệt.
3. **Đưa vào tài liệu chính thức.** Author được giao file ownership tổng hợp kết luận vào docs/plans, docs/contracts hoặc ADR trên branch/PR riêng; liên kết Final Decision và baseline, không sửa/xóa lịch sử. Review PR độc lập, kiểm docs guards/CI/test phù hợp. Reviewer đủ gate có thể chốt và merge **documentation/design PR** khi skill được gọi trực tiếp; không cần owner ký riêng. Thay đổi API/schema/security/ABI phải có G2/ADR + migration/test, không tự biến proposal thành implemented/verified.
4. **Giao việc.** Sau khi PR docs/contract merge, coordinator (Atlas/Nova hoặc agent được giao rõ) xác minh G3/source SHA/base SHA, GitHub Project Ready, dependency, một writer/file, reviewer và acceptance; tạo/giao task theo workflow. Không chờ owner chọn candidate thông thường, nhưng không tự claim task của agent khác hoặc né Project gate. PR implementation tiếp tục review độc lập/CI và merge có expected head SHA khi skill được gọi; không background merge.
5. **Owner theo dõi.** Owner nhận tiến độ, links quyết định/PR/Issue, blockers và phần chưa VERIFIED. Không có owner review/approval mặc định. Chỉ hỏi owner khi thiếu **yêu cầu sản phẩm thực sự mới** không thể suy ra PRD/chỉ đạo trước, hoặc cần quyền thao tác ngoài quyền agent. Chỉ đạo trực tiếp mới của owner luôn ưu tiên.
6. **Không hạ chuẩn an toàn.** Security HIGH, nguy cơ mất dữ liệu, ACL/credential/trust boundary, quyền truy cập ngoài hệ thống hay tests bắt buộc chưa đạt ⇒ **BLOCKED** + bổ sung evidence/phương án an toàn + reviewer độc lập. Reviewer không được waive hard blocker chỉ vì consensus. Ngoại lệ cần người nắm quyền thực tế phải có ủy quyền hợp lệ trước thao tác, không coi đó là owner review định kỳ. Không sửa live secrets/ACL, force push, bypass CI/protection hoặc triển khai ngoài scope. PR #7 W01-SEC-01 vẫn BLOCKED_SECURITY; PR #9/TW PAUSED; Issue #5 NO_DISPATCH.
7. **Không giả hoàn tất.** CONCLUDED_BY_AGENTS / DECIDED_BY_REVIEWER chỉ đóng vòng quyết định. AGREED cần tài liệu đã tích hợp; IMPLEMENTED, VERIFIED và MERGED là trạng thái riêng, cần evidence đúng SHA/OS/CI; thiếu môi trường ghi NOT_RUN, không sửa hồi tố evidence.

## Áp dụng và chuyển tiếp

- Áp dụng cho thảo luận/PR mở **sau khi ADR và các sửa đổi workflow merge vào main**. Issue #14 và #15 được coordinator đối chiếu turns đã đăng, chốt Final Decision hoặc reviewer verdict, không đăng lại turns cũ; sau đó tạo PR docs/task theo gate. Không tự đóng hay đánh dấu VERIFIED chỉ vì thay chính sách.
- Hướng dẫn lịch sử ghi NEEDS_OWNER cho quyết định kỹ thuật thông thường phản ánh chính sách cũ; không là rào cản sau khi sửa đổi này có hiệu lực. Giữ immutable links và các thread archived.
