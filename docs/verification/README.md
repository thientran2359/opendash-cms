# Báo cáo kiểm chứng

Lưu kiểm chứng có bằng chứng theo ngày hoặc milestone (`YYYY-MM-DD-<chu-de>.md`, `M1_*.md`, ...).

Mỗi báo cáo ghi rõ: **môi trường**, **đường dẫn mã**, **lệnh/check đã thực hiện**, **kết quả (PASS / FAIL / NOT RUN)**, **giới hạn**, **các lỗi còn mở**. Phân biệt:

- `AGREED`: chủ dự án đã chốt thiết kế/contract.
- `implemented`: source code có mặt và chứa thay đổi tương ứng.
- `verified`: có bằng chứng lệnh, test hoặc kiểm tra thực tế của phiên bản đó.

Không suy ra trạng thái `verified` từ tài liệu chuyển đến hoặc báo cáo của repository khác.

- [Kiểm kê workspace trước khi tạo scaffold (2026-11-06)](2026-11-06-repository-inventory.md).
- [Báo cáo build và smoke test scaffold (2026-11-06)](2026-11-06-scaffold-build.md).
- [Chuyển cấu trúc `cms/`, `database/` và kiểm chứng (2026-11-07)](2026-11-07-layout-migration.md).
- [Chuyển `docs/` ra thư mục gốc (2026-11-07)](2026-11-07-docs-root-move.md).
- [SitePaths và lần khởi động đầu của binary trên Windows (2026-11-07)](2026-11-07-site-paths.md).
- [Nhập M1/M2 và kiểm chứng tại đích trên Windows (2026-11-07)](2026-11-07-m1-m2-import.md).
- [Kiểm chứng nguồn M1/M2 trước khi nhập mã (2026-11-07)](2026-11-07-m1-m2-source-baseline.md).
- [Xây dựng Bộ khung Lõi (Core Kernel) & Quy chuẩn Code M2.5 (2026-11-07)](2026-11-07-m2-5-core-kernel.md).
- [M3.1 — Mô hình CPT và Custom Fields (2026-11-08)](2026-11-08-m3-1-content-domain.md).
- [M3.2 — SQLite và API tạo/liệt kê CPT (2026-11-08)](2026-11-08-m3-2-content-types-backend.md).
- [M3.3 — CRUD entry nội dung và kiểm chứng lưu bền vững (2026-11-08)](2026-11-08-m3-3-content-entry-crud.md).
- [M3.4 — Giao diện Admin quản lý CPT và Content Entry (2026-11-08)](2026-11-08-m3-4-admin-content-ui.md).
- [M3.5 — Media Library, thumbnail, picker và kiểm tra tham chiếu (2026-11-09)](2026-11-09-m3-5-media-library.md).
- [M3.6 — Public CPT Rendering, theme templates và public media ACL (2026-11-09)](2026-11-09-m3-6-public-content-rendering.md).
- [Đối chiếu tài liệu M3, workflow GitHub và CodeGraph (2026-11-09)](2026-11-09-agent-documentation-reconciliation.md) — chỉ xác minh tài liệu/công cụ, không phải kiểm thử lại CMS.
- [G5 GitHub Management (2026-11-09)](2026-11-09-github-governance-g5.md) — Issue Forms, Project, Ruleset, CI và giới hạn của evidence.
- [CodeGraph — thiết lập và truy vấn trên Windows (2026-11-07)](2026-11-07-codegraph-setup.md) — kiểm chứng công cụ/index và MCP, **không phải CMS runtime E2E hoặc kết nối CodeGraph cho mọi agent**.
- [Skill review/merge thủ công của Atlas/Nova (2026-11-10)](2026-11-10-review-merge-skill.md) — kiểm thử quy trình, artifact guard và checker theo báo cáo; **không phải CMS HTTP/browser/Windows–Linux E2E**, không suy đã merge từ trạng thái local ghi trong báo cáo.
- [Adapter Atlas → CoS Windows: W01 ACL và PONG thực tế (2026-11-12)](2026-11-12-cos-dispatch-w01-pong.md) — chứng cứ gửi chat trên Forge và giới hạn token rotation/production Issue pickup.
- [Kiểm chứng M5 G5 trên Windows (2026-11-13)](2026-11-13-m5-g5-windows.md) — build, full workspace tests và cross-module E2E `M5-S08-T01` trên Windows.
- [Kiểm chứng M5 #157 native macOS Intel (thực chạy 2026-11-17)](2026-11-13-m5-g5-macos.md) — M4→M5→M4, Chrome và preservation PASS trong phạm vi ghi nhận; full cross-OS G5 chưa đóng.
- [Windows source-matched M5 #157 / M7 #214 (2026-11-17)](2026-11-17-m5-m7-windows-source-rebaseline.md) — binary/rollback/tests trên `c59d2f6` (M5) và `69d311b` (M7); browser assertion receipts, runner teardown và giới hạn full G5 được phân biệt rõ.
- [G5 native macOS Intel — M8-S10-T01 / #318 (2026-11-17)](2026-11-17-m8-s10-t01-g5-macos.md) — release suite, Safe Mode khi restore/restart, actual old/new/old và negative ACL/CSRF; Windows F08 vẫn là follow-up trước common RC.
- [G5 native macOS Intel — M7-S15-T01 / #319 (2026-11-17)](2026-11-17-m7-s15-t01-g5-macos.md) — real Chrome picker Page/Post/CPT, bounded pagination, 401/403/412, stable IDs, rollback old/new/old và M7 browser regression; Windows F01 chưa revalidate.
- [G5 native macOS Intel — M5-S11-T01 / #320 (2026-11-17)](2026-11-17-m5-s11-t01-g5-macos.md) — real Chrome v2 revision discovery/activation, legacy workflow, sample import, 401/403/412/409, release Rust 5/5, old/new/old data continuity; Windows F02 chưa revalidate.
- [G5 native macOS Intel — M7-S15-T02 / #321 (2026-11-17)](2026-11-17-m7-s15-t02-g5-macos.md) — Chrome draft navigation/Save/412/reload/delayed, store ACL test, Rust release 3/3 và old/new/old; tổ hợp browser 401/downgrade/reauth và Windows F03 vẫn NOT_RUN.
- [G5 native macOS Intel — M7-S15-T03 / #322 (2026-11-17)](2026-11-17-m7-s15-t03-g5-macos.md) — atomic published Post/Page create v1, SQLite trigger/rollback, Chrome Admin trực tiếp, Rust release 5/5, old/new/old/new preservation; Windows F04/common RC chưa revalidate.
- [G5 native macOS Intel — M7-S15-T04 / #323 (2026-11-17)](2026-11-17-m7-s15-t04-g5-macos.md) — Image/Gallery Picker Chrome release 13/13, Rust media/ACL release 5/5, route stale/cancel/401/invalid media; generic old/new/old/new compatibility. Windows F05/common RC chưa revalidate.
- [G5 native macOS Intel — M7-S15-T05 / #324 (2026-11-17)](2026-11-17-m7-s15-t05-g5-macos.md) — Chrome full Admin navigation runner PASS, lỗi selector/server/watchdog exit 1/1/124 kèm phase, six screenshots, cleanup owned processes; Windows F07 root cause/native follow-up chưa revalidate.
- [G5 native macOS Intel — M6-S09-T01 / #325 (2026-11-17)](2026-11-17-m6-s09-t01-g5-macos.md) — native unfiltered Rust release 2× 162/162, isolated SSRF 1/1, redirect hit counter zero, test-only per-invocation budget; Windows historical F09 còn cần native revalidation.
- [G5 native macOS Intel — M8-S10-T02 / #326 (2026-11-17)](2026-11-17-m8-s10-t02-g5-macos.md) — tracked Chrome encrypted backup/new-root Safe Mode/reauth/activation/Visual Builder/restart PASS; missing navigation/server/wrong Admin/wrong archive passphrase/watchdog bounded failures, recovered/source roots preserved. Windows F06/common RC chưa revalidate.

- [M7 planning G1/G2/G3 — 14/11/2026](2026-11-14-m7-planning.md): docs/source validation only, full14-task coverage; M7 runtime/native/browser NOT_RUN.

- [M8 G1/G2/G3 planning — 14/11/2026](2026-11-14-m8-planning.md): all18 tasks/8 candidates specified, source/docs checks only; M8 native/runtime NOT_RUN.

- [Spec 2 S01–S03 closure and drift gate (2026-11-21)](2026-11-21-spec2-closure.md) — merged-source/native artifact verification; remains Implemented pending conformance fixes.
- [Spec 2 D1–D3 conformance macOS Intel (#451, 2026-11-21)](2026-11-21-egm-spec2-conformance-d1-d3-macos.md) — Media sidebar, selected Pattern placement and Builder timestamp native receipts; final `Verified` promotion remains a coordinator drift-review gate.

- [Spec 2 post-conformance closure (2026-11-22)](2026-11-22-spec2-closure.md) — no remaining feature drift; exact-main verification and outstanding formatter gate.
- [Spec 2 Formatter Gate (#454) & Admin Style A Closure (2026-11-22)](2026-11-22-admin-style-a-and-fmt-closure.md) — whole-workspace `cargo fmt --all --check` PASS, Style A drift report clean, promotes Spec 2 and Admin Style A to `Verified`.
- [OpenDash Studio Customizer, UX Builder & Unified Entry Content Closure (#458–#461, 2026-11-22)](2026-11-22-opendash-studio-closure.md) — exact-main (`4309bc9`) Rust release tests (76 suites / 355 passed), live browser E2E, zero Blocker/Major drift, promotes OpenDash Studio Spec to `Verified`.
- [WordPress-Style Permalinks (Phase 1) Closure (#478–#480, 2026-11-22)](2026-11-22-wordpress-permalinks-closure.md) — exact-main (`c05287a`) Rust release tests (77 suites / 399 passed), Admin typecheck/build, live browser E2E + `curl` header verification (`301`/`404`/`400`), zero Blocker/Major drift, promotes WordPress-Style Permalinks Spec to `Verified`.
- [Declarative `block.json` Block Engine & OpenDash Studio Schema-Driven Blocks Closure (#472–#475, 2026-11-22)](2026-11-22-declarative-block-engine-closure.md) — exact-main (`5a94b65`) Rust release tests (77 suites / 399 passed), Admin typecheck/build, live browser E2E (`<BlockSchemaForm>`, 5 core blocks, responsive CSS, repeater, Media ACL `404`→`200`/`409`), zero Blocker/Major drift, promotes Declarative Block Engine Spec to `Verified`.

- [Tailwind Frontend & Starter v3 coordinator audit (2026-11-23)](2026-11-23-tailwind-starter-closure.md) — merged S01/S02 source audit; remains Implemented with three P2 drift findings tracked in #495.
- [Starter typed URL & conformance delta S01 macOS Intel (#498, 2026-11-23)](2026-11-23-starter-url-delta-macos.md) — source-pinned old binary, typed C-SEC URL policy, Starter 1.0.1, native/rollback/Browser receipts and explicit outstanding gates. Coordinator Verified closure remains separate.
- [OpenDash Studio UI/UX Upgrade Closure (#504–#506, 2026-11-23)](2026-11-23-studio-ux-upgrade-closure.md) — exact-main (`e71222f9`) Node UX test (`15/15`), Admin typecheck/build, Rust release tests (`79` suites / `410` passed), Live Browser E2E (Mode 1/Mode 2, `.od-canvas-gear`, `Duplicate`/`Copy`/`Paste`/`Undo`/`Redo`, Viewport, Draft Save, `0` console/CSP errors), Goldfish drift report (`0 Blocker / 0 Major`), promotes `docs/designs/studio-builder-ux-upgrade.md` to `Verified`.
- [Theme Ergonomics, C-SEC Helpers & `--check-theme` CLI Closure (#521–#523, 2026-11-24)](2026-11-24-theme-ergonomics-closure.md) — exact-main (`651dffc`) Rust release tests (`85` targets / `460` passed gồm `210/210` server unit & `3/3` `theme_ergonomics_test`), CLI `--check-theme` smoke test, `check-standards.py` (`87` mutations), `check-plans.py`, Goldfish drift report (`0 Blocker / 0 Major / 1 Minor / 2 Nit`), promotes `docs/designs/theme-ergonomics-and-csec-helpers.md` to `Verified`.
- [Đa ngôn ngữ Nội dung (Content i18n) & Bản địa hóa Theme v3 (Theme L10n) Closure (#510–#514, 2026-11-24)](2026-11-24-multilingual-closure.md) — exact-main (`367c16c`) Admin typecheck/build (`117` modules), `15/15` HTTP E2E Rust release tests (`multilingual_s01_test`..`s04_test`, `multilingual_test`), Goldfish drift report (`0 Blocker / 0 Major / 3 Minor / 1 Nit`) kèm cập nhật `M8_WRITER_INVENTORY.md`, promotes `docs/designs/multilingual-content-and-theme-l10n.md` to `Verified`.
- [Tailwind Starter v3 & Typed URL Delta Closure (#491, #492, #495, #498, 2026-11-24)](2026-11-24-starter-specs-closure.md) — exact-main (`5e4c2fb`) + fix `M-01` (`BlockSchemaForm.tsx` URL control): deterministic `build:public-tailwind` (`70727` bytes, `0` diff), Admin `typecheck`/`build` (`117` modules), `starter_theme_v3_test` (`1/1`) + `starter_url_delta_test` (`2/2`, `typed_url_fresh_public_saved_and_unsaved_preview` PASS), kế thừa [biên bản macOS #498](2026-11-23-starter-url-delta-macos.md) (PR `#502` `19e21f41`), Goldfish drift audit (`0 Blocker / 0 Major / 0 Minor / 2 Nit` sau fix), promotes `tailwind-frontend-and-starter-theme-v3.md` & `starter-typed-url-and-conformance-delta.md` to `Verified`.
- [Spec 2: Plugin Hooks, Host Ops v1.1, Owner Settings (`SecretStore`) & Write-Time Projections Closure (#524–#527, 2026-11-24)](2026-11-24-spec2-plugin-hooks-closure.md) — `origin/main@65dd049` (PRs `#536`, `#540`, `#542`, `#543`) + hardening vá `M-01`/`M-02`/`Minor`/`Nit`: Admin `typecheck`/`build` (`118` modules), `--check-plugin sample-plugin.zip` CLI exit `0`, `--bin opendash` `223/223`, `m8_s01_t02_writer_barrier` `2/2`, `plugin_extensibility_test` `5/5`, `plugin_s03_pipelines_and_projections` `4/4`, Goldfish drift audit (`0 Blocker / 0 Major / 2 Minor / 2 Nit` sau hardening), promotes `docs/designs/plugin-hooks-host-ops-and-projections.md` to `Verified`.
