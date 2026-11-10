# M5 — Task planning: Theme & Templates

Mỗi hàng `M5-Pxx` là **task candidate**, không phải G3 approved hoặc Issue `Ready`. [PRD V1](../../../product/PRODUCT_REQUIREMENTS.md) và [module coverage](../COVERAGE_MATRIX.md) định nghĩa scope.

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance dự kiến |
| --- | --- | --- | --- | --- |
| M5-P01 | Theme ZIP registry/activation, templates và package ownership | THM-01 (FND-01) | M1 ZIP baseline, M4 AST, G2 package/owner lifecycle | Cài/đổi/cập nhật theme an toàn, file invalid/ZIP traversal bị chặn, provider missing vẫn thấy chẩn đoán |
| M5-P02 | URL policy + RequestContext cho CPT/page/taxonomy | CNT-04 (SITE-02, CNT-03) | M3 entry/taxonomy, Full-Track G2 reserved URLs/collision/visibility | Route precedence/slug collision/404/redirect/canonical tối thiểu, giữ homepage `/`, không để theme tự sở hữu URL matcher |
| M5-P03 | Template composition page/header/footer/archive/single | THM-02 (BLD-01/05) | P01/P02, M4 AST/binding | Gán template, HTML escaped, header/footer/layout đúng theo request; archive và phân trang hoạt động với entry thật |
| M5-P04 | Theme Options với settings engine dùng chung tối thiểu | SITE-01 (THM-02, FND-05) | Schema/owner/settings storage G2, P01/P03 | Auto-form từ declaration, phân biệt site/theme settings, quyền Admin và persistence qua restart/theme switch |
| M5-P05 | Import mẫu theo lựa chọn: page/menu/media/settings | THM-03 (SITE-01, CNT-05/04) | P01/P04, M3 media/entry; provenance/duplicate strategy G2 | Chọn nhập từng loại, không ghi đè content hiện hữu; lỗi một bước có rollback/diagnostic, retry không nhân bản |
| M5-P06 | Đổi/cập nhật theme bảo toàn layout người dùng và nội dung | THM-03 (BLD-02/03) | P01/P03/P05, M4 persisted user edits; Full-Track ownership G2 | Layout user chỉnh còn mở lại sửa, data/schema không bị xóa; chỉ thay bằng default nếu user chủ động chọn |
| M5-P07 | Missing-provider fallback và public accessibility/security | THM-01/02 (CNT-04, BLD-01) | P01..P06 | Block/template thiếu có thông báo và render fallback; CSP/XSS/security negative tests, không lộ metadata/plugin internals |
| M5-P08 | E2E public delivery và theme preservation | THM-02/03 (SITE-02, BLD-05) | P02..P07, M4 G5, M3 content | Admin tạo entry/CPT→template→preview chưa Save→Save→URL/archive/pagination; đổi/update theme→nội dung và layout vẫn edit được |

**Ranh giới với M7:** M5 chỉ triển khai **URL policy và settings/menu tối thiểu cần cho theme/import**; SEO title/OG/sitemap/robots đầy đủ và site settings UI hoàn chỉnh vẫn thuộc M7. **Ranh giới M6:** plugin manifest blocks/templates/settings không nằm trong theme ZIP implementation.
