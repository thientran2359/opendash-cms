# M3 — Hai task mở đầu theo ADR-0005 (G3)

**Phê duyệt quy trình:** [ADR-0005](../../../../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md) — owner giao Harbor/Forge tự thực hiện M3, không cần review agent khác. **Source baseline:** `main@35fd12f657095602ab43f9a0ca3506d0f97e728c`. **Đích:** G3 dưới đây chỉ có hiệu lực sau khi PR tài liệu merge vào `main`, GitHub Issues được tạo và Project #3 được kiểm tra. **Không** giả đã bắt đầu G4/G5 hoặc Windows runner. Phân công chỉ được thực hiện khi Project Ready xác minh.

## Lượt 1 — Harbor backend và Forge frontend không trùng file

### M3-S20-T01 — Post/Page backend: tạo draft, publish và public/media an toàn

- **Thuộc:** V1, M3, M3-S20; coverage M3-P07/P08/P16.
- **Primary Acceptance Slice của capability:** M3-S20 cho CNT-04.1 (Post/Page publication boundary); CNT-01/02/05 là tích hợp phụ.
- **Module:** CNT-04
- **Capability:** CNT-04.1
- **Track:** Full-Track
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** READY (đặc tả); chưa xác nhận GitHub Project Ready/claim.
- **Nguồn quyết định:** [ADR-0004](../../../../decisions/ADR-0004-m3-post-page-publication-boundary.md), [ADR-0005](../../../../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md), M3.1–M3.6 source/evidence và [G3 WP-A/B/C](2026-11-10-post-page-publication-g3.md).
- **Người triển khai / review:** Harbor (CoS macOS); **owner trực tiếp miễn peer review cho M3**, Harbor self-verifies acceptance/CI; không giả independent reviewer.

#### Tổng quan và giá trị

Admin cần có Bài viết/Trang dựng sẵn, lưu bản nháp riêng tư và xuất bản có kiểm soát. Dữ liệu CMS hiện cho phép public mọi entry được lưu, vì vậy chỉ thêm built-in keys là nguy cơ lộ bản nháp và ảnh; task này hoàn thành hợp đồng backend an toàn xuyên SQLite/API/public.

#### Mục tiêu và hành vi quan sát được

Sau cài mới hoặc upgrade, Admin/Editor tạo bài viết/trang ở `draft`, lưu không công khai và có thể preview sau xác thực; Publish/Unpublish với ETag làm thay đổi khả năng truy cập URL và media theo published status. Legacy CPT vẫn Save-public, Builder `/` không bị thay đổi, giữ dữ liệu qua restart.

#### Phạm vi công việc

- Seed `post/page` Core-owned; forward migration `publication_status`, preflight collision trước migrate, bảo toàn dữ liệu và quyền reserved key ở storage.
- Tất cả create paths cho built-in mặc định draft; Save giữ trạng thái hiện có; API chuyển trạng thái Admin/Editor, CSRF/Origin, version CAS và ETag.
- Authenticated preview draft; SQL published-filter **trước pagination** cho archive/single; public media reference ACL đầy đủ image/gallery và shared SHA256. Tích hợp WP-A/B/C **atomically** nếu fail-closed trạng thái trung gian chưa được chứng minh.
- Thêm test trên DB copy, HTTP/routing regression và ghi verification SHA/macOS.

#### Ngoài phạm vi

- Không chỉnh `cms/admin/src/**`, không giao diện Posts/Pages, không thêm taxonomy, revision, scheduled publish, media delete/GC, redirect chính thức cho collision.
- Không auto-rename user CPT có key `post/page`, không sửa live site DB; không sửa migrations `0001`–`0007`.

#### Phụ thuộc và điều kiện chặn

- ADR-0004 G2 AGREED, ADR-0005 M3 execution policy merged, no overlapping backend writer, source baseline/dependency checked.
- GitHub Issue phải có source SHA và Project #3 `Ready`; nếu thiếu quyền xác minh hoặc collision unsafe thì BLOCKED, không claim. Trước merge phải chứng minh không có trạng thái release làm lộ draft/media.

#### Kỹ thuật và file ownership

- **Writer duy nhất Harbor**. Allowed existing paths: `cms/crates/opendash-core/src/content.rs`, `cms/crates/opendash-server/src/storage/mod.rs`, `cms/crates/opendash-server/src/storage/content.rs`, `cms/crates/opendash-server/src/storage/entries.rs`, `cms/crates/opendash-server/src/storage/public_content.rs`, `cms/crates/opendash-server/src/routes/content.rs`, `cms/crates/opendash-server/src/routes/entries.rs`, `cms/crates/opendash-server/src/routes/public_content.rs`, `cms/crates/opendash-server/src/app.rs`.
- Allowed new migration `cms/migrations/0008_post_page_publication.sql` and smoke scripts `cms/scripts/content_types_smoke.py`, `cms/scripts/content_entries_smoke.py`, `cms/scripts/public_content_smoke.py`, `cms/scripts/media_smoke.py`, plus one report under `docs/verification/` named after task. No simultaneous edits by Forge to these files.
- If new file/helper becomes essential, update approved task source with exact name via Design Delta/PR before expanding ownership. Reuse existing storage SQLx and guarded sessions; no duplicate posts/pages tables.

#### Cách triển khai theo contract

Implement a pre-migration collision validation (before normal SQLx migrate), then fail-closed SQL migration and post/page bootstrap with consistent draft creation. Deny public built-in entries until all published SQL and media conditions and preview auth pass in the same integration. Keep row/status/ETag semantics from ADR-0004 and hook dispatch post-success only.

#### Nghiệm thu

- [ ] Fresh/upgrade/collision tests on isolated SQLite copies: Core seed idempotent, legacy data/IDs/Builder preserved, collision fails before destructive changes; real SQLx path tested.
- [ ] New post/page always draft; public single/archive/thumbnail/file draft-only 404; no draft title/slug leak; authenticated preview/no-store/CSP/escaping works.
- [ ] Publish/Unpublish and Save semantics, Admin/Editor session, CSRF/Origin, version-CAS stale 412; status persists after restart; no hook on failed CAS.
- [ ] Published archive paginated correctly; image/gallery shared SHA256 stays public when another published or legacy reference exists; last reference removal makes public 404.
- [ ] Rust build/tests and HTTP/security smoke PASS for changed SHA; compatibility tests for legacy CPT and Builder `/`; report macOS OS/build/SHA/results.

#### Kiểm thử, rủi ro và rollback

- Run `cargo test --manifest-path cms/Cargo.toml --locked`, `cargo build --manifest-path cms/Cargo.toml --locked` and affected `cms/scripts/*smoke.py` against isolated test server/site root, plus security negative fixtures; record exact exit status and commands.
- Migration preflight must occur **before** SQLx migrate; test occupied reserved key and partial-migration recovery; schema already applied requires forward migration, not Git revert. No actual-user DB writes. If any unsafe intermediate release arises, ship A+B+C atomically and stop on test failures.

#### Bàn giao và review

PR from `task/harbor-m3-s20-t01` to `main` with source/base SHA, implementation diff, test evidence, CI and security checklist. Harbor self-merge only after all gates PASS via expected head SHA; G5/Project Done and TASK_INDEX sync separately, not on merge alone.

### M3-S21-T01 — AdminShell Tailwind: điều hướng WordPress-like không đổi backend

- **Thuộc:** V1, M3, M3-S21; coverage M3-P09.
- **Primary Acceptance Slice của capability:** M3-S21 cho FND-05.1 (AdminShell và navigation V1).
- **Module:** FND-05
- **Capability:** FND-05.1
- **Track:** Fast-Track
- **G2:** NOT_APPLICABLE
- **Lý do Fast-Track:** Chỉ đổi giao diện, CSS, routing phía client sử dụng các endpoint session/RBAC và Builder hiện có; không thay DB, API, auth/security contract. Nếu phát sinh thay đổi API, RBAC hoặc public delivery phải STOP/đưa qua G2 Full-Track riêng.
- **G3:** READY
- **Trạng thái:** READY (đặc tả); chưa xác nhận GitHub Project Ready/Forge pickup.
- **Nguồn quyết định:** [ADR-0005](../../../../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md), [M3-P09 plan](../TASK_PLAN.md), FND-05 catalog và legacy Admin shell M3.4.
- **Người triển khai / review:** Forge (CoS Windows); **peer review waived riêng M3**, Forge self-verifies TypeScript/UI/CI và Windows behavior.

#### Tổng quan và giá trị

Admin hiện mới có giao diện/tabs cơ bản. Một AdminShell điều hướng rõ như WordPress với Tailwind, responsive và phân quyền giúp người quản trị có chỗ truy cập các module M3 mà không ảnh hưởng đến API sẵn có.

#### Mục tiêu và hành vi quan sát được

Sau login, người dùng thấy sidebar/topbar và điều hướng Dashboard, Posts, Pages, Media, Appearance/Themes, Plugins, Users, Settings, Builder; trang chưa có tính năng backend phải hiển thị trạng thái chưa hỗ trợ thay vì giả CRUD. Editor không được nhìn thấy thao tác Admin-only; Builder/Content cũ và logout/session/401 hoạt động khi chuyển màn hoặc reload.

#### Phạm vi công việc

- Gắn Tailwind CSS build có pin dependency, giữ cấu trúc React/Vite và assets đóng gói hiện hữu; responsive sidebar/topbar, keyboard focus/navigation.
- Client route/navigation shell theo role từ API session hiện có; màn chưa triển khai có trạng thái trung thực, không gọi endpoint giả và không mở quyền Admin-only bằng route.
- Tái sử dụng Builder, Content và các panel đã tồn tại; lưu/preview trước và sau điều hướng không làm mất dữ liệu hay biến draft thành published.

#### Ngoài phạm vi

- Không thay database, Rust, route backend, API auth/permissions, migration, public pages hay M3-P08 Posts/Pages CRUD đang do backend task khác triển khai.
- Không can thiệp PR #7/#9, watcher, Windows credentials hoặc nhánh `agent/forge` đang có PR cũ.

#### Phụ thuộc và điều kiện chặn

- M3.4 existing Admin React và auth, ADR-0005 merged; không phụ thuộc Post/Page API đang phát triển.
- GitHub Issue Project #3 `Ready` verified, exact source/base SHA, Forge tự xác minh workspace/branch sạch; **không reset `agent/forge`**, tạo task branch/worktree riêng từ main. Nếu Forge không thể chạy phiên Windows thì task chờ pickup thật, Harbor không tự tuyên bố triển khai.

#### Kỹ thuật và file ownership

- **Writer duy nhất Forge.** Allowed files: `cms/admin/package.json`, `cms/admin/package-lock.json`, `cms/admin/vite.config.ts`, `cms/admin/index.html`, `cms/admin/src/App.tsx`, `cms/admin/src/main.tsx`, `cms/admin/src/styles.css`, `cms/admin/src/shell/AdminShell.tsx`, `cms/admin/src/shell/AuthPanel.tsx`. New UI-only files under `cms/admin/src/shell/` must be explicitly listed in issue/Design Delta before editing.
- Không sửa `cms/admin/src/features/content/**` khi Harbor backend API đang thay đổi; shared `App.tsx` được Forge sở hữu trọn phiên task này. Không chạm Rust/migrations/`TASK_INDEX.md` đồng thời.

#### Cách triển khai theo contract

Route/shell là client-only; server vẫn enforce Admin/Editor RBAC. Existing Builder and Content components continue to use established API functions. Missing destinations show non-functional placeholders only where clearly labeled; do not expose privileged data to Editor merely by hiding UI controls.

#### Nghiệm thu

- [ ] `npm ci`, `npm run typecheck`, `npm run build` PASS on Windows; pinned dependencies audited at task SHA.
- [ ] Browser smoke on Windows: Login/Logout, role navigation and Admin-only hidden UI, responsive sidebar, keyboard/focus and broken-session 401.
- [ ] Builder preview/Save and existing Content forms preserve behavior, errors/unsaved changes not silently lost on navigation; no fake backend actions.
- [ ] GitHub required governance checks PASS on exact PR head and evidence report has SHA, Windows runner, browser screenshots/output, limitations.

#### Kiểm thử, rủi ro và rollback

- Use existing `cms/admin` scripts and real browser; verify CSS/bundle build inside Rust served Admin if available. No new backend permission assumptions. Rollback only frontend PR; avoid touching historical Forge branch/PR or user data.

#### Bàn giao và review

Forge owns `task/forge-m3-s21-t01` created from current main, publishes PR and complete Windows tests. Self-merge only after required checks + acceptance success and expected SHA; no independent reviewer required by ADR-0005. Project/G5 and TASK_INDEX closure follow actual evidence.
