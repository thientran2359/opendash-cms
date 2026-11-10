# M3 — Lưu trữ kế hoạch các slice đã triển khai

Sáu file plan M3.1–M3.6 đã được chuyển từ `docs/plans/` vào thư mục này để root `plans/` chỉ chứa tài liệu roadmap, module, workflow và các kế hoạch còn theo dõi. Di chuyển theo yêu cầu của chủ dự án ngày 10/11/2026, kế thừa việc phân loại của [Issue #14 — Final Decision](https://github.com/thientran2359/opendash-cms/issues/14#issuecomment-6760144662).

**Phạm vi:** các slice dưới đây có [báo cáo thực chạy](../../../verification/README.md) trên **macOS** theo SHA ghi trong mỗi báo cáo; đó là nghiệm thu hành vi *theo slice*, **không phải chứng nhận toàn M3 hoặc Windows/Linux**. [M3 vẫn PARTIAL](../../../plans/M3_STATUS_2026-11-09.md); taxonomy, provider bridge, video/delete/reconcile media và parity vẫn cần xử lý qua gates. Không sửa G2 lịch sử, không phát hành task Ready từ các plan đã lưu.

## Mapping và chứng cứ

| Slice | File cũ (dưới `docs/plans/`) | Plan hiện tại | Bằng chứng |
| --- | --- | --- | --- |
| M3.1 | `2026-11-08-m3-content-types-and-fields.md` | [CPT/Custom Fields](2026-11-08-m3-content-types-and-fields.md) | [Domain schema/validation](../../../verification/2026-11-08-m3-1-content-domain.md) |
| M3.2 | `2026-11-08-m3-2-content-types-backend.md` | [CPT SQLite/API](2026-11-08-m3-2-content-types-backend.md) | [GET/POST và restart](../../../verification/2026-11-08-m3-2-content-types-backend.md) |
| M3.3 | `2026-11-08-m3-3-content-entry-crud.md` | [Entry CRUD](2026-11-08-m3-3-content-entry-crud.md) | [CRUD/persistence](../../../verification/2026-11-08-m3-3-content-entry-crud.md) |
| M3.4 | `2026-11-08-m3-4-admin-content-ui.md` | [Admin Content UI](2026-11-08-m3-4-admin-content-ui.md) | [Browser/draft/401/412](../../../verification/2026-11-08-m3-4-admin-content-ui.md) |
| M3.5 | `2026-11-09-m3-5-media-library.md` | [Media Library](2026-11-09-m3-5-media-library.md) | [Raster/thumbnail/picker](../../../verification/2026-11-09-m3-5-media-library.md) |
| M3.6 | `2026-11-09-m3-6-public-content-rendering.md` | [Public Rendering](2026-11-09-m3-6-public-content-rendering.md) | [Public CPT/media ACL](../../../verification/2026-11-09-m3-6-public-content-rendering.md) |

Đối với mỗi hàng, đường dẫn mới là `docs/archive/plans/m3/<tên-file-cũ>`. Toàn bộ link Markdown nội bộ đến sáu plan và link tương đối bên trong từng plan được cập nhật cùng đợt di chuyển; [chỉ mục kế hoạch hiện hành](../../../plans/README.md) dẫn về đây. Không giữ stub ở root `docs/plans/` để đáp ứng yêu cầu dọn thư mục. **Giới hạn tương thích:** link bên ngoài dùng `blob/main/docs/plans/<tên-file-cũ>` cần cập nhật; link dùng SHA commit cũ vẫn giữ nguyên nội dung lịch sử trên GitHub. Không chuyển báo cáo verification, ADR, contracts hoặc snapshot M3 sang archive.
