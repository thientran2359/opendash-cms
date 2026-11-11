# M3 — G3 cho Dashboard, Themes, Plugins và nghiệm thu release

**Baseline G2:** `origin/main@9c7cfd68dfb0a4a1ee0b0e61af4367e1a43cffe5` · [ADR-0006](../../../../decisions/ADR-0006-m3-admin-and-release-contracts.md) · [ADR-0005](../../../../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md) · [Users và Settings](2026-11-11-m3-users-settings-g3.md) · [Issue #45](https://github.com/thientran2359/opendash-cms/issues/45).

Các đặc tả READY có hiệu lực sau khi PR G3 merge. Phát hành Issue cho cả đợt sau khi kiểm trùng và pin source SHA. Chỉ đưa vào Project Ready khi đủ dependency và không tranh file. Mỗi agent có tối đa một implementation task đang chạy. P18 chạy lại trong P19. P01–P09, P11 và P16 dùng evidence hiện có và regression trong P17/P19.

Thứ tự file tích hợp chung: Users → Settings → Dashboard → Themes. Plugins dùng file riêng và có thể chạy khi Forge rảnh. P17/P19 có thể chuẩn bị harness trước khi tính năng cuối merge. Chốt release SHA sau khi code sản phẩm và harness bắt buộc đã tích hợp, rồi chạy nghiệm thu cùng SHA trên hai hệ điều hành.

### M3-S26-T01 — Hiển thị thống kê Post và Page trên Dashboard

- **Thuộc:** V1, M3, M3-S26, M3-P10.
- **Primary Acceptance Slice của capability:** M3-S26 cho FND-05.2.
- **Module:** FND-05
- **Capability:** FND-05.2
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P10, ADR-0005.
- **Người triển khai / review:** Forge dự kiến, kiểm chứng Windows theo ADR-0005.

#### Tổng quan và giá trị

Admin và Editor cần thấy nhanh số bài viết và trang đã xuất bản hoặc còn nháp ngay sau khi vào Admin.

#### Mục tiêu và hành vi quan sát được

Dashboard hiển thị bốn số đếm và các lối tắt theo quyền. Khi API lỗi, người dùng vẫn điều hướng được và có thể thử tải lại.

#### Phạm vi công việc

Thêm `GET /api/admin/dashboard` theo ADR-0006 và query aggregate một snapshot trong storage. Nối Dashboard vào AdminShell, phân biệt loading, zero và error.

#### Ngoài phạm vi

Quản lý tài khoản, analytics, danh sách nội dung gần đây và thay đổi policy xem bài nằm ngoài task.

#### Phụ thuộc và điều kiện chặn

P07/P09 đã có trên `main`. Chờ bàn giao file chung sau tích hợp Settings. Forge chỉ nhận khi rảnh và Project Ready đã xác minh. P14 dùng chung router/shell nên chỉ tích hợp sau khi PR Dashboard merge hoặc có bàn giao độc quyền trên hai Issue.

#### Kỹ thuật và file ownership

Forge sở hữu `cms/crates/opendash-server/src/routes/dashboard.rs`, `storage/dashboard.rs`, `routes/mod.rs`, `storage/mod.rs`, `app.rs` trong cùng `src/`, `cms/admin/src/api/dashboard.ts`, `api/index.ts`, `App.tsx`, `shell/AdminShell.tsx`, `features/dashboard/DashboardPage.tsx` trong cùng Admin `src/`, `cms/scripts/dashboard_smoke.py`, `cms/scripts/dashboard_browser.py` và `docs/verification/2026-11-11-m3-s26-t01-dashboard-windows.md`. Cần Design Delta nếu mở rộng quyền sở hữu.

#### Cách triển khai theo contract

Dùng `require_session`, ApiError và SQL trong storage. Chỉ đếm built-in post/page theo draft/published trong một query. Trả 0 cho nhóm rỗng và `Cache-Control: no-store`. Tái sử dụng role policy và API client hiện hành.

#### Nghiệm thu

- [ ] Fixture có draft/published cho cả post và page. Kết quả thay đổi đúng sau publish/unpublish, không tính CPT khác.
- [ ] Guest 401. Admin và Editor thấy tập số đếm đúng quyền và shortcut đúng role. Response không chứa ID, username hoặc log.
- [ ] Browser kiểm zero/loading/error/retry, bàn phím và viewport 390 px. Builder và Posts/Pages giữ luồng hiện có.

#### Kiểm thử, rủi ro và rollback

Chạy build Admin/Rust locked, test Rust, dashboard smoke và browser trên site riêng. Task không có migration. Rollback thay đổi code sau khi xác nhận hành vi AdminShell và endpoint baseline.

#### Bàn giao và review

`task/forge-m3-s26-t01` sang `main`, ghi SHA, fixture, lệnh và browser. Chuyển file tích hợp chung cho P14 sau merge.

### M3-S27-T01 — Cài và kích hoạt theme, giữ dữ liệu homepage

- **Thuộc:** V1, M3, M3-S27, M3-P14.
- **Primary Acceptance Slice của capability:** M3-S27 cho THM-01.1.
- **Module:** THM-01
- **Capability:** THM-01.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P14, ADR-0005.
- **Người triển khai / review:** Harbor dự kiến, kiểm chứng macOS theo ADR-0005.

#### Tổng quan và giá trị

Quản trị viên cần cài và đổi theme mà vẫn dùng nội dung homepage đã nhập trong Admin. Theme mới có thể không hỗ trợ block đang dùng.

#### Mục tiêu và hành vi quan sát được

Đổi theme cập nhật giao diện public. Block tương thích hiển thị theo theme đang kích hoạt. Block thiếu hỗ trợ ẩn trên website, còn dữ liệu trong Builder và hiện lại khi quay về theme tương thích.

#### Phạm vi công việc

Thêm danh sách, cài ZIP và kích hoạt theme có CAS, active pointer và resolver dùng chung cho public/preview/Save. Thực hiện `homepage-compat.json` và hợp đồng preview headers theo ADR-0006.

#### Ngoài phạm vi

Builder AST, nhiều block, chỉnh sửa template, Theme Options, sample import, theme upgrade/uninstall và thay đổi permalink hoặc publication ACL nằm ngoài task.

#### Phụ thuộc và điều kiện chặn

P09 và Builder baseline, G2 ADR-0006. Public context của Settings và lượt tích hợp router/shell Dashboard phải merge hoặc có bàn giao độc quyền. Chốt migration number theo `main`. Chặn nếu đổi theme làm mất dữ liệu, hỏng ACL hoặc không thể khôi phục.

#### Kỹ thuật và file ownership

Harbor sở hữu `cms/crates/opendash-server/src/theme.rs`, `page.rs`, `routes/themes.rs`, `routes/builder.rs`, `routes/public.rs`, `routes/public_content.rs`, `routes/mod.rs`, `storage/page.rs`, `storage/theme.rs`, `storage/mod.rs`, `app.rs`, `error.rs` trong cùng `src/`, `cms/migrations/0011_theme_activation.sql` dự kiến. UI gồm `cms/admin/src/App.tsx`, `shell/AdminShell.tsx`, `builder/BuilderWorkspace.tsx`, `api/builder.ts`, `api/client.ts`, `api/themes.ts`, `api/index.ts`, `features/themes/ThemesPanel.tsx` trong cùng Admin `src/`. Kiểm chứng tại `cms/scripts/themes_smoke.py`, `cms/scripts/themes_browser.py`, `docs/verification/2026-11-11-m3-s27-t01-themes-macos.md`. Cần Design Delta nếu mở rộng quyền sở hữu.

#### Cách triển khai theo contract

Dùng ZIP validator, registry ownership và namespaced ID hiện có. Kiểm alias theo schema trước write. Chỉ đổi active pointer atomic. Giữ row homepage và fallback legacy trước lần activate đầu. Với block thiếu hỗ trợ, public trả HTML an toàn và Builder dùng header để báo ẩn. Giữ draft chưa lưu và dispatch hook sau Save đã commit.

#### Nghiệm thu

- [ ] Cài legacy ZIP và package có alias hợp lệ. ZIP/schema/field mismatch bị từ chối không tạo state. Guest 401, Editor 403, CSRF/Origin sai 403 và stale activation 412.
- [ ] A→B render nội dung đã nhập bằng theme B. A→C ẩn block trên homepage nhưng giữ dữ liệu trong Builder. C→A khôi phục hiển thị kể cả sau restart.
- [ ] Preview dùng draft, Save cập nhật phần tương thích, active state qua restart. So sánh home_page, CPT, media refs, public URL và draft ACL trước/sau.

#### Kiểm thử, rủi ro và rollback

Chạy build Admin/Rust locked, Rust tests, themes smoke/browser, public_content_smoke trên macOS và DB copy. Kiểm rollback bằng kích hoạt theme cũ và mở bản snapshot với binary baseline.

#### Bàn giao và review

`task/harbor-m3-s27-t01` sang `main` kèm evidence A/B/C, ZIP và dữ liệu. Sau merge bàn giao fixture cho P17/P19.

### M3-S28-T01 — Hoàn thiện quản lý Plugins và phục hồi khi worker lỗi

- **Thuộc:** V1, M3, M3-S28, M3-P15.
- **Primary Acceptance Slice của capability:** M3-S28 cho PLG-01.1.
- **Module:** PLG-01
- **Capability:** PLG-01.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P15, ADR-0005.
- **Người triển khai / review:** Forge dự kiến, kiểm chứng Windows theo ADR-0005.

#### Tổng quan và giá trị

Admin cần biết plugin nào đang chạy, plugin yêu cầu quyền gì và cách tắt plugin gặp lỗi ngay trong Admin.

#### Mục tiêu và hành vi quan sát được

Admin cài ZIP, xem quyền, bật/tắt và thử lại có chủ đích. Thông tin trạng thái khớp với server sau thao tác và restart. Admin vẫn truy cập Plugins khi worker lỗi.

#### Phạm vi công việc

Hoàn thiện PluginsPanel dùng API hiện có, permission summary, cảnh báo replace-and-enable khi upload, feedback và reload theo kết quả server. Kiểm trên ZIP fixture hiện có.

#### Ngoài phạm vi

Không đổi backend lifecycle, plugin ABI, Wasmi worker, permission contract hoặc bridge CPT/field. Nếu phải đổi các phần này thì áp dụng Stop-and-Propose và Full-Track.

#### Phụ thuộc và điều kiện chặn

P09 và Wasmi baseline. Phạm vi UI riêng không giữ router/AdminShell. Forge nhận theo một active task, xác minh Ready và ownership trước khi bắt đầu.

#### Kỹ thuật và file ownership

Forge sở hữu `cms/admin/src/features/plugins/PluginsPanel.tsx`, `cms/admin/src/api/plugins.ts`, `cms/scripts/windows_plugin_smoke.py`, `cms/scripts/windows_plugins_browser.py` và `docs/verification/2026-11-11-m3-s28-t01-plugins-windows.md`. App.tsx và AdminShell thuộc lượt tích hợp khác. Cần Design Delta nếu mở rộng quyền sở hữu.

#### Cách triển khai theo contract

Giữ endpoint và replace-and-enable theo ADR-0006. Xác nhận upload trước submit, hiện manifest permissions đã trả về trước enable, khóa thao tác đang gửi và refresh state. Giữ lỗi đã lọc, route vẫn dùng được để disable.

#### Nghiệm thu

- [ ] Admin cài/list/enable/disable được, thay cùng ID có cảnh báo và trạng thái server đúng. Editor 403 kể cả gọi trực tiếp.
- [ ] ZIP lỗi không đổi state. Timeout worker vẫn cho Admin mở lại Plugins và disable. Disabled plugin không nhận invocation mới, KV và nội dung giữ qua restart.
- [ ] Browser kiểm quyền, feedback pending, lỗi và retry, keyboard và 390 px.

#### Kiểm thử, rủi ro và rollback

Chạy build Admin/Rust locked, windows_plugin_smoke và browser trên Windows bằng site root riêng. Task UI không có migration. Rollback UI giữ dữ liệu và quyền backend.

#### Bàn giao và review

`task/forge-m3-s28-t01` sang `main` kèm evidence browser/runtime và CI. Nếu cần đụng API/worker, ghi blocker trước khi mở rộng.

### M3-S29-T01 — Nghiệm thu toàn bộ M3 trên macOS

- **Thuộc:** V1, M3, M3-S29, M3-P17.
- **Primary Acceptance Slice của capability:** M3-S29 cho OPS-02.1.
- **Module:** OPS-02
- **Capability:** OPS-02.1
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P17, ADR-0005.
- **Người triển khai / review:** Harbor dự kiến, kiểm chứng macOS theo ADR-0005.

#### Tổng quan và giá trị

Các phần M3 đã có bằng chứng theo task. Cần chạy luồng quản trị hoàn chỉnh trên cùng một bản phát hành để kiểm tương tác và bảo toàn dữ liệu.

#### Mục tiêu và hành vi quan sát được

Một bản ghi nghiệm thu macOS theo SHA chứng minh website vận hành xuyên từ cài mới đến xuất bản, đổi thiết lập, theme và plugin, rồi restart và rollback trên fixture.

#### Phạm vi công việc

Chuẩn bị và chạy browser/runtime E2E trên macOS. Ghi evidence fresh, upgrade pre-0008, upgrade post-0008 và restore snapshot. Phối hợp cùng release SHA với P19.

#### Ngoài phạm vi

Sửa lỗi hành vi CMS, phát hành public và nghiệm thu Windows/Linux nằm ngoài task. Hồi quy sản phẩm chuyển thành blocker có bằng chứng.

#### Phụ thuộc và điều kiện chặn

P07–P16 đã merge và có acceptance theo phạm vi, harness cần thiết P17/P19 đã tích hợp trước khi pin release SHA. Có macOS và browser thật, isolated site root. Thiếu ca bắt buộc hoặc không còn chung SHA với P19 thì chưa đạt.

#### Kỹ thuật và file ownership

Harbor sở hữu `cms/scripts/macos_m3_e2e.py`, `docs/verification/2026-11-11-m3-s29-t01-release-macos.md`. Dùng fixtures hiện có qua bản sao. File shared fixture bổ sung cần Design Delta trước khi sửa. Cần Design Delta nếu mở rộng quyền sở hữu.

#### Cách triển khai theo contract

Dùng binary locked có embedded Admin, SQLite thật và cổng loopback riêng. Tái sử dụng ca P08/P11, kiểm G2 P10/P12–P15 và regressions. Chờ process dừng trước khi dọn dữ liệu. Pin SHA với Forge trước khi nghiệm thu.

#### Nghiệm thu

- [ ] Browser chạy bootstrap/login, Dashboard, Posts/Pages, Media, Builder, Users, Settings, Themes, Plugins, logout/restart trên cùng site fixture.
- [ ] Kiểm 401/403/412, Origin/CSRF, draft/privacy, CSP/escaping, ACL, last-admin, theme rollback và worker recovery.
- [ ] Upgrade trên hai fixture, so dữ liệu/refs và restore snapshot với binary baseline. Evidence ghi đúng SHA, OS, browser, fixture, lệnh và kết quả.

#### Kiểm thử, rủi ro và rollback

Build Admin/Rust locked và chạy Rust tests, macOS E2E, public/auth/plugin regressions theo ADR-0006. Nếu cần PR test harness, merge harness trước khi chốt release SHA. Không dùng dữ liệu website thật.

#### Bàn giao và review

`task/harbor-m3-s29-t01` sang `main` cho harness/evidence khi phù hợp. Chuyển SHA và báo cáo macOS cho P19. Khi release SHA đổi thì đánh giá lại và chạy lại acceptance liên quan trước closure.

### M3-S30-T01 — Nghiệm thu Windows release và kiểm điều kiện đóng M3

- **Thuộc:** V1, M3, M3-S30, M3-P19.
- **Primary Acceptance Slice của capability:** M3-S30 cho OPS-02.2.
- **Module:** OPS-02
- **Capability:** OPS-02.2
- **Track:** Fast-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P19, ADR-0005.
- **Người triển khai / review:** Forge dự kiến, kiểm chứng Windows theo ADR-0005.

#### Tổng quan và giá trị

M3 cần bằng chứng chạy được từ bản đóng gói Windows và dữ liệu nâng cấp an toàn. Hai harness legacy còn cần cập nhật để kiểm đúng hành vi hiện hành.

#### Mục tiêu và hành vi quan sát được

Chạy Windows release có Admin nhúng và Wasmi worker, đối chiếu cùng SHA macOS, ghi kết quả parity và chỉ xem xét đóng M3 khi mọi điều kiện đạt.

#### Phạm vi công việc

Sửa harness browser và distribution trong phạm vi test, pin release SHA với P17, chạy bốn A0 smoke P18 và full Admin E2E, kiểm upgrade/rollback. Báo Linux early smoke riêng nếu có runner.

#### Ngoài phạm vi

Không sửa assertion để che lỗi sản phẩm, không thay runtime, không tạo lại task P18 khi bốn harness đã pass. Linux V1 release và backup engine thuộc M8.

#### Phụ thuộc và điều kiện chặn

P07–P16 đã merge. Chuẩn bị harness trước khi pin cùng release SHA với P17. Closure chờ evidence macOS P17 và kết quả Windows. Nếu bất kỳ ca bắt buộc fail hoặc runtime SHA khác nhau thì giữ milestone mở.

#### Kỹ thuật và file ownership

Forge sở hữu `cms/scripts/windows_admin_browser.py`, `cms/scripts/windows_distribution_smoke.py`, `cms/scripts/windows_m3_release_e2e.py` và `docs/verification/2026-11-11-m3-s30-t01-release-windows.md`. Bốn harness A0 là đầu vào read-only trừ khi regression được cấp gói test-only riêng. Harbor giữ harness macOS. Cần Design Delta nếu mở rộng quyền sở hữu.

#### Cách triển khai theo contract

Dùng Dashboard heading/counts trong browser, health qua API. Distribution smoke kiểm migration list từ source release và đóng SQLite ngay cả khi lỗi. Chốt một SHA sau khi harness merge, dùng cùng fixture contract với P17. Report Linux theo runner hoặc `NOT_RUN`.

#### Nghiệm thu

- [ ] Build Windows release có Admin nhúng, Wasmi worker và chạy từ site root sạch. Browser chạy đủ checklist P17 với negative cases.
- [ ] Bốn harness P18 exit 0 và cleanup ổn định. Auth, distribution, upgrade/restart và restore snapshot trên Windows đều có bằng chứng.
- [ ] Evidence Windows/macOS cùng SHA, không còn blocker security/data và có trạng thái Project đã kiểm tra. Ghi Linux early smoke độc lập.

#### Kiểm thử, rủi ro và rollback

Chạy `cms/scripts/build.ps1 -Release`, Rust tests locked, Windows browser/distribution/auth và bốn lệnh A0 theo ADR-0006. Fix harness land trước release SHA. Rollback dùng site snapshot và binary tương ứng.

#### Bàn giao và review

`task/forge-m3-s30-t01` sang `main` cho harness/evidence. Đối chiếu P17, checklist G5 và Project trước khi đề nghị đóng M3. Không đóng theo kết quả merge PR.
