# M7 — Task planning: Site Features

## Usable Beta integration overlay — 16/11/2026

[G3 M7-S14](slices/2026-11-16-usable-beta-g3.md) bổ sung bốn task evidence/planning trên
capability đã có, không thêm candidate sản phẩm: T01 Forge audit Windows; T02 Atlas
triage/G3 fixes; T03 Forge retest/human dogfood; T04 Atlas Beta readiness.
T03 chờ required fixes G5 và human tester; T04 chờ candidate evidence.
T01 không chờ full Linux/macOS closure. Product fixes phát hiện sau audit cần G3 riêng,
exact ownership và Ready, không thuộc quyền sửa của audit. Status bên dưới là snapshot
planning 14/11, không thay Project.

`M7-Pxx` là candidates lập kế hoạch, chỉ trở thành GitHub tasks sau khi G2/G3 được review/merge, owner/reviewer và dependencies rõ. [PRD V1](../../../product/PRODUCT_REQUIREMENTS.md) · [M5 prerequisites](../M5/README.md).

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance dự kiến |
| --- | --- | --- | --- | --- |
| M7-P01 | **Mở rộng** Site Settings UI/engine core M3, phân quyền | SITE-01 (FND-05) | M3-P13 General Settings, M5 Theme Options, G2 schema/lifecycle | Admin đổi cấu hình nâng cao/ngôn ngữ nội dung/config, Editor bị chặn; persistence/restart, provider disabled vẫn quản trị được |
| M7-P02 | Menu/navigation CRUD và binding theme **trên model M5**, không tạo persistence riêng | SITE-01 (THM-02/03) | **M5-P09** menu site-owned persistence, **M5-P05** import mẫu, M5 template contract | Admin tạo/sửa/xóa/sắp xếp menu trên cùng stable IDs/refs đã nhập từ theme; binding page/CPT URL, preview/Save, đổi theme/restart không mất menu hoặc bản người dùng chỉnh |
| M7-P03 | SEO fields: URL/redirect/canonical/title/description/OG/robots | SITE-02 (CNT-04, CNT-05) | M5 URL policy, Full-Track G2 collision/redirect | Canonical/OG/robots đúng nội dung và escaped, redirect không loop, slug/reserved path không conflict |
| M7-P04 | Sitemap tự động và invalidation | SITE-02 | P03, M3 content persistence | Sitemap có URLs public thực và metadata hợp lệ; Save/Delete/URL change phản ánh đúng, nội dung private/unreferenced không bị lộ |
| M7-P05 | Builder contact form schema/validation/submission persistence | SITE-03 (BLD-02/03, FND-02) | M4 Builder, M5 template, G2 abuse/RBAC/storage | Người dùng tạo form trường cơ bản, đặt lên page; khách gửi được, nhập sai bị từ chối, Admin xem submissions; rate/CSRF hoặc abuse guards theo contract |
| M7-P06 | SMTP settings + thông báo/giám sát khi gửi lỗi | SITE-03 (SITE-01) | P01/P05, G2 secret storage/error semantics | Admin cấu hình SMTP, notification chính xác, lỗi SMTP không làm mất submission; credentials không lộ qua API/log |
| M7-P07 | E2E Site Features + video / security | SITE-01/02/03 (CNT-05) | P01..P06 + P08, M5 public delivery | Site menu→SEO→form gửi→Admin submissions→SMTP error regression, media video/ACL, auth/escaping/abuse/restart; evidence SHA/OS |
| M7-P08 | **D từ #15 (video):** media video V1 có metadata/serving/limits an toàn | CNT-05 (SITE-02) | M3 raster library/ACL + P03 nếu video URL/OG; Full-Track G2 codec/MIME/Range/size/privacy | Upload và serve video đúng quyền/Range/size, invalid codec/MIME/range bị chặn, restart/refs không rò dữ liệu; chưa hard-delete/reconcile (M8) |

**Ranh giới:** M8 chịu trách nhiệm backup/restore và Linux release. M7 không bao gồm đa ngữ, analytics, form nâng cao/repeater, SEO AI hoặc revision (ngoài phạm vi V1).

## G1–G3 đã chuẩn bị cho toàn milestone (2026-11-14)

[G1 inventory](../../../../.agents/research/2026-11-14-m7-site-features-g1.md) · [ADR-0014](../../../decisions/ADR-0014-m7-site-features.md) · [Contract](../../../contracts/M7_SITE_FEATURES.md) · [G3 đủ14 task](slices/2026-11-14-m7-site-features-g3.md). G2/G3 có hiệu lực sau merge design PR; mọi candidate ở bảng trên đã có task/Primary Acceptance/owner/dependency/acceptance, không candidate nào bị bỏ. Project Ready vẫn chờ M5/M6 Windows development gate và từng seam bàn giao (macOS NOT_RUN hoãn theo ADR-0014). Implementation NOT_STARTED, runtime/native/browser NOT_RUN. Không phát hành Issue hoặc dispatch từ lượt planning này.

## Beta remediation disposition — 17/11/2026

[Findings and M7 repair G3](slices/2026-11-16-usable-beta-fixes-g3.md) records all
nine findings from #300. Aster owns #301 by direct owner handoff; Forge reviews.
M7-S15-T01..T05 cover menu target array consumption, draft preservation, atomic
published create, media picker and bounded navigation harness. M5-S11-T01 theme
v2 discovery, M6-S09-T01 SSRF test reproducibility and M8-S10-T01/T02 recovery
security/harness use separate milestone docs PRs; no runtime ownership is granted
by this planning update. Mandatory fixes and harness receipts precede #302.
F08 remains a security blocker; no Beta/native/release PASS from specs or CI.

## Owner review notes01–07 — 17/11/2026

[Harbor follow-up G3](slices/2026-11-17-owner-admin-review-g3.md) covers the bounded
Admin delta under [ADR-0018](../../../decisions/ADR-0018-owner-admin-review.md).
PLANNED, not Project Ready or runtime VERIFIED. See shared serial delivery order.


## Technical retest extension — 20/11/2026

[M7-S14-T05 G3](slices/2026-11-20-beta-technical-retest-g3.md) tách Harbor technical
retest macOS khỏi human #302. Candidate 5ad3738, C1–C4 impact, report-only ownership;
Aster coordinator. Specs merged khác Project Ready. #302 human và #303 readiness
giữ gates riêng; không Beta PASS từ T05.
