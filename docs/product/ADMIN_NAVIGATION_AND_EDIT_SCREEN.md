# Admin Navigation và bố cục màn hình chỉnh sửa

**Ngày:** 15/11/2026 · **Owner điều phối:** Atlas · **Draft G2 Contract/ADR:** Nova.
**Trạng thái:** Yêu cầu sản phẩm được chủ dự án xác nhận trong chat (bao gồm chỉ đạo tách hẳn màn hình cấu hình `Cài đặt → Loại nội dung & Trường` khỏi nhập liệu entry); dự thảo [ADR-0016](../decisions/ADR-0016-admin-navigation-and-single-edit.md) và [Contract Admin Navigation & Single Edit](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) đang ở trạng thái **`G2 NEEDS_RESPONSE`** (xử lý `R1`–`R5` tại [Issue #268](https://github.com/thientran2359/opendash-cms/issues/268) và `270-R1`–`270-R6` tại [PR #270](https://github.com/thientran2359/opendash-cms/pull/270#issuecomment-6902861188), chưa `AGREED` toàn bộ cho tới khi Atlas ghi Final Decision); triển khai và kiểm chứng phần bổ sung này chưa thực hiện.

Nguồn phạm vi: [Product Requirements](PRODUCT_REQUIREMENTS.md). Áp dụng quy trình [Planning Workflow](../plans/PLANNING_WORKFLOW.md), quy chuẩn [Core Foundation](../contracts/CORE_FOUNDATION.md), [Plugin ABI](../contracts/PLUGIN_ABI.md) và dự thảo [Contract Admin Navigation & Single Edit](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) khi thiết kế tiếp.

## 1. Mục tiêu

Người quen WordPress có thể tìm đúng công cụ trong Admin, quản lý nội dung và chỉnh từng trang theo bố cục nhất quán. Các tính năng phát triển sau đăng ký thành phần vào vùng có sẵn; không phải thêm các panel nối tiếp tùy ý vào một màn hình lớn.

Ưu tiên đợt này là cấu trúc Admin, menu nhiều cấp và màn hình single edit. Không tự mở rộng sang Gutenberg, editor mới, revisions, autosave, lịch đăng hoặc workflow duyệt bài.

## 2. Menu chính và submenu

| Menu chính | Submenu mục tiêu |
| --- | --- |
| Tổng quan | Dashboard |
| Bài viết | Tất cả bài viết · Thêm mới · Chuyên mục · Thẻ |
| Trang | Tất cả trang · Thêm mới |
| Thư viện | Tất cả media · Tải lên |
| Giao diện | Themes · Tùy chỉnh theme · Menus · Templates · Builder |
| Liên hệ | Form liên hệ · Tin nhắn nhận được · Cấu hình email |
| Plugins | Plugins đã cài · Cài từ ZIP |
| Người dùng | Tất cả người dùng · Thêm mới · Hồ sơ của tôi |
| Công cụ | Sao lưu · Phục hồi · Đối soát media · Tình trạng hệ thống |
| Cài đặt | Tổng quát · Đường dẫn · SEO website · Loại nội dung & Trường |

Đây là sơ đồ sản phẩm mục tiêu, không khẳng định mọi submenu đã có chức năng. Trước G3 phải lập bảng ánh xạ submenu → màn hình/API hiện có → khoảng thiếu; mục mới như hồ sơ cá nhân cần phạm vi và acceptance riêng nếu chưa có.

- CPT như Dự án/Dịch vụ có menu riêng (`#/cpt/:typeKey`): danh sách, thêm mới và taxonomy gắn với CPT đó. Taxonomy dùng chung giữ cùng dữ liệu, không tạo bản sao vì xuất hiện ở nhiều menu.
- Phần khai báo CPT/schema/fields đặt cố định tại **`Cài đặt → Loại nội dung & Trường`** (`#/settings/content-types`, chỉ hiển thị cho `Admin`, luôn có mặt kể cả khi website có `0` CPT). Theo chỉ đạo trực tiếp của chủ dự án, khu vực này **chỉ dùng để tạo/sửa CPT, quản lý nhóm custom fields và cấu hình vị trí panel; tuyệt đối không có danh sách hay form nhập liệu entry**.
- Mỗi màn hình có URL riêng, mở trực tiếp hoặc reload vẫn đến đúng màn hình; back/forward hoạt động. Bảng hash route chuẩn được đặc tả tại [Contract Admin Navigation & Single Edit](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md#51-bảng-định-tuyến-hash-url-chuẩn--chuyển-hướng-tương-thích-ngược).
- Menu cha tự mở theo màn hình hiện tại; submenu được đánh dấu rõ. Có breadcrumb, tiêu đề và hành động chính nhất quán.
- Menu theo quyền Admin/Editor; ẩn menu không thay thế kiểm quyền server. URL trực tiếp vẫn phải được bảo vệ.
- Sidebar thu gọn trên desktop, dạng ngăn kéo trên mobile; hỗ trợ bàn phím và trạng thái mở/đóng có nhãn truy cập.
- Giữ bản nháp khi chuyển màn hình, cảnh báo thay đổi chưa lưu đúng mục; khi hết hạn phiên (`401`), bản nháp được quarantine (ẩn khỏi DOM trước khi đăng nhập lại) và sau khi xác thực lại phải kiểm tra lại quyền theo cấu hình hiệu lực (`effective-editor-config`) để không lộ trường nhạy cảm nếu tài khoản vừa bị hạ quyền (`R5`). Logout/đổi tài khoản/đổi site xóa sạch bản nháp người trước.

## 3. Các vùng của màn hình single edit

Áp dụng cho Post, Page và CPT. Bố cục desktop mục tiêu:

```text
Thanh màn hình: breadcrumb / tiêu đề / Xem trước / Lưu hoặc Xuất bản
┌──────────────────────────────────┬─────────────────────────┐
│ Tiêu đề nội dung, đường dẫn       │ Xuất bản / trạng thái   │
├──────────────────────────────────┼─────────────────────────┤
│ Panel phía trên nội dung         │ Thuộc tính / template   │
│ Nội dung chính / mở Builder      │ Ảnh đại diện / taxonomy │
│ Panel phía dưới nội dung         │ Nhóm field phụ          │
│ Nhóm custom fields / SEO         │ Panel mở rộng           │
└──────────────────────────────────┴─────────────────────────┘
```

- Core sở hữu thanh hành động và quy tắc Lưu/Xuất bản. Không cho panel tùy ý ghi đè hành động core.
- Các vùng đăng ký chuẩn: `header_bar` (chỉ dành cho core), `main_top` (phía trên nội dung chính), `main_primary` (nội dung chính), `main_bottom` (phía dưới nội dung chính) và `sidebar`.
- Mobile (`< 1024px`) chuyển sang một cột theo thứ tự DOM và thứ tự đọc thống nhất: **`header_bar → main_top → main_primary → main_bottom → sidebar`**; không mất field, lỗi validation hay hành động lưu.
- Phân biệt chỉnh dữ liệu nội dung, chỉnh layout qua Builder và chỉnh template dùng chung; mở Builder phải xác định phạm vi ảnh hưởng (template dùng chung toàn site chỉ dành cho `Admin`, `Editor` chỉ sửa override gắn với `entry_id` cụ thể) và bảo toàn dữ liệu chưa lưu.

## 4. Panel registry và nhóm custom fields

Developer đăng ký panel theo cùng cơ chế mà các tính năng core sử dụng. Yêu cầu khai báo gồm định danh có cấu trúc (`ScopedResourceId`), chủ sở hữu (`Core`, `User`, `Theme`, `Plugin`), tiêu đề, vùng hiển thị, thứ tự, loại nội dung áp dụng, quyền xem/sửa (`min_view_role`, `min_edit_role`), mức phơi bày công khai của field (`public_exposure`) và khả năng thu gọn. Kiểu dữ liệu và API đăng ký được đặc tả trong [Contract Admin Navigation & Single Edit](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md).

- Người quản trị chọn các panel có sẵn áp dụng cho CPT: nội dung, ảnh đại diện, taxonomy, thuộc tính, SEO và panel mở rộng tương thích. Panel bắt buộc của core và panel chứa field `required: true` không bị tắt tùy ý.
- Người quản trị đặt **nhóm field** (bao gồm cả việc chia các base fields có sẵn của CPT thành nhiều nhóm `base_field_panels` đặt ở các vùng khác nhau — `270-R6`, cũng như các nhóm `content_field_groups`) vào vùng mong muốn và sắp xếp field bên trong nhóm (`subgroups`). Ví dụ “Thông tin dự án” dưới nội dung, “Trạng thái dự án” ở sidebar.
- Đổi vị trí chỉ thay cách trình bày, không đổi field ID, quyền sở hữu schema, giá trị đã lưu trong `content_entries.values_json` hoặc binding của template.
- Cấu hình vị trí của CPT/nhóm field là cấu hình dùng chung. Tùy chọn bố cục riêng từng người dùng và kéo thả panel trực tiếp chưa nằm trong yêu cầu đã chốt.
- Plugin và Theme khai báo panel/field group bằng cấu hình JSON tĩnh theo contract; không cho phép JavaScript/React/HTML tùy ý. Quyền phải được kiểm cả client và server (bao gồm tách bạch `min_view_role`, `min_edit_role` và `public_exposure`, bảo toàn toàn bộ các nhánh nguồn media công khai hiện hữu — `270-R2`, cùng kiểm soát `ScopedFieldGrant` và lọc payload gửi vào Wasm hooks/actions theo quyền thực tế của `actor.user.role` và capability của plugin — `R2`, `270-R3`).
- Registry xử lý trùng ID, thứ tự bằng nhau, provider bị tắt/rút khai báo và vị trí không hợp lệ một cách xác định. Khi tắt plugin, đổi theme (`provider_inactive`), rút nhóm field, bỏ `target_type` hoặc rút một field (`withdrawn` — `270-R1`), hệ thống **giữ nguyên schema đã lưu, quyền hạn, kiểm kê tham chiếu media (`collect_pins_in_tx`) và dựng form chuẩn của Core để người có quyền tiếp tục xem/sửa dữ liệu nội dung**; chỉ dừng các hành động (`plugin_action`), Wasm hooks và template thuộc về provider đã tắt (`R1`, `R4`, `270-R1`). Không được xóa giá trị vì panel đang ẩn hoặc hard-delete nhóm/target/field đang có dữ liệu.

## 5. Lưu, validation và hành động riêng

- Các field thuộc cùng nội dung dùng chung draft state, validation, thông báo thay đổi chưa lưu và nút Lưu nguyên tử một lần (`CompositeEntrySaveInput` với `PatchField<T>` 3 trạng thái bảo đảm `Omitted = Preserve` khi update, kèm `expected_schema_token` bắt buộc và kiểm tra lại snapshot trong giao dịch `BEGIN IMMEDIATE` — `R3`, `270-R4`, `270-R5`). Không tạo nhiều form lưu riêng gây mất nhất quán cho cùng record.
- Field lỗi trong panel thu gọn phải được chỉ rõ; đưa người dùng đến field cần sửa. Giữ dữ liệu nhập khi gặp lỗi xác thực, quyền hoặc xung đột phiên bản.
- Giữ semantics hiện có: Lưu draft Post/Page không công khai; Xuất bản mới công khai; Lưu nội dung đã published cập nhật public. Không tự thêm workflow draft cho CPT/Builder.
- Panel có hành động độc lập (`PanelKind::Action`), như gửi email thử hoặc đồng bộ CRM, dùng nút/phản hồi riêng và khai báo quyền/phạm vi tác động; không tự chạy khi bấm Lưu nội dung.
- Ranh giới lưu nguyên tử, xử lý xung đột CAS/schema token và lọc dữ liệu nhạy cảm qua Wasm hooks/actions được quy định tại [Contract Admin Navigation & Single Edit](../contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md#4-ma-trận-endpoint-apiadmincontent-types-schema_token-r3-patchfieldt-3-trạng-thái-270-r5-và-quyền-private-của-plugin-270-r3).

## 6. Đầu ra cần chốt trước triển khai

1. Inventory màn hình/route hiện tại và sơ đồ điều hướng có ánh xạ quyền; phân biệt tái tổ chức UI với chức năng mới.
2. Mockup desktop cho danh sách bài viết, single edit Page/CPT và Settings; trạng thái mobile, lỗi, dirty và quyền hạn.
3. Contract registry: identity/owner (`ScopedResourceId`), vị trí/thứ tự (`EditorLayoutConfigV1` / `base_field_panels`), field groups (`content_field_groups` + `content_field_group_targets`), lưu cấu hình, validation, provider lifecycle (`R1`–`R5`, `270-R1`–`270-R6`) và tương thích plugin/theme hiện tại.
4. Kế hoạch migration, task G3 với ownership/dependency, acceptance theo từng slice và mức tái sử dụng Base APIs. Chưa cấp mã milestone/task hoặc `Project Ready` cho tới khi contract G2 được Atlas duyệt và merge.

## 7. Tiêu chí đánh giá sản phẩm

- Người dùng tìm được màn hình từ menu/submenu; URL trực tiếp, reload, back/forward và active navigation nhất quán.
- Admin/Editor thấy đúng các công cụ được phép và không vượt quyền qua URL/API (`Editor` đọc được `effective-editor-config` đã lọc theo quyền để dựng Single Edit nhưng không sửa được raw schema/layout hay template toàn site).
- Một Page và một CPT có nhóm field (gồm cả việc chia base fields sẵn có thành nhiều nhóm panel ở các vùng khác nhau — `270-R6`) đặt ở vùng chính/sidebar, thứ tự ổn định sau reload; sửa vị trí không mất dữ liệu; tắt plugin/đổi theme/bỏ target vẫn xem/sửa được dữ liệu và giữ nguyên tham chiếu media (`R1`, `270-R1`).
- Panel core và panel mở rộng tuân theo cùng quy tắc bố cục; lỗi/trùng ID/provider mất được xử lý rõ ràng.
- Một lần Lưu xử lý nguyên tử các field của nội dung; navigation, `401` (quarantine + kiểm quyền lại sau khi đăng nhập) và conflict không làm mất draft hoặc lộ trường vừa mất quyền (`R5`). Hành động độc lập không chạy nhầm.
- Dùng [site demo #258](https://github.com/thientran2359/opendash-cms/issues/258) để bổ sung case và screenshots theo source SHA; ảnh chụp thành công không tự là visual approval.

Yêu cầu này không đóng hoặc miễn nghiệm thu M8, không thay ownership task Nova đang thực hiện. Các lỗi đã xác nhận tại [#262](https://github.com/thientran2359/opendash-cms/issues/262) vẫn được theo dõi riêng.
