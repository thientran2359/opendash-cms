# M7-S14-T01 — Windows fresh-site usability audit (Issue #300)

- **Operator / reviewer:** Forge (native Windows) / Atlas. **Task verdict:** audit evidence submitted for review; **product verdict: BLOCKED**, **not Beta PASS / not native G5**.
- **Audit window:** 2026-11-17, Asia/Bangkok (UTC+07). Timing was observed during the session but no calibrated per-case stopwatch was captured; elapsed times are **not measured**, rather than guessed.
- **Approved G3 source:** `9d5929bcee8c3eb43a5d49977bd745c89e39ecfc`.
- **Product baseline:** `d1338d631e1421db075db6f024afabee988de99a`; `git rev-parse HEAD:cms` = `3f9f2dc0a06178b35ba1d5cf514995e3139d4a0c` (same product tree at the two pinned SHAs).
- **Worktree/branch:** dedicated Forge worktree, `task/forge-m7-s14-t01`; the only owned Git change is this report. Test roots and evidence are private **outside Git**, under an adjacent directory `../opendash-cms-m7-s14-audit-private/`. Do not publish or delete the primary site.
- **Host:** Windows 11 Pro; Python 3.13.4; Node 22.16.0; Cargo 1.99.1; Git 2.45.1.windows.1; Google Chrome 157.0.8103.52. UI testing used attached Chrome and isolated headless Playwright Chrome contexts.
- **Build:** `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release` **PASS** (embedded Admin plus sample plugin build); `cms/target/release/opendash.exe`, 26,073,088 bytes, SHA-256 `712bfba8ff166bf3662547f6c40eeeed708539f2ac5789230ad268bbbb8726df`.
- **Post-test binary provenance:** Running Cargo release tests subsequently rebuilt the executable at the same local path; post-test SHA-256 is `864fb02cfa65fdfe0a4c9415557f2e04bd51df0455a1fb1f190d4a4ad4b66205`. The **manual fresh-site browser findings** were captured on the preceding `712bf...` build. Cargo generated output only in ignored build directories, not product source changes.
- **Admin bundle evidence:** `cms/admin/dist/index.html` SHA-256 `c9dbcc301864c42862a8c02feb044c953dddccde4c2a1bed719c1d67440b0a10`; `cms/admin/dist/assets/index--Uuu_0M4.js` SHA-256 `bf7af9b836c968c3d8d6e98aeb51d8fdd7550b459e80b8610ff0b62eb073802a`; CSS `index-DjihXeWz.css` SHA-256 `445abf827a2f4d8768f0b2a17ef3ac38a6d25b44aa3641ca75bbb040696fd45b`.

## Safety boundaries, setup and repeatability

Native executable started from the pinned tree, working directory `cms`, with `OPENDASH_SITE_ROOT` pointing to a **new empty** primary root outside the repository and `OPENDASH_PORT=18950`; only loopback addresses were used. All primary content was authored via ordinary Admin/Editor UI, **without SQL, direct API writes, dev.py seed or fixtures preloaded into the primary root**. API fetches described below are *read-only probes* unless specified as authentication/CSRF negative probes. The separate backup-generated restored root was started on `18951` for a read-only recovery check, then its port-owning test process was stopped. The existing site and encrypted backup are preserved outside Git.

Reproduction startup (substitute your own *new empty* isolated root; do not point these commands at a populated site):

```powershell
$env:OPENDASH_SITE_ROOT = '<isolated-new-root-outside-repo>'
$env:OPENDASH_PORT = '18950'
Remove-Item Env:OPENDASH_DB_PATH -ErrorAction SilentlyContinue
& ./cms/target/release/opendash.exe
```

Use browser `http://127.0.0.1:18950/admin/`, bootstrap an Admin account, then create an Editor through the Admin Users UI. Do not replay any operation against the source audit root. All credentials and generated session cookies remain private. File selection for ZIP/PNG in automated browser evidence used Playwright `set_input_files` or DOM `File/DataTransfer` change events, **not a tested Windows native OS picker**. Tests of UI click, form save, public GET and authorization are distinguished from independent automated harness results.

## B01–B10 observation matrix

Statuses apply to the **complete prescribed case**, not isolated passing assertions. `NOT_RUN` can contain observed partial success and names the missing subcases. **Elapsed:** not measured for every row; no SLA inference. **Help:** Forge agent operated the UI; a human operator was not used.

| Case | Status | Agent-driven browser / native outcome; missing work, dependencies and owner | Independent harness / evidence |
| --- | --- | --- | --- |
| **B01** | **NOT_RUN** (partial passes) | Blank root bootstrap, Admin login/logout/login, empty Dashboard, Editor login/logout, role-appropriate sidebar all worked. Deep-link/back/expired-session refresh and error-state coverage incomplete. Owner Forge/Atlas to add focused rerun; do not infer full B01 PASS. | `artifacts/manual-browser/B01-dashboard-desktop.png`. |
| **B02** | **FAIL** | Admin added `project` CPT, text `client_name`, image `project_image`, group `project_brief.architect`; Editor created CPT entry and draft post. Admin created/published post and About page. Editor uploaded 1200×800 PNG successfully. Navigation menu `primary_nav` created, but selecting actual page/post menu targets failed due F01. Image picker button did not show dialog in two attempts (F05 candidate; needs confirmation). After controlled native server restart, published Post/Page still returned 200 and draft still 404; full CPT field/category/tag/panel-position persistence not retested. | `artifacts/manual-browser/B02-cpt-schema.png`; F01/F04/F05; post-restart public probes. |
| **B03** | **NOT_RUN** (partial passes) | Unpublished post `/content/post/can-ho-moc-studio` was 404, became 200 with matching title after Publish+Save; draft `/content/post/ghi-chep-hau-truong` stayed 404; published About page 200. Builder preview of unsaved heading showed heading while public home did not; after Builder Save, public home showed it and ETag advanced v1→v2. Archive/sitemap/media exposure boundary and session expiry unverified. | `artifacts/manual-browser/B03-post-published-edit.png`; `B03-public-post-{1440,768,390}.png`. |
| **B04** | **FAIL** | Uploaded v2 theme ZIP generated from repository fixture via browser: Python-repacked ZIP returned HTTP **201**, UI said `Đã cài theme undefined`, but Themes listing still `Đã cài (0)` and GET list contained no activated theme. No Admin activation route for v2 (F02). PowerShell ZIP variant returned 400 `Đường dẫn trong ZIP v2 không hợp lệ`; packaging incompatibility **not established as product defect**. Separate sample import into nonempty root, cancellation/duplicate preservation **NOT_RUN** due unreachable intended theme-enabled UI workflow; follow-up owner Atlas/Nova. | `artifacts/manual-browser/B06-themes-panel.png`; theme ZIP SHA manifest below. |
| **B05** | **BLOCKED** | Homepage Section→Row→Column→Heading authored via UI; preview-before-save/public-after-save observed. Header/footer/single/archive template edit and reopen/compare blocked by v2 theme activation F02; no missing-block fallback fixture run. Responsive *public published post* screenshots at 1440/768/390 exist; these are **not** proof of full template/mobile-device readiness. | `artifacts/manual-browser/B05-builder-home.png`, public post screenshots. |
| **B06** | **NOT_RUN** (partial passes) | The repository sample plugin ZIP installed through Admin/Playwright file-input and GET `/api/admin/plugins` showed `sample-plugin` v1.0.0 enabled with SQLite kv read/write and HTTP allowlist. Admin refreshed list showing one plugin, clicked **Tắt**, and UI showed **Đã tắt**. Subsequent enable/restart/plugin execution/trap/timeout not fully evidenced. Theme switch/update and preservation are blocked by F02. | Sample plugin ZIP hash below. Plugin upload uses browser file-picker simulation, not direct REST seed. |
| **B07** | **FAIL** | Editor sidebar hid Themes/Plugins/Tools; Editor direct GET Themes and Backups returned 403; Editor Dashboard/CPT endpoints returned 200; unauthenticated Backups GET returned 401. **F03:** CPT field showed `● Có thay đổi chưa lưu`; sidebar click discarded it **without confirm**; reopen showed original value. Expired authenticated session, 412 stale, role downgrade and CSRF/Origin negative probes not completed in full. | `artifacts/nav-harness/04-mobile-editor-403-rbac.png`; F03. |
| **B08** | **FAIL** | Admin created encrypted `.odbackup.age` (620,060 bytes), browser-authenticated download returned 200 with exactly that size. Correct-passphrase restore job `425e2157...` **succeeded** into a separate root with `READY.json` (`recovery_safe_mode=true`); original public content remained available. On launching the restored root as a separate process **without operator activation**, health instead reported `safe_mode=false`, public home and published post returned 200, and public home contained **no `noindex`**; F08. No explicit activation should be claimed. Restored session invalidation, SMTP-off and edit-after-activation unverified. | Encrypted artifact SHA `45a5542654792ebfccc6f0df3ffc214d79b9f8c615433f6eb9b7780ded72b61d`; `READY.json` and job receipt private. |
| **B09** | **NOT_RUN** (partial pass) | Wrong passphrase restore job `29aad894...` transitioned to `failed` with `Không thể giải mã hoặc xác thực gói backup.`; original root remained normal. One v2 theme ZIP failed with 400, repacked fixture succeeded 201. Rust M8 negative corpus **PASS**, but tamper/missing artifact or secret/newer schema/disk-full/trap denied permissions not all run through agent UI; Atlas/M8 technical negative owners to supply applicable current-candidate evidence. | Private job receipts, fixtures; technical regressions below. |
| **B10** | **NOT_RUN** | Published post/page/draft/public GET and noindex recovery probe covered; Rust release tests for SEO/contact/video/public ACL independently **PASS**; comprehensive agent-driven regressions and native/performance gates not completed. Native Windows operator experience here is **not** macOS/Linux/mobile device or human usability certification. | Rust/harness results below; owner Atlas to schedule independent native/performance/release track. |

### Concrete authored fresh-site fixtures / steps

1. **Admin** bootstrapped empty root and observed Dashboard counts zero; created Editor user. Admin created CPT `project` (label `Dự án`), fields `client_name` (text) and `project_image` (image), group `project_brief` with `architect` field. Sidebar exposed `Dự án` for Editor.
2. Admin wrote `Căn hộ Mộc Studio` (`can-ho-moc-studio`), published it after initial draft; wrote `Về Mộc Studio` (`ve-moc-studio`) as draft then published following direct-publish HTTP 500 F04.
3. **Editor** wrote unpublished `Ghi chép hậu trường` (`ghi-chep-hau-truong`); saved `Dự án Mộc Studio` (`du-an-moc-studio`) with CPT text/group values, uploaded `studio-project.png` to Media Library (1200×800 thumbnail, reported 17.6 KiB processed).
4. Admin created `primary_nav` menu; adding actual content targets failed in Menus panel. Homepage Visual Builder authored Section→Row→Column→Heading with `Mộc Studio - Kiến tạo không gian sống`, previewed before Save, then saved and verified public.
5. Admin used UI Tools→Backups to create encrypted backup; uploaded same downloaded backup in UI Restore twice: wrong passphrase failure and correct passphrase success, recorded distinct job IDs. Explicit fresh recovered-root launch was a **negative check**, never a silent approval/activation.

## Findings for Atlas triage (no product/harness changes in this PR)

### F01 — S1 · Menus list contract mismatch (B02/B04/B05)

- **Expected:** Admin can select published Page/Post/CPT entries as menu targets and assign navigation.
- **Actual:** `Me.items is not iterable`; menu `primary_nav` exists but target entry picker unusable despite existing published entries.
- **Reproduce:** On freshly authored site, open Admin `#/appearance/menus`, create menu, click **+ Thêm mục**, choose `entry`; inspect error; repeat embedded Menus panel in Themes.
- **Likely cause (source inspection, no patch):** `cms/admin/src/features/menus/MenusPanel.tsx:118-123` accesses `entries.items.map`; `GET /api/admin/content-types/{type}/entries` returns a **bare array**.
- **Impact/owner:** Blocks required navigation assembly; Nova (Admin UI), Atlas triage. Evidence: UI observation and source mismatch. Dependents: B02 → B04/B05.

### F02 — S1 · Theme v2 install acknowledged but cannot activate (B04/B05/B06)

- **Expected:** Successful Admin ZIP install exposes installed revision and an activation path.
- **Actual:** Python ZIP from `cms/fixtures/demo-website/theme-moc-studio` install responded **201**, UI printed `Đã cài theme undefined`; GET `/api/admin/themes` remained `{"items":[],"active_id":null,"version":1,"homepage_compatible":null}`. Visible installed count stayed zero.
- **Reproduce:** Zip fixture using Python `zipfile.ZipFile(..., ZIP_DEFLATED)`, upload on `#/themes` via Admin UI. Verify 201 response and empty list. The PowerShell `Compress-Archive` ZIP gave 400, which is a **packaging caveat**, not the confirmed defect.
- **Likely cause:** `cms/admin/src/features/themes/ThemesPanel.tsx:67-69` assumes install response `.id`; v2 server returns `revision_id/theme_id/version/revision_digest/source_format/activated`, while listing covers legacy items.
- **Impact/owner:** Theme v2 activation, sample-import/theme template workflow blocked; Nova + theme backend owner via Atlas. Evidence ZIP manifest below.

### F03 — S1 · Dirty navigation silently loses edits (B07)

- **Expected:** Unsaved CPT field edit triggers leave guard with Cancel/Stay.
- **Actual:** After `client_name` changed from `Khách hàng thử nghiệm` to `Khách hàng thử nghiệm - sửa chưa lưu`, badge `● Có thay đổi chưa lưu` appeared. Clicking sidebar **Bài viết** immediately navigated; reopening CPT entry retained old value, with no confirmation dialog.
- **Reproduce:** Editor → `Dự án Mộc Studio` existing entry, edit text field without saving, click sidebar Bài viết, return. **Owner:** Nova / Atlas. Confirmed agent-driven UI.

### F04 — S2 provisional · Direct published Page creation returns 500 (B02)

- **Expected:** New Page selected `published` creates normally.
- **Actual:** Admin `Về Mộc Studio` directly submitted `published` returned 500 / UI `Không truy cập được dữ liệu`. **UI-only workaround:** create same Page as draft, change to published, Save; public URL returned 200. No duplicate created in primary. **Owner:** page composite-save backend + Nova, Atlas decides severity based on mandatory journey.

### F05 — S2 candidate · CPT image picker not shown (B02)

- **Expected:** `Chọn ảnh từ thư viện` in `project_image` opens MediaPicker.
- **Actual:** Two Editor clicks accepted but no `role=dialog` appeared; independent Media Library PNG upload succeeded. Needs isolated repeat to exclude UI focus/overlay issue; not a verified root cause. **Owner:** Nova; not classified as confirmed blocker.

### F06 — Harness-only · M8 browser selector mismatch (B08/B10)

- `python cms/scripts/m8_browser_e2e.py --binary cms/target/release/opendash.exe --site-root ../opendash-cms-m7-s14-audit-private/m8-harness-site --artifact-dir ../opendash-cms-m7-s14-audit-private/artifacts/m8-harness --keep-artifacts --timeout 180`
- **FAIL / nonzero:** Playwright timed out 30s looking for button named `Builder` while current sidebar uses `Visual Builder`. This is a harness compatibility gap **not proof of product restore failure**. Owner M8 harness maintainer / Atlas.

### F07 — Harness-only · Admin nav single-edit runner hung (B01/B07)

- `python cms/scripts/admin_nav_single_edit_e2e.py --binary cms/target/release/opendash.exe --screenshot-dir ../opendash-cms-m7-s14-audit-private/artifacts/nav-harness`
- Ran for over 18 minutes with six screenshots produced but no terminal finish; owned Python PID explicitly stopped. Terminal exit -1. Treat **incomplete**; screenshots are partial evidence, not test PASS. Owner harness maintainer / Atlas.

### F09 — Verification flake, severity unassigned · M6 egress SSRF suite intermittent (B10)

- Full native release command `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --release --no-fail-fast -- --test-threads=4` executed unit and integration targets. **All targets except one passed**. `m6_s04_t01_egress_ssrf` initially **FAILED** on mock redirect expectation at `tests/m6_s04_t01_egress_ssrf.rs:206` (`expected 302 returned without following redirect: "Lỗi kết nối HTTP: Không thể hoàn thành yêu cầu."`), yielding global exit **101**.
- Immediately reran the single target without source changes: `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --release --test m6_s04_t01_egress_ssrf -- --test-threads=1` → **PASS**, exit **0**, 1/1 tests in 0.05 s. This suggests intermittent test/loopback mock behavior but **does not prove a product SSRF bypass or full-suite PASS**.
- **Owner:** Atlas + M6 egress test maintainer for repeatability/root cause; do not suppress this failed full-suite receipt. This is a **technical test reproducibility finding**, not a classified S0–S3 product defect without confirmation.

### F08 — S0 candidate / security acceptance blocker · Restored root is public before operator activation (B08)

- **Expected:** Newly restored root with `READY.json.recovery_safe_mode=true` launches in Safe Mode: public noindex and contact/plugin/SMTP restricted pending explicit operator activation.
- **Actual (unambiguous native reproduction):** Correct restore job **succeeded** and `READY.json` stated `recovery_safe_mode=true`, but generated `config/site.json` stated `recovery_safe_mode=false`. Start this restored root with the same release executable at isolated port `18951` (**no activation request sent**): `GET /api/health` returned `{"status":"ok","database":"ready","scaffold":false,"safe_mode":false}`; public homepage and published post returned **200**, homepage lacked `noindex`. Draft stayed 404. Stopped the owned port-18951 process after evidence; original port 18950 unaffected.
- **Source mechanism:** `cms/crates/opendash-server/src/config.rs:83-114` loads `config/site.json` first and only forces Safe Mode from READY if **config/site.json does not exist**. Restored archive contains copied `config/site.json` with false.
- **Impact / open verification:** Explicit activation gate bypassed for public exposure on recovery. We have **not** proven SMTP send, plugin execution or restored-session reuse; do not claim these. Security/data exposure risk makes this **S0 candidate**, with final severity and restore product ownership assigned to Atlas/M8; must not ship Beta as safe without resolving and retesting.
- **Repro isolation:** Never launch restored root on a public host. From a fresh encrypted backup on an isolated test site, restore through Admin UI, inspect generated READY/config flags, start generated root on loopback only; check health/header/meta and stop owned process. Do **not** activate before the negative probe.

## Tool, automated and screenshot evidence

| Execution | Observed result / interpretation |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release` | **PASS**; pinned binary SHA above. |
| `python cms/scripts/windows_smoke.py --theme ../opendash-cms-m7-s14-audit-private/fixtures/sample-banner.zip` | Fixture ZIP generation **PASS**, **not full windows smoke**. |
| `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --release --no-fail-fast -- --test-threads=4` (first attempts) | **NO TESTS RUN**, exit 101 from Windows file-lock error 5 replacing `opendash.exe` (active primary and an orphan executable from the terminated nav harness). The owned orphan was identified by PID/parent/executable path and stopped; unrelated processes untouched. |
| Same full release suite after clean owned-process stop | **FAIL, exit 101**; 88/88 Rust unit tests passed, all integration targets **except** `m6_s04_t01_egress_ssrf` passed. M8 snapshot/codec/restore/negative-corpus/admin activation and M7 SEO/contact/video/public ACL tests all passed. Initial M6 mock redirect errored, F09. |
| Isolated `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --release --test m6_s04_t01_egress_ssrf -- --test-threads=1` | **PASS**, exit 0, 1/1; intermittent full-suite failure remains tracked as F09, not silently waived. |
| M8 browser harness | **FAIL**, missing `Builder` selector, F06. |
| Admin navigation harness | **INCOMPLETE**, stopped after >18 minutes, F07. |
| Agent browser journey | UI authoring and negatives as B01–B10; screenshots and browser observations are **not** native human/mobile usability certification. |

### Sanitized artifact manifest (private; not committed)

All paths below are relative to private audit root `../opendash-cms-m7-s14-audit-private/` unless explicitly prefixed `cms/`. Do not attach unredacted browser session, credentials, SQLite, encrypted backups or screenshots containing secrets to GitHub. SHA-256 digests identify local evidence for Atlas to reproduce/review on the authorized machine.

| Private artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/manual-browser/B01-dashboard-desktop.png` | 161061 | `b25b1cf746799f701054ff548f597ab6b6885817c33fae257cbaff3c48a20a92` |
| `artifacts/manual-browser/B02-cpt-schema.png` | 90594 | `3868397ee3aa9c4a656d190c51eaba5c55c68d52000c7ba8ef922bda2cacc81b` |
| `artifacts/manual-browser/B03-post-published-edit.png` | 104049 | `8fd244519069a64d4b57f2a20fccd4afdc4621025cc11b49414b7f1f2613e2f7` |
| `artifacts/manual-browser/B03-public-post-1440.png` | 16544 | `e98bd2988b940027ca76c3ce20a6e1075c1a8180a676ddcd549d6a468c79f151` |
| `artifacts/manual-browser/B03-public-post-768.png` | 14533 | `4bd7a9e4c8075bb685ec56f03e52e88be67bf14a1c1d811d4cc3c4a7966f139b` |
| `artifacts/manual-browser/B03-public-post-390.png` | 13513 | `2b3d69db06810ba171eea548e525ca5d0a873cf1037dbec491cc4952e472cdc9` |
| `artifacts/manual-browser/B05-builder-home.png` | 101336 | `c021b69a5cd180bca265a3be165303a682c8b0b36b126589e615faafc0eb9fc1` |
| `artifacts/manual-browser/B06-themes-panel.png` | 77486 | `861386801b50deea00c040e6388c0d14a914ab069900d5e7e2aeb59e65ca760f` |
| `artifacts/nav-harness/01-desktop-settings-content-types-zero-cpt.png` | 81117 | `0e2b7de6eda92725b2e2bd107c139d016db0625da4867f192152b7c714d8f7cb` |
| `artifacts/nav-harness/02-desktop-schema-settings-project.png` | 89222 | `fd39455abd643234a56b285c1e410857756fc7fd0fe6ee22a6f9fdf8a4ec825b` |
| `artifacts/nav-harness/03-desktop-single-edit-5-regions.png` | 82788 | `98ecf8294daccef87a1b7a8a576a379b96e8ecf8fbfa33e8c4ba8f589a8be5a5` |
| `artifacts/nav-harness/04-mobile-editor-403-rbac.png` | 23060 | `01b73506e519b4b6d8a48a973b900a0b0e505197e05f87d0c2a51c22f2793e14` |
| `artifacts/nav-harness/05-mobile-single-edit-390px.png` | 35411 | `7762c85d49bdb5c9df98762a7d24b023a9af3b48d5b6cf90162a4b6105e7d093` |
| `artifacts/nav-harness/06-mobile-user-profile.png` | 25107 | `ea81d421ddea916550e35ee32d10b8a8c7b61975ce100166429f3c7fe55ffefc` |
| `fixtures/studio-project.png` | 6184 | `ac60e98ac4a002c6c38ba314f30deb564253605a611bbcd6e0929e10ca1c0c9f` |
| `fixtures/theme-moc-studio-py.zip` | 6699 | `a8026d7d3a6e486452cbb7e0e59eb93b10a0e2f1b54e4c38f5982f2fa9809673` |
| `fixtures/theme-moc-studio.zip` | 6699 | `03778368e60564ebe70e4869ec696d7ac82b548556d6255a0d74ccd2ba85851d` |
| `fixtures/sample-banner.zip` | 350 | `e5af0489ac2507ae58036dc7df6825c14f0bdc574e1879682cebacfaa85bfeaf` |
| `cms/examples/plugins/sample-plugin/sample-plugin.zip` | 17682 | `b04306779e1a76381ec0ce3e00d416d7b98ce2da045b59845cefb30cdaab2a37` |
| `primary-site/backups/artifacts/<backup-id>.odbackup.age` (**never publish binary**) | 620060 | `45a5542654792ebfccc6f0df3ffc214d79b9f8c615433f6eb9b7780ded72b61d` |

## Handoff to Atlas / resolution owners

1. **Prioritize F08** recovery activation/strict Safe Mode invariant and create a focused M8 regression that restores `config/site.json` with `recovery_safe_mode=false` while READY requires true. Review actual public exposure, noindex, SMTP, plugin and sessions separately. Do not equate validated decrypted backup with safe activation.
2. **Route F01, F02 and F03** to T02 product owners (UI + theme runtime as needed). Retest theme v2 activation/menu assembly/dirty-leave before claiming usable Beta.
3. Triage F04 severity (safe draft→publish UI workaround), verify F05 image picker, and repair F06/F07 outdated/hung harness **outside T01 scope**.
4. Close remaining browser/security/native/performance coverage at T02–T04 with explicit applicable current-candidate evidence; do not accept missing tests as PASS. Preserve two-root model, backup, hashes and fail screenshots for review. Human/macOS/Linux/full release = **NOT_RUN** in this audit.

The worktree commit/PR contains **only this verification report**; product source, scripts, fixtures, G3, planning index, verification index, private data and harness were not modified for T01.
