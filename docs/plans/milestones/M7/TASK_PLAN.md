# M7 — Task planning: Site Features

`M7-Pxx` là candidates lập kế hoạch, chỉ trở thành GitHub tasks sau khi G2/G3 được review/merge, owner/reviewer và dependencies rõ. [PRD V1](../../../product/PRODUCT_REQUIREMENTS.md) · [M5 prerequisites](../M5/README.md).

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance dự kiến |
| --- | --- | --- | --- | --- |
| M7-P01 | **Mở rộng** Site Settings UI/engine core M3, phân quyền | SITE-01 (FND-05) | M3-P13 General Settings, M5 Theme Options, G2 schema/lifecycle | Admin đổi cấu hình nâng cao/ngôn ngữ nội dung/config, Editor bị chặn; persistence/restart, provider disabled vẫn quản trị được |
| M7-P02 | Menu/navigation CRUD và binding theme | SITE-01 (THM-02/03) | M5 template/menu import contract | Tạo menu, đổi thứ tự, gắn đúng page/CPT URL, preview/Save và theme switch không xóa menu user |
| M7-P03 | SEO fields: URL/redirect/canonical/title/description/OG/robots | SITE-02 (CNT-04, CNT-05) | M5 URL policy, Full-Track G2 collision/redirect | Canonical/OG/robots đúng nội dung và escaped, redirect không loop, slug/reserved path không conflict |
| M7-P04 | Sitemap tự động và invalidation | SITE-02 | P03, M3 content persistence | Sitemap có URLs public thực và metadata hợp lệ; Save/Delete/URL change phản ánh đúng, nội dung private/unreferenced không bị lộ |
| M7-P05 | Builder contact form schema/validation/submission persistence | SITE-03 (BLD-02/03, FND-02) | M4 Builder, M5 template, G2 abuse/RBAC/storage | Người dùng tạo form trường cơ bản, đặt lên page; khách gửi được, nhập sai bị từ chối, Admin xem submissions; rate/CSRF hoặc abuse guards theo contract |
| M7-P06 | SMTP settings + thông báo/giám sát khi gửi lỗi | SITE-03 (SITE-01) | P01/P05, G2 secret storage/error semantics | Admin cấu hình SMTP, notification chính xác, lỗi SMTP không làm mất submission; credentials không lộ qua API/log |
| M7-P07 | E2E Site Features + video / security | SITE-01/02/03 (CNT-05) | P01..P06 + P08, M5 public delivery | Site menu→SEO→form gửi→Admin submissions→SMTP error regression, media video/ACL, auth/escaping/abuse/restart; evidence SHA/OS |
| M7-P08 | **D từ #15 (video):** media video V1 có metadata/serving/limits an toàn | CNT-05 (SITE-02) | M3 raster library/ACL + P03 nếu video URL/OG; Full-Track G2 codec/MIME/Range/size/privacy | Upload và serve video đúng quyền/Range/size, invalid codec/MIME/range bị chặn, restart/refs không rò dữ liệu; chưa hard-delete/reconcile (M8) |

**Ranh giới:** M8 chịu trách nhiệm backup/restore và Linux release. M7 không bao gồm đa ngữ, analytics, form nâng cao/repeater, SEO AI hoặc revision (ngoài phạm vi V1).
