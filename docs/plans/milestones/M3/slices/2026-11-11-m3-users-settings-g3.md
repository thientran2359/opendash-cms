# M3 — G3 đợt Users và General Settings

**Baseline G2:** `origin/main@9c7cfd68dfb0a4a1ee0b0e61af4367e1a43cffe5` ([PR #46](https://github.com/thientran2359/opendash-cms/pull/46)). **Nguồn:** [ADR-0006](../../../../decisions/ADR-0006-m3-admin-and-release-contracts.md), [ADR-0005](../../../../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md), [Issue #45](https://github.com/thientran2359/opendash-cms/issues/45).

`G3: READY` trong tài liệu là trạng thái đặc tả có hiệu lực khi PR này được duyệt và merge vào `main`. Khi phát hành phải pin source commit chứa G3, kiểm lại `main`, dependency, Issue trùng và Project #3. Forge/Harbor dùng task branch riêng. Hai gói có thể xây phần riêng song song. Chỉ Forge sửa các file dùng chung cho lượt tích hợp Users. Harbor nhận các file chung sau khi Users merge và đổi base tích hợp sang `main` mới; giữ ghi nhận SHA trong Issue/PR. Nếu Users dừng, việc đổi thứ tự phải ghi rõ trên hai Issue để luôn có một writer.

### M3-S24-T01 — Quản lý Users và bảo vệ tài khoản Admin cuối

- **Thuộc:** V1, M3, M3-S24, M3-P12.
- **Primary Acceptance Slice của capability:** M3-S24 cho FND-02.1.
- **Module:** FND-02
- **Capability:** FND-02.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P12, ADR-0005, ADR-0004 (hồi quy quyền biên tập).
- **Người triển khai / review:** Forge dự kiến, runtime Windows và tự kiểm chứng theo ADR-0005.

#### Tổng quan và giá trị

Quản trị viên cần quản lý tài khoản Admin và Editor ngay trong Admin. Màn hiện tại chỉ tạo Editor. Khi đổi vai trò, reset mật khẩu hoặc xóa tài khoản, CMS phải giữ ít nhất một Admin đăng nhập được và thu hồi phiên cũ của tài khoản bị thay đổi.

#### Mục tiêu và hành vi quan sát được

Admin xem danh sách phân trang, tạo Admin/Editor, sửa username/role, reset mật khẩu và xóa tài khoản. Editor bị chặn cả UI lẫn API. Sau thay đổi bảo mật, phiên cũ hết hiệu lực và người dùng đăng nhập lại bằng thông tin mới.

#### Phạm vi công việc

- Mở đủ các route trong ADR-0006. Thêm version cho Users, kiểm CAS, bảo vệ Admin cuối trong transaction và revoke session trong cùng commit.
- Đóng race login/reset bằng conditional insert session sau verify password. Giữ bootstrap, legacy tạo Editor và định dạng lỗi dùng chung.
- Nối `#/users` với danh sách, form, xác nhận xóa/reset và phản hồi lỗi. Giữ draft khi request lỗi, chuyển sang login khi actor tự thay đổi và phiên bị revoke.

#### Ngoài phạm vi

- Email reset, SSO, permission tùy biến, thay policy password/cookie và quản trị người dùng từ plugin nằm ngoài task.
- Settings, theme/plugin, CPT, media và migration đã phát hành thuộc các phạm vi khác.

#### Phụ thuộc và điều kiện chặn

- G3 đã merge, Project #3 Ready đã xác minh, auth FND-02 baseline và AdminShell P09 hoạt động. Mọi race làm mất Admin cuối hoặc giữ phiên đã revoke đều chặn merge.
- Forge giữ quyền tích hợp các file chung đến khi PR Users merge. Nếu migration number đã được cấp trên main, cập nhật số trước khi file được phát hành và ghi rõ trong task.

#### Kỹ thuật và file ownership

- Forge sở hữu `cms/crates/opendash-server/src/auth.rs`, `routes/auth.rs`, `routes/users.rs`, `storage/auth.rs` trong cùng `src/`. Thêm `cms/migrations/0009_users_version.sql` theo baseline và điều kiện cấp số bên trên.
- UI sở hữu `cms/admin/src/api/auth.ts`, `cms/admin/src/features/users/UsersPanel.tsx`. Lượt tích hợp Users sở hữu `cms/crates/opendash-server/src/app.rs`, `routes/mod.rs`, `storage/mod.rs`, `error.rs` cùng `src/` và `cms/admin/src/App.tsx`, `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/api/index.ts`. Khi PR Users merge, chuyển các file chung cho Harbor qua Issue.
- Kiểm chứng trong `cms/scripts/windows_auth_smoke.py`, `cms/scripts/windows_users_browser.py` và `docs/verification/2026-11-11-m3-s24-t01-users-windows.md`. Mở rộng danh sách file qua Design Delta trước khi sửa.

#### Cách triển khai theo contract

Dùng Auth guards và Argon2 helper hiện có. Hash trước write reservation, sau đó kiểm actor, target/version và invariant Admin cuối trong storage transaction. CAS trước khi mutate. Test race đồng bộ tại ranh giới hai connection, giữ điều kiện version khi login insert session. Dùng AdminShell và API client hiện có, giữ response tương thích với client Editor-create cũ.

#### Nghiệm thu

- [ ] Fresh/upgrade SQLite giữ user, password hash, role và session cũ hợp lệ. Tạo tài khoản cả hai role, legacy request thiếu role vẫn tạo Editor.
- [ ] Hai mutation đồng thời còn ít nhất một Admin. Xóa/hạ quyền Admin cuối trả 409 và không revoke phiên hợp lệ của người đó.
- [ ] Reset, đổi username/role và xóa thu hồi phiên target. Race login/reset không tạo phiên từ mật khẩu cũ. Actor tự đổi được đưa về login.
- [ ] Guest 401, Editor 403, sai Origin/CSRF 403, thiếu ETag 428, stale 412, duplicate 409 và thiếu target 404.
- [ ] Browser Admin thật thao tác được trên desktop và 390 px. Không lộ hash/token, giữ input khi lỗi. Auth, Builder và Post/Page role regression PASS.

#### Kiểm thử, rủi ro và rollback

Chạy `npm ci --prefix cms/admin`, `npm run build --prefix cms/admin`, `cargo test --manifest-path cms/Cargo.toml --locked` và build locked. Chạy `windows_auth_smoke.py` với fixture hiện có và `windows_users_browser.py` trên Windows. Chạy race tests trên DB copy với hai kết nối. Xác minh restore snapshot bằng binary baseline. Dừng nếu có mất tài khoản, bypass quyền hoặc revoke thiếu.

#### Bàn giao và review

PR `task/forge-m3-s24-t01` sang `main` kèm source SHA, implementation base/head, lệnh, OS, fixtures, CI và evidence. Merge khi gate ADR-0005 đạt. Bàn giao file tích hợp chung cho P13 sau merge. G5 và Project Done ghi riêng khi có kết quả thực.

### M3-S25-T01 — Lưu General Settings và hiển thị tên website

- **Thuộc:** V1, M3, M3-S25, M3-P13.
- **Primary Acceptance Slice của capability:** M3-S25 cho SITE-01.1.
- **Module:** SITE-01
- **Capability:** SITE-01.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả sau merge).
- **Nguồn quyết định:** ADR-0006 § P13, ADR-0005 và [IANA tzdb 2026f](https://www.iana.org/time-zones/releases/2026f).
- **Người triển khai / review:** Harbor dự kiến, runtime macOS và tự kiểm chứng theo ADR-0005.

#### Tổng quan và giá trị

Quản trị viên cần đổi tên website, tagline và múi giờ mà không sửa file. Giá trị phải tồn tại qua restart và hiển thị an toàn trong các trang public có hỗ trợ.

#### Mục tiêu và hành vi quan sát được

Admin mở General Settings, lưu ba field và thấy giá trị còn nguyên sau restart. Site title/tagline đã escape xuất hiện ở khung mặc định danh sách/chi tiết. Ví dụ giờ địa phương phản ánh múi giờ đã chọn. Hai tab sửa cùng lúc được báo xung đột và giữ dữ liệu chưa lưu.

#### Phạm vi công việc

- Forward migration singleton, danh mục timezone 2026f kèm provenance, GET Settings/timezones và PUT CAS. SITE-01 sở hữu dữ liệu.
- Nối panel Settings, preview giờ địa phương, loading/error, 412 giữ form và Admin-only guard.
- Bổ sung hai chuỗi đã escape cho public default và `site.title`/`site.tagline` cho template custom. Giữ template đã cài, Builder `/` và ACL.

#### Ngoài phạm vi

- Theme Options, plugin settings, SEO, menu, SMTP và render homepage theo theme P14 nằm ngoài task. Timestamp DB/session vẫn UTC.

#### Phụ thuộc và điều kiện chặn

- G3 đã merge và Project #3 Ready đã xác minh. Dùng FND-02, FND-03 và AdminShell P09 hiện có.
- Phần riêng có thể bắt đầu cùng P12. Chỉ nối file chung sau khi PR Users merge, hoặc khi Issue ghi bàn giao độc quyền khác. Kiểm tra lại migration number và source/base SHA tại điểm tích hợp.

#### Kỹ thuật và file ownership

- Harbor sở hữu file riêng `cms/crates/opendash-server/src/routes/settings.rs`, `storage/settings.rs`, `storage/settings_timezones_2026f.txt`, `routes/public_content.rs`, `content_render.rs` và `theme.rs` trong cùng `src/`. Dự kiến `cms/migrations/0010_site_general_settings.sql` sau 0009 của Users.
- UI riêng gồm `cms/admin/src/api/settings.ts`, `cms/admin/src/features/system/GeneralSettingsPanel.tsx`. Sau bàn giao Users, Harbor được sửa file chung `cms/crates/opendash-server/src/app.rs`, `routes/mod.rs`, `storage/mod.rs`, `error.rs` cùng `src/`, `cms/admin/src/App.tsx`, `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/api/index.ts`.
- Kiểm chứng trong `cms/scripts/general_settings_smoke.py`, `cms/scripts/general_settings_browser.py` và `docs/verification/2026-11-11-m3-s25-t01-settings-macos.md`. Mở rộng danh sách file qua Design Delta trước khi sửa.

#### Cách triển khai theo contract

Dùng migration forward-only, SQL trong storage, `ApiError` và guard hiện có. Kiểm validation với danh mục server. CAS ghi nguyên object trong một transaction. Public renderer chỉ nhận hai field đã định kiểu và dùng cơ chế escape/template hiện hành. UI lấy timezone catalog qua API và định dạng một timestamp bằng `Intl.DateTimeFormat` với timezone explicit.

#### Nghiệm thu

- [ ] Fresh và upgrade trả defaults `OpenDash`, tagline rỗng, `UTC`. Dữ liệu cũ giữ nguyên ngoài singleton mới.
- [ ] Admin lưu dữ liệu qua restart. Editor 403, guest 401, Origin/CSRF sai 403, thiếu ETag 428 và stale 412 giữ form.
- [ ] Kiểm Unicode, input quá dài, ký tự control, timezone sai và timezone hợp lệ. Ví dụ cho `Asia/Ho_Chi_Minh` và `America/New_York` đúng với timestamp cố định mùa hè/mùa đông.
- [ ] Tên site/tagline đã escape tại public default. Template cũ vẫn render, theme custom đọc được `site.title`/`site.tagline`, draft/media ACL và Builder `/` giữ hành vi.
- [ ] Browser macOS xác minh form và viewport 390 px, nhập liệu bằng bàn phím, retry và restart trên site fixture.

#### Kiểm thử, rủi ro và rollback

Chạy build Admin/Rust locked và `cargo test --manifest-path cms/Cargo.toml --locked`. Chạy `general_settings_smoke.py`, `general_settings_browser.py` và `public_content_smoke.py` trên macOS. Kiểm DB copy trước và sau migration. Khôi phục snapshot sang site root riêng và mở bằng binary baseline nếu rollback. Dừng khi escaping, draft ACL hoặc dữ liệu legacy bị ảnh hưởng.

#### Bàn giao và review

PR `task/harbor-m3-s25-t01` sang `main` kèm source SHA, implementation base/head sau bàn giao, lệnh, OS, fixture, CI và evidence. Merge khi gate ADR-0005 đạt. G5 và Project Done ghi riêng khi có kết quả thực.
