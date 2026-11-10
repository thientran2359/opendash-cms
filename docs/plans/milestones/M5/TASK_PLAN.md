# M5 — Task planning: Theme & Templates

Mỗi hàng `M5-Pxx` là **task candidate**, không phải G3 approved hoặc Issue `Ready`. [PRD V1](../../../product/PRODUCT_REQUIREMENTS.md) và [module coverage](../COVERAGE_MATRIX.md) định nghĩa scope.

| Candidate | Slice / task outcome | Module primary (phối hợp) | Dependency / gate | Acceptance dự kiến |
| --- | --- | --- | --- | --- |
| M5-P01 | **Mở rộng** theme registry/activation M3: template ownership, update/version và package lifecycle | THM-01 (FND-01) | M3-P14 basic theme install/list/activate, M4 AST, Full-Track G2 package/owner lifecycle | Cài/đổi/cập nhật theme có version/rollback policy an toàn, ZIP traversal bị chặn, provider missing có chẩn đoán; không viết lại UI M3 |
| M5-P02 | URL policy nâng cao + RequestContext cho CPT/post/page/taxonomy | CNT-04 (SITE-02, CNT-03) | M3 post/page public routes, M4-P09/P10 taxonomy, Full-Track G2 reserved URLs/collision/visibility | Precedence/slug collision/404/redirect/canonical tối thiểu, giữ homepage `/`, không để theme tự sở hữu matcher |
| M5-P03 | Template composition page/header/footer/archive/single | THM-02 (BLD-01/05) | P01/P02, M4 AST/binding | Gán template, HTML escaped, header/footer/layout đúng theo request; archive và phân trang hoạt động với entry thật |
| M5-P04 | Theme Options **mở rộng Settings core M3** qua declarations | SITE-01 (THM-02, FND-05) | M3-P13 General Settings engine, G2 theme option schema/owner, P01/P03 | Auto-form từ declaration, phân biệt site/theme options, RBAC, persistence qua restart/theme switch; không nhân đôi site settings storage |
| M5-P05 | Import mẫu theo lựa chọn: page/menu/media/settings | THM-03 (SITE-01, CNT-05/04) | P01/P04, M3 media/entry; provenance/duplicate strategy G2 | Chọn nhập từng loại, không ghi đè content hiện hữu; lỗi một bước có rollback/diagnostic, retry không nhân bản |
| M5-P06 | Đổi/cập nhật theme bảo toàn layout người dùng và nội dung | THM-03 (BLD-02/03) | P01/P03/P05, M4 persisted user edits; Full-Track ownership G2 | Layout user chỉnh còn mở lại sửa, data/schema không bị xóa; chỉ thay bằng default nếu user chủ động chọn |
| M5-P07 | Missing-provider fallback và public accessibility/security | THM-01/02 (CNT-04, BLD-01) | P01..P06 | Block/template thiếu có thông báo và render fallback; CSP/XSS/security negative tests, không lộ metadata/plugin internals |
| M5-P08 | E2E public delivery và theme preservation | THM-02/03 (SITE-02, BLD-05) | P02..P07, M4 G5, M3 content | Admin tạo entry/CPT→template→preview chưa Save→Save→URL/archive/pagination; đổi/update theme→nội dung và layout vẫn edit được |

**Ranh giới:** General Settings tối thiểu và theme ZIP/list/activate đã là M3, không làm lại ở M5. M5 thêm URL policy/theme options/template/import; SEO title/OG/sitemap/robots, navigation và site settings nâng cao thuộc M7. Plugin provider bridge và manifest blocks/templates/settings thuộc M6.
