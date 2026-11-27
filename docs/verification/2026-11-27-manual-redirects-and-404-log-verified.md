# Quản lý Redirect Thủ công (301/302), Nhập/Xuất CSV & Nhật ký 404 — EGM Closure Verification (2026-11-27)

- **Design Spec:** [`docs/designs/manual-redirects-and-404-log.md`](../designs/manual-redirects-and-404-log.md) (Owner Approved; S01–S03 implemented)
- **Drift Report:** [`.agents/research/2026-11-27-manual-redirects-404-drift-report.md`](../../.agents/research/2026-11-27-manual-redirects-404-drift-report.md) — 2 vòng Goldfish `egm-codebase-details` độc lập (`18ff02c8` → `03914088`) → **`PASS — Ready for Verified`** (`0 Blocker · 0 Major · 0 Minor · 0 Nit` trong phạm vi tính năng)
- **Worktree & Branch:** `feat/manual-redirects-and-404-log` (rebased trên `origin/main@5ed3b45` — bao gồm Gettext `.pot`/`.po`/`.mo` `#615`/`#618` và migration `0056_unified_field_content_cutover.sql` `#616`; chuyển slot thành [`0057_manual_redirects_and_404_log.sql`](../../cms/migrations/0057_manual_redirects_and_404_log.sql); worktree cô lập `/Users/thientran/Local Sites/opendash-workspace/opendash-redirects-404`)
- **Environment:** macOS `15.8.2` (`x86_64-apple-darwin`), `rustc 1.99.1`, Node/Vite (`cms/admin`, Vite `6.4.6`), Google Chrome `158.0.8127.37`, `TMPDIR` chuẩn hóa bỏ symlink `/var`.
- **Overall Result:** **`PASS`** — đủ điều kiện giữ trạng thái [`docs/designs/manual-redirects-and-404-log.md`](../designs/manual-redirects-and-404-log.md) là **`Verified`** trên [`SITEMAP.md`](../designs/SITEMAP.md).

---

## 1. Admin Build, Gettext I18n, Standards & Documentation Gates

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `node cms/scripts/check-i18n.mjs` | Zero hardcoded Vietnamese trong `cms/admin/src/` và hiện diện `cms/l10n/{opendash.pot, vi.po}` | **PASS** | Exit `0` (`error_codes.json`: `2,183` exact entries + `163` templates; `opendash.pot` / `vi.po`: `4,703` entries) |
| `npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build` | TypeScript check & Vite bundle (`cms/admin/dist/` nhúng vào binary Rust) | **PASS** | `✓ 163 modules transformed.`, `✓ built in 4.05s` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting toàn workspace Rust | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` & `--self-test` | Ranh giới `core`/`storage`/`routes`/`admin`, SQL-only-in-storage, danh mục CSRF/Origin, `<= 600 LOC`, negative regressions | **PASS** | `registrations=108 resolved_handlers=108 direct_guarded=102 delegated_csrf_guarded=1 public_origin_guarded=3 bootstrap_exemptions=2` |
| `python3 cms/scripts/check-plans.py` & `--self-test` | Liên kết Markdown tương đối, bao phủ `SITEMAP.md`, chứng cứ `docs/verification/` & 13 fixture `--self-test` | **PASS** | Exit `0` |
| `python3 .agents/scripts/check-agent-artifacts.py` | Vệ sinh kỹ năng & zero secret leak | **PASS** | Exit `0` |
| `python3 cms/scripts/run-tests.py --self-test` | Kiểm tra routing rule runner (`i18n_redirects.rs`, `redirects/`, `storage/redirects/`, `routes/redirects/`) | **PASS** | Exit `0` (`10` unit tests in `0.011s`) |
| `python3 -m unittest discover -s .agents/scripts` | `test_prepare_github_issue.py` & `test_cos_*.py` | **PASS** | Exit `0` |
| `git diff --check HEAD^ HEAD` | Conflict marker & trailing whitespace trên diff gộp rebase | **PASS** | Exit `0` |

---

## 2. Full Workspace Rust `--release` Verification (`671` passed, `0` failed)

Sau rebase lên `origin/main@5ed3b45`, sửa `#[cfg(test)] mod i18n_redirects;` trong `main.rs`, đồng bộ `error_codes.json`/`opendash.pot`/`vi.po`, và cập nhật assertion `migration_version == 57` ở [`provider_taxonomy_and_routes.rs`](../../cms/crates/opendash-server/tests/content/provider_taxonomy_and_routes.rs):

- `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` đã chạy toàn phần và PASS `opendash-core`, `opendash-plugin-sdk`, `opendash` bin unit, `builder`, `content`, `i18n`, `plugin`, `plugin_egress_ssrf`. Trên macOS Intel, khi chạy mặc định song song nhiều process hoặc cold-start lần đầu (`Run 0: 3.381s` so với `Run 1: 0.935s`), một số test dùng ngưỡng poll `0..60` (`3.0s`) gặp timeout khởi động `server did not become healthy` nên đã chạy lại dứt điểm bằng `--test ops -- --test-threads=4`, `--test theme -- --test-threads=2`, và `--test site_media_seo -- --test-threads=1`:

| Cargo Test Target (`--locked --release`) | Result | Count | Duration | Notable Coverage |
| --- | --- | --- | --- | --- |
| `opendash-core` (lib unit) | **PASS** | `158/158` | `0.02s` | Bao gồm chuẩn hóa path, `%20` / UTF-8, `RESERVED_SYSTEM_ROOTS`, CSV parser/formatter chống công thức |
| `opendash-plugin-sdk` (lib unit) | **PASS** | `5/5` | `0.00s` | SDK guest checks không hồi quy |
| `opendash` (bin unit) | **PASS** | `279/279` | `158.92s` | Bao gồm `i18n::tests::exact_table_has_valid_codes_and_bidirectional_coverage` (`2,183` exact entries) & `i18n_redirects` (`redirects.*` error codes + explicit `413`) |
| `tests/builder.rs` | **PASS** | `14/14` | `7.86s` | Builder AST & schema contracts |
| `tests/content.rs` | **PASS** | `44/44` | `26.74s` | Bao gồm `migration_0056_upgrades_baseline_0001_*` & `c4_upgrade_0040_to_0041` (`MAX(version) == 57`) |
| `tests/i18n.rs` | **PASS** | `19/19` | `3.68s` | Gettext `.pot`/`.po`/`.mo` & Admin localization E2E API suites |
| `tests/ops.rs` (`--test-threads=4`, canonical `TMPDIR`) | **PASS** | `26/26` | `27.37s` | Bao gồm 2/2 `writer_barrier` (7 mutating redirect/404 handlers & `routes/redirects/`) + backup/restore/coordinator |
| `tests/plugin.rs` | **PASS** | `27/27` | `15.41s` | Wasmi + JS runtime + Host Ops + SSRF |
| `tests/plugin_egress_ssrf.rs` | **PASS** | `1/1` | `0.06s` | DNS-pinned SSRF isolation |
| `tests/site_media_seo.rs` (`--test-threads=1`, canonical `TMPDIR`) | **PASS** | `19/19` | `17.96s` | Bao gồm 3/3 suite `manual_redirects_404` (CRUD/CAS/RBAC/precedence/`public_origin`/`%2e%2e` wire `400`, CSV >500 lỗi `X-OpenDash-Error-Code`, 404 telemetry & `< 256` schema budget) |
| `tests/theme.rs` (`--test-threads=2`, canonical `TMPDIR`) | **PASS** | `79/79` | `36.53s` | Theme v3, C-SEC & Starter theme checks |

---

## 3. Live Browser E2E Verification (`--release` binary, Real Chrome `158.0.8127.37`)

- **Release Binary SHA-256 (`cms/target/release/opendash`):** `28c3c2391730ab59b512875047f49e537da661863d4d2c189c938ee4fd0b45c8`

| Playwright Chrome E2E Script | Checks | Result | Verified Scope |
| --- | --- | --- | --- |
| [`cms/scripts/manual_redirects_404_e2e.py`](../../cms/scripts/manual_redirects_404_e2e.py) | `7/7` | **PASS** | CUJ 1 (menu `Công cụ → Chuyển hướng & 404`, CRUD, shadowed banner, filter/search, `301` query preservation, live `200` priority), CUJ 2 (CSV dry-run error block, `skip` commit, BOM UTF-8 export + formula neutralization `"'=1+1"`), CUJ 3 (404 telemetry, `/Admin/wp-login.php` & `/%61dmin/x` exclusion, `#/tools/404-log` in-place 1-click redirect, bulk delete, clear all), CUJ 5 (`412` conflict & rebase, `401` cookie expiry + RAM draft recovery, `vi` locale + `390x844` mobile, Editor `rbac-forbidden` & API `403`, zero `pageerror`). |
| `cms/scripts/admin_navigation_e2e.py` *(retired)* | `37/37` | **PASS** | Bao gồm 25+ destination routes (`#/tools/redirects`, `#/tools/404-log`), sidebar active state, `390px` mobile drawer, Editor RBAC `403` trên cả `#/tools/redirects` & `#/tools/404-log`, restart persistence & zero `pageerror`. |

*(Giới hạn môi trường: kiểm thử cục bộ chạy trên macOS Intel `15.8.2` với site root cô lập và cổng loopback ephemeral; sẽ được đối chiếu thêm với required CI trên GitHub Actions `governance-ubuntu-latest`, `governance-windows-latest`, `cms-ci` khi mở PR.)*
